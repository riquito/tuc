use anyhow::{bail, Result};
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

fn field_to_std_range(parts_length: usize, f: &Range) -> Result<std::ops::Range<usize>> {
    let start: usize = match f.l {
        Side::Continue => 0,
        Side::Some(v) => {
            if v.abs() as usize > parts_length {
                bail!("Out of bounds: {}", v);
            }
            if v < 0 {
                parts_length - v.abs() as usize
            } else {
                v as usize - 1
            }
        }
    };

    let end: usize = match f.r {
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

    Ok(std::ops::Range { start, end })
}

/*
 * Build a vector of ranges (start/end) for each field.
 *
 * The vector is expected to be empty (we reuse an existing vector
 * for performance reasons).
 */
fn get_fields_as_ranges<'a>(
    fields_as_ranges: &'a mut Vec<std::ops::Range<usize>>,
    line: &str,
    delimiter: &str,
) -> &'a mut Vec<std::ops::Range<usize>> {
    let delimiter_lenth = delimiter.len();
    let mut next_part_start = 0;

    for mat in line.match_indices(&delimiter) {
        fields_as_ranges.push(std::ops::Range {
            start: next_part_start,
            end: mat.0,
        });
        next_part_start = mat.0 + delimiter_lenth;
    }

    fields_as_ranges.push(std::ops::Range {
        start: next_part_start,
        end: line.len(),
    });

    fields_as_ranges
}

fn compress_delimiter(
    fields_as_ranges: &[std::ops::Range<usize>],
    line: &str,
    delimiter: &str,
) -> String {
    fields_as_ranges
        .iter()
        .map(|r| &line[r.start..r.end])
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(&delimiter)
}

fn cut(
    line: &str,
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    fields_as_ranges: &mut Vec<std::ops::Range<usize>>,
) -> Result<()> {
    let mut line: &str = match opt.trim {
        None => &line,
        Some(Trim::Both) => line
            .trim_start_matches(&opt.delimiter)
            .trim_end_matches(&opt.delimiter),
        Some(Trim::Left) => line.trim_start_matches(&opt.delimiter),
        Some(Trim::Right) => line.trim_end_matches(&opt.delimiter),
    };

    let mut fields_as_ranges = get_fields_as_ranges(fields_as_ranges, &line, &opt.delimiter);
    let compressed_line: String;

    if opt.compress_delimiter {
        compressed_line = compress_delimiter(fields_as_ranges, &line, &opt.delimiter);
        line = &compressed_line;
        fields_as_ranges.clear();
        fields_as_ranges = get_fields_as_ranges(fields_as_ranges, &line, &opt.delimiter);
    }

    if fields_as_ranges.len() == 1 {
        if !opt.only_delimited {
            stdout.write_all(line.as_bytes())?;
        }
        stdout.write_all(b"\n")?;
        return Ok(());
    }

    opt.fields.0.iter().try_for_each(|f| -> Result<()> {
        let r = field_to_std_range(fields_as_ranges.len(), f)?;
        let idx_start = fields_as_ranges[r.start].start;
        let idx_end = fields_as_ranges[r.end - 1].end;
        let output = &line[idx_start..idx_end];

        if let Some(replace_delimiter) = &opt.replace_delimiter {
            stdout.write_all(
                output
                    .replace(&opt.delimiter, &replace_delimiter)
                    .as_bytes(),
            )?;
        } else {
            stdout.write_all(output.as_bytes())?;
        }

        Ok(())
    })?;

    stdout.write_all(b"\n")?;

    Ok(())
}

fn main() -> Result<()> {
    let matches = Opt::clap()
        .setting(structopt::clap::AppSettings::AllowLeadingHyphen)
        .get_matches();

    let opt = Opt::from_clap(&matches);

    let stdin = std::io::stdin();
    let stdin = std::io::BufReader::with_capacity(32 * 1024, stdin.lock());

    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::with_capacity(32 * 1024, stdout.lock());

    let mut fields_as_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(100);

    stdin
        .lines()
        .try_for_each::<_, Result<()>>(|maybe_line| -> Result<()> {
            let line = maybe_line?;
            cut(&line, &opt, &mut stdout, &mut fields_as_ranges)?;
            fields_as_ranges.clear();
            Ok(())
        })?;

    stdout.flush()?;

    Ok(())
}
