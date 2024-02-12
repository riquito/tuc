use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList};
use crate::options::{Opt, Trim, EOL};
use anyhow::{bail, Result};
use bstr::ByteSlice;
use std::convert::TryFrom;
use std::io::{self, BufRead};
use std::ops::Deref;
use std::str::FromStr;
use std::{io::Write, ops::Range};

use bstr::io::BufReadExt;

fn trim<'a>(buffer: &'a [u8], trim_kind: &Trim, delimiter: u8) -> &'a [u8] {
    match trim_kind {
        Trim::Both => buffer
            .trim_start_with(|x| x == delimiter as char)
            .trim_end_with(|x| x == delimiter as char),
        Trim::Left => buffer.trim_start_with(|x| x == delimiter as char),
        Trim::Right => buffer.trim_end_with(|x| x == delimiter as char),
    }
}

fn cut_str_fast_lane<W: Write>(
    initial_buffer: &[u8],
    opt: &FastOpt,
    stdout: &mut W,
    fields: &mut Vec<Range<usize>>,
    last_interesting_field: Side,
) -> Result<()> {
    let mut buffer = initial_buffer;

    if opt.trim.is_some() {
        buffer = trim(buffer, opt.trim.as_ref().unwrap(), opt.delimiter)
    }

    if buffer.is_empty() {
        if !opt.only_delimited {
            stdout.write_all(&[opt.eol.into()])?;
        }
        return Ok(());
    }

    let bounds = &opt.bounds;

    let mut prev_field_start = 0;

    let mut curr_field = 0;

    fields.clear();

    for i in memchr::memchr_iter(opt.delimiter, buffer) {
        curr_field += 1;

        let (start, end) = (prev_field_start, i); // end exclusive
        prev_field_start = i + 1;

        fields.push(Range { start, end });

        if Side::Some(curr_field) == last_interesting_field {
            // We have no use for any other fields in this line
            break;
        }
    }

    if curr_field == 0 && opt.only_delimited {
        // The delimiter was not found
        return Ok(());
    }

    // After the last loop ended, everything remaining is the field
    // after the last delimiter (we want it), or "useless" fields after the
    // last one that the user is interested in (and we can ignore them).
    if Side::Some(curr_field) != last_interesting_field {
        fields.push(Range {
            start: prev_field_start,
            end: buffer.len(),
        });
    }

    let num_fields = fields.len();

    match num_fields {
        1 if bounds.len() == 1 && fields[0].end == buffer.len() => {
            stdout.write_all(buffer)?;
        }
        _ => {
            bounds
                .iter()
                .enumerate()
                .try_for_each(|(bounds_idx, bof)| -> Result<()> {
                    let b = match bof {
                        BoundOrFiller::Filler(f) => {
                            stdout.write_all(f.as_bytes())?;
                            return Ok(());
                        }
                        BoundOrFiller::Bound(b) => b,
                    };

                    let is_last = bounds_idx == bounds.len() - 1;

                    output_parts(buffer, b, fields, stdout, is_last, opt)
                })?;
        }
    }

    stdout.write_all(&[opt.eol.into()])?;

    Ok(())
}

#[inline(always)]
fn output_parts<W: Write>(
    line: &[u8],
    // which parts to print
    b: &UserBounds,
    // where to find the parts inside `line`
    fields: &[Range<usize>],
    stdout: &mut W,
    is_last: bool,
    opt: &FastOpt,
) -> Result<()> {
    let r = b.try_into_range(fields.len())?;

    let idx_start = fields[r.start].start;
    let idx_end = fields[r.end - 1].end;
    let output = &line[idx_start..idx_end];

    let field_to_print = output;
    stdout.write_all(field_to_print)?;

    if opt.join && !(is_last) {
        stdout.write_all(&[opt.delimiter])?;
    }

    Ok(())
}

#[derive(Debug)]
pub struct FastOpt {
    delimiter: u8,
    join: bool,
    eol: EOL,
    bounds: ForwardBounds,
    only_delimited: bool,
    trim: Option<Trim>,
}

impl Default for FastOpt {
    fn default() -> Self {
        Self {
            delimiter: b'\t',
            join: false,
            eol: EOL::Newline,
            bounds: ForwardBounds::try_from(&UserBoundsList::from_str("1:").unwrap()).unwrap(),
            only_delimited: false,
            trim: None,
        }
    }
}

impl TryFrom<&Opt> for FastOpt {
    type Error = &'static str;

    fn try_from(value: &Opt) -> Result<Self, Self::Error> {
        if !value.delimiter.as_bytes().len() == 1 {
            return Err("Delimiter must be 1 byte wide for FastOpt");
        }

        if value.complement
            || value.greedy_delimiter
            || value.compress_delimiter
            || value.json
            || value.bounds_type != BoundsType::Fields
            || value.replace_delimiter.is_some()
            || value.regex_bag.is_some()
        {
            return Err(
                "FastOpt supports solely forward fields, join and single-character delimiters",
            );
        }

        if let Ok(forward_bounds) = ForwardBounds::try_from(&value.bounds) {
            Ok(FastOpt {
                delimiter: value.delimiter.as_bytes().first().unwrap().to_owned(),
                join: value.join,
                eol: value.eol,
                bounds: forward_bounds,
                only_delimited: value.only_delimited,
                trim: value.trim,
            })
        } else {
            Err("Bounds cannot be converted to ForwardBounds")
        }
    }
}

#[derive(Debug)]
struct ForwardBounds {
    pub list: UserBoundsList,
    // Optimization that we can use to stop searching for fields
    // It's available only when every bound use positive indexes.
    // When conditions do not apply, Side::Continue is used.
    last_interesting_field: Side,
}

impl TryFrom<&UserBoundsList> for ForwardBounds {
    type Error = anyhow::Error;

    fn try_from(value: &UserBoundsList) -> Result<Self, Self::Error> {
        if value.is_empty() {
            bail!("Cannot create ForwardBounds from an empty UserBoundsList");
        } else {
            let value: UserBoundsList = UserBoundsList(value.iter().cloned().collect());

            let mut rightmost_bound: Option<Side> = None;
            if value.is_sortable() {
                value.iter().for_each(|bof| {
                    if let BoundOrFiller::Bound(b) = bof {
                        if rightmost_bound.is_none() || b.r > rightmost_bound.unwrap() {
                            rightmost_bound = Some(b.r);
                        }
                    }
                });
            }

            Ok(ForwardBounds {
                list: value,
                last_interesting_field: rightmost_bound.unwrap_or(Side::Continue),
            })
        }
    }
}

impl Deref for ForwardBounds {
    type Target = UserBoundsList;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl ForwardBounds {
    fn get_last_bound(&self) -> Side {
        self.last_interesting_field
    }
}

impl FromStr for ForwardBounds {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bounds_list = UserBoundsList::from_str(s)?;
        ForwardBounds::try_from(&bounds_list)
    }
}

pub fn read_and_cut_text_as_bytes<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &FastOpt,
) -> Result<()> {
    let mut fields: Vec<Range<usize>> = Vec::with_capacity(16);

    let last_interesting_field = opt.bounds.get_last_bound();

    match opt.eol {
        EOL::Newline => stdin.for_byte_line(|line| {
            cut_str_fast_lane(line, opt, stdout, &mut fields, last_interesting_field)
                // XXX Should map properly the error
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x.to_string()))
                .and(Ok(true))
        })?,
        EOL::Zero => stdin.for_byte_record(opt.eol.into(), |line| {
            cut_str_fast_lane(line, opt, stdout, &mut fields, last_interesting_field)
                // XXX Should map properly the error
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x.to_string()))
                .and(Ok(true))
        })?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::options::Trim;

    use super::*;

    fn make_fields_opt() -> FastOpt {
        FastOpt {
            delimiter: b'-',
            ..FastOpt::default()
        }
    }

    #[test]
    fn test_read_and_cut_str_echo_non_delimited_strings() {
        // read_and_cut_str is difficult to test, let's verify at least
        // that it reads the input and appears to call cut_str

        let opt = make_fields_opt();
        let mut input = b"foo".as_slice();
        let mut output = Vec::new();
        read_and_cut_text_as_bytes(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"foo\n".as_slice());
    }

    fn make_cut_str_buffers() -> (Vec<u8>, Vec<Range<usize>>) {
        let output = Vec::new();
        let fields = Vec::new();
        (output, fields)
    }

    #[test]
    fn cut_str_echo_non_delimited_strings() {
        let opt = make_fields_opt();

        // non-empty line missing the delimiter
        let line = b"foo";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"foo\n".as_slice());

        // empty line
        let line = b"";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"\n".as_slice());
    }

    #[test]
    fn cut_str_skip_non_delimited_strings_when_requested() {
        let mut opt = make_fields_opt();

        opt.only_delimited = true;

        // non-empty line missing the delimiter
        let line = b"foo";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"".as_slice());

        // empty line
        let line = b"";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"".as_slice());
    }

    #[test]
    fn cut_str_it_cut_a_field() {
        let mut opt = make_fields_opt();
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = ForwardBounds::from_str("1").unwrap();

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"a\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_with_negative_indices() {
        let mut opt = make_fields_opt();

        let line = b"a-b-c";

        // just one negative index
        opt.bounds = ForwardBounds::from_str("-1").unwrap();
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"c\n".as_slice());

        // multiple negative indices, in forward order
        opt.bounds = ForwardBounds::from_str("-2,-1").unwrap();
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"bc\n".as_slice());

        // multiple negative indices, in non-forward order
        opt.bounds = ForwardBounds::from_str("-1,-2").unwrap();
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"cb\n".as_slice());

        // mix positive and negative indices
        // (this is particularly useful to verify that we don't screw
        // up optimizations on last field to check)
        opt.bounds = ForwardBounds::from_str("-1,1").unwrap();
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"ca\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_consecutive_delimiters() {
        let mut opt = make_fields_opt();
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = ForwardBounds::from_str("1,3").unwrap();

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_supports_zero_terminated_lines() {
        let mut opt = make_fields_opt();
        let (mut output, mut fields) = make_cut_str_buffers();
        opt.eol = EOL::Zero;

        let line = b"a-b-c";
        opt.bounds = ForwardBounds::from_str("2").unwrap();

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"b\0".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields() {
        let mut opt = make_fields_opt();
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = ForwardBounds::from_str("1,3").unwrap();
        opt.join = true;

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"a-c\n".as_slice());
    }

    #[test]
    fn cut_str_it_format_fields() {
        let mut opt = make_fields_opt();
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = ForwardBounds::from_str("{1} < {3} > {2}").unwrap();

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"a < c > b\n".as_slice());
    }

    #[test]
    fn cut_str_it_trim_fields() {
        let mut opt = make_fields_opt();
        let line = b"--a--b--c--";

        // check Trim::Both
        opt.trim = Some(Trim::Both);
        opt.bounds = ForwardBounds::from_str("1,3,-1").unwrap();

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        opt.trim = Some(Trim::Left);
        opt.bounds = ForwardBounds::from_str("1,3,-3").unwrap();

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        opt.trim = Some(Trim::Right);
        opt.bounds = ForwardBounds::from_str("3,5,-1").unwrap();

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.get_last_bound(),
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }
}
