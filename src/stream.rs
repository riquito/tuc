use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList, UserBoundsTrait};
use crate::options::{Opt, EOL};
use anyhow::Result;
use bstr::ByteSlice;
use std::convert::TryFrom;
use std::io::BufRead;
use std::io::Write;
use std::ops::Deref;

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
    join: bool,
    eol: EOL,
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
        if !value.delimiter.as_bytes().len() == 1 {
            return Err("Delimiter must be 1 byte wide for FastOpt");
        }

        if value.complement
            || value.greedy_delimiter
            || value.compress_delimiter
            || value.json
            || value.bounds_type != BoundsType::Fields
            || value.replace_delimiter.is_some()
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
                join: value.join,
                eol: value.eol,
                bounds: forward_bounds,
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
    let last_interesting_field = opt.bounds.get_last_bound().r;
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

fn cut_bytes_stream<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &StreamOpt,
    last_interesting_field: Side,
) -> Result<()> {
    //let mut buffer: Vec<u8> = Vec::with_capacity(64 * 1024);

    let eol: u8 = opt.eol.into();

    // With this algorithm we can only move forward, so we can't
    // support overlapping ranges
    // XXX TODO panic (or move to something different than StreamOpt)

    'outer: loop {
        // new line
        //     dbg!("new line");

        let mut bounds_idx = 0;
        let mut available;

        let mut curr_field = 0;
        let mut go_to_next_line = false;
        let mut field_is_continuation = false;

        let mut used;

        'fields: loop {
            available = stdin.fill_buf()?;
            //let tmp = available.to_str_lossy();

            if available.is_empty() {
                // end of file
                stdout.write_all(&[opt.eol.into()])?;
                break 'outer;
            }

            let mut prev_idx = 0;
            for idx in memchr::memchr2_iter(opt.delimiter, eol, available) {
                used = idx + 1;

                curr_field += 1;

                if let Some(BoundOrFiller::Bound(b)) = opt.bounds.get(bounds_idx) {
                    // TODO creates a dedicated match function
                    if b.matches(curr_field).unwrap() {
                        if field_is_continuation && idx == 0 {
                            bounds_idx += 1;
                        } else {
                            print_field(
                                stdout,
                                &available[prev_idx..idx],
                                //opt.delimiter,
                                b'\t',
                                !field_is_continuation
                                    && curr_field > 1
                                    && (opt.join || (b.l != b.r || b.r == Side::Continue)),
                            )?;

                            if b.r == Side::Some(curr_field) {
                                bounds_idx += 1;
                            }
                        }
                    }
                }

                field_is_continuation = false;

                prev_idx = idx + 1;

                if available[idx] == eol {
                    // end of line reached
                    break 'fields;
                }

                if Side::Some(curr_field) == last_interesting_field {
                    // There are no more fields we're interested in,
                    // let's move to the next line
                    go_to_next_line = true;
                    break 'fields;
                }
            }

            // We exhausted the buffer before reaching the next line, so
            // - there could be more fields to read
            // - the last byte was likely not a delimiter and there is the
            //   start of a field still in the buffer

            if !available[prev_idx..].is_empty() {
                curr_field += 1;

                if let Some(BoundOrFiller::Bound(b)) = opt.bounds.get(bounds_idx) {
                    if b.matches(curr_field).unwrap() {
                        print_field(
                            stdout,
                            &available[prev_idx..],
                            // opt.delimiter, false)?;
                            b'\t',
                            !field_is_continuation && curr_field > 1 && (opt.join),
                        )?;
                    }
                }

                // the field was split in two parts, let's reset its counter
                curr_field -= 1;
                field_is_continuation = true;
            }

            // We keep `curr_field` as-is, consume the buffer and read the next block

            used = available.len();
            stdin.consume(used);
        }

        // We consumed every field we were interested in in this line

        if go_to_next_line {
            let mut idx = used - 1; // remove one. We know it wasn't a newline and
                                    // it ensure that the buffer is not empty during the first loop

            // let mut must_read_more = true;
            loop {
                //if !must_read_more && available[idx..].is_empty() {
                if available[idx..].is_empty() {
                    stdout.write_all(&[opt.eol.into()])?;
                    break 'outer;
                }

                if let Some(eol_idx) = memchr::memchr(eol, &available[idx..]) {
                    used = idx + eol_idx + 1;
                    break;
                }

                // Whops, eol was not found in the current buffer. Let's read some more

                used = available.len();
                stdin.consume(used);
                available = stdin.fill_buf()?;
                idx = 0;
                //must_read_more = true;
            }
        }

        stdin.consume(used);

        stdout.write_all(&[opt.eol.into()])?;
    }

    Ok(())
}
