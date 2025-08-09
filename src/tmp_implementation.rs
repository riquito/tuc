// let find_iter = if opt.regex_bag.is_some() && cfg!(feature = "regex") {
//     #[cfg(feature = "regex")]
//     {
//         opt.regex_bag.as_ref().unwrap().greedy.find_iter
//     }
// } else {
//     memchr::memmem::Finder::new(opt.delimiter.as_bytes()).find_iter
// };

// Simple enum for delimiter strategy: Regex or Finder (owned)
enum DelimiterStrategy<'a> {
    #[cfg(feature = "regex")]
    Regex(regex::bytes::Regex),
    Memmem(memchr::memmem::Finder<'a>, usize),
}

impl<'a> DelimiterStrategy<'a> {
    fn find_ranges(&'a self, line: &'a [u8]) -> DelimiterFindIter<'a> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterStrategy::Regex(re) => DelimiterFindIter::Regex(re.find_iter(line)),
            DelimiterStrategy::Memmem(finder, len) => {
                DelimiterFindIter::Memmem(finder.find_iter(line), *len)
            }
        }
    }
}

enum DelimiterFindIter<'a> {
    #[cfg(feature = "regex")]
    Regex(regex::bytes::Matches<'a, 'a>),
    Memmem(memchr::memmem::FindIter<'a, 'a>, usize),
}

impl<'a> Iterator for DelimiterFindIter<'a> {
    type Item = Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(feature = "regex")]
            DelimiterFindIter::Regex(iter) => iter.next().map(|m| m.start()..m.end()),
            DelimiterFindIter::Memmem(iter, len) => iter.next().map(|idx| idx..idx + *len),
        }
    }
}

fn main() {
    // Example usage of the DelimiterStrategy
    let opt = Opt {
        delimiter: b"delimiter".to_vec(),
        eol: EOL::Newline,
        bounds: UserBoundsList::default(),
        bounds_type: BoundsType::Fields,
        only_delimited: false,
        greedy_delimiter: false,
        compress_delimiter: false,
        replace_delimiter: None,
        trim: None,
        version: false,
        complement: false,
        join: false,
        json: false,
        fixed_memory: None,
        fallback_oob: None,
    };
    // Build the delimiter strategy once
    let delimiter_strategy = if let Some(ref regex_bag) = opt.regex_bag {
        #[cfg(feature = "regex")]
        {
            DelimiterStrategy::Regex(regex_bag.greedy.clone())
        }
        #[cfg(not(feature = "regex"))]
        {
            unreachable!()
        }
    } else {
        let finder = memchr::memmem::Finder::new(&opt.delimiter).into_owned();
        let len = opt.delimiter.len();
        DelimiterStrategy::Memmem(finder, len)
    };
    // Example usage (remove if not needed):
    // let k = delimiter_strategy.find_ranges("a".as_bytes());
}
