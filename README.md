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

```
cargo install tuc
```

## Help

```
tuc 0.10.0 [UNRELEASED]
Cut text (or bytes) at delimiter, then keep the desired parts.
A default delimiter is set when cutting lines, characters or bytes.

The data is read from standard input.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -g, --greedy-delimiter        Split fields using a greedy delimiter
    -p, --compress-delimiter      Collapse any sequence of delimiters
    -s, --only-delimited          Do not print lines not containing delimiters
    -V, --version                 Prints version information
    -z, --zero-terminated         line delimiter is NUL (\0), not LF (\n)
    -h, --help                    Prints this help and exit
    -m, --complement              keep the opposite fields than the one selected
    -j, --(no-)join               write the delimiter between fields
    -e, --regex                   use --delimiter as a regular expression

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
                                  '["{1}", "{2}", "{3}"]' => ["a", "b", "c"]

                                  You can escape { and } using {{ and }}.

    -b, --bytes <bounds>          Same as --fields, but it keeps bytes
    -c, --characters <bounds>     Same as --fields, but it keeps characters
    -l, --lines <bounds>          Same as --fields, but it keeps lines
                                  Implies --join (use --no-join to concat lines)
    -d, --delimiter <delimiter>   Delimiter used by -f to cut the text
                                  [default: \t]
    -r, --replace-delimiter <s>   Replace the delimiter with the provided text
    -t, --trim <trim>             Trim the delimiter (greedy). Valid values are
                                  (l|L)eft, (r|R)ight, (b|B)oth

Notes:
    --trim and --compress-delimiter are applied before --fields
    --lines does not load the whole input in memory if the fields are ordered
      and non-negative (e.g. -l 1,3:4,4,7) and options -p/-m have not been set
```

## Examples

```sh
# Cut and rearrange fields
❯ echo "foo bar baz" | tuc -d ' ' -f 3,2,1
bazbarfoo
```

```sh
# Keep ranges
❯ echo "foo bar baz" | tuc -d ' ' -f 2:
bar baz
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
# Cut lines
❯ printf "a\nb\nc\nd\ne" | tuc -l 2:-2
b
c
d
```

```sh
# Concat lines
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
# Accept regular expressions (requires a release with the feature enabled)
❯ echo "a,b, c" | tuc -E -d '[, ]+' -f 1,3
ac
```

```sh
# Delimiters can be any number of characters long
❯ echo "a<sep>b<sep>c" | tuc -d '<sep>' -f 1,3
ac
```

```sh
# Can split on unicode scalar values (it expects UTF-8 encoding)
❯ echo "a𝌆b𝌆c" | tuc -d '𝌆' -f 1,3
ac
```

```sh
# Can split on characters
❯ echo "😁🤩😝😎" | tuc -c 4,3,2,1
😎😝🤩😁
```

```sh
# Can split on bytes (the following emoji are 4 bytes each)
❯ echo "😁🤩😝😎" | tuc -b 5:8
🤩
```

```sh
# Can keep the opposite fields
❯ echo "a b c" | tuc --complement -d ' ' -f 2
ac
```

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
