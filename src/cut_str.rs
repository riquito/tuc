use anyhow::Result;
use std::io::Write;
use std::ops::Range;

use crate::bounds::{bounds_to_std_range, BoundOrFiller, BoundsType};
use crate::options::{Opt, Trim};
use crate::read_utils::read_line_with_eol;

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
        if !(greedy && idx == next_part_start) {
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

pub fn cut_str(
    line: &str,
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    bounds_as_ranges: &mut Vec<Range<usize>>,
    compressed_line_buf: &mut String,
    eol: &[u8],
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
            stdout.write_all(eol)?;
        }
        return Ok(());
    }

    build_ranges_vec(bounds_as_ranges, line, &opt.delimiter, opt.greedy_delimiter);

    if opt.compress_delimiter
        && (opt.bounds_type == BoundsType::Fields || opt.bounds_type == BoundsType::Lines)
    {
        compressed_line_buf.clear();
        compress_delimiter(bounds_as_ranges, line, &opt.delimiter, compressed_line_buf);
        line = compressed_line_buf;
        build_ranges_vec(bounds_as_ranges, line, &opt.delimiter, opt.greedy_delimiter);
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

                        if let Some(replace_delimiter) = &opt.replace_delimiter {
                            stdout.write_all(
                                output.replace(&opt.delimiter, replace_delimiter).as_bytes(),
                            )?;
                        } else {
                            stdout.write_all(output.as_bytes())?;
                        }

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

pub fn read_and_cut_str(
    stdin: &mut std::io::BufReader<std::io::StdinLock>,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    opt: Opt,
) -> Result<()> {
    let mut line_buf = String::with_capacity(1024);
    let mut bounds_as_ranges: Vec<Range<usize>> = Vec::with_capacity(100);
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
        build_ranges_vec(&mut v_range, "a--b", "-", true);
        assert_eq!(
            v_range,
            vec![Range { start: 0, end: 1 }, Range { start: 3, end: 4 }]
        );

        v_range.clear();
        build_ranges_vec(&mut v_range, "a--", "-", true);
        assert_eq!(
            v_range,
            vec![Range { start: 0, end: 1 }, Range { start: 3, end: 3 }]
        );
    }
}
