use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::convert::TryInto;
use std::fmt;
use std::ops::Range;
use std::str::FromStr;

use crate::bounds::Side;

#[derive(Debug, Eq, Clone)]
pub struct UserBounds {
    pub l: Side,
    pub r: Side,
    pub is_last: bool,
    pub fallback_oob: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundOrFiller {
    Bound(UserBounds),
    Filler(Vec<u8>),
}

impl fmt::Display for UserBounds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (Side::Continue, Side::Continue) => write!(f, "1:-1"),
            (l, r) if l == r => write!(f, "{l}"),
            (l, r) => write!(f, "{l}:{r}"),
        }
    }
}

impl FromStr for UserBounds {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            bail!("Field format error: empty field");
        } else if s == ":" {
            bail!("Field format error, no numbers next to `:`");
        }

        let mut fallback_oob: Option<Vec<u8>> = None;
        let mut s = s;
        if let Some((range_part, fallback)) = s.split_once('=') {
            fallback_oob = Some(fallback.into());
            s = range_part;
        }

        let (l, r) = match s.find(':') {
            None => {
                let side = Side::from_str(s)?;
                (side, side)
            }
            Some(idx_colon) if idx_colon == 0 => {
                (Side::Continue, Side::from_str(&s[idx_colon + 1..])?)
            }
            Some(idx_colon) if idx_colon == s.len() - 1 => {
                (Side::from_str(&s[..idx_colon])?, Side::Continue)
            }
            Some(idx_colon) => (
                Side::from_str(&s[..idx_colon])?,
                Side::from_str(&s[idx_colon + 1..])?,
            ),
        };

        match (l, r) {
            (Side::Some(0), _) => {
                bail!("Field value 0 is not allowed (fields are 1-indexed)");
            }
            (_, Side::Some(0)) => {
                bail!("Field value 0 is not allowed (fields are 1-indexed)");
            }
            (Side::Some(left), Side::Some(right))
                if right < left && (right * left).is_positive() =>
            {
                bail!("Field left value cannot be greater than right value");
            }
            _ => (),
        }

        let mut b = UserBounds::new(l, r);
        b.fallback_oob = fallback_oob;
        Ok(b)
    }
}

impl From<Range<usize>> for UserBounds {
    fn from(value: Range<usize>) -> Self {
        let start: i32 = value
            .start
            .try_into()
            .expect("range was bigger than expected");

        let end: i32 = value
            .end
            .try_into()
            .expect("range was bigger than expected");

        UserBounds::new(Side::Some(start + 1), Side::Some(end))
    }
}

impl PartialOrd for UserBounds {
    /// Compare UserBounds. Note that you cannot reliably compare
    /// bounds with a mix of positive/negative indices (you cannot
    /// compare `-1` with `3` without kwowing how many parts are there).
    /// Check with UserBounds.is_sortable before comparing.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.r.partial_cmp(&other.l)
    }
}

impl PartialEq for UserBounds {
    fn eq(&self, other: &Self) -> bool {
        (self.l, self.r) == (other.l, other.r)
    }
}

impl Default for UserBounds {
    fn default() -> Self {
        UserBounds::new(Side::Some(1), Side::Continue)
    }
}

pub trait UserBoundsTrait<T> {
    fn new(l: Side, r: Side) -> Self;
    fn with_fallback(l: Side, r: Side, fallback_oob: Option<Vec<u8>>) -> Self;
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>>;
    fn matches(&self, idx: T) -> Result<bool>;
    fn unpack(&self, num_fields: usize) -> Vec<UserBounds>;
    fn complement(&self, num_fields: usize) -> Result<Vec<UserBounds>>;
}

impl UserBoundsTrait<i32> for UserBounds {
    fn new(l: Side, r: Side) -> Self {
        UserBounds {
            l,
            r,
            is_last: false,
            fallback_oob: None,
        }
    }

    fn with_fallback(l: Side, r: Side, fallback_oob: Option<Vec<u8>>) -> Self {
        UserBounds {
            l,
            r,
            is_last: false,
            fallback_oob,
        }
    }

    /**
     * Check if a field is between the bounds.
     *
     * It errors out if the index has different sign than the bounds
     * (we can't verify if e.g. -1 idx is between 3:5 without knowing the number
     * of matching bounds).
     *
     * Fields are 1-indexed.
     */
    #[inline(always)]
    fn matches(&self, idx: i32) -> Result<bool> {
        match (self.l, self.r) {
            (Side::Some(left), _) if (left * idx).is_negative() => {
                bail!(
                    "sign mismatch. Can't verify if index {} is between bounds {}",
                    idx,
                    self
                )
            }
            (_, Side::Some(right)) if (right * idx).is_negative() => {
                bail!(
                    "sign mismatch. Can't verify if index {} is between bounds {}",
                    idx,
                    self
                )
            }
            (Side::Continue, Side::Continue) => Ok(true),
            (Side::Some(left), Side::Some(right)) if left <= idx && idx <= right => Ok(true),
            (Side::Continue, Side::Some(right)) if idx <= right => Ok(true),
            (Side::Some(left), Side::Continue) if left <= idx => Ok(true),
            _ => Ok(false),
        }
    }

    /// Transform UserBounds into std::opt::Range
    ///
    /// UserBounds is 1-indexed and inclusive on both sides, while
    /// the resulting range is 0-indexed and exclusive on the  right side.
    ///
    /// `parts_length` is necessary to calculate Side::Continue on
    /// the right side, or any negative indexes.
    ///
    /// e.g.
    ///
    /// ```rust
    /// # use tuc::bounds::{UserBounds, UserBoundsTrait};
    /// # use std::ops::Range;
    /// # use tuc::bounds::Side;
    /// # use std::str::FromStr;
    ///
    /// assert_eq!(
    ///   UserBounds::from_str("1:2").unwrap().try_into_range(5).unwrap(),
    ///   Range { start: 0, end: 2} // 2, not 1, because it's exclusive
    /// );
    ///
    /// assert_eq!(
    ///   UserBounds::from_str("1:").unwrap().try_into_range(5).unwrap(),
    ///   Range { start: 0, end: 5}
    /// );
    /// ```
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>> {
        let parts_length = parts_length as i32;

        let start: i32 = match self.l {
            Side::Continue => 0,
            Side::Some(v) => {
                if v > parts_length || v < -parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length + v
                } else {
                    v - 1
                }
            }
        };

        let end: i32 = match self.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v > parts_length || v < -parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length + v + 1
                } else {
                    v
                }
            }
        };

        if end <= start {
            // `end` must always be 1 or more greater than start
            bail!("Field left value cannot be greater than right value");
        }

        Ok(Range {
            start: start as usize,
            end: end as usize,
        })
    }

    /// Transform a ranged bound into a list of one or more
    /// slot bound
    fn unpack(&self, num_fields: usize) -> Vec<UserBounds> {
        let mut bounds = Vec::new();
        let n: i32 = num_fields
            .try_into()
            .expect("num_fields was bigger than expected");

        let (start, end): (i32, i32) = match (self.l, self.r) {
            (Side::Continue, Side::Continue) => (1, n),
            (Side::Continue, Side::Some(right)) => {
                (1, if right > 0 { right } else { n + 1 + right })
            }
            (Side::Some(left), Side::Some(right)) => (
                if left > 0 { left } else { n + 1 + left },
                if right > 0 { right } else { n + 1 + right },
            ),
            (Side::Some(left), Side::Continue) => (if left > 0 { left } else { n + 1 + left }, n),
        };

        for i in start..=end {
            bounds.push(UserBounds::new(Side::Some(i), Side::Some(i)))
        }

        bounds
    }

    /// Transform a bound in its complement (invert the bound).
    fn complement(&self, num_fields: usize) -> Result<Vec<UserBounds>> {
        let r = self.try_into_range(num_fields)?;
        let r_complement = complement_std_range(num_fields, &r);
        Ok(r_complement.into_iter().map(|x| x.into()).collect())
    }
}

fn complement_std_range(parts_length: usize, r: &Range<usize>) -> Vec<Range<usize>> {
    match (r.start, r.end) {
        // full match => no match
        (0, end) if end == parts_length => Vec::new(),
        // match left side => match right side
        #[allow(clippy::single_range_in_vec_init)]
        (0, right) => vec![right..parts_length],
        // match right side => match left side
        #[allow(clippy::single_range_in_vec_init)]
        (left, end) if end == parts_length => vec![0..left],
        // match middle of string => match before and after
        (left, right) => vec![0..left, right..parts_length],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complement_std_range() {
        // remember, it assumes that ranges are "legit" (not out of bounds)

        let empty_vec: Vec<Range<usize>> = vec![];

        // test 1-long string
        assert_eq!(complement_std_range(1, &(0..1)), empty_vec);

        // test ranges that reach left or right bounds
        assert_eq!(complement_std_range(5, &(0..5)), empty_vec);
        assert_eq!(complement_std_range(5, &(0..3)), vec![3..5]);
        assert_eq!(complement_std_range(5, &(3..5)), vec![0..3]);

        // test internal range
        assert_eq!(complement_std_range(5, &(1..3)), vec![0..1, 3..5]);

        // test 2-long string
        assert_eq!(complement_std_range(2, &(0..2)), empty_vec);
        assert_eq!(complement_std_range(2, &(0..1)), vec![1..2]);
        assert_eq!(complement_std_range(2, &(1..2)), vec![0..1]);
    }

    #[test]
    fn test_user_bounds_formatting() {
        assert_eq!(
            UserBounds::new(Side::Continue, Side::Continue).to_string(),
            "1:-1"
        );
        assert_eq!(
            UserBounds::new(Side::Continue, Side::Some(3)).to_string(),
            ":3"
        );
        assert_eq!(
            UserBounds::new(Side::Some(3), Side::Continue).to_string(),
            "3:"
        );
        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Some(2)).to_string(),
            "1:2"
        );
        assert_eq!(
            UserBounds::new(Side::Some(-1), Side::Some(-2)).to_string(),
            "-1:-2"
        );
    }

    #[test]
    fn test_user_bounds_from_str() {
        assert_eq!(
            UserBounds::from_str("1").ok(),
            Some(UserBounds::new(Side::Some(1), Side::Some(1))),
        );
        assert_eq!(
            UserBounds::from_str("-1").ok(),
            Some(UserBounds::new(Side::Some(-1), Side::Some(-1))),
        );
        assert_eq!(
            UserBounds::from_str("1:2").ok(),
            Some(UserBounds::new(Side::Some(1), Side::Some(2))),
        );
        assert_eq!(
            UserBounds::from_str("-2:-1").ok(),
            Some(UserBounds::new(Side::Some(-2), Side::Some(-1))),
        );
        assert_eq!(
            UserBounds::from_str("1:").ok(),
            Some(UserBounds::new(Side::Some(1), Side::Continue)),
        );
        assert_eq!(
            UserBounds::from_str("-1:").ok(),
            Some(UserBounds::new(Side::Some(-1), Side::Continue)),
        );
        assert_eq!(
            UserBounds::from_str(":1").ok(),
            Some(UserBounds::new(Side::Continue, Side::Some(1))),
        );
        assert_eq!(
            UserBounds::from_str(":-1").ok(),
            Some(UserBounds::new(Side::Continue, Side::Some(-1))),
        );

        assert_eq!(
            UserBounds::from_str("1").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(1),
                Side::Some(1),
                None
            )),
        );

        assert_eq!(
            UserBounds::from_str("1=foo").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(1),
                Side::Some(1),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1:2=foo").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(1),
                Side::Some(2),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("-1=foo").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(-1),
                Side::Some(-1),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1=allow:colon:in:fallback").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(1),
                Side::Some(1),
                Some("allow:colon:in:fallback".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1:2=allow:colon:in:fallback").ok(),
            Some(UserBounds::with_fallback(
                Side::Some(1),
                Side::Some(2),
                Some("allow:colon:in:fallback".as_bytes().to_owned())
            )),
        );

        {
            #![allow(clippy::bind_instead_of_map)]
            assert_eq!(
                UserBounds::from_str("2:1")
                    .err()
                    .and_then(|x| Some(x.to_string())),
                Some(String::from(
                    "Field left value cannot be greater than right value"
                ))
            );
            assert_eq!(
                UserBounds::from_str("-1:-2")
                    .err()
                    .and_then(|x| Some(x.to_string())),
                Some(String::from(
                    "Field left value cannot be greater than right value"
                ))
            );
        }
    }

    #[test]
    fn test_unpack_bound() {
        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Some(1)).unpack(2),
            vec![UserBounds::new(Side::Some(1), Side::Some(1))],
        );

        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Continue).unpack(2),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(2), Side::Some(2))
            ],
        );

        assert_eq!(
            UserBounds::new(Side::Continue, Side::Some(2)).unpack(2),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(2), Side::Some(2))
            ],
        );

        assert_eq!(
            UserBounds::new(Side::Continue, Side::Continue).unpack(2),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(2), Side::Some(2))
            ],
        );

        assert_eq!(
            UserBounds::new(Side::Some(-1), Side::Continue).unpack(2),
            vec![UserBounds::new(Side::Some(2), Side::Some(2)),],
        );

        assert_eq!(
            UserBounds::new(Side::Continue, Side::Some(-1)).unpack(2),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(2), Side::Some(2))
            ],
        );

        assert_eq!(
            UserBounds::new(Side::Some(-2), Side::Some(-1)).unpack(2),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(2), Side::Some(2))
            ],
        );
    }

    #[test]
    fn test_complement_bound() {
        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Some(1))
                .complement(2)
                .unwrap(),
            vec![UserBounds::new(Side::Some(2), Side::Some(2))],
        );

        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Continue)
                .complement(2)
                .unwrap(),
            Vec::new(),
        );

        assert_eq!(
            UserBounds::new(Side::Some(-3), Side::Some(3))
                .complement(4)
                .unwrap(),
            vec![
                UserBounds::new(Side::Some(1), Side::Some(1)),
                UserBounds::new(Side::Some(4), Side::Some(4)),
            ],
        );
    }
}
