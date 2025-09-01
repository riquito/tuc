use anyhow::{Result, bail};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    // Pack positive and negative fields into a single u64:
    // - bit 63: is_negative flag
    // - bits 0-62: value (supports values up to 2^62)
    packed: u64,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_negative() {
            write!(f, "-{}", self.value_unchecked() + 1)
        } else {
            write!(f, "{}", self.value_unchecked() + 1)
        }
    }
}

impl Side {
    const NEGATIVE_FLAG: u64 = 1u64 << 63;
    const VALUE_MASK: u64 = !Self::NEGATIVE_FLAG;
    const MAX_VALUE: usize = (Self::VALUE_MASK) as usize; // 2^63 - 1

    /**
     * Create a new Side, using the provided
     * value as-is. The value must be 0-indexed.
     */
    #[inline(always)]
    pub fn with_pos_value(value: usize) -> Self {
        debug_assert!(value <= (Self::MAX_VALUE), "Value too large");
        Self {
            packed: (value as u64),
        }
    }

    /**
     * Create a new Side, using the provided
     * value as-is. The value must be 0-indexed.
     */
    #[inline(always)]
    pub fn with_neg_value(value: usize) -> Self {
        debug_assert!(value <= (Self::MAX_VALUE), "Value too large");
        Self {
            packed: (value as u64) | Self::NEGATIVE_FLAG,
        }
    }

    /**
     * Create a new Side, positive, with
     * value set to Self::max_right()
     */
    #[inline(always)]
    pub fn with_pos_inf() -> Self {
        Self {
            packed: Self::MAX_VALUE as u64,
        }
    }

    #[inline(always)]
    pub fn value_unchecked(&self) -> usize {
        (self.packed & Self::VALUE_MASK) as usize
    }

    #[inline(always)]
    #[must_use]
    pub fn value(&self) -> (bool, usize) {
        (self.is_negative(), self.value_unchecked())
    }

    #[inline(always)]
    pub const fn max_right() -> usize {
        Self::MAX_VALUE
    }

    #[inline(always)]
    pub fn is_negative(&self) -> bool {
        self.packed & Self::NEGATIVE_FLAG != 0
    }

    pub fn from_str_left_bound(s: &str) -> Result<Self, anyhow::Error> {
        Self::from_str(s, true)
    }

    pub fn from_str_right_bound(s: &str) -> Result<Self, anyhow::Error> {
        Self::from_str(s, false)
    }

    fn from_str(s: &str, is_left_bound: bool) -> Result<Self, anyhow::Error> {
        Ok(match s {
            "" => Side::with_pos_value(if is_left_bound { 0 } else { Self::max_right() }),
            _ => {
                let v = s
                    .parse::<isize>()
                    .or_else(|_| bail!("Not a number `{}`", s))?;

                if v == 0 {
                    bail!("Zero is not a valid field");
                }

                if v > 0 {
                    Side::with_pos_value(usize::try_from(v.abs()).unwrap() - 1)
                } else {
                    Side::with_neg_value(usize::try_from(v.abs()).unwrap() - 1)
                }
            }
        })
    }
}

impl PartialOrd for Side {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.is_negative() ^ other.is_negative() {
            // We can't compare two sides with different sign
            return None;
        }

        if self.is_negative() {
            Some(other.value_unchecked().cmp(&self.value_unchecked()))
        } else {
            Some(self.value_unchecked().cmp(&other.value_unchecked()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_some() {
        assert_eq!(
            Side::from_str_left_bound("42").unwrap().value(),
            (false, 41)
        );
        assert_eq!(Side::from_str_left_bound("-7").unwrap().value(), (true, 6));
    }

    #[test]
    fn test_from_str_continue() {
        assert_eq!(Side::from_str_left_bound("").unwrap().value(), (false, 0));
        assert_eq!(
            Side::from_str_right_bound("").unwrap().value(),
            (false, Side::max_right())
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
