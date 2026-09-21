//! The authorization model, asserted against a live AXIAM (AUTHZ-01, AUTHZ-02).
//!
//! # Why these are not the unit tests again
//!
//! Plan 01-04 proved the naming scheme and the resource helper in isolation,
//! where nothing can disagree with them. These assertions ask AXIAM itself: do
//! ancestors come back in the order every later tree walk assumes, does an
//! empty branch answer with an empty list rather than an error, and does an
//! access decision actually follow group membership? A model where membership
//! is decorative passes every happy-path check and fails the six-state
//! sequence below.
//!
//! # Every failure names itself
//!
//! Each case prints its own line and, on failure, both the expected and the
//! observed value. An assertion that can only report "failed" is not worth
//! writing here: the entire value of this stage is telling you *which* link in
//! a chain broke, in one run, without a stack trace.
//!
//! # Re-runnability is a property of each case, not of a cleanup at the end
//!
//! Every case that mutates state restores it before returning — memberships
//! are removed, lazily-created groups are deleted. A case that left the tenant
//! in its end state would pass once and then assert the wrong starting point
//! forever after.

use anyhow::{Context, Result};
use axiam_sdk::management::models::{AddMemberRequest, AssignRoleToGroupRequest, CreateGroupRequest, Resource};
use axiam_sdk::management::page::PageRequest;
use axiam_sdk::rest::authz::reason_code;
use uuid::Uuid;

use super::{Branch, OTHER_TENANT, PROBE_ACCOUNT, TENANT, TEST_USER, find_group, find_user};
use crate::naming::{self, Kind};
use crate::stages::tree::{DOMAIN_ID_KEY, ensure_resource};
use crate::stages::{Env, OrgClient, TenantClient, fail, ok, step};

/// Accumulates outcomes so one run reports every broken invariant rather than
/// only the first.
struct Report {
    passed: bool,
}

impl Report {
    fn new() -> Self {
        Self { passed: true }
    }

    /// Record one named case, printing both values when it fails.
    fn case(&mut self, name: &str, ok_: bool, expected: &str, observed: &str) {
        if ok_ {
            ok(&format!("{name}  {observed}"));
        } else {
            fail(&format!("{name}  expected {expected}, got {observed}"));
            self.passed = false;
        }
    }
}

/// Run every live authorization assertion over the branch the `smoke` stage
/// built.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;
    let tenant_id = org.tenant_id(TENANT).await?;
    let client = TenantClient::login(&env, TENANT, tenant_id).await?;
    // Resolve, never build: this recipe must be re-runnable in seconds against
    // the branch `smoke-tree` already made, so a failing assertion can be
    // re-checked without a rebuild.
    let branch = super::resolve(&client).await?;

    let mut r = Report::new();

    tree_shape(&client, &branch, &mut r).await?;
    empty_case(&env, &org, &mut r).await?;
    duplicate_case(&client, &branch, &mut r).await?;
    group_pattern(&client, &branch, &mut r).await?;
    membership(&client, &branch, &mut r).await?;
    membership_reverse_order(&client, &branch, &mut r).await?;
    service_account(&client, &branch, &mut r).await?;
    no_deny_rule(&client, &branch, &mut r).await?;

    anyhow::ensure!(r.passed, "smoke-authz found at least one broken invariant");
    println!("✓ smoke-authz");
    Ok(())
}

/// Ancestors in order, the two common areas through their parents, and a
/// stable identifier on every node.
async fn tree_shape(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let ancestors = client
        .resources()
        .list_ancestors(b.device)
        .await
        .context("listing the device's ancestors")?;
    let observed: Vec<&str> = ancestors.iter().map(|a| a.name.as_str()).collect();
    let expected = vec![
        naming::resource_name(Kind::Apartment, &b.apartment_slug)?,
        naming::resource_name(Kind::Building, "smoke-tower")?,
        naming::resource_name(Kind::Site, "smoke-park")?,
        "portfolio".to_owned(),
    ];
    // Compared as an ORDERED sequence, deliberately. An implementation that
    // returned the right resources in the wrong order would pass a set
    // comparison and mislead every later tree walk — which is exactly the
    // class of bug a live assertion is for.
    r.case(
        "ancestors",
        observed == expected,
        &format!("{expected:?} (leaf to root)"),
        &format!("{observed:?}"),
    );

    // The common areas are not on the device's ancestor path, so they are
    // checked through their parents' child lists instead.
    let under_site = child_named(client, b.site, &naming::resource_name(Kind::SiteCommon, "smoke-lobby")?).await?;
    let under_building =
        child_named(client, b.building, &naming::resource_name(Kind::BuildingCommon, "smoke-lobby")?).await?;
    let distinct = match (under_site, under_building) {
        (Some(a), Some(c)) => a != c,
        _ => false,
    };
    r.case(
        "common-areas",
        distinct,
        "two distinct resources sharing the common-area type prefix",
        &format!("site={under_site:?} building={under_building:?}"),
    );

    let nodes = [
        ("portfolio", b.portfolio),
        ("site", b.site),
        ("site-common", b.site_common),
        ("building", b.building),
        ("building-common", b.building_common),
        ("apartment", b.apartment),
        ("device", b.device),
    ];
    let mut missing = Vec::new();
    for (label, id) in nodes {
        let resource = client
            .resources()
            .get(id)
            .await
            .with_context(|| format!("reading the {label} resource"))?;
        let stable = resource
            .metadata
            .get(DOMAIN_ID_KEY)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty() && *s != resource.name);
        if stable.is_none() {
            missing.push(label);
        }
    }
    r.case(
        "stable-identifiers",
        missing.is_empty(),
        "every node carries a stable identifier distinct from its name",
        &if missing.is_empty() {
            format!("{} nodes stamped", nodes.len())
        } else {
            format!("missing on {missing:?}")
        },
    );
    Ok(())
}

/// The tenant with no branch: an empty child list, not an error, and exactly
/// one resource in the whole tenant.
async fn empty_case(env: &Env, org: &OrgClient, r: &mut Report) -> Result<()> {
    let tenant_id = org.tenant_id(OTHER_TENANT).await?;
    let client = TenantClient::login(env, OTHER_TENANT, tenant_id).await?;

    let all = client
        .resources()
        .list_all(PageRequest::first(200))
        .await
        .context("listing the other tenant's resources")?;
    let portfolio = all
        .iter()
        .find(|x| x.parent_id.is_none() && x.name == "portfolio")
        .context("the other tenant has no portfolio")?;

    // An error here rather than an empty list would break every tree walk that
    // runs before anything has been built — Phase 2's seed included.
    let children = client
        .resources()
        .list_children(portfolio.id)
        .await
        .context("listing the other tenant's portfolio children")?;
    r.case(
        "empty-portfolio",
        children.is_empty(),
        "an empty child list",
        &format!("{} children", children.len()),
    );
    r.case(
        "empty-tenant",
        all.len() == 1,
        "exactly 1 resource",
        &format!("{} resources", all.len()),
    );
    Ok(())
}

/// Creating what already exists returns the existing resource and leaves
/// exactly one child with that name (P-8).
async fn duplicate_case(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let again = ensure_resource(client, Kind::Apartment, &b.apartment_slug, Some(b.building)).await?;
    r.case(
        "duplicate-create",
        again.id == b.apartment,
        &format!("the existing id {}", b.apartment),
        &format!("{}", again.id),
    );

    let name = naming::resource_name(Kind::Apartment, &b.apartment_slug)?;
    let siblings = client
        .resources()
        .list_children(b.building)
        .await
        .context("listing the building's children")?;
    let count = siblings.iter().filter(|x| x.name == name).count();
    r.case(
        "duplicate-siblings",
        count == 1,
        &format!("exactly 1 child named '{name}'"),
        &format!("{count}"),
    );
    Ok(())
}

/// Every structural group exists by the scheme's exact name, holds exactly its
/// own role, and holds it scoped to its own resource.
async fn group_pattern(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let expected: Vec<(&str, Kind, &str, Uuid)> = vec![
        ("installer", Kind::Site, "smoke-park", b.site),
        ("concierge", Kind::Site, "smoke-park", b.site),
        ("common-operator", Kind::SiteCommon, "smoke-lobby", b.site_common),
        ("common-device-manager", Kind::SiteCommon, "smoke-lobby", b.site_common),
        ("common-operator", Kind::BuildingCommon, "smoke-lobby", b.building_common),
        ("common-device-manager", Kind::BuildingCommon, "smoke-lobby", b.building_common),
        ("resident", Kind::Apartment, &b.apartment_slug, b.apartment),
    ];

    let mut ids = Vec::new();
    for (role, kind, slug, resource) in &expected {
        let name = naming::group_name(role, *kind, slug)?;
        let Some(id) = find_group(client, &name).await? else {
            r.case("group-pattern", false, &format!("group '{name}'"), "absent");
            continue;
        };
        ids.push((name.clone(), id));

        let roles = client
            .groups()
            .list_roles(id)
            .await
            .with_context(|| format!("listing the roles of '{name}'"))?;
        // Exactly one, scoped to exactly this resource. "At least one" would
        // accept a group that also holds someone else's role, and an unscoped
        // binding would reach the whole portfolio — the difference between an
        // installer on one site and an installer everywhere.
        let correct = roles.len() == 1
            && roles[0].role.name == *role
            && roles[0].resource_id == Some(*resource);
        let observed: Vec<String> = roles
            .iter()
            .map(|a| format!("{}@{:?}", a.role.name, a.resource_id))
            .collect();
        r.case(
            "group-pattern",
            correct,
            &format!("'{name}' holding only '{role}' scoped to {resource}"),
            &format!("{observed:?}"),
        );
    }

    // The site's two groups are distinct objects: a single group answering to
    // both names would make "installer" and "concierge" the same thing.
    let installer = find_group(client, &naming::group_name("installer", Kind::Site, "smoke-park")?).await?;
    let concierge = find_group(client, &naming::group_name("concierge", Kind::Site, "smoke-park")?).await?;
    r.case(
        "group-isolation",
        matches!((installer, concierge), (Some(a), Some(c)) if a != c),
        "two distinct groups on the same site",
        &format!("installer={installer:?} concierge={concierge:?}"),
    );
    Ok(())
}

/// The six-state membership sequence, with the role binding already in place
/// before the first membership change.
///
/// Deny before membership, allow after adding, deny after removing, allow
/// after re-adding — each asserted, and the membership removed at the end so
/// the next run starts from the same state.
async fn membership(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let user = find_user(client, TEST_USER)
        .await?
        .context("the smoke test user does not exist — run `just smoke-tree`")?;
    let group_name = naming::group_name("resident", Kind::Apartment, &b.apartment_slug)?;
    let group = find_group(client, &group_name)
        .await?
        .with_context(|| format!("group '{group_name}' does not exist"))?;

    remove_member(client, group, user).await?;

    let mut states = Vec::new();
    states.push(("before-membership", false, decide(client, user, "device:operate", b.apartment).await?));
    add_member(client, group, user).await?;
    states.push(("after-adding", true, decide(client, user, "device:operate", b.apartment).await?));
    remove_member(client, group, user).await?;
    states.push(("after-removing", false, decide(client, user, "device:operate", b.apartment).await?));
    add_member(client, group, user).await?;
    states.push(("after-re-adding", true, decide(client, user, "device:operate", b.apartment).await?));

    // Restore, so the sequence asserts the same starting point next run.
    remove_member(client, group, user).await?;
    states.push(("after-final-removal", false, decide(client, user, "device:operate", b.apartment).await?));

    let wrong: Vec<String> = states
        .iter()
        .filter(|(_, want, got)| want != got)
        .map(|(label, want, got)| format!("{label}: wanted {want}, got {got}"))
        .collect();
    r.case(
        "membership",
        wrong.is_empty(),
        "deny → allow → deny → allow → deny across five membership states",
        &if wrong.is_empty() {
            format!("{} states, all as expected", states.len())
        } else {
            format!("{wrong:?}")
        },
    );
    Ok(())
}

/// The same convergence with the two writes in the opposite order: the member
/// is added to a group that holds no role yet, and the binding follows.
///
/// This is also the lazy grant path Phase 2 walks on every resident-to-
/// installer grant, and the empty-group case: a group with a binding but no
/// members grants nothing.
async fn membership_reverse_order(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let user = find_user(client, TEST_USER)
        .await?
        .context("the smoke test user does not exist")?;
    let name = naming::group_name("granted-operator", Kind::Device, "smoke-peer-light")?;

    // A group with no role bound to it at all.
    let group = match find_group(client, &name).await? {
        Some(id) => id,
        None => {
            step(&format!("creating group '{name}' (no binding yet)"));
            client
                .groups()
                .create(&CreateGroupRequest {
                    description: "smoke: membership-before-binding".to_owned(),
                    metadata: None,
                    name: name.clone(),
                })
                .await
                .with_context(|| format!("creating group '{name}'"))?
                .id
        }
    };

    let empty_group = decide(client, user, "device:operate", b.peer_device).await?;
    add_member(client, group, user).await?;
    let member_no_binding = decide(client, user, "device:operate", b.peer_device).await?;

    let role = role_id(client, "granted-operator").await?;
    client
        .roles()
        .assign_to_group(
            role,
            &AssignRoleToGroupRequest {
                group_id: group,
                resource_id: Some(b.peer_device),
                tenant_scope: None,
            },
        )
        .await
        .context("binding granted-operator to the smoke grant group")?;
    let member_and_binding = decide(client, user, "device:operate", b.peer_device).await?;

    // Delete the group outright: it is a lazy grant group, exactly as Phase 2
    // deletes one on revoke, and it restores the starting state for the next
    // run in a single call.
    client
        .groups()
        .delete(group)
        .await
        .with_context(|| format!("deleting group '{name}'"))?;
    let after_revoke = decide(client, user, "device:operate", b.peer_device).await?;

    r.case(
        "empty-group",
        !empty_group,
        "deny for a group with no members",
        &format!("{empty_group}"),
    );
    r.case(
        "membership-before-binding",
        !member_no_binding && member_and_binding && !after_revoke,
        "deny (member, no binding) → allow (binding added) → deny (group revoked)",
        &format!("{member_no_binding} → {member_and_binding} → {after_revoke}"),
    );
    Ok(())
}

/// The probe's account reaches its own device and nothing else.
///
/// The second half is what proves the group scope is doing work: an unscoped
/// binding would allow both.
async fn service_account(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let accounts = client
        .service_accounts()
        .list_all(PageRequest::first(200))
        .await
        .context("listing service accounts")?;
    let probe = accounts
        .iter()
        .find(|a| a.name == PROBE_ACCOUNT)
        .context("the probe's device account does not exist")?;

    let own = decide(client, probe.id, "twin:report", b.device).await?;
    let outside = decide(client, probe.id, "twin:report", b.peer_device).await?;
    r.case(
        "device-self",
        own && !outside,
        "allow on its own device, deny on the sibling device",
        &format!("own={own} sibling={outside}"),
    );
    Ok(())
}

/// No deny rule exists at or above any apartment or device node.
///
/// Asked of AXIAM rather than of our own code: a refusal must read `no_grant`
/// — nothing matched — and never `denied_by_rule`. A deny above an apartment
/// would beat the resident-to-installer grant the demo exists to show, and the
/// engine is deny-override at every depth, so this is the one shape the model
/// must never contain (T-04-05).
async fn no_deny_rule(client: &TenantClient, b: &Branch, r: &mut Report) -> Result<()> {
    let user = find_user(client, TEST_USER)
        .await?
        .context("the smoke test user does not exist")?;

    let mut offenders = Vec::new();
    for (label, id) in [
        ("portfolio", b.portfolio),
        ("site", b.site),
        ("site-common", b.site_common),
        ("building", b.building),
        ("building-common", b.building_common),
        ("apartment", b.apartment),
        ("device", b.device),
        ("peer-device", b.peer_device),
    ] {
        let decision = client
            .check_access_as(user, "device:operate", id, None)
            .await
            .with_context(|| format!("checking access at the {label} node"))?;
        if decision.reason_code.as_deref() == Some(reason_code::DENIED_BY_RULE) {
            offenders.push(label);
        }
    }
    r.case(
        "no-deny-rule",
        offenders.is_empty(),
        "every refusal reading 'no_grant', never 'denied_by_rule'",
        &if offenders.is_empty() {
            "8 nodes, no deny rule".to_owned()
        } else {
            format!("deny rule at {offenders:?}")
        },
    );
    Ok(())
}

// --- small helpers ----------------------------------------------------------

/// One access decision for `subject`, reduced to its outcome.
async fn decide(client: &TenantClient, subject: Uuid, action: &str, resource: Uuid) -> Result<bool> {
    let decision = client
        .check_access_as(subject, action, resource, None)
        .await
        .with_context(|| format!("checking '{action}' for {subject} on {resource}"))?;
    Ok(decision.allowed)
}

/// Add a member, treating "already a member" as success.
async fn add_member(client: &TenantClient, group: Uuid, user: Uuid) -> Result<()> {
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
async fn remove_member(client: &TenantClient, group: Uuid, user: Uuid) -> Result<()> {
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
async fn role_id(client: &TenantClient, role: &str) -> Result<Uuid> {
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
async fn child_named(client: &TenantClient, parent: Uuid, name: &str) -> Result<Option<Uuid>> {
    let children: Vec<Resource> = client
        .resources()
        .list_children(parent)
        .await
        .context("listing children")?;
    Ok(children.iter().find(|c| c.name == name).map(|c| c.id))
}
