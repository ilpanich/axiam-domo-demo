//! `AxiamClient` factories.
//!
//! # Why there are two of these (P-1, D-37)
//!
//! The SDK never sends an acting-tenant header. Which tenant a management call
//! acts on is decided by the identity that logged in, so "organization work"
//! and "tenant work" need two *separate* logged-in clients — not one client
//! with a switchable scope. Getting this wrong does not fail loudly: the call
//! succeeds against the wrong tenant.
//!
//! [`org_client`] uses the reserved `organization` tenant slug (CONTRACT
//! §5.2.1) and logs in as the super-admin. [`tenant_client`] logs in as that
//! tenant's own admin, which is what leaf issuance must use so the certificate
//! is signed by the right tenant's CA (P-11).

use anyhow::{Context, Result};
use axiam_sdk::client::AxiamClient;

use crate::ORG_TENANT_SLUG;

/// Build an organization-scoped client. Not yet logged in.
pub fn org_client(base_url: &str, org_slug: &str, root_pem: &[u8]) -> Result<AxiamClient> {
    AxiamClient::builder()
        .base_url(base_url)
        .context("invalid AXIAM base URL")?
        .org_slug(org_slug)
        .tenant_slug(ORG_TENANT_SLUG)
        .with_custom_ca(root_pem)
        .context("AXIAM SDK rejected the organization root CA")?
        .build()
        .context("building the organization-scoped AXIAM client")
}

/// Build a tenant-scoped client. Not yet logged in.
pub fn tenant_client(
    base_url: &str,
    org_slug: &str,
    tenant_slug: &str,
    root_pem: &[u8],
) -> Result<AxiamClient> {
    AxiamClient::builder()
        .base_url(base_url)
        .context("invalid AXIAM base URL")?
        .org_slug(org_slug)
        .tenant_slug(tenant_slug)
        .with_custom_ca(root_pem)
        .context("AXIAM SDK rejected the organization root CA")?
        .build()
        .with_context(|| format!("building the AXIAM client for tenant '{tenant_slug}'"))
}
