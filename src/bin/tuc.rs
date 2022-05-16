use anyhow::{bail, Result};
use std::fmt;
use std::io::Write;
use std::ops::Range;
use std::str::FromStr;

const HELP: &str = concat!(
    "tuc ",
    env!("CARGO_PKG_VERSION"),
    "
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -p, --compress-delimiter      Collapse any sequence of delimiters
    -s, --only-delimited          Do not print lines not containing delimiters
    -V, --version                 Prints version information
    -z, --zero-terminated         line delimiter is NUL (\\0), not LF (\\n)
    -h, --help                    Prints this help and exit

OPTIONS:
    -b, --bytes <fields>          Same as --fields, but it cuts on bytes instead
                                  (doesn't require a delimiter)
    -d, --delimiter <delimiter>   Delimiter to use to cut the text into pieces
                                  [default: \\t]
    -f, --fields <fields>         Fields to keep, 1-indexed, comma separated.
                                  Use colon for inclusive ranges.
                                  e.g. 1:3 or 3,2 or 1: or 3,1:2 or -3 or -3:-2
                                  [default 1:]
    -c, --characters <fields>     Same as --fields, but it keeps characters instead
                                  (doesn't require a delimiter)
    -r, --replace-delimiter <s>   Replace the delimiter with the provided text
    -t, --trim <trim>             Trim the delimiter. Valid trim values are
                                  (l|L)eft, (r|R)ight, (b|B)oth
    -m, --complement              keep the opposite fields than the one selected

Notes:
    --trim and --compress-delimiter are applied before --fields
"
);

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum EOL {
    Zero = 0,
    Newline = 10,
}

#[derive(Debug, PartialEq)]
enum BoundsType {
    Bytes,
    Characters,
    Fields,
    Lines,
}

#[derive(Debug)]
struct Opt {
    delimiter: String,
    eol: EOL,
    bounds: UserBoundsList,
    bounds_type: BoundsType,
    only_delimited: bool,
    compress_delimiter: bool,
    replace_delimiter: Option<String>,
    trim: Option<Trim>,
    version: bool,
    complement: bool,
}

fn parse_args() -> Result<Opt, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let mut maybe_fields: Option<UserBoundsList> = pargs.opt_value_from_str(["-f", "--fields"])?;
    let maybe_characters: Option<UserBoundsList> =
        pargs.opt_value_from_str(["-c", "--characters"])?;
    let maybe_bytes: Option<UserBoundsList> = pargs.opt_value_from_str(["-b", "--bytes"])?;
    let maybe_lines: Option<UserBoundsList> = pargs.opt_value_from_str(["-l", "--lines"])?;

    let bounds_type = if maybe_fields.is_some() {
        BoundsType::Fields
    } else if maybe_bytes.is_some() {
        BoundsType::Bytes
    } else if maybe_characters.is_some() {
        BoundsType::Characters
    } else if maybe_lines.is_some() {
        BoundsType::Lines
    } else {
        maybe_fields = Some(UserBoundsList::from_str("1:").unwrap());
        BoundsType::Fields
    };

    let delimiter = match bounds_type {
        BoundsType::Fields => pargs
            .opt_value_from_str(["-d", "--delimiter"])?
            .unwrap_or_else(|| String::from('\t')),
        _ => String::new(),
    };

    let args = Opt {
        complement: pargs.contains(["-m", "--complement"]),
        only_delimited: pargs.contains(["-s", "--only-delimited"]),
        compress_delimiter: pargs.contains(["-p", "--compress-delimiter"]),
        version: pargs.contains(["-V", "--version"]),
        eol: if pargs.contains(["-z", "--zero-terminated"]) {
            EOL::Zero
        } else {
            EOL::Newline
        },
        delimiter,
        bounds_type,
        bounds: maybe_fields
            .or(maybe_characters)
            .or(maybe_bytes)
            .or(maybe_lines)
            .unwrap(),
        replace_delimiter: pargs.opt_value_from_str(["-r", "--replace-delimiter"])?,
        trim: pargs.opt_value_from_str(["-t", "--trim"])?,
    };

    let remaining = pargs.finish();

    if args.version {
        println!("tuc {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if !remaining.is_empty() {
        eprintln!("tuc: unexpected arguments {:?}", remaining);
        eprintln!("Try 'tuc --help' for more information.");
        std::process::exit(1);
    }

    Ok(args)
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
struct UserBoundsList(Vec<UserBounds>);

impl FromStr for UserBoundsList {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let k: Result<Vec<UserBounds>, _> = s.split(',').map(UserBounds::from_str).collect();
        Ok(UserBoundsList(k?))
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
            Side::Continue => write!(f, ""),
        }
    }
}

#[derive(Debug)]
struct UserBounds {
    l: Side,
    r: Side,
}

impl fmt::Display for UserBounds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.l, self.r) {
            (Side::Continue, Side::Continue) => write!(f, "1:-1"),
            (l, r) if l == r => write!(f, "{}", l),
            (l, r) => write!(f, "{}:{}", l, r),
        }
    }
}

impl FromStr for UserBounds {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Field format error: empty field".into());
        } else if s == ":" {
            return Err("Field format error, no numbers next to `:`".into());
        }

        let (l, r) = match s.find(':') {
            None => {
                let side = Side::from_str(s)?;
                (side, side)
            }
            Some(idx_colon) if idx_colon == 0 => {
                (Side::Continue, Side::from_str(&s[idx_colon + 1..])?)
            }
            Some(idx_colon) if idx_colon == s.len() - 1 => {
                (Side::from_str(&s[..idx_colon])?, Side::Continue)
            }
            Some(idx_colon) => (
                Side::from_str(&s[..idx_colon])?,
                Side::from_str(&s[idx_colon + 1..])?,
            ),
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

        Ok(UserBounds::new(l, r))
    }
}

impl UserBounds {
    pub fn new(l: Side, r: Side) -> Self {
        UserBounds { l, r }
    }
}

impl Default for UserBounds {
    fn default() -> Self {
        UserBounds::new(Side::Some(1), Side::Some(1))
    }
}

fn complement_std_range(parts_length: usize, r: &Range<usize>) -> Vec<Range<usize>> {
    match (r.start, r.end) {
        // full match => no match
        (0, end) if end == parts_length => Vec::new(),
        // match left side => match right side
        (0, right) => vec![right..parts_length],
        // match right side => match left side
        (left, end) if end == parts_length => vec![0..left],
        // match middle of string => match before and after
        (left, right) => vec![0..left, right..parts_length],
    }
}

fn bounds_to_std_range(parts_length: usize, bounds: &UserBounds) -> Result<Range<usize>> {
    let start: usize = match bounds.l {
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

    let end: usize = match bounds.r {
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

    Ok(Range { start, end })
}

/*
 * Split a string into parts and build a vector of ranges that match those parts.
 *
 * `buffer` - empty vector that will be filled with ranges
 * `line` - the string to split
 * `delimiter` - what to search to split the string
 */
fn build_ranges_vec(buffer: &mut Vec<Range<usize>>, line: &str, delimiter: &str) {
    let delimiter_length = delimiter.len();
    let mut next_part_start = 0;

    for mat in line.match_indices(&delimiter) {
        buffer.push(Range {
            start: next_part_start,
            end: mat.0,
        });
        next_part_start = mat.0 + delimiter_length;
    }

    buffer.push(Range {
        start: next_part_start,
        end: line.len(),
    });
}

fn compress_delimiter(
    bounds_as_ranges: &[Range<usize>],
    line: &str,
    delimiter: &str,
    output: &mut String,
) {
    bounds_as_ranges.iter().enumerate().for_each(|(i, r)| {
        if r.start == r.end {
            return;
        }

        if output.is_empty() && r.start > 0 {
            output.push_str(delimiter);
        }

        output.push_str(&line[r.start..r.end]);

        if (i < bounds_as_ranges.len() - 1) || (r.end < line.len() - 1) {
            output.push_str(delimiter);
        }
    });
}

fn cut_str(
    line: &str,
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    bounds_as_ranges: &mut Vec<Range<usize>>,
    compressed_line_buf: &mut String,
    eol: u8,
) -> Result<()> {
    let mut line: &str = match opt.trim {
        None => line,
        Some(Trim::Both) => line
            .trim_start_matches(&opt.delimiter)
            .trim_end_matches(&opt.delimiter),
        Some(Trim::Left) => line.trim_start_matches(&opt.delimiter),
        Some(Trim::Right) => line.trim_end_matches(&opt.delimiter),
    };

    if line.is_empty() {
        if !opt.only_delimited {
            stdout.write_all(&[eol])?;
        }
        return Ok(());
    }

    build_ranges_vec(bounds_as_ranges, line, &opt.delimiter);

    if opt.compress_delimiter && opt.bounds_type == BoundsType::Fields {
        compressed_line_buf.clear();
        compress_delimiter(bounds_as_ranges, line, &opt.delimiter, compressed_line_buf);
        line = compressed_line_buf;
        bounds_as_ranges.clear();
        build_ranges_vec(bounds_as_ranges, line, &opt.delimiter);
    }

    if opt.bounds_type == BoundsType::Characters && bounds_as_ranges.len() > 2 {
        // Unless the line is empty (which should have already been handled),
        // then the empty-string delimiter generated ranges alongside each
        // character, plus one at each boundary, e.g. _f_o_o_. We drop them.
        bounds_as_ranges.pop();
        bounds_as_ranges.drain(..1);
    }

    match bounds_as_ranges.len() {
        1 if opt.only_delimited => stdout.write_all(b"")?,
        1 => {
            stdout.write_all(line.as_bytes())?;
            stdout.write_all(&[eol])?;
        }
        _ => {
            opt.bounds.0.iter().try_for_each(|f| -> Result<()> {
                let r_array = [bounds_to_std_range(bounds_as_ranges.len(), f)?];
                let mut r_iter = r_array.iter();
                let _complements;

                if opt.complement {
                    _complements = complement_std_range(bounds_as_ranges.len(), &r_array[0]);
                    r_iter = _complements.iter();
                }

                for r in r_iter {
                    let idx_start = bounds_as_ranges[r.start].start;
                    let idx_end = bounds_as_ranges[r.end - 1].end;
                    let output = &line[idx_start..idx_end];

                    if let Some(replace_delimiter) = &opt.replace_delimiter {
                        stdout.write_all(
                            output.replace(&opt.delimiter, replace_delimiter).as_bytes(),
                        )?;
                    } else {
                        stdout.write_all(output.as_bytes())?;
                    }
                }

                Ok(())
            })?;

            stdout.write_all(&[eol])?;
        }
    }

    Ok(())
}

fn cut_bytes(
    data: &[u8],
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    opt.bounds.0.iter().try_for_each(|f| -> Result<()> {
        let r = bounds_to_std_range(data.len(), f)?;
        let output = &data[r.start..r.end];

        stdout.write_all(output)?;

        Ok(())
    })?;

    Ok(())
}

fn read_and_cut_str(
    stdin: &mut reuse_buffer_reader::BufReader<std::io::StdinLock>,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    opt: Opt,
    bounds_as_ranges: &mut Vec<Range<usize>>,
) -> Result<()> {
    let mut line_buf = String::with_capacity(1024);
    let mut compressed_line_buf = if opt.compress_delimiter {
        String::with_capacity(line_buf.capacity())
    } else {
        String::new()
    };

    while let Some(line) = stdin.read_line_with_eol(&mut line_buf, opt.eol) {
        let line = line?;
        let line: &str = line.as_ref();
        let line = line.strip_suffix(opt.eol as u8 as char).unwrap_or(line);
        cut_str(
            line,
            &opt,
            stdout,
            bounds_as_ranges,
            &mut compressed_line_buf,
            opt.eol as u8,
        )?;
        bounds_as_ranges.clear();
    }
    Ok(())
}

fn read_and_cut_bytes(
    stdin: &mut reuse_buffer_reader::BufReader<std::io::StdinLock>,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    opt: Opt,
) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(32 * 1024);
    stdin.read_bytes_to_end(&mut buffer);
    cut_bytes(&buffer, &opt, stdout)?;
    Ok(())
}

fn main() -> Result<()> {
    let opt: Opt = parse_args()?;

    let stdin = std::io::stdin();
    let mut stdin = reuse_buffer_reader::BufReader::with_capacity(32 * 1024, stdin.lock());

    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::with_capacity(32 * 1024, stdout.lock());

    if opt.bounds_type == BoundsType::Bytes {
        read_and_cut_bytes(&mut stdin, &mut stdout, opt)?;
    } else {
        let mut bounds_as_ranges: Vec<Range<usize>> = Vec::with_capacity(100);
        read_and_cut_str(&mut stdin, &mut stdout, opt, &mut bounds_as_ranges)?;
    }

    stdout.flush()?;

    Ok(())
}

mod reuse_buffer_reader {
    pub use super::EOL;
    use std::io::{self, prelude::*};

    pub struct BufReader<R> {
        reader: io::BufReader<R>,
    }

    impl<R: Read> BufReader<R> {
        pub fn with_capacity(capacity: usize, inner: R) -> BufReader<R> {
            let reader = io::BufReader::with_capacity(capacity, inner);

            Self { reader }
        }

        pub fn read_bytes_to_end<'buf>(
            &mut self,
            buffer: &'buf mut Vec<u8>,
        ) -> Option<io::Result<&'buf mut Vec<u8>>> {
            buffer.clear();

            self.reader
                .read_to_end(buffer)
                .map(|u| if u == 0 { None } else { Some(buffer) })
                .transpose()
        }

        pub fn read_line_with_eol<'buf>(
            &mut self,
            buffer: &'buf mut String,
            eol: EOL,
        ) -> Option<io::Result<&'buf mut String>> {
            buffer.clear();

            match eol {
                // read_line is more optimized/safe than read_until for strings
                EOL::Newline => self.reader.read_line(buffer),
                EOL::Zero => unsafe { self.reader.read_until(eol as u8, buffer.as_mut_vec()) },
            }
            .map(|u| if u == 0 { None } else { Some(buffer) })
            .transpose()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complement_std_range() {
        // remember, it assumes that ranges are "legit" (not out of bounds)

        // test empty string
        assert_eq!(complement_std_range(0, &(0..0)), vec![]);

        // test 1-long string
        assert_eq!(complement_std_range(1, &(0..1)), vec![]);

        // test ranges that reach left or right bounds
        assert_eq!(complement_std_range(5, &(0..5)), vec![]);
        assert_eq!(complement_std_range(5, &(0..3)), vec![3..5]);
        assert_eq!(complement_std_range(5, &(3..5)), vec![0..3]);

        // test internal range
        assert_eq!(complement_std_range(5, &(1..3)), vec![0..1, 3..5]);

        // test 2-long string
        assert_eq!(complement_std_range(2, &(0..2)), vec![]);
        assert_eq!(complement_std_range(2, &(0..1)), vec![1..2]);
        assert_eq!(complement_std_range(2, &(1..2)), vec![0..1]);
    }

    #[test]
    fn test_user_bounds_formatting() {
        assert_eq!(
            UserBounds::new(Side::Continue, Side::Continue).to_string(),
            "1:-1"
        );
        assert_eq!(
            UserBounds::new(Side::Continue, Side::Some(3)).to_string(),
            ":3"
        );
        assert_eq!(
            UserBounds::new(Side::Some(3), Side::Continue).to_string(),
            "3:"
        );
        assert_eq!(
            UserBounds::new(Side::Some(1), Side::Some(2)).to_string(),
            "1:2"
        );
        assert_eq!(
            UserBounds::new(Side::Some(-1), Side::Some(-2)).to_string(),
            "-1:-2"
        );
    }
}
