use crate::bounds::{BoundsType, UserBoundsList};
use anyhow::Result;
use std::str::FromStr;

#[cfg(feature = "regex")]
use regex::Regex;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum EOL {
    Zero = 0,
    Newline = 10,
}

#[derive(Debug)]
pub struct Opt {
    pub delimiter: String,
    pub eol: EOL,
    pub bounds: UserBoundsList,
    pub bounds_type: BoundsType,
    pub only_delimited: bool,
    pub greedy_delimiter: bool,
    pub compress_delimiter: bool,
    pub replace_delimiter: Option<String>,
    pub trim: Option<Trim>,
    pub version: bool,
    pub complement: bool,
    pub join: bool,
    #[cfg(feature = "regex")]
    pub regex: Option<Regex>,
    #[cfg(not(feature = "regex"))]
    pub regex: Option<()>,
}

impl Default for Opt {
    fn default() -> Self {
        Opt {
            delimiter: String::from("-"),
            eol: EOL::Newline,
            bounds: UserBoundsList(Vec::new()),
            bounds_type: BoundsType::Fields,
            only_delimited: false,
            greedy_delimiter: false,
            compress_delimiter: false,
            replace_delimiter: None,
            trim: None,
            version: false,
            complement: false,
            join: false,
            regex: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trim {
    Left,
    Right,
    Both,
}

impl FromStr for Trim {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "l" | "L" => Trim::Left,
            "r" | "R" => Trim::Right,
            "b" | "B" => Trim::Both,
            _ => return Err("Valid trim values are (l|L)eft, (r|R)ight, (b|B)oth".into()),
        })
    }
}
