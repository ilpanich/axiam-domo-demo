//! The resource and group naming scheme (D-17) as pure functions.
//!
//! RED phase: the types and signatures are settled so `tests/naming.rs` can
//! state the behaviour. The bodies are deliberately wrong; the tests fail on
//! their own assertions, which is what authorises the GREEN commit.

use anyhow::Result;

/// A node of the resource hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A tenant's root.
    Portfolio,
    /// A property.
    Site,
    /// A site's common area — where site-level devices live.
    SiteCommon,
    /// A building within a site.
    Building,
    /// A building's common area — where building-level devices live.
    BuildingCommon,
    /// A dwelling.
    Apartment,
    /// A single device.
    Device,
}

/// AXIAM's `resource_type` discriminator for a kind.
#[must_use]
pub fn resource_type(_kind: Kind) -> &'static str {
    ""
}

/// The readable, type-prefixed resource name (D-17).
pub fn resource_name(_kind: Kind, _slug: &str) -> Result<String> {
    Ok(String::new())
}

/// The `{building}-{unit}` slug an apartment is named by.
pub fn apartment_slug(_building: &str, _unit: &str) -> Result<String> {
    Ok(String::new())
}

/// The group that holds `role` scoped to one resource (D-17).
pub fn group_name(_role: &str, _kind: Kind, _slug: &str) -> Result<String> {
    Ok(String::new())
}

/// Accept only the slug form every downstream system can carry.
pub fn validate_slug(_slug: &str) -> Result<()> {
    Ok(())
}

/// The structural groups created eagerly with a resource of this kind (D-21).
#[must_use]
pub fn structural_groups_for(_kind: Kind) -> Vec<&'static str> {
    Vec::new()
}
