use anyhow::Result;
use memmap2::Mmap;
use std::convert::TryFrom;
use std::ffi::OsString;
use std::io::Write;
use tuc::args;
use tuc::bounds::BoundsType;
use tuc::cut_bytes::read_and_cut_bytes;
use tuc::cut_lines::read_and_cut_lines;
use tuc::cut_str::read_and_cut_str;
use tuc::help::{get_help, get_short_help};
use tuc::options::Opt;
use tuc::stream::{StreamOpt, read_and_cut_bytes_stream};

#[cfg(feature = "fast-lane")]
use tuc::fast_lane::{FastOpt, read_and_cut_text_as_bytes};

#[cfg(not(feature = "fast-lane"))]
struct FastOpt {}

#[cfg(not(feature = "fast-lane"))]
impl<'a> TryFrom<&'a Opt> for FastOpt {
    type Error = &'static str;

    fn try_from(_value: &'a Opt) -> Result<Self, Self::Error> {
        Err("This binary was not compiled with the feature fast-lane")
    }
}

#[cfg(not(feature = "fast-lane"))]
fn read_and_cut_text_as_bytes<R: std::io::BufRead, W: Write>(
    _stdin: &mut R,
    _stdout: &mut W,
    _fast_opt: &FastOpt,
) -> Result<()> {
    Err(anyhow::Error::msg(
        "This binary was not compiled with the feature fast-lane",
    ))
}

fn run() -> Result<()> {
    if std::env::args_os().len() == 1 {
        print!("{}", get_short_help());
        std::process::exit(0);
    }

    let mut raw_args: Vec<OsString> = std::env::args_os().collect();
    // remove executable path
    raw_args.remove(0);

    let maybe_args = args::parse_args(raw_args);

    if let Err(error) = maybe_args {
        match error {
            args::ArgsParseError::HelpRequested => {
                print!("{}", get_help());
                std::process::exit(0);
            }
            args::ArgsParseError::VersionRequested => {
                println!("tuc {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            args::ArgsParseError::PicoArgs(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    let args = maybe_args.expect("We already handled the possible errors");
    let opt: Opt = args
        .try_into()
        .map_err(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        })
        .unwrap();

    let mut stdout = std::io::BufWriter::with_capacity(64 * 1024, std::io::stdout().lock());

    let mmap;
    let mut mmap_cursor;
    let mut file_reader;
    let mut stdin;

    let mut reader: &mut dyn std::io::BufRead = if opt.path.is_some() {
        let file = std::fs::File::open(opt.path.as_ref().unwrap()).map_err(|e| {
            let path = opt.path.as_ref().unwrap();
            anyhow::anyhow!("{}.\nWas attempting to read {:?}", e, &path)
        })?;

        if opt.use_mmap {
            mmap = unsafe { Mmap::map(&file)? };
            mmap_cursor = std::io::Cursor::new(&mmap[..]);
            &mut mmap_cursor
        } else {
            file_reader = std::io::BufReader::with_capacity(64 * 1024, file);
            &mut file_reader
        }
    } else {
        stdin = std::io::BufReader::with_capacity(64 * 1024, std::io::stdin().lock());
        &mut stdin
    };

    if opt.fixed_memory.is_some() {
        let stream_opt = StreamOpt::try_from(&opt).unwrap_or_else(|e| {
            eprintln!("tuc: runtime error. {e}");
            std::process::exit(1);
        });
        read_and_cut_bytes_stream(&mut reader, &mut stdout, &stream_opt)?;
        return Ok(());
    }

    if opt.bounds_type == BoundsType::Bytes {
        read_and_cut_bytes(&mut reader, &mut stdout, &opt)?;
    } else if opt.bounds_type == BoundsType::Lines {
        read_and_cut_lines(&mut reader, &mut stdout, &opt)?;
    } else if let Ok(fast_opt) = FastOpt::try_from(&opt) {
        read_and_cut_text_as_bytes(&mut reader, &mut stdout, &fast_opt)?;
    } else {
        read_and_cut_str(&mut reader, &mut stdout, &opt)?;
    }

    stdout.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    if let Err(e) = run() {
        if let Some(io_error) = e.downcast_ref::<std::io::Error>()
            && io_error.kind() == std::io::ErrorKind::BrokenPipe
        {
            std::process::exit(0);
        }

        Err(e)
    } else {
        Ok(())
    }
}
