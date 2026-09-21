//! Per-tenant admin principals — the stage that makes tenant-scoped work
//! possible at all (D-37).
//!
//! # Why this exists
//!
//! The Rust SDK never sends AXIAM's acting-tenant header, so an
//! organization-level principal cannot address a tenant through it. There is
//! no flag to set and no scope to switch: which tenant a management call lands
//! in is decided by the identity that logged in. So each tenant needs its own
//! administrator, and creating one is the one piece of bootstrap that cannot
//! go through the SDK.
//!
//! # DF-008 — the four hand-rolled calls
//!
//! [`domo_common::hand_rolled::HandRolled::provision_tenant_admin`] makes
//! them, each carrying `X-Axiam-Tenant`:
//!
//! 1. create the user (409 is success: recover the id by username)
//! 2. activate it — AXIAM creates users `PendingVerification` and there is no
//!    mailbox here to click a link in
//! 3. find the tenant's seeded `super-admin` role
//! 4. assign it with no `resource_id`, so the grant is global *within this
//!    tenant* and reaches nothing outside it
//!
//! That session is a cookie jar with a CSRF token, not a bearer token.
//! Forgetting to echo the token surfaces as a bare 403, which reads like an
//! authorization bug and sends you looking in the policy model.
//!
//! # C-5 — and what this principal is for afterwards
//!
//! Service-account tokens carry `aud: axiam:m2m`, which every REST management
//! route rejects. So the `mgmt@` and `twin@` accounts of
//! [`super::service_certs`] are usable for authorization checks and
//! device-style authentication but cannot make a management call. This
//! tenant-admin user is the principal Phase 2's Management Platform will need
//! instead. Recorded as a finding; the decision is Phase 2's.

use anyhow::{Context, Result};

use domo_common::hand_rolled::HandRolled;

use super::{Env, OrgClient, ok, step, super_admin_credentials, tenant_admin_credentials};

/// Provision the admin user of every demo tenant.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;
    let tenants = org.demo_tenants().await?;

    let creds = super_admin_credentials(&env.org_slug)?;
    let mut session = HandRolled::new(&env.axiam_url, &env.root_pem)?;
    session
        .login(&env.org_slug, &creds.email, &creds.password)
        .await?;

    for tenant in &tenants {
        ensure(&session, &env, tenant.id, &tenant.slug).await?;
    }

    domo_common::secrets::mark_done("tenant-admin")?;
    Ok(())
}

/// Provision (or re-resolve) one tenant's admin, and persist its credentials.
///
/// DF-008. Idempotent by probe: the password is minted once and kept in
/// `.secrets/axiam/tenant-admin-<slug>.json` at mode 0600 (D-13), so a re-run
/// re-uses the same account rather than resetting a live one's password.
pub async fn ensure(
    session: &HandRolled,
    env: &Env,
    tenant_id: uuid::Uuid,
    slug: &str,
) -> Result<uuid::Uuid> {
    let admin = tenant_admin_credentials(&env.org_slug, slug)?;
    step(&format!("provisioning the tenant admin for '{slug}'"));
    let user_id = session
        .provision_tenant_admin(tenant_id, &admin.username, &admin.email, &admin.password)
        .await
        .with_context(|| format!("provisioning the '{slug}' tenant admin"))?;
    ok(&format!("tenant admin ready for '{slug}' ({user_id})"));
    Ok(user_id)
}
