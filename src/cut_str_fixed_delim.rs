use anyhow::{Result, bail};
use bstr::ByteSlice;
use bstr::io::BufReadExt;
#[cfg(feature = "regex")]
use regex::bytes::Regex;
use std::io::{BufRead, Write};
use std::ops::Range;

use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList, UserBoundsTrait};
use crate::multibyte_str::{self, FieldPlan};
use crate::options::{EOL, Opt, Trim};

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

#[cfg(feature = "regex")]
fn maybe_replace_delimiter<'a>(text: &'a [u8], opt: &Opt) -> std::borrow::Cow<'a, [u8]> {
    if opt.bounds_type == BoundsType::Characters {
        std::borrow::Cow::Borrowed(text)
    } else if let Some(new_delimiter) = opt.replace_delimiter.as_ref() {
        if let Some(re_bag) = &opt.regex_bag {
            re_bag.normal.replace_all(text, new_delimiter)
        } else {
            std::borrow::Cow::Owned(text.replace(&opt.delimiter, new_delimiter))
        }
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

#[cfg(not(feature = "regex"))]
fn maybe_replace_delimiter<'a>(text: &'a [u8], opt: &Opt) -> std::borrow::Cow<'a, [u8]> {
    if opt.bounds_type == BoundsType::Characters {
        std::borrow::Cow::Borrowed(text)
    } else if let Some(new_delimiter) = opt.replace_delimiter.as_ref() {
        std::borrow::Cow::Owned(text.replace(&opt.delimiter, new_delimiter))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
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

pub fn cut_str<W: Write>(
    line: &[u8],
    opt: &Opt,
    stdout: &mut W,
    compressed_line_buf: &mut Vec<u8>,
    eol: &[u8],
    plan: &mut FieldPlan,
) -> Result<()> {
    if opt.regex_bag.is_some() {
        if opt.compress_delimiter && opt.replace_delimiter.is_none() {
            // TODO return a proper error; do not tie cli options to errors at this level
            bail!("Cannot use --regex and --compress-delimiter without --replace-delimiter");
        }

        if opt.join && opt.replace_delimiter.is_none() {
            // TODO return a proper error; do not tie cli options to errors at this level
            bail!("Cannot use --regex and --join without --replace-delimiter");
        }
    }

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

    #[allow(unused_variables)]
    let line_holder: std::borrow::Cow<[u8]>;
    #[allow(unused_mut)]
    let mut should_build_ranges_using_regex = opt.regex_bag.is_some() && cfg!(feature = "regex");
    #[allow(unused_mut)]
    let mut delimiter = &opt.delimiter;
    let should_compress_delimiter = opt.compress_delimiter
        && (opt.bounds_type == BoundsType::Fields || opt.bounds_type == BoundsType::Lines);

    if should_compress_delimiter {
        if opt.regex_bag.is_some() && cfg!(feature = "regex") {
            #[cfg(feature = "regex")]
            {
                delimiter = opt.replace_delimiter.as_ref().unwrap(); // we checked earlier the invariant
                line_holder = compress_delimiter_with_regex(
                    line,
                    &opt.regex_bag.as_ref().unwrap().greedy,
                    delimiter,
                );
                line = &line_holder;
                should_build_ranges_using_regex = false;
            }
        } else {
            compress_delimiter(line, &opt.delimiter, compressed_line_buf);
            line = compressed_line_buf;
        }
    }

    (plan.extract_func)(line, plan);

    let fields = &mut plan.positive_fields;

    if opt.bounds_type == BoundsType::Characters && fields.len() > 2 {
        // Unless the line is empty (which should have already been handled),
        // then the empty-string delimiter generated ranges alongside each
        // character, plus one at each boundary, e.g. _f_o_o_. We drop them.
        fields.pop();
        fields.drain(..1);
    }

    let num_fields = fields.len();

    if opt.only_delimited && num_fields == 1 {
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
        _bounds = bounds.complement(num_fields)?;
        bounds = &_bounds;

        if bounds.is_empty() {
            // If the original bounds matched all the fields, the complement is empty
            if !opt.only_delimited {
                stdout.write_all(eol)?;
            }
            return Ok(());
        }
    }

    if opt.json || (opt.bounds_type == BoundsType::Characters && opt.replace_delimiter.is_some()) {
        // Unpack bounds such as 1:3 or 2: into single-field bounds
        // such as 1:1,2:2,3:3 etc...

        // Start by checking if we actually need to rewrite the bounds, since
        // it's an expensive operation.
        if bounds.iter().any(|b| {
            matches!(
                b,
                BoundOrFiller::Bound(UserBounds {
                    l: x,
                    r: y,
                    is_last: _,
                    fallback_oob: _,
                }) if x != y || x == &Side::Continue
            )
        }) {
            _bounds = bounds.unpack(num_fields);
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
        let output = if field.is_ok() {
            let field = field.unwrap();
            let idx_start = field.start;
            let idx_end = field.end;
            &line[idx_start..idx_end]
        } else if b.fallback_oob.is_some() {
            b.fallback_oob.as_ref().unwrap()
        } else if let Some(generic_fallback) = &opt.fallback_oob {
            generic_fallback
        } else {
            return Err(field.unwrap_err()); // XXX likely have to return a proper error
        };

        let field_to_print = maybe_replace_delimiter(output, opt);
        write_maybe_as_json!(stdout, field_to_print, opt.json);

        if opt.join && !b.is_last {
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

pub fn read_and_cut_str_multibyte<B: BufRead, W: Write>(
    stdin: &mut B,
    stdout: &mut W,
    opt: Opt,
) -> Result<()> {
    let line_buf: Vec<u8> = Vec::with_capacity(1024);
    let mut compressed_line_buf = if opt.compress_delimiter {
        Vec::with_capacity(line_buf.capacity())
    } else {
        Vec::new()
    };

    let the_regex = opt.regex_bag.as_ref().map(|x| &x.greedy);
    let mut plan = FieldPlan::from_bounds(&opt.bounds, &opt.delimiter, the_regex)?;

    match opt.eol {
        EOL::Newline => stdin.for_byte_line(|line| {
            let line = line.strip_suffix(&[opt.eol as u8]).unwrap_or(line);
            cut_str(
                line,
                &opt,
                stdout,
                &mut compressed_line_buf,
                &[opt.eol as u8],
                &mut plan,
            )
            // XXX Should map properly the error
            .map_err(|x| std::io::Error::other(x.to_string()))
            .and(Ok(true))
        })?,
        EOL::Zero => stdin.for_byte_record(opt.eol.into(), |line| {
            let line = line.strip_suffix(&[opt.eol as u8]).unwrap_or(line);
            cut_str(
                line,
                &opt,
                stdout,
                &mut compressed_line_buf,
                &[opt.eol as u8],
                &mut plan,
            )
            // XXX Should map properly the error
            .map_err(|x| std::io::Error::other(x.to_string()))
            .and(Ok(true))
        })?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{bounds::UserBoundsList, options::EOL};

    #[cfg(feature = "regex")]
    use crate::options::{RegexBag, Trim};

    use std::str::FromStr;

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
        read_and_cut_str_multibyte(&mut input, &mut output, opt).unwrap();
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
        read_and_cut_str_multibyte(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"foo\0".as_slice());
    }

    fn make_cut_str_buffers() -> (Vec<u8>, Vec<Range<usize>>, Vec<u8>) {
        let output = Vec::new();
        let bounds_as_ranges = Vec::new();
        let compressed_line_buffer = Vec::new();
        (output, bounds_as_ranges, compressed_line_buffer)
    }

    // #[test]
    // fn cut_str_echo_non_delimited_strings() {
    //     let opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"foo";

    //     // non-empty line missing the delimiter
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"foo\n".as_slice());

    //     // empty line
    //     let line = b"";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"\n".as_slice());
    // }

    // #[test]
    // fn cut_str_skip_non_delimited_strings_when_requested() {
    //     let mut opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];

    //     opt.only_delimited = true;

    //     // non-empty line missing the delimiter
    //     let line = b"foo";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"".as_slice());

    //     // empty line
    //     let line = b"";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"".as_slice());
    // }

    // #[test]
    // fn cut_str_it_cut_a_field() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1").unwrap();

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_cut_ranges() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1,1:3").unwrap();

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"aa-b-c\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_cut_a_field() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a.b,c";
    //     opt.bounds = UserBoundsList::from_str("1,2,3").unwrap();
    //     opt.regex_bag = Some(make_regex_bag());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_left_match() {
    //     let line: &[u8] = b"---a-b---";
    //     let trim_kind = Trim::Left;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"a-b---");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_left_no_match_risk_wrong_match() {
    //     let line: &[u8] = b"a-b---";
    //     let trim_kind = Trim::Left;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"a-b---");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_left_no_match() {
    //     let line: &[u8] = b"abc";
    //     let trim_kind = Trim::Left;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"abc");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_right() {
    //     let line: &[u8] = b"---a-b---";
    //     let trim_kind = Trim::Right;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"---a-b");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_right_no_match() {
    //     let line: &[u8] = b"---a-b";
    //     let trim_kind = Trim::Right;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"---a-b");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_both() {
    //     let line: &[u8] = b"---a-b---";
    //     let trim_kind = Trim::Both;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"a-b");
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn test_trim_regex_both_no_match() {
    //     let line: &[u8] = b"a-b";
    //     let trim_kind = Trim::Both;
    //     let regex = Regex::new("-+").unwrap();
    //     let result = trim_regex(line, &trim_kind, &regex);

    //     assert_eq!(result, b"a-b");
    // }

    // #[test]
    // fn cut_str_it_cut_consecutive_delimiters() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"ac\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_compress_delimiters() {
    //     let mut opt = make_fields_opt();
    //     opt.bounds = UserBoundsList::from_str("2").unwrap();

    //     let line = b"--a---b--";
    //     let eol = &[EOL::Newline as u8];

    //     // first we verify we get an empty string without compressing delimiters
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.compress_delimiter = false;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"\n".as_slice());

    //     // now we do it again while compressing delimiters
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.compress_delimiter = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a\n".as_slice());

    //     // and again but this time requesting a full range
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.bounds = UserBoundsList::from_str("1:").unwrap();
    //     opt.compress_delimiter = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"-a-b-\n".as_slice());

    //     // let's check with a line that doesn't start/end with delimiters
    //     let line = b"a---b";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.bounds = UserBoundsList::from_str("1:").unwrap();
    //     opt.compress_delimiter = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a-b\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_cannot_compress_delimiters_without_replace_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b".,a,,,b..c";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.bounds = UserBoundsList::from_str("2,3,4").unwrap();
    //     opt.compress_delimiter = true;
    //     opt.regex_bag = Some(make_regex_bag());
    //     opt.replace_delimiter = None;

    //     assert_eq!(
    //         cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol)
    //             .err()
    //             .map(|x| x.to_string()),
    //         Some(
    //             "Cannot use --regex and --compress-delimiter without --replace-delimiter"
    //                 .to_owned()
    //         )
    //     );
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_compress_delimiters() {
    //     let mut opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b".,a,,,b..c";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.bounds = UserBoundsList::from_str("2,3,4").unwrap();
    //     opt.compress_delimiter = true;
    //     opt.regex_bag = Some(make_regex_bag());
    //     opt.replace_delimiter = Some("-".into());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());

    //     let line = b".,a,,,b..c";
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     opt.bounds = UserBoundsList::from_str("1:").unwrap();
    //     opt.compress_delimiter = true;
    //     opt.regex_bag = Some(make_regex_bag());
    //     opt.replace_delimiter = Some("-".into());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"-a-b-c\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_it_cut_characters() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = "😁🤩😝😎".as_bytes();
    //     opt.bounds = UserBoundsList::from_str("2").unwrap();
    //     opt.bounds_type = BoundsType::Characters;
    //     opt.regex_bag = Some(make_cut_characters_regex_bag());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, "🤩\n".as_bytes());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_it_cut_characters_and_replace_the_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = "😁🤩😝😎".as_bytes();
    //     opt.bounds = UserBoundsList::from_str("1,2,3:4").unwrap();
    //     opt.bounds_type = BoundsType::Characters;
    //     opt.regex_bag = Some(make_cut_characters_regex_bag());
    //     opt.replace_delimiter = Some("-".into());
    //     opt.join = true; // implied when using BoundsType::Characters

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(&String::from_utf8_lossy(&output), "😁-🤩-😝-😎\n");
    // }

    // #[test]
    // fn cut_str_it_supports_zero_terminated_lines() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Zero as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("2").unwrap();
    //     opt.eol = EOL::Zero;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"b\0".as_slice());
    // }

    // #[test]
    // fn cut_str_it_complement_ranges() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("2").unwrap();
    //     opt.complement = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"ac\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_join_fields() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();
    //     opt.join = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a-c\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_join_fields_with_a_custom_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();
    //     opt.join = true;
    //     opt.replace_delimiter = Some("*".into());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a*c\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_cannot_join_fields_without_replace_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a,,b..c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();
    //     opt.delimiter = "[.,]".into();
    //     opt.regex_bag = Some(make_regex_bag());
    //     opt.join = true;

    //     assert_eq!(
    //         cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol)
    //             .err()
    //             .map(|x| x.to_string()),
    //         Some("Cannot use --regex and --join without --replace-delimiter".to_owned())
    //     );
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_join_fields_with_a_custom_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a.b,c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();
    //     opt.delimiter = "[.,]".into();
    //     opt.regex_bag = Some(make_regex_bag());
    //     opt.join = true;
    //     opt.replace_delimiter = Some("<->".into());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a<->c\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_format_fields() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("{1} < {3} > {2}").unwrap();

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"a < c > b\n".as_slice());
    // }

    // #[test]
    // fn cut_str_supports_greedy_delimiter() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a---b---c";
    //     opt.bounds = UserBoundsList::from_str("2").unwrap();
    //     opt.greedy_delimiter = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"b\n".as_slice());

    //     // check that, opposite to compress_delimiter, the delimiter is kept long
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a---b---c";
    //     opt.bounds = UserBoundsList::from_str("2:3").unwrap();
    //     opt.greedy_delimiter = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"b---c\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_supports_greedy_delimiter() {
    //     // also check that, contrary to compress_delimiter, the delimiter is kept long
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a,,.,b..,,c";
    //     opt.bounds = UserBoundsList::from_str("2:3").unwrap();

    //     opt.greedy_delimiter = true;
    //     opt.delimiter = "[.,]".into();
    //     opt.regex_bag = Some(make_regex_bag());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"b..,,c\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_trim_fields() {
    //     let mut opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];
    //     let line = b"--a--b--c--";

    //     // check Trim::Both
    //     opt.trim = Some(Trim::Both);
    //     opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());

    //     // check Trim::Left
    //     opt.trim = Some(Trim::Left);
    //     opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());

    //     // check Trim::Right
    //     opt.trim = Some(Trim::Right);
    //     opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_regex_it_trim_fields() {
    //     let mut opt = make_fields_opt();
    //     let eol = &[EOL::Newline as u8];
    //     let line = b"..a,.b..c,,";

    //     opt.delimiter = "[.,]".into();
    //     opt.regex_bag = Some(make_regex_bag());

    //     // check Trim::Both
    //     opt.trim = Some(Trim::Both);
    //     opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());

    //     // check Trim::Left
    //     opt.trim = Some(Trim::Left);
    //     opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());

    //     // check Trim::Right
    //     opt.trim = Some(Trim::Right);
    //     opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(output, b"abc\n".as_slice());
    // }

    // #[test]
    // fn cut_str_it_produce_json_output() {
    //     let mut opt = make_fields_opt();
    //     opt.json = true;
    //     opt.replace_delimiter = Some(",".into());
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1,3").unwrap();
    //     opt.join = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(
    //         output,
    //         br#"["a","c"]
    // "#
    //         .as_slice()
    //     );
    // }

    // #[test]
    // fn cut_str_json_with_single_field_is_still_an_array() {
    //     let mut opt = make_fields_opt();
    //     opt.json = true;
    //     opt.replace_delimiter = Some(",".into());
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("1").unwrap();
    //     opt.join = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(
    //         output,
    //         br#"["a"]
    // "#
    //         .as_slice()
    //     );
    // }

    // #[test]
    // fn cut_str_complement_works_with_json() {
    //     let mut opt = make_fields_opt();
    //     opt.json = true;
    //     opt.replace_delimiter = Some(",".into());
    //     opt.complement = true;
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a-b-c";
    //     opt.bounds = UserBoundsList::from_str("2,2:3,-1").unwrap();
    //     opt.join = true;

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(
    //         output,
    //         br#"["a","c","a","a","b"]
    // "#
    //         .as_slice()
    //     );
    // }

    // #[cfg(feature = "regex")]
    // #[test]
    // fn cut_str_json_on_characters_works() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = "😁🤩😝😎".as_bytes();
    //     opt.bounds = UserBoundsList::from_str("1,2,3:4").unwrap();
    //     opt.bounds_type = BoundsType::Characters;
    //     opt.join = true;
    //     opt.json = true;
    //     opt.replace_delimiter = Some(",".into());
    //     opt.regex_bag = Some(make_cut_characters_regex_bag());

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
    //     assert_eq!(
    //         &String::from_utf8_lossy(&output),
    //         r#"["😁","🤩","😝","😎"]
    // "#
    //     );
    // }

    // #[test]
    // fn test_cut_bytes_stream_cut_simplest_field_with_eol_and_fallbacks() {
    //     let mut opt = make_fields_opt();
    //     let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
    //     let eol = &[EOL::Newline as u8];

    //     let line = b"a";
    //     opt.fallback_oob = Some(b"generic fallback".to_vec());
    //     opt.bounds = UserBoundsList::from_str("{1}-fill-{2}-more fill-{3=last fill}").unwrap();

    //     cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();

    //     assert_eq!(
    //         &String::from_utf8_lossy(&output),
    //         "a-fill-generic fallback-more fill-last fill\n"
    //     );
    // }
}
