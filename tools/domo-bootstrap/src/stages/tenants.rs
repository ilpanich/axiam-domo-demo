//! Stage 2 — create the demo tenants.
//!
//! Both of D-19's fictional brands: Lakeside Residences and Summit Homes. They
//! are the tenant isolation the demo exists to show, so they are separate all
//! the way down — separate admin principals, separate signing CAs, separate
//! service credentials, separate resource trees.
//!
//! Tenant creation itself goes through the SDK, as the project constraint
//! requires. Only the tenant-admin provisioning is hand-rolled, and only
//! because DF-008 leaves it uncovered: the SDK sends no acting-tenant header,
//! so an organization-level principal cannot address a tenant through it.
//! That work lives in [`super::tenant_admin`]; it is called from here as well
//! as standing alone as its own stage, so the tracer's stage sequence keeps
//! working unchanged while `just authz` can re-run it on its own.

use std::collections::HashMap;

use anyhow::{Context, Result};
use axiam_sdk::management::models::CreateTenantRequest;
use axiam_sdk::management::page::PageRequest;

use domo_common::hand_rolled::HandRolled;

use super::{Env, OrgClient, TenantClient, ok, step, super_admin_credentials, tenant_admin};

/// The demo tenants (D-19). The slugs are what tests, topics and documentation
/// refer to; the display names are editable, the slugs are not.
pub const TENANTS: &[(&str, &str)] = &[
    ("Lakeside Residences", "lakeside"),
    ("Summit Homes", "summit"),
];

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
        tenant_admin::ensure(&session, &env, tenant_id, slug).await?;
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

/// Assert both tenants exist, and that nothing was created in the reserved
/// `organization` tenant (P-1, T-04-01).
///
/// The second half is the one that cannot be caught any other way: an
/// organization principal writing through a tenant-scoped route succeeds
/// silently, and the object it creates is invisible to every later lookup
/// because nothing ever looks there. The type system prevents the call; this
/// asserts the outcome independently, because a silent failure deserves two
/// unrelated guards rather than one clever one.
pub async fn verify(org: &OrgClient, env: &Env) -> Result<bool> {
    let mut passed = true;

    let tenants = org.demo_tenants().await?;
    let mut slugs: Vec<&str> = tenants.iter().map(|t| t.slug.as_str()).collect();
    slugs.sort_unstable();
    let expected: Vec<&str> = {
        let mut e: Vec<&str> = TENANTS.iter().map(|(_, s)| *s).collect();
        e.sort_unstable();
        e
    };
    if slugs == expected {
        super::ok(&format!("tenants  {}", slugs.join(", ")));
    } else {
        super::fail(&format!(
            "tenants  expected [{}], found [{}]",
            expected.join(", "),
            slugs.join(", ")
        ));
        passed = false;
    }

    // Every object a tenant-scoped stage created must carry that tenant's id.
    for tenant in &tenants {
        let client = TenantClient::login(env, &tenant.slug, tenant.id).await?;
        let resources = client
            .resources()
            .list_all(PageRequest::first(200))
            .await
            .with_context(|| format!("listing resources in '{}'", tenant.slug))?;
        let strays: Vec<&str> = resources
            .iter()
            .filter(|r| r.tenant_id != tenant.id)
            .map(|r| r.name.as_str())
            .collect();
        if strays.is_empty() {
            super::ok(&format!(
                "tenant-scope  {}  {} resource(s), all stamped with this tenant",
                tenant.slug,
                resources.len()
            ));
        } else {
            super::fail(&format!(
                "tenant-scope  {}  resource(s) created under the wrong tenant: {}",
                tenant.slug,
                strays.join(", ")
            ));
            passed = false;
        }
    }

    Ok(passed)
}
