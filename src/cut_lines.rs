use anyhow::{bail, Result};
use std::io::{BufRead, Write};
use std::ops::Range;

use crate::bounds::{BoundOrFiller, Side};
use crate::cut_str::cut_str;
use crate::options::Opt;
use crate::read_utils::read_line_with_eol;

fn cut_lines_forward_only<A: BufRead, B: Write>(
    stdin: &mut A,
    stdout: &mut B,
    opt: Opt,
) -> Result<()> {
    let mut line_buf = String::with_capacity(1024);
    let mut line_idx = 0;
    let mut bounds_idx = 0; // keep track of which bounds have been used
    let mut add_newline_next = false;
    while let Some(line) = read_line_with_eol(stdin, &mut line_buf, opt.eol) {
        line_idx += 1;

        let line = line?;
        let line: &str = line.as_ref();
        let line = line.strip_suffix(opt.eol as u8 as char).unwrap_or(line);

        // Print the matching fields. Fields are ordered but can still be
        // duplicated, e.g. 1-2,2,3 , so we may have to print the same
        // line multiple times
        while bounds_idx < opt.bounds.0.len() {
            let bof = opt.bounds.0.get(bounds_idx).unwrap();

            let b = match bof {
                BoundOrFiller::Filler(f) => {
                    stdout.write_all(f.as_bytes())?;
                    bounds_idx += 1;
                    continue;
                }
                BoundOrFiller::Bound(b) => b,
            };

            if b.matches(line_idx).unwrap_or(false) {
                if add_newline_next {
                    stdout.write_all(&[opt.eol as u8])?;
                }

                stdout.write_all(line.as_bytes())?;
                add_newline_next = true;

                if b.r == Side::Some(line_idx) {
                    // we exhausted the use of that bound, move on
                    bounds_idx += 1;
                    add_newline_next = false;

                    // if opt.join and it was not the last matching bound
                    if opt.join && bounds_idx != opt.bounds.0.len() {
                        stdout.write_all(&[opt.eol as u8])?;
                    }

                    continue; // let's see if the next bound matches too
                }
            }

            break; // nothing matched, let's go to the next line
        }

        if bounds_idx == opt.bounds.0.len() {
            // no need to read the rest, we don't have other bounds to test
            break;
        }
    }

    // Outout is finished. Did we output every bound?
    if let Some(BoundOrFiller::Bound(b)) = opt.bounds.0.get(bounds_idx) {
        // not good, we still have bounds to print but the input is exhausted
        bail!("Out of bounds: {}", b);
    }

    Ok(())
}

fn cut_lines<A: BufRead, B: Write>(stdin: &mut A, stdout: &mut B, opt: Opt) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(32 * 1024);
    stdin.read_to_end(&mut buffer)?;
    let buffer_as_str = std::str::from_utf8(&buffer)?;
    let mut bounds_as_ranges: Vec<Range<usize>> = Vec::with_capacity(100);
    let mut compressed_line_buf = String::new();

    // Just use cut_str, we're cutting a (big) string whose delimiter is newline
    cut_str(
        buffer_as_str,
        &opt,
        stdout,
        &mut bounds_as_ranges,
        &mut compressed_line_buf,
        b"",
    )
}

pub fn read_and_cut_lines<A: BufRead, B: Write>(
    stdin: &mut A,
    stdout: &mut B,
    opt: Opt,
) -> Result<()> {
    // If bounds cut from left to right and do not internally overlap
    // (e.g. 1:2,2,4:5,8) then we can use a streaming algorithm and avoid
    // allocating everything in memory.
    let can_be_streamed =
        { !opt.complement && !opt.compress_delimiter && opt.bounds.is_forward_only() };

    if can_be_streamed {
        cut_lines_forward_only(stdin, stdout, opt)?;
    } else {
        cut_lines(stdin, stdout, opt)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::bounds::{BoundsType, UserBounds, UserBoundsList};

    use super::*;

    fn make_lines_opt() -> Opt {
        Opt {
            bounds_type: BoundsType::Lines,
            delimiter: String::from("\n"),
            ..Opt::default()
        }
    }

    const BOF_F1: BoundOrFiller = BoundOrFiller::Bound(UserBounds {
        l: Side::Some(1),
        r: Side::Some(1),
    });

    const BOF_F2: BoundOrFiller = BoundOrFiller::Bound(UserBounds {
        l: Side::Some(2),
        r: Side::Some(2),
    });

    const BOF_F3: BoundOrFiller = BoundOrFiller::Bound(UserBounds {
        l: Side::Some(3),
        r: Side::Some(3),
    });

    const BOF_R2_3: BoundOrFiller = BoundOrFiller::Bound(UserBounds {
        l: Side::Some(2),
        r: Side::Some(3),
    });

    const BOF_NEG1: BoundOrFiller = BoundOrFiller::Bound(UserBounds {
        l: Side::Some(-1),
        r: Side::Some(-1),
    });

    #[test]
    fn fwd_cut_one_field() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_F1]);

        let mut input = b"a\nb".as_slice();
        let mut output = Vec::with_capacity(100);
        cut_lines_forward_only(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"a");
    }

    #[test]
    fn fwd_cut_multiple_fields() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_F1, BOF_F2]);

        let mut input = b"a\nb".as_slice();
        let mut output = Vec::with_capacity(100);
        cut_lines_forward_only(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"ab");
    }

    #[test]
    fn fwd_support_ranges() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_F1, BOF_R2_3]);

        let mut input = b"a\nb\nc".as_slice();
        let mut output = Vec::with_capacity(100);
        cut_lines_forward_only(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"ab\nc");
    }

    #[test]
    fn fwd_supports_join() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_F1, BOF_F3]);
        opt.join = true;

        let mut input = b"a\nb\nc".as_slice();
        let mut output = Vec::with_capacity(100);
        cut_lines_forward_only(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"a\nc");
    }

    #[test]
    fn fwd_handle_out_of_bounds() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_F3]);
        opt.join = true;

        let mut input = b"a\nb".as_slice();
        let mut output = Vec::with_capacity(100);
        let res = cut_lines_forward_only(&mut input, &mut output, opt);
        assert_eq!(res.unwrap_err().to_string(), "Out of bounds: 3");
    }

    #[test]
    fn cut_lines_handle_negative_idx() {
        let mut opt = make_lines_opt();
        opt.bounds = UserBoundsList(vec![BOF_NEG1]);

        let mut input = b"a\nb".as_slice();
        let mut output = Vec::with_capacity(100);
        cut_lines(&mut input, &mut output, opt).unwrap();
        assert_eq!(output, b"b");
    }
}
