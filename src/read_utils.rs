use crate::options::EOL;
use std::io::{BufRead, Read};

pub fn read_bytes_to_end<'buf>(
    reader: &mut std::io::BufReader<std::io::StdinLock>,
    buffer: &'buf mut Vec<u8>,
) -> Option<std::io::Result<&'buf mut Vec<u8>>> {
    buffer.clear();

    reader
        .read_to_end(buffer)
        .map(|u| if u == 0 { None } else { Some(buffer) })
        .transpose()
}

pub fn read_line_with_eol<'buf>(
    reader: &mut std::io::BufReader<std::io::StdinLock>,
    buffer: &'buf mut String,
    eol: EOL,
) -> Option<std::io::Result<&'buf mut String>> {
    buffer.clear();

    match eol {
        // read_line is more optimized/safe than read_until for strings
        EOL::Newline => reader.read_line(buffer),
        EOL::Zero => unsafe { reader.read_until(eol as u8, buffer.as_mut_vec()) },
    }
    .map(|u| if u == 0 { None } else { Some(buffer) })
    .transpose()
}
