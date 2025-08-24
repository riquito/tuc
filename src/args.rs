use crate::{bounds::UserBoundsList, options::Trim};
use anyhow::Result;
use std::{ffi::OsString, path::PathBuf};

/**
 * Args represent what the user requested.
 * It may very well represent a non-working configuration.
 */
#[derive(Debug)]
pub struct Args {
    pub cut_by_fields: Option<UserBoundsList>,
    pub cut_by_characters: Option<UserBoundsList>,
    pub cut_by_bytes: Option<UserBoundsList>,
    pub cut_by_lines: Option<UserBoundsList>,
    pub delimiter: Option<Vec<u8>>,
    pub replace_delimiter: Option<Vec<u8>>,
    pub trim: Option<Trim>,
    pub fixed_memory_kb: Option<usize>,
    pub fallback_oob: Option<Vec<u8>>,
    pub path: Option<PathBuf>,
    pub regex: Option<String>,
    pub complement: bool,
    pub compress_delimiter: bool,
    pub greedy_delimiter: bool,
    pub join_yes: bool,
    pub join_no: bool,
    pub json: bool,
    pub mmap_no: bool,
    pub only_delimited: bool,
    pub zero_terminated: bool,
}

pub fn parse_args(args: Vec<OsString>) -> Result<Args, ArgsParseError> {
    let mut pargs = pico_args::Arguments::from_vec(args);

    if pargs.contains(["-h", "--help"]) {
        return Err(ArgsParseError::HelpRequested);
    }

    if pargs.contains(["-V", "--version"]) {
        return Err(ArgsParseError::VersionRequested);
    }

    let cut_by_fields: Option<UserBoundsList> = pargs.opt_value_from_str(["-f", "--fields"])?;
    let cut_by_characters: Option<UserBoundsList> =
        pargs.opt_value_from_str(["-c", "--characters"])?;
    let cut_by_bytes: Option<UserBoundsList> = pargs.opt_value_from_str(["-b", "--bytes"])?;
    let cut_by_lines: Option<UserBoundsList> = pargs.opt_value_from_str(["-l", "--lines"])?;

    let delimiter = pargs
        .opt_value_from_str(["-d", "--delimiter"])?
        .map(|x: String| x.into());

    let greedy_delimiter = pargs.contains(["-g", "--greedy-delimiter"]);
    let tmp_replace_delimiter: Option<String> =
        pargs.opt_value_from_str(["-r", "--replace-delimiter"])?;
    let replace_delimiter: Option<Vec<u8>> = tmp_replace_delimiter.map(|x| x.into());

    let fixed_memory_kb: Option<usize> = pargs.opt_value_from_str(["-M", "--fixed-memory"])?;

    let has_json = pargs.contains("--json");

    let join_yes = pargs.contains(["-j", "--join"]);
    let join_no = pargs.contains("--no-join");

    #[cfg(not(feature = "regex"))]
    let regex = None;

    #[cfg(feature = "regex")]
    let regex = pargs.opt_value_from_str::<_, String>(["-e", "--regex"])?;

    let complement = pargs.contains(["-m", "--complement"]);
    let only_delimited = pargs.contains(["-s", "--only-delimited"]);
    let compress_delimiter = pargs.contains(["-p", "--compress-delimiter"]);
    let trim: Option<Trim> = pargs.opt_value_from_str(["-t", "--trim"])?;

    let zero_terminated = pargs.contains(["-z", "--zero-terminated"]);

    let fallback_oob: Option<Vec<u8>> = pargs
        .opt_value_from_str("--fallback-oob")
        .or_else(|e| match e {
            pico_args::Error::OptionWithoutAValue(_) => {
                // We must consume the arg ourselves (it's not done on error)
                pargs.contains("--fallback-oob=");

                Ok(Some("".into()))
            }
            _ => Err(e),
        })?
        .map(|x: String| x.into());

    // Use mmap if there's a file to open and it's not macOS (performance reasons)
    let mmap_no = pargs.contains("--no-mmap");

    // We read all the options. We can still have (one) free argument
    let remaining = pargs.finish();

    if remaining.len() > 1 {
        eprintln!("tuc: unexpected arguments: {remaining:?}");
        eprintln!("Try 'tuc --help' for more information.");
        std::process::exit(1);
    }

    let path = remaining
        .first()
        .and_then(|x| x.to_str())
        .map(PathBuf::from);

    if let Some(some_path) = path.as_ref() {
        if !some_path.exists() {
            // Last argument should be a path, but if it looks like an option
            // (e.g. starts with a dash), we print a dedicated error message.
            if some_path.as_path().to_string_lossy().starts_with("-") {
                eprintln!("tuc: unexpected arguments: {remaining:?}");
                eprintln!("Try 'tuc --help' for more information.");
                std::process::exit(1);
            }

            eprintln!("tuc: runtime error. The file {some_path:?} does not exist");
            std::process::exit(1);
        }

        if !some_path.is_file() {
            eprintln!("tuc: runtime error. The path {some_path:?} is not a file");
            std::process::exit(1);
        }
    }

    let args = Args {
        cut_by_fields,
        cut_by_characters,
        cut_by_bytes,
        cut_by_lines,
        complement,
        only_delimited,
        greedy_delimiter,
        compress_delimiter,
        zero_terminated,
        join_yes,
        join_no,
        json: has_json,
        fixed_memory_kb,
        delimiter,
        replace_delimiter,
        trim,
        fallback_oob,
        regex,
        path,
        mmap_no,
    };

    Ok(args)
}

#[derive(Debug)]
pub enum ArgsParseError {
    PicoArgs(pico_args::Error),
    HelpRequested,
    VersionRequested,
}

impl std::fmt::Display for ArgsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ArgsParseError::PicoArgs(e) => write!(f, "Argument parsing error: {}", e),
            ArgsParseError::HelpRequested => write!(f, "Help requested"),
            ArgsParseError::VersionRequested => write!(f, "Version requested"),
        }
    }
}

impl std::error::Error for ArgsParseError {}

// Automatic conversion from pico_args::Error
impl From<pico_args::Error> for ArgsParseError {
    fn from(error: pico_args::Error) -> Self {
        ArgsParseError::PicoArgs(error)
    }
}
