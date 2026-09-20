//! Shared building blocks for the AXIAM Domo Demo.
//!
//! Every binary in this workspace — `domo-bootstrap`, `domo-probe` and
//! `device-twin` — talks to the same AXIAM, trusts the same organization root
//! and addresses the same MQTT topic space. Those three facts live here so the
//! binaries cannot drift apart on any of them.

pub mod axiam;
pub mod hand_rolled;
pub mod secrets;
pub mod tls;
pub mod topic;

/// The reserved tenant slug for organization-level work (CONTRACT §5.2.1).
pub const ORG_TENANT_SLUG: &str = "organization";

/// The MQTT vhost devices connect to. AXIAM's own `/` vhost is never touched.
pub const DOMO_VHOST: &str = "domo";
