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
            write!(f, "-{}", self.value + 1)
        } else {
            write!(f, "{}", self.value + 1)
        }
    }
}

impl Side {
    /**
     * Create a new Side, using the provided
     * value as-is. The value must be 0-indexed.
     */
    #[inline(always)]
    pub fn with_pos_value(value0idx: usize) -> Self {
        Self {
            value: value0idx,
            is_negative: false,
        }
    }

    /**
     * Create a new Side, using the provided
     * value as-is. The value must be 0-indexed.
     */
    #[inline(always)]
    pub fn with_neg_value(value0idx: usize) -> Self {
        Self {
            value: value0idx,
            is_negative: true,
        }
    }

    /**
     * Create a new Side, positive, with
     * value set to Self::max_right()
     */
    #[inline(always)]
    pub fn with_pos_inf() -> Self {
        Self {
            value: Self::max_right(),
            is_negative: false,
        }
    }

    #[inline(always)]
    pub fn value_unchecked(&self) -> usize {
        self.value
    }

    #[inline(always)]
    #[must_use]
    pub fn value(&self) -> (bool, usize) {
        (self.is_negative, self.value)
    }

    #[inline(always)]
    pub const fn max_right() -> usize {
        usize::MAX
    }

    #[inline(always)]
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
                value: if is_left_bound { 0 } else { Self::max_right() },
                is_negative: false,
            },
            _ => {
                let v = s
                    .parse::<isize>()
                    .or_else(|_| bail!("Not a number `{}`", s))?;

                if v == 0 {
                    bail!("Zero is not a valid field");
                }

                if v > 0 {
                    Side {
                        value: usize::try_from(v.abs()).unwrap() - 1,
                        is_negative: false,
                    }
                } else {
                    Side {
                        value: usize::try_from(v.abs()).unwrap() - 1,
                        is_negative: true,
                    }
                }
            }
        })
    }
}

impl PartialOrd for Side {
    #[inline(always)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_some() {
        assert_eq!(
            Side::from_str_left_bound("42").unwrap(),
            Side {
                value: 41,
                is_negative: false
            }
        );
        assert_eq!(
            Side::from_str_left_bound("-7").unwrap(),
            Side {
                value: 6,
                is_negative: true
            }
        );
    }

    #[test]
    fn test_from_str_continue() {
        assert_eq!(
            Side::from_str_left_bound("").unwrap(),
            Side {
                value: 0,
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
        assert!(Side::from_str_left_bound("0").is_err());
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
