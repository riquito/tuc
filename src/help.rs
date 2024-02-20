pub const HELP: &str = concat!(
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

                                  e.g. cutting the string 'a-b-c-d' on '-'
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
                                    -f '({1}, {2})' => (a, b)

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

pub const SHORT_HELP: &str = concat!(
    "tuc ",
    env!("CARGO_PKG_VERSION"),
    r#" - Created by Riccardo Attilio Galli

Cuts selected parts and lets you re-arrange them.

Some examples:

    $ echo "a/b/c" | tuc -d / -f 1,-1
    ac

    $ echo "a/b/c" | tuc -d / -f 2:
    b/c

    $ echo "hello.bak" | tuc -d . -f 'mv {1:} {1}'
    mv hello.bak hello

    $ printf "a\nb\nc\nd\ne" | tuc -l 2:-2
    b
    c
    d

Run `tuc --help` for more detailed information.
"#
);
