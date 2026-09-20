//! The four authorization decisions, as pure functions.
//!
//! Nothing in this module reads the clock, the filesystem or the network. Every
//! fact a decision needs is passed in — including the outcome of token
//! verification, which the caller performs. That is what makes the whole denial
//! surface testable in milliseconds without a broker, a device or a key set.
//!
//! # The identity chain (T-05-01)
//!
//! Three hops, two enforcers, one identity:
//!
//! 1. The **broker**, at the TLS handshake: `mqtt.ssl_cert_client_id_from =
//!    distinguished_name` refuses a CONNECT whose `client_id` differs from the
//!    certificate's subject DN.
//! 2. **Here:** `client_id == "CN=" + username`.
//! 3. **Here:** the verified token's `sub` equals `username`.
//!
//! Hops 2 and 3 are what make hop 1 mean anything. Remove either and a stolen
//! token becomes usable with any certificate.

use domo_common::DOMO_VHOST;
use domo_common::topic::owns_routing_key;

/// The marker RabbitMQ renders in front of a CN-only certificate subject.
///
/// Confirmed empirically in plan 01-01 (research question A1): the broker
/// handed the backend `client_id = Some("CN=<sa-uuid>")`.
pub const CN_MARKER: &str = "CN=";

/// Why a request was refused.
///
/// Deliberately a **fieldless** enum. A reason can therefore never interpolate
/// a password, a token, or any other request content — not by accident and not
/// by a later edit (T-05-07). Every message is a compile-time constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The request named a virtual host other than `domo`.
    VhostNotDomo,
    /// The user name is not a service-account identifier.
    UsernameNotAServiceAccount,
    /// The broker derived no client identifier from the certificate.
    ClientIdMissing,
    /// The client identifier is not this user name with the marker prefixed.
    ClientIdNotBoundToUsername,
    /// Verification failed, or was never attempted.
    TokenDidNotVerify,
    /// The token verified but names a different account.
    TokenSubjectMismatch,
    /// No cached session for this user name.
    NoSession,
    /// The cached session's token has expired.
    SessionExpired,
    /// The request did not parse against this endpoint's strict form.
    MalformedRequest,
    /// The token's tenant has no verifier registered with this Twin.
    UnknownTenant,
    /// The named exchange or queue is not this device's.
    ResourceNotOwned,
    /// The routing key lies outside this device's own namespace.
    RoutingKeyOutsideNamespace,
}

impl DenyReason {
    /// A constant, credential-free explanation for the operator log.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::VhostNotDomo => "virtual host is not domo",
            Self::UsernameNotAServiceAccount => {
                "user name is not a service-account identifier"
            }
            Self::ClientIdMissing => "no client identifier derived from the certificate",
            Self::ClientIdNotBoundToUsername => {
                "client identifier is not the marker-prefixed user name"
            }
            Self::TokenDidNotVerify => "token did not verify",
            Self::TokenSubjectMismatch => {
                "token subject does not name the connecting account"
            }
            Self::NoSession => "no authenticated session for this user",
            Self::SessionExpired => "the cached session has expired",
            Self::MalformedRequest => "request did not parse",
            Self::UnknownTenant => "tenant is not known to this Twin",
            Self::ResourceNotOwned => "resource is not owned by this device",
            Self::RoutingKeyOutsideNamespace => {
                "routing key is outside the device's own namespace"
            }
        }
    }
}

/// The answer the broker understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Refused. The reason is for *our* log; the wire only ever sees `deny`.
    Deny(DenyReason),
}

impl Decision {
    /// The literal body the broker expects, for either outcome.
    ///
    /// Both are plain text and both ride an HTTP 200: the broker treats any
    /// other status as a backend *error* rather than as a denial (T-05-06).
    #[must_use]
    pub fn body(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny(_) => "deny",
        }
    }

    #[must_use]
    pub fn is_allow(self) -> bool {
        matches!(self, Self::Allow)
    }

    /// The refusal reason, when there is one.
    #[must_use]
    pub fn reason(self) -> Option<DenyReason> {
        match self {
            Self::Allow => None,
            Self::Deny(r) => Some(r),
        }
    }
}

/// An authenticated device session, cached after a successful `/rmq/user`.
///
/// This is the *only* evidence the three token-less endpoints have, which is
/// why a miss denies rather than defers (T-05-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub tenant_id: String,
    pub tenant_slug: String,
    /// The token's own expiry, in Unix seconds.
    pub exp: i64,
}

/// What the broker tells us at CONNECT, plus the outcome of verification.
///
/// `token_sub` is `None` when verification failed — the caller does the
/// network work, this module does the deciding.
#[derive(Debug, Clone)]
pub struct UserFacts<'a> {
    pub username: &'a str,
    pub vhost: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub token_sub: Option<&'a str>,
}

/// The client identifier this user name — and only this user name — may present.
#[must_use]
pub fn expected_client_id(username: &str) -> String {
    format!("{CN_MARKER}{username}")
}

/// Decide a CONNECT end to end.
///
/// Composed of the two halves below so the caller can run the cheap one first
/// and skip the key-set work entirely on a hostile connect.
#[must_use]
pub fn decide_user(f: &UserFacts<'_>) -> Decision {
    let identity = decide_user_identity(f.username, f.vhost, f.client_id);
    if !identity.is_allow() {
        return identity;
    }
    decide_user_subject(f.username, f.token_sub)
}

/// The purely local half of a CONNECT decision: virtual host, user-name shape
/// and the certificate binding. No token is consulted.
#[must_use]
pub fn decide_user_identity(
    username: &str,
    vhost: Option<&str>,
    client_id: Option<&str>,
) -> Decision {
    // The broker omits `vhost` on the user path in some configurations; when
    // it is present it must be ours. `/rmq/vhost` re-checks unconditionally.
    if let Some(v) = vhost
        && v != DOMO_VHOST
    {
        return Decision::Deny(DenyReason::VhostNotDomo);
    }
    if username.is_empty() {
        return Decision::Deny(DenyReason::UsernameNotAServiceAccount);
    }
    match client_id {
        Some(cid) if cid == expected_client_id(username) => {}
        Some(_) => return Decision::Deny(DenyReason::ClientIdNotBoundToUsername),
        None => return Decision::Deny(DenyReason::ClientIdMissing),
    }
    Decision::Allow
}

/// The token half of a CONNECT decision: the verified subject must be this
/// very account.
#[must_use]
pub fn decide_user_subject(username: &str, token_sub: Option<&str>) -> Decision {
    match token_sub {
        Some(sub) if sub == username => Decision::Allow,
        Some(_) => Decision::Deny(DenyReason::TokenSubjectMismatch),
        None => Decision::Deny(DenyReason::TokenDidNotVerify),
    }
}

/// Decide a virtual-host check. Only `domo`, and only for a live session.
#[must_use]
pub fn decide_vhost(vhost: &str, session: Option<&Session>) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny(DenyReason::VhostNotDomo);
    }
    if session.is_none() {
        return Decision::Deny(DenyReason::NoSession);
    }
    Decision::Allow
}

/// Decide a resource (exchange/queue) check.
///
/// A device may use the shared topic exchange and its own MQTT plumbing
/// queues, and nothing else. The queue names are the broker's own convention,
/// derived from the client identifier — which the CONNECT decision has already
/// pinned to this account.
#[must_use]
pub fn decide_resource(
    vhost: &str,
    username: &str,
    resource: &str,
    name: &str,
    session: Option<&Session>,
) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny(DenyReason::VhostNotDomo);
    }
    if session.is_none() {
        return Decision::Deny(DenyReason::NoSession);
    }
    if resource == "exchange" && name == SHARED_TOPIC_EXCHANGE {
        return Decision::Allow;
    }
    if resource == "queue" && owns_mqtt_queue(username, name) {
        return Decision::Allow;
    }
    Decision::Deny(DenyReason::ResourceNotOwned)
}

/// The one exchange every device shares. The MQTT plugin publishes into it.
pub const SHARED_TOPIC_EXCHANGE: &str = "amq.topic";

/// The broker's per-client queue names, for `client_id = "CN=<username>"`.
///
/// Read from `rabbit_mqtt_util.erl`'s `queue_name_bin` on the 4.3.x line (the
/// broker in use is 4.3.6). A future broker minor could change the derivation,
/// in which case this narrows silently — plan 01-06's live subscribe against
/// the real broker is the backstop that would catch it.
#[must_use]
pub fn owns_mqtt_queue(username: &str, name: &str) -> bool {
    let cid = expected_client_id(username);
    name == format!("mqtt-subscription-{cid}qos0")
        || name == format!("mqtt-subscription-{cid}qos1")
        || name == format!("mqtt-will-{cid}")
}

/// Decide a topic (routing-key) check.
#[must_use]
pub fn decide_topic(
    vhost: &str,
    username: &str,
    routing_key: &str,
    session: Option<&Session>,
) -> Decision {
    if vhost != DOMO_VHOST {
        return Decision::Deny(DenyReason::VhostNotDomo);
    }
    let Some(s) = session else {
        return Decision::Deny(DenyReason::NoSession);
    };
    if owns_routing_key(&s.tenant_slug, username, routing_key) {
        Decision::Allow
    } else {
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    }
}
