//! `authz/catalog.toml` — the declarative role and permission catalog (D-16).
//!
//! # Why this is declarative and the rest of the tree is not
//!
//! Roles and permissions are tenant-wide and carry no resource scope, which is
//! exactly the shape the SDK's [`ManagementManifest`] describes. Applying a
//! manifest is idempotent by construction: it deletes nothing, treats a field
//! it does not state as "not a difference", and converges — the second `plan`
//! over unchanged state is all no-change. So the catalog needs no
//! exists-check loop of its own.
//!
//! The rest of Phase 1's AXIAM state does not fit: DF-011 records that the
//! manifest can carry neither resource metadata (which is where the stable
//! domain identifier lives, D-17), nor a resource-scoped group-to-role
//! binding (which is the entire group-indirection model), nor service
//! accounts. Those are created imperatively in `stages/tree.rs` and
//! `stages/service_certs.rs`.
//!
//! # Validation happens before the first request
//!
//! The SDK refuses a structurally broken manifest — dangling keys, cycles —
//! before it issues anything. That check cannot see the rules that are ours
//! rather than AXIAM's: D-16 forbids a wildcard outright, and a role granting
//! an action nothing defines is a typo we want named at the offending line
//! rather than reported as a missing key. [`Catalog::validate`] runs first and
//! names what it found.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use axiam_sdk::management::manifest::{
    GrantSpec, ManagementManifest, PermissionSpec, RoleSpec,
};
use serde::Deserialize;

/// Default location of the catalog, relative to the repository root.
pub const DEFAULT_PATH: &str = "authz/catalog.toml";

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

/// Deserialize a catalog and validate it. Reaches no network.
pub fn parse(src: &str) -> Result<Catalog> {
    let catalog: Catalog = toml::from_str(src).context("parsing the catalog TOML")?;
    catalog.validate()?;
    Ok(catalog)
}

/// Read and validate the catalog at `path`.
pub fn load(path: impl AsRef<Path>) -> Result<Catalog> {
    let path = path.as_ref();
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading the catalog at {}", path.display()))?;
    parse(&src).with_context(|| format!("in {}", path.display()))
}

/// Anything AXIAM might one day read as "all of them".
///
/// Checked as a character class rather than the literal `*`: D-16's rule is
/// "every `resource:verb` pair is written out", and a `%` or `?` glob would
/// break that rule just as quietly the day a server learns to expand it.
fn wildcard_in(s: &str) -> Option<char> {
    s.chars().find(|c| matches!(c, '*' | '?' | '%'))
}

impl Catalog {
    /// Reject a catalog that cannot be applied, before any network call.
    ///
    /// Every error names the offending entry, not just the file: a catalog is
    /// read by a human who has to find the line.
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "catalog version {} is not supported by this build (expected 1)",
                self.version
            );
        }
        if self.group_pattern.trim().is_empty() {
            bail!("`group_pattern` is empty; the group naming scheme has no other source");
        }

        // --- permissions ---------------------------------------------------
        let mut actions: HashSet<&str> = HashSet::new();
        for p in &self.permissions {
            if p.action.trim().is_empty() {
                bail!("a permission has an empty action");
            }
            if let Some(c) = wildcard_in(&p.action) {
                bail!(
                    "permission '{}' contains the wildcard character '{c}'; \
                     D-16 requires every resource:verb pair to be written out",
                    p.action
                );
            }
            if p.description.trim().is_empty() {
                bail!("permission '{}' has an empty description", p.action);
            }
            if !actions.insert(p.action.as_str()) {
                bail!("permission '{}' is defined more than once", p.action);
            }
        }

        // --- roles ----------------------------------------------------------
        let mut names: HashSet<&str> = HashSet::new();
        for r in &self.roles {
            if r.name.trim().is_empty() {
                bail!("a role has an empty name");
            }
            if let Some(c) = wildcard_in(&r.name) {
                bail!("role '{}' contains the wildcard character '{c}'", r.name);
            }
            if r.description.trim().is_empty() {
                bail!("role '{}' has an empty description", r.name);
            }
            if !names.insert(r.name.as_str()) {
                bail!("role '{}' is defined more than once", r.name);
            }

            let mut granted: HashSet<&str> = HashSet::new();
            for action in &r.permissions {
                if let Some(c) = wildcard_in(action) {
                    bail!(
                        "role '{}' grants '{action}', which contains the wildcard \
                         character '{c}'; D-16 requires it to be expanded",
                        r.name
                    );
                }
                if !actions.contains(action.as_str()) {
                    bail!(
                        "role '{}' grants '{action}', which no [[permission]] block defines",
                        r.name
                    );
                }
                if !granted.insert(action.as_str()) {
                    bail!("role '{}' grants '{action}' more than once", r.name);
                }
            }
        }

        // --- group templates ------------------------------------------------
        let mut types: HashSet<&str> = HashSet::new();
        for t in &self.group_templates {
            if t.resource_type.trim().is_empty() {
                bail!("a group template has an empty resource_type");
            }
            if !types.insert(t.resource_type.as_str()) {
                bail!(
                    "group template for resource type '{}' is defined more than once",
                    t.resource_type
                );
            }
            for role in &t.roles {
                if !names.contains(role.as_str()) {
                    bail!(
                        "group template for '{}' names role '{role}', which no [[role]] \
                         block defines",
                        t.resource_type
                    );
                }
            }
        }

        Ok(())
    }

    /// Project the catalog onto the SDK's declarative management manifest.
    ///
    /// The manifest-local key of a permission is its own action and that of a
    /// role its own name: both are already unique within a tenant (validation
    /// above proves it), so a second, synthetic key would only be one more
    /// thing to keep in step.
    ///
    /// Groups are deliberately NOT projected. A structural group's whole point
    /// is the resource its role binding is scoped to, and the manifest carries
    /// no resource scope on a group-to-role binding (DF-011) — projecting the
    /// group here would create it tenant-wide and unscoped, which grants
    /// across the entire portfolio rather than one site.
    #[must_use]
    pub fn to_manifest(&self) -> ManagementManifest {
        let mut manifest = ManagementManifest::new();
        for p in &self.permissions {
            manifest = manifest.with_permission(PermissionSpec::new(
                p.action.clone(),
                p.action.clone(),
                p.description.clone(),
            ));
        }
        for r in &self.roles {
            let mut role = RoleSpec::new(r.name.clone(), r.name.clone(), r.description.clone());
            for action in &r.permissions {
                role = role.granting(GrantSpec::allow(action.clone()));
            }
            // Never `.global()`: these roles reach a subject through a group
            // scoped to one resource. A global role would reach the whole
            // tenant, which is the difference between "installer on this site"
            // and "installer everywhere".
            manifest = manifest.with_role(role);
        }
        manifest
    }

    /// The eager structural roles for `resource_type` (D-21).
    ///
    /// An unlisted type returns an empty set, which is the right answer for a
    /// device: its `granted-operator` group is created on first grant in
    /// Phase 2, not eagerly here.
    #[must_use]
    pub fn roles_for(&self, resource_type: &str) -> Vec<String> {
        self.group_templates
            .iter()
            .find(|t| t.resource_type == resource_type)
            .map(|t| t.roles.clone())
            .unwrap_or_default()
    }
}
