//! Per-tenant verifier registry and the user-to-tenant session cache.
//!
//! # Why one verifier per tenant
//!
//! AXIAM's key-set endpoint is **organization-wide**. A valid signature
//! therefore proves only "some tenant in this organization issued this token" —
//! never "*this* tenant issued it". Without a per-tenant assertion,
//! cross-tenant impersonation is one valid signature away (T-05-02).
//!
//! So each tenant gets its own [`JwksVerifier`], built with
//! `expect_tenant_id` and the machine-to-machine audience. A token is routed to
//! exactly one of them — never tried against each in turn, which would make the
//! assertion meaningless, because a token would succeed as soon as *any* tenant
//! accepted it.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use axiam_sdk::token::JwksVerifier;

pub use crate::rmq::decide::Session;

/// The audience AXIAM stamps on a device (machine-to-machine) token.
pub const M2M_AUDIENCE: &str = "axiam:m2m";

/// Tenant slug and verifier, keyed by tenant identifier.
pub struct TenantRegistry {
    http: reqwest::Client,
    axiam_url: url::Url,
    tenant_map_file: String,
    /// tenant_id → slug, re-read from disk on a miss.
    slugs: RwLock<HashMap<String, String>>,
    /// tenant_id → verifier, built once per tenant.
    verifiers: RwLock<HashMap<String, Arc<JwksVerifier>>>,
}

impl TenantRegistry {
    /// Build the registry, eagerly loading whatever tenants are on disk.
    #[must_use]
    pub fn new(http: reqwest::Client, axiam_url: url::Url, tenant_map_file: String) -> Self {
        let slugs = load_tenant_map(&tenant_map_file);
        Self {
            http,
            axiam_url,
            tenant_map_file,
            slugs: RwLock::new(slugs),
            verifiers: RwLock::new(HashMap::new()),
        }
    }

    /// Build every verifier the tenant map currently names.
    ///
    /// Called once at startup so a misconfigured tenant is a startup failure
    /// rather than a surprise on the first CONNECT. A tenant added later is
    /// still picked up lazily by [`Self::verifier`] — the bootstrap writes the
    /// map after the Twin is already listening, so eager-only would break the
    /// very first run.
    ///
    /// # Errors
    ///
    /// When a tenant identifier on disk is not a UUID, or the verifier cannot
    /// be constructed against the configured AXIAM URL.
    pub fn warm(&self) -> Result<usize> {
        let ids: Vec<String> = self
            .slugs
            .read()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        for id in &ids {
            self.verifier(id)?;
        }
        Ok(ids.len())
    }

    /// How many verifiers are currently registered.
    #[must_use]
    pub fn registered(&self) -> usize {
        self.verifiers.read().map(|v| v.len()).unwrap_or(0)
    }

    /// Resolve a tenant slug, re-reading the map file on a miss.
    #[must_use]
    pub fn slug(&self, tenant_id: &str) -> Option<String> {
        if let Ok(map) = self.slugs.read()
            && let Some(slug) = map.get(tenant_id)
        {
            return Some(slug.clone());
        }
        let fresh = load_tenant_map(&self.tenant_map_file);
        let slug = fresh.get(tenant_id).cloned();
        if let Ok(mut map) = self.slugs.write() {
            *map = fresh;
        }
        slug
    }

    /// True when this tenant identifier is one this Twin serves.
    #[must_use]
    pub fn knows(&self, tenant_id: &str) -> bool {
        self.slug(tenant_id).is_some()
    }

    /// The verifier pinned to one tenant and to the m2m audience.
    ///
    /// # Errors
    ///
    /// When `tenant_id` is not a UUID, or the verifier cannot be built.
    pub fn verifier(&self, tenant_id: &str) -> Result<Arc<JwksVerifier>> {
        if let Ok(v) = self.verifiers.read()
            && let Some(found) = v.get(tenant_id)
        {
            return Ok(found.clone());
        }
        let uuid: uuid::Uuid = tenant_id.parse().context("tenant_id claim is not a UUID")?;
        let built = Arc::new(
            JwksVerifier::new(self.http.clone(), &self.axiam_url)
                .context("building the key-set verifier")?
                .expect_tenant_id(uuid)
                .expect_audience(M2M_AUDIENCE),
        );
        if let Ok(mut v) = self.verifiers.write() {
            v.insert(tenant_id.to_owned(), built.clone());
        }
        Ok(built)
    }
}

/// The in-memory user-to-session cache.
///
/// Filled by a successful CONNECT and read by the three token-less endpoints.
/// A Twin restart empties it, which forces every device to reconnect rather
/// than silently granting — accepted for Phase 1 and revisited in Phase 3,
/// where 114 devices make a synchronized reconnect a real concern (MQTT-04).
#[derive(Default)]
pub struct SessionCache(RwLock<HashMap<String, Session>>);

impl SessionCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, username: &str) -> Option<Session> {
        self.0.read().ok()?.get(username).cloned()
    }

    pub fn insert(&self, username: impl Into<String>, session: Session) {
        if let Ok(mut s) = self.0.write() {
            s.insert(username.into(), session);
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.read().map(|s| s.len()).unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Read `tenant_id` out of a token **without verifying it**.
///
/// This only chooses *which* verifier to use. The chosen verifier is built with
/// `expect_tenant_id`, so a token lying about its tenant fails verification a
/// moment later — the unverified peek is a routing hint and can promote
/// nothing.
#[must_use]
pub fn peek_tenant_id(jwt: &str) -> Option<String> {
    use base64::Engine as _;
    let payload = jwt.split('.').nth(1)?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    v.get("tenant_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// tenant_id → slug, from the map the bootstrap publishes.
///
/// A missing or malformed file is not fatal: it means no tenant resolves, so
/// every decision that needs one denies. Failing closed beats failing loudly.
#[must_use]
pub fn load_tenant_map(path: &str) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(&s).ok())
        .unwrap_or_default()
}
