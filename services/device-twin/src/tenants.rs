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
    /// tenant_id → slug, re-read from disk when the file changes.
    slugs: RwLock<HashMap<String, String>>,
    /// The modification time the `slugs` map was last read from.
    ///
    /// A miss re-reads only when this has moved. Without the gate, a stream of
    /// connects naming random tenants would each cost a read and a parse —
    /// unbounded work from an unauthenticated caller (T-05-08). With it, an
    /// unknown tenant costs one `stat`.
    slugs_read_at: RwLock<Option<std::time::SystemTime>>,
    /// tenant_id → verifier, built once per tenant.
    verifiers: RwLock<HashMap<String, Arc<JwksVerifier>>>,
}

impl TenantRegistry {
    /// Build the registry, eagerly loading whatever tenants are on disk.
    #[must_use]
    pub fn new(http: reqwest::Client, axiam_url: url::Url, tenant_map_file: String) -> Self {
        let slugs = load_tenant_map(&tenant_map_file);
        let read_at = map_modified_at(&tenant_map_file);
        Self {
            http,
            axiam_url,
            tenant_map_file,
            slugs: RwLock::new(slugs),
            slugs_read_at: RwLock::new(read_at),
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

    /// Resolve a tenant slug, re-reading the map file when it has changed.
    ///
    /// Re-reading at all is what lets the bootstrap add a tenant while the Twin
    /// is already listening — which it does on the very first run, since the
    /// Twin starts before the tenants exist. Re-reading only when the file has
    /// *moved* is what stops a stream of connects naming random tenants from
    /// costing a read and a parse each (T-05-08).
    #[must_use]
    pub fn slug(&self, tenant_id: &str) -> Option<String> {
        if let Ok(map) = self.slugs.read()
            && let Some(slug) = map.get(tenant_id)
        {
            return Some(slug.clone());
        }
        let on_disk = map_modified_at(&self.tenant_map_file);
        if self.slugs_read_at.read().is_ok_and(|seen| *seen == on_disk) {
            // Nothing has changed since the map we already hold. A miss now is
            // a miss, and costs one `stat`.
            return None;
        }
        let fresh = load_tenant_map(&self.tenant_map_file);
        let slug = fresh.get(tenant_id).cloned();
        if let Ok(mut map) = self.slugs.write() {
            *map = fresh;
        }
        if let Ok(mut seen) = self.slugs_read_at.write() {
            *seen = on_disk;
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
    /// **A tenant this Twin does not serve resolves to no verifier at all.**
    /// That refusal belongs here rather than at a call site: falling back to
    /// another tenant's verifier — or building a fresh one for any well-formed
    /// identifier — would make the tenant assertion meaningless, because a
    /// token would succeed as soon as *some* verifier accepted it (T-05-02).
    ///
    /// # Errors
    ///
    /// When the tenant is not one this Twin serves, when `tenant_id` is not a
    /// UUID, or when the verifier cannot be built.
    pub fn verifier(&self, tenant_id: &str) -> Result<Arc<JwksVerifier>> {
        anyhow::ensure!(
            self.knows(tenant_id),
            "tenant is not served by this Twin"
        );
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

/// When the tenant map was last written, or `None` if it is not there yet.
///
/// `None` is a legitimate state, not an error: the Twin starts before the
/// bootstrap has published anything, and the transition from `None` to `Some`
/// is exactly the change that should trigger a re-read.
fn map_modified_at(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
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
