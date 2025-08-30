use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList, UserBoundsTrait};
use crate::options::{EOL, Opt};
use anyhow::Result;
use anyhow::bail;
use bstr::ByteSlice;
use core::panic;
use std::convert::TryFrom;
use std::io::BufRead;
use std::io::Write;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug)]
struct ForwardBounds {
    pub list: UserBoundsList,
    last_bound_idx: usize,
}

impl TryFrom<&UserBoundsList> for ForwardBounds {
    type Error = &'static str;

    fn try_from(value: &UserBoundsList) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err("Cannot create ForwardBounds from an empty UserBoundsList")
        } else if value.is_forward_only() {
            let mut prev_bound_idx = 0;
            value.iter().try_for_each(|bof| {
                if let BoundOrFiller::Bound(b) = bof {
                    if b.l().abs_value() == prev_bound_idx {
                        return Err("Bounds are sorted, but can't be repeated");
                    }
                    prev_bound_idx = b.l().abs_value();
                }
                Ok(())
            })?;

            let value: UserBoundsList =
                value.iter().cloned().collect::<Vec<BoundOrFiller>>().into();
            let mut maybe_last_bound: Option<usize> = None;
            value.iter().enumerate().rev().any(|(idx, bof)| {
                if matches!(bof, BoundOrFiller::Bound(_)) {
                    maybe_last_bound = Some(idx);
                    true
                } else {
                    false
                }
            });

            if let Some(last_bound_idx) = maybe_last_bound {
                Ok(ForwardBounds {
                    list: value,
                    last_bound_idx,
                })
            } else {
                Err("Cannot create ForwardBounds from UserBoundsList without bounds")
            }
        } else {
            Err("The provided UserBoundsList is not forward only")
        }
    }
}

impl FromStr for ForwardBounds {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            bail!("UserBoundsList must contain at least one UserBounds");
        }
        let bounds = UserBoundsList::from_str(s)?;
        ForwardBounds::try_from(&bounds).map_err(|e| anyhow::anyhow!(e))
    }
}

impl Deref for ForwardBounds {
    type Target = UserBoundsList;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl ForwardBounds {
    fn get_last_bound(&self) -> &UserBounds {
        if let Some(BoundOrFiller::Bound(b)) = self.list.get(self.last_bound_idx) {
            b
        } else {
            panic!("Invariant error: last_bound_idx failed to match a bound")
        }
    }
}

#[derive(Debug)]
pub struct StreamOpt {
    delimiter: u8,
    replace_delimiter: Option<u8>,
    join: bool,
    eol: EOL,
    fallback_oob: Option<Vec<u8>>,
    bounds: ForwardBounds,
    // ## only_delimited: bool, ##
    // We can't support it, because we read fixed blocks of data.
    // If we don't find the delimiter in one block and move the next block,
    // and then we find the delimiter, we can't print the content from the
    // previous block, it's lost.
    // The alternative would be to start buffering blocks, but who knows
    // how much they'd grow: it would be not different from buffering the
    // whole line, and this mode is about doing the job with fixed memory.
}

impl TryFrom<&Opt> for StreamOpt {
    type Error = &'static str;

    fn try_from(value: &Opt) -> Result<Self, Self::Error> {
        if value.delimiter.as_bytes().len() != 1 {
            return Err("Delimiter must be 1 byte wide for FastOpt");
        }

        if value.complement
            || value.greedy_delimiter
            || value.compress_delimiter
            || value.json
            || value.bounds_type != BoundsType::Fields
            || (value.replace_delimiter.is_some() && value.replace_delimiter.as_ref().unwrap().len() != 1)
            || value.trim.is_some()
            || value.regex_bag.is_some()
            // only_delimited can't be supported without reading the full line first
            // to search for delimiters, which can't be done if we read by chunks.
            || value.only_delimited
        {
            return Err(
                "StreamOpt supports solely forward fields, join and single-character delimiters",
            );
        }

        if let Ok(forward_bounds) = ForwardBounds::try_from(&value.bounds) {
            Ok(StreamOpt {
                delimiter: value.delimiter.as_bytes().first().unwrap().to_owned(),
                replace_delimiter: value
                    .replace_delimiter
                    .as_ref()
                    .map(|s| s.as_bytes().first().unwrap().to_owned()),
                join: value.join,
                eol: value.eol,
                bounds: forward_bounds,
                fallback_oob: value.fallback_oob.clone(),
            })
        } else {
            Err("Bounds cannot be converted to ForwardBounds")
        }
    }
}

pub fn read_and_cut_bytes_stream<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &StreamOpt,
) -> Result<()> {
    let last_interesting_field = *opt.bounds.get_last_bound().r();
    cut_bytes_stream(stdin, stdout, opt, last_interesting_field)?;
    Ok(())
}

#[inline(always)]
fn print_field<W: Write>(
    stdin: &mut W,
    buffer: &[u8],
    delim: u8,
    prepend_delimiter: bool,
) -> Result<()> {
    if prepend_delimiter {
        stdin.write_all(&[delim])?;
    }
    stdin.write_all(buffer)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn print_bof<W: Write>(
    stdout: &mut W,
    opt: &StreamOpt,
    bof_idx: usize,
    curr_field: usize,
    chunk: &[u8],
    prev_chunk_idx: usize,
    chunk_idx: usize,
    prev_chunk_may_be_truncated: bool,
) -> Result<usize> {
    let mut bof_idx = bof_idx;

    if let Some(BoundOrFiller::Filler(f)) = opt.bounds.get(bof_idx) {
        stdout.write_all(f)?;
        bof_idx += 1;
    }

    if let Some(BoundOrFiller::Bound(b)) = opt.bounds.get(bof_idx) {
        // Bound may not match when, at example, we are waiting to print
        // field 4 but we are at field 2.
        if b.matches(curr_field).unwrap() {
            let prepend_delimiter = !prev_chunk_may_be_truncated
                && curr_field > 1
                && (opt.join || (b.l().abs_value() != curr_field));

            let delimiter = opt.replace_delimiter.unwrap_or(opt.delimiter);

            print_field(
                stdout,
                &chunk[prev_chunk_idx..chunk_idx],
                delimiter,
                prepend_delimiter,
            )?;

            if b.r().abs_value() == curr_field {
                bof_idx += 1;
            }
        }
    }

    Ok(bof_idx)
}

// Exhaust bounds and print fillers or fallbacks.
// This function is meant to be called after every
// bound has been found and possibly printed, and
// the rest of the bof (bound or filler) are expected
// to be either filler or bounds with a fallback.
fn print_filler_or_fallbacks<W: Write>(
    stdout: &mut W,
    bof_idx: usize,
    opt: &StreamOpt,
) -> Result<()> {
    for bof in opt.bounds[bof_idx..].iter() {
        let b = match bof {
            BoundOrFiller::Filler(f) => {
                stdout.write_all(f.as_bytes())?;
                continue;
            }
            BoundOrFiller::Bound(b) => b,
        };

        if b.r().abs_value() == Side::max_right() {
            break;
        }

        let output = if b.fallback_oob().is_some() {
            b.fallback_oob().as_ref().unwrap()
        } else if let Some(generic_fallback) = &opt.fallback_oob {
            generic_fallback
        } else {
            bail!("Out of bounds: {}", b.l());
        };

        stdout.write_all(output)?;
    }

    Ok(())
}

fn cut_bytes_stream<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &StreamOpt,
    last_interesting_field: Side,
) -> Result<()> {
    let eol: u8 = opt.eol.into();
    let mut eof = false;

    'new_line: loop {
        let mut bof_idx = 0;
        let mut curr_field = 1;
        let mut prev_chunk_may_be_truncated = false;
        let mut eol_reached = false;
        let mut empty_line = true;

        'new_chunk: while !eol_reached && !eof {
            let chunk = stdin.fill_buf()?;

            if chunk.is_empty() {
                eof = true;
                if empty_line {
                    eol_reached = true;
                }
                break 'new_chunk;
            }

            empty_line = false;

            let mut chunk_part_start_idx = 0;
            let mut bytes_to_consume = 0;

            // Process chunk looking for delimiters or EOL
            for chunk_idx in memchr::memchr2_iter(opt.delimiter, eol, chunk) {
                eol_reached = chunk[chunk_idx] == eol;
                bytes_to_consume = chunk_idx + 1;

                // Handle field content before delimiter/EOL
                if bytes_to_consume > 1 {
                    bof_idx = print_bof(
                        stdout,
                        opt,
                        bof_idx,
                        curr_field,
                        chunk,
                        chunk_part_start_idx,
                        chunk_idx,
                        prev_chunk_may_be_truncated,
                    )?;
                }

                prev_chunk_may_be_truncated = false;
                // Update chunk_part_start_idx to point to the next field
                chunk_part_start_idx = chunk_idx + 1;

                // EOL handling
                if eol_reached {
                    print_filler_or_fallbacks(stdout, bof_idx, opt)?;
                    stdout.write_all(&[opt.eol.into()])?;
                    break;
                }

                // If we've found the last field we're interested in
                if curr_field == last_interesting_field.abs_value() {
                    // Print any remaining fillers (no fallbacks, since we're done with the fields)
                    print_filler_or_fallbacks(stdout, bof_idx, opt)?;

                    // Attempt to skip to EOL (if it's not in this chunk we'll wait for the next chunk)
                    if let Some(eol_idx) = memchr::memchr(eol, &chunk[bytes_to_consume..]) {
                        bytes_to_consume = bytes_to_consume + eol_idx + 1;
                        eol_reached = true;
                        stdout.write_all(&[opt.eol.into()])?;
                    }

                    break;
                }

                curr_field += 1;
            }

            // Handle remaining data in chunk
            if !eol_reached {
                let chunk_has_unused_content = chunk.len() > bytes_to_consume;

                if chunk_has_unused_content {
                    // Process potential partial field
                    bof_idx = print_bof(
                        stdout,
                        opt,
                        bof_idx,
                        curr_field,
                        chunk,
                        chunk_part_start_idx,
                        chunk.len(),
                        prev_chunk_may_be_truncated,
                    )?;
                    prev_chunk_may_be_truncated = true;
                }

                bytes_to_consume = chunk.len();
            }

            stdin.consume(bytes_to_consume);

            // let's loop and read the next chunk
        }

        // Handle EOF at end of line
        if eof && !eol_reached {
            print_filler_or_fallbacks(stdout, bof_idx, opt)?;
            stdout.write_all(&[opt.eol.into()])?;
            break 'new_line;
        }

        // If we've reached EOF, exit the outer loop too
        if eof {
            break 'new_line;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use std::{io::BufReader, str::FromStr};

    use super::*;

    fn make_fields_opt() -> StreamOpt {
        StreamOpt {
            delimiter: b'-',
            replace_delimiter: None,
            eol: EOL::Newline,
            join: false,
            fallback_oob: None,
            bounds: ForwardBounds::from_str("1").unwrap(),
        }
    }

    #[test]
    fn test_try_from_valid_forward_bounds() {
        let bounds = UserBoundsList::from_str("1,2,3:5").unwrap();
        assert!(ForwardBounds::try_from(&bounds).is_ok());
    }

    #[test]
    fn test_try_from_repeated_bounds() {
        let bounds = UserBoundsList::from_str("1,2,2,3").unwrap();
        let error = ForwardBounds::try_from(&bounds).unwrap_err();
        assert_eq!(
            format!("{error}"),
            "Bounds are sorted, but can't be repeated"
        );
    }

    #[test]
    fn test_try_from_non_forward_bounds() {
        let bounds = UserBoundsList::from_str("1,3,2").unwrap();
        let error = ForwardBounds::try_from(&bounds).unwrap_err();
        assert_eq!(
            format!("{error}"),
            "The provided UserBoundsList is not forward only"
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_no_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eol_and_fallbacks() {
        let mut stdout = Vec::new();
        let mut stdin = b"a\n".as_slice();
        let mut opt = make_fields_opt();
        opt.fallback_oob = Some(b"generic fallback".to_vec());
        opt.bounds = ForwardBounds::from_str("{1}-fill-{2}-more fill-{3=last fill}").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(
            stdout.to_str_lossy(),
            b"a-fill-generic fallback-more fill-last fill\n"
                .as_slice()
                .to_str_lossy()
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eol_out_of_bounds() {
        let mut stdout = Vec::new();
        let mut stdin = b"a\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        let res = cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field);
        let error = res.unwrap_err();
        assert_eq!(format!("{error}"), "Out of bounds: 2");
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eof_and_fallbacks() {
        let mut stdout = Vec::new();
        let mut stdin = b"a".as_slice();
        let mut opt = make_fields_opt();
        opt.fallback_oob = Some(b"generic fallback".to_vec());
        opt.bounds = ForwardBounds::from_str("{1}-fill-{2}-more fill-{3=last fill}").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(
            stdout.to_str_lossy(),
            b"a-fill-generic fallback-more fill-last fill\n"
                .as_slice()
                .to_str_lossy()
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_with_eof_out_of_bounds() {
        let mut stdout = Vec::new();
        let mut stdin = b"a".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        let res = cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field);
        let error = res.unwrap_err();
        assert_eq!(format!("{error}"), "Out of bounds: 2");
    }

    #[test]
    fn test_cut_bytes_stream_only_fillers_and_fallbacks() {
        let mut stdout = Vec::new();
        let mut stdin = b"a".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("foo-{2=waitforit}-bar").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(
            stdout.to_str_lossy(),
            b"foo-waitforit-bar\n".as_slice().to_str_lossy()
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_range_right_unlimited_case_1() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1:").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-b-c\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_range_right_unlimited_case_2() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("2:").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"b-c\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_range_left_unlimited_case_1() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str(":1").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_range_left_unlimited_case_2() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str(":2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_range_right_unlimited() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("2:").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"b-c\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_no_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_keep_few_no_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_keep_few_with_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_eol_small_buffer_dropped_fields() {
        let mut stdout = Vec::new();
        let stdin_content = b"a-b-c-d-e-f\ng-h-i-l-m\n".as_slice();
        let mut stdin = BufReader::with_capacity(3, stdin_content);
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\ngh\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_small_buffer_few_no_eol_with_fallbacks() {
        let mut stdout = Vec::new();
        let stdin_content = b"foobar".as_slice();
        let mut stdin = BufReader::with_capacity(2, stdin_content);
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("{9=fallback}").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(
            stdout.to_str_lossy(),
            b"fallback\n".as_slice().to_str_lossy()
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_small_buffer_few_with_eol_with_fallbacks() {
        let mut stdout = Vec::new();
        let stdin_content = b"foobar\n".as_slice();
        let mut stdin = BufReader::with_capacity(2, stdin_content);
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("{9=fallback}").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(
            stdout.to_str_lossy(),
            b"fallback\n".as_slice().to_str_lossy()
        );
    }

    #[test]
    fn test_cut_bytes_stream_cut_simplest_field_multiline_with_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a\nb\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a\nb\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_multiline_with_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b\nc-d\n".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"ab\ncd\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_cut_multiple_fields_with_join_and_eol() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b\n".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_multiple_chunks_sizes() {
        // todo add same test case but with multi-bytes fields (3 bytes fields)
        let stdin_content = b"a-b\n".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.bounds = ForwardBounds::from_str("1,2").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        let mut stdout = Vec::new();
        let mut stdin = BufReader::with_capacity(1, stdin_content);
        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();
        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());

        let mut stdout = Vec::new();
        let mut stdin = BufReader::with_capacity(2, stdin_content);
        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();
        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());

        let mut stdout = Vec::new();
        let mut stdin = BufReader::with_capacity(3, stdin_content);
        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();
        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());

        let mut stdout = Vec::new();
        let mut stdin = BufReader::with_capacity(4, stdin_content);
        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();
        assert_eq!(stdout.to_str_lossy(), b"a-b\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_it_supports_ranges() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.bounds = ForwardBounds::from_str("1:2,3").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-bc\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_it_supports_ranges_with_join_case_1() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.bounds = ForwardBounds::from_str("1:2,3").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-b-c\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_it_supports_ranges_with_join_case_2() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.bounds = ForwardBounds::from_str("1,2:3").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a-b-c\n".as_slice().to_str_lossy());
    }

    #[test]
    fn test_cut_bytes_stream_it_supports_replacing_delimiter_case_1() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.replace_delimiter = Some(b'/');
        opt.bounds = ForwardBounds::from_str("1,2:3").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a/b/c\n".as_slice().to_str_lossy());
    }
    #[test]
    fn test_cut_bytes_stream_it_supports_replacing_delimiter_case_2() {
        let mut stdout = Vec::new();
        let mut stdin = b"a-b-c".as_slice();
        let mut opt = make_fields_opt();
        opt.join = true;
        opt.replace_delimiter = Some(b'/');
        opt.bounds = ForwardBounds::from_str("1,2:").unwrap();
        let last_interesting_field = *opt.bounds.get_last_bound().r();

        cut_bytes_stream(&mut stdin, &mut stdout, &opt, last_interesting_field).unwrap();

        assert_eq!(stdout.to_str_lossy(), b"a/b/c\n".as_slice().to_str_lossy());
    }
}
