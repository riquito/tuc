# tuc (when cut doesn't cut it)
[![version](https://img.shields.io/crates/v/tuc.svg)](https://crates.io/crates/tuc)
![ci](https://github.com/riquito/tuc/actions/workflows/ci.yml/badge.svg)
[![license](https://img.shields.io/crates/l/tuc.svg)](https://crates.io/crates/tuc)

We've all been there. You want to `cut` some string on a delimiter repeated in a non-deterministic way. Maybe you even want to use negative indexes or replace the delimiters in the cut part with something else...
That's where `tuc` can help.

## Install

Download one of the [prebuilt binaries](https://github.com/riquito/tuc/releases)

or run

```
cargo install tuc
```

## Help

```
tuc 0.8.0 [UNRELEASED]
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -p, --compress-delimiter      Collapse any sequence of delimiters
    -s, --only-delimited          Do not print lines not containing delimiters
    -V, --version                 Prints version information
    -z, --zero-terminated         line delimiter is NUL (\0), not LF (\n)
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
                                  '["{1}", "{2}", "{3}"]' => ["a", "b", "c"]

                                  You can escape { and } using {{ and }}.

    -b, --bytes <bounds>          Same as --fields, but it keeps bytes
    -c, --characters <bounds>     Same as --fields, but it keeps characters
    -l, --lines <bounds>          Same as --fields, but it keeps lines
    -d, --delimiter <delimiter>   Delimiter used by -f to cut the text
                                  [default: \t]
    -r, --replace-delimiter <s>   Replace the delimiter with the provided text
    -t, --trim <trim>             Trim the delimiter. Valid trim values are
                                  (l|L)eft, (r|R)ight, (b|B)oth

Notes:
    --trim and --compress-delimiter are applied before --fields
    --lines does not load the whole input in memory if the fields are ordered
      and non-negative (e.g. -l 1,3:4,4,7) and options -p/-m have not been set
```

## Examples

```sh
# Cut using a greedy delimiter
❯ echo "foo    bar   baz" | tuc -d ' ' -f 2:
bar   baz
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
echo "a b c" | tuc --complement -d ' ' -f 2
ac
```

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
