//! RabbitMQ HTTP authorization backend — the thin handler layer.
//!
//! The broker calls these four endpoints for every CONNECT, publish and
//! subscribe. The contract is narrow and unforgiving: respond **HTTP 200** with
//! a plain-text body of exactly `allow` or `deny`. Any other status is read by
//! the broker as a backend *error* rather than as a denial (T-05-06), which
//! silently changes failure semantics — so no handler here returns 4xx or 5xx
//! on an ordinary decision, including one that failed to parse.
//!
//! Everything decidable lives in [`decide`]; everything parseable lives in
//! [`forms`]. This module only joins them to the network.

pub mod decide;
pub mod forms;

use std::sync::Arc;

use actix_web::{HttpResponse, Responder, web};

use decide::{
    Decision, DenyReason, Session, UserFacts, decide_resource, decide_topic, decide_user_identity,
    decide_user_subject, decide_vhost,
};
use forms::{ResourceReq, TopicReq, UserReq, VhostReq};

use crate::tenants::{SessionCache, TenantRegistry, peek_tenant_id};

/// Everything the handlers share.
pub struct TwinState {
    pub tenants: Arc<TenantRegistry>,
    pub sessions: Arc<SessionCache>,
}

impl TwinState {
    #[must_use]
    pub fn new(tenants: Arc<TenantRegistry>, sessions: Arc<SessionCache>) -> Self {
        Self { tenants, sessions }
    }
}

/// Register the health endpoint and the four authorization endpoints.
///
/// Shared by `main.rs` and by the test suite, so the tests exercise the very
/// routing table the broker talks to.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/healthz", web::get().to(healthz))
        .route("/rmq/user", web::post().to(rmq_user))
        .route("/rmq/vhost", web::post().to(rmq_vhost))
        .route("/rmq/resource", web::post().to(rmq_resource))
        .route("/rmq/topic", web::post().to(rmq_topic));
}

/// The one response shape this service ever produces.
fn reply(d: Decision) -> HttpResponse {
    if let Some(reason) = d.reason() {
        // A compile-time-constant string. There is no path by which a password
        // or a token can reach a log record from here (T-05-07).
        tracing::debug!(reason = reason.message(), "denied");
    }
    HttpResponse::Ok().content_type("text/plain").body(d.body())
}

async fn healthz() -> impl Responder {
    HttpResponse::Ok().content_type("text/plain").body("ok")
}

/// CONNECT. The only endpoint that verifies a token; the rest read its result.
async fn rmq_user(
    form: Result<web::Form<UserReq>, actix_web::Error>,
    st: web::Data<TwinState>,
) -> HttpResponse {
    // A parse failure is a *decision*, not a transport error: returning the
    // extractor's own 400 would make the broker report a backend outage.
    let Ok(form) = form else {
        return reply(Decision::Deny(DenyReason::MalformedRequest));
    };
    let r = form.into_inner();

    // Identity fields only — never `r.password`, which is the access token.
    tracing::debug!(
        username = %r.username,
        client_id = ?r.client_id,
        vhost = ?r.vhost,
        "CONNECT attempt"
    );

    // The cheap, purely local hops first, so a malformed or hostile connect
    // costs no key-set work at all (T-05-08).
    let identity = decide_user_identity(&r.username, r.vhost.as_deref(), r.client_id.as_deref());
    if !identity.is_allow() {
        return reply(identity);
    }

    // Route to a verifier using the token's *unverified* tenant claim, then
    // let that verifier re-check the claim it was chosen by.
    let Some(tenant_id) = peek_tenant_id(&r.password) else {
        return reply(Decision::Deny(DenyReason::UnknownTenant));
    };
    if !st.tenants.knows(&tenant_id) {
        // Deny before building or invoking any verifier: a token naming an
        // unregistered tenant must never be tried against another's.
        return reply(Decision::Deny(DenyReason::UnknownTenant));
    }
    let verifier = match st.tenants.verifier(&tenant_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "could not build a verifier");
            return reply(Decision::Deny(DenyReason::UnknownTenant));
        }
    };

    let claims = match verifier.verify(&r.password).await {
        Ok(c) => c,
        Err(e) => {
            // The SDK's error never carries the token itself.
            tracing::debug!(error = %e, "token verification failed");
            return reply(Decision::Deny(DenyReason::TokenDidNotVerify));
        }
    };

    let decision = decide_user_subject(&r.username, Some(&claims.sub));
    if decision.is_allow() {
        let Some(slug) = st.tenants.slug(&claims.tenant_id) else {
            return reply(Decision::Deny(DenyReason::UnknownTenant));
        };
        st.sessions.insert(
            r.username.clone(),
            Session {
                tenant_id: claims.tenant_id.clone(),
                tenant_slug: slug,
                exp: claims.exp,
            },
        );
        tracing::info!(account = %r.username, tenant = %claims.tenant_id, "CONNECT allowed");
    }
    reply(decision)
}

/// The full CONNECT decision, for callers that already hold every fact.
#[must_use]
pub fn decide_user(f: &UserFacts<'_>) -> Decision {
    decide::decide_user(f)
}

async fn rmq_vhost(
    form: Result<web::Form<VhostReq>, actix_web::Error>,
    st: web::Data<TwinState>,
) -> HttpResponse {
    let Ok(form) = form else {
        return reply(Decision::Deny(DenyReason::MalformedRequest));
    };
    let r = form.into_inner();
    reply(decide_vhost(&r.vhost, st.sessions.get(&r.username).as_ref()))
}

async fn rmq_resource(
    form: Result<web::Form<ResourceReq>, actix_web::Error>,
    st: web::Data<TwinState>,
) -> HttpResponse {
    let Ok(form) = form else {
        return reply(Decision::Deny(DenyReason::MalformedRequest));
    };
    let r = form.into_inner();
    reply(decide_resource(
        &r.vhost,
        &r.username,
        &r.resource,
        &r.name,
        st.sessions.get(&r.username).as_ref(),
    ))
}

async fn rmq_topic(
    form: Result<web::Form<TopicReq>, actix_web::Error>,
    st: web::Data<TwinState>,
) -> HttpResponse {
    let Ok(form) = form else {
        return reply(Decision::Deny(DenyReason::MalformedRequest));
    };
    let r = form.into_inner();
    reply(decide_topic(
        &r.vhost,
        &r.username,
        &r.routing_key,
        st.sessions.get(&r.username).as_ref(),
    ))
}
