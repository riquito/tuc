# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),

## [Unreleased]
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
