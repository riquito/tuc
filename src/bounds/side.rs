use anyhow::{Result, bail};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

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
                if s.is_negative() ^ o.is_negative() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_some() {
        assert_eq!(Side::from_str("42").unwrap(), Side::Some(42));
        assert_eq!(Side::from_str("-7").unwrap(), Side::Some(-7));
    }

    #[test]
    fn test_from_str_continue() {
        assert_eq!(Side::from_str("").unwrap(), Side::Continue);
    }

    #[test]
    fn test_from_str_zero() {
        assert_eq!(Side::from_str("0").unwrap(), Side::Some(0));
    }

    #[test]
    fn test_from_str_invalid() {
        assert!(Side::from_str("abc").is_err());
        assert!(Side::from_str("4.2").is_err());
    }

    #[test]
    fn test_partial_ord_same_sign() {
        assert_eq!(
            Side::Some(3).partial_cmp(&Side::Some(5)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Side::Some(5).partial_cmp(&Side::Some(3)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Side::Some(7).partial_cmp(&Side::Some(7)),
            Some(Ordering::Equal)
        );
        assert_eq!(
            Side::Some(-2).partial_cmp(&Side::Some(-1)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn test_partial_ord_different_sign() {
        assert_eq!(Side::Some(3).partial_cmp(&Side::Some(-5)), None);
        assert_eq!(Side::Some(-5).partial_cmp(&Side::Some(3)), None);
    }

    #[test]
    fn test_partial_ord_continue() {
        assert_eq!(
            Side::Continue.partial_cmp(&Side::Some(1)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Side::Some(1).partial_cmp(&Side::Continue),
            Some(Ordering::Less)
        );
        assert_eq!(
            Side::Continue.partial_cmp(&Side::Continue),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn test_partial_ord_zero() {
        // Zero compared to positive
        assert_eq!(
            Side::Some(0).partial_cmp(&Side::Some(5)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Side::Some(5).partial_cmp(&Side::Some(0)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Side::Some(0).partial_cmp(&Side::Some(0)),
            Some(Ordering::Equal)
        );

        // Zero compared to negative
        assert_eq!(Side::Some(0).partial_cmp(&Side::Some(-5)), None);
        assert_eq!(Side::Some(-5).partial_cmp(&Side::Some(0)), None);
    }

    #[test]
    fn test_eq_trait() {
        assert_eq!(Side::Some(1), Side::Some(1));
        assert_eq!(Side::Continue, Side::Continue);

        assert_ne!(Side::Some(1), Side::Some(2));
        assert_ne!(Side::Continue, Side::Some(0));
    }
}
