//! Loading a smoke fixture's identity, and the one AXIAM call the SDK does not
//! cover.
//!
//! # The two halves of a device identity
//!
//! A fixture is only usable when its certificate and its private key are a
//! matching pair, and the certificate is chained to the CA that signed it.
//! [`Fixture::load`] assembles exactly that, and nothing that reaches it can be
//! half-formed: a missing piece is an error naming the recipe that produces it,
//! rather than a partially-built identity that fails much later as "building
//! the device HTTP client".
//!
//! # DF-009 — why the login is hand-rolled
//!
//! The Rust SDK has no `/api/v1/auth/device` operation. Its `device_login` is
//! the unrelated OAuth 2.0 Device Authorization Grant — the "type this code on
//! another screen" flow — so reaching for it here would be a category error.
//! The C++ SDK has `authenticate_device()`; the Rust one does not.
//!
//! # Nothing here prints a credential
//!
//! A response body is read unconditionally so a refusal can be classified, and
//! is never logged. The token it may contain is returned to the caller and
//! never rendered (T-06-08).

use anyhow::{Context, Result};
use serde::Deserialize;

/// `POST /api/v1/auth/device` response.
#[derive(Debug, Deserialize)]
pub struct DeviceAuth {
    pub access_token: String,
    pub token_type: String,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// One fixture's on-disk identity: the account it names, the tenant that
/// issued it, and the material to present.
pub struct Fixture {
    pub sa_id: String,
    pub tenant: String,
    /// Leaf first, then the issuing tenant's signing CA. Without the
    /// intermediate, neither AXIAM nor the broker can anchor the leaf.
    pub chain: Vec<u8>,
    pub key: Vec<u8>,
}

impl Fixture {
    pub fn load(name: &str) -> Result<Self> {
        let sa_id = domo_common::secrets::read_string(format!("smoke/{name}.sa-id"))
            .with_context(|| format!("no account id for '{name}' — run `just smoke-tree`"))?;
        let tenant = domo_common::secrets::read_string(format!("smoke/{name}.tenant"))
            .with_context(|| format!("no tenant recorded for '{name}'"))?;
        let leaf = domo_common::secrets::read(format!("smoke/{name}.pem"))
            .with_context(|| format!("no issued leaf for '{name}' — run `just smoke-certs`"))?;
        let key = domo_common::secrets::read(format!("smoke/{name}.key"))
            .with_context(|| format!("no private key for '{name}'"))?;
        let ca = domo_common::secrets::read(format!("axiam/{tenant}-ca.pem"))
            .with_context(|| format!("no signing CA for '{tenant}'"))?;

        let mut chain = leaf;
        chain.push(b'\n');
        chain.extend_from_slice(&ca);
        Ok(Self {
            sa_id,
            tenant,
            chain,
            key,
        })
    }
}

/// The REST device login, returning just its status.
pub async fn device_login_status(root_pem: &[u8], f: &Fixture) -> Result<u16> {
    let (status, _) = device_login(root_pem, f).await?;
    Ok(status)
}

/// The REST device login, returning the token it issued.
pub async fn device_login_token(root_pem: &[u8], f: &Fixture) -> Result<String> {
    let (status, body) = device_login(root_pem, f).await?;
    anyhow::ensure!(
        status == 200,
        "device login for the fixture returned {status}"
    );
    let auth: DeviceAuth = serde_json::from_str(&body).context("decoding the device token")?;
    anyhow::ensure!(auth.token_type == "Bearer", "unexpected token type");
    anyhow::ensure!(!auth.access_token.is_empty(), "empty access token");
    Ok(auth.access_token)
}

/// `POST /api/v1/auth/device` over mTLS. Hand-rolled: the Rust SDK has no
/// device-login operation (DF-009).
pub async fn device_login(root_pem: &[u8], f: &Fixture) -> Result<(u16, String)> {
    let axiam = env_or("DOMO_AXIAM_URL", "https://axiam-server:8090");

    let mut identity_pem = f.chain.clone();
    identity_pem.push(b'\n');
    identity_pem.extend_from_slice(&f.key);
    let identity = reqwest::Identity::from_pem(&identity_pem)
        .context("building the mTLS identity (leaf + CA + PKCS#8 key)")?;

    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(
            reqwest::Certificate::from_pem(root_pem).context("root CA is not valid PEM")?,
        )
        .identity(identity)
        .build()
        .context("building the device HTTP client")?;

    let resp = http
        .post(format!("{}/api/v1/auth/device", axiam.trim_end_matches('/')))
        .send()
        .await
        .context("POST /api/v1/auth/device")?;
    let status = resp.status().as_u16();
    // Read unconditionally: a non-200 body is never parsed as a token, and the
    // token itself is never printed.
    let body = resp.text().await.unwrap_or_default();
    Ok((status, body))
}
