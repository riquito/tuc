% tuc(1) Tuc 1.3.0 | Tuc Manual
%
% Sep 10, 2025

NAME
====

**tuc** — cut text or bytes and keep what you need

SYNOPSIS
========

| **tuc** \[FLAGS]... \[OPTIONS]... \< input
| **tuc** \[FLAGS]... \[OPTIONS]... filepath

DESCRIPTION
===========

Cut text (or bytes) where a delimiter matches, then keep the desired parts.  

FLAGS
=====

-g, \--greedy-delimiter
:   Match consecutive delimiters as if it was one

-p, \--compress-delimiter
:   Print only the first delimiter of a sequence

-s, \--only-delimited
:   Print only lines containing the delimiter

-V, \--version
:   Print version information

-z, \--zero-terminated
:   Line delimiter is NUL (\0), not LF (\\n)

-h, \--help
:   Print this help and exit

-m, \--complement
:   Invert fields (e.g. \'2\' becomes \'1,3:\')

-j, \--(no-)join
:   Print selected parts with delimiter in between

\--json
:   Print fields as a JSON array of strings

\--no-mmap
:   Disable memory mapping

OPTIONS
=======

| **-f**, **\--fields** [bounds]
|        Fields to keep, 1-indexed, comma separated.
|        Use colon (:) to match a range (inclusive).
|        Use equal (=) to apply out of bound fallback.
|        Fields can be negative (-1 is the last field).

|        [default 1:]

|        e.g. cutting the string \'a-b-c-d\' on \'-\'
|          `-f 1     => a`
|          `-f 1:    => a-b-c-d`
|          `-f 1:3   => a-b-c`
|          `-f 3,2   => cb`
|          `-f 3,1:2 => ca-b`
|          `-f -3:-2 => b-c`
|          `-f 1,8=fallback => afallback`

|        To re-apply the delimiter add -j, to replace
|        it add -r (followed by the new delimiter)

|        You can also format the output using {} syntax
|        e.g.
|          `-f '({1}, {2})' => (a, b)`

|        You can escape { and } using {{ and }}.

| **-b**, **\--bytes** [bounds]
|        Same as \--fields, but it keeps bytes

| **-c**, **\--characters** [bounds]
|        Same as \--fields, but it keeps characters

| **-l**, **\--lines** [bounds]
|        Same as \--fields, but it keeps lines
|        Implies \--join. To merge lines, use \--no-join

| **-d**, **\--delimiter** [delimiter]
|        Delimiter used by \--fields to cut the text
|        [default: \\t]

| **-e**, **\--regex** [some regex]
|        Use a regular expression as delimiter

| **-r**, **\--replace-delimiter** [new delimiter]
|        Replace the delimiter with the provided text

| **-t**, **\--trim** [type]
|        Trim the delimiter (greedy).
|        Valid values are (l|L)eft, (r|R)ight, (b|B)oth

|     **\--fallback-oob** [fallback]
|        Generic fallback output for any field that
|        cannot be found (oob stands for out of bound).
|        It's overridden by any fallback assigned to a
|        specific field (see -f for help)

| **-M**, **\--fixed-memory** [size]
|        Read the input in chunks of <size> kilobytes.
|        This allows to read lines arbitrarily large.
|        Works only with single-byte delimiters,
|        fields in ascending order, -z, -j, -r

OPTIONS PRECEDENCE
==================

\--trim and \--compress-delimiter are applied before \--fields or similar

MEMORY CONSUMPTION
==================

\--characters and \--fields read and allocate memory one line at a time

| \--lines allocate memory one line at a time as long as the requested fields are
| ordered and non-negative (e.g. -l 1,3:4,4,7), otherwise it allocates
| the whole input in memory (it also happens when -p or -m are being used)

\--bytes allocate the whole input in memory

| \--fixed-memory will read the input in chunks of <size> kilobytes. This
| allows to read lines arbitrarily large. Works only with single-byte
| delimiters, fields in ascending order, -z, -j, -r

COLORS
======

| Help is displayed using colors. Colors will be suppressed in the
| following circumstances:

- when the TERM environment variable is not set or set to "dumb"
- when the NO_COLOR environment variable is set (regardless of value)

BUGS
====

See GitHub Issues: <https://github.com/riquito/tuc/issues>

AUTHOR
======

Riccardo Attilio Galli <riccardo@sideralis.org>

SEE ALSO
========

**cut(1)**, **sed(1)**, **awk(1)**
