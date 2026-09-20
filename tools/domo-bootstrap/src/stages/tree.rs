//! Each tenant's `portfolio` root, and the helper that creates any resource
//! together with the structural groups its type calls for (D-18, D-21).
//!
//! # Why the tree is imperative when the catalog is declarative
//!
//! The SDK's management manifest cannot express any of the three things a node
//! of this tree needs (DF-011): resource metadata, which is where the stable
//! domain identifier lives (D-17); a resource-scoped group-to-role binding,
//! which is the entire group-indirection model; and service accounts. A
//! manifest-created group would hold its role tenant-wide — the difference
//! between "installer on this site" and "installer everywhere". So the tree is
//! built by explicit calls, and the catalog stays declarative because nothing
//! about a role or a permission is scoped.
//!
//! # Resolve before create, always (P-8)
//!
//! No uniqueness index on resource names was found in AXIAM, so two siblings
//! with the same name can both exist. That is not a cosmetic problem: an
//! authorization decision about "the apartment called A1" becomes ambiguous,
//! and which one wins depends on list order. Every create below therefore
//! resolves under the parent first, using the parent's own child list rather
//! than a client-side filter of one page of a tenant-wide listing.
//!
//! # No deny rule, anywhere
//!
//! The model is permit-by-group-membership over the hierarchy. AXIAM's RBAC
//! engine is deny-override at every depth and at equal specificity, so a deny
//! at or above an apartment would beat the resident-to-installer grant the
//! demo exists to show. Nothing here creates one (T-04-05).
//!
//! # What this stage does NOT build
//!
//! Only the roots. The smoke branch is plan 01-06's, prefixed `smoke-` so it
//! cannot collide with Phase 2's seed, and the full two-tenant seed is the
//! Management Platform's — so no data ever has two writers (D-18).

use anyhow::{Context, Result};
use axiam_sdk::management::models::{
    AssignRoleToGroupRequest, CreateGroupRequest, CreateResourceRequest, Resource,
};
use axiam_sdk::management::page::PageRequest;
use uuid::Uuid;

use super::{Env, OrgClient, TenantClient, fail, ok, step};
use crate::naming::{self, Kind};

/// The metadata key holding a resource's stable domain identifier.
///
/// The readable name is for humans and the console; this is what later phases
/// join on, so a site can be renamed without breaking anything pointing at it.
/// AXIAM's `CreateResourceRequest` carries free-form `metadata`, which is the
/// field D-17 was waiting on research to name.
pub const DOMAIN_ID_KEY: &str = "domo_id";

/// Create every tenant's portfolio root and its `property-manager` group.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    for tenant in org.demo_tenants().await? {
        let client = TenantClient::login(&env, &tenant.slug, tenant.id).await?;
        let portfolio = ensure_resource(&client, Kind::Portfolio, "", None).await?;
        ok(&format!(
            "portfolio ready for '{}' ({})",
            tenant.slug, portfolio.id
        ));
    }

    domo_common::secrets::mark_done("tree")?;
    Ok(())
}

/// Create a resource under `parent` together with its structural groups, or
/// resolve what is already there.
///
/// This is the helper D-18 asks Phase 1 to establish: plan 01-06 builds the
/// smoke branch with it, and Phase 2's Management Platform follows the same
/// shape. It does three things, in order, each idempotent:
///
/// 1. resolve the resource by name under `parent`, creating it only if absent
///    and stamping it with its stable domain identifier;
/// 2. for each role [`naming::structural_groups_for`] returns, resolve or
///    create the group named `{role}@{type}:{slug}`;
/// 3. bind that role to that group, scoped to this resource.
///
/// A device gets no group here at all: its `granted-operator` group is created
/// on the first grant, in Phase 2.
pub async fn ensure_resource(
    client: &TenantClient,
    kind: Kind,
    slug: &str,
    parent: Option<Uuid>,
) -> Result<Resource> {
    let name = naming::resource_name(kind, slug)?;

    let existing = children_of(client, parent).await?;
    let matches: Vec<&Resource> = existing.iter().filter(|r| r.name == name).collect();
    if matches.len() > 1 {
        // Refuse rather than pick one: which duplicate an authorization
        // decision lands on would otherwise depend on list order.
        anyhow::bail!(
            "'{name}' exists {} times under the same parent in '{}'; \
             an authorization decision about it would be ambiguous (P-8)",
            matches.len(),
            client.slug
        );
    }

    let resource = if let Some(found) = matches.first() {
        step(&format!("resource '{name}' already exists — resolving"));
        (*found).clone()
    } else {
        step(&format!("creating resource '{name}'"));
        let metadata = serde_json::json!({
            DOMAIN_ID_KEY: Uuid::new_v4().to_string(),
            "kind": naming::resource_type(kind),
        });
        client
            .resources()
            .create(&CreateResourceRequest {
                metadata: Some(metadata),
                name: name.clone(),
                parent_id: parent,
                resource_type: naming::resource_type(kind).to_owned(),
            })
            .await
            .with_context(|| format!("creating resource '{name}' in '{}'", client.slug))?
    };

    for role in naming::structural_groups_for(kind) {
        ensure_group_binding(client, role, kind, slug, resource.id).await?;
    }

    Ok(resource)
}

/// The children of `parent`, or the tenant's roots when `parent` is `None`.
async fn children_of(client: &TenantClient, parent: Option<Uuid>) -> Result<Vec<Resource>> {
    match parent {
        // Server-side and exact: the parent's own child list, not a
        // client-side filter of one page of everything.
        Some(id) => client
            .resources()
            .list_children(id)
            .await
            .context("listing the parent's children"),
        // A root has no parent to ask, so the roots are the tenant-wide
        // listing filtered to those without one.
        None => {
            let all = client
                .resources()
                .list_all(PageRequest::first(200))
                .await
                .context("listing resources")?;
            Ok(all.into_iter().filter(|r| r.parent_id.is_none()).collect())
        }
    }
}

/// Resolve or create `{role}@{type}:{slug}` and bind `role` to it, scoped to
/// `resource_id`.
async fn ensure_group_binding(
    client: &TenantClient,
    role: &str,
    kind: Kind,
    slug: &str,
    resource_id: Uuid,
) -> Result<()> {
    let group_name = naming::group_name(role, kind, slug)?;

    // Server-side search on the natural key, then an exact match: `search` is
    // a free-text filter, so its result still has to be checked by name.
    let found = client
        .groups()
        .list_all(PageRequest {
            offset: 0,
            limit: Some(100),
            search: Some(group_name.clone()),
        })
        .await
        .with_context(|| format!("searching for group '{group_name}'"))?;

    let group_id = if let Some(g) = found.iter().find(|g| g.name == group_name) {
        g.id
    } else {
        step(&format!("creating group '{group_name}'"));
        match client
            .groups()
            .create(&CreateGroupRequest {
                description: format!("{role} scoped to {}", naming::resource_name(kind, slug)?),
                metadata: None,
                name: group_name.clone(),
            })
            .await
        {
            Ok(g) => g.id,
            Err(e) => {
                // AXIAM holds a unique index on (tenant, group name), so a
                // concurrent or repeated run loses the race rather than
                // duplicating. Resolving is the correct outcome, not a retry.
                let all = client
                    .groups()
                    .list_all(PageRequest {
                        offset: 0,
                        limit: Some(100),
                        search: Some(group_name.clone()),
                    })
                    .await
                    .with_context(|| format!("re-resolving group '{group_name}' after {e}"))?;
                all.iter()
                    .find(|g| g.name == group_name)
                    .map(|g| g.id)
                    .with_context(|| format!("creating group '{group_name}' failed: {e}"))?
            }
        }
    };

    let role_id = role_id_by_name(client, role).await?;

    // Scoped to THIS resource. Unscoped would grant across the whole
    // portfolio, which is the difference between an installer on one site and
    // an installer everywhere. An allow, never a deny (T-04-05).
    let assigned = client
        .roles()
        .assign_to_group(
            role_id,
            &AssignRoleToGroupRequest {
                group_id,
                resource_id: Some(resource_id),
                // Omitted: `tenant_scope` narrows an assignment made in an
                // ORGANIZATION's scope. This one is made by the tenant's own
                // admin and already reaches nowhere else.
                tenant_scope: None,
            },
        )
        .await;
    if let Err(e) = assigned {
        // The binding already existing is the normal state of a re-run. Prove
        // it is there — and that it is scoped to THIS resource — rather than
        // assuming the error said so. An existing binding at the wrong scope
        // would be a far worse outcome than a failed call.
        let bound = client
            .roles()
            .list_groups(role_id)
            .await
            .with_context(|| format!("checking the binding of '{role}' after {e}"))?;
        anyhow::ensure!(
            bound
                .iter()
                .any(|a| a.group.id == group_id && a.resource_id == Some(resource_id)),
            "binding role '{role}' to group '{group_name}' scoped to {resource_id} failed: {e}"
        );
    }

    Ok(())
}

/// Resolve a role by its name — the natural key `authz/catalog.toml` uses.
async fn role_id_by_name(client: &TenantClient, role: &str) -> Result<Uuid> {
    let roles = client
        .roles()
        .list_all(PageRequest {
            offset: 0,
            limit: Some(200),
            search: Some(role.to_owned()),
        })
        .await
        .with_context(|| format!("searching for role '{role}'"))?;
    roles
        .iter()
        .find(|r| r.name == role)
        .map(|r| r.id)
        .with_context(|| {
            format!(
                "tenant '{}' has no role '{role}' — apply authz/catalog.toml first",
                client.slug
            )
        })
}

/// Assert each tenant has exactly one portfolio, carrying a stable identifier
/// distinct from its readable name, and that a tenant with no branch yet
/// reports an empty child list rather than an error (AUTHZ-01).
pub async fn verify(org: &OrgClient, env: &Env) -> Result<bool> {
    let mut passed = true;

    for tenant in org.demo_tenants().await? {
        let client = TenantClient::login(env, &tenant.slug, tenant.id).await?;
        let roots = children_of(&client, None).await?;
        let portfolios: Vec<&Resource> = roots.iter().filter(|r| r.name == "portfolio").collect();

        if portfolios.len() != 1 {
            fail(&format!(
                "tree  {} has {} resources named portfolio, expected 1",
                tenant.slug,
                portfolios.len()
            ));
            passed = false;
            continue;
        }
        let portfolio = portfolios[0];

        let domain_id = portfolio
            .metadata
            .get(DOMAIN_ID_KEY)
            .and_then(serde_json::Value::as_str);
        match domain_id {
            Some(id) if !id.is_empty() && id != portfolio.name => {
                ok(&format!("tree  {}  portfolio id {id}", tenant.slug));
            }
            Some(_) => {
                fail(&format!(
                    "tree  {} portfolio's stable identifier is not distinct from its name",
                    tenant.slug
                ));
                passed = false;
            }
            None => {
                fail(&format!(
                    "tree  {} portfolio has no stable identifier in metadata",
                    tenant.slug
                ));
                passed = false;
            }
        }

        // An empty branch must be an empty list, not an error: plan 01-06 and
        // Phase 2 both walk this before anything has been built under it.
        let children = client
            .resources()
            .list_children(portfolio.id)
            .await
            .with_context(|| format!("listing portfolio children in '{}'", tenant.slug))?;
        ok(&format!("tree  portfolio children: {}", children.len()));

        // The portfolio's own structural group, bound and scoped.
        let group_name = naming::group_name("property-manager", Kind::Portfolio, "")?;
        let groups = client
            .groups()
            .list_all(PageRequest {
                offset: 0,
                limit: Some(100),
                search: Some(group_name.clone()),
            })
            .await
            .context("searching for the portfolio group")?;
        if groups.iter().any(|g| g.name == group_name) {
            ok(&format!("tree  {}  {group_name}", tenant.slug));
        } else {
            fail(&format!(
                "tree  {} has no '{group_name}' group",
                tenant.slug
            ));
            passed = false;
        }
    }

    Ok(passed)
}
