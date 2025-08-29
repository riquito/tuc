use anyhow::Result;
use bstr::ByteSlice;
use bstr::io::BufReadExt;
use std::io::{BufRead, Write};

use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBoundsList};
use crate::finders::common::DelimiterFinder;
use crate::options::{EOL, Opt, Trim};
use crate::plan::FieldPlan;

#[cfg(feature = "regex")]
use regex::bytes::Regex;

fn compress_delimiter(line: &[u8], delimiter: &[u8], output: &mut Vec<u8>) {
    output.clear();
    let mut prev_idx = 0;

    for idx in line.find_iter(delimiter) {
        let prev_part = &line[prev_idx..idx];

        if idx == 0 {
            output.extend(delimiter);
        } else if !prev_part.is_empty() {
            output.extend(prev_part);
            output.extend(delimiter);
        }

        prev_idx = idx + delimiter.len();
    }

    if prev_idx < line.len() {
        output.extend(&line[prev_idx..]);
    }
}

#[cfg(feature = "regex")]
fn compress_delimiter_with_regex<'a>(
    line: &'a [u8],
    re: &Regex,
    new_delimiter: &[u8],
) -> std::borrow::Cow<'a, [u8]> {
    re.replace_all(line, new_delimiter)
}

fn trim<'a>(buffer: &'a [u8], trim_kind: &Trim, delimiter: &[u8]) -> &'a [u8] {
    match trim_kind {
        Trim::Both => {
            let mut idx = 0;
            let mut r_idx = buffer.len();

            while buffer[idx..].starts_with(delimiter) {
                idx += delimiter.len();
            }

            while buffer[idx..r_idx].ends_with(delimiter) {
                r_idx -= delimiter.len();
            }

            &buffer[idx..r_idx]
        }
        Trim::Left => {
            let mut idx = 0;

            while buffer[idx..].starts_with(delimiter) {
                idx += delimiter.len();
            }

            &buffer[idx..]
        }
        Trim::Right => {
            let mut r_idx = buffer.len();

            while buffer[..r_idx].ends_with(delimiter) {
                r_idx -= delimiter.len();
            }

            &buffer[..r_idx]
        }
    }
}

#[cfg(feature = "regex")]
fn trim_regex<'a>(line: &'a [u8], trim_kind: &Trim, re: &Regex) -> &'a [u8] {
    let mut iter = re.find_iter(line);
    let mut idx_start = 0;
    let mut idx_end = line.len();

    if (trim_kind == &Trim::Both || trim_kind == &Trim::Left)
        && let Some(m) = iter.next()
        && m.start() == 0
    {
        idx_start = m.end();
    }

    if (trim_kind == &Trim::Both || trim_kind == &Trim::Right)
        && let Some(m) = iter.last()
        && m.end() == line.len()
    {
        idx_end = m.start();
    }

    &line[idx_start..idx_end]
}

macro_rules! write_maybe_as_json {
    ($writer:ident, $to_print:ident, $as_json:expr) => {{
        if $as_json {
            let x;
            $writer.write_all(unsafe {
                // Safe as long as we were not requested to cut in the middle of a codepoint
                // (and then we're pretty much doing what was asked)
                x = serde_json::to_string(std::str::from_utf8_unchecked(&$to_print))?;
                x.as_bytes()
            })?;
        } else {
            $writer.write_all(&$to_print)?;
        }
    }};
}

pub fn cut_str<W: Write, F, R>(
    line: &[u8],
    opt: &Opt,
    stdout: &mut W,
    compressed_line_buf: &mut Vec<u8>,
    eol: &[u8],
    plan: &mut FieldPlan<F, R>,
) -> Result<()>
where
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    let mut line = line;

    if let Some(trim_kind) = opt.trim {
        if opt.regex_bag.is_some() {
            #[cfg(feature = "regex")]
            {
                line = trim_regex(line, &trim_kind, &opt.regex_bag.as_ref().unwrap().greedy);
            }
        } else {
            line = trim(line, &trim_kind, &opt.delimiter);
        }
    }

    if line.is_empty() {
        if !opt.only_delimited {
            stdout.write_all(eol)?;
        }
        return Ok(());
    }

    #[cfg(feature = "regex")]
    let line_holder: std::borrow::Cow<[u8]>;

    if opt.compress_delimiter {
        if opt.regex_bag.is_some() && cfg!(feature = "regex") {
            #[cfg(feature = "regex")]
            {
                let delimiter = opt.replace_delimiter.as_ref().unwrap(); // we checked earlier the invariant
                line_holder = compress_delimiter_with_regex(
                    line,
                    &opt.regex_bag.as_ref().unwrap().greedy,
                    delimiter,
                );
                line = &line_holder;
            }
        } else {
            compress_delimiter(line, &opt.delimiter, compressed_line_buf);
            line = compressed_line_buf;
        }
    }

    let maybe_maybe_num_fields = (plan.extract_func)(line, plan);
    let maybe_num_fields = maybe_maybe_num_fields.unwrap_or(None);

    if opt.only_delimited
        && maybe_num_fields
            .expect("We didn't use an extract function that counted the number of fields")
            == 1
    {
        // If there's only 1 field it means that there were no delimiters
        // and when used alogside `only_delimited` we must skip the line
        return Ok(());
    }

    if opt.json {
        stdout.write_all(b"[")?;
    }

    let mut _bounds: UserBoundsList;
    let mut bounds = &opt.bounds;

    if opt.complement {
        _bounds =
            bounds
                .complement(maybe_num_fields.expect(
                    "We didn't use an extract function that counted the number of fields",
                ))?;
        bounds = &_bounds;

        if bounds.is_empty() {
            // If the original bounds matched all the fields, the complement is empty
            if !opt.only_delimited {
                stdout.write_all(eol)?;
            }
            return Ok(());
        }
    }

    if opt.unpack {
        // Unpack bounds such as 1:3 or 2: into single-field bounds
        // such as 1:1,2:2,3:3 etc...

        // Start by checking if we actually need to rewrite the bounds
        // (are there ranges in the first place?), since it's an
        // expensive operation.
        if bounds.iter().any(|bof| match bof {
            BoundOrFiller::Bound(b) => b.l() != b.r() || *b.l() == Side::Continue,
            BoundOrFiller::Filler(_) => false,
        }) {
            _bounds = bounds.unpack(
                maybe_num_fields
                    .expect("We didn't use an extract function that counted the number of fields"),
            );
            bounds = &_bounds;
        }
    }

    bounds.iter().try_for_each(|bof| -> Result<()> {
        let b = match bof {
            BoundOrFiller::Filler(f) => {
                stdout.write_all(f.as_bytes())?;
                return Ok(());
            }
            BoundOrFiller::Bound(b) => b,
        };

        let field = plan.get_field(b, line.len());
        let output = if let Ok(field) = field {
            &line[field.start..field.end]
        } else if b.fallback_oob().is_some() {
            b.fallback_oob().as_ref().unwrap()
        } else if let Some(generic_fallback) = &opt.fallback_oob {
            generic_fallback
        } else {
            return Err(field.unwrap_err());
        };

        let mut field_to_print = output;
        let output_with_delimiter_replaced;

        if let Some(replace_func) = opt.replace_delimiter_fn {
            output_with_delimiter_replaced = replace_func(output, opt);
            field_to_print = &output_with_delimiter_replaced;
        }

        write_maybe_as_json!(stdout, field_to_print, opt.json);

        if opt.join && !b.is_last() {
            stdout.write_all(
                opt.replace_delimiter
                    .as_ref()
                    .unwrap_or(&opt.delimiter)
                    .as_bytes(),
            )?;
        }

        Ok(())
    })?;

    if opt.json {
        stdout.write_all(b"]")?;
    }

    stdout.write_all(eol)?;

    Ok(())
}

pub fn read_and_cut_str<B: BufRead, W: Write>(
    stdin: &mut B,
    stdout: &mut W,
    opt: &Opt,
) -> Result<()> {
    let line_buf: Vec<u8> = Vec::with_capacity(1024);
    let mut compressed_line_buf = if opt.compress_delimiter {
        Vec::with_capacity(line_buf.capacity())
    } else {
        Vec::new()
    };

    // Determine which plan type to use based on options
    let should_compress_delimiter = opt.compress_delimiter
        && (opt.bounds_type == BoundsType::Fields || opt.bounds_type == BoundsType::Lines);

    #[cfg(feature = "regex")]
    let maybe_regex = opt.regex_bag.as_ref().map(|x| {
        if opt.greedy_delimiter {
            &x.greedy
        } else {
            &x.normal
        }
    });
    #[cfg(not(feature = "regex"))]
    let maybe_regex: Option<()> = None;

    if should_compress_delimiter && maybe_regex.is_some() && opt.replace_delimiter.is_some() {
        // Special case: compressed delimiter + regex + delimiter replacement.
        // We setup now the search plan, taking into account that when we start searching
        // for the delimiter it will have been already replaced (so we won't use
        // the regex to search for the original delimiter, we will do a fixed-string search
        // for the new delimiter).
        let replace_delimiter = opt.replace_delimiter.as_ref().unwrap();
        let mut plan = FieldPlan::from_opt_fixed_with_custom_delimiter(opt, replace_delimiter)?;

        process_lines_with_plan(stdin, stdout, opt, &mut compressed_line_buf, &mut plan)
    } else if maybe_regex.is_some() {
        #[cfg(feature = "regex")]
        {
            let regex = maybe_regex.unwrap();
            let trim_empty = opt.bounds_type == BoundsType::Characters;
            let mut plan = FieldPlan::from_opt_regex(opt, regex.clone(), trim_empty)?;
            process_lines_with_plan(stdin, stdout, opt, &mut compressed_line_buf, &mut plan)
        }
        #[cfg(not(feature = "regex"))]
        {
            unreachable!()
        }
    } else if opt.greedy_delimiter {
        let mut plan = FieldPlan::from_opt_fixed_greedy(opt)?;
        process_lines_with_plan(stdin, stdout, opt, &mut compressed_line_buf, &mut plan)
    } else {
        // Default memmem case
        let mut plan = FieldPlan::from_opt_fixed(opt)?;
        process_lines_with_plan(stdin, stdout, opt, &mut compressed_line_buf, &mut plan)
    }
}

// Generic helper function that works with any plan type
fn process_lines_with_plan<B, W, F, R>(
    stdin: &mut B,
    stdout: &mut W,
    opt: &Opt,
    compressed_line_buf: &mut Vec<u8>,
    plan: &mut FieldPlan<F, R>,
) -> Result<()>
where
    B: BufRead,
    W: Write,
    F: DelimiterFinder,
    R: DelimiterFinder,
{
    match (opt.read_to_end, opt.eol) {
        (false, EOL::Newline) => stdin.for_byte_line(|line| {
            let line = line.strip_suffix(&[opt.eol as u8]).unwrap_or(line);
            cut_str(
                line,
                opt,
                stdout,
                compressed_line_buf,
                &[opt.eol as u8],
                plan,
            )
            .map_err(|x| {
                x.downcast::<std::io::Error>()
                    .unwrap_or_else(|e| std::io::Error::other(e.to_string()))
            })
            .and(Ok(true))
        })?,
        (false, EOL::Zero) => stdin.for_byte_record(opt.eol.into(), |line| {
            let line = line.strip_suffix(&[opt.eol as u8]).unwrap_or(line);
            cut_str(
                line,
                opt,
                stdout,
                compressed_line_buf,
                &[opt.eol as u8],
                plan,
            )
            .map_err(|x| {
                x.downcast::<std::io::Error>()
                    .unwrap_or_else(|e| std::io::Error::other(e.to_string()))
            })
            .and(Ok(true))
        })?,
        (true, _) => {
            let mut line: Vec<u8> = Vec::new();
            stdin.read_to_end(&mut line)?;
            let line = line.strip_suffix(opt.delimiter.as_slice()).unwrap_or(&line);
            cut_str(line, opt, stdout, compressed_line_buf, &opt.delimiter, plan)?
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{bounds::UserBoundsList, options::EOL};

    #[cfg(feature = "regex")]
    use crate::options::{RegexBag, Trim};

    use std::{io::Cursor, str::FromStr};

    use super::*;

    fn make_fields_opt() -> Opt {
        Opt {
            bounds_type: BoundsType::Fields,
            delimiter: "-".into(),
            ..Opt::default()
        }
    }

    #[cfg(feature = "regex")]
    fn make_regex_bag() -> RegexBag {
        RegexBag {
            normal: Regex::from_str("[.,]").unwrap(),
            greedy: Regex::from_str("([.,])+").unwrap(),
        }
    }

    #[cfg(feature = "regex")]
    fn make_cut_characters_regex_bag() -> RegexBag {
        RegexBag {
            normal: Regex::from_str("\\b|\\B").unwrap(),
            greedy: Regex::from_str("(\\b|\\B)+").unwrap(),
        }
    }

    #[test]
    fn test_read_and_cut_str_echo_non_delimited_strings() {
        // read_and_cut_str is difficult to test, let's verify at least
        // that it reads the input and appears to call cut_str

        let opt = make_fields_opt();
        let mut input = b"foo".as_slice();
        let mut output = Vec::new();
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"foo\n".as_slice());
    }

    #[test]
    fn test_read_and_cut_str_echo_non_delimited_strings_with_eol_zero() {
        // read_and_cut_str is difficult to test, let's verify at least
        // that it reads the input and appears to call cut_str

        let mut opt = make_fields_opt();
        opt.eol = EOL::Zero;
        let mut input = b"foo".as_slice();
        let mut output = Vec::new();
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"foo\0".as_slice());
    }

    fn make_cut_str_buffers() -> (Vec<u8>, Vec<u8>) {
        let output = Vec::new();
        let compressed_line_buffer = Vec::new();
        (output, compressed_line_buffer)
    }

    #[test]
    fn read_and_cut_str_echo_non_delimited_strings() {
        let opt = make_fields_opt();

        let line = b"foo";

        // non-empty line missing the delimiter
        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"foo\n".as_slice());

        // empty line
        let line = b"";
        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"".as_slice());
    }

    #[test]
    fn read_and_cut_str_skip_non_delimited_strings_when_requested() {
        let mut opt = make_fields_opt();

        opt.only_delimited = true;

        // non-empty line missing the delimiter
        let line = b"foo";
        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"".as_slice());

        // empty line
        let line = b"";
        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"".as_slice());
    }

    #[test]
    fn read_and_cut_str_it_cut_a_field() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1").unwrap();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a\n".as_slice());
    }

    #[test]
    fn read_and_cut_str_it_cut_ranges() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1,1:3").unwrap();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"aa-b-c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn read_and_cut_str_regex_it_cut_a_field() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a.b,c";
        opt.bounds = UserBoundsList::from_str("1,2,3").unwrap();
        opt.regex_bag = Some(make_regex_bag());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_left_match() {
        let line: &[u8] = b"---a-b---";
        let trim_kind = Trim::Left;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"a-b---");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_left_no_match_risk_wrong_match() {
        let line: &[u8] = b"a-b---";
        let trim_kind = Trim::Left;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"a-b---");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_left_no_match() {
        let line: &[u8] = b"abc";
        let trim_kind = Trim::Left;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"abc");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_right() {
        let line: &[u8] = b"---a-b---";
        let trim_kind = Trim::Right;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"---a-b");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_right_no_match() {
        let line: &[u8] = b"---a-b";
        let trim_kind = Trim::Right;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"---a-b");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_both() {
        let line: &[u8] = b"---a-b---";
        let trim_kind = Trim::Both;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"a-b");
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_trim_regex_both_no_match() {
        let line: &[u8] = b"a-b";
        let trim_kind = Trim::Both;
        let regex = Regex::new("-+").unwrap();
        let result = trim_regex(line, &trim_kind, &regex);

        assert_eq!(result, b"a-b");
    }

    #[test]
    fn cut_str_it_cut_consecutive_delimiters() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_compress_delimiters() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("2").unwrap();

        let line = b"--a---b--";

        // first we verify we get an empty string without compressing delimiters
        let (mut output, _) = make_cut_str_buffers();
        opt.compress_delimiter = false;
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"\n".as_slice());

        // now we do it again while compressing delimiters
        let (mut output, _) = make_cut_str_buffers();
        opt.compress_delimiter = true;
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a\n".as_slice());

        // and again but this time requesting a full range
        let (mut output, _) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"-a-b-\n".as_slice());

        // let's check with a line that doesn't start/end with delimiters
        let line = b"a---b";
        let (mut output, _) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a-b\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_compress_delimiters() {
        let mut opt = make_fields_opt();

        let line = b".,a,,,b..c";
        let (mut output, _) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("2,3,4").unwrap();
        opt.compress_delimiter = true;
        opt.regex_bag = Some(make_regex_bag());
        opt.replace_delimiter = Some("-".into());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        let line = b".,a,,,b..c";
        let (mut output, _) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;
        opt.regex_bag = Some(make_regex_bag());
        opt.replace_delimiter = Some("-".into());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"-a-b-c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_it_cut_characters() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = "😁🤩😝😎".as_bytes();
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.bounds_type = BoundsType::Characters;
        opt.regex_bag = Some(make_cut_characters_regex_bag());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, "🤩\n".as_bytes());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_it_cut_characters_and_replace_the_delimiter() {
        let opt: Opt = "-c 1,2,3:4 -r - ".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = "😁🤩😝😎".as_bytes();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(&String::from_utf8_lossy(&output), "😁-🤩-😝-😎\n");
    }

    #[test]
    fn cut_str_it_supports_zero_terminated_lines() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.eol = EOL::Zero;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"b\0".as_slice());
    }

    #[test]
    fn cut_str_it_complement_ranges() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.complement = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.join = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a-c\n".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields_with_a_custom_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.join = true;
        opt.replace_delimiter = Some("*".into());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a*c\n".as_slice());
    }

    #[test]
    fn cut_str_it_replace_delimiter() {
        let opt: Opt = "-d - -f 1:3 -r _".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a_b_c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_replace_delimiter() {
        let opt: Opt = "-e [,] -f 1:3 -r _".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a,b,c";

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a_b_c\n".as_slice());
    }

    #[test]
    fn cut_str_it_compress_and_replace_delimiter() {
        let opt: Opt = "-d - -f 1:3 -r _ -p".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a--b--c";

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a_b_c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_compress_and_replace_delimiter() {
        let opt: Opt = "-e [,] -f 1:3 -r _ -p".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a,,b,,c";

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a_b_c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_join_fields_with_a_custom_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a.b,c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.delimiter = "[.,]".into();
        opt.regex_bag = Some(make_regex_bag());
        opt.join = true;
        opt.replace_delimiter = Some("<->".into());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a<->c\n".as_slice());
    }

    #[test]
    fn cut_str_it_format_fields() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("{1} < {3} > {2}").unwrap();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"a < c > b\n".as_slice());
    }

    #[test]
    fn cut_str_supports_greedy_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a---b---c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.greedy_delimiter = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"b\n".as_slice());

        // check that, opposite to compress_delimiter, the delimiter is kept long
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a---b---c";
        opt.bounds = UserBoundsList::from_str("2:3").unwrap();
        opt.greedy_delimiter = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"b---c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_supports_greedy_delimiter() {
        // also check that, contrary to compress_delimiter, the delimiter is kept long
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a,,.,b..,,c";
        opt.bounds = UserBoundsList::from_str("2:3").unwrap();

        opt.greedy_delimiter = true;
        opt.delimiter = "[.,]".into();
        opt.regex_bag = Some(make_regex_bag());

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"b..,,c\n".as_slice());
    }

    #[test]
    fn cut_str_it_trim_fields() {
        let mut opt = make_fields_opt();
        let line = b"--a--b--c--";

        // check Trim::Both
        opt.trim = Some(Trim::Both);
        opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        opt.trim = Some(Trim::Left);
        opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        opt.trim = Some(Trim::Right);
        opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_trim_fields() {
        let mut opt = make_fields_opt();
        let line = b"..a,.b..c,,";

        opt.delimiter = "[.,]".into();
        opt.regex_bag = Some(make_regex_bag());

        // check Trim::Both
        opt.trim = Some(Trim::Both);
        opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        opt.trim = Some(Trim::Left);
        opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        opt.trim = Some(Trim::Right);
        opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

        let (mut output, _) = make_cut_str_buffers();
        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }

    #[test]
    fn cut_str_it_produce_json_output() {
        let mut opt = make_fields_opt();
        opt.json = true;
        opt.replace_delimiter = Some(",".into());
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.join = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(
            output,
            br#"["a","c"]
"#
            .as_slice()
        );
    }

    #[test]
    fn cut_str_json_with_single_field_is_still_an_array() {
        let mut opt = make_fields_opt();
        opt.json = true;
        opt.replace_delimiter = Some(",".into());
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";
        opt.bounds = UserBoundsList::from_str("1").unwrap();
        opt.join = true;

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(
            output,
            br#"["a"]
"#
            .as_slice()
        );
    }

    #[test]
    fn cut_str_complement_works_with_json() {
        let opt: Opt = "-d - -f 2,2:3,-1 -j --json --complement".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a-b-c";

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(
            output,
            br#"["a","c","a","a","b"]
"#
            .as_slice()
        );
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_json_on_characters_works() {
        let opt: Opt = "-c 1,2,3:4 --json".parse().unwrap();
        let (mut output, _) = make_cut_str_buffers();

        let line = "😁🤩😝😎".as_bytes();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();
        assert_eq!(
            &String::from_utf8_lossy(&output),
            r#"["😁","🤩","😝","😎"]
"#
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eol_and_fallbacks() {
        let mut opt = make_fields_opt();
        let (mut output, _) = make_cut_str_buffers();

        let line = b"a";
        opt.fallback_oob = Some(b"generic fallback".to_vec());
        opt.bounds = UserBoundsList::from_str("{1}-fill-{2}-more fill-{3=last fill}").unwrap();

        let mut input = Cursor::new(line);
        read_and_cut_str(&mut input, &mut output, &opt).unwrap();

        assert_eq!(
            &String::from_utf8_lossy(&output),
            "a-fill-generic fallback-more fill-last fill\n"
        );
    }
}
