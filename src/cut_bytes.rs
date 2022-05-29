use anyhow::Result;
use std::io::{Read, Write};

use crate::bounds::{bounds_to_std_range, BoundOrFiller};
use crate::options::Opt;
use crate::read_utils::read_bytes_to_end;

fn cut_bytes<W: Write>(data: &[u8], opt: &Opt, stdout: &mut W) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    opt.bounds.0.iter().try_for_each(|bof| -> Result<()> {
        let output = match bof {
            BoundOrFiller::Bound(b) => {
                let r = bounds_to_std_range(data.len(), b)?;
                &data[r.start..r.end]
            }
            BoundOrFiller::Filler(f) => f.as_bytes(),
        };

        stdout.write_all(output)?;

        Ok(())
    })?;

    Ok(())
}

pub fn read_and_cut_bytes<R: Read, W: Write>(
    stdin: &mut R,
    stdout: &mut W,
    opt: &Opt,
) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(32 * 1024);
    read_bytes_to_end(stdin, &mut buffer);
    cut_bytes(&buffer, opt, stdout)?;
    Ok(())
}
