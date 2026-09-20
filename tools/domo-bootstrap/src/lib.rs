//! `domo-bootstrap` as a library.
//!
//! The binary is a thin `clap` shell over these modules. Everything that can
//! be decided without a server lives in [`catalog`] and [`naming`] as pure
//! functions, so the role catalog and the naming scheme are tested by
//! `cargo test` rather than by a live run that needs AXIAM, a broker and a
//! database to be up.

pub mod catalog;
pub mod naming;
pub mod stages;
