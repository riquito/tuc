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
