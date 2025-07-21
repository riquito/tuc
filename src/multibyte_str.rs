//! Efficient multibyte delimiter string field extraction for `tuc`.
//!
//! This module provides a fast, allocation-minimizing algorithm for extracting fields from a delimited string,
//! given a set of user-specified bounds (possibly with ranges and out-of-order indices).
//!
//! The main entrypoint is `pub fn extract_fields`, which takes a line, delimiter, and a list of bounds,
//! and yields the requested fields in user order, minimizing allocations and delimiter scans.

use std::ops::Range;

use crate::bounds::{BoundOrFiller, Side, UserBounds};
use anyhow::{Result, bail};
use regex::bytes::Regex;

// override dbg macro with a no-op to avoid debug output in release builds
#[cfg(not(debug_assertions))]
macro_rules! dbg {
    ($($arg:tt)*) => {};
}

/// A compact, sorted, deduplicated list of field indices to extract, in user order.
pub struct FieldPlan<'a> {
    /// The user-specified order of fields/ranges (flattened to indices, 0-based).
    indices: Vec<i32>,
    positive_indices: Vec<usize>,
    negative_indices: Vec<usize>,
    pub positive_fields: Vec<Range<usize>>,
    negative_fields: Vec<Range<usize>>,
    pub extract_func: fn(&[u8], &mut FieldPlan) -> Result<()>,
    finder: DelimiterStrategy<'a>,
    finder_rev: DelimiterStrategy<'a>,
}

impl<'a> FieldPlan<'a> {
    /// Build a plan from a list of UserBounds, for a line with unknown field count.
    ///
    /// This flattens all bounds into a Vec<usize> of indices, 0-indexed, sorted and unique.
    /// "Continue" as right bound is ignored.
    ///
    /// e.g. for bounds `1,2:8,4:,:2`, it would produce
    /// `[0,1,3,7]`
    pub fn from_bounds(
        bounds: &[BoundOrFiller],
        needle: &'a [u8],
        maybe_regex: Option<&'a Regex>,
    ) -> Result<Self> {
        // Create a vector to hold the indices. At most we will have as many indices as bounds, doubled to hold both ends of ranges.
        let mut indices: Vec<i32> = Vec::with_capacity(bounds.len() * 2);

        // First collect all indices from bounds, keeping duplicates and original order.
        for bof in bounds {
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

        // XXX to test
        //         let indices: Vec<i32> = bounds.iter().flat_map(|bof| {
        //     if let BoundOrFiller::Bound(b) = bof {
        //         let left = match b.l {
        //             Side::Some(l) => Some(l),
        //             Side::Continue => Some(0),
        //         };
        //         let right = match b.r {
        //             Side::Some(r) => Some(r),
        //             Side::Continue => None,
        //         };
        //         left.into_iter().chain(right)
        //     } else {
        //         std::iter::empty()
        //     }
        // }).collect();

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

        dbg!(&indices, &positive_indices, &negative_indices);

        let max_field_to_search_pos = positive_indices.last().map(|x| x + 1).unwrap_or(0);
        let max_field_to_search_neg = negative_indices.last().map(|x| x + 1).unwrap_or(0);

        let extract_func = match (max_field_to_search_pos, max_field_to_search_neg) {
            (0, 0) => bail!("No indices found in bounds"), // invariant, shouldn't occur
            (_, 0) => extract_fields_using_pos_indices,
            (0, _) => extract_fields_using_negative_indices,
            _ => |line: &[u8], plan: &mut FieldPlan| {
                extract_fields_using_pos_indices(line, plan)?;
                extract_fields_using_negative_indices(line, plan)?;
                Ok(())
            },
        };

        // Build the delimiter strategy once
        let finder = if let Some(regex) = maybe_regex {
            #[cfg(feature = "regex")]
            {
                DelimiterStrategy::Regex(regex)
            }
            #[cfg(not(feature = "regex"))]
            {
                unreachable!()
            }
        } else {
            let finder = memchr::memmem::Finder::new(&needle).into_owned();
            let len = needle.len();
            DelimiterStrategy::Memmem(finder, len)
        };

        let finder_rev = if let Some(regex) = maybe_regex {
            #[cfg(feature = "regex")]
            {
                // DelimiterStrategy::RegexRev(regex)

                use core::panic;
                panic!("RegexRev is not implemented yet");
            }
            #[cfg(not(feature = "regex"))]
            {
                unreachable!()
            }
        } else {
            let rfinder = memchr::memmem::FinderRev::new(&needle).into_owned();
            let len = needle.len();
            DelimiterStrategy::MemmemRev(rfinder, len)
        };

        Ok(FieldPlan {
            indices,
            positive_indices,
            negative_indices,
            // XXX maybe I can reduce the capacity here
            // by storing fields by original index position?
            positive_fields: vec![0..0; max_field_to_search_pos], // initialize with empty ranges
            negative_fields: vec![0..0; max_field_to_search_neg], // initialize with empty ranges,
            extract_func,
            finder,
            finder_rev,
        })
    }

    pub fn get_field(&self, b: &UserBounds, line_len: usize) -> Result<Range<usize>> {
        // if a side is negative, search in negative_fields, otherwise
        // in positive_fields
        let start = match b.l {
            Side::Some(l) => {
                (if l < 0 {
                    self.negative_fields
                        .get(l.unsigned_abs() as usize - 1)
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", l))?
                } else {
                    self.positive_fields.get(l as usize - 1).ok_or_else(|| {
                        anyhow::anyhow!("Out of bounds: {} (max {})", l, self.positive_fields.len())
                    })?
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
                        .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", r))?
                } else {
                    self.positive_fields.get(r as usize - 1).ok_or_else(|| {
                        anyhow::anyhow!("Out of bounds: {} (max {})", r, self.positive_fields.len())
                    })?
                })
                .end
            }
            Side::Continue => line_len,
        };

        Ok(start..end)
    }
}

pub fn extract_fields_using_pos_indices(line: &[u8], plan: &mut FieldPlan) -> Result<()> {
    if line.is_empty() {
        return Ok(());
    }

    dbg!("extract_fields_using_pos_indices");

    dbg!(line, &plan.indices);
    let mut seen = 0;
    // Define an iterator over the delimiter positions, starting with a sentinel value.
    // This allows us to get the starting position (0) of the first field, which is gotten by
    // adding the delimiter length to the first found position.
    // During the first iteration we will get 0 because we will overflow the usize.
    let mut delim_iterator = std::iter::once(std::ops::Range { start: 0, end: 0 })
        .chain(plan.finder.find_ranges(line))
        .peekable();

    let line_len = line.len();
    let eol_range = std::ops::Range {
        start: line_len,
        end: line_len,
    };

    //for &desired_field in plan.indices() {

    for i in 0..plan.positive_indices.len() {
        let desired_field = plan.positive_indices[i];
        dbg!(desired_field, seen);
        let f_start = delim_iterator
            .nth(desired_field - seen)
            .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", desired_field + 1))?
            .end;

        dbg!(f_start);
        dbg!(delim_iterator.peek());
        let f_end = delim_iterator.peek().unwrap_or(&eol_range).start;
        dbg!(f_end);

        plan.positive_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(())
}

pub fn extract_fields_using_negative_indices(line: &[u8], plan: &mut FieldPlan) -> Result<()> {
    if line.is_empty() {
        return Ok(());
    }

    dbg!("extract_fields_using_negative_indices");
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
        dbg!(desired_field, seen);

        let f_end = delim_iterator
            .nth(desired_field - seen) // unwrap or bail with bail! macro if out of bounds
            .ok_or_else(|| anyhow::anyhow!("Out of bounds: {}", desired_field - 1))?
            .start;

        let f_start = delim_iterator.peek().unwrap_or(&start_range).end;

        dbg!(f_start);
        dbg!(f_end);

        plan.negative_fields[desired_field] = Range {
            start: f_start,
            end: f_end,
        };

        seen = desired_field + 1;
    }

    Ok(())
}

// Simple enum for delimiter strategy: Regex or Finder (owned)
enum DelimiterStrategy<'a> {
    #[cfg(feature = "regex")]
    Regex(&'a regex::bytes::Regex),
    // RegexRev(&'a regex::bytes::Regex),
    Memmem(memchr::memmem::Finder<'a>, usize),
    MemmemRev(memchr::memmem::FinderRev<'a>, usize),
}

impl<'a> DelimiterStrategy<'a> {
    fn find_ranges(&'a self, line: &'a [u8]) -> DelimiterFindIter<'a> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterStrategy::Regex(re) => DelimiterFindIter::Regex(re.find_iter(line)),
            DelimiterStrategy::Memmem(finder, len) => {
                DelimiterFindIter::Memmem(finder.find_iter(line), *len)
            }
            DelimiterStrategy::MemmemRev(finder_rev, len) => {
                DelimiterFindIter::MemmemRev(finder_rev.rfind_iter(line), *len)
            } // DelimiterStrategy::RegexRev(re) => {
              //     let m = re.find_iter(line).rev();
              //     DelimiterFindIter::RegexRev(re.find_iter(line).rev())
              // }
        }
    }
}

enum DelimiterFindIter<'a> {
    #[cfg(feature = "regex")]
    Regex(regex::bytes::Matches<'a, 'a>),
    // RegexRev(std::iter::Rev<regex::bytes::Matches<'a, 'a>>),
    Memmem(memchr::memmem::FindIter<'a, 'a>, usize),
    MemmemRev(memchr::memmem::FindRevIter<'a, 'a>, usize),
}

impl<'a> Iterator for DelimiterFindIter<'a> {
    type Item = Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterFindIter::Regex(iter) => iter.next().map(|m| m.start()..m.end()),
            DelimiterFindIter::Memmem(iter, len) => iter.next().map(|idx| idx..idx + *len),
            DelimiterFindIter::MemmemRev(iter, len) => iter.next().map(|idx| idx..idx + *len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::UserBounds;
    use std::str::FromStr;

    #[test]
    fn extract_fields_basic() {
        let line = b"a--b--c";
        let delimiter = b"--";
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("1").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("2").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("3").unwrap()),
        ];
        let mut plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields, vec![0..1, 3..4, 6..7]);
    }

    #[test]
    fn extract_fields_out_of_order() {
        let line = b"foo--bar--baz";
        let delimiter = b"--";
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("3").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("1").unwrap()),
        ];
        let mut plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert!(extract_fields_using_pos_indices(line, &mut plan).is_ok());
        assert_eq!(plan.positive_fields[2], 10..13);
        assert_eq!(plan.positive_fields[0], 0..3);
    }

    #[test]
    fn extract_fields_multibyte_delim_and_missing_field() {
        let line = b"x==y==z";
        let delimiter = b"==";
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("1").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("4").unwrap()), // out of bounds
        ];
        let mut plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        let result = extract_fields_using_pos_indices(line, &mut plan);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Out of bounds: 4");
    }

    #[test]
    fn test_field_plan_from_bounds_single_and_range() {
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("1").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("2:4").unwrap()),
        ];
        let delimiter = b"-";
        let plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3]);
    }

    #[test]
    fn test_field_plan_from_bounds_range_and_single_out_of_order() {
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("2:3").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("1").unwrap()),
        ];
        let delimiter = b"-";
        let plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_multiple_ranges_and_order() {
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("4:5").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str(":2").unwrap()),
        ];
        let delimiter = b"-";
        let plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert_eq!(plan.positive_indices, vec![0, 1, 3, 4]);
    }

    #[test]
    fn test_field_plan_from_bounds_duplicate_fields() {
        let bounds = vec![
            BoundOrFiller::Bound(UserBounds::from_str("1:2").unwrap()),
            BoundOrFiller::Bound(UserBounds::from_str("2:3").unwrap()),
        ];
        let delimiter = b"-";
        let plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        // 1:2 gives 0,1; 2:3 gives 1,2; deduped order: 0,1,2
        assert_eq!(plan.positive_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_field_plan_from_bounds_full_range() {
        // Use "1:-1" to mean all fields (from 1 to last)
        let bounds = vec![BoundOrFiller::Bound(UserBounds::from_str("1:-1").unwrap())];
        let delimiter = b"-";
        let plan = FieldPlan::from_bounds(&bounds, delimiter, None).unwrap();
        assert_eq!(plan.indices, vec![-1, 1]);
        assert_eq!(plan.positive_indices, vec![0]);
        assert_eq!(plan.negative_indices, vec![0]);
    }
}
