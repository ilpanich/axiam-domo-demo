//! The small, shared operations the live assertions are written in terms of.
//!
//! Split out so `assertions.rs` reads as a list of named cases rather than as
//! cases interleaved with the plumbing each one needs.
//!
//! # Two of these swallow an error on purpose
//!
//! [`add_member`] and [`remove_member`] treat "already in that state" as
//! success — established by re-reading the membership, not by trusting an
//! error's text. The six-state sequence has to be able to start from either
//! state, because it runs against a tenant a previous run may have left
//! part-way through, and an assertion that failed because its *setup* failed
//! would name the wrong cause.

use anyhow::{Context, Result};
use axiam_sdk::management::models::{AddMemberRequest, Resource};
use axiam_sdk::management::page::PageRequest;
use uuid::Uuid;

use crate::stages::TenantClient;

/// One access decision for `subject`, reduced to its outcome.
pub async fn decide(
    client: &TenantClient,
    subject: Uuid,
    action: &str,
    resource: Uuid,
) -> Result<bool> {
    let decision = client
        .check_access_as(subject, action, resource, None)
        .await
        .with_context(|| format!("checking '{action}' for {subject} on {resource}"))?;
    Ok(decision.allowed)
}

/// Add a member, treating "already a member" as success.
pub async fn add_member(client: &TenantClient, group: Uuid, user: Uuid) -> Result<()> {
    if let Err(e) = client
        .groups()
        .add_member(group, &AddMemberRequest { user_id: user })
        .await
    {
        let members = client
            .groups()
            .list_members_all(group, PageRequest::first(200))
            .await
            .with_context(|| format!("re-checking membership after {e}"))?;
        anyhow::ensure!(
            members.iter().any(|m| m.id == user),
            "adding the member failed: {e}"
        );
    }
    Ok(())
}

/// Remove a member, treating "not a member" as success.
pub async fn remove_member(client: &TenantClient, group: Uuid, user: Uuid) -> Result<()> {
    if client.groups().remove_member(group, user).await.is_err() {
        let members = client
            .groups()
            .list_members_all(group, PageRequest::first(200))
            .await
            .context("re-checking membership after a failed removal")?;
        anyhow::ensure!(
            !members.iter().any(|m| m.id == user),
            "the member is still in the group after removal"
        );
    }
    Ok(())
}

/// Resolve a role by the natural key `authz/catalog.toml` uses.
pub async fn role_id(client: &TenantClient, role: &str) -> Result<Uuid> {
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
        .find(|x| x.name == role)
        .map(|x| x.id)
        .with_context(|| format!("tenant '{}' has no role '{role}'", client.slug))
}

/// The id of the child of `parent` with this exact name, if there is one.
pub async fn child_named(
    client: &TenantClient,
    parent: Uuid,
    name: &str,
) -> Result<Option<Uuid>> {
    let children: Vec<Resource> = client
        .resources()
        .list_children(parent)
        .await
        .context("listing children")?;
    Ok(children.iter().find(|c| c.name == name).map(|c| c.id))
}
