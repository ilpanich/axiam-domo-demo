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
//!
//! The D-34 stage markers (`.secrets/state/<stage>`, no suffix) are the same
//! kind of hint one level up, and `state::probe` is what stops them being
//! believed. See [`domo_bootstrap::state`].

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use domo_bootstrap::{checklist, stages, state};

#[derive(Parser)]
#[command(name = "domo-bootstrap", about = "Provision AXIAM for the Domo demo")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// Provision each tenant's admin user — the principal every tenant-scoped
    /// stage logs in as (D-37).
    TenantAdmin,
    /// Issue one service account and certificate per (service, tenant) (D-20).
    ServiceCerts,
    /// Create each tenant's `portfolio` root and its structural group (D-18).
    Tree,
    /// Assert every Phase 1 authorization invariant.
    AuthzVerify,
    /// Build the one reserved-prefix branch of the tree, with its structural
    /// groups and the probe's device accounts (D-18).
    Smoke,
    /// Assert the authorization model against live AXIAM over the smoke branch
    /// (AUTHZ-01, AUTHZ-02).
    SmokeVerify,
    /// Sign every smoke fixture's certificate request, and attempt the
    /// cross-tenant issuance the matrix records either way (PKI-03, DF-017).
    SmokeCerts,
    /// Remove every reserved-prefix fixture, and nothing else.
    ///
    /// This plan's clean-state mechanism: `just demo-reset` arrives with plan
    /// 01-07, which depends on this one, so nothing here may call it.
    SmokeTeardown,
    /// Run one D-34 stage of the staged, resumable checklist: gate on
    /// `DOMO_FAIL_AT`, do the work, write `.secrets/state/<stage>`.
    ///
    /// `just` runs the stages it owns itself (the Docker lifecycle); this is
    /// the AXIAM-facing half of the same sequence.
    StageRun {
        /// A D-34 stage name: org-bootstrap, tenants, catalog, service-certs
        /// or broker.
        name: String,
    },
    /// Ask whether a stage's work is genuinely already there. Read-only.
    ///
    /// Exit 0 when done, 1 when not. This is the re-probe that makes
    /// `.secrets/state/<stage>` a skip hint rather than a claim: a marker left
    /// behind by a run that died before doing the work does not survive it.
    StageProbe {
        /// A D-34 stage name.
        name: String,
    },
    /// Print where this machine stands, one line per stage, from the markers.
    Checklist,
    /// Print the demo card and write it to `.secrets/demo-card.txt` (D-36).
    ///
    /// `just` supplies the two values only the host knows: the operator's
    /// configured host name, and the root fingerprint from the export that has
    /// already been checked against the in-use root.
    DemoCard {
        /// The portal host name; the console is `axiam.<host>`.
        #[arg(long)]
        host: String,
        /// The organization root's SHA-256 fingerprint.
        #[arg(long)]
        fingerprint: String,
    },
    /// Apply `authz/catalog.toml` to one tenant (D-16).
    Catalog {
        /// Tenant slug to apply the catalog to.
        #[arg(long)]
        tenant: String,
        /// Report what would change and write nothing.
        #[arg(long)]
        plan_only: bool,
    },
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
    match cli.command {
        Command::OrgBootstrap { setup_token } => {
            stages::org_bootstrap::run(setup_token.as_deref()).await
        }
        Command::Tenants => stages::tenants::run().await,
        Command::Pki => stages::pki::run().await,
        Command::DeviceAccount { name, tenant } => {
            stages::device_identity::ensure_account(&name, &tenant)
                .await
                .map(|_| ())
        }
        Command::DeviceIdentity { csr, out, tenant } => {
            stages::device_identity::sign(&csr, &out, &tenant).await
        }
        Command::Broker => stages::broker::run().await,
        Command::TenantAdmin => stages::tenant_admin::run().await,
        Command::ServiceCerts => stages::service_certs::run().await,
        Command::Tree => stages::tree::run().await,
        Command::AuthzVerify => stages::verify_all().await,
        Command::Smoke => stages::smoke::run().await,
        Command::SmokeVerify => stages::smoke::assertions::run().await,
        Command::SmokeCerts => stages::smoke::certs::run().await,
        Command::SmokeTeardown => stages::smoke::teardown::run().await,
        Command::Catalog { tenant, plan_only } => {
            stages::catalog::run(&tenant, plan_only).await
        }
        Command::StageRun { name } => {
            let stage = resolve(&name)?;
            checklist::running(stage);
            match state::run(stage).await {
                Ok(()) => {
                    let produced = state::marker(stage)
                        .map_or_else(String::new, |m| m.produced);
                    checklist::succeeded(stage, &produced);
                    Ok(())
                }
                Err(e) => {
                    // Print the checklist form, then propagate: `just` needs a
                    // non-zero exit to stop the sequence and tail the log.
                    checklist::failed(stage, &format!("{e:#}"));
                    Err(e)
                }
            }
        }
        Command::StageProbe { name } => {
            let stage = resolve(&name)?;
            // Exit status IS the answer: `just` reads it and nothing else.
            if state::probe(stage).await? {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
        Command::Checklist => checklist::run(),
        Command::DemoCard { host, fingerprint } => checklist::demo_card(&host, &fingerprint),
    }
}

/// Resolve a D-34 stage name, listing the valid ones when it is not.
fn resolve(name: &str) -> Result<state::Stage> {
    state::Stage::from_name(name).with_context(|| {
        let all: Vec<&str> = state::Stage::ALL.iter().map(|s| s.name()).collect();
        format!("'{name}' is not a stage. The sequence is: {}", all.join(" → "))
    })
}
