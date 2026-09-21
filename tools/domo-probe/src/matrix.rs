//! Runs the connect matrix and reports one line per case.
//!
//! # What "observed" means here
//!
//! Every attempt is classified by the *layer* that refused it, not by whether
//! it failed. A TLS alert before any CONNACK exists is a different fact from a
//! CONNACK carrying a refusal, and both are different from a connection that
//! was accepted and then closed when the device published somewhere it does
//! not own. Collapsing the three into "error" is what lets a broker with its
//! peer-certificate requirement switched off look exactly like one that has it
//! on (T-06-02).
//!
//! # No case is skipped
//!
//! A case that cannot run reports a failure, never a silent pass, and the
//! runner prints a count at the end so a shortened matrix is visible in the
//! output rather than only in a diff (T-06-09).
//!
//! # Nothing here prints a credential
//!
//! Tokens are read, mutated and sent; they are never logged, and no failure
//! message interpolates one. Inherited from plan 01-01's logging discipline
//! (T-06-08).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rumqttc::{ConnectReturnCode, ConnectionError, Event, Packet, QoS};
use domo_common::topic::device_prefix;

use crate::cases::{self, Credential, Expect, Password, Probe, Target};
use crate::fixtures::{
    DeviceAuth, Fixture, device_login, device_login_status, device_login_token, env_or,
};

/// How long a single attempt is given before it is called a timeout.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(20);

/// Margin added on top of a token's own lifetime before the slow case retries.
///
/// The SDK's verifier applies a named, non-configurable 60-second clock-skew
/// allowance to `exp` (plan 01-05 asserts it from both sides), so waiting only
/// until `expires_in` would test nothing: the token is still accepted. This is
/// that allowance plus a little.
const EXPIRY_MARGIN: Duration = Duration::from_secs(90);

/// What actually happened, classified by the layer that decided it.
#[derive(Debug)]
enum Observed {
    RoundTripped,
    ConnectedButNoRoundTrip,
    Handshake(String),
    Refused(ConnectReturnCode),
    ClosedAfterConnect(String),
    Timeout,
    LoginStatus(u16),
    /// AXIAM refused a request naming another tenant's signing CA.
    IssuanceRefused(String),
    /// AXIAM minted it. `detail` names the certificate and the tenant it was
    /// stamped with, which is the evidence the finding needs.
    IssuanceAccepted(String),
}

impl Observed {
    fn describe(&self) -> String {
        match self {
            Self::RoundTripped => "connected, published, subscribed, message came back".into(),
            Self::ConnectedButNoRoundTrip => "connected but the message never came back".into(),
            Self::Handshake(e) => format!("TLS handshake failure ({e})"),
            Self::Refused(code) => format!("CONNACK refusal: {code:?}"),
            Self::ClosedAfterConnect(e) => {
                format!("connection closed after CONNACK ({e})")
            }
            Self::Timeout => "no decision within the attempt timeout".into(),
            Self::LoginStatus(s) => format!("device login returned HTTP {s}"),
            Self::IssuanceRefused(d) => format!("AXIAM refused the request ({d})"),
            Self::IssuanceAccepted(d) => {
                format!("AXIAM ISSUED the certificate — DF-017 confirmed: {d}")
            }
        }
    }

    /// Whether this outcome satisfies the case's expectation.
    fn satisfies(&self, expect: Expect) -> bool {
        match (expect, self) {
            (Expect::Connected, Self::RoundTripped) => true,
            (Expect::HandshakeFailed, Self::Handshake(_)) => true,
            // `BadClientId` is the broker's own `RC_CLIENT_IDENTIFIER_NOT_VALID`
            // — the certificate-subject check firing before the backend is
            // asked anything.
            (Expect::BrokerRejectedIdentity, Self::Refused(ConnectReturnCode::BadClientId)) => true,
            (
                Expect::AuthorizationDenied,
                Self::Refused(
                    ConnectReturnCode::NotAuthorized | ConnectReturnCode::BadUserNamePassword,
                ),
            ) => true,
            // The broker accepts the CONNECT and then closes the connection
            // when the topic check refuses the publish. No PUBACK ever comes.
            (Expect::PublishRefused, Self::ClosedAfterConnect(_) | Self::Timeout) => true,
            // 401 was the predicted status; AXIAM answers 403 for a
            // certificate that is valid TLS material but bound to no account.
            // Both are accepted, and nothing else is: a 200 still fails the
            // case, which is the property being asserted. The observed value
            // is recorded in this plan's summary rather than quietly replaced.
            (Expect::LoginUnauthorized, Self::LoginStatus(401 | 403)) => true,
            (Expect::IssuanceRefused, Self::IssuanceRefused(_)) => true,
            _ => false,
        }
    }
}

/// Everything one MQTT attempt needs.
struct Attempt {
    client_id: String,
    username: String,
    password: String,
    /// Empty when the case presents no client certificate at all.
    chain: Vec<u8>,
    key: Vec<u8>,
    subscribe: String,
    publish: String,
    /// True when the publish is expected to land in the device's own
    /// namespace, so a round trip is the success signal.
    expect_round_trip: bool,
}

/// Run every case and exit non-zero if any observed outcome differs from its
/// expectation.
pub async fn run() -> Result<()> {
    let root_path = env_or("DOMO_ROOT_CA", "/etc/domo/pki/root.pem");
    let root_pem = std::fs::read(&root_path)
        .with_context(|| format!("reading the organization root at {root_path}"))?;

    let probe = Fixture::load(cases::PROBE)?;
    let probe_token = device_login_token(&root_pem, &probe).await?;
    let other = Fixture::load(cases::OTHER)?;
    let other_token = device_login_token(&root_pem, &other).await?;

    let mut passed = 0usize;
    let mut failed = 0usize;

    for case in cases::all() {
        let observed = match case.probe {
            Probe::DeviceLogin { fixture } => {
                let f = Fixture::load(fixture)?;
                Observed::LoginStatus(device_login_status(&root_pem, &f).await?)
            }
            Probe::CrossTenantIssuance => cross_tenant_outcome()?,
            Probe::Connect {
                credential,
                password,
                target,
            } => {
                let attempt = build_attempt(
                    &probe,
                    credential,
                    password,
                    target,
                    &probe_token,
                    &other_token,
                )?;
                attempt_connect(&root_pem, &attempt).await?
            }
        };

        if observed.satisfies(case.expect) {
            println!("✓ {}  {}", case.name, observed.describe());
            passed += 1;
        } else {
            println!(
                "✗ {}  expected {}, got {}",
                case.name,
                case.expect.describe(),
                observed.describe()
            );
            println!("    this case failing would allow {}", case.why);
            failed += 1;
        }
    }

    // A count, so a matrix that quietly lost cases is visible in the output
    // rather than only in a diff (T-06-09).
    println!("  matrix: {passed} passed, {failed} failed, {} cases", passed + failed);
    if failed > 0 {
        bail!("{failed} matrix case(s) did not behave as expected");
    }
    println!("✓ smoke-matrix");
    Ok(())
}

/// Read the answer `smoke-certs` recorded for the cross-tenant experiment.
///
/// A missing file is a failure, not a skip: a case that did not run must never
/// be indistinguishable from one that passed (T-06-09).
fn cross_tenant_outcome() -> Result<Observed> {
    let raw = domo_common::secrets::read(super::CROSS_TENANT_OUTCOME)
        .context("no cross-tenant issuance outcome on disk — run `just smoke-certs`")?;
    let v: serde_json::Value =
        serde_json::from_slice(&raw).context("the recorded outcome is not valid JSON")?;

    if v.get("accepted").and_then(serde_json::Value::as_bool) == Some(true) {
        let acting = format!(
            "{} ({})",
            field(&v, "acting_tenant"),
            field(&v, "acting_tenant_id")
        );
        Ok(Observed::IssuanceAccepted(format!(
            "certificate {} stamped with tenant {}, issued by CA {} on behalf of '{acting}'",
            field(&v, "certificate_id"),
            field(&v, "certificate_tenant_id"),
            field(&v, "issuer_ca_id"),
        )))
    } else {
        Ok(Observed::IssuanceRefused(field(&v, "detail")))
    }
}

fn field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

/// The one case the fast suite cannot cover: a genuinely expired token.
///
/// AXIAM issues device tokens with a fixed lifetime and offers no per-request
/// override (P-13), so the only honest way to observe a real expiry end to end
/// is to wait it out. Expiry is covered at unit level in plan 01-05; this is
/// the live counterpart, kept in its own recipe because it costs minutes.
pub async fn run_expired() -> Result<()> {
    let root_path = env_or("DOMO_ROOT_CA", "/etc/domo/pki/root.pem");
    let root_pem = std::fs::read(&root_path)
        .with_context(|| format!("reading the organization root at {root_path}"))?;

    let probe = Fixture::load(cases::PROBE)?;
    let (status, body) = device_login(&root_pem, &probe).await?;
    anyhow::ensure!(status == 200, "device login returned {status}");
    let auth: DeviceAuth = serde_json::from_str(&body).context("decoding the device token")?;

    let lifetime = Duration::from_secs(
        u64::try_from(auth.expires_in.unwrap_or(900)).context("negative token lifetime")?,
    );
    let wait = lifetime + EXPIRY_MARGIN;
    println!(
        "  → waiting {}s for the device token to expire in real time",
        wait.as_secs()
    );
    tokio::time::sleep(wait).await;

    let own = device_prefix(&probe.tenant, &probe.sa_id);
    let attempt = Attempt {
        client_id: format!("CN={}", probe.sa_id),
        username: probe.sa_id.clone(),
        password: auth.access_token,
        chain: probe.chain.clone(),
        key: probe.key.clone(),
        subscribe: format!("{own}/#"),
        publish: format!("{own}/reported"),
        expect_round_trip: true,
    };
    let observed = attempt_connect(&root_pem, &attempt).await?;

    if observed.satisfies(Expect::AuthorizationDenied) {
        println!("✓ expired-token  {}", observed.describe());
        println!("✓ smoke-slow");
        Ok(())
    } else {
        println!(
            "✗ expired-token  expected {}, got {}",
            Expect::AuthorizationDenied.describe(),
            observed.describe()
        );
        bail!("an expired device token was not refused")
    }
}

/// Assemble the credentials and topics one case calls for.
fn build_attempt(
    probe: &Fixture,
    credential: Credential,
    password: Password,
    target: Target,
    probe_token: &str,
    other_token: &str,
) -> Result<Attempt> {
    // The client identifier and the user name always name the PROBE: the case
    // varies one hop of the chain at a time, so a refusal names which hop.
    let client_id = format!("CN={}", probe.sa_id);
    let own = device_prefix(&probe.tenant, &probe.sa_id);

    let (chain, key) = match credential {
        Credential::Own => (probe.chain.clone(), probe.key.clone()),
        Credential::CertOf(name) => {
            let f = Fixture::load(name)?;
            (f.chain, f.key)
        }
        Credential::ForgedCommonName => forged_identity(probe)?,
        Credential::None => (Vec::new(), Vec::new()),
    };

    let password = match password {
        Password::OwnToken => probe_token.to_owned(),
        Password::Malformed => "not-a-token".to_owned(),
        Password::CorruptSignature => corrupt_signature(probe_token)?,
        Password::TokenOf(name) if name == cases::OTHER => other_token.to_owned(),
        Password::TokenOf(name) => bail!("no token is loaded for fixture '{name}'"),
    };

    let publish = match target {
        Target::Own => format!("{own}/reported"),
        Target::Sibling => {
            let peer = Fixture::load(cases::PEER)?;
            format!("{}/reported", device_prefix(&probe.tenant, &peer.sa_id))
        }
        Target::OtherTenant => {
            let other = Fixture::load(cases::OTHER)?;
            format!("{}/reported", device_prefix(&other.tenant, &probe.sa_id))
        }
        // A level that EXTENDS the probe's own identifier. The boundary has to
        // fall on a separator for this to be refused.
        Target::Adjacent => format!(
            "{}/reported",
            device_prefix(&probe.tenant, &format!("{}-extra", probe.sa_id))
        ),
    };

    Ok(Attempt {
        client_id,
        username: probe.sa_id.clone(),
        password,
        chain,
        key,
        subscribe: format!("{own}/#"),
        publish,
        expect_round_trip: matches!(target, Target::Own),
    })
}

/// The forged-common-name identity: the probe's OWN key and common name, with
/// a leaf signed by the OTHER tenant's authority.
///
/// Chained to the other tenant's signing CA, because that is the authority
/// that signed it — the chain the broker would actually be offered. When AXIAM
/// refuses to mint such a leaf, this falls back to the other tenant's own
/// device certificate: the same boundary, tested less sharply, and the
/// substitution is announced rather than silent.
fn forged_identity(probe: &Fixture) -> Result<(Vec<u8>, Vec<u8>)> {
    let Ok(leaf) = domo_common::secrets::read(super::FORGED_PEM) else {
        println!(
            "  … no forged-common-name leaf on disk (AXIAM refused to mint one); \
             falling back to the other tenant's own device certificate"
        );
        let f = Fixture::load(cases::OTHER)?;
        return Ok((f.chain, f.key));
    };
    let other = Fixture::load(cases::OTHER)?;
    let ca = domo_common::secrets::read(format!("axiam/{}-ca.pem", other.tenant))
        .context("no signing CA for the other tenant")?;
    let mut chain = leaf;
    chain.push(b'\n');
    chain.extend_from_slice(&ca);
    // The probe's own private key: the forged leaf was minted from the probe's
    // own certificate signing request, so this is a matching pair.
    Ok((chain, probe.key.clone()))
}

/// Replace a token's signature segment with a different one of the same shape.
///
/// Not "signed by another key" but observationally the thing that matters: the
/// header and payload still parse, so the request reaches signature
/// verification and is refused there rather than at the shape check. The token
/// itself is never printed.
fn corrupt_signature(token: &str) -> Result<String> {
    let (body, signature) = token
        .rsplit_once('.')
        .context("the device token is not a three-segment JWT")?;
    let flipped: String = signature
        .chars()
        .map(|c| match c {
            'a'..='y' | 'A'..='Y' | '0'..='8' => char::from(c as u8 + 1),
            'z' => 'a',
            'Z' => 'A',
            '9' => '0',
            other => other,
        })
        .collect();
    anyhow::ensure!(
        flipped != signature,
        "the signature segment could not be altered"
    );
    Ok(format!("{body}.{flipped}"))
}

/// Drive one MQTT attempt to a decision, and classify what decided it.
async fn attempt_connect(root_pem: &[u8], a: &Attempt) -> Result<Observed> {
    let host = env_or("DOMO_MQTT_HOST", "rabbitmq");
    let port: u16 = env_or("DOMO_MQTT_PORT", "8883")
        .parse()
        .context("DOMO_MQTT_PORT is not a port number")?;

    let tls = if a.chain.is_empty() {
        // No client certificate at all: the case that tells us whether the
        // broker's peer-certificate requirement is genuinely in force.
        domo_common::tls::client_config(root_pem)?
    } else {
        (*domo_common::tls::shared_client_config_with_identity(root_pem, &a.chain, &a.key)?).clone()
    };

    let mut opts = rumqttc::MqttOptions::new(a.client_id.clone(), &host, port);
    opts.set_credentials(a.username.clone(), a.password.clone());
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_transport(rumqttc::Transport::tls_with_config(
        rumqttc::TlsConfiguration::Rustls(Arc::new(tls)),
    ));

    let (client, mut eventloop) = rumqttc::AsyncClient::new(opts, 10);

    let mut connected = false;
    let mut round_tripped = false;
    let deadline = tokio::time::Instant::now() + ATTEMPT_TIMEOUT;

    while tokio::time::Instant::now() < deadline && !round_tripped {
        let polled = tokio::time::timeout_at(deadline, eventloop.poll()).await;
        let event = match polled {
            Err(_) => break,
            Ok(Ok(e)) => e,
            Ok(Err(e)) => {
                client.disconnect().await.ok();
                // Whether a CONNACK was ever seen is what separates "the
                // handshake was refused" from "the credentials were refused"
                // from "the publish was refused".
                return Ok(classify(e, connected));
            }
        };

        match event {
            Event::Incoming(Packet::ConnAck(ack))
                if ack.code == ConnectReturnCode::Success =>
            {
                connected = true;
                client
                    .subscribe(&a.subscribe, QoS::AtLeastOnce)
                    .await
                    .context("subscribing")?;
            }
            Event::Incoming(Packet::ConnAck(ack)) => {
                client.disconnect().await.ok();
                return Ok(Observed::Refused(ack.code));
            }
            Event::Incoming(Packet::SubAck(_)) => {
                client
                    .publish(
                        &a.publish,
                        QoS::AtLeastOnce,
                        false,
                        format!(r#"{{"probe":"{}"}}"#, a.username),
                    )
                    .await
                    .context("publishing")?;
            }
            Event::Incoming(Packet::Publish(p)) if p.topic == a.publish => {
                round_tripped = true;
            }
            _ => {}
        }
    }

    client.disconnect().await.ok();
    Ok(match (connected, round_tripped, a.expect_round_trip) {
        (_, true, _) => Observed::RoundTripped,
        // A publish outside the device's own namespace is never echoed back to
        // it, so a connected attempt that simply runs out of time IS the
        // refusal — reported as a timeout so it is never mistaken for a pass.
        (true, false, false) => Observed::Timeout,
        (true, false, true) => Observed::ConnectedButNoRoundTrip,
        (false, _, _) => Observed::Timeout,
    })
}

/// Which layer a connection error came from.
fn classify(e: ConnectionError, connected: bool) -> Observed {
    match e {
        ConnectionError::ConnectionRefused(code) => Observed::Refused(code),
        other if connected => Observed::ClosedAfterConnect(other.to_string()),
        // Before any CONNACK: a TLS alert, or the peer closing on us during
        // the handshake. Both mean the handshake never completed.
        other => Observed::Handshake(other.to_string()),
    }
}

