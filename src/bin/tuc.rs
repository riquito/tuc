use anyhow::{bail, Result};
use regex::{escape, NoExpand, Regex};
use std::fmt;
use std::io::{BufRead, Write};
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "tuc", about = "When cut doesn't cut it.")]

struct Opt {
    /// Delimiter to use to cut the text into pieces
    #[structopt(short, long, default_value = "\t")]
    delimiter: String,
    /// Fields to keep, like 1:3 or 3,2 or 1: or 3,1:2 or -3 or -3:-2
    #[structopt(short, long, default_value = "1:")]
    fields: RangeList,
    /// Do not print lines not containing delimiters
    #[structopt(short = "s", long = "only-delimited")]
    only_delimited: bool,
    /// Display the delimiter at most once in a sequence
    #[structopt(short = "p", long)]
    compress_delimiter: bool,
    /// Replace the delimiter
    #[structopt(short = "r")]
    replace_delimiter: Option<String>,
    /// Trim the delimiter (trim is applied before any other cut or replace)
    #[structopt(
        short = "t",
        help = "Valid trim values are (l|L)eft, (r|R)ight, (b|B)oth"
    )]
    trim: Option<Trim>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Trim {
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
struct RangeList(Vec<Range>);

impl FromStr for RangeList {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let k: Result<Vec<Range>, _> = s.split(',').map(Range::from_str).collect();
        Ok(RangeList(k?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side {
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
            Side::Continue => write!(f, "-"),
        }
    }
}

#[derive(Debug)]
struct Range {
    l: Side,
    r: Side,
    raw: String,
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl FromStr for Range {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pair: Vec<&str> = s.split(':').collect::<Vec<&str>>();

        let (l, r): (Side, Side) = match &pair[..] {
            [""] => {
                return Err("Field format error: empty field".into());
            }
            ["", ""] => {
                return Err("Field format error, no numbers next to `:`".into());
            }
            [x, y] => (Side::from_str(x)?, Side::from_str(y)?),
            [x] => (Side::from_str(x)?, Side::from_str(x)?),
            _ => {
                return Err(format!("Field format error (too many `:` in `{}`)", s).into());
            }
        };

        match (l, r) {
            (Side::Some(0), _) => {
                return Err("Field value 0 is not allowed (fields are 1-indexed)".into());
            }
            (_, Side::Some(0)) => {
                return Err("Field value 0 is not allowed (fields are 1-indexed)".into());
            }
            _ => (),
        }

        Ok(Range::new(l, r, s.to_string()))
    }
}

impl Range {
    pub fn new(l: Side, r: Side, raw: String) -> Self {
        Range { l, r, raw }
    }
}

impl Default for Range {
    fn default() -> Self {
        Range::new(Side::Some(1), Side::Some(1), String::from("1"))
    }
}

fn get_parts_by_fields_range<'a>(parts: &'a [&'a str], f: &Range) -> Result<&'a [&'a str]> {
    let parts_length: usize = parts.len();

    let l: usize;
    let r: usize;

    l = match f.l {
        Side::Continue => 1,
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
    } - 1;

    r = match f.r {
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

    Ok(&parts[l..r])
}

fn main() -> Result<()> {
    let matches = Opt::clap()
        .setting(structopt::clap::AppSettings::AllowLeadingHyphen)
        .get_matches();

    let opt = Opt::from_clap(&matches);
    let re: Regex = Regex::new(format!("({})+", escape(&opt.delimiter)).as_ref()).unwrap();

    let stdin = std::io::stdin();
    let mut stdout = grep_cli::stdout(termcolor::ColorChoice::Never);

    stdin
        .lock()
        .lines()
        .try_for_each::<_, Result<()>>(|maybe_line| {
            let line = &maybe_line?;

            let line = match opt.trim {
                Some(Trim::Both) => line
                    .trim_start_matches(&opt.delimiter)
                    .trim_end_matches(&opt.delimiter),
                Some(Trim::Left) => line.trim_start_matches(&opt.delimiter),
                Some(Trim::Right) => line.trim_end_matches(&opt.delimiter),
                _ => line,
            };

            let owner_compress;
            let line: &str = if opt.compress_delimiter {
                owner_compress = re.replace_all(line, NoExpand(&opt.delimiter));
                owner_compress.as_ref()
            } else {
                line
            };

            let parts = line.split(&opt.delimiter);
            let collected_parts = parts.collect::<Vec<_>>();

            match collected_parts.len() {
                1 if opt.only_delimited => (),
                1 => {
                    writeln!(stdout, "{}", &line)?;
                }
                _ => {
                    for f in &opt.fields.0 {
                        let matching_parts = get_parts_by_fields_range(&collected_parts, f)?;

                        let cut_line: &str = &matching_parts.join(&opt.delimiter);
                        let mut edited_line: &str = cut_line;

                        let owner_replace;

                        if let Some(replace_delimiter) = &opt.replace_delimiter {
                            owner_replace = edited_line.replace(&opt.delimiter, &replace_delimiter);
                            edited_line = &owner_replace;
                        }

                        write!(stdout, "{}", edited_line)?;
                    }
                    writeln!(stdout)?;
                }
            };

            Ok(())
        })?;

    Ok(())
}
