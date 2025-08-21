use crate::finders::common::DelimiterFinder;
use regex::bytes::Regex;
use std::ops::Range;

pub struct RegexFinder {
    regex: Regex,
    trim_empty: bool,
}

impl RegexFinder {
    pub fn new(regex: Regex) -> Self {
        Self::new_with_trim(regex, false)
    }

    pub fn new_with_trim(regex: Regex, trim_empty: bool) -> Self {
        Self { regex, trim_empty }
    }
}

impl DelimiterFinder for RegexFinder {
    type Iter<'a> = Box<dyn Iterator<Item = Range<usize>> + 'a>;

    fn find_ranges<'a>(&'a self, line: &'a [u8]) -> Self::Iter<'a> {
        if self.trim_empty {
            let line_len = line.len();
            Box::new(
                self.regex
                    .find_iter(line)
                    .filter(move |m| {
                        !((m.start() == 0 && m.end() == 0)
                            || (m.start() == line_len && m.end() == line_len))
                    })
                    .map(|m| m.start()..m.end()),
            )
        } else {
            Box::new(self.regex.find_iter(line).map(|m| m.start()..m.end()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::LazyLock;

    use super::*;

    static SMALL_DELIM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::from_str("[-]").unwrap());
    static BIG_DELIM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::from_str("-{2}").unwrap());

    #[test]
    fn forward_small_delimiter_empty_line() {
        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);
    }

    #[test]
    fn forward_small_delimiter_only_delimiter() {
        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"-").collect();
        assert_eq!(vec![0..1], result);
    }

    #[test]
    fn forward_small_delimiter_regular_scenarios() {
        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);

        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b").collect();
        assert_eq!(vec![1..2], result);

        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b-c").collect();
        assert_eq!(vec![1..2, 3..4], result);

        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a-b-c-").collect();
        assert_eq!(vec![1..2, 3..4, 5..6], result);

        let finder = RegexFinder::new(SMALL_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"-a-b-c").collect();
        assert_eq!(vec![0..1, 2..3, 4..5], result);
    }

    #[test]
    fn forward_big_delimiter_empty_line() {
        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);
    }

    #[test]
    fn forward_big_delimiter_only_delimiter() {
        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"--").collect();
        assert_eq!(vec![0..2], result);
    }

    #[test]
    fn forward_big_delimiter_regular_scenarios() {
        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a").collect();
        assert_eq!(Vec::<Range<usize>>::new(), result);

        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b").collect();
        assert_eq!(vec![1..3], result);

        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b--c").collect();
        assert_eq!(vec![1..3, 4..6], result);

        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());
        let result: Vec<Range<usize>> = finder.find_ranges(b"a--b--c--").collect();
        assert_eq!(vec![1..3, 4..6, 7..9], result);
        let finder = RegexFinder::new(BIG_DELIM_REGEX.clone());

        let result: Vec<Range<usize>> = finder.find_ranges(b"--a--b--c").collect();
        assert_eq!(vec![0..2, 3..5, 6..8], result);
    }
}
