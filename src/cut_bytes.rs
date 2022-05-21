use anyhow::Result;
use std::io::Write;

use crate::bounds::bounds_to_std_range;
use crate::options::Opt;
use crate::read_utils::read_bytes_to_end;

fn cut_bytes(
    data: &[u8],
    opt: &Opt,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    opt.bounds.0.iter().try_for_each(|f| -> Result<()> {
        let r = bounds_to_std_range(data.len(), f)?;
        let output = &data[r.start..r.end];

        stdout.write_all(output)?;

        Ok(())
    })?;

    Ok(())
}

pub fn read_and_cut_bytes(
    stdin: &mut std::io::BufReader<std::io::StdinLock>,
    stdout: &mut std::io::BufWriter<std::io::StdoutLock>,
    opt: Opt,
) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(32 * 1024);
    read_bytes_to_end(stdin, &mut buffer);
    cut_bytes(&buffer, &opt, stdout)?;
    Ok(())
}
