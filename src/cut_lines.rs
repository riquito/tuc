use anyhow::Result;
use std::io::{Read, Write};
use std::ops::Range;

use crate::bounds::{BoundOrFiller, Side};
use crate::cut_str::cut_str;
use crate::options::Opt;
use crate::read_utils::read_line_with_eol;

pub fn read_and_cut_lines(
    stdin: &mut std::io::BufReader<std::io::StdinLock>,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    opt: Opt,
) -> Result<()> {
    // If bounds cut from left to right and do not internally overlap
    // (e.g. 1:2,2,4:5,8) then we can use a streaming algorithm and avoid
    // allocating everything in memory.
    let can_be_streamed = {
        !opt.complement
        && !opt.compress_delimiter
        // indexes must be positive (negative indexes require to allocate the whole data)
        && opt.bounds.is_sortable()
        // XXX 2022-05-18 nightly-only && opt.bounds.0.iter().is_sorted()
        && opt.bounds.is_sorted()
    };

    if can_be_streamed {
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
    } else {
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
        )?;
    }

    Ok(())
}
