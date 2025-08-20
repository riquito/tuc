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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_small_delimiter_empty_line() {
        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);
    }

    #[test]
    fn forward_small_delimiter_only_delimiter() {
        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"-").collect();
        assert_eq!(vec![0..1], result);
    }

    #[test]
    fn forward_small_delimiter_regular_scenarios() {
        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);

        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b").collect();
        assert_eq!(vec![1..2], result);

        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b-c").collect();
        assert_eq!(vec![1..2, 3..4], result);

        let finder = FixedFinder::new(b"-");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b-c-").collect();
        assert_eq!(vec![1..2, 3..4, 5..6], result);
    }

    #[test]
    fn forward_big_delimiter_empty_line() {
        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);
    }

    #[test]
    fn forward_big_delimiter_only_delimiter() {
        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"--").collect();
        assert_eq!(vec![0..2], result);
    }

    #[test]
    fn forward_big_delimiter_regular_scenarios() {
        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);

        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b").collect();
        assert_eq!(vec![1..3], result);

        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b--c").collect();
        assert_eq!(vec![1..3, 4..6], result);

        let finder = FixedFinder::new(b"--");
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b--c--").collect();
        assert_eq!(vec![1..3, 4..6, 7..9], result);
    }
}
