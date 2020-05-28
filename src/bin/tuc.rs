use anyhow::{bail, Result};
use regex::{escape, Regex};
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
    /// Fields to keep, like 1-3 or 3,2 or 1- or 3,1-2 or -3 or -3--2
    #[structopt(short, long, default_value = "1-")]
    fields: RangeList,
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
                    .or_else(|_| Err(format!("Not a number `{}`", s)))?,
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
            &[""] => {
                return Err("Field format error: empty field".into());
            }
            &["", ""] => {
                return Err("Field format error, no numbers next to `:`".into());
            }
            &[x, y] => (Side::from_str(x)?, Side::from_str(y)?),
            &[x] => (Side::from_str(x)?, Side::from_str(x)?),
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

fn cut_line(out: &mut dyn Write, re: &Regex, fields: &RangeList, content: String) -> Result<()> {
    let delimiter_indices: Vec<(usize, usize)> = re
        .find_iter(&content)
        .map(|m| (m.start(), m.end()))
        .collect::<Vec<_>>();
    let parts_length: usize = delimiter_indices.len() + 1;

    for f in &fields.0 {
        let l: usize;
        let r: usize;

        l = match f.l {
            Side::Continue => 1,
            Side::Some(v) => {
                if v.abs() as usize > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length - (v * -1) as usize + 1
                } else {
                    v as usize
                }
            }
        };

        r = match f.r {
            Side::Continue => parts_length,
            Side::Some(v) => {
                if v.abs() as usize > parts_length {
                    bail!("Out of bounds: {}", v);
                }
                if v < 0 {
                    parts_length - (v * -1) as usize + 1
                } else {
                    v as usize
                }
            }
        };

        if l > r {
            bail!("Invalid decreasing range")
        }

        //      0       delimiter_indices
        //  1       2   parts
        // 012 345 678  indices
        // aaa bbb ccc  content

        let str_l_idx: usize = match l {
            1 => 0,
            v => delimiter_indices[(v - 2) as usize].1,
        };

        let str_r_idx: usize = match r {
            v if v as usize == parts_length => content.len(),
            v => delimiter_indices[(v - 1) as usize].0,
        };

        write!(out, "{}", &content[str_l_idx..str_r_idx])?;
    }

    Ok(())
}

fn main() -> Result<()> {
    let matches = Opt::clap()
        .setting(structopt::clap::AppSettings::AllowLeadingHyphen)
        .get_matches();

    let opt = Opt::from_clap(&matches);
    let re: Regex = Regex::new(format!("({})+", escape(&opt.delimiter)).as_ref()).unwrap();

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    stdin
        .lock()
        .lines()
        .try_for_each(|line| cut_line(&mut stdout, &re, &opt.fields, line?))?;

    println!("");

    Ok(())
}
