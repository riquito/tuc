use crate::finders::common::DelimiterFinder;
use std::ops::Range;

pub struct FixedFinder {
    finder: memchr::memmem::Finder<'static>,
    len: usize,
}

impl FixedFinder {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::Finder::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for FixedFinder {
    type Iter<'a> =
        std::iter::Map<memchr::memmem::FindIter<'a, 'a>, Box<dyn Fn(usize) -> Range<usize> + 'a>>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let len = self.len;
        self.finder
            .find_iter(line)
            .map(Box::new(move |idx| idx..idx + len))
    }
}

pub struct FixedFinderRev {
    finder: memchr::memmem::FinderRev<'static>,
    len: usize,
}

impl FixedFinderRev {
    pub fn new(pattern: &[u8]) -> Self {
        Self {
            finder: memchr::memmem::FinderRev::new(pattern).into_owned(),
            len: pattern.len(),
        }
    }
}

impl DelimiterFinder for FixedFinderRev {
    type Iter<'a> = std::iter::Map<
        memchr::memmem::FindRevIter<'a, 'a>,
        Box<dyn Fn(usize) -> Range<usize> + 'a>,
    >;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        let len = self.len;
        self.finder
            .rfind_iter(line)
            .map(Box::new(move |idx| idx..idx + len))
    }
}
