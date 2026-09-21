//! Removing every reserved-prefix fixture, and nothing else.
//!
//! This is the suite's clean-state mechanism, and the only one that exists at
//! its wave: `just demo-reset` is built in plan 01-07, which depends on this
//! plan, so nothing here may call it.
//!
//! # The order is forced, not stylistic
//!
//! Groups first: a group bound to a resource can outlive it and would then be
//! an orphan nothing looks for. Resources leaf-first, because a parent with
//! children cannot be deleted. The on-disk material last — a stale certificate
//! paired with a rebuilt account is an mTLS identity whose two halves disagree,
//! and that failure surfaces far from its cause.
//!
//! # The prefix decides everything this deletes
//!
//! [`is_smoke_name`] asks about the *slug*, after the type prefix —
//! `site:smoke-park`, not `smoke-site:park`. A Phase 2 seed name that happened
//! to carry the reserved prefix would be destroyed here without a word, which
//! is why D-18 reserves the prefix rather than merely suggesting it.

use anyhow::{Context, Result};
use axiam_sdk::management::models::Resource;
use axiam_sdk::management::page::PageRequest;

use super::{OTHER_TENANT, PREFIX, TENANT, certs};
use crate::stages::{Env, OrgClient, TenantClient, step};

/// Remove every prefixed fixture from both tenants, and nothing else.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    for slug in [TENANT, OTHER_TENANT] {
        let tenant_id = org.tenant_id(slug).await?;
        let client = TenantClient::login(&env, slug, tenant_id).await?;
        if slug == TENANT {
            // The forged-common-name leaf is unbound, so deleting the accounts
            // below would leave it live. Revoked explicitly.
            certs::revoke_recorded_forgery(&client).await?;
        }
        tenant(&client).await?;
    }

    let dir = domo_common::secrets::path("smoke")?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("removing {}", dir.display()))?;
        step("removed the on-disk smoke fixture material");
    }

    println!("✓ smoke-teardown");
    Ok(())
}

/// Empty one tenant of its prefixed fixtures.
async fn tenant(client: &TenantClient) -> Result<()> {
    let groups = client
        .groups()
        .list_all(PageRequest::first(500))
        .await
        .context("listing groups")?;
    for group in groups.iter().filter(|g| is_smoke_group(&g.name)) {
        step(&format!("removing group '{}'", group.name));
        client
            .groups()
            .delete(group.id)
            .await
            .with_context(|| format!("deleting group '{}'", group.name))?;
    }

    let resources = client
        .resources()
        .list_all(PageRequest::first(500))
        .await
        .context("listing resources")?;
    let mut smoke: Vec<&Resource> = resources.iter().filter(|r| is_smoke_name(&r.name)).collect();
    smoke.sort_by_key(|r| std::cmp::Reverse(depth_of(r, &resources)));
    for resource in smoke {
        step(&format!("removing resource '{}'", resource.name));
        client
            .resources()
            .delete(resource.id)
            .await
            .with_context(|| format!("deleting resource '{}'", resource.name))?;
    }

    let accounts = client
        .service_accounts()
        .list_all(PageRequest::first(500))
        .await
        .context("listing service accounts")?;
    for account in accounts.iter().filter(|a| a.name.starts_with(PREFIX)) {
        step(&format!("removing service account '{}'", account.name));
        client
            .service_accounts()
            .delete(account.id)
            .await
            .with_context(|| format!("deleting service account '{}'", account.name))?;
    }

    let users = client
        .users()
        .list_all(PageRequest::first(500))
        .await
        .context("listing users")?;
    for user in users.iter().filter(|u| u.username.starts_with(PREFIX)) {
        step(&format!("removing user '{}'", user.username));
        client
            .users()
            .delete(user.id)
            .await
            .with_context(|| format!("deleting user '{}'", user.username))?;
    }

    Ok(())
}

/// True when a resource name's slug carries the reserved prefix.
#[must_use]
pub fn is_smoke_name(name: &str) -> bool {
    name.rsplit(':').next().is_some_and(|s| s.starts_with(PREFIX))
}

/// True for `{role}@{resource name}` where the resource name is a fixture's.
fn is_smoke_group(name: &str) -> bool {
    name.split_once('@').is_some_and(|(_, r)| is_smoke_name(r))
}

/// How many parents a resource has, walked over the tenant's full listing.
fn depth_of(resource: &Resource, all: &[Resource]) -> usize {
    let mut depth = 0;
    let mut cursor = resource.parent_id;
    // Bounded by the listing's length: a cycle would otherwise spin forever,
    // and a tree deep enough to matter does not exist in this model.
    while let Some(id) = cursor {
        depth += 1;
        if depth > all.len() {
            break;
        }
        cursor = all.iter().find(|r| r.id == id).and_then(|r| r.parent_id);
    }
    depth
}
