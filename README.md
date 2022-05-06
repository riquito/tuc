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
tuc 0.4.0
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -p, --compress-delimiter    Display the delimiter at most once in a sequence
    -h, --help                  Prints help information
    -s, --only-delimited        Do not print lines not containing delimiters
    -V, --version               Prints version information

OPTIONS:
    -d, --delimiter <delimiter>    Delimiter to use to cut the text into pieces [default: 	]
    -f, --fields <fields>          Fields to keep, like 1:3 or 3,2 or 1: or 3,1:2 or -3 or -3:-2 [default: 1:]
    -r <replace-delimiter>         Replace the delimiter
    -t <trim>                      Valid trim values are (l|L)eft, (r|R)ight, (b|B)oth
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
