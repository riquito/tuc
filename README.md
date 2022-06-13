# tuc (when cut doesn't cut it)
[![version](https://img.shields.io/crates/v/tuc.svg)](https://crates.io/crates/tuc)
![ci](https://github.com/riquito/tuc/actions/workflows/ci.yml/badge.svg)
[![license](https://img.shields.io/crates/l/tuc.svg)](https://crates.io/crates/tuc)

You want to `cut` on more than just a character, perhaps using negative indexes 
or format the selected fields as you want...
Maybe you want to cut on lines (ever needed to drop first and last line?)...
That's where `tuc` can help.

## Install

Download one of the [prebuilt binaries](https://github.com/riquito/tuc/releases)

or run

```sh
cargo install tuc # append `--features regex` if you want regex support
```

## Help

```
tuc 0.11.0 [UNRELEASED]
Cut text (or bytes) where a delimiter matches, then keep the desired parts.

The data is read from standard input.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -g, --greedy-delimiter        Match consecutive delmiters as if it was one
    -p, --compress-delimiter      Print only the first delimiter of a sequence
    -s, --only-delimited          Print only lines containing the delimiter
    -V, --version                 Print version information
    -z, --zero-terminated         Line delimiter is NUL (\0), not LF (\n)
    -h, --help                    Print this help and exit
    -m, --complement              Invert fields (e.g. '2' becomes '1,3:')
    -j, --(no-)join               Print selected parts with delimiter inbetween

OPTIONS:
    -f, --fields <bounds>         Fields to keep, 1-indexed, comma separated.
                                  Use colon to include everything in a range.
                                  Fields can be negative (-1 is the last field).
                                  [default 1:]

                                  e.g. cutting on '-' the string 'a-b-c-d'
                                    1     => a
                                    1:    => a-b-c-d
                                    1:3   => a-b-c
                                    3,2   => cb
                                    3,1:2 => ca-b
                                    -3:-2 => b-c

                                  To re-apply the delimiter add -j, to replace
                                  it add -r (followed by the new delimiter).

                                  You can also format the output using {} syntax
                                  e.g.
                                  '["{1}", "{2}"]' => ["a", "b"]

                                  You can escape { and } using {{ and }}.

    -b, --bytes <bounds>          Same as --fields, but it keeps bytes
    -c, --characters <bounds>     Same as --fields, but it keeps characters
    -l, --lines <bounds>          Same as --fields, but it keeps lines
                                  Implies --join. To merge lines, use --no-join
    -d, --delimiter <delimiter>   Delimiter used by --fields to cut the text
                                  [default: \t]
    -e, --regex                   Use a regular expression as delimiter
    -r, --replace-delimiter <new> Replace the delimiter with the provided text
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
```

## Examples

```sh
# Cut and rearrange fields...
❯ echo "foo bar baz" | tuc -d ' ' -f 3,2,1
bazbarfoo
```

```sh
# ...and apply back the delimiter...
❯ echo "foo bar baz" | tuc -j -d ' ' -f 3,2,1
baz bar foo
```

```sh
# ...or replace it
❯ echo "foo bar baz" | tuc -j -r ' ➡ ' -d ' ' -f 3,2,1
baz ➡ bar ➡ foo
```

```sh
# Keep ranges
❯ echo "foo bar    baz" | tuc -d ' ' -f 2:
bar    baz
```

```sh
# Cut using a greedy delimiter
❯ echo "foo    bar" | tuc -g -d ' ' -f 1,2
foobar
```

```sh
# Format output
❯ echo "foo bar baz" | tuc -d ' ' -f '{1}, {2} and lastly {3}'
foo, bar and lastly baz
# Support \n
❯ echo "100Kb README.txt 2049-02-01" | tuc -d ' ' -f '{2}\n├── {1}\n└── {3}'
README.txt
├── 100Kb
└── 2049-02-01
```

```sh
# Cut lines (e.g. drop first and last line)
❯ printf "a\nb\nc\nd\ne" | tuc -l 2:-2
b
c
d
```

```sh
# Concatenate lines
❯ printf "a\nb\nc\nd\ne" | tuc -l 1,2 --no-join
ab
```

```sh
# Compress delimiters after cut
❯ echo "foo    bar   baz" | tuc -d ' ' -f 2: -p
bar baz
```

```sh
# Replace remaining delimiters with something else
❯ echo "foo    bar   baz" | tuc -d ' ' -f 2: -p -r ' -> '
bar -> baz
```

```sh
# Indexes can be negative and rearranged
❯ echo "a b c" | tuc -d ' ' -f -1,-2,-3
cba
```

```sh
# Cut using regular expressions (requires a release with regex features enabled)
❯ echo "a,b, c" | tuc -e '[, ]+' -f 1,3
ac
```

```sh
# Delimiters can be any number of characters long
❯ echo "a<sep>b<sep>c" | tuc -d '<sep>' -f 1,3
ac
```

```sh
# Cut characters (expects UTF-8 input)
❯ echo "😁🤩😝😎" | tuc -c 4,3,2,1
😎😝🤩😁
```

```sh
# Cut bytes (the following emoji are 4 bytes each)
❯ echo "😁🤩😝😎" | tuc -b 5:8
🤩
```

```sh
# Keep non-matching fields
❯ echo "a b c" | tuc --complement -d ' ' -f 2
ac
```

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
