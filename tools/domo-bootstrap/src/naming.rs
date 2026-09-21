//! The resource and group naming scheme (D-17) as pure functions.
//!
//! # Why this is one function and not a format string at each call site
//!
//! These names are the only lookup key the rest of the project has. Nothing
//! maps a domain object to its AXIAM resource or group except the name this
//! module produces: Phase 2 resolves which group to add a user to by building
//! the name, and Phase 3 explains a denial by naming the group that was
//! missing. A second call site formatting `installer@site:{slug}` by hand
//! would work until the day it did not, and the failure would be a group
//! nothing looks for rather than an error.
//!
//! # Readable name, stable identifier
//!
//! The name is for humans and for the AXIAM console. The stable key is the
//! domain identifier in the resource's metadata (see `stages/tree.rs`), which
//! is what later phases join on — so a site can be renamed without breaking
//! anything that points at it.
//!
//! # Why the slug rules are this strict
//!
//! Lowercase letters, digits and hyphens, and nothing else. Two of the
//! exclusions are not stylistic:
//!
//! - **A dot** would split an MQTT topic segment. The topic scheme maps path
//!   separators onto dots, so `lakeside.park` becomes two segments and the
//!   device's topic filter silently stops matching.
//! - **A colon** would repoint RabbitMQ's virtual-host split, which happens at
//!   the *last* colon of the connection's user name.
//!
//! Neither fails where the slug is accepted. Both fail later, in the broker,
//! as a connection refused for no visible reason — which is exactly the kind
//! of bug a validator at the boundary is for.

use anyhow::{Result, bail};

/// A node of the resource hierarchy.
///
/// Devices hang off a common area or an apartment, never directly off a site
/// or a building. That is what keeps an operate grant from cascading into an
/// apartment, and it is why the model needs no deny rule anywhere.
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
///
/// Both common areas share the `common` discriminator: they differ in where
/// they sit in the tree, not in what they are, and the group templates of
/// `authz/catalog.toml` are keyed by this value.
#[must_use]
pub fn resource_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Portfolio => "portfolio",
        Kind::Site => "site",
        Kind::SiteCommon | Kind::BuildingCommon => "common",
        Kind::Building => "building",
        Kind::Apartment => "apartment",
        Kind::Device => "device",
    }
}

/// The type prefix a resource name carries.
fn prefix(kind: Kind) -> &'static str {
    match kind {
        Kind::Portfolio => "portfolio",
        Kind::Site => "site",
        Kind::SiteCommon => "common:site",
        Kind::Building => "building",
        Kind::BuildingCommon => "common:building",
        Kind::Apartment => "apartment",
        Kind::Device => "device",
    }
}

/// The readable, type-prefixed resource name (D-17).
///
/// A portfolio takes no slug: a tenant has exactly one root, so the bare
/// `portfolio` is already unique within it.
pub fn resource_name(kind: Kind, slug: &str) -> Result<String> {
    if kind == Kind::Portfolio {
        return Ok(prefix(kind).to_owned());
    }
    validate_slug(slug)?;
    Ok(format!("{}:{slug}", prefix(kind)))
}

/// The `{building}-{unit}` slug an apartment is named by.
///
/// Composed rather than free-form so unit `1` of two different towers cannot
/// collide — AXIAM has no uniqueness index on resource names (P-8), so two
/// resources called `apartment:1` would both exist and an authorization
/// decision about "apartment 1" would be ambiguous.
pub fn apartment_slug(building: &str, unit: &str) -> Result<String> {
    validate_slug(building)?;
    validate_slug(unit)?;
    Ok(format!("{building}-{unit}"))
}

/// The group that holds `role` scoped to one resource (D-17).
///
/// Always `{role}@{resource name}`, so the group name contains the resource
/// name verbatim and a human reading a denial can see both at once.
pub fn group_name(role: &str, kind: Kind, slug: &str) -> Result<String> {
    if role.trim().is_empty() {
        bail!("a group needs a role name");
    }
    validate_slug(role)?;
    Ok(format!("{role}@{}", resource_name(kind, slug)?))
}

/// Accept only the slug form every downstream system can carry.
///
/// Lowercase letters, digits and hyphens. See the module docs for why the dot
/// and the colon are called out by name in the error.
pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        bail!("a slug must not be empty");
    }
    if let Some(c) = slug.chars().find(|c| *c == '.') {
        bail!(
            "slug '{slug}' contains '{c}': the MQTT topic scheme maps path separators \
             onto dots, so a dot here would split a topic segment"
        );
    }
    if let Some(c) = slug.chars().find(|c| *c == ':') {
        bail!(
            "slug '{slug}' contains '{c}': the broker splits a virtual host from a user \
             name at the last colon, so a colon here would repoint that split"
        );
    }
    if let Some(c) = slug
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
    {
        bail!(
            "slug '{slug}' contains '{c}': only lowercase letters, digits and hyphens \
             are accepted"
        );
    }
    Ok(())
}

/// The structural groups created eagerly with a resource of this kind (D-21).
///
/// Empty for a device: its `granted-operator` group is created on the first
/// grant in Phase 2 and deleted on revoke, so creating it eagerly would leave
/// an empty group on every device — which reads as a grant that is not there.
///
/// Empty for a building too: a building's own devices live under its
/// `common:building` node, and an installer reaches its buildings through the
/// site-level binding, which cascades.
///
/// This set must equal the `[[group_template]]` blocks of
/// `authz/catalog.toml`; `tests/naming.rs` asserts that they have not drifted.
#[must_use]
pub fn structural_groups_for(kind: Kind) -> Vec<&'static str> {
    match kind {
        Kind::Portfolio => vec!["property-manager"],
        Kind::Site => vec!["installer", "concierge"],
        Kind::SiteCommon | Kind::BuildingCommon => {
            vec!["common-operator", "common-device-manager"]
        }
        Kind::Apartment => vec!["resident"],
        Kind::Building | Kind::Device => Vec::new(),
    }
}
