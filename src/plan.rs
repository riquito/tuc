use std::ops::Range;

#[cfg(feature = "regex")]
use crate::finders::regex::RegexFinder;
#[cfg(feature = "regex")]
use regex::bytes::Regex;

use crate::{
    bounds::{BoundOrFiller, BoundsType, Side, UserBounds},
    finders::{
        common::DelimiterFinder,
        fixed::{FixedFinder, FixedFinderRev},
        fixed_greedy::{FixedGreedyFinder, FixedGreedyFinderRev},
    },
    options::Opt,
};
use anyhow::{Result, bail};

type ExtractFunc<F, R> = fn(&[u8], &mut FieldPlan<F, R>) -> Result<Option<usize>>;

#[derive(Debug)]
pub struct FieldPlan<F, R>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    positive_indices: Vec<usize>,
    negative_indices: Vec<usize>,
    pub positive_fields: Vec<Range<usize>>,
    negative_fields: Vec<Range<usize>>,
    pub extract_func: ExtractFunc<F, R>,
    finder: F,
    finder_rev: R,
}
impl<F, R> FieldPlan<F, R>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    pub fn from_opt_with_finders(opt: &Opt, finder: F, finder_rev: R) -> Result<Self> {
        let need_num_fields = opt.only_delimited
            || opt.complement
            || opt.json
            || (opt.bounds_type == BoundsType::Characters && opt.replace_delimiter.is_some());

        #[cfg(feature = "regex")]
        let maybe_regex: Option<&Regex> = opt.regex_bag.as_ref().map(|x| {
            if opt.greedy_delimiter {
                &x.greedy
            } else {
                &x.normal
            }
        });

        let mut positive_indices = Vec::with_capacity(opt.bounds.len());
        let mut negative_indices = Vec::with_capacity(opt.bounds.len());

        // First collect all indices from bounds, keeping duplicates and original order.
        for bof in opt.bounds.iter() {
            if let BoundOrFiller::Bound(b) = bof {
                let (l_is_negative, l_value) = b.l().value();
                let (r_is_negative, r_value) = b.r().value();

                if l_is_negative {
                    negative_indices.push(l_value);
                } else {
                    positive_indices.push(l_value);
                }

                if r_is_negative {
                    // XXX can negative ever be max_right?
                    if r_value != Side::max_right() {
                        negative_indices.push(r_value);
                    } // else ignore "continue" as right bound
                } else if r_value != Side::max_right() {
                    positive_indices.push(r_value);
                }
            }
        }

        // Then sort and deduplicate the indices.
        positive_indices.sort_unstable();
        positive_indices.dedup();
        negative_indices.sort_unstable();
        negative_indices.dedup();

        let max_field_to_search_pos = positive_indices.last().map(|x| x + 1).unwrap_or(0);
        let max_field_to_search_neg = negative_indices.last().map(|x| x + 1).unwrap_or(0);

        let extract_func = if need_num_fields {
            extract_every_field
        } else {
            match (!positive_indices.is_empty(), !negative_indices.is_empty()) {
                (false, false) => bail!("No indices found in bounds"), // invariant, shouldn't occur
                (true, false) => extract_fields_using_pos_indices,
                #[cfg(feature = "regex")]
                (_, true) if maybe_regex.is_some() => {
                    // I can't reverse search a regex, so if there are negative indices,
                    // I'll have to search for every field.
                    extract_every_field
                }
                (false, true) => extract_fields_using_negative_indices,
                (true, true) => |line: &[u8], plan: &mut FieldPlan<F, R>| {
                    extract_fields_using_pos_indices(line, plan)?;
                    extract_fields_using_negative_indices(line, plan)?;
                    Ok(None)
                },
            }
        };

        Ok(FieldPlan {
            positive_indices,
            negative_indices,
            // XXX maybe I can reduce the capacity here
            // by storing fields by original index position?
            positive_fields: vec![Side::max_right()..Side::max_right(); max_field_to_search_pos], // initialize with empty ranges
            negative_fields: vec![Side::max_right()..Side::max_right(); max_field_to_search_neg], // initialize with empty ranges,
            extract_func,
            finder,
            finder_rev,
        })
    }
    #[inline(always)]
    fn get_field_bound(&self, side_val: &Side) -> Result<&Range<usize>> {
        let is_negative = side_val.is_negative();

        let fields = if is_negative {
            &self.negative_fields
        } else {
            &self.positive_fields
        };

        let index = side_val.value_unchecked();

        if index >= fields.len() {
            return Err(anyhow::anyhow!("Out of bounds: {}", side_val));
        }

        if let Some(field) = fields.get(index)
        // Field can start at max_right when the fields
        // array is initialized that way.
         && field.start != Side::max_right()
        {
            Ok(field)
        } else {
            Err(anyhow::anyhow!("Out of bounds: {}", side_val))
        }
    }

    #[inline(always)]
    pub fn get_field(&self, b: &UserBounds, line_len: usize) -> Result<Range<usize>> {
        let l = b.l();
        let r = b.r();

        let l_bound = self.get_field_bound(l)?;

        let start = l_bound.start;

        let end = if l == r {
            l_bound.end
        } else if r.value_unchecked() == Side::max_right() {
            line_len
        } else {
            self.get_field_bound(r)?.end
        };

        if end >= start {
            Ok(start..end)
        } else {
            bail!("Field left value cannot be greater than right value");
        }
    }
}

#[inline(always)]
fn extract_fields_using_pos_indices<F, R>(
    line: &[u8],
    plan: &mut FieldPlan<F, R>,
) -> Result<Option<usize>>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    if line.is_empty() {
        return Ok(None);
    }

    let mut seen = 0;

    let mut delim_iterator = std::iter::once(std::ops::Range { start: 0, end: 0 })
        .chain(plan.finder.find_ranges(line))
        .peekable();

    let line_len = line.len();
    let eol_range = std::ops::Range {
        start: line_len,
        end: line_len,
    };

    for i in 0..plan.positive_indices.len() {
        let desired_field = plan.positive_indices[i];

        let f_start = delim_iterator
            .nth(desired_field - seen)
            .ok_or_else(|| {
                plan.positive_fields[desired_field..].fill(Range {
                    start: Side::max_right(),
                    end: Side::max_right(),
                });
                anyhow::anyhow!("Out of bounds: {}", desired_field + 1)
            })?
            .end;

        let f_end = delim_iterator.peek().unwrap_or(&eol_range).start;

        plan.positive_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(None)
}

#[inline(always)]
fn extract_fields_using_negative_indices<F, R>(
    line: &[u8],
    plan: &mut FieldPlan<F, R>,
) -> Result<Option<usize>>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    if line.is_empty() {
        return Ok(None);
    }

    let mut delim_iterator = std::iter::once(std::ops::Range {
        start: line.len(),
        end: line.len(),
    })
    .chain(plan.finder_rev.find_ranges(line))
    .peekable();

    let mut seen = 0;

    let start_range = std::ops::Range { start: 0, end: 0 };

    for i in 0..plan.negative_indices.len() {
        // negative_indices is sorted from biggest (-1) to smallest (-X)
        let desired_field = plan.negative_indices[i];

        let f_end = delim_iterator
            .nth(desired_field - seen)
            .ok_or_else(|| {
                plan.negative_fields[desired_field..].fill(Range {
                    start: Side::max_right(),
                    end: Side::max_right(),
                });
                anyhow::anyhow!("Out of bounds: -{}", desired_field + 1)
            })?
            .start;

        let f_start = delim_iterator.peek().unwrap_or(&start_range).end;

        plan.negative_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(None)
}

#[inline(always)]
fn extract_every_field<F, R>(line: &[u8], plan: &mut FieldPlan<F, R>) -> Result<Option<usize>>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    let mut num_fields = 0;

    if line.is_empty() {
        return Ok(Some(num_fields));
    }

    let mut next_part_start = 0;

    // "clear()" is necessary because we push on top of the vec.
    // Other "extract_" algorithms do not clear it because they
    // update the fields they need and read only those later.
    plan.positive_fields.clear();

    for r in plan.finder.find_ranges(line) {
        plan.positive_fields.push(Range {
            start: next_part_start,
            end: r.start,
        });

        next_part_start = r.end;
    }

    plan.positive_fields.push(Range {
        start: next_part_start,
        end: line.len(),
    });

    // Now that I know about every positive field,
    // let's fill the negative fields.
    num_fields = plan.positive_fields.len();

    let mut out_of_bound_pos_idx = None;
    // Do we have any positive out of bounds?
    if plan.positive_indices.last() > Some(&(num_fields - 1)) {
        // need to find out which one is the first index out of bound
        out_of_bound_pos_idx = Some(
            plan.positive_indices
                .binary_search(&(num_fields))
                .unwrap_or_else(|idx| idx)
                + 1, // wouldn't work for empty positive indices but here's ok
        );
    }

    let mut out_of_bound_neg_idx = None;

    for i in 0..plan.negative_indices.len() {
        let desired_field = plan.negative_indices[i];

        if num_fields < desired_field + 1 {
            plan.negative_fields[desired_field..].fill(Range {
                start: Side::max_right(),
                end: Side::max_right(),
            });
            out_of_bound_neg_idx = Some(desired_field);
            break;
        }

        let field = &plan.positive_fields[num_fields - desired_field - 1];

        let f_start = field.start;
        let f_end = field.end;

        plan.negative_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };
    }

    if let Some(idx) = out_of_bound_pos_idx {
        bail!("Out of bounds: {}", idx + 1);
    }

    if let Some(idx) = out_of_bound_neg_idx {
        bail!("Out of bounds: -{}", idx + 1);
    }

    Ok(Some(num_fields))
}

// Convenience constructor functions
impl FieldPlan<FixedFinder, FixedFinderRev> {
    pub fn from_opt_fixed(opt: &Opt) -> Result<Self> {
        Self::from_opt_fixed_with_custom_delimiter(opt, &opt.delimiter)
    }

    pub fn from_opt_fixed_with_custom_delimiter(opt: &Opt, delimiter: &[u8]) -> Result<Self> {
        let finder = FixedFinder::new(delimiter);
        let finder_rev = FixedFinderRev::new(delimiter);
        Self::from_opt_with_finders(opt, finder, finder_rev)
    }
}

impl FieldPlan<FixedGreedyFinder, FixedGreedyFinderRev> {
    pub fn from_opt_fixed_greedy(opt: &Opt) -> Result<Self> {
        let finder = FixedGreedyFinder::new(&opt.delimiter);
        let finder_rev = FixedGreedyFinderRev::new(&opt.delimiter);
        Self::from_opt_with_finders(opt, finder, finder_rev)
    }
}

#[cfg(feature = "regex")]
impl FieldPlan<RegexFinder, RegexFinder> {
    pub fn from_opt_regex(opt: &Opt, regex: Regex, trim_empty: bool) -> Result<Self> {
        let finder = RegexFinder::new_with_trim(regex.clone(), trim_empty);
        // XXX TBD We are not actually going to use "finder_rev"
        // because regexes can't search backwards (I'm simplifying).
        // Is there a way to avoid the fake finder?
        let finder_rev = RegexFinder::new_with_trim(regex, trim_empty);
        Self::from_opt_with_finders(opt, finder, finder_rev)
    }
}

// Type aliases for common configurations
pub type FixedFieldPlan = FieldPlan<FixedFinder, FixedFinderRev>;

#[cfg(feature = "regex")]
pub type RegexFieldPlan = FieldPlan<RegexFinder, RegexFinder>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::UserBoundsList;
    use std::str::FromStr;

    fn make_fields_opt() -> Opt {
        Opt {
            bounds_type: BoundsType::Fields,
            delimiter: "-".into(),
            ..Opt::default()
        }
    }

    #[test]
    fn extract_fields_should_fail_if_without_bounds() {
        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList {
            list: Vec::new(),
            last_interesting_field: Side::with_pos_inf(),
        };

        let maybe_plan = FieldPlan::from_opt_fixed(&opt);
        assert_eq!(
            maybe_plan.unwrap_err().to_string(),
            "No indices found in bounds"
        );
    }

    #[test]
    fn extract_fields_basic() {
        let line = b"a--b--c";

        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("1,2,3").unwrap();

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields, vec![0..1, 3..4, 6..7]);
    }

    #[test]
    fn extract_fields_out_of_order() {
        let line = b"foo--bar--baz";

        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("3,1").unwrap();

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields[2], 10..13);
        assert_eq!(plan.positive_fields[0], 0..3);
    }

    #[test]
    fn extract_fields_multibyte_delim_and_missing_field() {
        let line = b"x==y==z";

        let mut opt = make_fields_opt();
        opt.delimiter = "==".into();
        opt.bounds = UserBoundsList::from_str("1,4").unwrap();

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        let result = extract_fields_using_pos_indices(line, &mut plan);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Out of bounds: 4");
    }

    #[test]
    fn extract_fields_no_delimiter() {
        let line = b"singlefield";

        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("1").unwrap();

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();

        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields, vec![0..11]);
        assert_eq!(plan.negative_fields, Vec::<Range<usize>>::new());
    }

    #[test]
    fn test_field_plan_from_bounds_single_and_range() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,2,4").unwrap();

        let plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3]);
    }

    #[test]
    fn test_field_plan_from_bounds_range_and_single_out_of_order() {
        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("2:3,1").unwrap();

        let plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_multiple_ranges_and_order() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("4:5,:2").unwrap();

        let plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3, 4]);
    }

    #[test]
    fn test_field_plan_from_bounds_duplicate_fields() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1:2,2:3").unwrap();

        let plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        // 1:2 gives 0,1; 2:3 gives 1,2; deduped order: 0,1,2
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_full_range() {
        // Use "1:-1" to mean all fields (from 1 to last)

        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1:-1").unwrap();

        let plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0]);
        assert_eq!(plan.negative_indices, vec![0]);
    }

    #[test]
    fn extract_empty_line() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,-1").unwrap();

        let line = b"";
        let expected_pos_indices = vec![0];
        let expected_neg_indices = vec![0];
        #[allow(clippy::single_range_in_vec_init)]
        let expected_ranges = vec![Side::max_right()..Side::max_right()];

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);
        extract_fields_using_pos_indices(line, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, expected_ranges);

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_neg_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);
        extract_fields_using_negative_indices(line, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, expected_ranges);

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);
        let res = extract_every_field(line, &mut plan).unwrap();
        assert_eq!(res, Some(0));
        assert_eq!(plan.positive_fields, expected_ranges);
    }

    #[test]
    fn test_extract_positive_fields() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,2,4").unwrap();

        let line = b"a-b-c-d-e";
        let expected_indices = vec![0, 1, 3];
        let expected_ranges = vec![0..1, 2..3, Side::max_right()..Side::max_right(), 6..7];

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_indices);
        extract_fields_using_pos_indices(line, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, expected_ranges);

        let mut plan = FieldPlan::from_opt_fixed_greedy(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_indices);
        extract_fields_using_pos_indices(line, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, expected_ranges);

        #[cfg(feature = "regex")]
        {
            let re = Regex::new("-").unwrap();
            let mut plan = FieldPlan::from_opt_regex(&opt, re, false).unwrap();
            assert_eq!(plan.positive_indices, expected_indices);
            extract_fields_using_pos_indices(line, &mut plan).unwrap();
            assert_eq!(plan.positive_fields, expected_ranges);
        }
    }

    #[test]
    fn test_extract_positive_fields_with_fields_of_different_range() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("2").unwrap();

        let line1 = b"a-b";
        let line2 = b"foo-bar";
        let line3 = b"baaz-hello";
        let expected_pos_indices = vec![1];

        // from_opt_mem
        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);

        extract_fields_using_pos_indices(line1, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 2..3]
        );

        extract_fields_using_pos_indices(line2, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 4..7]
        );

        extract_fields_using_pos_indices(line3, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 5..10]
        );

        // from_opt_fixed_greedy
        let mut plan = FieldPlan::from_opt_fixed_greedy(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);

        extract_fields_using_pos_indices(line1, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 2..3]
        );

        extract_fields_using_pos_indices(line2, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 4..7]
        );

        extract_fields_using_pos_indices(line3, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 5..10]
        );
    }

    #[test]
    fn test_extract_negative_fields() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("-5,-4,-2").unwrap();

        let line = b"a-b-c-d-e";
        let expected_indices = vec![1, 3, 4];
        let expected_ranges = vec![
            Side::max_right()..Side::max_right(),
            6..7,
            Side::max_right()..Side::max_right(),
            2..3,
            0..1,
        ];

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.negative_indices, expected_indices);
        extract_fields_using_negative_indices(line, &mut plan).unwrap();
        assert_eq!(plan.negative_fields, expected_ranges);

        let mut plan = FieldPlan::from_opt_fixed_greedy(&opt).unwrap();
        assert_eq!(plan.negative_indices, expected_indices);
        extract_fields_using_negative_indices(line, &mut plan).unwrap();
        assert_eq!(plan.negative_fields, expected_ranges);

        // Actually there's no regex rev case (regex implies
        // extract_every_field). The test below would fail.

        // let re = Regex::new("-").unwrap();
        // let mut plan = FieldPlan::from_opt_regex(&opt, re, false).unwrap();
        // assert_eq!(plan.negative_indices, expected_indices);
        // extract_fields_using_negative_indices(line, &mut plan).unwrap();
        // assert_eq!(plan.negative_fields, expected_ranges);
    }

    #[test]
    fn test_extract_positive_fields_greedy_multybyte() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();

        let line = b"a--b--c";
        let expected_indices = vec![0, 2];
        let expected_ranges = vec![0..1, Side::max_right()..Side::max_right(), 6..7];

        let mut plan = FieldPlan::from_opt_fixed_greedy(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_indices);
        extract_fields_using_pos_indices(line, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, expected_ranges);
    }

    #[test]
    fn test_extract_negative_fields_greedy_multybyte() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("-3,-1").unwrap();

        let line = b"a--b--c";
        let expected_indices = vec![0, 2];
        let expected_ranges = vec![6..7, Side::max_right()..Side::max_right(), 0..1];

        let mut plan = FieldPlan::from_opt_fixed_greedy(&opt).unwrap();
        assert_eq!(plan.negative_indices, expected_indices);
        extract_fields_using_negative_indices(line, &mut plan).unwrap();
        assert_eq!(plan.negative_fields, expected_ranges);
    }

    #[test]
    fn test_extract_every_field() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,3,-3,-1").unwrap();

        let line = b"a-b-c-d";
        let expected_pos_indices = vec![0, 2];
        let expected_neg_indices = vec![0, 2];
        let expected_pos_ranges = vec![0..1, 2..3, 4..5, 6..7];
        let expected_neg_ranges = vec![6..7, Side::max_right()..Side::max_right(), 2..3];

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);
        let num_fields = extract_every_field(line, &mut plan).unwrap();
        assert_eq!(num_fields, Some(4));
        assert_eq!(plan.positive_fields, expected_pos_ranges);
        assert_eq!(plan.negative_fields, expected_neg_ranges);
    }

    #[test]
    fn test_extract_positive_fields_out_of_bound() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("2,3").unwrap();

        let line1 = b"a-b-c";
        let line2 = b"foo";
        let line3 = b"baaz-hello-world";
        let expected_pos_indices = vec![1, 2];

        // from_opt_mem
        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);

        extract_fields_using_pos_indices(line1, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 2..3, 4..5]
        );

        let res = extract_fields_using_pos_indices(line2, &mut plan);
        assert_eq!(res.unwrap_err().to_string(), "Out of bounds: 2");
        assert_eq!(
            plan.positive_fields,
            vec![
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right()
            ]
        );

        extract_fields_using_pos_indices(line3, &mut plan).unwrap();
        assert_eq!(
            plan.positive_fields,
            vec![Side::max_right()..Side::max_right(), 5..10, 11..16]
        );
    }

    #[test]
    fn test_extract_negative_fields_out_of_bound() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("-2,-3").unwrap();

        let line1 = b"a-b-c";
        let line2 = b"foo";
        let line3 = b"baaz-hello-world";
        let expected_neg_indices = vec![1, 2];

        // from_opt_mem
        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.negative_indices, expected_neg_indices);

        extract_fields_using_negative_indices(line1, &mut plan).unwrap();
        assert_eq!(
            plan.negative_fields,
            vec![Side::max_right()..Side::max_right(), 2..3, 0..1]
        );

        let res = extract_fields_using_negative_indices(line2, &mut plan);
        assert_eq!(res.unwrap_err().to_string(), "Out of bounds: -2");
        assert_eq!(
            plan.negative_fields,
            vec![
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right()
            ]
        );

        extract_fields_using_negative_indices(line3, &mut plan).unwrap();
        assert_eq!(
            plan.negative_fields,
            vec![Side::max_right()..Side::max_right(), 5..10, 0..4]
        );
    }

    #[test]
    fn test_extract_every_field_out_of_bound_positive() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("2,3,-1").unwrap();

        let line1 = b"a-b-c";
        let line2 = b"foo";
        let line3 = b"baaz-hello-world";
        let expected_pos_indices = vec![1, 2];
        let expected_neg_indices = vec![0];

        // from_opt_mem
        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);

        extract_every_field(line1, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, vec![0..1, 2..3, 4..5]);
        assert_eq!(plan.negative_fields, vec![4..5]);

        let res = extract_every_field(line2, &mut plan);
        assert_eq!(res.unwrap_err().to_string(), "Out of bounds: 2");
        // Even if it was out of bounds, we expect extract_every_field
        // to have filled all positive and negative fields anyway
        // (because we may have fallbacks for the positive fields
        // and later move to print the negatives).
        assert_eq!(plan.positive_fields, vec![0..3]);
        assert_eq!(plan.negative_fields, vec![0..3]);

        extract_every_field(line3, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, vec![0..4, 5..10, 11..16]);
        // extract_every_field extract every positive field, but
        // it keep only the necessary negative_fields around.
        assert_eq!(plan.negative_fields, vec![11..16]);
    }

    #[test]
    fn test_extract_every_field_out_of_bound_negative() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("-2,-3").unwrap();

        let line1 = b"a-b-c";
        let line2 = b"foo";
        let line3 = b"baaz-hello-world";
        let expected_pos_indices = Vec::<usize>::new();
        let expected_neg_indices = vec![1, 2];

        // from_opt_mem
        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();
        assert_eq!(plan.positive_indices, expected_pos_indices);
        assert_eq!(plan.negative_indices, expected_neg_indices);

        extract_every_field(line1, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, vec![0..1, 2..3, 4..5]);
        assert_eq!(
            plan.negative_fields,
            vec![Side::max_right()..Side::max_right(), 2..3, 0..1]
        );

        let res = extract_every_field(line2, &mut plan);
        assert_eq!(res.unwrap_err().to_string(), "Out of bounds: -2");
        // Even if it was out of bounds, we expect extract_every_field
        // to have filled all positive fields anyway
        // (because we may have fallbacks for the positive fields
        // and later move to print the negatives).
        assert_eq!(plan.positive_fields, vec![0..3]);
        assert_eq!(
            plan.negative_fields,
            vec![
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right(),
                Side::max_right()..Side::max_right()
            ]
        );

        extract_every_field(line3, &mut plan).unwrap();
        assert_eq!(plan.positive_fields, vec![0..4, 5..10, 11..16]);
        assert_eq!(
            plan.negative_fields,
            vec![Side::max_right()..Side::max_right(), 5..10, 0..4]
        );
    }

    #[test]
    fn test_get_field() {
        let opt = make_fields_opt();

        let mut plan = FieldPlan::from_opt_fixed(&opt).unwrap();

        let line = "foo-bar-baz";
        // simulate search, we're not testing that here
        plan.positive_fields = vec![0..3, 4..7, 8..11];
        plan.negative_fields = vec![8..11, 4..7, 0..3];

        let b = UserBounds::from_str("1").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 0..3);

        let b = UserBounds::from_str(":2").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 0..7);

        let b = UserBounds::from_str("2:").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 4..11);

        let b = UserBounds::from_str("-1").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 8..11);

        let b = UserBounds::from_str(":-2").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 0..7);

        let b = UserBounds::from_str("-2:").unwrap();
        assert_eq!(plan.get_field(&b, line.len()).unwrap(), 4..11);
    }
}
