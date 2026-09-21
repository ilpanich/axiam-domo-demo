//! Offline fixtures shared by the three test files.
//!
//! Nothing here reaches the real AXIAM. A `wiremock` server stands in for the
//! organization key-set endpoint and an Ed25519 key generated in-process signs
//! the tokens, so expiry, audience, tenant and algorithm can each be varied
//! independently. Real device tokens live roughly 900 seconds with no
//! per-test override (RESEARCH P-13), which no fast suite can wait out.

#![allow(dead_code)] // each test file uses a different subset

use std::io::Write as _;
use std::sync::{Arc, Mutex};

use actix_web::{App, test, web};
use base64::Engine as _;
use device_twin::rmq::{self, TwinState};
use device_twin::tenants::{SessionCache, TenantRegistry};
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The path the SDK's verifier appends to the configured AXIAM base URL.
pub const JWKS_PATH: &str = "/oauth2/jwks";

/// The audience AXIAM stamps on a device (machine-to-machine) token.
pub const M2M_AUDIENCE: &str = "axiam:m2m";

/// The clock-skew allowance the SDK's verifier applies to `exp` and `nbf`.
/// Named, bounded and not operator-settable (`CLOCK_SKEW_LEEWAY_SECS`).
pub const CLOCK_SKEW_SECS: i64 = 60;

pub const LAKESIDE_ID: &str = "11111111-1111-1111-1111-111111111111";
pub const LAKESIDE_SLUG: &str = "lakeside";
pub const HARBOUR_ID: &str = "22222222-2222-2222-2222-222222222222";
pub const HARBOUR_SLUG: &str = "harbour";

/// A device service account, and another one for the "not you" cases.
pub const SA: &str = "01a0be43-4c1e-4f8f-9c0a-2f1d3b5e7a90";
pub const OTHER_SA: &str = "7f2c9d10-55aa-4b3c-8e21-0d4f6a8b1c33";

/// The client identifier the broker derives from `SA`'s certificate.
#[must_use]
pub fn cn(username: &str) -> String {
    format!("CN={username}")
}

/// Unix seconds, for tests that need a real "now".
#[must_use]
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before the epoch")
        .as_secs() as i64
}

// --- token signing ----------------------------------------------------------

/// An in-process Ed25519 signing key, plus the JWK the fake key set publishes.
pub struct FixtureKey {
    encoding: jsonwebtoken::EncodingKey,
    public_x: String,
    kid: String,
}

impl FixtureKey {
    #[must_use]
    pub fn generate(kid: &str) -> Self {
        let kp = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
            .expect("generating a fixture Ed25519 key");
        let raw = kp.public_key_raw();
        assert_eq!(raw.len(), 32, "Ed25519 public keys are 32 bytes");
        Self {
            encoding: jsonwebtoken::EncodingKey::from_ed_der(&kp.serialize_der()),
            public_x: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw),
            kid: kid.to_owned(),
        }
    }

    /// The single-key JWK set this key publishes.
    #[must_use]
    pub fn jwks(&self) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "use": "sig",
                "alg": "EdDSA",
                "kid": self.kid,
                "x": self.public_x,
            }]
        })
    }

    /// Sign a set of claims with EdDSA and this key's `kid`.
    #[must_use]
    pub fn sign<T: Serialize>(&self, claims: &T) -> String {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        jsonwebtoken::encode(&header, claims, &self.encoding).expect("signing a fixture token")
    }
}

/// A device access token, shaped like AXIAM's own.
#[derive(Debug, Clone, Serialize)]
pub struct TokenClaims {
    pub sub: String,
    pub tenant_id: String,
    pub iss: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
}

impl TokenClaims {
    /// A token that should verify: this tenant, the m2m audience, alive for an
    /// hour.
    #[must_use]
    pub fn device(sub: &str, tenant_id: &str) -> Self {
        let iat = now();
        Self {
            sub: sub.to_owned(),
            tenant_id: tenant_id.to_owned(),
            iss: "https://axiam.test".into(),
            aud: M2M_AUDIENCE.into(),
            exp: iat + 3_600,
            iat,
        }
    }

    #[must_use]
    pub fn expiring_at(mut self, exp: i64) -> Self {
        self.exp = exp;
        self
    }

    #[must_use]
    pub fn with_audience(mut self, aud: &str) -> Self {
        self.aud = aud.to_owned();
        self
    }

    #[must_use]
    pub fn with_tenant(mut self, tenant_id: &str) -> Self {
        self.tenant_id = tenant_id.to_owned();
        self
    }

    #[must_use]
    pub fn with_subject(mut self, sub: &str) -> Self {
        self.sub = sub.to_owned();
        self
    }
}

/// A token signed with HS256 rather than EdDSA — must be rejected on the
/// header alone, before the key set is ever consulted.
#[must_use]
pub fn hs256_token(claims: &TokenClaims) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(b"not the fixture key"),
    )
    .expect("signing an HS256 token")
}

// --- the fake organization key set ------------------------------------------

/// A running fake AXIAM: one key set, served over plain HTTP on localhost.
pub struct FakeAxiam {
    pub server: MockServer,
    pub key: FixtureKey,
}

impl FakeAxiam {
    pub async fn start() -> Self {
        Self::start_with(FixtureKey::generate("fixture-kid")).await
    }

    pub async fn start_with(key: FixtureKey) -> Self {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(key.jwks()))
            .mount(&server)
            .await;
        Self { server, key }
    }

    /// A key set that publishes a *different* key than the one signing, so a
    /// well-formed token has no matching public key.
    pub async fn start_with_foreign_key() -> Self {
        let published = FixtureKey::generate("fixture-kid");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(published.jwks()))
            .mount(&server)
            .await;
        // The caller signs with this one, which the set does not carry.
        Self {
            server,
            key: FixtureKey::generate("fixture-kid"),
        }
    }

    #[must_use]
    pub fn url(&self) -> String {
        self.server.uri()
    }

    /// How many requests the key-set endpoint has received.
    pub async fn jwks_hits(&self) -> usize {
        self.server
            .received_requests()
            .await
            .map(|r| r.iter().filter(|q| q.url.path() == JWKS_PATH).count())
            .unwrap_or(0)
    }
}

// --- state and app builders -------------------------------------------------

/// Write a tenant map (tenant_id → slug) to a unique temporary file.
#[must_use]
pub fn tenant_map_file(entries: &[(&str, &str)]) -> String {
    let map: std::collections::HashMap<&str, &str> = entries.iter().copied().collect();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "domo-twin-tenants-{}-{}.json",
        std::process::id(),
        now_nanos()
    ));
    let mut f = std::fs::File::create(&path).expect("creating the fixture tenant map");
    f.write_all(serde_json::to_string(&map).unwrap().as_bytes())
        .expect("writing the fixture tenant map");
    path.to_string_lossy().into_owned()
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before the epoch")
        .as_nanos()
}

/// A registry pointed at the fake AXIAM, knowing the given tenants.
#[must_use]
pub fn registry(axiam_url: &str, tenants: &[(&str, &str)]) -> Arc<TenantRegistry> {
    Arc::new(TenantRegistry::new(
        reqwest::Client::new(),
        axiam_url.parse().expect("fixture AXIAM URL"),
        tenant_map_file(tenants),
    ))
}

/// A registry that reaches nothing: for the paths that must deny before any
/// token work happens.
#[must_use]
pub fn offline_registry(tenants: &[(&str, &str)]) -> Arc<TenantRegistry> {
    registry("http://127.0.0.1:1", tenants)
}

/// The state one test's requests share. The session cache lives out here, so a
/// CONNECT and the virtual-host call that follows it see the same cache even
/// though each call builds its own application instance.
pub struct Twin {
    pub tenants: Arc<TenantRegistry>,
    pub sessions: Arc<SessionCache>,
}

impl Twin {
    #[must_use]
    pub fn new(tenants: Arc<TenantRegistry>) -> Self {
        Self {
            tenants,
            sessions: Arc::new(SessionCache::new()),
        }
    }

    /// A Twin that can reach no key set at all: for the paths that must deny
    /// before any token work happens.
    #[must_use]
    pub fn offline(tenants: &[(&str, &str)]) -> Self {
        Self::new(offline_registry(tenants))
    }

    /// POST a form to the real routing table and return `(status, body)` —
    /// exactly the two things the broker reads.
    pub async fn post(&self, uri: &str, form: &[(&str, &str)]) -> (u16, String) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(TwinState::new(
                    self.tenants.clone(),
                    self.sessions.clone(),
                )))
                .configure(rmq::configure),
        )
        .await;
        let req = test::TestRequest::post()
            .uri(uri)
            .set_form(form)
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    /// POST a raw urlencoded body — for the malformed cases a typed form
    /// helper cannot express.
    pub async fn post_raw(&self, uri: &str, body: &'static str) -> (u16, String) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(TwinState::new(
                    self.tenants.clone(),
                    self.sessions.clone(),
                )))
                .configure(rmq::configure),
        )
        .await;
        let req = test::TestRequest::post()
            .uri(uri)
            .insert_header(("content-type", "application/x-www-form-urlencoded"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status().as_u16();
        let out = test::read_body(resp).await;
        (status, String::from_utf8_lossy(&out).into_owned())
    }
}

// --- log capture ------------------------------------------------------------

/// A `tracing` writer that keeps everything in memory, so a test can assert
/// what did *not* reach the log.
#[derive(Clone, Default)]
pub struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl CapturedLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log buffer poisoned")).into_owned()
    }

    /// Install as the thread-local subscriber for the duration of the guard.
    #[must_use]
    pub fn install(&self) -> tracing::subscriber::DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(self.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }
}

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer poisoned").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
