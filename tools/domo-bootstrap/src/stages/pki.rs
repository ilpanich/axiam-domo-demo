//! Stage 3 — hand AXIAM the organization root (BYOK) and mint tenant CAs.
//!
//! This is the architectural bet of the whole project in three calls:
//!
//! 1. `import_ca` — the offline root crosses the process boundary exactly once,
//!    over TLS, private key included, so AXIAM can sign beneath it.
//! 2. `set_mtls_trust_anchor` — AXIAM will now accept certificates that chain
//!    to it at the TLS layer.
//! 3. `generate_signing_ca` — one intermediate per tenant, signed by that root.
//!
//! After this, every device certificate in the demo chains
//! root → tenant CA → leaf, and there is no second certificate authority
//! anywhere.
//!
//! # Handling the returned key
//!
//! `generate_signing_ca` returns the tenant CA's private key ONCE, as
//! `Sensitive<String>`. AXIAM keeps custody; we discard it deliberately and
//! never debug-format a struct that might still contain one (T-01-04).

use anyhow::{Context, Result, bail};
use axiam_sdk::Sensitive;
use axiam_sdk::management::models::{
    CreateIntermediateCaRequest, ImportCaCertificateRequest, KeyAlgorithm, SetMtlsTrustAnchor,
};
use axiam_sdk::management::page::PageRequest;

use super::{Env, ok, step, super_admin_credentials};

/// Validity of a tenant signing CA. Comfortably inside the root's 10 years.
const SIGNING_CA_DAYS: i32 = 1825;

/// Compare two PEM certificates by their base64 payload.
///
/// Fingerprint formats differ between AXIAM's rendering and OpenSSL's, so the
/// certificate body itself is the one comparison that cannot disagree on
/// formatting.
fn same_certificate(a: &str, b: &str) -> bool {
    let strip = |s: &str| -> String {
        s.lines()
            .filter(|l| !l.starts_with("-----"))
            .flat_map(str::chars)
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    let (a, b) = (strip(a), strip(b));
    !a.is_empty() && a == b
}

pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let creds = super_admin_credentials(&env.org_slug)?;
    let root_pem = String::from_utf8(env.root_pem.clone())
        .context("the organization root is not valid UTF-8 PEM")?;

    let sdk = domo_common::axiam::org_client(&env.axiam_url, &env.org_slug, &env.root_pem)?;
    let login = sdk
        .login(&creds.email, &creds.password)
        .await
        .context("super-admin login failed — run the org-bootstrap stage first")?;
    let org_id = login
        .org_id
        .context("organization-level login returned no org_id")?;

    // --- 1. import the root, once ------------------------------------------
    let cas = sdk
        .ca_certificates()
        .in_org(org_id)
        .list_all(PageRequest::first(100))
        .await
        .context("listing organization CA certificates")?;

    let root_ca = if let Some(found) = cas
        .iter()
        .find(|c| same_certificate(&c.public_cert_pem, &root_pem))
    {
        step("organization root already imported — reusing");
        found.clone()
    } else {
        step("importing the organization root (BYOK, private key included)");
        let key_pem = domo_common::secrets::read_string("pki/root.key")
            .context("reading the root private key for the BYOK import")?;
        sdk.ca_certificates()
            .in_org(org_id)
            .import_ca(&ImportCaCertificateRequest {
                // The ONLY time this key crosses a process boundary.
                private_key_pem: Some(Sensitive::new(key_pem)),
                public_cert_pem: root_pem.clone(),
            })
            .await
            .context("importing the organization root CA")?
    };

    // --- 2. anchor it for mTLS ---------------------------------------------
    if root_ca.mtls_trust_anchor == Some(true) {
        step("root is already the mTLS trust anchor");
    } else {
        step("setting the root as the mTLS trust anchor");
        sdk.ca_certificates()
            .in_org(org_id)
            .set_mtls_trust_anchor(root_ca.id, &SetMtlsTrustAnchor { enabled: true })
            .await
            .context("anchoring the organization root for mTLS")?;
    }
    ok(&format!("organization root anchored ({})", root_ca.id));

    // --- 3. one signing CA per tenant --------------------------------------
    let tenants = sdk
        .tenants()
        .in_org(org_id)
        .list_all(PageRequest::first(100))
        .await
        .context("listing tenants")?;
    if tenants.is_empty() {
        bail!("no tenants exist — run the tenants stage first");
    }

    for tenant in tenants {
        // `organization` is AXIAM's reserved org-level tenant (CONTRACT §5.2.1),
        // not a demo tenant. It holds no devices, so minting a signing CA for it
        // would be an object nobody issues from — and would break the
        // "one signing CA per tenant, no more and no fewer" count (PKI-02).
        if tenant.slug == domo_common::ORG_TENANT_SLUG {
            continue;
        }

        let existing = sdk
            .ca_certificates()
            .in_org(org_id)
            .list_signing_cas_all(tenant.id, PageRequest::first(100))
            .await
            .with_context(|| format!("listing signing CAs for '{}'", tenant.slug))?;

        let ca_pem = if let Some(found) = existing.first() {
            step(&format!(
                "tenant '{}' already has a signing CA — reusing",
                tenant.slug
            ));
            found.public_cert_pem.clone()
        } else {
            step(&format!("generating the signing CA for '{}'", tenant.slug));
            let generated = sdk
                .ca_certificates()
                .in_org(org_id)
                .generate_signing_ca(
                    tenant.id,
                    &CreateIntermediateCaRequest {
                        // D-11/D-27: Ed25519. If Erlang's TLS rejects the chain
                        // (assumption A2), the one-field fallback is
                        // KeyAlgorithm::Rsa4096 — but record the finding first.
                        key_algorithm: KeyAlgorithm::Ed25519,
                        parent_ca_id: root_ca.id,
                        // The bare common name, NOT "CN=<name>": AXIAM builds
                        // the DN itself, so passing a prefixed value yields a
                        // subject of `CN=CN=<name>` (observed at runtime).
                        subject: format!("{} Signing CA", tenant.name),
                        validity_days: SIGNING_CA_DAYS,
                    },
                )
                .await
                .with_context(|| format!("generating the signing CA for '{}'", tenant.slug))?;
            // The private key was returned once. AXIAM has custody; drop it
            // here without reading, logging or persisting it.
            drop(generated.private_key_pem);
            generated.public_cert_pem
        };

        // The probe needs this to build its certificate chain for mTLS.
        domo_common::secrets::write_string(
            format!("axiam/{}-ca.pem", tenant.slug),
            &ca_pem,
        )?;
        ok(&format!("signing CA ready for '{}'", tenant.slug));
    }

    domo_common::secrets::mark_done("pki")?;
    Ok(())
}
