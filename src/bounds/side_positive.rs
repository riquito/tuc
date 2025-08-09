use anyhow::{Result, bail};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

trait SomeSide {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsSide {
    Some(usize),
    Continue,
}

pub struct Foo(Option<usize>);

impl FromStr for AbsSide {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "" => AbsSide::Continue,
            _ => AbsSide::Some(
                s.parse::<usize>()
                    .or_else(|_| bail!("Not a number `{}`", s))?,
            ),
        })
    }
}

impl fmt::Display for AbsSide {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AbsSide::Some(v) => write!(f, "{v}"),
            AbsSide::Continue => write!(f, ""),
        }
    }
}

impl PartialOrd for AbsSide {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (AbsSide::Some(s), AbsSide::Some(o)) => {
                if !(s * o).is_positive() {
                    // We can't compare two sides with different sign
                    return None;
                }
                Some(s.cmp(o))
            }
            (AbsSide::Continue, AbsSide::Some(_)) => Some(Ordering::Greater),
            (AbsSide::Some(_), AbsSide::Continue) => Some(Ordering::Less),
            (AbsSide::Continue, AbsSide::Continue) => Some(Ordering::Equal),
        }
    }
}
