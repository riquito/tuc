use anyhow::{bail, Result};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Side<T> {
    Some(T),
    Continue,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct Rel(i32);
#[derive(Debug, PartialEq, PartialOrd)]
pub struct Abs(usize);

trait Num {
    type T;
    fn zero() -> Self::T;
    fn value(&self) -> Self::T;
}

impl Num for Rel {
    type T = i32;
    fn zero() -> T {
        return 0;
    }
    fn value(&self) -> T {
        return self.0;
    }
}

#[derive(Debug, PartialOrd, PartialEq)]
pub struct Bounds<T> {
    left: Side<T>,
    right: Side<T>,
}

pub type RelBounds = Bounds<Rel>;
pub type AbsBounds = Bounds<Abs>;

pub enum BoundsOrFiller<T> {
    Bounds(Bounds<T>),
    Filler,
}

pub struct BoundsList<T> {
    pub list: Vec<BoundsOrFiller<T>>,
}

fn bigger_than(lhs: &Bounds<Rel>, rhs: &Bounds<Rel>) -> bool {
    lhs.left.partial_cmp(&rhs.left).is_some()
}

fn foobar(lhs: &Bounds<Rel>) -> bool {
    match &lhs.left {
        Side::Some(Rel(x)) => {
            *x > 0;
        }
        Side::Continue => {}
    };

    true
}

fn foobar222<T: Num>(lhs: &Bounds<T>) -> bool {
    match &lhs.left {
        Side::Some(x) => {
            x.value() > T::zero();
        }
        Side::Continue => {}
    };

    true
}

fn main() {
    let lol = BoundsList {
        list: vec![
            BoundsOrFiller::Bounds(Bounds {
                left: Side::Some(Rel(1)),
                right: Side::Some(Rel(5)),
            }),
            BoundsOrFiller::Filler,
            BoundsOrFiller::Bounds(Bounds {
                left: Side::Some(Rel(1)),
                right: Side::Some(Rel(5)),
            }),
        ],
    };

    let prove: Vec<&Bounds<Rel>> = lol
        .list
        .iter()
        .filter_map(|x| match (x) {
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
