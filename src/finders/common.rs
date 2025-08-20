use std::ops::Range;

pub trait DelimiterFinder {
    type Iter<'a>: Iterator<Item = Range<usize>> + 'a
    where
        Self: 'a;
    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a>;
}
