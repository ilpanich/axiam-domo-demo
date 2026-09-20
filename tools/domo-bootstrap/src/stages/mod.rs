//! Stage implementations and the context they share.

pub mod broker;
pub mod catalog;
pub mod device_identity;
pub mod org_bootstrap;
pub mod pki;
pub mod service_certs;
pub mod tenant_admin;
pub mod tenants;

use std::ops::Deref;

use anyhow::{Context, Result};
use axiam_sdk::client::AxiamClient;
use axiam_sdk::management::page::PageRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Credentials persisted under `.secrets/axiam/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub org_slug: String,
    pub email: String,
    pub username: String,
    pub password: String,
}

/// Environment shared by every stage.
pub struct Env {
    pub axiam_url: String,
    pub org_slug: String,
    pub root_pem: Vec<u8>,
}

impl Env {
    pub fn load() -> Result<Self> {
        let axiam_url = std::env::var("DOMO_AXIAM_URL")
            .unwrap_or_else(|_| "https://axiam-server:8090".into());
        let org_slug = std::env::var("DOMO_ORG_SLUG")
            .context("DOMO_ORG_SLUG is not set (operator key — see .env.example)")?;
        let root_path = std::env::var("DOMO_ROOT_CA")
            .unwrap_or_else(|_| "/etc/domo/pki/root.pem".into());
        let root_pem = std::fs::read(&root_path)
            .with_context(|| format!("reading the organization root at {root_path}"))?;
        Ok(Self {
            axiam_url,
            org_slug,
            root_pem,
        })
    }
}

/// Read the saved super-admin credentials, or mint and persist a fresh set.
///
/// Generated rather than operator-supplied so no password is ever typed into a
/// file the operator edits; `just` never sees it either.
pub fn super_admin_credentials(org_slug: &str) -> Result<Credentials> {
    const PATH: &str = "axiam/super-admin.json";
    if domo_common::secrets::exists(PATH) {
        let raw = domo_common::secrets::read(PATH)?;
        return serde_json::from_slice(&raw).context("super-admin.json is malformed");
    }
    // Upper, lower, digit and symbol, so it satisfies any sane password policy.
    let password = format!("Ax1!{}", uuid::Uuid::new_v4());
    let creds = Credentials {
        org_slug: org_slug.to_owned(),
        email: "admin@domo.local".to_owned(),
        username: "admin".to_owned(),
        password,
    };
    persist_credentials(PATH, &creds)?;
    Ok(creds)
}

/// Read the saved tenant-admin credentials, or mint and persist a fresh set.
pub fn tenant_admin_credentials(org_slug: &str, tenant_slug: &str) -> Result<Credentials> {
    let path = format!("axiam/tenant-admin-{tenant_slug}.json");
    if domo_common::secrets::exists(&path) {
        let raw = domo_common::secrets::read(&path)?;
        return serde_json::from_slice(&raw)
            .with_context(|| format!("{path} is malformed"));
    }
    let creds = Credentials {
        org_slug: org_slug.to_owned(),
        email: format!("admin@{tenant_slug}.domo.local"),
        username: format!("{tenant_slug}-admin"),
        password: format!("Ax1!{}", uuid::Uuid::new_v4()),
    };
    persist_credentials(&path, &creds)?;
    Ok(creds)
}

fn persist_credentials(path: &str, creds: &Credentials) -> Result<()> {
    let json = serde_json::to_vec_pretty(creds).context("serializing credentials")?;
    domo_common::secrets::write(path, &json)?;
    Ok(())
}

/// A logged-in, **organization-scoped** client.
///
/// # Why this is a distinct type (T-04-01, P-1)
///
/// The SDK sends no acting-tenant header, so which tenant a management call
/// lands in is decided entirely by who logged in. An organization principal
/// addressing a tenant-scoped route does not fail: it succeeds, against the
/// reserved `organization` tenant, and creates the resource, the role or the
/// service account in a place nothing will ever look. No error is returned and
/// nothing in the response says which tenant was written.
///
/// Making the two clients different types moves that mistake from runtime to
/// compile time. A stage that must act inside a tenant takes `&TenantClient`
/// and simply cannot be handed this:
///
/// ```compile_fail
/// use domo_bootstrap::stages::{OrgClient, TenantClient};
///
/// fn tenant_scoped(_c: &TenantClient) {}
///
/// fn wrong(org: &OrgClient) {
///     // error[E0308]: expected `&TenantClient`, found `&OrgClient`
///     tenant_scoped(org);
/// }
/// ```
pub struct OrgClient {
    client: AxiamClient,
    /// The organization this client is logged into.
    pub org_id: Uuid,
}

impl Deref for OrgClient {
    type Target = AxiamClient;
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl OrgClient {
    /// Log in as the organization super-admin.
    ///
    /// The super-admin is used by `domo-bootstrap` and by nothing else: no
    /// service and no later phase holds it (T-04-03).
    pub async fn login(env: &Env) -> Result<Self> {
        let creds = super_admin_credentials(&env.org_slug)?;
        let client =
            domo_common::axiam::org_client(&env.axiam_url, &env.org_slug, &env.root_pem)?;
        let login = client
            .login(&creds.email, &creds.password)
            .await
            .context("super-admin login failed — run the org-bootstrap stage first")?;
        let org_id = login
            .org_id
            .context("organization-level login returned no org_id")?;
        Ok(Self { client, org_id })
    }

    /// Every tenant of the demo, with AXIAM's reserved `organization` tenant
    /// filtered out.
    ///
    /// That tenant is org-level plumbing, not a property-management company:
    /// it holds no devices, and treating it as one mints objects nothing
    /// issues from — which is exactly how plan 01-01 ended up with a signing
    /// CA too many.
    pub async fn demo_tenants(&self) -> Result<Vec<axiam_sdk::management::models::Tenant>> {
        let all = self
            .client
            .tenants()
            .in_org(self.org_id)
            .list_all(PageRequest::first(100))
            .await
            .context("listing tenants")?;
        Ok(all
            .into_iter()
            .filter(|t| t.slug != domo_common::ORG_TENANT_SLUG)
            .collect())
    }

    /// Resolve one demo tenant's id by its slug.
    pub async fn tenant_id(&self, slug: &str) -> Result<Uuid> {
        let tenants = self.demo_tenants().await?;
        tenants
            .iter()
            .find(|t| t.slug == slug)
            .map(|t| t.id)
            .with_context(|| format!("no tenant with slug '{slug}' — run the tenants stage first"))
    }

    /// The one signing CA of `tenant_id`, as a `(ca_id, pem)` pair.
    ///
    /// Signing CAs live on an organization-scoped route, so they are read here
    /// and handed to the tenant-scoped call that issues beneath them.
    pub async fn signing_ca(&self, tenant_id: Uuid) -> Result<(Uuid, String)> {
        let cas = self
            .client
            .ca_certificates()
            .in_org(self.org_id)
            .list_signing_cas_all(tenant_id, PageRequest::first(100))
            .await
            .context("listing signing CAs")?;
        let ca = cas
            .first()
            .context("this tenant has no signing CA — run the pki stage first")?;
        Ok((ca.id, ca.public_cert_pem.clone()))
    }
}

/// A logged-in, **tenant-scoped** client: the tenant's own admin principal.
///
/// Every object this creates inherits the acting user's tenant, which is what
/// makes PKI-03 work — a leaf signed through this client is that tenant's, not
/// the organization's and not the other tenant's. See [`OrgClient`] for why
/// the two are separate types.
pub struct TenantClient {
    client: AxiamClient,
    /// The tenant's slug, for messages and secret paths.
    pub slug: String,
    /// The tenant this client acts inside.
    pub tenant_id: Uuid,
}

impl Deref for TenantClient {
    type Target = AxiamClient;
    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl TenantClient {
    /// Log in as `slug`'s tenant admin.
    ///
    /// The admin is provisioned by the `tenant-admin` stage; a failure here
    /// usually means that stage has not run rather than that the password is
    /// wrong.
    pub async fn login(env: &Env, slug: &str, tenant_id: Uuid) -> Result<Self> {
        let admin = tenant_admin_credentials(&env.org_slug, slug)?;
        let client = domo_common::axiam::tenant_client(
            &env.axiam_url,
            &env.org_slug,
            slug,
            &env.root_pem,
        )?;
        client
            .login(&admin.email, &admin.password)
            .await
            .with_context(|| {
                format!("'{slug}' tenant-admin login failed — run the tenant-admin stage first")
            })?;
        Ok(Self {
            client,
            slug: slug.to_owned(),
            tenant_id,
        })
    }
}

/// Print one progress line in the stack's shared vocabulary.
pub fn step(msg: &str) {
    println!("  → {msg}");
}

/// Print one success line.
pub fn ok(msg: &str) {
    println!("  ✓ {msg}");
}

/// Print one failure line. Verification prints these and keeps going, so one
/// run reports every broken invariant rather than only the first.
pub fn fail(msg: &str) {
    println!("  ✗ {msg}");
}

/// True when a CSR and a certificate carry the same public key.
///
/// Compared as raw SubjectPublicKeyInfo bytes, the one representation that
/// cannot disagree on encoding details. A parse failure returns `false`: if we
/// cannot prove they match, we must not reuse — a certificate paired with the
/// wrong key is an mTLS identity whose halves disagree, and the failure
/// surfaces far from its cause.
#[must_use]
pub fn public_keys_match(csr_pem: &str, cert_pem: &str) -> bool {
    use x509_parser::prelude::{FromDer, X509Certificate, X509CertificationRequest};

    let Ok((_, csr_block)) = x509_parser::pem::parse_x509_pem(csr_pem.as_bytes()) else {
        return false;
    };
    let Ok((_, csr)) = X509CertificationRequest::from_der(&csr_block.contents) else {
        return false;
    };
    let Ok((_, cert_block)) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes()) else {
        return false;
    };
    let Ok((_, cert)) = X509Certificate::from_der(&cert_block.contents) else {
        return false;
    };
    csr.certification_request_info.subject_pki.raw == cert.tbs_certificate.subject_pki.raw
}

/// Run every Phase 1 authorization invariant and report each as `✓` or `✗`.
///
/// Fails the process only at the end, so a single run tells you everything
/// that is wrong rather than the first thing.
pub async fn verify_all() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    let mut passed = true;
    passed &= tenants::verify(&org, &env).await?;
    passed &= pki::verify(&org).await?;
    passed &= service_certs::verify(&org, &env).await?;

    anyhow::ensure!(passed, "authz-verify found at least one broken invariant");
    println!("✓ authz-verify");
    Ok(())
}
