//! `domo-bootstrap` — the staged, resumable AXIAM provisioner.
//!
//! Runs as a one-shot compose service (`docker compose run --rm
//! domo-bootstrap <stage>`) so the API work happens on the compose network and
//! the demo machine needs no cargo toolchain (D-35).
//!
//! # Idempotency is by probe, not by marker
//!
//! Every stage resolves its objects by natural key first — tenant by slug,
//! service account by name, CA by fingerprint — and creates only what is
//! genuinely absent. `.secrets/state/<stage>.done` is a *skip hint*, never the
//! source of truth: delete every marker and a re-run still converges to the
//! same objects instead of duplicating them (P-8).

use anyhow::Result;
use clap::{Parser, Subcommand};
use domo_bootstrap::stages;

#[derive(Parser)]
#[command(name = "domo-bootstrap", about = "Provision AXIAM for the Domo demo")]
struct Cli {
    #[command(subcommand)]
    stage: Stage,
}

#[derive(Subcommand)]
enum Stage {
    /// Create the organization and its super-admin (first run only).
    OrgBootstrap {
        /// Setup token scraped from the axiam-server log by `just`.
        #[arg(long)]
        setup_token: Option<String>,
    },
    /// Create the demo tenants. Phase 1 tracer: Lakeside only.
    Tenants,
    /// Import the offline root (BYOK), anchor it, and mint tenant signing CAs.
    Pki,
    /// Phase 1 of device identity: create the device's service account.
    ///
    /// Separate from `device-identity` because the CSR's subject must be
    /// `CN=<service-account UUID>`, so the account has to exist before the
    /// device can generate the CSR that names it.
    DeviceAccount {
        /// Service-account name (the natural key this stage resolves by).
        #[arg(long)]
        name: String,
        /// Tenant that owns the account.
        #[arg(long, default_value = "lakeside")]
        tenant: String,
    },
    /// Phase 2 of device identity: sign a device CSR and bind the result.
    DeviceIdentity {
        /// Path to a PEM PKCS#10 CSR.
        #[arg(long)]
        csr: String,
        /// Where to write the issued leaf certificate.
        #[arg(long)]
        out: String,
        /// Tenant whose signing CA must issue the certificate.
        #[arg(long, default_value = "lakeside")]
        tenant: String,
    },
    /// Create the `domo` MQTT vhost, touching no other vhost.
    Broker,
}

#[tokio::main]
async fn main() -> Result<()> {
    // P-10: install the rustls provider before any TLS configuration exists.
    domo_common::tls::install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.stage {
        Stage::OrgBootstrap { setup_token } => {
            stages::org_bootstrap::run(setup_token.as_deref()).await
        }
        Stage::Tenants => stages::tenants::run().await,
        Stage::Pki => stages::pki::run().await,
        Stage::DeviceAccount { name, tenant } => {
            stages::device_identity::ensure_account(&name, &tenant)
                .await
                .map(|_| ())
        }
        Stage::DeviceIdentity { csr, out, tenant } => {
            stages::device_identity::sign(&csr, &out, &tenant).await
        }
        Stage::Broker => stages::broker::run().await,
    }
}
