//! RabbitMQ HTTP authorization backend.
//!
//! The broker calls these four endpoints for every CONNECT, publish and
//! subscribe. The contract is narrow and unforgiving: respond `200` with a
//! body of exactly `allow`, `deny`, or `allow <tags>`. Any other status — or
//! any other body — is treated by the broker as a backend error, and the
//! operation is refused with a message that does not say why.
//!
//! # The identity chain (T-01-01)
//!
//! Four hops, each enforced by a different component:
//!
//! 1. TLS: the broker requires a client certificate chaining to the org root.
//! 2. `mqtt.ssl_cert_client_id_from = distinguished_name`: the broker refuses a
//!    CONNECT whose `client_id` differs from the certificate's subject DN.
//! 3. Here: `client_id == "CN=" + username`.
//! 4. Here: the JWT verifies against AXIAM's JWKS *for the tenant it claims*,
//!    and its `sub` equals `username`.
//!
//! Hops 3 and 4 are what make hop 2 mean something: together they force the
//! certificate, the connection identity and the token to name one account. A
//! stolen JWT replayed without the matching certificate dies at hop 1.
//!
//! The decision logic is a pure function ([`decide_user`] and friends) taking
//! already-gathered facts, so it is unit-testable with no server, no broker and
//! no network.

use std::collections::HashMap;

use serde::Deserialize;

use domo_common::DOMO_VHOST;
use domo_common::topic::owns_routing_key;

/// The answer the broker understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Denied. The reason is for *our* logs, never for the wire: the broker
    /// only ever sees `deny`.
    Deny(&'static str),
}

impl Decision {
    /// The literal body the broker expects.
    #[must_use]
    pub fn body(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny(_) => "deny",
        }
    }

    #[must_use]
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// An authenticated device session, cached after a successful `/rmq/user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub tenant_id: String,
    pub tenant_slug: String,
    pub exp: i64,
}

/// What the broker tells us at CONNECT, plus the verified token's subject.
///
/// `jwt_sub` is `None` when verification failed — the caller does the network
/// work, this function does the deciding.
#[derive(Debug, Clone)]
pub struct UserFacts<'a> {
    pub username: &'a str,
    pub vhost: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub jwt_sub: Option<&'a str>,
}

/// Decide a CONNECT.
#[must_use]
pub fn decide_user(f: &UserFacts<'_>) -> Decision {
    if f.username.is_empty() {
        return Decision::Deny("empty username");
    }
    // The broker omits `vhost` on the user path in some configurations; when it
    // is present it must be ours. The vhost path re-checks it unconditionally.
    if let Some(v) = f.vhost
        && v != DOMO_VHOST
    {
        return Decision::Deny("vhost is not domo");
    }
    // Hop 3: the connection identity must be the certificate subject for this
    // very account. Absent client_id means the broker did not derive one from
    // the certificate, which is itself disqualifying.
    match f.client_id {
        Some(cid) if cid == format!("CN={}", f.username) => {}
        Some(_) => return Decision::Deny("client_id does not match CN=<username>"),
        None => return Decision::Deny("no client_id derived from the certificate"),
    }
    // Hop 4: the token must verify AND name this same account.
    match f.jwt_sub {
        Some(sub) if sub == f.username => Decision::Allow,
        Some(_) => Decision::Deny("token subject does not match the connecting account"),
        None => Decision::Deny("token did not verify"),
    }
}

/// Decide a vhost access check. Only `domo`, and only for a live session.
#[must_use]
pub fn decide_vhost(vhost: &str, session: Option<&Session>) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny("vhost is not domo");
    }
    if session.is_none() {
        return Decision::Deny("no authenticated session for this user");
    }
    Decision::Allow
}

/// Decide a resource (exchange/queue) check.
///
/// A device may use the shared `amq.topic` exchange and its own MQTT plumbing
/// queues, and nothing else. The queue names are RabbitMQ's own MQTT plugin
/// convention, derived from the client id — which hop 3 has already pinned to
/// this account.
#[must_use]
pub fn decide_resource(
    vhost: &str,
    username: &str,
    resource: &str,
    name: &str,
    session: Option<&Session>,
) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny("vhost is not domo");
    }
    if session.is_none() {
        return Decision::Deny("no authenticated session for this user");
    }
    if resource == "exchange" && name == "amq.topic" {
        return Decision::Allow;
    }
    if resource == "queue" && owns_mqtt_queue(username, name) {
        return Decision::Allow;
    }
    Decision::Deny("resource is not owned by this device")
}

/// The MQTT plugin's per-client queue names, for `client_id = "CN=<username>"`.
#[must_use]
pub fn owns_mqtt_queue(username: &str, name: &str) -> bool {
    let cid = format!("CN={username}");
    name == format!("mqtt-subscription-{cid}qos0")
        || name == format!("mqtt-subscription-{cid}qos1")
        || name == format!("mqtt-will-{cid}")
}

/// Decide a topic (routing-key) check.
///
/// Allowed only beneath `domo.<tenant_slug>.<sa-uuid>.`, which is what confines
/// one device to its own topic space.
#[must_use]
pub fn decide_topic(
    vhost: &str,
    username: &str,
    routing_key: &str,
    session: Option<&Session>,
) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny("vhost is not domo");
    }
    let Some(s) = session else {
        return Decision::Deny("no authenticated session for this user");
    };
    // A subscribe on `domo/<tenant>/<sa>/#` arrives as this exact prefix with a
    // trailing `#`, which the ownership check accepts.
    if owns_routing_key(&s.tenant_slug, username, routing_key) {
        Decision::Allow
    } else {
        Decision::Deny("routing key is outside the device's own topic space")
    }
}

/// Read `tenant_id` out of a JWT **without verifying it**.
///
/// This only chooses *which* verifier to use. The chosen verifier is built with
/// `expect_tenant_id`, so a token lying about its tenant fails verification a
/// moment later — the unverified peek cannot promote anything.
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

// --- wire forms -------------------------------------------------------------
//
// `#[serde(default)]` throughout: RabbitMQ's field set varies by backend
// version and by whether the MQTT plugin derived a client id. A missing field
// must produce a deny, not a 400 that the broker reports as a backend error.

#[derive(Debug, Deserialize)]
pub struct UserReq {
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub vhost: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VhostReq {
    pub username: String,
    pub vhost: String,
}

// `permission` ("configure"/"write"/"read") is captured but not branched on:
// ownership of the resource is the whole decision here, and a device that owns
// its queue may do all three to it. Kept so the payload is documented in full
// and so a future plan can tighten per-permission rules without re-deriving it.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ResourceReq {
    pub username: String,
    pub vhost: String,
    pub resource: String,
    pub name: String,
    #[serde(default)]
    pub permission: String,
}

// Same reasoning: only `routing_key` decides, the rest documents the payload.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TopicReq {
    pub username: String,
    pub vhost: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub permission: String,
    #[serde(default)]
    pub routing_key: String,
}

/// In-memory session cache, keyed by service-account UUID.
pub type SessionCache = HashMap<String, Session>;

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            tenant_id: "11111111-1111-1111-1111-111111111111".into(),
            tenant_slug: "lakeside".into(),
            exp: 4_102_444_800,
        }
    }

    #[test]
    fn connect_allowed_when_every_hop_agrees() {
        let f = UserFacts {
            username: "sa-1",
            vhost: Some("domo"),
            client_id: Some("CN=sa-1"),
            jwt_sub: Some("sa-1"),
        };
        assert_eq!(decide_user(&f), Decision::Allow);
    }

    #[test]
    fn connect_refused_when_client_id_is_not_the_certificate_subject() {
        // This is the hop that makes a stolen token useless without the cert.
        let f = UserFacts {
            username: "sa-1",
            vhost: Some("domo"),
            client_id: Some("CN=sa-2"),
            jwt_sub: Some("sa-1"),
        };
        assert!(!decide_user(&f).is_allow());
    }

    #[test]
    fn connect_refused_when_token_names_another_account() {
        let f = UserFacts {
            username: "sa-1",
            vhost: Some("domo"),
            client_id: Some("CN=sa-1"),
            jwt_sub: Some("sa-2"),
        };
        assert!(!decide_user(&f).is_allow());
    }

    #[test]
    fn connect_refused_without_a_verified_token_or_a_client_id() {
        let base = UserFacts {
            username: "sa-1",
            vhost: Some("domo"),
            client_id: Some("CN=sa-1"),
            jwt_sub: None,
        };
        assert!(!decide_user(&base).is_allow());

        let no_cid = UserFacts {
            client_id: None,
            jwt_sub: Some("sa-1"),
            ..base.clone()
        };
        assert!(!decide_user(&no_cid).is_allow());
    }

    #[test]
    fn other_vhosts_are_never_reachable() {
        let f = UserFacts {
            username: "sa-1",
            vhost: Some("/"),
            client_id: Some("CN=sa-1"),
            jwt_sub: Some("sa-1"),
        };
        assert!(!decide_user(&f).is_allow());
        assert!(!decide_vhost("/", Some(&session())).is_allow());
    }

    #[test]
    fn resources_are_limited_to_the_shared_exchange_and_own_queues() {
        let s = session();
        assert!(decide_resource("domo", "sa-1", "exchange", "amq.topic", Some(&s)).is_allow());
        assert!(
            decide_resource(
                "domo",
                "sa-1",
                "queue",
                "mqtt-subscription-CN=sa-1qos0",
                Some(&s)
            )
            .is_allow()
        );
        // Another device's queue.
        assert!(
            !decide_resource(
                "domo",
                "sa-1",
                "queue",
                "mqtt-subscription-CN=sa-2qos0",
                Some(&s)
            )
            .is_allow()
        );
        // Any other exchange.
        assert!(!decide_resource("domo", "sa-1", "exchange", "amq.fanout", Some(&s)).is_allow());
    }

    #[test]
    fn topics_are_confined_to_the_devices_own_space() {
        let s = session();
        assert!(decide_topic("domo", "sa-1", "domo.lakeside.sa-1.reported", Some(&s)).is_allow());
        assert!(decide_topic("domo", "sa-1", "domo.lakeside.sa-1.#", Some(&s)).is_allow());
        assert!(!decide_topic("domo", "sa-1", "domo.lakeside.sa-2.reported", Some(&s)).is_allow());
        assert!(!decide_topic("domo", "sa-1", "domo.harbour.sa-1.reported", Some(&s)).is_allow());
    }

    #[test]
    fn everything_denies_without_a_session() {
        assert!(!decide_vhost("domo", None).is_allow());
        assert!(!decide_resource("domo", "sa-1", "exchange", "amq.topic", None).is_allow());
        assert!(!decide_topic("domo", "sa-1", "domo.lakeside.sa-1.x", None).is_allow());
    }

    #[test]
    fn tenant_peek_reads_the_claim_without_verifying() {
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"sub":"sa-1","tenant_id":"t-9"}"#);
        let jwt = format!("aGVhZGVy.{payload}.c2ln");
        assert_eq!(peek_tenant_id(&jwt).as_deref(), Some("t-9"));
        assert_eq!(peek_tenant_id("not-a-jwt"), None);
    }

    #[test]
    fn the_wire_body_never_leaks_a_reason() {
        assert_eq!(Decision::Allow.body(), "allow");
        assert_eq!(Decision::Deny("anything at all").body(), "deny");
    }
}
