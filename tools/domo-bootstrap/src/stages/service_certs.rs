//! One service account, and one certificate, per (service, tenant) — D-20.
//!
//! `mgmt@lakeside`, `mgmt@summit`, `twin@lakeside`, `twin@summit`. The point
//! of the split is blast radius: each account's client certificate is issued
//! by *its own* tenant's signing CA, so a bug in the Management Platform or
//! the Twin cannot reach across tenants no matter what it asks for. The
//! organization super-admin is used by `domo-bootstrap` and by nothing else
//! (T-04-03).
//!
//! # The issuing CA is the caller's responsibility (P-11, T-04-02)
//!
//! AXIAM checks that the issuing CA belongs to the organization. It does not
//! appear to check that a *tenant* signing CA belongs to the acting tenant —
//! so passing the wrong tenant's CA would produce a certificate that verifies
//! under the other tenant's chain, quietly dissolving the isolation the demo
//! exists to show. Every signing call below passes the acting tenant's own CA,
//! read from the organization-scoped route and handed to the tenant-scoped
//! call. `verify` asserts both directions afterwards; plan 01-06 asserts the
//! negative case and records what AXIAM actually does with it.
//!
//! # The bind is not optional
//!
//! Without it the certificate is valid TLS material that authenticates as
//! nobody: the handshake succeeds and the authorization check then finds no
//! subject to check. Device login returns 401 rather than an anonymous
//! principal, which is the right failure but a confusing one to debug.
//!
//! # C-5 — what these tokens can and cannot do
//!
//! A service-account token carries `aud: axiam:m2m`, and every REST management
//! route rejects that audience: each handler takes an authenticated *user*.
//! `POST /authz/check` does accept them. So these four accounts are usable for
//! authorization checks and device-style authentication, and are NOT usable
//! for management calls — Phase 2's Management Platform needs the per-tenant
//! admin user from [`super::tenant_admin`] instead.

use anyhow::{Context, Result};
use axiam_sdk::management::models::{
    BindCertificate, CertificateStatus, CertificateType, CreateServiceAccountRequest,
    SignCertificateCsrRequest,
};
use axiam_sdk::management::page::PageRequest;
use uuid::Uuid;

use super::{Env, OrgClient, TenantClient, fail, ok, step};

/// The services that get one account per tenant (D-20).
const SERVICES: &[&str] = &["mgmt", "twin"];

/// Service leaf validity. Well inside the tenant CA's own five-year window.
const SERVICE_CERT_DAYS: i32 = 365;

/// `mgmt@lakeside` — the account's natural key, and the file stem of its
/// credentials.
fn account_name(service: &str, slug: &str) -> String {
    format!("{service}@{slug}")
}

fn cert_path(service: &str, slug: &str) -> String {
    format!("axiam/service/{}.pem", account_name(service, slug))
}

fn key_path(service: &str, slug: &str) -> String {
    format!("axiam/service/{}.key", account_name(service, slug))
}

/// Issue every service credential for every demo tenant.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    for tenant in org.demo_tenants().await? {
        let (issuer_ca_id, _) = org.signing_ca(tenant.id).await?;
        let client = TenantClient::login(&env, &tenant.slug, tenant.id).await?;
        for service in SERVICES {
            ensure(&client, service, issuer_ca_id).await?;
        }
    }

    domo_common::secrets::mark_done("service-certs")?;
    Ok(())
}

/// Create (or resolve) one service account and give it exactly one
/// certificate, issued by `issuer_ca_id`.
async fn ensure(client: &TenantClient, service: &str, issuer_ca_id: Uuid) -> Result<()> {
    let slug = &client.slug;
    let name = account_name(service, slug);

    // --- the account, by natural key -------------------------------------
    let accounts = client
        .service_accounts()
        .list_all(PageRequest::first(200))
        .await
        .context("listing service accounts")?;
    let sa_id = if let Some(found) = accounts.iter().find(|a| a.name == name) {
        step(&format!("service account '{name}' already exists — reusing"));
        found.id
    } else {
        step(&format!("creating service account '{name}'"));
        client
            .service_accounts()
            .create(&CreateServiceAccountRequest {
                name: name.clone(),
                description: Some(format!(
                    "{service} service for tenant {slug} (D-20); mTLS client identity"
                )),
            })
            .await
            .with_context(|| format!("creating service account '{name}'"))?
            .id
        // The response also carries a client_secret, returned once. These
        // accounts authenticate by certificate, so it is deliberately not
        // persisted: an unused secret on disk is only a liability.
    };

    // --- the keypair, generated locally and reused ------------------------
    //
    // Reused rather than regenerated because the certificate is bound to a
    // specific public key. A fresh key on every run would leave the leaf on
    // disk paired with a key it does not match — an mTLS identity whose two
    // halves disagree, which surfaces far away from here as a handshake or
    // client-construction failure.
    let key = if domo_common::secrets::exists(key_path(service, slug)) {
        let pem = domo_common::secrets::read_string(key_path(service, slug))?;
        rcgen::KeyPair::from_pem(&pem)
            .with_context(|| format!("reading the stored keypair for '{name}'"))?
    } else {
        step(&format!("generating an Ed25519 keypair for '{name}'"));
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
            .with_context(|| format!("generating the keypair for '{name}'"))?;
        domo_common::secrets::write_string(key_path(service, slug), &key.serialize_pem())?;
        key
    };

    // The subject IS the identity: AXIAM resolves the service account from the
    // certificate's CN, so it must be the account's UUID and nothing else.
    let mut params =
        rcgen::CertificateParams::new(Vec::<String>::new()).context("building CSR params")?;
    let mut dn = rcgen::DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, sa_id.to_string());
    params.distinguished_name = dn;
    let csr_pem = params
        .serialize_request(&key)
        .context("serializing the CSR")?
        .pem()
        .context("encoding the CSR")?;

    // --- the certificate --------------------------------------------------
    let existing = client
        .certificates()
        .list_all(PageRequest::first(200))
        .await
        .context("listing certificates")?;
    if let Some(found) = existing.iter().find(|c| {
        c.bound_service_account_id == Some(sa_id)
            && c.status == CertificateStatus::Active
            && super::public_keys_match(&csr_pem, &c.public_cert_pem)
    }) {
        step(&format!("certificate for '{name}' already issued and bound — reusing"));
        domo_common::secrets::write_string(cert_path(service, slug), &found.public_cert_pem)?;
        ok(&format!("service credential ready for '{name}' ({})", found.id));
        return Ok(());
    }

    step(&format!("signing '{name}' under the '{slug}' signing CA"));
    let cert = client
        .certificates()
        .sign_csr(&SignCertificateCsrRequest {
            cert_type: CertificateType::Service,
            csr_pem,
            // Never the root, and never the other tenant's CA (P-11).
            issuer_ca_id,
            metadata: None,
            validity_days: SERVICE_CERT_DAYS,
        })
        .await
        .with_context(|| format!("signing the CSR for '{name}'"))?;

    step(&format!("binding the certificate to '{name}'"));
    client
        .service_accounts()
        .bind_certificate(
            sa_id,
            &BindCertificate {
                certificate_id: cert.id,
            },
        )
        .await
        .with_context(|| format!("binding the certificate to '{name}'"))?;

    domo_common::secrets::write_string(cert_path(service, slug), &cert.public_cert_pem)?;
    ok(&format!("service credential issued for '{name}' ({})", cert.id));
    Ok(())
}

/// Assert every service account has exactly one active certificate, issued by
/// its own tenant's signing CA (PKI-03).
///
/// Asserted by count, not by existence: two active certificates for one
/// account is not a harmless leftover — it means a re-run minted a second
/// identity, and which one a handshake presents is then a matter of which file
/// happens to be on disk.
pub async fn verify(org: &OrgClient, env: &Env) -> Result<bool> {
    let mut passed = true;

    for tenant in org.demo_tenants().await? {
        let (issuer_ca_id, _) = org.signing_ca(tenant.id).await?;
        let client = TenantClient::login(env, &tenant.slug, tenant.id).await?;

        let accounts = client
            .service_accounts()
            .list_all(PageRequest::first(200))
            .await
            .context("listing service accounts")?;
        let certs = client
            .certificates()
            .list_all(PageRequest::first(200))
            .await
            .context("listing certificates")?;

        for service in SERVICES {
            let name = account_name(service, &tenant.slug);
            let Some(account) = accounts.iter().find(|a| a.name == name) else {
                fail(&format!("service-certs  '{name}' does not exist"));
                passed = false;
                continue;
            };

            let bound: Vec<_> = certs
                .iter()
                .filter(|c| {
                    c.bound_service_account_id == Some(account.id)
                        && c.status == CertificateStatus::Active
                })
                .collect();

            if bound.len() != 1 {
                fail(&format!(
                    "service-certs  '{name}' has {} active certificate(s), expected 1",
                    bound.len()
                ));
                passed = false;
                continue;
            }
            let cert = bound[0];
            if cert.issuer_ca_id != issuer_ca_id {
                fail(&format!(
                    "service-certs  '{name}' was issued by CA {} but its tenant's CA is {}",
                    cert.issuer_ca_id, issuer_ca_id
                ));
                passed = false;
                continue;
            }
            if cert.tenant_id != tenant.id {
                fail(&format!(
                    "service-certs  '{name}' carries tenant {} but belongs to {}",
                    cert.tenant_id, tenant.id
                ));
                passed = false;
                continue;
            }
            ok(&format!(
                "service-certs  {name}  1 active certificate, issued by its own tenant's CA"
            ));
        }
    }

    Ok(passed)
}
