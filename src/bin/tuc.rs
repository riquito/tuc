use anyhow::Result;
use std::convert::TryFrom;
use std::env::args;
use std::io::Write;
use std::str::FromStr;
use tuc::bounds::{BoundOrFiller, BoundsType, UserBoundsList};
use tuc::cut_bytes::read_and_cut_bytes;
use tuc::cut_lines::read_and_cut_lines;
use tuc::cut_str::read_and_cut_str;
use tuc::fast_lane::{read_and_cut_text_as_bytes, FastOpt};
use tuc::help::{get_help, get_short_help};
use tuc::options::{Opt, EOL};

#[cfg(feature = "regex")]
use tuc::options::RegexBag;

#[cfg(feature = "regex")]
use regex::bytes::Regex;

fn parse_args() -> Result<Opt, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if args().len() == 1 {
        print!("{}", get_short_help());
        std::process::exit(0);
    }

    if pargs.contains(["-h", "--help"]) {
        print!("{}", get_help());
        std::process::exit(0);
    }

    let mut maybe_fields: Option<UserBoundsList> = pargs.opt_value_from_str(["-f", "--fields"])?;
    let maybe_characters: Option<UserBoundsList> =
        pargs.opt_value_from_str(["-c", "--characters"])?;
    let maybe_bytes: Option<UserBoundsList> = pargs.opt_value_from_str(["-b", "--bytes"])?;
    let maybe_lines: Option<UserBoundsList> = pargs.opt_value_from_str(["-l", "--lines"])?;

    let bounds_type = if maybe_fields.is_some() {
        BoundsType::Fields
    } else if maybe_bytes.is_some() {
        BoundsType::Bytes
    } else if maybe_characters.is_some() {
        BoundsType::Characters
    } else if maybe_lines.is_some() {
        BoundsType::Lines
    } else {
        maybe_fields = Some(UserBoundsList::from_str("1:").unwrap());
        BoundsType::Fields
    };

    if bounds_type == BoundsType::Fields
        && (maybe_fields.is_none() || maybe_fields.as_ref().unwrap().is_empty())
    {
        eprintln!("tuc: invariant error. At this point we expected to find at least 1 field bound");
        std::process::exit(1);
    }

    let delimiter: Vec<u8> = match bounds_type {
        BoundsType::Fields => pargs
            .opt_value_from_str(["-d", "--delimiter"])?
            .map(|x: String| x.into())
            .unwrap_or_else(|| "\t".into()),
        BoundsType::Lines => "\n".into(),
        _ => Vec::new(),
    };

    let greedy_delimiter = pargs.contains(["-g", "--greedy-delimiter"]);
    let tmp_replace_delimiter: Option<String> =
        pargs.opt_value_from_str(["-r", "--replace-delimiter"])?;
    let mut replace_delimiter: Option<Vec<u8>> = tmp_replace_delimiter.map(|x| x.into());

    let has_json = pargs.contains("--json");
    let has_join = pargs.contains(["-j", "--join"]);
    let has_no_join = pargs.contains("--no-join");

    if has_join && has_no_join {
        eprintln!(
            "tuc: runtime error. It's not possible to use --join and --no-join simultaneously"
        );
        std::process::exit(1);
    }

    if has_json && has_no_join {
        eprintln!("tuc: runtime error. Using both --json and --no-join is not permitted");
        std::process::exit(1);
    }

    if replace_delimiter.is_some() {
        if has_no_join {
            eprintln!("tuc: runtime error. You can't pass --no-join when using --replace, which implies --join");
            std::process::exit(1);
        } else if has_json {
            eprintln!("tuc: runtime error. The use of --replace with --json is not supported");
            std::process::exit(1);
        }
    }

    if bounds_type == BoundsType::Characters && has_no_join {
        eprintln!(
            "tuc: runtime error. Since --characters implies --join, you can't pass --no-join"
        );
        std::process::exit(1);
    }

    if bounds_type == BoundsType::Characters && cfg!(not(feature = "regex")) {
        eprintln!(
            "tuc: runtime error. The use of --characters requires `tuc` to be compiled with `regex` support"
        );
        std::process::exit(1);
    }

    if bounds_type == BoundsType::Characters {
        replace_delimiter = Some("".into());
    }

    if has_json {
        replace_delimiter = Some(",".into());
    }

    let join = has_join
        || has_json
        || replace_delimiter.is_some()
        || (bounds_type == BoundsType::Lines && !has_no_join)
        || (bounds_type == BoundsType::Characters);

    if has_json && bounds_type != BoundsType::Characters && bounds_type != BoundsType::Fields {
        eprintln!(
            "tuc: runtime error. --json support is available only for --fields and --characters"
        );
        std::process::exit(1);
    }

    #[cfg(not(feature = "regex"))]
    let regex_bag = None;

    #[cfg(feature = "regex")]
    let regex_bag: Option<RegexBag> = (if bounds_type == BoundsType::Characters {
        Some("\\b|\\B".to_owned())
    } else {
        pargs.opt_value_from_str::<_, String>(["-e", "--regex"])?
    })
    .map(|regex_text| RegexBag {
        normal: Regex::new(&regex_text).unwrap_or_else(|e| {
            eprintln!("tuc: runtime error. The regular expression is malformed. {e}");
            std::process::exit(1);
        }),
        greedy: Regex::new(&format!("({})+", &regex_text)).unwrap_or_else(|e| {
            eprintln!("tuc: runtime error. The regular expression is malformed. {e}");
            std::process::exit(1);
        }),
    });

    if regex_bag.is_some() && cfg!(not(feature = "regex")) {
        eprintln!("tuc: invariant error. There should not be any regex when compiled without regex support");
        std::process::exit(1);
    }

    let bounds = maybe_fields
        .or(maybe_characters)
        .or(maybe_bytes)
        .or(maybe_lines)
        .unwrap();

    if has_json && bounds.iter().any(|s| matches!(s, BoundOrFiller::Filler(_))) {
        eprintln!("tuc: runtime error. Cannot format fields when using --json");
        std::process::exit(1);
    }

    let args = Opt {
        complement: pargs.contains(["-m", "--complement"]),
        only_delimited: pargs.contains(["-s", "--only-delimited"]),
        greedy_delimiter,
        compress_delimiter: pargs.contains(["-p", "--compress-delimiter"]),
        version: pargs.contains(["-V", "--version"]),
        eol: if pargs.contains(["-z", "--zero-terminated"]) {
            EOL::Zero
        } else {
            EOL::Newline
        },
        join,
        json: has_json,
        delimiter,
        bounds_type,
        bounds,
        replace_delimiter,
        trim: pargs.opt_value_from_str(["-t", "--trim"])?,
        fallback_oob: pargs
            .opt_value_from_str("--fallback-oob")?
            .map(|x: String| x.into()),
        regex_bag,
    };

    let remaining = pargs.finish();

    if args.version {
        println!("tuc {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if !remaining.is_empty() {
        eprintln!("tuc: unexpected arguments {remaining:?}");
        eprintln!("Try 'tuc --help' for more information.");
        std::process::exit(1);
    }

    Ok(args)
}

fn main() -> Result<()> {
    let opt: Opt = parse_args()?;

    let mut stdin = std::io::BufReader::with_capacity(64 * 1024, std::io::stdin().lock());
    let mut stdout = std::io::BufWriter::with_capacity(64 * 1024, std::io::stdout().lock());

    if opt.bounds_type == BoundsType::Bytes {
        read_and_cut_bytes(&mut stdin, &mut stdout, &opt)?;
    } else if opt.bounds_type == BoundsType::Lines {
        read_and_cut_lines(&mut stdin, &mut stdout, &opt)?;
    } else if let Ok(fast_opt) = FastOpt::try_from(&opt) {
        read_and_cut_text_as_bytes(&mut stdin, &mut stdout, &fast_opt)?;
    } else {
        read_and_cut_str(&mut stdin, &mut stdout, opt)?;
    }

    stdout.flush()?;

    Ok(())
}
