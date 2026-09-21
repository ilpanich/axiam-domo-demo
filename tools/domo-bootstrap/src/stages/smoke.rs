//! One real branch of the resource tree, and the live proof that the
//! authorization model behaves — positively and negatively (D-18, D-21).
//!
//! # Why this exists as a stage rather than a test
//!
//! Plan 01-04 proved the naming scheme and the resource-with-its-groups helper
//! at unit level, where nothing can disagree with them. This is where they meet
//! a real AXIAM: a bug in `structural_groups_for` or in the resolve-before-
//! create path is invisible to a unit test and shows up here as a duplicate
//! sibling or a missing group. Phase 2's Management Platform builds its seed
//! with the *same* helper, so every defect this finds is a defect it would have
//! inherited.
//!
//! # The reserved prefix is a cross-phase commitment
//!
//! Every fixture name carries [`PREFIX`]. Phase 2's seed writes into this same
//! tenant, so a colliding name would surface as a Management Platform bug
//! rather than as a smoke-fixture bug — a diagnosis that costs an afternoon.
//! [`ensure_prefixed`] refuses a fixture without it rather than trusting the
//! constants below to stay correct.
//!
//! # Clean state at this wave
//!
//! [`teardown`] removes only prefixed fixtures. It is this plan's clean-state
//! mechanism and it has to stand alone: `just demo-reset` is built in plan
//! 01-07, which depends on this plan, so nothing here may call it.

use anyhow::{Context, Result, bail};
use axiam_sdk::management::models::{
    AddServiceAccountMemberRequest, CreateServiceAccountRequest, CreateUserRequest, Resource,
};
use axiam_sdk::management::page::PageRequest;
use axiam_sdk::Sensitive;
use uuid::Uuid;

use super::tree::ensure_resource;
use super::{Env, OrgClient, TenantClient, ok, step};
use crate::naming::{self, Kind};

/// The reserved fixture prefix (D-18).
///
/// Costly to change: Phase 2's seed must keep avoiding it, and `teardown`
/// decides what to delete by it. A seed name that happened to start with this
/// would be destroyed by a smoke teardown without a word.
pub const PREFIX: &str = "smoke-";

/// The tenant the branch is built in. One tenant only, by D-18.
pub const TENANT: &str = "lakeside";

/// The other demo tenant — never given a branch, so it stays the live
/// empty-portfolio case, and the source of the cross-tenant negatives.
pub const OTHER_TENANT: &str = "summit";

const SITE: &str = "smoke-park";
/// Deliberately the SAME slug for both common areas: they then share the
/// `common:` type prefix and differ only by parent, which is the duplicate-
/// sibling question asked for real rather than assumed away.
const COMMON: &str = "smoke-lobby";
const BUILDING: &str = "smoke-tower";
const UNIT: &str = "a1";
const DEVICE: &str = "smoke-probe-light";
const PEER_DEVICE: &str = "smoke-peer-light";

/// The probe's own device service account.
pub const PROBE_ACCOUNT: &str = "smoke-probe-device";
/// A second device in the same apartment — the mismatched-certificate and
/// namespace-escape target.
pub const PEER_ACCOUNT: &str = "smoke-peer-device";
/// An account that is deliberately never given a bound certificate: "the
/// certificate authenticates as nobody" made concrete.
pub const UNBOUND_ACCOUNT: &str = "smoke-unbound-device";
/// A device account in the OTHER tenant. A service account, not a branch —
/// D-18's "one tenant" is about the resource tree.
pub const OTHER_ACCOUNT: &str = "smoke-summit-device";
/// The user the membership sequence moves in and out of groups.
pub const TEST_USER: &str = "smoke-resident";

/// A device's own role, held through a group scoped to that device (D-21).
///
/// Created lazily, here, rather than by the resource helper: eager creation
/// would leave an empty group on every device, which reads as a grant that is
/// not there.
const DEVICE_SELF_ROLE: &str = "device-self";

/// Where a fixture publishes its service-account id for the probe to read.
pub fn sa_id_path(account: &str) -> String {
    format!("smoke/{account}.sa-id")
}

/// Refuse any fixture name that does not carry the reserved prefix.
///
/// Checked rather than assumed: the constants above are correct today, and a
/// later edit that drops the prefix would otherwise create a fixture that
/// teardown cannot see and Phase 2's seed could collide with.
fn ensure_prefixed(name: &str) -> Result<()> {
    if !name.starts_with(PREFIX) {
        bail!("fixture '{name}' does not carry the reserved '{PREFIX}' prefix (D-18)");
    }
    Ok(())
}

/// The branch, by resource id. Held so the assertions can walk it without
/// resolving every name a second time.
#[derive(Debug, Clone)]
pub struct Branch {
    pub portfolio: Uuid,
    pub site: Uuid,
    pub site_common: Uuid,
    pub building: Uuid,
    pub building_common: Uuid,
    pub apartment: Uuid,
    pub device: Uuid,
    pub peer_device: Uuid,
    /// `smoke-tower-a1` — composed, so unit `a1` of two towers cannot collide.
    pub apartment_slug: String,
}

/// Build the branch, its structural groups and the probe's device accounts.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;
    let tenant_id = org.tenant_id(TENANT).await?;
    let client = TenantClient::login(&env, TENANT, tenant_id).await?;

    let branch = build(&client).await?;
    accounts(&client, &branch).await?;
    other_tenant_account(&env, &org).await?;

    println!("✓ smoke-tree");
    let _ = branch;
    Ok(())
}

/// Resolve or create every node of the branch, in parent-before-child order.
///
/// Every create goes through [`ensure_resource`], which resolves by name under
/// the parent first — so a second run reports "already exists — resolving" and
/// creates nothing (P-8).
pub async fn build(client: &TenantClient) -> Result<Branch> {
    let apartment_slug = naming::apartment_slug(BUILDING, UNIT)?;
    for slug in [SITE, COMMON, BUILDING, DEVICE, PEER_DEVICE, apartment_slug.as_str()] {
        ensure_prefixed(slug)?;
    }

    let portfolio = resolve_portfolio(client).await?;

    step("building the smoke branch");
    let site = node(client, Kind::Site, SITE, portfolio).await?;
    // Both common areas, same slug, different parents. Two distinct resources
    // sharing a type prefix — asserted, not assumed (AUTHZ-01).
    let site_common = node(client, Kind::SiteCommon, COMMON, site).await?;
    let building = node(client, Kind::Building, BUILDING, site).await?;
    let building_common = node(client, Kind::BuildingCommon, COMMON, building).await?;
    let apartment = node(client, Kind::Apartment, &apartment_slug, building).await?;
    let device = node(client, Kind::Device, DEVICE, apartment).await?;
    let peer_device = node(client, Kind::Device, PEER_DEVICE, apartment).await?;

    ok("smoke branch ready: site → common → building → common → apartment → device");
    Ok(Branch {
        portfolio,
        site,
        site_common,
        building,
        building_common,
        apartment,
        device,
        peer_device,
        apartment_slug,
    })
}

/// One node, with the structural groups its type calls for, asserted to belong
/// to the acting tenant.
async fn node(client: &TenantClient, kind: Kind, slug: &str, parent: Uuid) -> Result<Uuid> {
    let resource = ensure_resource(client, kind, slug, Some(parent)).await?;
    // The organization tenant is reserved plumbing; a resource landing there
    // is the silent failure mode of an org-scoped client on a tenant-scoped
    // route (P-1), and it returns 2xx.
    if resource.tenant_id != client.tenant_id {
        bail!(
            "'{}' was created in tenant {} but this client acts in {}",
            resource.name,
            resource.tenant_id,
            client.tenant_id
        );
    }
    Ok(resource.id)
}

/// The tenant's `portfolio` root, which the `tree` stage created.
async fn resolve_portfolio(client: &TenantClient) -> Result<Uuid> {
    let all = client
        .resources()
        .list_all(PageRequest::first(200))
        .await
        .context("listing resources")?;
    let roots: Vec<&Resource> = all
        .iter()
        .filter(|r| r.parent_id.is_none() && r.name == "portfolio")
        .collect();
    match roots.as_slice() {
        [one] => Ok(one.id),
        [] => bail!("tenant '{}' has no portfolio — run `just tree` first", client.slug),
        many => bail!(
            "tenant '{}' has {} portfolios; an authorization decision about it \
             would be ambiguous (P-8)",
            client.slug,
            many.len()
        ),
    }
}

/// The device service accounts, and the probe's membership of its own device's
/// self-service group.
///
/// Certificates are deliberately NOT issued here: the probe generates its own
/// key and certificate signing request so the private key never leaves it
/// (D-23), and signing happens in the `smoke-certs` stage afterwards.
async fn accounts(client: &TenantClient, branch: &Branch) -> Result<()> {
    let probe = ensure_account(client, PROBE_ACCOUNT).await?;
    let peer = ensure_account(client, PEER_ACCOUNT).await?;
    let _unbound = ensure_account(client, UNBOUND_ACCOUNT).await?;

    // The lazy grant path, exercised once here so Phase 2 inherits a proven
    // shape: a group named for (role, resource), the role bound scoped to that
    // resource, and the subject added as a member.
    self_service_group(client, DEVICE, branch.device, probe).await?;
    self_service_group(client, PEER_DEVICE, branch.peer_device, peer).await?;

    ensure_user(client, TEST_USER).await?;
    Ok(())
}

/// A service account in the other tenant, for the cross-tenant negatives.
///
/// No resource branch is built there: D-18's "one tenant" governs the resource
/// tree, and a bare account is what the wrong-tenant token and other-tenant-CA
/// cases need.
async fn other_tenant_account(env: &Env, org: &OrgClient) -> Result<()> {
    let tenant_id = org.tenant_id(OTHER_TENANT).await?;
    let client = TenantClient::login(env, OTHER_TENANT, tenant_id).await?;
    ensure_account(&client, OTHER_ACCOUNT).await?;
    Ok(())
}

/// Resolve or create one service account by its natural key, and publish its
/// id where the probe can read it.
async fn ensure_account(client: &TenantClient, name: &str) -> Result<Uuid> {
    ensure_prefixed(name)?;
    let existing = client
        .service_accounts()
        .list_all(PageRequest::first(200))
        .await
        .context("listing service accounts")?;

    let id = if let Some(found) = existing.iter().find(|a| a.name == name) {
        step(&format!("service account '{name}' already exists — reusing"));
        found.id
    } else {
        step(&format!("creating service account '{name}'"));
        client
            .service_accounts()
            .create(&CreateServiceAccountRequest {
                name: name.to_owned(),
                description: Some(format!("smoke fixture in '{}' (D-18)", client.slug)),
            })
            .await
            .with_context(|| format!("creating service account '{name}'"))?
            .id
    };

    domo_common::secrets::write_string(sa_id_path(name), &id.to_string())?;
    Ok(id)
}

/// `device-self@device:<slug>` — created, bound to the device resource, and
/// given the device's own account as its only member.
async fn self_service_group(
    client: &TenantClient,
    slug: &str,
    resource_id: Uuid,
    account: Uuid,
) -> Result<()> {
    let group_name =
        super::tree::ensure_group_binding(client, DEVICE_SELF_ROLE, Kind::Device, slug, resource_id)
            .await?;

    let group = find_group(client, &group_name)
        .await?
        .with_context(|| format!("group '{group_name}' vanished after being created"))?;

    let members = client
        .groups()
        .list_service_accounts_all(group, PageRequest::first(100))
        .await
        .with_context(|| format!("listing members of '{group_name}'"))?;
    if members.iter().any(|m| m.id == account) {
        step(&format!("'{group_name}' already has its device — reusing"));
        return Ok(());
    }

    step(&format!("adding the device's account to '{group_name}'"));
    client
        .groups()
        .add_service_account(
            group,
            &AddServiceAccountMemberRequest {
                service_account_id: account,
            },
        )
        .await
        .with_context(|| format!("adding the device account to '{group_name}'"))?;
    Ok(())
}

/// Resolve a group id by its exact name.
///
/// `search` is a free-text filter, so its result is still matched by name —
/// the same discipline `tree.rs` applies, for the same reason.
pub async fn find_group(client: &TenantClient, name: &str) -> Result<Option<Uuid>> {
    let found = client
        .groups()
        .list_all(PageRequest {
            offset: 0,
            limit: Some(200),
            search: Some(name.to_owned()),
        })
        .await
        .with_context(|| format!("searching for group '{name}'"))?;
    Ok(found.iter().find(|g| g.name == name).map(|g| g.id))
}

/// Resolve a user id by username.
pub async fn find_user(client: &TenantClient, username: &str) -> Result<Option<Uuid>> {
    let users = client
        .users()
        .list_all(PageRequest::first(200))
        .await
        .context("listing users")?;
    Ok(users.iter().find(|u| u.username == username).map(|u| u.id))
}

/// The membership sequence's subject. Left `PendingVerification`: it never logs
/// in, and an access check names it by id.
async fn ensure_user(client: &TenantClient, username: &str) -> Result<Uuid> {
    ensure_prefixed(username)?;
    if let Some(id) = find_user(client, username).await? {
        step(&format!("test user '{username}' already exists — reusing"));
        return Ok(id);
    }
    step(&format!("creating test user '{username}'"));
    let created = client
        .users()
        .create(&CreateUserRequest {
            email: format!("{username}@smoke.invalid"),
            metadata: None,
            opaque: None,
            password: Sensitive::new(format!("Ax1!{}", Uuid::new_v4())),
            username: username.to_owned(),
        })
        .await
        .with_context(|| format!("creating user '{username}'"))?;
    Ok(created.id)
}

/// Remove every prefixed fixture from both tenants, and nothing else.
///
/// This is the plan's clean-state mechanism, and the only one that exists at
/// this wave. Resources are deleted leaf-first because a parent with children
/// cannot go; groups and accounts follow, then the on-disk fixture material so
/// a rebuild cannot pair a fresh account with a stale certificate.
pub async fn teardown() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    for slug in [TENANT, OTHER_TENANT] {
        let tenant_id = org.tenant_id(slug).await?;
        let client = TenantClient::login(&env, slug, tenant_id).await?;
        teardown_tenant(&client).await?;
    }

    // Stale certificates and keys are worse than none: a rebuilt account has a
    // new id, and a leaf naming the old one is an mTLS identity whose halves
    // disagree — which surfaces far from here.
    let dir = domo_common::secrets::path("smoke")?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("removing {}", dir.display()))?;
        step("removed the on-disk smoke fixture material");
    }

    println!("✓ smoke-teardown");
    Ok(())
}

async fn teardown_tenant(client: &TenantClient) -> Result<()> {
    // Groups first: a group bound to a resource can outlive it and would then
    // be an orphan nothing looks for.
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
    // Leaf-first: depth descending, so no parent is deleted while it still has
    // children. Depth is walked over this same set plus its ancestors.
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
///
/// The prefix sits on the *slug*, after the type prefix — `site:smoke-park`,
/// not `smoke-site:park` — so this asks about the last colon-separated part.
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
