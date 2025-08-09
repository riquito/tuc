use anyhow::{Result, bail};
use std::cmp::Ordering;
use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side<T> {
    Some(T),
    Continue,
}

pub type SideI32 = Side<i32>;
pub type SideUsize = Side<usize>;

impl FromStr for Side<i32> {
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

impl FromStr for Side<usize> {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => Side::Continue,
            _ => Side::Some(
                s.parse::<usize>()
                    .or_else(|_| bail!("Not a number `{}`", s))?,
            ),
        })
    }
}

impl fmt::Display for Side<i32> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Side::Some(v) => write!(f, "{v}"),
            Side::Continue => write!(f, ""),
        }
    }
}

impl fmt::Display for Side<usize> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Side::Some(v) => write!(f, "{v}"),
            Side::Continue => write!(f, ""),
        }
    }
}

impl PartialOrd for Side<i32> {
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

impl PartialOrd for Side<usize> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Side::Some(s), Side::Some(o)) => Some(s.cmp(o)),
            (Side::Continue, Side::Some(_)) => Some(Ordering::Greater),
            (Side::Some(_), Side::Continue) => Some(Ordering::Less),
            (Side::Continue, Side::Continue) => Some(Ordering::Equal),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UserBounds<T> {
    pub l: Side<T>,
    pub r: Side<T>,
    pub is_last: bool,
    pub fallback_oob: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundOrFiller<T> {
    Bound(UserBounds<T>),
    Filler(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct UserBoundsList<T> {
    pub list: Vec<BoundOrFiller<T>>,
    /// Optimization that we can use to stop searching for fields.
    /// It's available only when every bound uses positive indexes.
    /// When conditions do not apply, its value is `Side::Continue`.
    pub last_interesting_field: Side<T>,
}

impl fmt::Display for UserBounds<i32> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (Side::Continue, Side::Continue) => write!(f, "1:-1"),
            (l, r) if l == r => write!(f, "{l}"),
            (l, r) => write!(f, "{l}:{r}"),
        }
    }
}

impl fmt::Display for UserBounds<usize> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (Side::Continue, Side::Continue) => write!(f, "1:-1"),
            (l, r) if l == r => write!(f, "{l}"),
            (l, r) => write!(f, "{l}:{r}"),
        }
    }
}

impl UserBounds<i32> {
    /// ```
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>> {
        let parts_length = parts_length as i32;

        let start: i32 = match self.l {
            Side::Continue => 0,
            Side::Some(v) => {
                if v > parts_length || v < -parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 { parts_length + v } else { v - 1 }
            }
        };

        let end: i32 = match self.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v > parts_length || v < -parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 { parts_length + v + 1 } else { v }
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
}

impl UserBounds<usize> {
    /// ```
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>> {
        let start: usize = match self.l {
            Side::Continue => 0,
            Side::Some(v) => {
                if v > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                v - 1
            }
        };

        let end: usize = match self.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                v
            }
        };

        if end <= start {
            // `end` must always be 1 or more greater than start
            bail!("Field left value cannot be greater than right value");
        }

        Ok(Range { start, end })
    }
}

impl TryFrom<Range<usize>> for UserBounds<i32> {
    type Error = anyhow::Error;

    fn try_from(r: Range<usize>) -> Result<UserBounds<i32>> {
        if r.start >= r.end {
            bail!("Range start must be less than end");
        }
        Ok(UserBounds {
            l: Side::Some((r.start + 1) as i32), // convert to 1-indexed
            r: Side::Some(r.end as i32),         // exclusive end, so no need to convert
            is_last: false,
            fallback_oob: None,
        })
    }
}

impl TryFrom<Range<usize>> for UserBounds<usize> {
    type Error = anyhow::Error;

    fn try_from(r: Range<usize>) -> Result<UserBounds<usize>> {
        if r.start >= r.end {
            bail!("Range start must be less than end");
        }
        Ok(UserBounds {
            l: Side::Some(r.start + 1), // convert to 1-indexed
            r: Side::Some(r.end),       // exclusive end, so no need to convert
            is_last: false,
            fallback_oob: None,
        })
    }
}

pub trait UserBoundsTrait<T> {
    fn new(l: Side<T>, r: Side<T>) -> Self;
    fn with_fallback(l: Side<T>, r: Side<T>, fallback_oob: Option<Vec<u8>>) -> Self;
    fn try_into_range(&self, parts_length: usize) -> Result<Range<usize>>;
    fn matches(&self, idx: T) -> Result<bool>;
    fn unpack(&self, num_fields: usize) -> Vec<UserBounds<T>>;

    /// Transform a bound in its complement (invert the bound).
    fn complement(&self, num_fields: usize) -> Result<Vec<UserBounds<T>>> {
        let r = self.try_into_range(num_fields)?;
        let r_complement = complement_std_range(num_fields, &r);
        Ok(r_complement
            .into_iter()
            .map(|x| {
                x.try_into()
                    .expect("Range should be convertible to UserBounds")
            })
            .collect())
    }
}

impl UserBoundsTrait<i32> for UserBounds<i32> {
    fn new(l: Side<i32>, r: Side<i32>) -> Self {
        UserBounds {
            l,
            r,
            is_last: false,
            fallback_oob: None,
        }
    }

    fn with_fallback(l: Side<i32>, r: Side<i32>, fallback_oob: Option<Vec<u8>>) -> Self {
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
                if v < 0 { parts_length + v } else { v - 1 }
            }
        };

        let end: i32 = match self.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v > parts_length || v < -parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 { parts_length + v + 1 } else { v }
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
    fn unpack(&self, num_fields: usize) -> Vec<UserBounds<i32>> {
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

fn main() {
    let abs_list = UserBoundsList {
        list: vec![
            BoundOrFiller::Bound(UserBounds {
                l: Side::Some(1usize),
                r: Side::Some(3usize),
                is_last: false,
                fallback_oob: None,
            }),
            BoundOrFiller::Filler(vec![b'F', b'i', b'l', b'l', b'e', b'r']),
        ],
        last_interesting_field: Side::Continue,
    };

    let i32_list = UserBoundsList {
        list: vec![
            BoundOrFiller::Bound(UserBounds {
                l: Side::Some(-1),
                r: Side::Some(-3),
                is_last: false,
                fallback_oob: None,
            }),
            BoundOrFiller::Filler(vec![b'F', b'i', b'l', b'l', b'e', b'r']),
        ],
        last_interesting_field: Side::Continue,
    };

    dbg!(&abs_list);
    dbg!(&i32_list);
}

struct Container(i32, i32);

// A trait which checks if 2 items are stored inside of container.
// Also retrieves first or last value.
trait Contains {
    // Define generic types here which methods will be able to utilize.
    type A;
    type B;

    fn contains(&self, _: &Self::A, _: &Self::B) -> bool;
    fn first(&self) -> i32;
    fn last(&self) -> i32;
}

impl Contains for Container {
    // Specify what types `A` and `B` are. If the `input` type
    // is `Container(i32, i32)`, the `output` types are determined
    // as `i32` and `i32`.
    type A = i32;
    type B = i32;

    // `&Self::A` and `&Self::B` are also valid here.
    fn contains(&self, number_1: &i32, number_2: &i32) -> bool {
        (&self.0 == number_1) && (&self.1 == number_2)
    }
    // Grab the first number.
    fn first(&self) -> i32 {
        self.0
    }

    // Grab the last number.
    fn last(&self) -> i32 {
        self.1
    }
}

impl Contains for Container {
    // Specify what types `A` and `B` are. If the `input` type
    // is `Container(i32, i32)`, the `output` types are determined
    // as `i32` and `i32`.
    type A = usize;
    type B = i32;

    // `&Self::A` and `&Self::B` are also valid here.
    fn contains(&self, number_1: &i32, number_2: &i32) -> bool {
        (&self.0 == number_1) && (&self.1 == number_2)
    }
    // Grab the first number.
    fn first(&self) -> i32 {
        self.0
    }

    // Grab the last number.
    fn last(&self) -> i32 {
        self.1
    }
}

fn difference<C: Contains>(container: &C) -> i32 {
    container.last() - container.first()
}
