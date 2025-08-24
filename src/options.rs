use crate::{
    args,
    bounds::{BoundOrFiller, BoundsType, UserBoundsList},
};
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

#[derive(Debug)]
pub enum OptError {
    NoFieldBound,
    FixedMemoryZero,
    JoinNoJoin,
    JsonNoJoin,
    CharactersNoJoin,
    CharactersRequireRegexSupport,
    NoJoinReplace,
    JsonReplace,
    JsonPartialSupport,
    FormatFieldJson,
    #[cfg(feature = "regex")]
    MalformedRegex(regex::Error),
}

impl std::fmt::Display for OptError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OptError::NoFieldBound => write!(
                f,
                "tuc: invariant error. At this point we expected to find at least 1 field bound"
            ),
            OptError::FixedMemoryZero => {
                write!(f, "tuc: runtime error. --fixed-memory cannot be 0")
            }
            OptError::JoinNoJoin => {
                write!(
                    f,
                    "tuc: runtime error. It's not possible to use --join and --no-join simultaneously"
                )
            }
            OptError::JsonNoJoin => {
                write!(
                    f,
                    "tuc: runtime error. Using both --json and --no-join is not permitted"
                )
            }
            OptError::NoJoinReplace => {
                write!(
                    f,
                    "tuc: runtime error. You can't pass --no-join when using --replace, which implies --join"
                )
            }
            OptError::JsonReplace => {
                write!(
                    f,
                    "tuc: runtime error. The use of --replace with --json is not supported"
                )
            }
            OptError::CharactersNoJoin => {
                write!(
                    f,
                    "tuc: runtime error. Since --characters implies --join, you can't pass --no-join"
                )
            }
            OptError::CharactersRequireRegexSupport => {
                write!(
                    f,
                    "tuc: runtime error. The use of --characters requires `tuc` to be compiled with `regex` support"
                )
            }
            OptError::JsonPartialSupport => {
                write!(
                    f,
                    "tuc: runtime error. --json support is available only for --fields and --characters"
                )
            }
            #[cfg(feature = "regex")]
            OptError::MalformedRegex(e) => {
                write!(
                    f,
                    "tuc: runtime error. The regular expression is malformed. {e}"
                )
            }
            OptError::FormatFieldJson => {
                write!(
                    f,
                    "tuc: runtime error. Cannot format fields when using --json"
                )
            }
        }
    }
}

impl std::error::Error for OptError {}

impl TryFrom<args::Args> for Opt {
    type Error = OptError;
    fn try_from(mut value: args::Args) -> std::result::Result<Self, Self::Error> {
        let bounds_type = if value.cut_by_fields.is_some() {
            BoundsType::Fields
        } else if value.cut_by_bytes.is_some() {
            BoundsType::Bytes
        } else if value.cut_by_characters.is_some() {
            BoundsType::Characters
        } else if value.cut_by_lines.is_some() {
            BoundsType::Lines
        } else {
            // Default to the match every field
            value.cut_by_fields = Some(UserBoundsList::from_str("1:").unwrap());
            BoundsType::Fields
        };

        if bounds_type == BoundsType::Fields
            && (value.cut_by_fields.is_none() || value.cut_by_fields.as_ref().unwrap().is_empty())
        {
            return Err(OptError::NoFieldBound);
        }

        let bounds = value
            .cut_by_fields
            .or(value.cut_by_characters)
            .or(value.cut_by_bytes)
            .or(value.cut_by_lines)
            .unwrap();

        let delimiter: Vec<u8> = match bounds_type {
            BoundsType::Fields => value.delimiter.unwrap_or_else(|| "\t".into()),
            BoundsType::Lines => "\n".into(),
            _ => Vec::new(),
        };

        if value.fixed_memory_kb == Some(0) {
            return Err(OptError::FixedMemoryZero);
        };

        // convert from kilobytes to megabytes
        let fixed_memory = value.fixed_memory_kb.map(|x| x * 1024);

        if value.join_yes && value.join_no {
            return Err(OptError::JoinNoJoin);
        }

        if value.json && value.join_no {
            return Err(OptError::JsonNoJoin);
        }

        if value.replace_delimiter.is_some() {
            if value.join_no {
                return Err(OptError::NoJoinReplace);
            } else if value.json {
                return Err(OptError::JsonReplace);
            }
        }

        if bounds_type == BoundsType::Characters && value.join_no {
            return Err(OptError::CharactersNoJoin);
        }

        if bounds_type == BoundsType::Characters && cfg!(not(feature = "regex")) {
            return Err(OptError::CharactersRequireRegexSupport);
        }

        if bounds_type == BoundsType::Characters {
            value.replace_delimiter = Some("".into());
        }

        if value.json {
            value.replace_delimiter = Some(",".into());
        }

        let join = value.join_yes
            || value.json
            || value.replace_delimiter.is_some()
            || (bounds_type == BoundsType::Lines && !value.join_no)
            || (bounds_type == BoundsType::Characters);

        if value.json && bounds_type != BoundsType::Characters && bounds_type != BoundsType::Fields
        {
            return Err(OptError::JsonPartialSupport);
        }

        #[cfg(not(feature = "regex"))]
        let regex_bag: Option<()> = None;

        #[cfg(feature = "regex")]
        let regex_bag: Option<RegexBag> = (if bounds_type == BoundsType::Characters {
            Some("\\b|\\B".to_owned())
        } else {
            value.regex
        })
        .map(|regex_text| -> Result<RegexBag, OptError> {
            Ok(RegexBag {
                normal: Regex::new(&regex_text).map_err(OptError::MalformedRegex)?,
                greedy: Regex::new(&format!("({})+", &regex_text))
                    .map_err(OptError::MalformedRegex)?,
            })
        })
        .transpose()?;

        if value.json && bounds.iter().any(|s| matches!(s, BoundOrFiller::Filler(_))) {
            return Err(OptError::FormatFieldJson);
        }

        let eol = if value.zero_terminated {
            EOL::Zero
        } else {
            EOL::Newline
        };

        let use_mmap = value.path.is_some() && !value.mmap_no && !cfg!(target_os = "macos");

        Ok(Opt {
            // derived
            bounds_type,
            bounds,
            delimiter,
            fixed_memory,
            join,
            regex_bag,
            eol,
            use_mmap,

            // direct
            replace_delimiter: value.replace_delimiter,
            complement: value.complement,
            compress_delimiter: value.compress_delimiter,
            fallback_oob: value.fallback_oob,
            only_delimited: value.only_delimited,
            greedy_delimiter: value.greedy_delimiter,
            trim: value.trim,
            json: value.json,
            path: value.path,

            // decided later at runtime
            read_to_end: false,
        })
    }
}

#[cfg(test)]
impl std::str::FromStr for Opt {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let args: crate::args::Args = s.parse()?;
        args.try_into().map_err(|e: OptError| e.into())
    }
}
