//! One request struct per broker endpoint.
//!
//! The parameter lists come from `rabbit_auth_backend_http.erl` on the 4.3.x
//! line, transported as `application/x-www-form-urlencoded` over POST (never
//! GET — the broker debug-logs the full URL, credentials included).
//!
//! Every struct is **strict**: `deny_unknown_fields`, and required fields that
//! are genuinely required rather than silently defaulted. That is this
//! service's input-validation control (ASVS V5) — a field the broker did not
//! send must not read as an empty string that happens to compare unequal.
//!
//! Strictness cuts both ways, and deliberately: if a future broker minor adds
//! a parameter, these structs reject the request and the decision is `deny`,
//! which is the safe direction but a visible one. Plan 01-06's live CONNECT
//! against the real broker is what would surface it immediately.

use serde::Deserialize;

/// `POST /rmq/user` — the CONNECT decision.
///
/// `password` is the device's access token. It is required rather than
/// defaulted: a CONNECT without one must be refused, not treated as an empty
/// credential.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserReq {
    pub username: String,
    pub password: String,
    /// Absent in some broker configurations; checked when present.
    #[serde(default)]
    pub vhost: Option<String>,
    /// Optional *at the type level* on purpose: a request without one must
    /// reach the decision core and be denied there, not fail to parse.
    #[serde(default)]
    pub client_id: Option<String>,
}

/// `POST /rmq/vhost`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VhostReq {
    pub username: String,
    pub vhost: String,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub tags: Option<String>,
}

/// `POST /rmq/resource`.
///
/// `permission` (`configure`/`write`/`read`) is captured but not branched on:
/// ownership of the resource is the whole decision, and a device that owns its
/// queue may do all three to it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReq {
    pub username: String,
    pub vhost: String,
    pub resource: String,
    pub name: String,
    pub permission: String,
    #[serde(default)]
    pub tags: Option<String>,
}

/// `POST /rmq/topic`.
///
/// The broker sends the exchange in `name` and the AMQP-translated topic in
/// `routing_key`; `variable_map.*` repeats the connection's own identity.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicReq {
    pub username: String,
    pub vhost: String,
    pub resource: String,
    pub name: String,
    pub permission: String,
    pub routing_key: String,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default, rename = "variable_map.username")]
    pub variable_map_username: Option<String>,
    #[serde(default, rename = "variable_map.vhost")]
    pub variable_map_vhost: Option<String>,
    #[serde(default, rename = "variable_map.client_id")]
    pub variable_map_client_id: Option<String>,
}
