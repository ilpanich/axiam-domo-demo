//! `domo-probe` — one simulated device, proving the whole chain.
//!
//! Two phases, matching the two-phase device identity:
//!
//! * `gen-csr` — generate an Ed25519 keypair and a PKCS#10 CSR whose subject is
//!   `CN=<service-account UUID>`. **The private key never leaves this process's
//!   filesystem** (D-23, DEV-05): only the CSR is handed to `domo-bootstrap`.
//! * `connect` — log in to AXIAM over mTLS, then CONNECT to the broker with the
//!   certificate AND the returned JWT, publish, and subscribe.
//!
//! # Why device login is hand-rolled (DF-009)
//!
//! The Rust SDK has no `/api/v1/auth/device` operation. Its `device_login` is
//! the unrelated OAuth 2.0 Device Authorization Grant — the "type this code on
//! another screen" flow — and reaching for it here would be a category error.
//! The C++ SDK has `authenticate_device()`; the Rust one does not.

mod cases;
mod fixtures;
mod matrix;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;

use domo_common::topic::device_prefix;

/// Where `domo-bootstrap`'s `smoke-certs` stage records the cross-tenant
/// issuance answer. Mirrors `stages/smoke/certs.rs`'s `OUTCOME_PATH`; the two
/// tools are separate crates, so the path is the only thing they can share.
pub const CROSS_TENANT_OUTCOME: &str = "smoke/cross-tenant-issuance.json";
/// The forged-common-name leaf, present only when AXIAM agreed to mint one.
pub const FORGED_PEM: &str = "smoke/forged-cn.pem";

const KEY_PATH: &str = "probe/device.key";
const CSR_PATH: &str = "probe/device.csr";
const LEAF_PATH: &str = "probe/leaf.pem";
const SA_ID_PATH: &str = "probe/sa-id";
/// Records which account the on-disk keypair was generated for.
const CSR_FOR_PATH: &str = "probe/csr-for";

#[derive(Parser)]
#[command(name = "domo-probe", about = "Simulated device for the Phase 1 tracer")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate the device keypair and a CSR naming its service account.
    GenCsr,
    /// Device login over mTLS, then MQTT CONNECT, publish and subscribe.
    Connect,
    /// Generate a keypair and CSR for every smoke fixture the bootstrap has
    /// published an account id for.
    ///
    /// Each fixture's private key is generated here and stays here: only the
    /// certificate signing request goes to `domo-bootstrap` (D-23, DEV-05).
    SmokeKeys,
    /// Run the positive and negative device connect matrix (D-25).
    Matrix,
    /// Wait out a real device-token expiry and assert the refusal end to end.
    ///
    /// Costs minutes by construction: AXIAM offers no per-request lifetime
    /// override (P-13), so this is the only honest live form of the case.
    MatrixExpired,
}

/// Generate one keypair and CSR per fixture, discovered from the account ids
/// the `smoke` bootstrap stage published.
///
/// Idempotent per account, not per file: a fixture whose account was rebuilt
/// gets a fresh key, because a certificate bound to the OLD key would be an
/// mTLS identity whose halves disagree — and that failure surfaces far from
/// here, as "building the device HTTP client".
fn smoke_keys() -> Result<()> {
    let dir = domo_common::secrets::path("smoke")
        .context("resolving the smoke fixture directory")?;
    if !dir.exists() {
        bail!("no smoke fixtures on disk — run `just smoke-tree` first");
    }

    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "sa-id")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            names.push(stem.to_owned());
        }
    }
    names.sort();
    anyhow::ensure!(!names.is_empty(), "no fixture account ids found under {}", dir.display());

    for name in &names {
        let sa_id = domo_common::secrets::read_string(format!("smoke/{name}.sa-id"))?;
        let key_path = format!("smoke/{name}.key");
        let csr_path = format!("smoke/{name}.csr");
        let for_path = format!("smoke/{name}.csr-for");

        if domo_common::secrets::exists(&key_path)
            && domo_common::secrets::exists(&csr_path)
            && domo_common::secrets::read_string(&for_path).ok().as_deref() == Some(&sa_id)
        {
            println!("  ✓ {name}: keypair and CSR already exist for CN={sa_id}");
            continue;
        }

        println!("  → {name}: generating an Ed25519 keypair (the key never leaves this host)");
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
            .with_context(|| format!("generating the keypair for '{name}'"))?;
        let mut params =
            rcgen::CertificateParams::new(Vec::<String>::new()).context("building CSR params")?;
        let mut dn = rcgen::DistinguishedName::new();
        // The subject IS the identity: the broker derives `client_id` from
        // this DN and the Twin requires `client_id == "CN=" + username`.
        dn.push(rcgen::DnType::CommonName, sa_id.clone());
        params.distinguished_name = dn;
        let csr = params
            .serialize_request(&key)
            .context("serializing the CSR")?;

        domo_common::secrets::write_string(&key_path, &key.serialize_pem())?;
        domo_common::secrets::write_string(&csr_path, &csr.pem().context("encoding the CSR")?)?;
        domo_common::secrets::write_string(&for_path, &sa_id)?;
        println!("  ✓ {name}: CSR ready for CN={sa_id}");
    }
    Ok(())
}

/// `POST /api/v1/auth/device` response.
#[derive(Debug, Deserialize)]
struct DeviceAuth {
    access_token: String,
    token_type: String,
    #[serde(default)]
    #[allow(dead_code)]
    expires_in: Option<i64>,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn gen_csr() -> Result<()> {
    let sa_id = domo_common::secrets::read_string(SA_ID_PATH)
        .context("no device account id on disk — run the device-account stage first")?;

    // Idempotent per account. Minting a fresh keypair on every run would
    // silently break the second run: `device-identity` reuses the certificate
    // already bound to this account, and that certificate belongs to the OLD
    // key — so the mTLS identity would no longer be a matching pair. A new key
    // is generated only when the account itself is new.
    if domo_common::secrets::exists(KEY_PATH)
        && domo_common::secrets::exists(CSR_PATH)
        && domo_common::secrets::read_string(CSR_FOR_PATH).ok().as_deref() == Some(&sa_id)
    {
        println!("  ✓ keypair and CSR already exist for CN={sa_id} — reusing");
        return Ok(());
    }

    println!("  → generating an Ed25519 keypair (the key never leaves this host)");
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
        .context("generating the device keypair")?;

    let mut params =
        rcgen::CertificateParams::new(Vec::<String>::new()).context("building CSR params")?;
    let mut dn = rcgen::DistinguishedName::new();
    // The subject IS the identity: the broker derives `client_id` from this DN
    // and the Twin requires `client_id == "CN=" + username`.
    dn.push(rcgen::DnType::CommonName, sa_id.clone());
    params.distinguished_name = dn;

    let csr = params
        .serialize_request(&key)
        .context("serializing the CSR")?;

    // PKCS#8. Written at 0600 by the secrets helper.
    domo_common::secrets::write_string(KEY_PATH, &key.serialize_pem())?;
    domo_common::secrets::write_string(CSR_PATH, &csr.pem().context("encoding the CSR")?)?;
    domo_common::secrets::write_string(CSR_FOR_PATH, &sa_id)?;
    println!("  ✓ CSR ready for CN={sa_id}");
    Ok(())
}

async fn connect() -> Result<()> {
    let axiam = env_or("DOMO_AXIAM_URL", "https://axiam-server:8090");
    let mqtt_host = env_or("DOMO_MQTT_HOST", "rabbitmq");
    let mqtt_port: u16 = env_or("DOMO_MQTT_PORT", "8883")
        .parse()
        .context("DOMO_MQTT_PORT is not a port number")?;
    let tenant = env_or("DOMO_TENANT_SLUG", "lakeside");
    let root_path = env_or("DOMO_ROOT_CA", "/etc/domo/pki/root.pem");

    let sa_id = domo_common::secrets::read_string(SA_ID_PATH)?;
    let root_pem = std::fs::read(&root_path)
        .with_context(|| format!("reading the organization root at {root_path}"))?;
    let leaf_pem = domo_common::secrets::read(LEAF_PATH)
        .context("no issued leaf on disk — run the device-identity stage first")?;
    let key_pem = domo_common::secrets::read(KEY_PATH)?;
    let ca_pem = domo_common::secrets::read(format!("axiam/{tenant}-ca.pem"))
        .with_context(|| format!("no signing CA for '{tenant}' — run the pki stage first"))?;

    // The chain the server must see: leaf first, then the tenant signing CA.
    // Without the intermediate, AXIAM can anchor the leaf only if it happens to
    // hold the CA already — and the broker certainly cannot.
    let mut chain = Vec::new();
    chain.extend_from_slice(&leaf_pem);
    chain.push(b'\n');
    chain.extend_from_slice(&ca_pem);

    // --- device login over mTLS -------------------------------------------
    println!("  → device login over mTLS at {axiam}");
    let mut identity_pem = chain.clone();
    identity_pem.push(b'\n');
    identity_pem.extend_from_slice(&key_pem);
    let identity = reqwest::Identity::from_pem(&identity_pem)
        .context("building the mTLS identity (leaf + CA + PKCS#8 key)")?;

    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(
            reqwest::Certificate::from_pem(&root_pem).context("root CA is not valid PEM")?,
        )
        .identity(identity)
        .build()
        .context("building the device HTTP client")?;

    let resp = http
        .post(format!("{}/api/v1/auth/device", axiam.trim_end_matches('/')))
        .send()
        .await
        .context("POST /api/v1/auth/device")?;
    let status = resp.status();
    if !status.is_success() {
        bail!("device login returned {status}");
    }
    let auth: DeviceAuth = resp.json().await.context("decoding the device token")?;
    if auth.token_type != "Bearer" {
        bail!("unexpected token_type '{}'", auth.token_type);
    }
    if auth.access_token.is_empty() {
        bail!("device login returned an empty access_token");
    }
    // Never print the token.
    println!("  ✓ device authenticated (Bearer token received)");

    // --- MQTT CONNECT ------------------------------------------------------
    let client_id = format!("CN={sa_id}");
    println!("  → MQTT CONNECT to {mqtt_host}:{mqtt_port} as {client_id}");

    let tls = domo_common::tls::shared_client_config_with_identity(&root_pem, &chain, &key_pem)?;
    let mut opts = rumqttc::MqttOptions::new(client_id, &mqtt_host, mqtt_port);
    // username = the account UUID, password = the JWT. The Twin checks that
    // both name the same account as the certificate.
    opts.set_credentials(sa_id.clone(), auth.access_token.clone());
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_transport(rumqttc::Transport::tls_with_config(
        rumqttc::TlsConfiguration::Rustls(Arc::new(
            (*tls).clone(),
        )),
    ));

    let prefix = device_prefix(&tenant, &sa_id);
    let topic = format!("{prefix}/reported");
    let filter = format!("{prefix}/#");

    let (client, mut eventloop) = rumqttc::AsyncClient::new(opts, 10);

    let payload = format!(r#"{{"probe":"{sa_id}","state":"online"}}"#);
    let mut subscribed = false;
    let mut published = false;
    let mut round_tripped = false;

    // One event loop, driven to the round trip or to a timeout. Every failure
    // path surfaces the broker's own reason rather than a generic message.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline && !round_tripped {
        let event = tokio::time::timeout_at(deadline, eventloop.poll()).await;
        let event = match event {
            Err(_) => break,
            Ok(Ok(e)) => e,
            Ok(Err(e)) => {
                // A CONNACK refusal or a TLS alert lands here. These are the two
                // signals the plan's assumptions A1 and A2 are about.
                bail!("MQTT connection failed: {e}");
            }
        };

        use rumqttc::{Event, Packet};
        match event {
            Event::Incoming(Packet::ConnAck(ack)) => {
                println!("  ✓ CONNACK: {:?}", ack.code);
                client
                    .subscribe(&filter, rumqttc::QoS::AtLeastOnce)
                    .await
                    .context("subscribing")?;
            }
            Event::Incoming(Packet::SubAck(_)) => {
                subscribed = true;
                println!("  ✓ subscribed to {filter}");
                client
                    .publish(&topic, rumqttc::QoS::AtLeastOnce, false, payload.clone())
                    .await
                    .context("publishing")?;
            }
            Event::Incoming(Packet::PubAck(_)) => {
                published = true;
                println!("  ✓ published to {topic}");
            }
            Event::Incoming(Packet::Publish(p)) if p.topic == topic => {
                round_tripped = true;
                println!("  ✓ round trip observed on {}", p.topic);
            }
            _ => {}
        }
    }

    if !subscribed {
        bail!("never received a SubAck — the broker refused the subscription");
    }
    // The round trip is strictly stronger evidence than the PubAck: a message
    // that came back was, necessarily, published. The broker may deliver the
    // subscribed copy BEFORE the acknowledgement, so requiring the PubAck first
    // would fail a run that in fact proved more than the PubAck ever could.
    if !round_tripped {
        if published {
            bail!("publish was acknowledged, but the message never came back");
        }
        bail!("never received a PubAck — the broker refused the publish");
    }

    client.disconnect().await.ok();
    println!("  ✓ probe complete: cert -> client_id -> username -> sub all agree");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // P-10: before any TLS configuration exists.
    domo_common::tls::install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    match Cli::parse().cmd {
        Cmd::GenCsr => gen_csr(),
        Cmd::Connect => connect().await,
        Cmd::SmokeKeys => smoke_keys(),
        Cmd::Matrix => matrix::run().await,
        Cmd::MatrixExpired => matrix::run_expired().await,
    }
}
