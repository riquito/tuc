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
tuc 0.5.0
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -p, --compress-delimiter      Collapse any sequence of delimiters
    -s, --only-delimited          Do not print lines not containing delimiters
    -V, --version                 Prints version information
    -h, --help                    Prints this help and exit

OPTIONS:
    -b, --bytes <fields>          Same as --fields, but it cuts on bytes instead
                                  (doesn't require a delimiter)
    -d, --delimiter <delimiter>   Delimiter to use to cut the text into pieces
                                  [default: \\t]
    -f, --fields <fields>         Fields to keep, 1-indexed, comma separated.
                                  Use colon for inclusive ranges.
                                  e.g. 1:3 or 3,2 or 1: or 3,1:2 or -3 or -3:-2
                                  [default 1:]
    -c, --characters <fields>     Same as --fields, but it keeps characters instead
                                  (doesn't require a delimiter)
    -r, --replace-delimiter <s>   Replace the delimiter with the provided text
    -t, --trim <trim>             Trim the delimiter. Valid trim values are
                                  (l|L)eft, (r|R)ight, (b|B)oth
```

## Examples

```sh
# Cut using a greedy delimiter
❯ echo "foo    bar   baz" | tuc -d ' ' -f 2:
bar   baz
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
# Can split on unicode scalar values (expect UTF-8 encoding)
❯ echo "a𝌆b𝌆c" | tuc -d '𝌆' -f 1,3
ac
```

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
