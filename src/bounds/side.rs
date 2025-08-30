use anyhow::{Result, bail};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    value: usize,
    is_negative: bool,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_negative {
            write!(f, "-{}", self.value)
        } else {
            write!(f, "{}", self.value)
        }
    }
}

impl From<i32> for Side {
    fn from(value: i32) -> Self {
        if value >= 0 {
            Side {
                value: usize::try_from(value).unwrap(),
                is_negative: false,
            }
        } else {
            Side {
                value: usize::try_from(value.abs()).unwrap(),
                is_negative: true,
            }
        }
    }
}

impl Side {
    pub fn abs_value(&self) -> usize {
        self.value
    }

    pub fn abs_value_unchecked(&self) -> usize {
        self.value
    }

    #[must_use]
    pub fn value(&self) -> (bool, usize) {
        (self.is_negative, self.value)
    }

    pub fn new_inf_left() -> Self {
        Self {
            value: Self::min_left(),
            is_negative: false,
        }
    }

    pub fn new_inf_right() -> Self {
        Self {
            value: Self::max_right(),
            is_negative: false,
        }
    }

    pub const fn min_left() -> usize {
        1
    }

    pub const fn max_right() -> usize {
        usize::MAX
    }

    pub fn is_negative(&self) -> bool {
        self.is_negative
    }

    pub fn from_str_left_bound(s: &str) -> Result<Self, anyhow::Error> {
        Self::from_str(s, true)
    }

    pub fn from_str_right_bound(s: &str) -> Result<Self, anyhow::Error> {
        Self::from_str(s, false)
    }

    fn from_str(s: &str, is_left_bound: bool) -> Result<Self, anyhow::Error> {
        Ok(match s {
            "" => Side {
                value: if is_left_bound {
                    Self::min_left()
                } else {
                    Self::max_right()
                },
                is_negative: false,
            },
            _ => {
                let v = s
                    .parse::<isize>()
                    .or_else(|_| bail!("Not a number `{}`", s))?;

                if v >= 0 {
                    Side {
                        value: usize::try_from(v.abs()).unwrap(),
                        is_negative: false,
                    }
                } else {
                    Side {
                        value: usize::try_from(v.abs()).unwrap(),
                        is_negative: true,
                    }
                }
            }
        })
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
    pub fn between(&self, other: &Self, idx: usize) -> Result<bool> {
        if self.is_negative ^ other.is_negative {
            // We can't compare two sides with different sign
            bail!(
                "sign mismatch. Can't verify if index {} is between bounds {}",
                idx,
                self
            )
        }

        Ok((self.value..=other.value).contains(&idx))
    }
}

impl PartialOrd for Side {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.is_negative ^ other.is_negative {
            // We can't compare two sides with different sign
            return None;
        }

        if self.is_negative {
            Some(other.value.cmp(&self.value))
        } else {
            Some(self.value.cmp(&other.value))
        }
    }
}

impl From<usize> for Side {
    fn from(value: usize) -> Self {
        Side {
            value,
            is_negative: false,
        }
    }
}

impl From<isize> for Side {
    fn from(value: isize) -> Self {
        if value >= 0 {
            Side {
                value: usize::try_from(value).unwrap(),
                is_negative: false,
            }
        } else {
            Side {
                value: usize::try_from(value.abs()).unwrap(),
                is_negative: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_some() {
        assert_eq!(
            Side::from_str_left_bound("42").unwrap(),
            Side {
                value: 42,
                is_negative: false
            }
        );
        assert_eq!(
            Side::from_str_left_bound("-7").unwrap(),
            Side {
                value: 7,
                is_negative: true
            }
        );
    }

    #[test]
    fn test_from_str_continue() {
        assert_eq!(
            Side::from_str_left_bound("").unwrap(),
            Side {
                value: 1,
                is_negative: false
            }
        );
        assert_eq!(
            Side::from_str_right_bound("").unwrap(),
            Side {
                value: usize::MAX,
                is_negative: false
            }
        );
    }

    #[test]
    fn test_from_str_zero() {
        assert_eq!(
            Side::from_str_left_bound("0").unwrap(),
            Side {
                value: 0,
                is_negative: false
            }
        );
    }

    #[test]
    fn test_from_str_invalid() {
        assert!(Side::from_str_left_bound("abc").is_err());
        assert!(Side::from_str_left_bound("4.2").is_err());
    }

    #[test]
    fn test_partial_ord_same_sign() {
        assert_eq!(
            Side::from_str_left_bound("3")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("5").unwrap()),
            Some(Ordering::Less)
        );
        assert_eq!(
            Side::from_str_left_bound("5")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("3").unwrap()),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Side::from_str_left_bound("7")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("7").unwrap()),
            Some(Ordering::Equal)
        );
        assert_eq!(
            Side::from_str_left_bound("-2")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("-1").unwrap()),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn test_partial_ord_different_sign() {
        assert_eq!(
            Side::from_str_left_bound("3")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("-5").unwrap()),
            None
        );
        assert_eq!(
            Side::from_str_left_bound("-5")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("3").unwrap()),
            None
        );
    }

    #[test]
    fn test_partial_ord_zero() {
        // Zero compared to positive
        assert_eq!(
            Side::from_str_left_bound("")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("5").unwrap()),
            Some(Ordering::Less)
        );
        assert_eq!(
            Side::from_str_left_bound("5")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("0").unwrap()),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Side::from_str_left_bound("0")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("0").unwrap()),
            Some(Ordering::Equal)
        );

        // Zero compared to negative
        assert_eq!(
            Side::from_str_left_bound("0")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("-5").unwrap()),
            None
        );
        assert_eq!(
            Side::from_str_left_bound("-5")
                .unwrap()
                .partial_cmp(&Side::from_str_left_bound("0").unwrap()),
            None
        );
    }

    #[test]
    fn test_eq_trait() {
        assert_eq!(
            Side::from_str_left_bound("1").unwrap(),
            Side::from_str_left_bound("1").unwrap()
        );

        assert_eq!(
            Side::from_str_left_bound("-1").unwrap(),
            Side::from_str_left_bound("-1").unwrap()
        );

        assert_ne!(
            Side::from_str_left_bound("1").unwrap(),
            Side::from_str_left_bound("2").unwrap()
        );

        assert_ne!(
            Side::from_str_left_bound("-1").unwrap(),
            Side::from_str_left_bound("1").unwrap()
        );
    }
}
