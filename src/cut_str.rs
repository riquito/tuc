use anyhow::{bail, Result};
use std::io::{BufRead, Write};
use std::ops::Range;

use crate::bounds::{bounds_to_std_range, BoundOrFiller, BoundsType};
use crate::options::{Opt, Trim};
use crate::read_utils::read_line_with_eol;

#[cfg(feature = "regex")]
use regex::Regex;

fn complement_std_range(parts_length: usize, r: &Range<usize>) -> Vec<Range<usize>> {
    match (r.start, r.end) {
        // full match => no match
        (0, end) if end == parts_length => Vec::new(),
        // match left side => match right side
        #[allow(clippy::single_range_in_vec_init)]
        (0, right) => vec![right..parts_length],
        // match right side => match left side
        #[allow(clippy::single_range_in_vec_init)]
        (left, end) if end == parts_length => vec![0..left],
        // match middle of string => match before and after
        (left, right) => vec![0..left, right..parts_length],
    }
}

// Split a string into parts and build a vector of ranges that match those parts.
//
// `buffer` - vector that will be filled with ranges
// `line` - the string to split
// `delimiter` - what to search to split the string
// `greedy` - whether to consider consecutive delimiters as one or not
fn build_ranges_vec(buffer: &mut Vec<Range<usize>>, line: &str, delimiter: &str, greedy: bool) {
    buffer.clear();

    if line.is_empty() {
        return;
    }

    let delimiter_length = delimiter.len();
    let mut next_part_start = 0;

    for (idx, _) in line.match_indices(&delimiter) {
        if !(greedy && idx == next_part_start && idx != 0) {
            buffer.push(Range {
                start: next_part_start,
                end: idx,
            });
        }

        next_part_start = idx + delimiter_length;
    }

    buffer.push(Range {
        start: next_part_start,
        end: line.len(),
    });
}

#[cfg(feature = "regex")]
fn build_ranges_vec_from_regex(buffer: &mut Vec<Range<usize>>, line: &str, re: &Regex) {
    buffer.clear();

    if line.is_empty() {
        return;
    }

    let mut next_part_start = 0;

    for mat in re.find_iter(line) {
        buffer.push(Range {
            start: next_part_start,
            end: mat.start(),
        });

        next_part_start = mat.end();
    }

    buffer.push(Range {
        start: next_part_start,
        end: line.len(),
    });
}

fn compress_delimiter(line: &str, delimiter: &str, output: &mut String) {
    output.clear();
    let mut prev_idx = 0;

    for (idx, _) in line.match_indices(delimiter) {
        let prev_part = &line[prev_idx..idx];

        if idx == 0 {
            output.push_str(delimiter);
        } else if !prev_part.is_empty() {
            output.push_str(prev_part);
            output.push_str(delimiter);
        }

        prev_idx = idx + delimiter.len();
    }

    if prev_idx < line.len() {
        output.push_str(&line[prev_idx..]);
    }
}

#[cfg(feature = "regex")]
fn compress_delimiter_with_regex<'a>(
    line: &'a str,
    re: &Regex,
    new_delimiter: &str,
) -> std::borrow::Cow<'a, str> {
    re.replace_all(line, new_delimiter)
}

#[cfg(feature = "regex")]
fn maybe_replace_delimiter<'a>(text: &'a str, opt: &Opt) -> std::borrow::Cow<'a, str> {
    if let Some(new_delimiter) = opt.replace_delimiter.as_ref() {
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
fn maybe_replace_delimiter<'a>(text: &'a str, opt: &Opt) -> std::borrow::Cow<'a, str> {
    if let Some(new_delimiter) = opt.replace_delimiter.as_ref() {
        std::borrow::Cow::Owned(text.replace(&opt.delimiter, new_delimiter))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

fn trim<'a>(line: &'a str, trim_kind: &Trim, delimiter: &str) -> &'a str {
    match trim_kind {
        Trim::Both => line
            .trim_start_matches(delimiter)
            .trim_end_matches(delimiter),
        Trim::Left => line.trim_start_matches(delimiter),
        Trim::Right => line.trim_end_matches(delimiter),
    }
}

#[cfg(feature = "regex")]
fn trim_regex<'a>(line: &'a str, trim_kind: &Trim, re: &Regex) -> &'a str {
    let mut iter = re.find_iter(line);
    let mut idx_start = 0;
    let mut idx_end = line.len();

    if trim_kind == &Trim::Both || trim_kind == &Trim::Left {
        if let Some(m) = iter.next() {
            if m.start() == 0 {
                idx_start = m.end();
            }
        }
    }

    if trim_kind == &Trim::Both || trim_kind == &Trim::Right {
        if let Some(m) = iter.last() {
            if m.end() == line.len() {
                idx_end = m.start();
            }
        }
    }

    &line[idx_start..idx_end]
}

pub fn cut_str<W: Write>(
    line: &str,
    opt: &Opt,
    stdout: &mut W,
    bounds_as_ranges: &mut Vec<Range<usize>>,
    compressed_line_buf: &mut String,
    eol: &[u8],
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
    let line_holder: std::borrow::Cow<str>;
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

    if should_build_ranges_using_regex {
        #[cfg(feature = "regex")]
        build_ranges_vec_from_regex(
            bounds_as_ranges,
            line,
            if opt.greedy_delimiter {
                &opt.regex_bag.as_ref().unwrap().greedy
            } else {
                &opt.regex_bag.as_ref().unwrap().normal
            },
        );
    } else {
        build_ranges_vec(bounds_as_ranges, line, delimiter, opt.greedy_delimiter);
    }

    if opt.bounds_type == BoundsType::Characters && bounds_as_ranges.len() > 2 {
        // Unless the line is empty (which should have already been handled),
        // then the empty-string delimiter generated ranges alongside each
        // character, plus one at each boundary, e.g. _f_o_o_. We drop them.
        bounds_as_ranges.pop();
        bounds_as_ranges.drain(..1);
    }

    match bounds_as_ranges.len() {
        1 if opt.only_delimited => (),
        1 if opt.bounds.0.len() == 1 => {
            stdout.write_all(line.as_bytes())?;
            stdout.write_all(eol)?;
        }
        _ => {
            opt.bounds
                .0
                .iter()
                .enumerate()
                .try_for_each(|(i, bof)| -> Result<()> {
                    let b = match bof {
                        BoundOrFiller::Filler(f) => {
                            stdout.write_all(f.as_bytes())?;
                            return Ok(());
                        }
                        BoundOrFiller::Bound(b) => b,
                    };

                    let r_array = [bounds_to_std_range(bounds_as_ranges.len(), b)?];
                    let mut r_iter = r_array.iter();
                    let _complements;
                    let mut n_ranges = 1;

                    if opt.complement {
                        _complements = complement_std_range(bounds_as_ranges.len(), &r_array[0]);
                        r_iter = _complements.iter();
                        n_ranges = _complements.len();
                    }

                    for (idx_r, r) in r_iter.enumerate() {
                        let idx_start = bounds_as_ranges[r.start].start;
                        let idx_end = bounds_as_ranges[r.end - 1].end;
                        let output = &line[idx_start..idx_end];

                        stdout.write_all(maybe_replace_delimiter(output, opt).as_bytes())?;

                        if opt.join && !(i == opt.bounds.0.len() - 1 && idx_r == n_ranges - 1) {
                            stdout.write_all(
                                opt.replace_delimiter
                                    .as_ref()
                                    .unwrap_or(&opt.delimiter)
                                    .as_bytes(),
                            )?;
                        }
                    }

                    Ok(())
                })?;

            stdout.write_all(eol)?;
        }
    }

    Ok(())
}

pub fn read_and_cut_str<B: BufRead, W: Write>(
    stdin: &mut B,
    stdout: &mut W,
    opt: Opt,
) -> Result<()> {
    let mut line_buf = String::with_capacity(256);
    let mut bounds_as_ranges: Vec<Range<usize>> = Vec::with_capacity(16);
    let mut compressed_line_buf = if opt.compress_delimiter {
        String::with_capacity(line_buf.capacity())
    } else {
        String::new()
    };

    while let Some(line) = read_line_with_eol(stdin, &mut line_buf, opt.eol) {
        let line = line?;
        let line: &str = line.as_ref();
        let line = line.strip_suffix(opt.eol as u8 as char).unwrap_or(line);
        cut_str(
            line,
            &opt,
            stdout,
            &mut bounds_as_ranges,
            &mut compressed_line_buf,
            &[opt.eol as u8],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{bounds::UserBoundsList, options::EOL};

    #[cfg(feature = "regex")]
    use crate::options::RegexBag;

    use std::str::FromStr;

    use super::*;

    fn make_fields_opt() -> Opt {
        Opt {
            bounds_type: BoundsType::Fields,
            delimiter: String::from("-"),
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
    fn test_build_ranges_vec() {
        let mut v_range: Vec<Range<usize>> = Vec::new();

        // non greedy

        v_range.clear();
        build_ranges_vec(&mut v_range, "", "-", false);
        assert_eq!(v_range, vec![]);

        v_range.clear();
        build_ranges_vec(&mut v_range, "a", "-", false);
        assert_eq!(v_range, vec![Range { start: 0, end: 1 }]);

        v_range.clear();
        build_ranges_vec(&mut v_range, "-", "-", true);
        assert_eq!(
            v_range,
            vec![Range { start: 0, end: 0 }, Range { start: 1, end: 1 }]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "a-b", "-", false);
        assert_eq!(
            v_range,
            vec![Range { start: 0, end: 1 }, Range { start: 2, end: 3 }]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "-a-", "-", false);
        assert_eq!(
            v_range,
            vec![
                Range { start: 0, end: 0 },
                Range { start: 1, end: 2 },
                Range { start: 3, end: 3 }
            ]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "a--", "-", false);
        assert_eq!(
            v_range,
            vec![
                Range { start: 0, end: 1 },
                Range { start: 2, end: 2 },
                Range { start: 3, end: 3 }
            ]
        );

        // greedy

        v_range.clear();
        build_ranges_vec(&mut v_range, "", "-", true);
        assert_eq!(v_range, vec![]);

        v_range.clear();
        build_ranges_vec(&mut v_range, "a", "-", true);
        assert_eq!(v_range, vec![Range { start: 0, end: 1 }]);

        v_range.clear();
        build_ranges_vec(&mut v_range, "-", "-", true);
        assert_eq!(
            v_range,
            vec![Range { start: 0, end: 0 }, Range { start: 1, end: 1 }]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "-a--b", "-", true);
        assert_eq!(
            v_range,
            vec![
                Range { start: 0, end: 0 },
                Range { start: 1, end: 2 },
                Range { start: 4, end: 5 }
            ]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "-a--", "-", true);
        assert_eq!(
            v_range,
            vec![
                Range { start: 0, end: 0 },
                Range { start: 1, end: 2 },
                Range { start: 4, end: 4 }
            ]
        );
    }

    #[test]
    fn test_read_and_cut_str_echo_non_delimited_strings() {
        // read_and_cut_str is difficult to test, let's verify at least
        // that it reads the input and appears to call cut_str

        let opt = make_fields_opt();
        let mut input = b"foo".as_slice();
        let mut output = Vec::new();
        read_and_cut_str(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"foo\n".as_slice());
    }

    fn make_cut_str_buffers() -> (Vec<u8>, Vec<Range<usize>>, String) {
        let output = Vec::new();
        let bounds_as_ranges = Vec::new();
        let compressed_line_buffer = String::new();
        (output, bounds_as_ranges, compressed_line_buffer)
    }

    #[test]
    fn cut_str_echo_non_delimited_strings() {
        let opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "foo";

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"foo\n".as_slice());
    }

    #[test]
    fn cut_str_skip_non_delimited_strings_when_requested() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        opt.only_delimited = true;
        let line = "foo";

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"".as_slice());
    }

    #[test]
    fn cut_str_it_cut_a_field() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("1").unwrap();

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_cut_a_field() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a.b,c";
        opt.bounds = UserBoundsList::from_str("1,2,3").unwrap();
        opt.regex_bag = Some(make_regex_bag());

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_consecutive_delimiters() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_compress_delimiters() {
        let mut opt = make_fields_opt();
        opt.bounds = UserBoundsList::from_str("2").unwrap();

        let line = "--a---b--";
        let eol = &[EOL::Newline as u8];

        // first we verify we get an empty string without compressing delimiters
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.compress_delimiter = false;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"\n".as_slice());

        // now we do it again while compressing delimiters
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.compress_delimiter = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a\n".as_slice());

        // and again but this time requesting a full range
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"-a-b-\n".as_slice());

        // let's check with a line that doesn't start/end with delimiters
        let line = "a---b";
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a-b\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_cannot_compress_delimiters_without_replace_delimiter() {
        let mut opt = make_fields_opt();
        let eol = &[EOL::Newline as u8];

        let line = ".,a,,,b..c";
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("2,3,4").unwrap();
        opt.compress_delimiter = true;
        opt.regex_bag = Some(make_regex_bag());
        opt.replace_delimiter = None;

        assert_eq!(
            cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol)
                .err()
                .map(|x| x.to_string()),
            Some(
                "Cannot use --regex and --compress-delimiter without --replace-delimiter"
                    .to_owned()
            )
        );
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_compress_delimiters() {
        let mut opt = make_fields_opt();
        let eol = &[EOL::Newline as u8];

        let line = ".,a,,,b..c";
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("2,3,4").unwrap();
        opt.compress_delimiter = true;
        opt.regex_bag = Some(make_regex_bag());
        opt.replace_delimiter = Some(String::from("-"));

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        let line = ".,a,,,b..c";
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        opt.bounds = UserBoundsList::from_str("1:").unwrap();
        opt.compress_delimiter = true;
        opt.regex_bag = Some(make_regex_bag());
        opt.replace_delimiter = Some(String::from("-"));

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"-a-b-c\n".as_slice());
    }

    #[test]
    fn cut_str_it_cut_characters() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "😁🤩😝😎";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.bounds_type = BoundsType::Characters;
        opt.delimiter = String::new();

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, "🤩\n".as_bytes());
    }

    #[test]
    fn cut_str_it_supports_zero_terminated_lines() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Zero as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.eol = EOL::Zero;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"b\0".as_slice());
    }

    #[test]
    fn cut_str_it_complement_ranges() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.complement = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"ac\n".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.join = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a-c\n".as_slice());
    }

    #[test]
    fn cut_str_it_join_fields_with_a_custom_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.join = true;
        opt.replace_delimiter = Some(String::from("*"));

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a*c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_cannot_join_fields_without_replace_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a,,b..c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.delimiter = String::from("[.,]");
        opt.regex_bag = Some(make_regex_bag());
        opt.join = true;

        assert_eq!(
            cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol)
                .err()
                .map(|x| x.to_string()),
            Some("Cannot use --regex and --join without --replace-delimiter".to_owned())
        );
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_join_fields_with_a_custom_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a.b,c";
        opt.bounds = UserBoundsList::from_str("1,3").unwrap();
        opt.delimiter = String::from("[.,]");
        opt.regex_bag = Some(make_regex_bag());
        opt.join = true;
        opt.replace_delimiter = Some(String::from("<->"));

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a<->c\n".as_slice());
    }

    #[test]
    fn cut_str_it_format_fields() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a-b-c";
        opt.bounds = UserBoundsList::from_str("{1} < {3} > {2}").unwrap();

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"a < c > b\n".as_slice());
    }

    #[test]
    fn cut_str_supports_greedy_delimiter() {
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a---b---c";
        opt.bounds = UserBoundsList::from_str("2").unwrap();
        opt.greedy_delimiter = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"b\n".as_slice());

        // check that, opposite to compress_delimiter, the delimiter is kept long
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a---b---c";
        opt.bounds = UserBoundsList::from_str("2:3").unwrap();
        opt.greedy_delimiter = true;

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"b---c\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_supports_greedy_delimiter() {
        // also check that, contrary to compress_delimiter, the delimiter is kept long
        let mut opt = make_fields_opt();
        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        let eol = &[EOL::Newline as u8];

        let line = "a,,.,b..,,c";
        opt.bounds = UserBoundsList::from_str("2:3").unwrap();

        opt.greedy_delimiter = true;
        opt.delimiter = String::from("[.,]");
        opt.regex_bag = Some(make_regex_bag());

        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        dbg!(std::str::from_utf8(&output).unwrap());
        assert_eq!(output, b"b..,,c\n".as_slice());
    }

    #[test]
    fn cut_str_it_trim_fields() {
        let mut opt = make_fields_opt();
        let eol = &[EOL::Newline as u8];
        let line = "--a--b--c--";

        // check Trim::Both
        opt.trim = Some(Trim::Both);
        opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        opt.trim = Some(Trim::Left);
        opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        opt.trim = Some(Trim::Right);
        opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }

    #[cfg(feature = "regex")]
    #[test]
    fn cut_str_regex_it_trim_fields() {
        let mut opt = make_fields_opt();
        let eol = &[EOL::Newline as u8];
        let line = "..a,.b..c,,";

        opt.delimiter = String::from("[.,]");
        opt.regex_bag = Some(make_regex_bag());

        // check Trim::Both
        opt.trim = Some(Trim::Both);
        opt.bounds = UserBoundsList::from_str("1,3,-1").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Left
        opt.trim = Some(Trim::Left);
        opt.bounds = UserBoundsList::from_str("1,3,-3").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());

        // check Trim::Right
        opt.trim = Some(Trim::Right);
        opt.bounds = UserBoundsList::from_str("3,5,-1").unwrap();

        let (mut output, mut buffer1, mut buffer2) = make_cut_str_buffers();
        cut_str(line, &opt, &mut output, &mut buffer1, &mut buffer2, eol).unwrap();
        assert_eq!(output, b"abc\n".as_slice());
    }
}
