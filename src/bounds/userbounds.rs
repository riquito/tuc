use crate::bounds::side::Side;
use anyhow::{Result, bail};
use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;
use std::str::FromStr;

#[derive(Debug, Eq, Clone)]
pub struct UserBounds {
    l: Side,
    r: Side,
    is_last: bool,
    fallback_oob: Option<Vec<u8>>,
}

impl UserBounds {
    pub fn new(l: Side, r: Side) -> Self {
        Self {
            l,
            r,
            is_last: false,
            fallback_oob: None,
        }
    }

    #[inline(always)]
    pub fn l(&self) -> &Side {
        &self.l
    }

    #[inline(always)]
    pub fn r(&self) -> &Side {
        &self.r
    }

    #[inline(always)]
    pub fn is_last(&self) -> bool {
        self.is_last
    }

    #[inline(always)]
    pub fn set_is_last(&mut self, is_last: bool) {
        self.is_last = is_last;
    }

    #[inline(always)]
    pub fn fallback_oob(&self) -> &Option<Vec<u8>> {
        &self.fallback_oob
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundOrFiller {
    Bound(UserBounds),
    Filler(Vec<u8>),
}

impl fmt::Display for UserBounds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (l, r) if l == r => write!(f, "{}", l),
            (l, r) if r.abs_value() == Side::max_right() => write!(f, "{}:-1", l),
            (l, r) => write!(f, "{}:{}", l, r),
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
                let side = Side::from_str_left_bound(s)?;
                (side, side)
            }
            Some(idx_colon) if idx_colon == 0 => (
                Side::new_inf_left(),
                Side::from_str_right_bound(&s[idx_colon + 1..])?,
            ),
            Some(idx_colon) if idx_colon == s.len() - 1 => (
                Side::from_str_left_bound(&s[..idx_colon])?,
                Side::new_inf_right(),
            ),
            Some(idx_colon) => (
                Side::from_str_left_bound(&s[..idx_colon])?,
                Side::from_str_right_bound(&s[idx_colon + 1..])?,
            ),
        };

        if l != r {
            if !l.is_negative() && !r.is_negative() && r.abs_value() < l.abs_value() {
                // both positive
                bail!("Field left value cannot be greater than right value");
            } else if l.is_negative() && r.is_negative() && l.abs_value() < r.abs_value() {
                // both negative. Because we use absolute numbers we inverted the check
                bail!("Field left value cannot be greater than right value")
            }
        }

        let mut b = UserBounds::new(l, r);
        b.fallback_oob = fallback_oob;
        Ok(b)
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
        UserBounds::new(Side::new_inf_left(), Side::new_inf_right())
    }
}

pub trait UserBoundsTrait<T> {
    fn new(l: Side, r: Side) -> Self;
    fn with_fallback(l: Side, r: Side, fallback_oob: Option<Vec<u8>>) -> Self;
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>>;
    fn matches(&self, idx: usize) -> Result<bool>;
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
     */
    #[inline(always)]
    fn matches(&self, idx: usize) -> Result<bool> {
        self.l.between(&self.r, idx)
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
        let r_value = std::cmp::min(self.r.abs_value(), parts_length - 1);

        if self.l.abs_value() >= parts_length {
            bail!("Out of bounds: {}", self.l);
        } else if r_value >= parts_length {
            bail!("Out of bounds: {}", self.r);
        };

        let start = if self.l.is_negative() {
            parts_length - self.l.abs_value() - 1
        } else {
            self.l.abs_value()
        };

        let end = if self.r.is_negative() {
            parts_length - r_value
        } else {
            r_value + 1
        };

        if end <= start {
            // `end` must always be 1 or more greater than start
            bail!("Field left value cannot be greater than right value");
        }

        Ok(Range { start, end })
    }

    /// Transform a ranged bound into a list of one or more
    /// slot bound
    fn unpack(&self, num_fields: usize) -> Vec<UserBounds> {
        let mut bounds = Vec::new();

        const RIGHT_MAX: usize = Side::max_right();

        let (start, end): (usize, usize) = match (self.l.value(), self.r.value()) {
            ((l_is_negative, l_value), (_, RIGHT_MAX)) => (
                if l_is_negative {
                    num_fields - l_value - 1
                } else {
                    l_value
                },
                num_fields - 1,
            ),
            ((l_is_negative, l_value), (r_is_negative, r_value)) => (
                if l_is_negative {
                    num_fields - l_value - 1
                } else {
                    l_value
                },
                if r_is_negative {
                    num_fields - r_value - 1
                } else {
                    r_value
                },
            ),
        };

        for i in start..=end {
            bounds.push(UserBounds::new(
                Side::with_pos_value(i),
                Side::with_pos_value(i),
            ))
        }

        bounds
    }

    /// Transform a bound in its complement (invert the bound).
    fn complement(&self, num_fields: usize) -> Result<Vec<UserBounds>> {
        let r = self.try_into_range(num_fields)?;
        let r_complement = complement_std_range(num_fields, &r);
        Ok(r_complement
            .into_iter()
            .map(|x| {
                UserBounds::new(
                    Side::with_pos_value(x.start),
                    // SAFETY
                    // complement_std_range won't use usize::MAX
                    Side::with_pos_value(x.end - 1),
                )
            })
            .collect())
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

    fn side_pos(l: usize) -> Side {
        Side::with_pos_value(l)
    }

    fn side_neg(l: usize) -> Side {
        Side::with_neg_value(l)
    }

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
        assert_eq!(UserBounds::from_str("1:").unwrap().to_string(), "1:-1");
        assert_eq!(UserBounds::from_str(":3").unwrap().to_string(), "1:3");
        assert_eq!(UserBounds::from_str("3:").unwrap().to_string(), "3:-1");
        assert_eq!(UserBounds::from_str("1:2").unwrap().to_string(), "1:2");
        assert_eq!(UserBounds::from_str("-2:-1").unwrap().to_string(), "-2:-1");
    }

    #[test]
    fn test_user_bounds_from_str() {
        assert_eq!(
            UserBounds::from_str("1").ok(),
            Some(UserBounds::new(side_pos(0), side_pos(0)))
        );
        assert_eq!(
            UserBounds::from_str("-1").ok(),
            Some(UserBounds::new(side_neg(0), side_neg(0)))
        );
        assert_eq!(
            UserBounds::from_str("1:2").ok(),
            Some(UserBounds::new(side_pos(0), side_pos(1)))
        );
        assert_eq!(
            UserBounds::from_str("-2:-1").ok(),
            Some(UserBounds::new(side_neg(1), side_neg(0)))
        );
        assert_eq!(
            UserBounds::from_str("1:").ok(),
            Some(UserBounds::new(side_pos(0), Side::new_inf_right())),
        );
        assert_eq!(
            UserBounds::from_str("-1:").ok(),
            Some(UserBounds::new(side_neg(0), Side::new_inf_right())),
        );
        assert_eq!(
            UserBounds::from_str(":1").ok(),
            Some(UserBounds::new(Side::new_inf_left(), side_pos(0))),
        );
        assert_eq!(
            UserBounds::from_str(":-1").ok(),
            Some(UserBounds::new(Side::new_inf_left(), side_neg(0))),
        );

        assert_eq!(
            UserBounds::from_str("1").ok(),
            Some(UserBounds::with_fallback(side_pos(0), side_pos(0), None)),
        );

        assert_eq!(
            UserBounds::from_str("1=foo").ok(),
            Some(UserBounds::with_fallback(
                side_pos(0),
                side_pos(0),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1:2=foo").ok(),
            Some(UserBounds::with_fallback(
                side_pos(0),
                side_pos(1),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("-1=foo").ok(),
            Some(UserBounds::with_fallback(
                side_neg(0),
                side_neg(0),
                Some("foo".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1=allow:colon:in:fallback").ok(),
            Some(UserBounds::with_fallback(
                side_pos(0),
                side_pos(0),
                Some("allow:colon:in:fallback".as_bytes().to_owned())
            )),
        );

        assert_eq!(
            UserBounds::from_str("1:2=allow:colon:in:fallback").ok(),
            Some(UserBounds::with_fallback(
                side_pos(0),
                side_pos(1),
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
            UserBounds::from_str("1").unwrap().unpack(2),
            vec![UserBounds::from_str("1").unwrap()],
        );

        assert_eq!(
            UserBounds::from_str("1:").unwrap().unpack(2),
            vec![
                UserBounds::from_str("1").unwrap(),
                UserBounds::from_str("2").unwrap()
            ],
        );

        assert_eq!(
            UserBounds::from_str(":2").unwrap().unpack(2),
            vec![
                UserBounds::from_str("1").unwrap(),
                UserBounds::from_str("2").unwrap()
            ],
        );

        assert_eq!(
            UserBounds::from_str("1:-1").unwrap().unpack(2),
            vec![
                UserBounds::from_str("1").unwrap(),
                UserBounds::from_str("2").unwrap()
            ],
        );

        assert_eq!(
            UserBounds::from_str("-1:").unwrap().unpack(2),
            vec![UserBounds::from_str("2").unwrap()],
        );

        assert_eq!(
            UserBounds::from_str(":-1").unwrap().unpack(2),
            vec![
                UserBounds::from_str("1").unwrap(),
                UserBounds::from_str("2").unwrap()
            ],
        );

        assert_eq!(
            UserBounds::from_str("-2:-1").unwrap().unpack(2),
            vec![
                UserBounds::from_str("1").unwrap(),
                UserBounds::from_str("2").unwrap()
            ],
        );
    }

    #[test]
    fn test_complement_bound() {
        assert_eq!(
            UserBounds::from_str("1:1").unwrap().complement(2).unwrap(),
            vec![UserBounds::from_str("2:2").unwrap()],
        );

        assert_eq!(
            UserBounds::from_str("1:").unwrap().complement(2).unwrap(),
            Vec::new(),
        );

        assert_eq!(
            UserBounds::from_str("-3:3").unwrap().complement(4).unwrap(),
            vec![
                UserBounds::from_str("1:1").unwrap(),
                UserBounds::from_str("4:4").unwrap(),
            ],
        );
    }
}
