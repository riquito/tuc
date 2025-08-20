use crate::finders::common::DelimiterFinder;
use std::ops::Range;

pub struct FixedGreedyFinder {
    finder: memchr::memmem::Finder<'static>,
    len: usize,
}

impl FixedGreedyFinder {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::Finder::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for FixedGreedyFinder {
    type Iter<'a> = FieldsLocationsGreedy<'a>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let iter = self.finder.find_iter(line);
        FieldsLocationsGreedy::new(iter, self.len)
    }
}

pub struct FieldsLocationsGreedy<'a> {
    iter: std::iter::Peekable<memchr::memmem::FindIter<'a, 'a>>,
    delimiter_len: usize,
    current_pos: usize,
    finished: bool,
}

impl<'a> FieldsLocationsGreedy<'a> {
    fn new(iter: memchr::memmem::FindIter<'a, 'a>, delimiter_len: usize) -> Self {
        Self {
            iter: iter.peekable(),
            delimiter_len,
            current_pos: 0,
            finished: false,
        }
    }
}

impl<'a> Iterator for FieldsLocationsGreedy<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Look for the delimiter in the remaining portion
        if let Some(idx) = self.iter.next() {
            let start_delimiter = idx;
            self.current_pos = start_delimiter + self.delimiter_len;

            // Skip any consecutive delimiters (greedy behavior)
            while self.iter.peek() == Some(&self.current_pos) {
                self.iter.next();
                self.current_pos += self.delimiter_len;
            }

            Some(Range {
                start: start_delimiter,
                end: self.current_pos,
            })
        } else {
            // No more delimiters found
            self.finished = true;
            None
        }
    }
}

pub struct FixedGreedyFinderRev {
    finder: memchr::memmem::FinderRev<'static>,
    len: usize,
}

impl FixedGreedyFinderRev {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::FinderRev::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for FixedGreedyFinderRev {
    type Iter<'a> = FieldsLocationsGreedyRev<'a>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let iter = self.finder.rfind_iter(line);
        FieldsLocationsGreedyRev::new(iter, self.len)
    }
}

pub struct FieldsLocationsGreedyRev<'a> {
    iter: std::iter::Peekable<memchr::memmem::FindRevIter<'a, 'a>>,
    delimiter_len: usize,
    current_pos: usize,
    finished: bool,
}

impl<'a> FieldsLocationsGreedyRev<'a> {
    fn new(iter: memchr::memmem::FindRevIter<'a, 'a>, delimiter_len: usize) -> Self {
        Self {
            iter: iter.peekable(),
            delimiter_len,
            current_pos: 0,
            finished: false,
        }
    }
}

impl<'a> Iterator for FieldsLocationsGreedyRev<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Look for the delimiter in the remaining portion
        if let Some(idx) = self.iter.next() {
            let end_delimiter = idx + self.delimiter_len;
            self.current_pos = idx;

            // Skip any consecutive delimiters (greedy behavior)
            while self.iter.peek() == Some(&(self.current_pos - self.delimiter_len)) {
                self.current_pos = self.iter.next().unwrap();
            }

            Some(Range {
                start: self.current_pos,
                end: end_delimiter,
            })
        } else {
            // No more delimiters found
            self.finished = true;
            None
        }
    }
}
