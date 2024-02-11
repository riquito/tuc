use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList};
use crate::options::{Opt, EOL};
use anyhow::Result;
use std::convert::TryFrom;
use std::io::{self, BufRead};
use std::ops::Deref;
use std::{io::Write, ops::Range};

use bstr::io::BufReadExt;

fn cut_str_fast_line<W: Write>(
    buffer: &[u8],
    opt: &FastOpt,
    stdout: &mut W,
    fields: &mut Vec<Range<usize>>,
    last_interesting_field: Side,
) -> Result<()> {
    if buffer.is_empty() {
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
        1 if bounds.len() == 1 => {
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

    // let field_to_print = maybe_replace_delimiter(output, opt);
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
            || value.trim.is_some()
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
            })
        } else {
            Err("Bounds cannot be converted to ForwardBounds")
        }
    }
}

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
            let value: UserBoundsList = UserBoundsList(value.iter().cloned().collect());
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

pub fn read_and_cut_text_as_bytes<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &FastOpt,
) -> Result<()> {
    let mut fields: Vec<Range<usize>> = Vec::with_capacity(16);

    // ForwardBounds guarantees that there is at least one field to check
    let last_interesting_field = opt.bounds.get_last_bound().r;

    match opt.eol {
        EOL::Newline => stdin.for_byte_line(|line| {
            cut_str_fast_line(line, opt, stdout, &mut fields, last_interesting_field)
                // XXX Should map properly the error
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x.to_string()))
                .and(Ok(true))
        })?,
        EOL::Zero => stdin.for_byte_record(opt.eol.into(), |line| {
            cut_str_fast_line(line, opt, stdout, &mut fields, last_interesting_field)
                // XXX Should map properly the error
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x.to_string()))
                .and(Ok(true))
        })?,
    }

    Ok(())
}
