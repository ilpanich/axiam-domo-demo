//! Device Twin — Phase 1 slice.
//!
//! In this plan the Twin is exactly one thing: RabbitMQ's HTTP authorization
//! backend, plus a health endpoint. Shadow state, commands and the SSE decision
//! feed are later plans; nothing here is a stub that would need an
//! architectural change to grow into them.
//!
//! This binary is a shell: it reads the environment, builds the TLS listener
//! and the shared state, and hands routing to [`device_twin::rmq::configure`].
//! Every decision lives in the library half, where the test suite can reach it
//! without a server.
//!
//! Listens on `:8443` with TLS 1.3 only (D-07) and is never published outside
//! the compose network (T-01-07).

use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use anyhow::{Context, Result};

use device_twin::rmq::{self, TwinState};
use device_twin::tenants::{SessionCache, TenantRegistry};

fn server_config(cert_path: &str, key_path: &str) -> Result<rustls::ServerConfig> {
    let cert_pem =
        std::fs::read(cert_path).with_context(|| format!("reading TLS certificate {cert_path}"))?;
    let key_pem = std::fs::read(key_path).with_context(|| format!("reading TLS key {key_path}"))?;

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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let bind = std::env::var("DOMO_TWIN_BIND").unwrap_or_else(|_| "0.0.0.0:8443".into());
    let cert = std::env::var("DOMO_TWIN_CERT").unwrap_or_else(|_| "/etc/domo/tls/server.pem".into());
    let key = std::env::var("DOMO_TWIN_KEY").unwrap_or_else(|_| "/etc/domo/tls/server.key".into());
    let root = std::env::var("DOMO_ROOT_CA").unwrap_or_else(|_| "/etc/domo/tls/root.pem".into());
    let axiam =
        std::env::var("DOMO_AXIAM_URL").unwrap_or_else(|_| "https://axiam-server:8090".into());
    let tenant_map_file = std::env::var("DOMO_TENANT_MAP_FILE")
        .unwrap_or_else(|_| "/etc/domo/state/tenants.json".into());

    let root_pem = std::fs::read(&root).with_context(|| format!("reading root CA {root}"))?;
    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(
            reqwest::Certificate::from_pem(&root_pem).context("root CA is not valid PEM")?,
        )
        .build()
        .context("building the key-set HTTP client")?;

    let tenants = Arc::new(TenantRegistry::new(
        http,
        axiam.parse().context("DOMO_AXIAM_URL is not a URL")?,
        tenant_map_file,
    ));
    // One verifier per tenant, built up front. A tenant the bootstrap adds
    // after we start is still picked up lazily on first use.
    let warmed = tenants
        .warm()
        .context("building the per-tenant verifiers at startup")?;
    let sessions = Arc::new(SessionCache::new());

    let tls = server_config(&cert, &key)?;
    tracing::info!(
        %bind,
        tenants = warmed,
        "device-twin listening (TLS 1.3, auth backend only)"
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(TwinState::new(
                tenants.clone(),
                sessions.clone(),
            )))
            .configure(rmq::configure)
    })
    .bind_rustls_0_23(&bind, tls)
    .with_context(|| format!("binding {bind}"))?
    .run()
    .await
    .context("device-twin server error")
}
