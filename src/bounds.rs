use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum BoundsType {
    Bytes,
    Characters,
    Fields,
    Lines,
}

#[derive(Debug)]
pub struct UserBoundsList(pub Vec<UserBounds>);

impl FromStr for UserBoundsList {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let k: Result<Vec<UserBounds>, _> = s.split(',').map(UserBounds::from_str).collect();
        Ok(UserBoundsList(k?))
    }
}

impl UserBoundsList {
    pub fn is_sortable(&self) -> bool {
        let mut has_positive_idx = false;
        let mut has_negative_idx = false;
        self.0.iter().for_each(|b| {
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

    pub fn is_sorted(&self) -> bool {
        self.0.windows(2).all(|w| w[0] <= w[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Some(i32),
    Continue,
}

impl FromStr for Side {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => Side::Continue,
            _ => Side::Some(
                s.parse::<i32>()
                    .map_err(|_| format!("Not a number `{}`", s))?,
            ),
        })
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Side::Some(v) => write!(f, "{}", v),
            Side::Continue => write!(f, ""),
        }
    }
}

#[derive(Debug, Eq, Clone)]
pub struct UserBounds {
    pub l: Side,
    pub r: Side,
}

impl fmt::Display for UserBounds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (Side::Continue, Side::Continue) => write!(f, "1:-1"),
            (l, r) if l == r => write!(f, "{}", l),
            (l, r) => write!(f, "{}:{}", l, r),
        }
    }
}

impl FromStr for UserBounds {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Field format error: empty field".into());
        } else if s == ":" {
            return Err("Field format error, no numbers next to `:`".into());
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
                return Err("Field value 0 is not allowed (fields are 1-indexed)".into());
            }
            (_, Side::Some(0)) => {
                return Err("Field value 0 is not allowed (fields are 1-indexed)".into());
            }
            (Side::Some(left), Side::Some(right)) if right < left => {
                return Err("Field left value cannot be greater than right value".into());
            }
            _ => (),
        }

        Ok(UserBounds::new(l, r))
    }
}

impl UserBounds {
    pub fn new(l: Side, r: Side) -> Self {
        UserBounds { l, r }
    }
    /**
     * Check if an index is between the bounds.
     *
     * It errors out if the index has different sign than the bounds
     * (we can't verify if e.g. -1 idx is between 3:5 without knowing the number
     * of matching bounds).
     */
    pub fn matches(&self, idx: i32) -> Result<bool> {
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
}

impl Ord for UserBounds {
    /*
     * Compare UserBounds. Note that comparison gives wrong results if
     * bounds happen to have a mix of positive/negative indexes (you cannot
     * reliably compare -1 with 3 without kwowing how many parts are there).
     * Check with UserBounds.is_sortable before comparing.
     */
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            return Ordering::Equal;
        }

        match (self.l, self.r, other.l, other.r) {
            (_, Side::Some(s_r), Side::Some(o_l), _) if (s_r * o_l).is_positive() && s_r <= o_l => {
                Ordering::Less
            }
            _ => Ordering::Greater,
        }
    }
}

impl PartialOrd for UserBounds {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UserBounds {
    fn eq(&self, other: &Self) -> bool {
        (self.l, self.r) == (other.l, other.r)
    }
}

impl Default for UserBounds {
    fn default() -> Self {
        UserBounds::new(Side::Some(1), Side::Some(1))
    }
}

pub fn bounds_to_std_range(parts_length: usize, bounds: &UserBounds) -> Result<Range<usize>> {
    let start: usize = match bounds.l {
        Side::Continue => 0,
        Side::Some(v) => {
            if v.abs() as usize > parts_length {
                bail!("Out of bounds: {}", v);
            }
            if v < 0 {
                parts_length - v.abs() as usize
            } else {
                v as usize - 1
            }
        }
    };

    let end: usize = match bounds.r {
        Side::Continue => parts_length,
        Side::Some(v) => {
            if v.abs() as usize > parts_length {
                bail!("Out of bounds: {}", v);
            }
            if v < 0 {
                parts_length - v.abs() as usize + 1
            } else {
                v as usize
            }
        }
    };

    Ok(Range { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_user_bounds_is_sortable() {
        assert!(UserBoundsList(Vec::new()).is_sortable());

        assert!(UserBoundsList(vec![UserBounds::from_str("1").unwrap(),]).is_sortable());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("1").unwrap(),
            UserBounds::from_str("2").unwrap(),
        ])
        .is_sortable());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("3").unwrap(),
            UserBounds::from_str("2").unwrap(),
        ])
        .is_sortable());

        assert!(!UserBoundsList(vec![
            UserBounds::from_str("-1").unwrap(),
            UserBounds::from_str("1").unwrap(),
        ])
        .is_sortable());

        assert!(!UserBoundsList(vec![
            UserBounds::from_str("-1:").unwrap(),
            UserBounds::from_str(":1").unwrap(),
        ])
        .is_sortable());
    }

    #[test]
    fn test_vec_of_bounds_is_sorted() {
        assert!(UserBoundsList(vec![UserBounds::from_str("1").unwrap(),]).is_sorted());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("1").unwrap(),
            UserBounds::from_str("2").unwrap(),
        ])
        .is_sorted());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("-2").unwrap(),
            UserBounds::from_str("-1").unwrap(),
        ])
        .is_sorted());

        assert!(UserBoundsList(vec![
            UserBounds::from_str(":1").unwrap(),
            UserBounds::from_str("2:4").unwrap(),
            UserBounds::from_str("5:").unwrap(),
        ])
        .is_sorted());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("1").unwrap(),
            UserBounds::from_str("1:2").unwrap(),
        ])
        .is_sorted());

        assert!(UserBoundsList(vec![
            UserBounds::from_str("1").unwrap(),
            UserBounds::from_str("1").unwrap(),
            UserBounds::from_str("2").unwrap(),
        ])
        .is_sorted());
    }
}
