//! Stage implementations and the context they share.

pub mod broker;
pub mod device_identity;
pub mod org_bootstrap;
pub mod pki;
pub mod tenants;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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

/// Print one progress line in the stack's shared vocabulary.
pub fn step(msg: &str) {
    println!("  → {msg}");
}

/// Print one success line.
pub fn ok(msg: &str) {
    println!("  ✓ {msg}");
}
