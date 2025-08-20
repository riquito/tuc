use crate::finders::common::DelimiterFinder;
use regex::bytes::Regex;
use std::ops::Range;

pub struct RegexFinder {
    regex: Regex,
    trim_empty: bool,
}

impl RegexFinder {
    pub fn new(regex: Regex, trim_empty: bool) -> Self {
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
