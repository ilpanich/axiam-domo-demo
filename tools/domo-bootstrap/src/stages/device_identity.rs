//! Stage 4 — give one device a real identity.
//!
//! Two phases, because the ordering is forced by the design: the certificate's
//! subject must be `CN=<service-account UUID>`, and that UUID does not exist
//! until the account has been created. So the account is created first
//! ([`ensure_account`]), the probe then generates a key and a CSR naming it,
//! and only then can the CSR be signed ([`sign`]).
//!
//! # Why this runs tenant-scoped (P-1, P-11, D-37)
//!
//! The SDK sends no acting-tenant header, so "which tenant signs this" is
//! decided entirely by who is logged in. Signing as the super-admin would
//! silently issue under the wrong tenant's authority. The signing call
//! therefore runs on a client logged in as the *tenant* admin that stage 2
//! provisioned.
//!
//! # The bind is not optional
//!
//! Without `bind_certificate`, the issued certificate is valid TLS material
//! that authenticates as nobody: the handshake succeeds and the authorization
//! check finds no subject to check.

use anyhow::{Context, Result, bail};
use axiam_sdk::management::models::{
    BindCertificate, CertificateType, CreateServiceAccountRequest, SignCertificateCsrRequest,
};
use axiam_sdk::management::page::PageRequest;
use uuid::Uuid;

use super::{Env, ok, step, super_admin_credentials, tenant_admin_credentials};

/// Device leaf validity. Well inside the tenant CA's own window.
const DEVICE_CERT_DAYS: i32 = 90;

/// Where the probe's account id is published for the probe to read.
const SA_ID_PATH: &str = "probe/sa-id";

/// Resolve `(org_id, tenant_id, signing_ca_id)` using the organization session.
///
/// The signing-CA list lives on an organization-scoped route, so it is read
/// with the super-admin client and then handed to the tenant-scoped call.
async fn resolve_tenant_ca(env: &Env, tenant_slug: &str) -> Result<(Uuid, Uuid)> {
    let creds = super_admin_credentials(&env.org_slug)?;
    let sdk = domo_common::axiam::org_client(&env.axiam_url, &env.org_slug, &env.root_pem)?;
    let login = sdk
        .login(&creds.email, &creds.password)
        .await
        .context("super-admin login failed — run the org-bootstrap stage first")?;
    let org_id = login
        .org_id
        .context("organization-level login returned no org_id")?;

    let tenants = sdk
        .tenants()
        .in_org(org_id)
        .list_all(PageRequest::first(100))
        .await
        .context("listing tenants")?;
    let tenant = tenants
        .iter()
        .find(|t| t.slug == tenant_slug)
        .with_context(|| format!("no tenant with slug '{tenant_slug}'"))?;

    let cas = sdk
        .ca_certificates()
        .in_org(org_id)
        .list_signing_cas_all(tenant.id, PageRequest::first(100))
        .await
        .with_context(|| format!("listing signing CAs for '{tenant_slug}'"))?;
    let ca = cas
        .first()
        .with_context(|| format!("tenant '{tenant_slug}' has no signing CA — run the pki stage"))?;

    Ok((tenant.id, ca.id))
}

/// Phase 1 — create (or find) the device's service account.
pub async fn ensure_account(name: &str, tenant_slug: &str) -> Result<Uuid> {
    let env = Env::load()?;
    let admin = tenant_admin_credentials(&env.org_slug, tenant_slug)?;

    let sdk = domo_common::axiam::tenant_client(
        &env.axiam_url,
        &env.org_slug,
        tenant_slug,
        &env.root_pem,
    )?;
    sdk.login(&admin.email, &admin.password)
        .await
        .with_context(|| format!("'{tenant_slug}' tenant-admin login failed"))?;

    // Natural key: the account name.
    let existing = sdk
        .service_accounts()
        .list_all(PageRequest::first(100))
        .await
        .context("listing service accounts")?;

    let id = if let Some(found) = existing.iter().find(|s| s.name == name) {
        step(&format!("service account '{name}' already exists — reusing"));
        found.id
    } else {
        step(&format!("creating service account '{name}'"));
        let created = sdk
            .service_accounts()
            .create(&CreateServiceAccountRequest {
                name: name.to_owned(),
                description: Some("Phase 1 tracer device".to_owned()),
            })
            .await
            .with_context(|| format!("creating service account '{name}'"))?;
        // The response also carries a client_secret, returned once. A device
        // that authenticates by certificate does not need it, so it is not
        // persisted.
        created.id
    };

    domo_common::secrets::write_string(SA_ID_PATH, &id.to_string())?;
    ok(&format!("device account ready ({id})"));
    Ok(id)
}

/// Phase 2 — sign the device's CSR and bind the result to its account.
pub async fn sign(csr_path: &str, out_path: &str, tenant_slug: &str) -> Result<()> {
    let env = Env::load()?;
    let admin = tenant_admin_credentials(&env.org_slug, tenant_slug)?;

    let csr_pem = std::fs::read_to_string(csr_path)
        .with_context(|| format!("reading the CSR at {csr_path}"))?;
    if csr_pem.contains("BEGIN NEW CERTIFICATE REQUEST") {
        bail!(
            "the CSR uses the legacy OpenSSL 'BEGIN NEW CERTIFICATE REQUEST' header, \
             which AXIAM does not accept; emit a 'BEGIN CERTIFICATE REQUEST' block"
        );
    }

    let sa_id: Uuid = domo_common::secrets::read_string(SA_ID_PATH)
        .context("no device account id on disk — run the device-account phase first")?
        .parse()
        .context("the stored device account id is not a UUID")?;

    let (_tenant_id, issuer_ca_id) = resolve_tenant_ca(&env, tenant_slug).await?;

    let sdk = domo_common::axiam::tenant_client(
        &env.axiam_url,
        &env.org_slug,
        tenant_slug,
        &env.root_pem,
    )?;
    sdk.login(&admin.email, &admin.password)
        .await
        .with_context(|| format!("'{tenant_slug}' tenant-admin login failed"))?;

    // Idempotency: a certificate already bound to this account and still valid
    // is reused rather than re-minted, so a re-run does not pile up leaves.
    let existing = sdk
        .certificates()
        .list_all(PageRequest::first(100))
        .await
        .context("listing certificates")?;
    if let Some(found) = existing
        .iter()
        .find(|c| c.bound_service_account_id == Some(sa_id))
    {
        step("device certificate already issued and bound — reusing");
        std::fs::write(out_path, &found.public_cert_pem)
            .with_context(|| format!("writing {out_path}"))?;
        ok(&format!("device certificate ready ({})", found.id));
        domo_common::secrets::mark_done("device-identity")?;
        return Ok(());
    }

    step("signing the device CSR under the tenant's signing CA");
    let cert = sdk
        .certificates()
        .sign_csr(&SignCertificateCsrRequest {
            cert_type: CertificateType::Device,
            csr_pem,
            issuer_ca_id,
            metadata: None,
            validity_days: DEVICE_CERT_DAYS,
        })
        .await
        .context("signing the device CSR")?;

    step("binding the certificate to the service account");
    sdk.service_accounts()
        .bind_certificate(
            sa_id,
            &BindCertificate {
                certificate_id: cert.id,
            },
        )
        .await
        .context("binding the device certificate to its service account")?;

    std::fs::write(out_path, &cert.public_cert_pem)
        .with_context(|| format!("writing {out_path}"))?;
    ok(&format!("device certificate issued and bound ({})", cert.id));

    domo_common::secrets::mark_done("device-identity")?;
    Ok(())
}

