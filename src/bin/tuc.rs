use anyhow::Result;
use std::io::Write;
use std::str::FromStr;
use tuc::bounds::{BoundsType, UserBoundsList};
use tuc::cut_bytes::read_and_cut_bytes;
use tuc::cut_lines::read_and_cut_lines;
use tuc::cut_str::read_and_cut_str;
use tuc::options::{Opt, EOL};

const HELP: &str = concat!(
    "tuc ",
    env!("CARGO_PKG_VERSION"),
    "
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -g, --greedy-delimiter        Split fields using a greedy delimiter
    -p, --compress-delimiter      Collapse any sequence of delimiters
    -s, --only-delimited          Do not print lines not containing delimiters
    -V, --version                 Prints version information
    -z, --zero-terminated         line delimiter is NUL (\\0), not LF (\\n)
    -h, --help                    Prints this help and exit
    -m, --complement              keep the opposite fields than the one selected
    -j, --join                    write the delimiter between fields

OPTIONS:
    -f, --fields <bounds>         Fields to keep, 1-indexed, comma separated.
                                  Use colon to include everything in a range.
                                  [default 1:]

                                  e.g. cutting on '-' the string 'a-b-c-d'
                                    1     => a
                                    1:    => a-b-c-d
                                    1:3   => a-b-c
                                    3,2   => cb
                                    3,1:2 => ca-b
                                    -3:-2 => b-c

                                  To re-add the delimiter check -j, to replace
                                  it check -r.

                                  You can also format the output using {} syntax
                                  e.g.
                                  '[\"{1}\", \"{2}\", \"{3}\"]' => [\"a\", \"b\", \"c\"]

                                  You can escape { and } using {{ and }}.

    -b, --bytes <bounds>          Same as --fields, but it keeps bytes
    -c, --characters <bounds>     Same as --fields, but it keeps characters
    -l, --lines <bounds>          Same as --fields, but it keeps lines
    -d, --delimiter <delimiter>   Delimiter used by -f to cut the text
                                  [default: \\t]
    -r, --replace-delimiter <s>   Replace the delimiter with the provided text
    -t, --trim <trim>             Trim the delimiter. Valid trim values are
                                  (l|L)eft, (r|R)ight, (b|B)oth

Notes:
    --trim and --compress-delimiter are applied before --fields
    --lines does not load the whole input in memory if the fields are ordered
      and non-negative (e.g. -l 1,3:4,4,7) and options -p/-m have not been set
"
);

fn parse_args() -> Result<Opt, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
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

    let delimiter = match bounds_type {
        BoundsType::Fields => pargs
            .opt_value_from_str(["-d", "--delimiter"])?
            .unwrap_or_else(|| String::from('\t')),
        BoundsType::Lines => String::from("\n"),
        _ => String::new(),
    };

    let args = Opt {
        complement: pargs.contains(["-m", "--complement"]),
        only_delimited: pargs.contains(["-s", "--only-delimited"]),
        greedy_delimiter: pargs.contains(["-g", "--greedy-delimiter"]),
        compress_delimiter: pargs.contains(["-p", "--compress-delimiter"]),
        version: pargs.contains(["-V", "--version"]),
        join: pargs.contains(["-j", "--join"]),
        eol: if pargs.contains(["-z", "--zero-terminated"]) {
            EOL::Zero
        } else {
            EOL::Newline
        },
        delimiter,
        bounds_type,
        bounds: maybe_fields
            .or(maybe_characters)
            .or(maybe_bytes)
            .or(maybe_lines)
            .unwrap(),
        replace_delimiter: pargs.opt_value_from_str(["-r", "--replace-delimiter"])?,
        trim: pargs.opt_value_from_str(["-t", "--trim"])?,
    };

    let remaining = pargs.finish();

    if args.version {
        println!("tuc {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if !remaining.is_empty() {
        eprintln!("tuc: unexpected arguments {:?}", remaining);
        eprintln!("Try 'tuc --help' for more information.");
        std::process::exit(1);
    }

    Ok(args)
}

fn main() -> Result<()> {
    let opt: Opt = parse_args()?;

    let stdin = std::io::stdin();
    let mut stdin = std::io::BufReader::with_capacity(32 * 1024, stdin.lock());

    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::with_capacity(32 * 1024, stdout.lock());

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
