#![forbid(unsafe_code)]
//! Application core for the Recording bounded context.
//!
//! [`domain`] holds the aggregate and its invariants, [`ports`] the
//! application-owned outbound contracts, [`data_plane`] the bounded media
//! sinks that connect capture adapters to the media pipeline, and
//! [`application`] the use cases that coordinate the aggregate through those
//! ports. With the `contract-tests` feature (or in this crate's own tests),
//! [`contract`] provides shared port contract suites and the fakes that pass
//! them.

pub mod application;
pub mod data_plane;
pub mod domain;
pub mod ports;
pub mod timeline;

#[cfg(any(test, feature = "contract-tests"))]
pub mod contract;

pub use application::*;
pub use domain::*;
