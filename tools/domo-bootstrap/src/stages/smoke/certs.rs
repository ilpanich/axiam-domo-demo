//! Certificates for the smoke fixtures, and the one experiment whose answer
//! nobody knew from reading the code (PKI-03, P-11, DF-017).
//!
//! # The split with the probe is the point
//!
//! The probe generates every private key and every certificate signing request
//! itself; this stage only signs what it is handed and binds the result. No
//! device private key ever exists on this side of the boundary (D-23, DEV-05).
//!
//! # One fixture is signed and deliberately NOT bound
//!
//! Without `bind_certificate` an issued certificate is valid TLS material that
//! authenticates as nobody: the handshake succeeds and AXIAM then finds no
//! subject. That is a real, reachable state, so the matrix asserts what it
//! looks like from outside — a 401 — rather than leaving it to a comment.
//!
//! # The cross-tenant issuance attempt
//!
//! [`cross_tenant_attempt`] asks AXIAM to sign the probe's request under the
//! OTHER tenant's signing CA, as this tenant's admin. Source reading says
//! `prepare_leaf_issuance` checks that the issuing CA belongs to the
//! organization but not that a tenant signing CA belongs to the acting tenant
//! (DF-017). A refusal closes that question; an acceptance is a confirmed gap,
//! is written into the findings log, and fails this stage — because a result
//! that only appears in a log nobody reads is the same as no result.

use anyhow::{Context, Result};
use axiam_sdk::management::models::{
    BindCertificate, CertificateStatus, CertificateType, SignCertificateCsrRequest,
};
use axiam_sdk::management::page::PageRequest;
use uuid::Uuid;

use super::{OTHER_ACCOUNT, OTHER_TENANT, PEER_ACCOUNT, PROBE_ACCOUNT, TENANT, UNBOUND_ACCOUNT};
use crate::stages::{Env, OrgClient, TenantClient, ok, public_keys_match, step};

/// Fixture leaf validity. Well inside the tenant CA's own window.
const FIXTURE_CERT_DAYS: i32 = 90;

/// Issue every fixture certificate, then run the cross-tenant experiment.
pub async fn run() -> Result<()> {
    let env = Env::load()?;
    let org = OrgClient::login(&env).await?;

    let lakeside_id = org.tenant_id(TENANT).await?;
    let (lakeside_ca, _) = org.signing_ca(lakeside_id).await?;
    let lakeside = TenantClient::login(&env, TENANT, lakeside_id).await?;

    for account in [PROBE_ACCOUNT, PEER_ACCOUNT] {
        issue(&lakeside, account, lakeside_ca, Bind::Yes).await?;
    }
    issue(&lakeside, UNBOUND_ACCOUNT, lakeside_ca, Bind::No).await?;

    let other_id = org.tenant_id(OTHER_TENANT).await?;
    let (other_ca, _) = org.signing_ca(other_id).await?;
    let other = TenantClient::login(&env, OTHER_TENANT, other_id).await?;
    issue(&other, OTHER_ACCOUNT, other_ca, Bind::Yes).await?;

    cross_tenant_attempt(&lakeside, other_ca).await?;

    println!("✓ smoke-certs");
    Ok(())
}

/// Whether an issued certificate is bound to its account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bind {
    Yes,
    /// Deliberately left unbound, so the matrix can observe what an unbound
    /// certificate authenticates as.
    No,
}

/// Sign one fixture's request under `issuer_ca`, and bind unless told not to.
async fn issue(client: &TenantClient, account: &str, issuer_ca: Uuid, bind: Bind) -> Result<()> {
    let sa_id: Uuid = domo_common::secrets::read_string(super::sa_id_path(account))
        .with_context(|| format!("no account id for '{account}' — run `just smoke-tree`"))?
        .parse()
        .context("the stored account id is not a UUID")?;
    let csr_pem = domo_common::secrets::read_string(format!("smoke/{account}.csr"))
        .with_context(|| format!("no CSR for '{account}' — run the smoke-keys step"))?;

    let existing = client
        .certificates()
        .list_all(PageRequest::first(500))
        .await
        .context("listing certificates")?;
    // Reuse only a certificate that matches the key the fixture holds RIGHT
    // NOW. One issued for an older keypair is worse than none: the two halves
    // of the mTLS identity would disagree.
    let want_binding = if bind == Bind::Yes { Some(sa_id) } else { None };
    if let Some(found) = existing.iter().find(|c| {
        c.status == CertificateStatus::Active
            && c.bound_service_account_id == want_binding
            && c.issuer_ca_id == issuer_ca
            && public_keys_match(&csr_pem, &c.public_cert_pem)
    }) {
        step(&format!("certificate for '{account}' already issued — reusing"));
        domo_common::secrets::write_string(
            format!("smoke/{account}.pem"),
            &found.public_cert_pem,
        )?;
        return Ok(());
    }

    step(&format!("signing '{account}' under the '{}' signing CA", client.slug));
    let cert = client
        .certificates()
        .sign_csr(&SignCertificateCsrRequest {
            cert_type: CertificateType::Device,
            csr_pem,
            issuer_ca_id: issuer_ca,
            metadata: None,
            validity_days: FIXTURE_CERT_DAYS,
        })
        .await
        .with_context(|| format!("signing the CSR for '{account}'"))?;

    if bind == Bind::Yes {
        step(&format!("binding the certificate to '{account}'"));
        client
            .service_accounts()
            .bind_certificate(
                sa_id,
                &BindCertificate {
                    certificate_id: cert.id,
                },
            )
            .await
            .with_context(|| format!("binding the certificate to '{account}'"))?;
    } else {
        step(&format!("'{account}': certificate issued and deliberately NOT bound"));
    }

    domo_common::secrets::write_string(format!("smoke/{account}.pem"), &cert.public_cert_pem)?;
    ok(&format!("fixture credential ready for '{account}' ({})", cert.id));
    Ok(())
}

/// Where the experiment's answer is left for the matrix to report.
pub const OUTCOME_PATH: &str = "smoke/cross-tenant-issuance.json";
/// The forged-common-name leaf, when AXIAM agrees to mint one.
pub const FORGED_PEM: &str = "smoke/forged-cn.pem";
/// That leaf's id, so teardown can revoke it.
pub const FORGED_ID: &str = "smoke/forged-cn.id";

/// Ask AXIAM, as THIS tenant's admin, to sign the probe's own request under the
/// OTHER tenant's signing CA.
///
/// # Why the outcome is recorded rather than decided here
///
/// The answer belongs to the matrix: the plan requires an acceptance to fail
/// *the matrix*, and only an admin client can produce the answer. Failing here
/// instead would stop the other eleven cases from running at all, which trades
/// away every other regression this suite exists to catch. So this stage
/// records what happened and the matrix reports and fails on it.
///
/// # Why the certificate is kept
///
/// If AXIAM agrees, what it just minted is a leaf carrying THIS tenant's device
/// as its subject, signed by the OTHER tenant's authority — precisely the
/// forged-common-name certificate D-24 names as the demo's residual risk. That
/// is the strict form of the "other tenant's CA" case, and it is only testable
/// because the gap is real. Keeping it is what lets the matrix ask whether the
/// compensating controls actually hold. `smoke-teardown` revokes it.
async fn cross_tenant_attempt(client: &TenantClient, other_ca: Uuid) -> Result<()> {
    let csr_pem = domo_common::secrets::read_string(format!("smoke/{PROBE_ACCOUNT}.csr"))
        .context("no probe CSR to attempt cross-tenant issuance with")?;

    // A leaf recorded from a previous run is revoked first: this experiment
    // must mint at most one live forged certificate at a time.
    revoke_recorded_forgery(client).await?;

    let attempted = client
        .certificates()
        .sign_csr(&SignCertificateCsrRequest {
            cert_type: CertificateType::Device,
            csr_pem,
            // Deliberately the WRONG CA: the other tenant's. Everywhere else in
            // this repository this argument is the acting tenant's own.
            issuer_ca_id: other_ca,
            metadata: None,
            validity_days: FIXTURE_CERT_DAYS,
        })
        .await;

    let outcome = match attempted {
        Err(e) => {
            step(&format!(
                "cross-tenant issuance refused by AXIAM ({})",
                first_line(&e.to_string())
            ));
            serde_json::json!({
                "accepted": false,
                "detail": first_line(&e.to_string()),
            })
        }
        Ok(cert) => {
            step(&format!(
                "cross-tenant issuance ACCEPTED — certificate {} minted under another \
                 tenant's CA (DF-017 confirmed)",
                cert.id
            ));
            domo_common::secrets::write_string(FORGED_PEM, &cert.public_cert_pem)?;
            domo_common::secrets::write_string(FORGED_ID, &cert.id.to_string())?;
            serde_json::json!({
                "accepted": true,
                "certificate_id": cert.id.to_string(),
                // Which tenant AXIAM stamps on a cross-issued leaf is the
                // evidence that says whether this is a bookkeeping slip or a
                // genuine isolation break.
                "certificate_tenant_id": cert.tenant_id.to_string(),
                "issuer_ca_id": cert.issuer_ca_id.to_string(),
                "acting_tenant": client.slug.clone(),
                "acting_tenant_id": client.tenant_id.to_string(),
            })
        }
    };

    domo_common::secrets::write(
        OUTCOME_PATH,
        &serde_json::to_vec_pretty(&outcome).context("serializing the experiment's outcome")?,
    )?;
    Ok(())
}

/// Revoke the forged leaf a previous run recorded, if there is one.
pub async fn revoke_recorded_forgery(client: &TenantClient) -> Result<()> {
    let Ok(raw) = domo_common::secrets::read_string(FORGED_ID) else {
        return Ok(());
    };
    let Ok(id) = raw.parse::<Uuid>() else {
        return Ok(());
    };
    match client.certificates().revoke(id).await {
        Ok(()) => step(&format!("revoked the previously forged certificate {id}")),
        // Already revoked, or already gone with its tenant: either way there is
        // nothing live left, which is the property that matters.
        Err(e) => step(&format!("forged certificate {id} could not be revoked ({e})")),
    }
    Ok(())
}

/// The first line of an error, so a multi-line transport message does not
/// break the one-line-per-case contract.
fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).trim().to_owned()
}
