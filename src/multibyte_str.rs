//! Efficient multibyte delimiter string field extraction for `tuc`.
//!
//! This module provides a fast, allocation-minimizing algorithm for extracting fields from a delimited string,
//! given a set of user-specified bounds (possibly with ranges and out-of-order indices).
//!
//! The main entrypoint is `pub fn extract_fields`, which takes a line, delimiter, and a list of bounds,
//! and yields the requested fields in user order, minimizing allocations and delimiter scans.

use std::{ops::Range, usize};

use crate::{
    bounds::{BoundOrFiller, BoundsType, Side, UserBounds},
    options::Opt,
};
use anyhow::{Result, bail};
use bstr::ByteSlice;
use itertools::Itertools;
use regex::bytes::Regex;

// override dbg macro with a no-op to avoid debug output in release builds
#[cfg(not(debug_assertions))]
macro_rules! dbg {
    ($($arg:tt)*) => {};
}

pub trait DelimiterFinder {
    type Iter<'a>: Iterator<Item = Range<usize>> + 'a
    where
        Self: 'a;
    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a>;
}

// Implementations for different finder types
pub struct MemmemFinder {
    finder: memchr::memmem::Finder<'static>,
    len: usize,
}

impl MemmemFinder {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::Finder::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for MemmemFinder {
    type Iter<'a> =
        std::iter::Map<memchr::memmem::FindIter<'a, 'a>, Box<dyn Fn(usize) -> Range<usize> + 'a>>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let len = self.len;
        self.finder
            .find_iter(line)
            .map(Box::new(move |idx| idx..idx + len))
    }
}

pub struct MemmemRevFinder {
    finder: memchr::memmem::FinderRev<'static>,
    len: usize,
}

impl MemmemRevFinder {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::FinderRev::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for MemmemRevFinder {
    type Iter<'a> = std::iter::Map<
        memchr::memmem::FindRevIter<'a, 'a>,
        Box<dyn Fn(usize) -> Range<usize> + 'a>,
    >;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let len = self.len;
        self.finder
            .rfind_iter(line)
            .map(Box::new(move |idx| idx..idx + len))
    }
}

#[cfg(feature = "regex")]
pub struct RegexFinder {
    regex: Regex,
    trim_empty: bool,
}

#[cfg(feature = "regex")]
impl RegexFinder {
    pub fn new(regex: Regex, trim_empty: bool) -> Self {
        Self { regex, trim_empty }
    }
}

#[cfg(feature = "regex")]
impl DelimiterFinder for RegexFinder {
    type Iter<'a> = Box<dyn Iterator<Item = Range<usize>> + 'a>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        if self.trim_empty {
            let line_len = line.len();
            Box::new(
                self.regex
                    .find_iter(line)
                    .filter(move |m| {
                        !((m.start() == 0 && m.end() == 0)
                            || (m.start() == line_len && m.end() == line_len))
                    })
                    .map(|m| m.start()..m.end()),
            )
        } else {
            Box::new(self.regex.find_iter(line).map(|m| m.start()..m.end()))
        }
    }
}

/// A compact, sorted, deduplicated list of field indices to extract, in user order.
pub struct FieldPlan<F, R>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    /// The user-specified order of fields/ranges (flattened to indices, 0-based).
    indices: Vec<i32>,
    positive_indices: Vec<usize>,
    negative_indices: Vec<usize>,
    pub positive_fields: Vec<Range<usize>>,
    negative_fields: Vec<Range<usize>>,
    pub extract_func: fn(&[u8], &mut FieldPlan<F, R>) -> Result<Option<usize>>,
    finder: F,
    finder_rev: R,
    need_num_fields: bool,
}
impl<F, R> FieldPlan<F, R>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    /// Build a plan from a list of UserBounds, for a line with unknown field count.
    pub fn from_opt_with_finders(opt: &Opt, finder: F, finder_rev: R) -> Result<Self> {
        // Create a vector to hold the indices. At most we will have as many indices as bounds, doubled to hold both ends of ranges.
        let mut indices: Vec<i32> = Vec::with_capacity(opt.bounds.len() * 2);

        let trim_empty_delimiter_at_bounds = opt.bounds_type == BoundsType::Characters;
        let need_num_fields = opt.only_delimited
            || opt.complement
            || opt.json
            || (opt.bounds_type == BoundsType::Characters && opt.replace_delimiter.is_some());

        let maybe_regex: Option<&Regex> = opt.regex_bag.as_ref().map(|x| {
            if opt.greedy_delimiter {
                &x.greedy
            } else {
                &x.normal
            }
        });

        // XXX should we expose this to reuse it?
        let should_compress_delimiter = opt.compress_delimiter
            && (opt.bounds_type == BoundsType::Fields || opt.bounds_type == BoundsType::Lines);

        // First collect all indices from bounds, keeping duplicates and original order.
        for bof in opt.bounds.iter() {
            if let BoundOrFiller::Bound(b) = bof {
                indices.push(match b.l {
                    Side::Some(l) => l,
                    Side::Continue => 1,
                });

                if let Side::Some(r) = b.r {
                    indices.push(r);
                } // else ignore "continue" as right bound
            }
        }

        // Then sort and deduplicate the indices.
        indices.sort_unstable();
        indices.dedup();

        // XXX these two can perhaps be two mutable slices?

        // Collect positive indices as usize
        let mut positive_indices: Vec<usize> = indices
            .iter()
            .filter(|x| **x >= 0)
            .map(|&x| x as usize - 1) // convert to 0-indexed
            .collect();
        positive_indices.sort_unstable();

        // Collect negative indices, sorted from largest to smallest
        let mut negative_indices: Vec<usize> = indices
            .iter()
            .filter(|x| **x < 0)
            .rev()
            .map(|x| x.unsigned_abs() as usize - 1) // convert to 0-indexed
            .collect();

        negative_indices.sort_unstable();

        //dbg!(&indices, &positive_indices, &negative_indices);

        let max_field_to_search_pos = positive_indices.last().map(|x| x + 1).unwrap_or(0);
        let max_field_to_search_neg = negative_indices.last().map(|x| x + 1).unwrap_or(0);

        let extract_func = if need_num_fields {
            extract_every_field
        } else {
            match (!positive_indices.is_empty(), !negative_indices.is_empty()) {
                (false, false) => bail!("No indices found in bounds"), // invariant, shouldn't occur
                (true, false) => extract_fields_using_pos_indices,
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

        // // Build the delimiter strategy once
        // let finder = if should_compress_delimiter
        //     && maybe_regex.is_some()
        //     && let Some(replace_delimiter) = &opt.replace_delimiter
        // {
        //     // This is the scenario where we update early on the line to replace
        //     // the delimiter, so later on we must search for the fields using
        //     // the new delimiter.
        //     let finder = memchr::memmem::Finder::new(replace_delimiter).into_owned();
        //     let len = replace_delimiter.len();
        //     DelimiterStrategy::Fixed(finder, len)
        // } else if let Some(regex) = maybe_regex {
        //     #[cfg(feature = "regex")]
        //     {
        //         DelimiterStrategy::Regex(regex, trim_empty_delimiter_at_bounds)
        //     }
        //     #[cfg(not(feature = "regex"))]
        //     {
        //         unreachable!()
        //     }
        // } else if opt.greedy_delimiter {
        //     let finder = memchr::memmem::Finder::new(&opt.delimiter).into_owned();
        //     let len = opt.delimiter.len();
        //     DelimiterStrategy::FixedGreedy(finder, len)
        // } else {
        //     let finder = memchr::memmem::Finder::new(&opt.delimiter).into_owned();
        //     let len = opt.delimiter.len();
        //     DelimiterStrategy::Fixed(finder, len)
        // };

        // let finder_rev = if let Some(regex) = maybe_regex {
        //     #[cfg(feature = "regex")]
        //     {
        //         // Storing regular regex, but we won't use it for reverse search
        //         // (we'll fallback to retrieve every field)
        //         DelimiterStrategy::Regex(regex, trim_empty_delimiter_at_bounds)
        //     }
        //     #[cfg(not(feature = "regex"))]
        //     {
        //         unreachable!()
        //     }
        // } else {
        //     let rfinder = memchr::memmem::FinderRev::new(&opt.delimiter).into_owned();
        //     let len = opt.delimiter.len();
        //     DelimiterStrategy::FixedRev(rfinder, len)
        // };

        Ok(FieldPlan {
            indices,
            positive_indices,
            negative_indices,
            // XXX maybe I can reduce the capacity here
            // by storing fields by original index position?
            positive_fields: vec![usize::MAX..usize::MAX; max_field_to_search_pos], // initialize with empty ranges
            negative_fields: vec![usize::MAX..usize::MAX; max_field_to_search_neg], // initialize with empty ranges,
            extract_func,
            finder,
            finder_rev,
            need_num_fields,
        })
    }

    pub fn get_field(&self, b: &UserBounds, line_len: usize) -> Result<Range<usize>> {
        // if a side is negative, search in negative_fields, otherwise
        // in positive_fields
        //dbg!("get field", &b);
        let start = match b.l {
            Side::Some(l) => {
                (if l < 0 {
                    self.negative_fields
                        .get(l.unsigned_abs() as usize - 1)
                        .and_then(|x| if x.start == usize::MAX { None } else { Some(x) })
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", l))?
                } else {
                    self.positive_fields
                        .get(l as usize - 1)
                        .and_then(|x| if x.start == usize::MAX { None } else { Some(x) })
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", l,))?
                })
                .start
            }
            Side::Continue => 0,
        };

        let end = match b.r {
            Side::Some(r) => {
                (if r < 0 {
                    self.negative_fields
                        .get(r.unsigned_abs() as usize - 1)
                        .and_then(|x| if x.start == usize::MAX { None } else { Some(x) })
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", r))?
                } else {
                    self.positive_fields
                        .get(r as usize - 1)
                        .and_then(|x| if x.start == usize::MAX { None } else { Some(x) })
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", r,))?
                })
                .end
            }
            Side::Continue => line_len,
        };

        if end < start {
            // `start` can't ever be greater than end
            bail!("Field left value cannot be greater than right value");
        }

        Ok(start..end)
    }
}

pub fn extract_fields_using_pos_indices<F, R>(
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

    //dbg!("extract_fields_using_pos_indices");

    //dbg!(line.to_str_lossy(), &plan.indices);
    let mut seen = 0;
    // Define an iterator over the delimiter positions, starting with a sentinel value.
    // This allows us to get the starting position (0) of the first field.

    // let mut all: Vec<Range<usize>> = std::iter::once(std::ops::Range { start: 0, end: 0 })
    //     .chain(plan.finder.find_ranges(line))
    //     .collect();

    // //dbg!(&all);

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
        //dbg!(desired_field, seen);
        let f_start = delim_iterator
            .nth(desired_field - seen)
            .ok_or_else(|| {
                plan.positive_fields[desired_field] = Range {
                    start: usize::MAX,
                    end: usize::MAX,
                };
                anyhow::anyhow!("Out of bounds: {}", desired_field + 1)
            })?
            .end;

        //dbg!(f_start);
        //dbg!(delim_iterator.peek());
        let f_end = delim_iterator.peek().unwrap_or(&eol_range).start;
        //dbg!(f_end);

        plan.positive_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(None)
}

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

    //dbg!("extract_fields_using_negative_indices");
    // And now let's do the inverse.
    // We will iterate the line again, right to left,
    // looking for the fields matching negative bounds.

    // -2  -1
    // a..-b
    // 0..12

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
        //dbg!(desired_field, seen);

        let f_end = delim_iterator
            .nth(desired_field - seen) // unwrap or bail with bail! macro if out of bounds
            .ok_or_else(|| {
                plan.negative_fields[desired_field] = Range {
                    start: usize::MAX,
                    end: usize::MAX,
                };
                anyhow::anyhow!("Out of bounds: -{}", desired_field + 1)
            })?
            .start;

        let f_start = delim_iterator.peek().unwrap_or(&start_range).end;

        //dbg!(f_start);
        //dbg!(f_end);

        plan.negative_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(None)
}

fn extract_every_field<F, R>(line: &[u8], plan: &mut FieldPlan<F, R>) -> Result<Option<usize>>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    //dbg!("extract_every_field");

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

    // XXX TODO we are not "zeroing" with usize::umax the unmatched negative_indices

    for i in 0..plan.negative_indices.len() {
        let desired_field = plan.negative_indices[i];

        if num_fields < desired_field + 1 {
            bail!("Out of bounds: -{}", desired_field + 1);
        }

        let field = &plan.positive_fields[num_fields - desired_field - 1];

        let f_start = field.start;
        let f_end = field.end;

        plan.negative_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };
    }

    Ok(Some(num_fields))
}

// Convenience constructor functions
impl FieldPlan<MemmemFinder, MemmemRevFinder> {
    pub fn from_opt_memmem(opt: &Opt) -> Result<Self> {
        let finder = MemmemFinder::new(&opt.delimiter);
        let finder_rev = MemmemRevFinder::new(&opt.delimiter);
        Self::from_opt_with_finders(opt, finder, finder_rev)
    }
}

#[cfg(feature = "regex")]
impl FieldPlan<RegexFinder, RegexFinder> {
    pub fn from_opt_regex(opt: &Opt, regex: Regex, trim_empty: bool) -> Result<Self> {
        let finder = RegexFinder::new(regex.clone(), trim_empty);
        let finder_rev = RegexFinder::new(regex, trim_empty);
        Self::from_opt_with_finders(opt, finder, finder_rev)
    }
}

// Type aliases for common configurations
pub type MemmemFieldPlan = FieldPlan<MemmemFinder, MemmemRevFinder>;

#[cfg(feature = "regex")]
pub type RegexFieldPlan = FieldPlan<RegexFinder, RegexFinder>;

// Simple enum for delimiter strategy: Regex or Finder (owned)
enum DelimiterStrategy<'a> {
    #[cfg(feature = "regex")]
    Regex(&'a regex::bytes::Regex, bool),
    Fixed(memchr::memmem::Finder<'a>, usize),
    FixedRev(memchr::memmem::FinderRev<'a>, usize),
    FixedGreedy(memchr::memmem::Finder<'a>, usize),
}

impl<'a> DelimiterStrategy<'a> {
    fn find_ranges(&'a self, line: &'a [u8]) -> DelimiterFindIter<'a> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterStrategy::Regex(re, false) => DelimiterFindIter::Regex(re.find_iter(line)),
            // This case is useful when we are cutting by characters,
            // because we want to skip empty matches at the start and end of the line.
            DelimiterStrategy::Regex(re, true) => {
                // If we're cutting by characters, the delimiter is the empty strings
                // and it will match at start and end of line, e.g. _f_o_o_. We drop those matches.
                DelimiterFindIter::RegexTrimmed(re.find_iter(line).skip_while(Box::new(move |m| {
                    (m.start() == 0 && m.end() == 0)
                        || (m.start() == line.len() && m.end() == line.len())
                })))
            }
            DelimiterStrategy::Fixed(finder, len) => {
                DelimiterFindIter::Fixed(finder.find_iter(line), *len)
            }
            DelimiterStrategy::FixedRev(finder_rev, len) => {
                DelimiterFindIter::FixedRev(finder_rev.rfind_iter(line), *len)
            }
            DelimiterStrategy::FixedGreedy(finder, len) => {
                // Define the mapping function
                fn make_range_from_tuple((start, len): (usize, usize)) -> Range<usize> {
                    Range {
                        start,
                        end: start + len,
                    }
                }

                // Define the coalescing function
                fn coalesce_ranges(
                    prev: Range<usize>,
                    curr: Range<usize>,
                ) -> Result<Range<usize>, (Range<usize>, Range<usize>)> {
                    if prev.end == curr.start {
                        Ok(prev.start..curr.end)
                    } else {
                        Err((prev, curr))
                    }
                }

                DelimiterFindIter::FixedGreedy(
                    finder
                        .find_iter(line)
                        .zip(std::iter::repeat(*len))
                        .map(make_range_from_tuple as fn((usize, usize)) -> Range<usize>) // Cast here
                        .coalesce(
                            coalesce_ranges
                                as fn(
                                    Range<usize>,
                                    Range<usize>,
                                )
                                    -> Result<Range<usize>, (Range<usize>, Range<usize>)>,
                        ), // Cast here too
                )
            }
        }
    }
}

type GreedyCoalesceIter<'a> = itertools::structs::Coalesce<
    std::iter::Map<
        std::iter::Zip<memchr::memmem::FindIter<'a, 'a>, std::iter::Repeat<usize>>,
        fn((usize, usize)) -> Range<usize>,
    >,
    fn(Range<usize>, Range<usize>) -> Result<Range<usize>, (Range<usize>, Range<usize>)>,
>;

enum DelimiterFindIter<'a> {
    #[cfg(feature = "regex")]
    Regex(regex::bytes::Matches<'a, 'a>),
    RegexTrimmed(
        std::iter::SkipWhile<
            regex::bytes::Matches<'a, 'a>,
            Box<dyn FnMut(&regex::bytes::Match) -> bool + 'a>,
        >,
    ),
    Fixed(memchr::memmem::FindIter<'a, 'a>, usize),
    FixedRev(memchr::memmem::FindRevIter<'a, 'a>, usize),
    FixedGreedy(GreedyCoalesceIter<'a>),
}

impl<'a> Iterator for DelimiterFindIter<'a> {
    type Item = Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterFindIter::Regex(iter) => iter.next().map(|m| m.start()..m.end()),
            #[cfg(feature = "regex")]
            DelimiterFindIter::RegexTrimmed(iter) => iter.next().map(|m| m.start()..m.end()),
            DelimiterFindIter::Fixed(iter, len) => iter.next().map(|idx| idx..idx + *len),
            DelimiterFindIter::FixedRev(iter, len) => iter.next().map(|idx| idx..idx + *len),
            DelimiterFindIter::FixedGreedy(iter) => iter.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::{UserBounds, UserBoundsList};
    use std::str::FromStr;

    fn make_fields_opt() -> Opt {
        Opt {
            bounds_type: BoundsType::Fields,
            delimiter: "-".into(),
            ..Opt::default()
        }
    }

    #[test]
    fn extract_fields_basic() {
        let line = b"a--b--c";

        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("1,2,3").unwrap();

        let mut plan = FieldPlan::from_opt(&opt).unwrap();
        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields, vec![0..1, 3..4, 6..7]);
    }

    #[test]
    fn extract_fields_out_of_order() {
        let line = b"foo--bar--baz";

        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("3,1").unwrap();

        let mut plan = FieldPlan::from_opt(&opt).unwrap();
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

        let mut plan = FieldPlan::from_opt(&opt).unwrap();
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

        let mut plan = FieldPlan::from_opt(&opt).unwrap();

        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields, vec![0..11]);
        assert_eq!(plan.negative_fields, Vec::<Range<usize>>::new());
    }

    #[test]
    fn test_field_plan_from_bounds_single_and_range() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1,2,4").unwrap();

        let plan = FieldPlan::from_opt(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3]);
    }

    #[test]
    fn test_field_plan_from_bounds_range_and_single_out_of_order() {
        let mut opt = make_fields_opt();
        opt.delimiter = "--".into();
        opt.bounds = UserBoundsList::from_str("2:3,1").unwrap();

        let plan = FieldPlan::from_opt(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_multiple_ranges_and_order() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("4:5,:2").unwrap();

        let plan = FieldPlan::from_opt(&opt).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3, 4]);
    }

    #[test]
    fn test_field_plan_from_bounds_duplicate_fields() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1:2,2:3").unwrap();

        let plan = FieldPlan::from_opt(&opt).unwrap();
        // 1:2 gives 0,1; 2:3 gives 1,2; deduped order: 0,1,2
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_full_range() {
        // Use "1:-1" to mean all fields (from 1 to last)

        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("1:-1").unwrap();

        let plan = FieldPlan::from_opt(&opt).unwrap();
        assert_eq!(plan.indices, vec![-1, 1]);
        assert_eq!(plan.positive_indices, vec![0]);
        assert_eq!(plan.negative_indices, vec![0]);
    }
}
