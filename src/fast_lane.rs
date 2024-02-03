use crate::bounds::{BoundOrFiller, BoundsType, Side, UserBounds, UserBoundsList};
use crate::options::{Opt, EOL};
use anyhow::Result;
use std::convert::TryFrom;
use std::io::{self, BufRead};
use std::{io::Write, ops::Range};

use bstr::io::BufReadExt;

fn cut_str_fast_line<W: Write>(buffer: &[u8], opt: &FastOpt, stdout: &mut W) -> Result<()> {
    if buffer.is_empty() {
        return Ok(());
    }

    let bounds = &opt.bounds;
    assert!(!bounds.0.is_empty());
    // if we're here there must be at least one bound to check
    let last_interesting_field = bounds.0.last().unwrap().end;

    let mut prev_field_start = 0;

    let mut fields: Vec<Range<usize>> = Vec::new();

    let mut curr_field = 0;

    fields.clear();

    for i in memchr::memchr_iter(opt.delimiter, buffer) {
        curr_field += 1;

        let (start, end) = (prev_field_start, i); // end exclusive
        prev_field_start = i + 1;

        fields.push(Range { start, end });

        if curr_field == last_interesting_field {
            // we have no use for this field or any of the following ones
            break;
        }
    }

    if curr_field == 0 && opt.only_delimited {
        // The delimiter was not found
        return Ok(());
    }

    if curr_field != last_interesting_field {
        fields.push(Range {
            start: prev_field_start,
            end: buffer.len(),
        });
    }

    let num_fields = fields.len();

    match num_fields {
        1 if bounds.0.len() == 1 => {
            stdout.write_all(buffer)?;
        }
        _ => {
            bounds
                .0
                .iter()
                .enumerate()
                .try_for_each(|(bounds_idx, b)| -> Result<()> {
                    let is_last = bounds_idx == bounds.0.len() - 1;

                    output_parts(buffer, b, &fields, stdout, is_last, opt)
                })?;
        }
    }

    stdout.write_all(&[b'\n'])?;

    Ok(())
}

#[inline]
fn output_parts<W: Write>(
    line: &[u8],
    // which parts to print
    r: &Range<usize>,
    // where to find the parts inside `line`
    fields: &[Range<usize>],
    stdout: &mut W,
    is_last: bool,
    opt: &FastOpt,
) -> Result<()> {
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

pub struct FastOpt {
    delimiter: u8,
    join: bool,
    eol: u8,
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
            || matches!(value.eol, EOL::Zero)
        {
            return Err(
                "FastOpt supports solely forward fields, join and single-character delimiters",
            );
        }

        if let Ok(forward_bounds) = ForwardBounds::try_from(&value.bounds) {
            Ok(FastOpt {
                delimiter: value.delimiter.as_bytes().first().unwrap().to_owned(),
                join: value.join,
                eol: b'\n',
                bounds: forward_bounds,
                only_delimited: value.only_delimited,
            })
        } else {
            Err("Bounds cannot be converted to ForwardBounds")
        }
    }
}

impl From<&UserBounds> for Range<usize> {
    fn from(value: &UserBounds) -> Self {
        // XXX this will explode in our face at the first negative value
        // XXX we should have a try into and more checks in place
        // (also, values must be sequential, but that should be covered by UserBounds
        // ... if we will still pass by it)

        let (l, r): (usize, usize) = match (value.l, value.r) {
            (Side::Some(l), Side::Some(r)) => ((l - 1) as usize, r as usize),
            (Side::Some(l), Side::Continue) => ((l - 1) as usize, usize::MAX),
            (Side::Continue, Side::Some(r)) => (0, r as usize),
            (Side::Continue, Side::Continue) => (0, usize::MAX),
        };

        Range { start: l, end: r }
    }
}

#[derive(Debug)]
pub struct ForwardBounds(Vec<Range<usize>>);

impl TryFrom<&UserBoundsList> for ForwardBounds {
    type Error = &'static str;

    fn try_from(value: &UserBoundsList) -> Result<Self, Self::Error> {
        if value.is_forward_only() {
            let mut v: Vec<Range<usize>> = Vec::with_capacity(value.0.len());
            for maybe_bounds in value.0.iter() {
                // XXX for now let's drop the fillers
                // XXX TODO

                if let BoundOrFiller::Bound(bounds) = maybe_bounds {
                    v.push(bounds.into());
                }
            }
            Ok(ForwardBounds(v))
        } else {
            Err("The provided UserBoundsList is not forward only")
        }
    }
}

pub fn read_and_cut_text_as_bytes<R: BufRead, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &FastOpt,
) -> Result<()> {
    stdin.for_byte_line(|line| {
        let mut fields: Vec<Range<usize>> = Vec::with_capacity(16);
        cut_str_fast_line(line, opt, stdout, &mut fields)
            // XXX Should map properly the error
            .map_err(|x| io::Error::new(io::ErrorKind::Other, x.to_string()))
            .and(Ok(true))
    })?;

    Ok(())
}
