use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::convert::TryInto;
use std::fmt;
use std::ops::{Deref, Range};
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum BoundsType {
    Bytes,
    Characters,
    Fields,
    Lines,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundOrFiller {
    Bound(UserBounds),
    Filler(String),
}

/**
 * Parse bound string. It can contain formatting elements or not.
 *
 * Valid bounds formats are e.g. 1 / -1 / 1:3 / :3 / 1: / 1,4
 * If '{' is present, the string is considered to be a format string:
 * in that case everything inside {} is considered a bound, and the rest
 * just some text to display when the bounds are found.
 * e.g. "Hello {1}, found {1:3} and {2,4}"
 */
pub fn parse_bounds_list(s: &str) -> Result<Vec<BoundOrFiller>> {
    if s.is_empty() {
        return Ok(Vec::new());
    }

    if s.contains(['{', '}']) {
        let mut bof: Vec<BoundOrFiller> = Vec::new();
        let mut inside_bound = false;
        let mut part_start = 0;

        let mut iter = s.char_indices().peekable();
        while let Some((idx, w0)) = iter.next() {
            let w1 = iter.peek().unwrap_or(&(0, 'x')).1;

            if w0 == w1 && (w0 == '{' || w0 == '}') {
                // escaped bracket, ignore it, we will replace it later
                iter.next();
            } else if w0 == '}' && !inside_bound {
                bail!("Field format error: missing opening parenthesis",);
            } else if w0 == '{' {
                // starting a new bound
                inside_bound = true;

                if idx - part_start > 0 {
                    bof.push(BoundOrFiller::Filler(
                        s[part_start..idx]
                            .replace("{{", "{")
                            .replace("}}", "}")
                            .replace("\\n", "\n")
                            .replace("\\t", "\t"),
                    ));
                }

                part_start = idx + 1;
            } else if w0 == '}' {
                // ending a bound
                inside_bound = false;

                // consider also comma separated bounds
                for maybe_bounds in s[part_start..idx].split(',') {
                    bof.push(BoundOrFiller::Bound(UserBounds::from_str(maybe_bounds)?));
                }

                part_start = idx + 1;
            }
        }

        if inside_bound {
            bail!("Field format error: missing closing parenthesis");
        } else if s.len() - part_start > 0 {
            bof.push(BoundOrFiller::Filler(
                s[part_start..]
                    .replace("{{", "{")
                    .replace("}}", "}")
                    .replace("\\n", "\n")
                    .replace("\\t", "\t"),
            ));
        }

        Ok(bof)
    } else {
        let k: Result<Vec<BoundOrFiller>, _> = s
            .split(',')
            .map(|x| UserBounds::from_str(x).map(BoundOrFiller::Bound))
            .collect();
        Ok(k?)
    }
}

#[derive(Debug, Clone)]
pub struct UserBoundsList {
    pub list: Vec<BoundOrFiller>,
    /// Optimization that we can use to stop searching for fields.
    /// It's available only when every bound uses positive indexes.
    /// When conditions do not apply, its value is `Side::Continue`.
    pub last_interesting_field: Side,
}

impl Deref for UserBoundsList {
    type Target = Vec<BoundOrFiller>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl From<Vec<BoundOrFiller>> for UserBoundsList {
    fn from(list: Vec<BoundOrFiller>) -> Self {
        let mut ubl = UserBoundsList {
            list,
            last_interesting_field: Side::Continue,
        };

        let mut rightmost_bound: Option<Side> = None;
        let mut last_bound: Option<&mut UserBounds> = None;

        let is_sortable = ubl.is_sortable();

        ubl.list.iter_mut().for_each(|bof| {
            if let BoundOrFiller::Bound(b) = bof {
                if rightmost_bound.is_none() || b.r > rightmost_bound.unwrap() {
                    rightmost_bound = Some(b.r);
                }

                last_bound = Some(b);
            }
        });

        if !is_sortable {
            rightmost_bound = None;
        }

        last_bound
            .expect("UserBoundsList must contain at least one UserBounds")
            .is_last = true;

        ubl.last_interesting_field = rightmost_bound.unwrap_or(Side::Continue);
        ubl
    }
}

impl FromStr for UserBoundsList {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            bail!("UserBoundsList must contain at least one UserBounds");
        }
        Ok(parse_bounds_list(s)?.into())
    }
}

impl UserBoundsList {
    /// Detect whether the list can be sorted.
    /// It can be sorted only if every bound
    /// has the same sign (all positive or all negative).
    pub fn is_sortable(&self) -> bool {
        let mut has_positive_idx = false;
        let mut has_negative_idx = false;
        self.get_userbounds_only().for_each(|b| {
            if let Side::Some(left) = b.l {
                if left.is_positive() {
                    has_positive_idx = true;
                } else {
                    has_negative_idx = true;
                }
            }

            if let Side::Some(right) = b.r {
                if right.is_positive() {
                    has_positive_idx = true;
                } else {
                    has_negative_idx = true;
                }
            }
        });

        !(has_negative_idx && has_positive_idx)
    }

    fn get_userbounds_only(&self) -> impl Iterator<Item = &UserBounds> + '_ {
        self.list.iter().flat_map(|b| match b {
            BoundOrFiller::Bound(x) => Some(x),
            _ => None,
        })
    }

    fn is_sorted(&self) -> bool {
        let mut prev_b: Option<&UserBounds> = None;
        for b in self.get_userbounds_only() {
            if prev_b.is_none() || prev_b <= Some(b) {
                prev_b = Some(b);
            } else {
                return false;
            }
        }

        true
    }

    fn has_negative_indices(&self) -> bool {
        self.get_userbounds_only().any(|b| {
            if let Side::Some(left) = b.l {
                if left.is_negative() {
                    return true;
                }
            }

            if let Side::Some(right) = b.r {
                if right.is_negative() {
                    return true;
                }
            }

            false
        })
    }

    /// Check if the bounds in the list match the following conditions:
    /// - they are in ascending order
    /// - they use solely positive indices
    /// - they don't overlap (but they can be adjacent, e.g. 1:2,2,3)
    pub fn is_forward_only(&self) -> bool {
        self.is_sortable() && self.is_sorted() && !self.has_negative_indices()
    }

    /// Create a new UserBoundsList with every ranged bound converted
    /// into single-field bounds.
    ///
    /// ```rust
    /// # use tuc::bounds::{UserBoundsList, UserBoundsTrait};
    /// # use std::ops::Range;
    /// # use tuc::bounds::Side;
    /// # use std::str::FromStr;
    ///
    /// assert_eq!(
    ///   UserBoundsList::from_str("1:3,4,-2:").unwrap().unpack(6).list,
    ///   UserBoundsList::from_str("1,2,3,4,5,6").unwrap().list,
    /// );
    /// ```
    pub fn unpack(&self, num_fields: usize) -> UserBoundsList {
        let list: Vec<BoundOrFiller> = self
            .list
            .iter()
            .flat_map(|bof| match bof {
                // XXX how to do it using only iterators, no collect?
                BoundOrFiller::Bound(b) => b
                    .unpack(num_fields)
                    .into_iter()
                    .map(BoundOrFiller::Bound)
                    .collect(),
                BoundOrFiller::Filler(f) => vec![BoundOrFiller::Filler(f.clone())],
            })
            .collect();

        list.into()
    }

    /// Create a new UserBoundsList with every range complemented (inverted).
    pub fn complement(&self, num_fields: usize) -> Result<UserBoundsList> {
        let list: Vec<BoundOrFiller> = self
            .list
            .iter()
            .flat_map(|bof| match bof {
                // XXX how to do it using only iterators, no collect?
                BoundOrFiller::Bound(b) => anyhow::Ok(
                    b.complement(num_fields)?
                        .into_iter()
                        .map(BoundOrFiller::Bound)
                        .collect(),
                ),
                BoundOrFiller::Filler(f) => Ok(vec![BoundOrFiller::Filler(f.clone())]),
            })
            .flatten()
            .collect();

        if list.is_empty() {
            bail!("the complement is empty");
        }

        Ok(list.into())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Some(i32),
    Continue,
}

impl FromStr for Side {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => Side::Continue,
            _ => Side::Some(
                s.parse::<i32>()
                    .or_else(|_| bail!("Not a number `{}`", s))?,
            ),
        })
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Side::Some(v) => write!(f, "{v}"),
            Side::Continue => write!(f, ""),
        }
    }
}

impl PartialOrd for Side {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Side::Some(s), Side::Some(o)) => {
                if !(s * o).is_positive() {
                    // We can't compare two sides with different sign
                    return None;
                }
                Some(s.cmp(o))
            }
            (Side::Continue, Side::Some(_)) => Some(Ordering::Greater),
            (Side::Some(_), Side::Continue) => Some(Ordering::Less),
            (Side::Continue, Side::Continue) => Some(Ordering::Equal),
        }
    }
}

#[derive(Debug, Eq, Clone)]
pub struct UserBounds {
    pub l: Side,
    pub r: Side,
    pub is_last: bool,
    pub fallback_oob: Option<Vec<u8>>,
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
        let start: usize = match self.l {
            Side::Continue => 0,
            Side::Some(v) => {
                if v.unsigned_abs() as usize > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length - v.unsigned_abs() as usize
                } else {
                    v as usize - 1
                }
            }
        };

        let end: usize = match self.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v.unsigned_abs() as usize > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length - v.unsigned_abs() as usize + 1
                } else {
                    v as usize
                }
            }
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
    fn test_parse_bounds_list() {
        // do not replicate tests from test_user_bounds_from_str, focus on
        // multiple bounds, bounds with format, and special cases (empty/one)

        assert_eq!(parse_bounds_list("").unwrap(), Vec::new());

        assert_eq!(
            &parse_bounds_list(",").unwrap_err().to_string(),
            "Field format error: empty field"
        );

        assert_eq!(
            &parse_bounds_list("{").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("}").unwrap_err().to_string(),
            "Field format error: missing opening parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{1}{").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{1}}").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{{1}").unwrap_err().to_string(),
            "Field format error: missing opening parenthesis"
        );

        assert_eq!(
            parse_bounds_list("1").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(1)
            ))],
        );

        assert_eq!(
            parse_bounds_list("1:").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::default())],
        );

        assert_eq!(
            parse_bounds_list("1,2").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
            ],
        );

        assert_eq!(
            parse_bounds_list("-1,1").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(-1), Side::Some(-1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
            ],
        );

        assert_eq!(
            parse_bounds_list("{1}").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(1)
            ))],
        );

        assert_eq!(
            parse_bounds_list("{1:2}").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(2)
            ))],
        );

        assert_eq!(
            parse_bounds_list("{1,2}").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2)))
            ],
        );

        assert_eq!(
            parse_bounds_list("hello {1,2} {{world}}").unwrap(),
            vec![
                BoundOrFiller::Filler(String::from("hello ")),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler(String::from(" {world}")),
            ],
        );

        assert_eq!(
            parse_bounds_list("{1}😎{2}").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Filler(String::from("😎")),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2)))
            ],
        );

        assert_eq!(
            parse_bounds_list("\\n\\t{{}}{1,2}\\n\\t{{}}").unwrap(),
            vec![
                BoundOrFiller::Filler(String::from("\n\t{}")),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler(String::from("\n\t{}")),
            ],
        );
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

    #[test]
    fn test_user_bounds_cannot_be_empty() {
        assert!(UserBoundsList::from_str("").is_err());
    }

    #[test]
    fn test_user_bounds_is_sortable() {
        assert!(UserBoundsList::from_str("1").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("1,2").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("3,2").unwrap().is_sortable());

        assert!(!UserBoundsList::from_str("-1,1").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("-1,-2").unwrap().is_sortable());

        assert!(!UserBoundsList::from_str("-1:,:1").unwrap().is_sortable());
    }

    #[test]
    fn test_vec_of_bounds_is_sorted() {
        assert!(UserBoundsList::from_str("1").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,2").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("-2,-1").unwrap().is_sorted());

        assert!(UserBoundsList::from_str(":1,2:4,5:").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,1:2").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,1,2").unwrap().is_sorted());

        assert!(!UserBoundsList::from_str("1,2,1").unwrap().is_sorted());
    }

    #[test]
    fn test_vec_of_bounds_is_forward_only() {
        assert!(UserBoundsList::from_str("{1}foo{2}")
            .unwrap()
            .is_forward_only());

        assert!(!UserBoundsList::from_str("{2}foo{1}")
            .unwrap()
            .is_forward_only());
    }

    #[test]
    fn test_vec_of_bounds_can_unpack() {
        assert_eq!(
            UserBoundsList::from_str("1,:1,2:3,4:")
                .unwrap()
                .unpack(4)
                .list,
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(3), Side::Some(3))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(4), Side::Some(4))),
            ]
        );

        assert_eq!(
            UserBoundsList::from_str("a{1:2}b").unwrap().unpack(4).list,
            vec![
                BoundOrFiller::Filler(String::from("a")),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler(String::from("b")),
            ]
        );
    }

    #[test]
    fn test_vec_of_bounds_can_complement() {
        assert_eq!(
            UserBoundsList::from_str("1:2,2:3,5,-2")
                .unwrap()
                .complement(6)
                .unwrap()
                .list,
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(3), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(4), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(4))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(6), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(4))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(6), Side::Some(6))),
            ]
        );

        assert_eq!(
            UserBoundsList::from_str("1:")
                .unwrap()
                .complement(6)
                .err()
                .map(|x| x.to_string()),
            Some("the complement is empty".to_owned())
        );
    }
}
