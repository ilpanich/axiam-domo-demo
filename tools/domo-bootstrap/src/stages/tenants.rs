//! Stage 2 — create the demo tenants.
//!
//! The tracer needs exactly one tenant. Plans 01-02 onward add the second and
//! the rest of the resource tree; the shape here is what they extend.
//!
//! Tenant creation itself goes through the SDK, as the project constraint
//! requires. Only the tenant-admin provisioning is hand-rolled, and only
//! because DF-008 leaves it uncovered: the SDK sends no acting-tenant header,
//! so an organization-level principal cannot address a tenant through it.

use std::collections::HashMap;

use anyhow::{Context, Result};
use axiam_sdk::management::models::CreateTenantRequest;
use axiam_sdk::management::page::PageRequest;

use domo_common::hand_rolled::HandRolled;

use super::{Env, ok, step, super_admin_credentials, tenant_admin_credentials};

/// The tracer's single tenant.
const TENANTS: &[(&str, &str)] = &[("Lakeside Residences", "lakeside")];

pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let creds = super_admin_credentials(&env.org_slug)?;

    let sdk = domo_common::axiam::org_client(&env.axiam_url, &env.org_slug, &env.root_pem)?;
    let login = sdk
        .login(&creds.email, &creds.password)
        .await
        .context("super-admin login failed — run the org-bootstrap stage first")?;
    let org_id = login
        .org_id
        .context("organization-level login returned no org_id")?;

    let mut session = HandRolled::new(&env.axiam_url, &env.root_pem)?;
    session
        .login(&env.org_slug, &creds.email, &creds.password)
        .await?;

    let mut map: HashMap<String, String> = HashMap::new();

    for (name, slug) in TENANTS {
        // Resolve by natural key (slug) before creating anything.
        let existing = sdk
            .tenants()
            .in_org(org_id)
            .list_all(PageRequest::first(100))
            .await
            .context("listing tenants")?;

        let tenant_id = if let Some(t) = existing.iter().find(|t| t.slug == *slug) {
            step(&format!("tenant '{slug}' already exists — reusing"));
            t.id
        } else {
            step(&format!("creating tenant '{slug}'"));
            let created = sdk
                .tenants()
                .in_org(org_id)
                .create(&CreateTenantRequest {
                    name: (*name).to_owned(),
                    slug: (*slug).to_owned(),
                    metadata: None,
                })
                .await
                .with_context(|| format!("creating tenant '{slug}'"))?;
            created.id
        };

        map.insert(tenant_id.to_string(), (*slug).to_owned());

        // The tenant admin is what leaf issuance logs in as, so that the
        // certificate is signed by THIS tenant's CA and not another's (P-11).
        let admin = tenant_admin_credentials(&env.org_slug, slug)?;
        step(&format!("provisioning tenant admin for '{slug}'"));
        session
            .provision_tenant_admin(tenant_id, &admin.username, &admin.email, &admin.password)
            .await
            .with_context(|| format!("provisioning the '{slug}' tenant admin"))?;
        ok(&format!("tenant '{slug}' ready ({tenant_id})"));
    }

    // The Device Twin reads this to turn a token's tenant_id claim into the
    // slug its topic scheme is keyed by. It re-reads on a miss, so writing it
    // here is enough — no Twin restart required.
    let json = serde_json::to_vec_pretty(&map).context("serializing the tenant map")?;
    // Written into the volume shared with the Twin, NOT into `.secrets/`: that
    // tree is 0700 because it holds the organization root key, and the Twin
    // runs as a different uid. A tenant slug is public in every topic name, so
    // there is nothing here that wants secret handling — only a reader under
    // another uid.
    let state_dir = std::env::var("DOMO_TWIN_STATE_DIR")
        .unwrap_or_else(|_| "/domo/state".to_owned());
    std::fs::create_dir_all(&state_dir)
        .with_context(|| format!("creating {state_dir}"))?;
    let path = std::path::Path::new(&state_dir).join("tenants.json");
    std::fs::write(&path, &json).with_context(|| format!("writing {}", path.display()))?;
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644))
        .with_context(|| format!("chmod 644 {}", path.display()))?;
    ok("tenant map written for the Device Twin");

    domo_common::secrets::mark_done("tenants")?;
    Ok(())
}
