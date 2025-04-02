use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Side<T> {
    Some(T),
    Continue,
}

trait Num {
    type T;
    fn zero() -> Self::T;
    fn value(&self, max: Self::T) -> Self::T;
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Bounds<T> {
    pub l: Side<T>,
    pub r: Side<T>,
}

pub enum BoundsOrFiller<T> {
    Bounds(Bounds<T>),
    Filler,
}

pub struct BoundsList<T> {
    pub list: Vec<BoundsOrFiller<T>>,
}

fn main() {
    let lol = BoundsList {
        list: vec![
            BoundsOrFiller::Bounds(Bounds {
                l: Side::Some(1),
                r: Side::Some(5),
            }),
            BoundsOrFiller::Filler,
            BoundsOrFiller::Bounds(Bounds {
                l: Side::Some(1),
                r: Side::Some(5),
            }),
        ],
    };

    let prove: Vec<&Bounds<i32>> = lol
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
