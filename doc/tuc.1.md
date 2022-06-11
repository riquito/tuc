% tuc(1) Tuc 0.9.0 | Tuc Manual
%
% Jun 10, 2022

NAME
====

**tuc** — cut text or bytes and keep what you need

SYNOPSIS
========

| **cut** \[FLAGS]... \[OPTIONS]...

DESCRIPTION
===========

Cut text (or bytes) at delimiter, then keep the desired parts.  
A default delimiter is set when cutting lines, characters or bytes.  

The data is read from standard input.

FLAGS
=====

-g, --greedy-delimiter
:   Split fields using a greedy delimiter

-p, --compress-delimiter
:   Collapse any sequence of delimiters

-s, --only-delimited
:   Do not print lines not containing delimiters

-V, --version
:   Prints version information

-z, --zero-terminated
:   line delimiter is NUL (\0), not LF (\n)

-h, --help
:   Prints this help and exit

-m, --complement
:   keep the opposite fields than the one selected

-j, --(no-)join
:   write the delimiter between fields

-E, --regex
:   use --delimiter as a regular expression


OPTIONS
=======

| **-f**, **--fields** [bounds]
|        Fields to keep, 1-indexed, comma separated.
|        Use colon to include everything in a range.

|        [default 1:]

|        e.g. cutting on '-' the string 'a-b-c-d'
|          1     => a
|          1:    => a-b-c-d
|          1:3   => a-b-c
|          3,2   => cb
|          3,1:2 => ca-b
|          -3:-2 => b-c

|        To re-add the delimiter check -j, to replace
|        it check -r.

|        You can also format the output using {} syntax
|        e.g.
|          '["{1}", "{2}"]' => ["a", "b"]

|        You can escape { and } using {{ and }}.

| **-b**, **--bytes** [bounds]
|        Same as --fields, but it keeps bytes

| **-c**, **--characters** [bounds]
|        Same as --fields, but it keeps characters

| **-l**, **--lines** [bounds]
|        Same as --fields, but it keeps lines
|        Implies --join (use --no-join to concat lines)

| **-d**, **--delimiter** [delimiter]
|        Delimiter used by -f to cut the text
|        [default: \\t]

| **-r**, **--replace-delimiter** [new delimiter]
|        Replace the delimiter with the provided text

| **-t**, **--trim** [type]
|        Trim the delimiter (greedy).
|        Valid values are (l|L)eft, (r|R)ight, (b|B)oth


BUGS
====

See GitHub Issues: <https://github.com/riquito/tuc/issues>

AUTHOR
======

Riccardo Attilio Galli <riccardo@sideralis.org>

SEE ALSO
========

**cut(1)**, **sed(1)**, **awk(1)**
