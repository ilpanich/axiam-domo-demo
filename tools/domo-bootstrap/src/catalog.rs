//! `authz/catalog.toml` — the declarative role and permission catalog (D-16).
//!
//! RED phase: the types and the signatures are settled here so the tests in
//! `tests/catalog.rs` can state the behaviour they expect. Validation and the
//! manifest projection are deliberately absent; the tests fail on their own
//! assertions, which is what authorises the GREEN commit that fills them in.

use anyhow::Result;
use axiam_sdk::management::manifest::ManagementManifest;
use serde::Deserialize;

/// One permission — an action, tenant-wide.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionEntry {
    /// The `resource:verb` pair. Its own natural key.
    pub action: String,
    /// Human-readable description. AXIAM requires one.
    pub description: String,
}

/// One role and the actions it grants.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleEntry {
    /// The role's name — its natural key within a tenant.
    pub name: String,
    /// Human-readable description. AXIAM requires one.
    pub description: String,
    /// The actions this role grants, each naming a [`PermissionEntry`].
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// The structural groups a resource type gets eagerly (D-21).
#[derive(Debug, Clone, Deserialize)]
pub struct GroupTemplate {
    /// The AXIAM `resource_type` discriminator this template applies to.
    pub resource_type: String,
    /// The role names whose groups are created with the resource.
    #[serde(default)]
    pub roles: Vec<String>,
}

/// The whole catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    /// Schema version, so a later incompatible shape can be detected.
    pub version: u32,
    /// The group-name pattern, shared with `naming.rs` and Phase 2.
    pub group_pattern: String,
    /// `[[permission]]` blocks.
    #[serde(default, rename = "permission")]
    pub permissions: Vec<PermissionEntry>,
    /// `[[role]]` blocks.
    #[serde(default, rename = "role")]
    pub roles: Vec<RoleEntry>,
    /// `[[group_template]]` blocks.
    #[serde(default, rename = "group_template")]
    pub group_templates: Vec<GroupTemplate>,
}

/// Deserialize a catalog and validate it.
pub fn parse(src: &str) -> Result<Catalog> {
    let catalog: Catalog = toml::from_str(src)?;
    catalog.validate()?;
    Ok(catalog)
}

impl Catalog {
    /// Reject a catalog that cannot be applied, before any network call.
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Project the catalog onto the SDK's declarative management manifest.
    #[must_use]
    pub fn to_manifest(&self) -> ManagementManifest {
        ManagementManifest::new()
    }

    /// The eager structural roles for `resource_type` (D-21).
    #[must_use]
    pub fn roles_for(&self, _resource_type: &str) -> Vec<String> {
        Vec::new()
    }
}
