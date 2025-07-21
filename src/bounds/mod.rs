pub mod side;
pub mod userbounds;
pub mod userboundslist;

#[derive(Debug, PartialEq)]
pub enum BoundsType {
    Bytes,
    Characters,
    Fields,
    Lines,
}

// pub mod side_new;
pub use side::Side;
pub use userbounds::{BoundOrFiller, UserBounds, UserBoundsTrait};
pub use userboundslist::UserBoundsList;
