#![forbid(unsafe_code)]
//! Application core for the Recording bounded context.

#[rustfmt::skip]
mod implementation;

pub use implementation::*;
pub mod ports;
