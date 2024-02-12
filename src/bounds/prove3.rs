use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct SideValue(usize, usize);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Side {
    Some(SideValue),
    Continue,
}

impl Side {
    fn new(v: usize) -> Self {
        Side::Some(SideValue(v, 0))
    }

    fn new_with_offset(v: usize, offset: usize) -> Self {
        Side::Some(SideValue(v, v + offset))
    }
}

trait Num {
    type T;
    fn zero() -> Self::T;
    fn value(&self, max: Self::T) -> Self::T;
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Bounds {
    pub l: Side,
    pub r: Side,
}

pub enum BoundsOrFiller {
    Bounds(Bounds),
    Filler,
}

pub struct BoundsList {
    pub list: Vec<BoundsOrFiller>,
}

fn main() {
    let lol = BoundsList {
        list: vec![
            BoundsOrFiller::Bounds(Bounds {
                l: Side::new(1),
                r: Side::new(5),
            }),
            BoundsOrFiller::Filler,
            BoundsOrFiller::Bounds(Bounds {
                l: Side::new(1),
                r: Side::new(5),
            }),
        ],
    };

    let prove: Vec<&Bounds> = lol
        .list
        .iter()
        .filter_map(|x| match x {
            BoundsOrFiller::Bounds(bounds) => {
                println!("Bounds: {:?}", bounds);
                Some(bounds)
            }
            BoundsOrFiller::Filler => {
                println!("Filler");
                None
            }
        })
        .collect();
}
