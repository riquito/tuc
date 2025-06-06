# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),

## [Unreleased]

- perf: much faster (from 2x up to 3x) implementation:
  - there is now a fast lane used when conditions apply (more or less
    it triggers when you cut fields on 1-byte characters)
  - fields cut is now done on bytes, not strings (as long as your
    delimiter is proper utf-8 you'll be fine)
  - files can be opened directly with mmap
- feat: display short help when run without arguments
- feat: add the ability to display fallback output when a field is out of bound
  (you can set it per-field using `-f <range>=somefallback` or by providing
  a generic fallback using `--fallback-oob somefallback`)
- feat: it is now possible to type \t while formatting fields and
  output a TAB (as we already do for \n) e.g. `-f '{1}\t{2}'`
- feat: you can now pass a file path as argument to tuc
  (it will be opened with mmap if available and as long as --no-mmap is not set)
- feat: new argument --fixed-memory (-M) to cut lines in chunks of
  a fixed size (in kilobytes), to allow cutting arbitrary long lines
- feat: --characters now depends on the (default) regex feature
- feat: help and short help are colored, as long as output is a tty and
  unless env var TERM=dumb or NO_COLOR (any value) is set
- refactor: --json internally uses serde_json, faster and more precise
- chore: improved test coverage

## [1.2.0] - 2024-01-01

- feat: new option --json to format output as JSON array
- feat: -r can be used when cutting --characters. It replaces
  the (empty) delimiter between characters with whatever you provided
- feat: exit early when some combinations of fields cannot be used together
- feat: updated dependencies. In particular the regex crate which offers new
  functionalities (word boundary assertions)
- fix: field formatting is now applied to field 1 even if no delimiters
  are found (similar to how we print the unformatted field 1)
- doc: many error messages have been rewritten for better clarity
- doc: new svg demo in the README

## [1.1.0] - 2023-12-02

- feat: no more need to pass --join when using --replace, it's implied
- feat: new error messages when applying some incompatible options
- doc: improved documentation, help, man page
- doc: mention that --regex requires an argument
- test: improved test coverage
- fix: better error message when --regex argument is missing

## [1.0.0] - 2023-02-25

- feat: smaller binaries by removing unnecessary (to us) regex features
- feat: regex cargo feature is enabled by default

## [0.11.0] - 2022-06-20

- fix: --lines could throw out of bounds with -f 1: in some situations
- chore: dependency updates
- doc: fixed typos
- doc: new section about community-managed install methods (macports)
- doc: man page generated using the mode modern pandoc 2.5

## [0.10.0] - 2022-06-13

- breaking: -E is now an option (-e), and accept the regex as value
- doc: added man page
- doc: improved documentation
- chore: updated pico-args

## [0.9.0] - 2022-06-05

- breaking: --lines output each bound on their own line
- feat: --regex support
- feat: minor tuning of buffers
- feat: internal improvements for --lines
- fix: right side of a range can be negative
- fix: emit proper error if right side of a range is behind left side
- fix: --lines with negative indexes were broken
- fix: --greedy-delimiter was cutting wrongly lines starting with delimiter

## [0.8.0] - 2022-05-23

- Add support for --greedy-delimiter
- Bounds can be formatted
- Major refactoring for better code maintainability

## [0.7.0] - 2022-05-21

- Add support for --join
- Add support for --lines
- Add support for --complement
- Add support for --zero-terminated

## [0.6.0] - 2022-05-13

- Add split-by-byte using --bytes
- Add split-by-character using --characters
- Faster performance when -p (compress delimiters) is on
- Faster performance when reading the input
- CI now fails if the linter is not satisfied
- Reviewd CI/release actions, simpler, faster
- Release binaries for ARM too

## [0.5.0] - 2021-07-21

- Better performances (faster, less allocations)
- Faster to compile
- Smaller binary size
- Display a better error message on unknown arguments
- Add an option to get the version back
- Migrate to pico-args
- Fix output when --only-delimited is present
- Delimiters are replaced once, allowing empty strings
- Updated dependencies
- More integration tests

## [0.4.0] - 2020-05-31

### Changed

- Build binaries for multiple operative systems
- Fixed typos in the documentation

## [0.3.0] - 2020-05-31

### Changed

- More examples
- New releases system

## [0.2.0] - 2020-05-30

### Added

- Option -p has now a long version, "compress-delimiter"

## [0.1.0] - 2020-05-30

### Added

- Can cut given a (multi)character delimiter
- Can compress the delimiter in the output
- Can replace the delimiter in the output with a string
- Can omit lines not matching delimiters
