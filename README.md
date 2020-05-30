# tuc (when cut doesn't cut it)

## Help

```
tuc 0.1.0
When cut doesn't cut it.

USAGE:
    tuc [FLAGS] [OPTIONS]

FLAGS:
    -p                      Display the delimiter at most once in a sequence
    -h, --help              Prints help information
    -s, --only-delimited    Do not print lines not containing delimiters
    -V, --version           Prints version information

OPTIONS:
    -d, --delimiter <delimiter>    Delimiter to use to cut the text into pieces [default: 	]
    -f, --fields <fields>          Fields to keep, like 1:3 or 3,2 or 1: or 3,1:2 or -3 or -3:-2 [default: 1:]
    -r <replace-delimiter>         Replace the delimiter
    -t <trim>                      Valid trim values are (l|L)eft, (r|R)ight, (b|B)oth
```

## Examples

```
# Cut using a greedy delimiter
$ echo "foo    bar   baz" | tuc -d ' ' -f 2:
bar   baz
```

```
# Compress delimiters after cut
$ echo "foo    bar   baz" | tuc -d ' ' -f 2: -p
bar baz
```

```
# Replace remaining delimiters with something else
$ echo "foo    bar   baz" | tuc -d ' ' -f 2: -p -r '/'
bar/baz
```
