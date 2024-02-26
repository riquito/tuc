# tuc (when cut doesn't cut it)
[![version](https://img.shields.io/crates/v/tuc.svg)](https://crates.io/crates/tuc)
![ci](https://github.com/riquito/tuc/actions/workflows/ci.yml/badge.svg)
[![license](https://img.shields.io/crates/l/tuc.svg)](https://crates.io/crates/tuc)

You want to `cut` on more than just a character, perhaps using negative indexes 
or format the selected fields as you want...
Maybe you want to cut on lines (ever needed to drop or keep first and last line?)...
That's where `tuc` can help.

## Install

Download one of the [prebuilt binaries](https://github.com/riquito/tuc/releases)

or run

```sh
# requires rustc >= 1.61.0
cargo install tuc # append `--no-default-features` for a smaller binary with no regex support
```

For other installation methods, check below the [community managed packages](#community-managed-packages)

## Try it out online

No time to install it? Play with a webassembly version online, the [tuc playground](https://riquito.github.io/tuc/playground/index.html)

## Demo

![svg](./doc/screencast.svg)

## Help

```
tuc 1.2.0
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
    -j, --(no-)join               Print selected parts with delimiter inbetween
    --json                        Print fields as a JSON array of strings

OPTIONS:
    -f, --fields <bounds>         Fields to keep, 1-indexed, comma separated.
                                  Use colon (:) to match a range (inclusive).
                                  Use equal (=) to apply out of bound fallback.
                                  Fields can be negative (-1 is the last field).
                                  [default 1:]

                                  e.g. cutting the string 'a-b-c-d' on '-'
                                    -f 1     => a
                                    -f 1:    => a-b-c-d
                                    -f 1:3   => a-b-c
                                    -f 3,2   => cb
                                    -f 3,1:2 => ca-b
                                    -f -3:-2 => b-c
                                    -f 1,8=fallback => afallback

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
    -r, --replace-delimiter <new> Replace the delimiter with the provided text
    -t, --trim <type>             Trim the delimiter (greedy). Valid values are
                                  (l|L)eft, (r|R)ight, (b|B)oth
        --fallback-oob <fallback> Generic fallback output for any field that
                                  cannot be found (oob stands for out of bound).
                                  It's overridden by any fallback assigned to a
                                  specific field (see -f for help)

Options precedence:
    --trim and --compress-delimiter are applied before --fields or similar

Memory consumption:
    --characters and --fields read and allocate memory one line at a time

    --lines allocate memory one line at a time as long as the requested fields
    are ordered and non-negative (e.g. -l 1,3:4,4,7), otherwise it allocates
    the whole input in memory (it also happens when -p or -m are being used)

    --bytes allocate the whole input in memory

Colors:
    Help is displayed using colors. Colors will be suppressed in the
    following circumstances:
    - when the TERM environment variable is not set or set to "dumb"
    - when the NO_COLOR environment variable is set (regardless of value)
```

## Examples

```sh
# Cut and rearrange fields...
❯ echo "foo bar baz" | tuc -d ' ' -f 3,2,1
bazbarfoo
```

```sh
# ...and join them back with the same delimiter
❯ echo "foo bar baz" | tuc -d ' ' -f 3,2,1 -j
baz bar foo
```

```sh
# Replace the delimiter with something else
❯ echo "foo bar baz" | tuc -d ' ' -r ' ➡ '
foo ➡ bar ➡ baz
```

```sh
# Keep a range of fields
❯ echo "foo bar    baz" | tuc -d ' ' -f 2:
bar    baz
```

```sh
# Indexes can be negative and rearranged
❯ echo "a b c" | tuc -d ' ' -f -1,-2,-3
cba
```

```sh
# Cut using regular expressions
❯ echo "a,b, c" | tuc -e '[, ]+' -f 1,3
ac
```

```sh
# Emit JSON output
❯ echo "foo bar baz" | tuc -d ' ' --json
["foo","bar","baz"]
```

```sh
# Delimiters can be any number of characters long
❯ echo "a<sep>b<sep>c" | tuc -d '<sep>' -f 1,3
ac
```

```sh
# Cut using a greedy delimiter
❯ echo "foo    bar" | tuc -d ' ' -f 1,2 -g
foobar
```

```sh
# Format output
❯ echo "foo bar baz" | tuc -d ' ' -f '{1}, {2} and lastly {3}'
foo, bar and lastly baz
# ...with support for \n
❯ echo "100Kb README.txt 2049-02-01" | tuc -d ' ' -f '{2}\n├── {1}\n└── {3}'
README.txt
├── 100Kb
└── 2049-02-01
```

```sh
# Cut lines (e.g. keep everything between first and last line)
❯ printf "a\nb\nc\nd\ne" | tuc -l 2:-2
b
c
d
```

```sh
# Concatenate lines (-l implies join with \n, so we need --no-join)
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
# Discard selected fields, keep the rest
❯ echo "a b c" | tuc --complement -d ' ' -f 2
ac
```

## Community-Managed Packages

Heartfelt thanks to package maintainers: you make it easy to access open source software ❤️

[![Packaging status](https://repology.org/badge/vertical-allrepos/tuc-cut.svg)](https://repology.org/project/tuc-cut/versions)

- [ArchLinux](https://aur.archlinux.org/packages/tuc):
  ```sh
  yay -S tuc # compile from source
  yay -S tuc-bin # install pre-built binaries tuc and tuc-regex
  ```

- [Brew](https://formulae.brew.sh/formula/tuc):
  ```sh
  brew install tuc
  ```

- [MacPorts](https://ports.macports.org/port/tuc/):
  ```sh
  sudo port install tuc
  ```

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
