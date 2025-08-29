use crate::bounds::{BoundOrFiller, Side, UserBounds, UserBoundsTrait};
use anyhow::{Result, bail};
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct UserBoundsList {
    pub list: Vec<BoundOrFiller>,
    /// Optimization that we can use to stop searching for fields.
    /// It's available only when every bound uses positive indexes.
    /// When conditions do not apply, its value is `Side::Continue`.
    pub last_interesting_field: Side,
}

impl Deref for UserBoundsList {
    type Target = Vec<BoundOrFiller>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl From<Vec<BoundOrFiller>> for UserBoundsList {
    fn from(list: Vec<BoundOrFiller>) -> Self {
        let mut ubl = UserBoundsList {
            list,
            last_interesting_field: Side::Continue,
        };

        let mut rightmost_bound: Option<Side> = None;
        let mut last_bound: Option<&mut UserBounds> = None;

        let is_sortable = ubl.is_sortable();

        ubl.list.iter_mut().for_each(|bof| {
            if let BoundOrFiller::Bound(b) = bof {
                if rightmost_bound.is_none() || *b.r() > rightmost_bound.unwrap() {
                    rightmost_bound = Some(*b.r());
                }

                last_bound = Some(b);
            }
        });

        if !is_sortable {
            rightmost_bound = None;
        }

        last_bound
            .expect("UserBoundsList must contain at least one UserBounds")
            .set_is_last(true);

        ubl.last_interesting_field = rightmost_bound.unwrap_or(Side::Continue);
        ubl
    }
}

impl FromStr for UserBoundsList {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            bail!("UserBoundsList must contain at least one UserBounds");
        }
        Ok(parse_bounds_list(s)?.into())
    }
}

impl UserBoundsList {
    /// Detect whether the list can be sorted.
    /// It can be sorted only if every bound
    /// has the same sign (all positive or all negative).
    pub fn is_sortable(&self) -> bool {
        let mut has_positive_idx = false;
        let mut has_negative_idx = false;
        self.get_userbounds_only().for_each(|b| {
            if let Side::Some(left) = b.l() {
                if left.is_positive() {
                    has_positive_idx = true;
                } else {
                    has_negative_idx = true;
                }
            }

            if let Side::Some(right) = b.r() {
                if right.is_positive() {
                    has_positive_idx = true;
                } else {
                    has_negative_idx = true;
                }
            }
        });

        !(has_negative_idx && has_positive_idx)
    }

    fn get_userbounds_only(&self) -> impl Iterator<Item = &UserBounds> + '_ {
        self.list.iter().flat_map(|b| match b {
            BoundOrFiller::Bound(x) => Some(x),
            _ => None,
        })
    }

    fn is_sorted(&self) -> bool {
        let mut prev_b: Option<&UserBounds> = None;
        for b in self.get_userbounds_only() {
            if prev_b.is_none() || prev_b <= Some(b) {
                prev_b = Some(b);
            } else {
                return false;
            }
        }

        true
    }

    fn has_negative_indices(&self) -> bool {
        self.get_userbounds_only().any(|b| {
            if let Side::Some(left) = b.l()
                && left.is_negative()
            {
                true
            } else if let Side::Some(right) = b.r()
                && right.is_negative()
            {
                true
            } else {
                false
            }
        })
    }

    /// Check if the bounds in the list match the following conditions:
    /// - they are in ascending order
    /// - they use solely positive indices
    /// - they don't overlap (but they can be adjacent, e.g. 1:2,2,3)
    pub fn is_forward_only(&self) -> bool {
        self.is_sortable() && self.is_sorted() && !self.has_negative_indices()
    }

    /// Create a new UserBoundsList with every ranged bound converted
    /// into single-field bounds.
    ///
    /// ```rust
    /// # use tuc::bounds::{UserBoundsList, UserBoundsTrait};
    /// # use std::ops::Range;
    /// # use tuc::bounds::Side;
    /// # use std::str::FromStr;
    ///
    /// assert_eq!(
    ///   UserBoundsList::from_str("1:3,4,-2:").unwrap().unpack(6).list,
    ///   UserBoundsList::from_str("1,2,3,4,5,6").unwrap().list,
    /// );
    /// ```
    pub fn unpack(&self, num_fields: usize) -> UserBoundsList {
        let list: Vec<BoundOrFiller> = self
            .list
            .iter()
            .flat_map(|bof| match bof {
                // XXX how to do it using only iterators, no collect?
                BoundOrFiller::Bound(b) => b
                    .unpack(num_fields)
                    .into_iter()
                    .map(BoundOrFiller::Bound)
                    .collect(),
                BoundOrFiller::Filler(f) => vec![BoundOrFiller::Filler(f.clone())],
            })
            .collect();

        list.into()
    }

    /// Create a new UserBoundsList with every range complemented (inverted).
    pub fn complement(&self, num_fields: usize) -> Result<UserBoundsList> {
        let list: Vec<BoundOrFiller> = self
            .list
            .iter()
            .flat_map(|bof| match bof {
                // XXX how to do it using only iterators, no collect?
                BoundOrFiller::Bound(b) => anyhow::Ok(
                    b.complement(num_fields)?
                        .into_iter()
                        .map(BoundOrFiller::Bound)
                        .collect(),
                ),
                BoundOrFiller::Filler(f) => Ok(vec![BoundOrFiller::Filler(f.clone())]),
            })
            .flatten()
            .collect();

        if list.is_empty() {
            bail!("the complement is empty");
        }

        Ok(list.into())
    }
}

/**
 * Parse bound string. It can contain formatting elements or not.
 *
 * Valid bounds formats are e.g. 1 / -1 / 1:3 / :3 / 1: / 1,4
 * If '{' is present, the string is considered to be a format string:
 * in that case everything inside {} is considered a bound, and the rest
 * just some text to display when the bounds are found.
 * e.g. "Hello {1}, found {1:3} and {2,4}"
 */
pub fn parse_bounds_list(s: &str) -> Result<Vec<BoundOrFiller>> {
    if s.is_empty() {
        return Ok(Vec::new());
    }

    if s.contains(['{', '}']) {
        let mut bof: Vec<BoundOrFiller> = Vec::new();
        let mut inside_bound = false;
        let mut part_start = 0;

        let mut iter = s.char_indices().peekable();
        while let Some((idx, w0)) = iter.next() {
            let w1 = iter.peek().unwrap_or(&(0, 'x')).1;

            if w0 == w1 && (w0 == '{' || w0 == '}') {
                // escaped bracket, ignore it, we will replace it later
                iter.next();
            } else if w0 == '}' && !inside_bound {
                bail!("Field format error: missing opening parenthesis",);
            } else if w0 == '{' {
                // starting a new bound
                inside_bound = true;

                if idx - part_start > 0 {
                    bof.push(BoundOrFiller::Filler(
                        s[part_start..idx]
                            .replace("{{", "{")
                            .replace("}}", "}")
                            .replace("\\n", "\n")
                            .replace("\\t", "\t")
                            .into_bytes(),
                    ));
                }

                part_start = idx + 1;
            } else if w0 == '}' {
                // ending a bound
                inside_bound = false;

                // consider also comma separated bounds
                for maybe_bounds in s[part_start..idx].split(',') {
                    bof.push(BoundOrFiller::Bound(UserBounds::from_str(maybe_bounds)?));
                }

                part_start = idx + 1;
            }
        }

        if inside_bound {
            bail!("Field format error: missing closing parenthesis");
        } else if s.len() - part_start > 0 {
            bof.push(BoundOrFiller::Filler(
                s[part_start..]
                    .replace("{{", "{")
                    .replace("}}", "}")
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .into_bytes(),
            ));
        }

        Ok(bof)
    } else {
        let k: Result<Vec<BoundOrFiller>, _> = s
            .split(',')
            .map(|x| UserBounds::from_str(x).map(BoundOrFiller::Bound))
            .collect();
        Ok(k?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bounds_list() {
        // do not replicate tests from test_user_bounds_from_str, focus on
        // multiple bounds, bounds with format, and special cases (empty/one)

        assert_eq!(parse_bounds_list("").unwrap(), Vec::new());

        assert_eq!(
            &parse_bounds_list(",").unwrap_err().to_string(),
            "Field format error: empty field"
        );

        assert_eq!(
            &parse_bounds_list("{").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("}").unwrap_err().to_string(),
            "Field format error: missing opening parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{1}{").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{1}}").unwrap_err().to_string(),
            "Field format error: missing closing parenthesis"
        );

        assert_eq!(
            &parse_bounds_list("{{1}").unwrap_err().to_string(),
            "Field format error: missing opening parenthesis"
        );

        assert_eq!(
            parse_bounds_list("1").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(1)
            ))],
        );

        assert_eq!(
            parse_bounds_list("1:").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::default())],
        );

        assert_eq!(
            parse_bounds_list("1,2").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
            ],
        );

        assert_eq!(
            parse_bounds_list("-1,1").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(-1), Side::Some(-1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
            ],
        );

        assert_eq!(
            parse_bounds_list("{1}").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(1)
            ))],
        );

        assert_eq!(
            parse_bounds_list("{1:2}").unwrap(),
            vec![BoundOrFiller::Bound(UserBounds::new(
                Side::Some(1),
                Side::Some(2)
            ))],
        );

        assert_eq!(
            parse_bounds_list("{1,2}").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2)))
            ],
        );

        assert_eq!(
            parse_bounds_list("hello {1,2} {{world}}").unwrap(),
            vec![
                BoundOrFiller::Filler("hello ".into()),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler(" {world}".into()),
            ],
        );

        assert_eq!(
            parse_bounds_list("{1}😎{2}").unwrap(),
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Filler("😎".into()),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2)))
            ],
        );

        assert_eq!(
            parse_bounds_list("\\n\\t{{}}{1,2}\\n\\t{{}}").unwrap(),
            vec![
                BoundOrFiller::Filler("\n\t{}".into()),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler("\n\t{}".into()),
            ],
        );
    }

    #[test]
    fn test_user_bounds_cannot_be_empty() {
        assert!(UserBoundsList::from_str("").is_err());
    }

    #[test]
    fn test_user_bounds_is_sortable() {
        assert!(UserBoundsList::from_str("1").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("1,2").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("3,2").unwrap().is_sortable());

        assert!(!UserBoundsList::from_str("-1,1").unwrap().is_sortable());

        assert!(UserBoundsList::from_str("-1,-2").unwrap().is_sortable());

        assert!(!UserBoundsList::from_str("-1:,:1").unwrap().is_sortable());
    }

    #[test]
    fn test_vec_of_bounds_is_sorted() {
        assert!(UserBoundsList::from_str("1").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,2").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("-2,-1").unwrap().is_sorted());

        assert!(UserBoundsList::from_str(":1,2:4,5:").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,1:2").unwrap().is_sorted());

        assert!(UserBoundsList::from_str("1,1,2").unwrap().is_sorted());

        assert!(!UserBoundsList::from_str("1,2,1").unwrap().is_sorted());
    }

    #[test]
    fn test_vec_of_bounds_is_forward_only() {
        assert!(
            UserBoundsList::from_str("{1}foo{2}")
                .unwrap()
                .is_forward_only()
        );

        assert!(
            !UserBoundsList::from_str("{2}foo{1}")
                .unwrap()
                .is_forward_only()
        );
    }

    #[test]
    fn test_vec_of_bounds_can_unpack() {
        assert_eq!(
            UserBoundsList::from_str("1,:1,2:3,4:")
                .unwrap()
                .unpack(4)
                .list,
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(3), Side::Some(3))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(4), Side::Some(4))),
            ]
        );

        assert_eq!(
            UserBoundsList::from_str("a{1:2}b").unwrap().unpack(4).list,
            vec![
                BoundOrFiller::Filler("a".into()),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(2), Side::Some(2))),
                BoundOrFiller::Filler("b".into()),
            ]
        );
    }

    #[test]
    fn test_vec_of_bounds_can_complement() {
        assert_eq!(
            UserBoundsList::from_str("1:2,2:3,5,-2")
                .unwrap()
                .complement(6)
                .unwrap()
                .list,
            vec![
                BoundOrFiller::Bound(UserBounds::new(Side::Some(3), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(1))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(4), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(4))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(6), Side::Some(6))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(1), Side::Some(4))),
                BoundOrFiller::Bound(UserBounds::new(Side::Some(6), Side::Some(6))),
            ]
        );

        assert_eq!(
            UserBoundsList::from_str("1:")
                .unwrap()
                .complement(6)
                .err()
                .map(|x| x.to_string()),
            Some("the complement is empty".to_owned())
        );
    }
}
