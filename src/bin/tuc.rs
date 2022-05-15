use anyhow::{bail, Result};
use std::fmt;
use std::io::Write;
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

#[derive(Debug)]
struct Opt {
    delimiter: String,
    eol: EOL,
    fields: RangeList,
    bytes: bool,
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

    let maybe_fields: Option<RangeList> = pargs.opt_value_from_str(["-f", "--fields"])?;
    let maybe_characters: Option<RangeList> = pargs.opt_value_from_str(["-c", "--characters"])?;
    let maybe_bytes: Option<RangeList> = pargs.opt_value_from_str(["-b", "--bytes"])?;

    let delimiter: String = (maybe_characters.is_some() || maybe_bytes.is_some())
        .then(String::new)
        .or_else(|| pargs.opt_value_from_str(["-d", "--delimiter"]).ok()?)
        .unwrap_or_else(|| String::from('\t'));

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
        bytes: maybe_bytes.is_some(),
        fields: maybe_fields
            .or(maybe_characters)
            .or(maybe_bytes)
            .or_else(|| RangeList::from_str("1:").ok())
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
            Side::Continue => write!(f, ""),
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
        if self.l == self.r {
            // note that Side::Continue, Side::Continue is not expected
            write!(f, "{}", self.l)
        } else {
            write!(f, "{}:{}", self.l, self.r)
        }
    }
}

impl FromStr for Range {
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

        Ok(Range::new(l, r))
    }
}

impl Range {
    pub fn new(l: Side, r: Side) -> Self {
        Range { l, r }
    }
}

impl Default for Range {
    fn default() -> Self {
        Range::new(Side::Some(1), Side::Some(1))
    }
}

fn complement_std_range(
    parts_length: usize,
    r: &std::ops::Range<usize>,
) -> Vec<std::ops::Range<usize>> {
    let mut output: Vec<std::ops::Range<usize>> = Vec::new();

    // full match => no match
    if parts_length == 1 || r.start == 0 && r.end == parts_length {
        return Vec::new();
    } else if r.start == 0 {
        // e.g. :3 with 3 fields is a full match => no match
        if r.end == parts_length {
            return Vec::new();
        } else {
            //e.g :3 with 5 fields => 4:5
            output.push(std::ops::Range {
                start: r.end,
                end: parts_length,
            });
        }
    } else if r.end == parts_length {
        // r.start == 0 already covered, 1-long already covered

        // e.g. 2: => 1:1
        output.push(std::ops::Range {
            start: 0,
            end: r.start,
        });
    } else {
        // we have room before and after start/end
        output.push(std::ops::Range {
            start: 0,
            end: r.start,
        });
        output.push(std::ops::Range {
            start: r.end,
            end: parts_length,
        });
    }

    output
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
    output: &mut String,
) {
    fields_as_ranges.iter().enumerate().for_each(|(i, r)| {
        if r.start == r.end {
            return;
        }

        if output.is_empty() && r.start > 0 {
            output.push_str(delimiter);
        }

        output.push_str(&line[r.start..r.end]);

        if (i < fields_as_ranges.len() - 1) || (r.end < line.len() - 1) {
            output.push_str(delimiter);
        }
    });
}

fn cut_str(
    line: &str,
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    fields_as_ranges: &mut Vec<std::ops::Range<usize>>,
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

    let mut fields_as_ranges = get_fields_as_ranges(fields_as_ranges, line, &opt.delimiter);

    if opt.compress_delimiter && opt.delimiter.as_str() != "" {
        compressed_line_buf.clear();
        compress_delimiter(fields_as_ranges, line, &opt.delimiter, compressed_line_buf);
        line = compressed_line_buf;
        fields_as_ranges.clear();
        fields_as_ranges = get_fields_as_ranges(fields_as_ranges, line, &opt.delimiter);
    }

    if opt.delimiter.as_str() == "" && fields_as_ranges.len() > 2 {
        // Unless the line is empty (which should have already been handled),
        // then the empty-string delimiter generated ranges alongside each
        // character, plus one at each boundary, e.g. _f_o_o_. We drop them.
        fields_as_ranges.pop();
        fields_as_ranges.drain(..1);
    }

    match fields_as_ranges.len() {
        1 if opt.only_delimited => stdout.write_all(b"")?,
        1 => {
            stdout.write_all(line.as_bytes())?;
            stdout.write_all(&[eol])?;
        }
        _ => {
            opt.fields.0.iter().try_for_each(|f| -> Result<()> {
                let r_array = [field_to_std_range(fields_as_ranges.len(), f)?];
                let mut r_iter = r_array.iter();
                let _complements;

                if opt.complement {
                    _complements = complement_std_range(fields_as_ranges.len(), &r_array[0]);
                    r_iter = _complements.iter();
                }

                for r in r_iter {
                    let idx_start = fields_as_ranges[r.start].start;
                    let idx_end = fields_as_ranges[r.end - 1].end;
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

    opt.fields.0.iter().try_for_each(|f| -> Result<()> {
        let r = field_to_std_range(data.len(), f)?;
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
    fields_as_ranges: &mut Vec<std::ops::Range<usize>>,
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
            fields_as_ranges,
            &mut compressed_line_buf,
            opt.eol as u8,
        )?;
        fields_as_ranges.clear();
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

    if opt.bytes {
        read_and_cut_bytes(&mut stdin, &mut stdout, opt)?;
    } else {
        let mut fields_as_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(100);
        read_and_cut_str(&mut stdin, &mut stdout, opt, &mut fields_as_ranges)?;
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
