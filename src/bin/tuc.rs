use anyhow::Result;
use std::io::Write;
use std::str::FromStr;
use tuc::bounds::{BoundOrFiller, BoundsType, UserBoundsList};
use tuc::cut_bytes::read_and_cut_bytes;
use tuc::cut_lines::read_and_cut_lines;
use tuc::cut_str::read_and_cut_str;
use tuc::options::{Opt, EOL};

#[cfg(feature = "regex")]
use tuc::options::RegexBag;

#[cfg(feature = "regex")]
use regex::Regex;

const HELP: &str = concat!(
    "tuc ",
    env!("CARGO_PKG_VERSION"),
    r#"
Cut text (or bytes) where a delimiter matches, then keep the desired parts.

The data is read from standard input.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -g, --greedy-delimiter        Match consecutive delimiters as if it was one
    -p, --compress-delimiter      Print only the first delimiter of a sequence
    -s, --only-delimited          Print only lines containing the delimiter
    -V, --version                 Print version information
    -z, --zero-terminated         Line delimiter is NUL (\0), not LF (\n)
    -h, --help                    Print this help and exit
    -m, --complement              Invert fields (e.g. '2' becomes '1,3:')
    -j, --(no-)join               Print selected parts with delimiter in between
    --json                        Print fields as a JSON array of strings

OPTIONS:
    -f, --fields <bounds>         Fields to keep, 1-indexed, comma separated.
                                  Use colon to include everything in a range.
                                  Fields can be negative (-1 is the last field).
                                  [default 1:]

                                  e.g. cutting on '-' the string 'a-b-c-d'
                                  -f 1     => a
                                  -f 1:    => a-b-c-d
                                  -f 1:3   => a-b-c
                                  -f 3,2   => cb
                                  -f 3,1:2 => ca-b
                                  -f -3:-2 => b-c

                                  To re-apply the delimiter add -j, to replace
                                  it add -r (followed by the new delimiter).

                                  You can also format the output using {} syntax
                                  e.g.
                                  -f '["{1}", "{2}"]' => ["a", "b"]

                                  You can escape { and } using {{ and }}.

    -b, --bytes <bounds>          Same as --fields, but it keeps bytes
    -c, --characters <bounds>     Same as --fields, but it keeps characters
    -l, --lines <bounds>          Same as --fields, but it keeps lines
                                  Implies --join. To merge lines, use --no-join
    -d, --delimiter <delimiter>   Delimiter used by --fields to cut the text
                                  [default: \t]
    -e, --regex <some regex>      Use a regular expression as delimiter
    -r, --replace-delimiter <new> Replace the delimiter with the provided text.
                                  Implies --join
    -t, --trim <type>             Trim the delimiter (greedy). Valid values are
                                  (l|L)eft, (r|R)ight, (b|B)oth

Options precedence:
    --trim and --compress-delimiter are applied before --fields or similar

Memory consumption:
    --characters and --fields read and allocate memory one line at a time

    --lines allocate memory one line at a time as long as the requested fields
    are ordered and non-negative (e.g. -l 1,3:4,4,7), otherwise it allocates
    the whole input in memory (it also happens when -p or -m are being used)

    --bytes allocate the whole input in memory
"#
);

fn parse_args() -> Result<Opt, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{HELP}");
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
        && (maybe_fields.is_none() || maybe_fields.as_ref().unwrap().0.is_empty())
    {
        eprintln!("tuc: invariant error. At this point we expected to find at least 1 field bound");
        std::process::exit(1);
    }

    let delimiter = match bounds_type {
        BoundsType::Fields => pargs
            .opt_value_from_str(["-d", "--delimiter"])?
            .unwrap_or_else(|| String::from('\t')),
        BoundsType::Lines => String::from("\n"),
        _ => String::new(),
    };

    let greedy_delimiter = pargs.contains(["-g", "--greedy-delimiter"]);
    let mut replace_delimiter = pargs.opt_value_from_str(["-r", "--replace-delimiter"])?;

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
    }

    if has_json {
        replace_delimiter = Some(",".to_owned());
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
    let regex_bag: Option<RegexBag> = pargs
        .opt_value_from_str::<_, String>(["-e", "--regex"])?
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

    if regex_bag.is_some() && !cfg!(feature = "regex") {
        eprintln!("tuc: runtime error. This version of tuc was compiled without regex support");
        std::process::exit(1);
    }

    let bounds = maybe_fields
        .or(maybe_characters)
        .or(maybe_bytes)
        .or(maybe_lines)
        .unwrap();

    if has_json
        && bounds
            .0
            .iter()
            .any(|s| matches!(s, BoundOrFiller::Filler(_)))
    {
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

    let mut stdin = std::io::BufReader::new(std::io::stdin().lock());
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

    if opt.bounds_type == BoundsType::Bytes {
        read_and_cut_bytes(&mut stdin, &mut stdout, &opt)?;
    } else if opt.bounds_type == BoundsType::Lines {
        read_and_cut_lines(&mut stdin, &mut stdout, &opt)?;
    } else {
        read_and_cut_str(&mut stdin, &mut stdout, opt)?;
    }

    stdout.flush()?;

    Ok(())
}
