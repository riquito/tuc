use anyhow::{bail, Context, Result};
use std::io::Read;
use std::str::FromStr;
use std::fmt;
use structopt::StructOpt;


#[derive(Debug, StructOpt)]
#[structopt(
    name = "tuc",
    about = "When cut doesn't cut it."
)]

struct Opt {
    /// Delimiter to use to cut the text into pieces
    #[structopt(short, long, default_value="\t")]
    delimiter: String,
    /// Fields to keep, like 1-3 or 3,2 or 1- or 3,1-2 or -3 or -3--2
    #[structopt(short, long, default_value="1-")]
    fields: RangeList,
}

#[derive(Debug)]
struct RangeList(Vec<Range>);

impl FromStr for RangeList {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let k: Result<Vec<Range>,_> = s.split(',').map(Range::from_str).collect();
        Ok(RangeList(k?))
    }
}

#[derive(Debug, PartialEq)]
enum Side {
    None,
    Some(i32),
    Continue,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Side::None => write!(f, ""),
            Side::Some(v) => write!(f, "{}", v),
            Side::Continue => write!(f, "-"),
        }
    }
}

#[derive(Debug)]
struct Range {
    l: Side,
    r: Side,
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}{}",
            self.l,
            if self.r != Side::None {
                "-"
            } else { "" },
            self.r,
        )
    }
}


impl FromStr for Range {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 0 {
            return Err("Empty field".into());
        }

        let mut search_start_from = 0;
        let mut chars = s.chars();

        if chars.next().unwrap() == '-' {
            if s.len() == 1 {
                return Err("Cannot parse `-` by itself, there must be number next it".into());
            } else {
                search_start_from = if chars.next() == Some('-') {
                    // e.g. --9 a.k.a. -(-9)
                    0
                } else {
                    // e.g. -1...-
                    1
                };
            }
        }

        let l: Side;
        let mut r: Side = Side::None;
        match s[search_start_from..].find('-') {
            Some(k) => {
                let idx = k + search_start_from;

                l = if s[..idx].len() == 0 {
                    Side::Continue
                } else {
                    Side::Some(s[..idx].parse::<i32>().or_else(|_| Err(format!("Not a number: {}`", s[..idx].to_string())))?)
                };

                r = if s[idx+1..].len() == 0 {
                    Side::Continue
                } else {
                    Side::Some(s[idx+1..].parse::<i32>().or_else(|_| Err(format!("Not a number: {}`", s[idx+1..].to_string())))?)
                };
            },
            None => {
                l = Side::Some(s.parse::<i32>().or_else(|_| Err(format!("Not a number `{}`", s)))?);
            }
        }

        Ok(Range::new(l,r))
    }
}

impl Range {
    pub fn new(l: Side, r: Side) -> Self {
        Range {
            l,
            r,
        }
    }
}

impl Default for Range {
    fn default() -> Self {
        Range::new(Side::Some(1), Side::None)
    }
}


fn main() -> Result<()> {
    let matches = Opt::clap()
        .setting(structopt::clap::AppSettings::AllowLeadingHyphen)
        .get_matches();

    let opt = Opt::from_clap(&matches);
    println!("{:?}",opt);

    let mut content = String::new();
    std::io::stdin()
            .read_to_string(&mut content)
            .with_context(|| format!("Cannot read from STDIN"))?;

    content = content.trim().to_string();

    let parts: Vec<&str> = content.split(&opt.delimiter).collect::<Vec<&str>>();

    let parts_length = parts.len();
    let fields_vec = match opt.fields { RangeList(v) => v };
    for f in fields_vec {
        match f.l {
            Side::Some(n) => {
                if n.abs() as usize > parts_length {
                    bail!("Out of bounds: {}. Index {} reaches beyond the number of parts ({})", f, n, parts_length);
                }
            }
            _ => (),
        }
        match f.r {
            Side::Some(n) => {
                if n.abs() as usize > parts_length {
                    bail!("Out of bounds: {}. Index {} reaches beyond the number of parts ({})", f, n, parts_length);
                }
            },
            _ => (),
        }
    }

    Ok(())
}