//! The lane every client of a Microduck shares: robotd over its unix
//! socket, the gait's limits, and the moves a body makes.
//!
//! Split out of `quacksat-core` on 2026-09-22 (the user's: quacksat is a
//! voice satellite, the navigation is its own repo — and both of them
//! talk to the same daemon through the same socket).
pub mod body;
pub mod gait;
pub mod lane;

pub use lane::Control;
