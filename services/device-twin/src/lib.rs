//! Device Twin — library half.
//!
//! The binary ([`main.rs`](../src/main.rs)) is a thin shell that reads the
//! environment, builds TLS and starts the server. Everything decidable lives
//! here so it can be exercised from `tests/` with no server, no broker and no
//! network:
//!
//! - [`rmq::decide`] — the four authorization decisions as pure functions.
//! - [`rmq::forms`] — one strict request struct per broker endpoint.
//! - [`rmq`] — the thin handler layer that joins the two.
//! - [`tenants`] — the per-tenant verifier registry and the session cache.

pub mod rmq;
pub mod tenants;
