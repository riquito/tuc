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

USAGE:
    tuc [FLAGS] [OPTIONS] < input
    tuc [FLAGS] [OPTIONS] filepath

FLAGS:
    -g, --greedy-delimiter        Match consecutive delimiters as if it was one
    -p, --compress-delimiter      Merge consecutive delimiters, then cut
    -s, --only-delimited          Print only lines containing the delimiter
    -V, --version                 Print version information
    -z, --zero-terminated         Line delimiter is NUL (\0), not LF (\n)
    -h, --help                    Print this help and exit
    -m, --complement              Invert fields (e.g. '2' becomes '1,3:')
    -j, --(no-)join               Print selected parts with delimiter inbetween
    --json                        Print fields as a JSON array of strings
    --no-mmap                     Disable memory mapping

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
    -M, --fixed-memory <size>     Read the input in chunks of <size> kilobytes.
                                  This allows to read lines arbitrarily large.
                                  Works only with single-byte delimiters,
                                  fields in ascending order, -z, -j, -r

Options precedence:
    --trim and --compress-delimiter are applied before --fields or similar

Memory consumption:
    --characters and --fields read and allocate memory one line at a time

    --lines allocate memory one line at a time as long as the requested fields
    are ordered and non-negative (e.g. -l 1,3:4,4,7), otherwise it allocates
    the whole input in memory (it also happens when -p or -m are being used)

    --bytes allocate the whole input in memory

    --fixed-memory will read the input in chunks of <size> kilobytes. This
    allows to read lines arbitrarily large. Works only with single-byte
    delimiters, fields in ascending order, -z, -j, -r

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
# Compress delimiters before cut
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

## Benchmarks

Benchmarks results will vary depending on the machine.
You can run them yourself using `./benchmark.sh`.

### Single char delimiter (sequential fields)

| Command                                                                  |       Mean [s] | Min [s] | Max [s] |     Relative |
| :----------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `./target/release/tuc -d , -f 1,8,19 tmp/data.csv > /dev/null`           |  1.080 ± 0.012 |   1.071 |   1.101 |         1.00 |
| `./target/release/tuc -d , -f 1,8,19 --no-mmap tmp/data.csv > /dev/null` |  1.230 ± 0.004 |   1.225 |   1.236 |  1.14 ± 0.01 |
| `hck -Ld, -f1,8,19 tmp/data.csv > /dev/null`                             |  1.276 ± 0.004 |   1.272 |   1.282 |  1.18 ± 0.01 |
| `hck -Ld, -f1,8,19 --no-mmap tmp/data.csv > /dev/null`                   |  1.364 ± 0.003 |   1.360 |   1.368 |  1.26 ± 0.01 |
| `uutils/coreutils cut -d , -f 1,8,19 tmp/data.csv > /dev/null`           |  1.764 ± 0.008 |   1.756 |   1.774 |  1.63 ± 0.02 |
| `hck -d, -f1,8,19  tmp/data.csv > /dev/null`                             |  2.006 ± 0.006 |   1.998 |   2.014 |  1.86 ± 0.02 |
| `hck -d, -f1,8,19  --no-mmap tmp/data.csv > /dev/null`                   |  2.130 ± 0.062 |   2.096 |   2.241 |  1.97 ± 0.06 |
| `choose -f , -i tmp/data.csv 0 7 18 > /dev/null`                         |  4.347 ± 0.014 |   4.329 |   4.365 |  4.03 ± 0.05 |
| `cut -d, -f1,8,19 tmp/data.csv > /dev/null`                              |  5.726 ± 0.012 |   5.712 |   5.742 |  5.30 ± 0.06 |
| `awk -F, '{print $1, $8, $19}' tmp/data.csv > /dev/null`                 | 35.852 ± 0.121 |  35.683 |  36.006 | 33.20 ± 0.39 |

### Single char delimiter (non sequential fields)

| Command                                                                  |       Mean [s] | Min [s] | Max [s] |     Relative |
| :----------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `./target/release/tuc -d , -f 1,19,8 tmp/data.csv > /dev/null`           |  1.093 ± 0.006 |   1.082 |   1.097 |         1.00 |
| `./target/release/tuc -d , -f 1,19,8 --no-mmap tmp/data.csv > /dev/null` |  1.231 ± 0.004 |   1.226 |   1.235 |  1.13 ± 0.01 |
| `hck -Ld, -f1,19,8 tmp/data.csv > /dev/null`                             |  1.465 ± 0.006 |   1.457 |   1.473 |  1.34 ± 0.01 |
| `hck -Ld, -f1,19,8 --no-mmap tmp/data.csv > /dev/null`                   |  1.568 ± 0.003 |   1.565 |   1.572 |  1.43 ± 0.01 |
| `uutils/coreutils cut -d , -f 1,19,8 tmp/data.csv > /dev/null`           |  1.769 ± 0.006 |   1.763 |   1.779 |  1.62 ± 0.01 |
| `hck -d, -f1,19,8  tmp/data.csv > /dev/null`                             |  2.012 ± 0.004 |   2.008 |   2.016 |  1.84 ± 0.01 |
| `hck -d, -f1,19,8  --no-mmap tmp/data.csv > /dev/null`                   |  2.112 ± 0.007 |   2.104 |   2.120 |  1.93 ± 0.01 |
| `choose -f , -i tmp/data.csv 0 18 7 > /dev/null`                         |  4.412 ± 0.105 |   4.320 |   4.577 |  4.04 ± 0.10 |
| `cut -d, -f1,19,8 tmp/data.csv > /dev/null`                              |  5.723 ± 0.005 |   5.718 |   5.728 |  5.24 ± 0.03 |
| `awk -F, '{print $1, $19, $8}' tmp/data.csv > /dev/null`                 | 36.106 ± 0.320 |  35.699 |  36.514 | 33.04 ± 0.34 |

### Multi chars delimiter

| Command                                                                                 |       Mean [s] | Min [s] | Max [s] |     Relative |
| :-------------------------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `./target/release/tuc -d'   ' -f 1,8,19 ./tmp/data-multichar.txt > /dev/null`           |  1.464 ± 0.016 |   1.443 |   1.489 |         1.00 |
| `./target/release/tuc -d'   ' -f 1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null` |  1.541 ± 0.009 |   1.531 |   1.554 |  1.05 ± 0.01 |
| `hck -Ld'   ' -f1,8,19 ./tmp/data-multichar.txt > /dev/null`                            |  1.640 ± 0.010 |   1.627 |   1.654 |  1.12 ± 0.01 |
| `hck -Ld'   ' -f1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null`                  |  1.697 ± 0.007 |   1.688 |   1.705 |  1.16 ± 0.01 |
| `hck -d'   ' -f1,8,19 ./tmp/data-multichar.txt > /dev/null`                             |  2.191 ± 0.005 |   2.185 |   2.197 |  1.50 ± 0.02 |
| `hck -d'   ' --no-mmap -f1,8,19 ./tmp/data-multichar.txt > /dev/null`                   |  2.252 ± 0.019 |   2.225 |   2.277 |  1.54 ± 0.02 |
| `choose -f '   ' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null`                       |  4.414 ± 0.036 |   4.371 |   4.470 |  3.02 ± 0.04 |
| `< ./tmp/data-multichar.txt tr -s ' ' \| hck -Ld' ' -f1,8,19 > /dev/null`               |  5.266 ± 0.042 |   5.210 |   5.327 |  3.60 ± 0.05 |
| `< ./tmp/data-multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`               |  5.310 ± 0.044 |   5.260 |   5.355 |  3.63 ± 0.05 |
| `awk -F' ' '{print $1, $8 $19}' ./tmp/data-multichar.txt > /dev/null`                   |  6.015 ± 0.063 |   5.942 |   6.105 |  4.11 ± 0.06 |
| `hck -d'\s+' -f1,8,19 ./tmp/data-multichar.txt > /dev/null`                             |  9.834 ± 0.049 |   9.749 |   9.872 |  6.72 ± 0.08 |
| `./target/release/tuc -e'\s+' -f 1,8,19 ./tmp/data-multichar.txt > /dev/null`           |  9.870 ± 0.056 |   9.801 |   9.940 |  6.74 ± 0.08 |
| `hck -d'\s+' -f1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null`                   |  9.876 ± 0.043 |   9.824 |   9.934 |  6.75 ± 0.08 |
| `./target/release/tuc -e'\s+' -f 1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null` | 10.009 ± 0.066 |   9.956 |  10.118 |  6.84 ± 0.09 |
| `choose -f '[[:space:]]' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null`               | 13.373 ± 0.398 |  13.022 |  13.829 |  9.13 ± 0.29 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./tmp/data-multichar.txt > /dev/null`         | 13.927 ± 0.233 |  13.672 |  14.298 |  9.51 ± 0.19 |
| `awk -F'   ' '{print $1, $8, $19}' ./tmp/data-multichar.txt > /dev/null`                | 14.471 ± 0.297 |  14.313 |  14.999 |  9.88 ± 0.23 |
| `choose -f '\s' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null`                        | 26.576 ± 0.232 |  26.328 |  26.846 | 18.15 ± 0.26 |

## LICENSE

Tuc is distributed under the GNU GPL license (version 3 or any later version).

See [LICENSE](./LICENSE) file for details.
