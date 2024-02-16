use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList, UserBoundsTrait};
use crate::options::{Opt, Trim, EOL};
use anyhow::Result;
use bstr::ByteSlice;
use std::convert::TryFrom;
use std::io::Write;
use std::io::{self, BufRead};

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

#[inline(always)]
fn cut_str_fast_lane<W: Write>(
    initial_buffer: &[u8],
    opt: &FastOpt,
    stdout: &mut W,
    fields: &mut Vec<usize>,
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

    let mut curr_field = 0;

    fields.clear();

    for i in memchr::memchr_iter(opt.delimiter, buffer) {
        curr_field += 1;

        fields.push(i);

        if Side::Some(curr_field) == last_interesting_field {
            // We have no use for any other fields in this line
            break;
        }
    }

    if curr_field == 0 && opt.only_delimited {
        // The delimiter was not found
        return Ok(());
    }

    if Side::Some(curr_field) != last_interesting_field {
        // We reached the end of the line. Who knows, maybe
        // the user is interested in this field too.
        fields.push(buffer.len());
    }

    let num_fields = fields.len();

    match num_fields {
        1 if bounds.len() == 1 && fields[0] == buffer.len() => {
            stdout.write_all(buffer)?;
        }
        _ => {
            bounds.iter().try_for_each(|bof| -> Result<()> {
                match bof {
                    BoundOrFiller::Filler(f) => {
                        stdout.write_all(f.as_bytes())?;
                    }
                    BoundOrFiller::Bound(b) => {
                        output_parts(buffer, b, fields, stdout, opt)?;
                    }
                };
                Ok(())
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
    fields: &[usize],
    stdout: &mut W,
    opt: &FastOpt,
) -> Result<()> {
    let r = b.try_into_range(fields.len())?;

    let idx_start = if r.start == 0 {
        0
    } else {
        fields[r.start - 1] + 1
    };
    let idx_end = fields[r.end - 1];

    let output = &line[idx_start..idx_end];

    let field_to_print = output;
    stdout.write_all(field_to_print)?;

    if opt.join && !b.is_last {
        stdout.write_all(&[opt.delimiter])?;
    }

    Ok(())
}

#[derive(Debug)]
pub struct FastOpt<'a> {
    delimiter: u8,
    join: bool,
    eol: EOL,
    bounds: &'a UserBoundsList,
    only_delimited: bool,
    trim: Option<Trim>,
}

impl<'a> TryFrom<&'a Opt> for FastOpt<'a> {
    type Error = &'static str;

    fn try_from(value: &'a Opt) -> Result<Self, Self::Error> {
        if value.delimiter.as_bytes().len() != 1 {
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

        let delimiter = value.delimiter.as_bytes().first().unwrap().to_owned();
        Ok(FastOpt {
            delimiter,
            join: value.join,
            eol: value.eol,
            bounds: &value.bounds,
            only_delimited: value.only_delimited,
            trim: value.trim,
        })
    }
}

pub fn read_and_cut_text_as_bytes<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &FastOpt,
) -> Result<()> {
    let mut fields: Vec<usize> = Vec::with_capacity(16);

    let last_interesting_field = opt.bounds.last_interesting_field;

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

    fn make_fields_opt(bounds_as_text: &str) -> FastOpt<'static> {
        let boxed_bounds = Box::new(UserBoundsList::from_str(bounds_as_text).unwrap());
        let bounds: &'static mut UserBoundsList = Box::leak(boxed_bounds);

        FastOpt {
            delimiter: b'-',
            join: false,
            eol: EOL::Newline,
            bounds,
            only_delimited: false,
            trim: None,
        }
    }

    #[test]
    fn test_read_and_cut_str_echo_non_delimited_strings() {
        // read_and_cut_str is difficult to test, let's verify at least
        // that it reads the input and appears to call cut_str

        let opt = make_fields_opt("1:");
        let mut input = b"foo".as_slice();
        let mut output = Vec::new();
        read_and_cut_text_as_bytes(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"foo\n".as_slice());
    }

    #[test]
    fn fail_to_convert_opt_with_long_delimiter_to_fastopt() {
        let opt = Opt {
            delimiter: "foo".to_owned(),
            ..Default::default()
        };

        assert!(FastOpt::try_from(&opt).is_err());
        assert_eq!(
            FastOpt::try_from(&opt).unwrap_err(),
            "Delimiter must be 1 byte wide for FastOpt"
        );
    }

    fn make_cut_str_buffers() -> (Vec<u8>, Vec<usize>) {
        let output = Vec::new();
        let fields = Vec::new();
        (output, fields)
    }

    #[test]
    fn cut_str_echo_non_delimited_strings() {
        let opt = make_fields_opt("1:");

        // non-empty line missing the delimiter
        let line = b"foo";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
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
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"\n".as_slice());
    }

    #[test]
    fn cut_str_skip_non_delimited_strings_when_requested() {
        let mut opt = make_fields_opt("1:");

        opt.only_delimited = true;

        // non-empty line missing the delimiter
        let line = b"foo";
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
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
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"".as_slice());
    }

    #[test]
    fn cut_str_it_cut_a_field() {
        let opt = make_fields_opt("1");
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"a\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_with_negative_indices() {
        // just one negative index
        let opt = make_fields_opt("-1");

        let line = b"a-b-c";

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"c\n".as_slice());

        // multiple negative indices, in forward order
        let opt = make_fields_opt("-2,-1");
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"bc\n".as_slice());

        // multiple negative indices, in non-forward order
        let opt = make_fields_opt("-1,-2");
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"cb\n".as_slice());

        // mix positive and negative indices
        // (this is particularly useful to verify that we don't screw
        // up optimizations on last field to check)
        let opt = make_fields_opt("-1,1");
        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"ca\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_consecutive_delimiters() {
        let opt = make_fields_opt("1,3");
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_supports_zero_terminated_lines() {
        let mut opt = make_fields_opt("2");
        let (mut output, mut fields) = make_cut_str_buffers();
        opt.eol = EOL::Zero;

        let line = b"a-b-c";

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"b\0".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields() {
        let mut opt = make_fields_opt("1,3");
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.join = true;

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"a-c\n".as_slice());
    }

    #[test]
    fn cut_str_it_format_fields() {
        let opt = make_fields_opt("{2}");
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output.to_str_lossy(), b"b\n".as_slice().to_str_lossy());

        let opt = make_fields_opt("{1} < {3} > {2}");
        let (mut output, mut fields) = make_cut_str_buffers();

        let line = b"a-b-c";

        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(
            output.to_str_lossy(),
            b"a < c > b\n".as_slice().to_str_lossy()
        );
    }

    #[test]
    fn cut_str_it_trim_fields() {
        let mut opt = make_fields_opt("1,3,-1");
        let line = b"--a--b--c--";

        // check Trim::Both
        opt.trim = Some(Trim::Both);

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        let mut opt = make_fields_opt("1,3,-3");
        opt.trim = Some(Trim::Left);

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        let mut opt = make_fields_opt("3,5,-1");
        opt.trim = Some(Trim::Right);

        let (mut output, mut fields) = make_cut_str_buffers();
        cut_str_fast_lane(
            line,
            &opt,
            &mut output,
            &mut fields,
            opt.bounds.last_interesting_field,
        )
        .unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }
}
