use crate::bounds::{BoundsType, UserBoundsList};
use anyhow::Result;
use std::{path::PathBuf, str::FromStr};

#[cfg(feature = "regex")]
use regex::bytes::Regex;

#[cfg(feature = "regex")]
#[derive(Debug)]
pub struct RegexBag {
    pub normal: Regex,
    pub greedy: Regex,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum EOL {
    Zero = 0,
    Newline = 10,
}

impl From<EOL> for u8 {
    fn from(value: EOL) -> Self {
        match value {
            EOL::Zero => b'\0',
            EOL::Newline => b'\n',
        }
    }
}

#[derive(Debug)]
pub struct Opt {
    pub delimiter: Vec<u8>,
    pub eol: EOL,
    pub bounds: UserBoundsList,
    pub bounds_type: BoundsType,
    pub only_delimited: bool,
    pub greedy_delimiter: bool,
    pub compress_delimiter: bool,
    pub replace_delimiter: Option<Vec<u8>>,
    pub trim: Option<Trim>,
    pub version: bool,
    pub complement: bool,
    pub join: bool,
    pub json: bool,
    pub fixed_memory: Option<usize>,
    pub fallback_oob: Option<Vec<u8>>,
    pub path: Option<PathBuf>,
    pub use_mmap: bool,
    pub read_to_end: bool,
    #[cfg(feature = "regex")]
    pub regex_bag: Option<RegexBag>,
    #[cfg(not(feature = "regex"))]
    pub regex_bag: Option<()>,
}

impl Default for Opt {
    fn default() -> Self {
        Opt {
            delimiter: "-".into(),
            eol: EOL::Newline,
            bounds: UserBoundsList::from_str("1:").unwrap(),
            bounds_type: BoundsType::Fields,
            only_delimited: false,
            greedy_delimiter: false,
            compress_delimiter: false,
            replace_delimiter: None,
            trim: None,
            version: false,
            complement: false,
            join: false,
            json: false,
            fixed_memory: None,
            fallback_oob: None,
            path: None,
            regex_bag: None,
            use_mmap: false,
            read_to_end: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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
