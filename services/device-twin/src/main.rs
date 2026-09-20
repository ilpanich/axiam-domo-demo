//! Device Twin — Phase 1 slice.
//!
//! In this plan the Twin is exactly one thing: RabbitMQ's HTTP authorization
//! backend, plus a health endpoint. Shadow state, commands and the SSE decision
//! feed are later plans; nothing here is a stub that would need an
//! architectural change to grow into them.
//!
//! Listens on `:8443` with TLS 1.3 only (D-07) and is never published outside
//! the compose network (T-01-07).

mod rmq;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use anyhow::{Context, Result};
use axiam_sdk::token::JwksVerifier;

use rmq::{
    Decision, ResourceReq, Session, SessionCache, TopicReq, UserFacts, UserReq, VhostReq,
    decide_resource, decide_topic, decide_user, decide_vhost, peek_tenant_id,
};

/// The audience AXIAM stamps on a device (machine-to-machine) token.
const M2M_AUDIENCE: &str = "axiam:m2m";

struct AppState {
    http: reqwest::Client,
    axiam_url: url::Url,
    tenant_map_file: String,
    /// tenant_id → slug, refreshed from disk on a miss.
    tenants: RwLock<HashMap<String, String>>,
    /// tenant_id → verifier, built once per tenant.
    verifiers: RwLock<HashMap<String, Arc<JwksVerifier>>>,
    /// username (service-account UUID) → session.
    sessions: RwLock<SessionCache>,
}

impl AppState {
    /// Resolve a tenant slug, re-reading the map file on a miss.
    ///
    /// Re-reading rather than caching-forever is what lets plans 01-02 onward
    /// add tenants without restarting the Twin.
    fn tenant_slug(&self, tenant_id: &str) -> Option<String> {
        if let Ok(map) = self.tenants.read()
            && let Some(slug) = map.get(tenant_id)
        {
            return Some(slug.clone());
        }
        let fresh = load_tenant_map(&self.tenant_map_file);
        let slug = fresh.get(tenant_id).cloned();
        if let Ok(mut map) = self.tenants.write() {
            *map = fresh;
        }
        slug
    }

    /// A JWKS verifier pinned to one tenant and to the m2m audience.
    ///
    /// `expect_tenant_id` is not optional: AXIAM's JWKS endpoint is
    /// organization-wide, so a verifier without it proves only "signed for
    /// *some* tenant in this organization" — which is not tenant isolation.
    fn verifier(&self, tenant_id: &str) -> Result<Arc<JwksVerifier>> {
        if let Ok(v) = self.verifiers.read()
            && let Some(found) = v.get(tenant_id)
        {
            return Ok(found.clone());
        }
        let uuid: uuid::Uuid = tenant_id.parse().context("tenant_id claim is not a UUID")?;
        let built = Arc::new(
            JwksVerifier::new(self.http.clone(), &self.axiam_url)
                .context("building the JWKS verifier")?
                .expect_tenant_id(uuid)
                .expect_audience(M2M_AUDIENCE),
        );
        if let Ok(mut v) = self.verifiers.write() {
            v.insert(tenant_id.to_owned(), built.clone());
        }
        Ok(built)
    }

    fn session(&self, username: &str) -> Option<Session> {
        self.sessions.read().ok()?.get(username).cloned()
    }
}

fn load_tenant_map(path: &str) -> HashMap<String, String> {
    // A missing or malformed file is not fatal: it means no tenant resolves,
    // so every topic decision denies. Failing closed beats failing loudly.
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(&s).ok())
        .unwrap_or_default()
}

fn reply(d: &Decision) -> HttpResponse {
    if let Decision::Deny(reason) = d {
        // The reason is for our operator, never for the wire.
        tracing::debug!(reason, "denied");
    }
    HttpResponse::Ok().content_type("text/plain").body(d.body())
}

async fn healthz() -> impl Responder {
    HttpResponse::Ok().content_type("text/plain").body("ok")
}

/// CONNECT. The only endpoint that verifies a token; the rest read its result.
async fn rmq_user(form: web::Form<UserReq>, st: web::Data<AppState>) -> impl Responder {
    let r = form.into_inner();

    // Pre-flight the cheap, purely local hops before spending a JWKS fetch on a
    // request that cannot succeed anyway.
    let prelim = decide_user(&UserFacts {
        username: &r.username,
        vhost: r.vhost.as_deref(),
        client_id: r.client_id.as_deref(),
        jwt_sub: Some(&r.username), // provisional: hop 4 is checked for real below
    });
    if !prelim.is_allow() {
        return reply(&prelim);
    }

    // Choose the verifier from the token's unverified tenant claim, then let
    // that verifier re-check the claim it was chosen by.
    let Some(tenant_id) = peek_tenant_id(&r.password) else {
        return reply(&Decision::Deny("token carries no tenant_id claim"));
    };
    let verifier = match st.verifier(&tenant_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "could not build a verifier");
            return reply(&Decision::Deny("verifier unavailable"));
        }
    };

    // Never log `r.password` — it is the access token (T-01-04).
    let claims = match verifier.verify(&r.password).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "token verification failed");
            return reply(&Decision::Deny("token did not verify"));
        }
    };

    let decision = decide_user(&UserFacts {
        username: &r.username,
        vhost: r.vhost.as_deref(),
        client_id: r.client_id.as_deref(),
        jwt_sub: Some(&claims.sub),
    });

    if decision.is_allow() {
        let Some(slug) = st.tenant_slug(&claims.tenant_id) else {
            return reply(&Decision::Deny("tenant is not known to this Twin"));
        };
        if let Ok(mut s) = st.sessions.write() {
            s.insert(
                r.username.clone(),
                Session {
                    tenant_id: claims.tenant_id.clone(),
                    tenant_slug: slug,
                    exp: claims.exp,
                },
            );
        }
        tracing::info!(account = %r.username, tenant = %claims.tenant_id, "CONNECT allowed");
    }
    reply(&decision)
}

async fn rmq_vhost(form: web::Form<VhostReq>, st: web::Data<AppState>) -> impl Responder {
    let r = form.into_inner();
    reply(&decide_vhost(&r.vhost, st.session(&r.username).as_ref()))
}

async fn rmq_resource(form: web::Form<ResourceReq>, st: web::Data<AppState>) -> impl Responder {
    let r = form.into_inner();
    reply(&decide_resource(
        &r.vhost,
        &r.username,
        &r.resource,
        &r.name,
        st.session(&r.username).as_ref(),
    ))
}

async fn rmq_topic(form: web::Form<TopicReq>, st: web::Data<AppState>) -> impl Responder {
    let r = form.into_inner();
    reply(&decide_topic(
        &r.vhost,
        &r.username,
        &r.routing_key,
        st.session(&r.username).as_ref(),
    ))
}

fn server_config(cert_path: &str, key_path: &str) -> Result<rustls::ServerConfig> {
    let cert_pem = std::fs::read(cert_path)
        .with_context(|| format!("reading TLS certificate {cert_path}"))?;
    let key_pem =
        std::fs::read(key_path).with_context(|| format!("reading TLS key {key_path}"))?;

    let certs = rustls_pemfile::certs(&mut &cert_pem[..])
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("malformed TLS certificate PEM")?;
    let key = rustls_pemfile::private_key(&mut &key_pem[..])
        .context("malformed TLS key PEM")?
        .context("no private key in the TLS key file")?;

    // D-07: TLS 1.3 only.
    rustls::ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("rustls rejected the Twin's certificate/key pair")
}

#[actix_web::main]
async fn main() -> Result<()> {
    // P-10: must be the first statement, before any TLS config is built.
    domo_common::tls::install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let bind = std::env::var("DOMO_TWIN_BIND").unwrap_or_else(|_| "0.0.0.0:8443".into());
    let cert = std::env::var("DOMO_TWIN_CERT").unwrap_or_else(|_| "/etc/domo/tls/server.pem".into());
    let key = std::env::var("DOMO_TWIN_KEY").unwrap_or_else(|_| "/etc/domo/tls/server.key".into());
    let root = std::env::var("DOMO_ROOT_CA").unwrap_or_else(|_| "/etc/domo/tls/root.pem".into());
    let axiam = std::env::var("DOMO_AXIAM_URL")
        .unwrap_or_else(|_| "https://axiam-server:8090".into());
    let tenant_map_file = std::env::var("DOMO_TENANT_MAP_FILE")
        .unwrap_or_else(|_| "/etc/domo/state/tenants.json".into());

    let root_pem = std::fs::read(&root).with_context(|| format!("reading root CA {root}"))?;
    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(
            reqwest::Certificate::from_pem(&root_pem).context("root CA is not valid PEM")?,
        )
        .build()
        .context("building the JWKS HTTP client")?;

    let state = web::Data::new(AppState {
        http,
        axiam_url: axiam.parse().context("DOMO_AXIAM_URL is not a URL")?,
        tenants: RwLock::new(load_tenant_map(&tenant_map_file)),
        tenant_map_file,
        verifiers: RwLock::new(HashMap::new()),
        sessions: RwLock::new(HashMap::new()),
    });

    let tls = server_config(&cert, &key)?;
    tracing::info!(%bind, "device-twin listening (TLS 1.3, auth backend only)");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/healthz", web::get().to(healthz))
            .route("/rmq/user", web::post().to(rmq_user))
            .route("/rmq/vhost", web::post().to(rmq_vhost))
            .route("/rmq/resource", web::post().to(rmq_resource))
            .route("/rmq/topic", web::post().to(rmq_topic))
    })
    .bind_rustls_0_23(&bind, tls)
    .with_context(|| format!("binding {bind}"))?
    .run()
    .await
    .context("device-twin server error")
}
