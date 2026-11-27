#![forbid(unsafe_code)]
//! Application core for the Recording bounded context.
//!
//! [`domain`] holds the aggregate and its invariants, [`ports`] the
//! application-owned outbound contracts, and [`application`] the use cases
//! that coordinate the aggregate through those ports.

pub mod application;
pub mod domain;
pub mod ports;

pub use application::*;
pub use domain::*;
