//! The CONNECT and virtual-host decisions: the pure core, the strict forms,
//! the response contract, and what must never reach a log.

mod common;

use common::{
    CLOCK_SKEW_SECS, CapturedLog, FakeAxiam, LAKESIDE_ID, LAKESIDE_SLUG, OTHER_SA, SA, TokenClaims,
    Twin, cn, now, registry,
};
use device_twin::rmq::decide::{
    Decision, DenyReason, Session, UserFacts, decide_user, decide_user_identity, decide_vhost,
};

fn session() -> Session {
    Session {
        tenant_id: LAKESIDE_ID.into(),
        tenant_slug: LAKESIDE_SLUG.into(),
        exp: 4_102_444_800,
    }
}

fn facts<'a>(
    username: &'a str,
    vhost: Option<&'a str>,
    client_id: Option<&'a str>,
    token_sub: Option<&'a str>,
) -> UserFacts<'a> {
    UserFacts {
        username,
        vhost,
        client_id,
        token_sub,
    }
}

// --- the three identity hops ------------------------------------------------

#[test]
fn connect_allowed_when_every_hop_agrees() {
    let cid = cn(SA);
    assert_eq!(decide_user(&facts(SA, Some("domo"), Some(&cid), Some(SA))), Decision::Allow);
}

#[test]
fn connect_refused_when_client_id_is_not_the_certificate_subject() {
    // The hop that makes a stolen token useless without the matching cert.
    let cid = cn(OTHER_SA);
    assert_eq!(
        decide_user(&facts(SA, Some("domo"), Some(&cid), Some(SA))),
        Decision::Deny(DenyReason::ClientIdNotBoundToUsername)
    );
}

#[test]
fn connect_refused_when_the_client_id_is_the_bare_user_name() {
    // No marker prefix at all: the broker never derives this from a subject.
    assert_eq!(
        decide_user(&facts(SA, Some("domo"), Some(SA), Some(SA))),
        Decision::Deny(DenyReason::ClientIdNotBoundToUsername)
    );
}

#[test]
fn connect_refused_when_token_names_another_account() {
    let cid = cn(SA);
    assert_eq!(
        decide_user(&facts(SA, Some("domo"), Some(&cid), Some(OTHER_SA))),
        Decision::Deny(DenyReason::TokenSubjectMismatch)
    );
}

#[test]
fn connect_refused_without_a_verified_token() {
    let cid = cn(SA);
    assert_eq!(
        decide_user(&facts(SA, Some("domo"), Some(&cid), None)),
        Decision::Deny(DenyReason::TokenDidNotVerify)
    );
}

#[test]
fn connect_refused_without_a_client_id() {
    assert_eq!(
        decide_user(&facts(SA, Some("domo"), None, Some(SA))),
        Decision::Deny(DenyReason::ClientIdMissing)
    );
}

#[test]
fn other_vhosts_are_never_reachable() {
    let cid = cn(SA);
    assert_eq!(
        decide_user(&facts(SA, Some("/"), Some(&cid), Some(SA))),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
    assert_eq!(
        decide_vhost("/", Some(&session())),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
}

#[test]
fn a_user_name_that_is_not_a_service_account_identifier_is_refused() {
    // Checked before any token work, so a hostile name costs no verification.
    for bogus in ["not-a-uuid", "", "sa-1", "root", "domo:admin"] {
        let cid = cn(bogus);
        assert_eq!(
            decide_user_identity(bogus, Some("domo"), Some(&cid)),
            Decision::Deny(DenyReason::UsernameNotAServiceAccount),
            "{bogus:?} must not pass as a service-account identifier"
        );
    }
}

#[test]
fn the_identity_half_needs_no_token_at_all() {
    let cid = cn(SA);
    assert_eq!(decide_user_identity(SA, Some("domo"), Some(&cid)), Decision::Allow);
}

#[test]
fn the_wire_body_never_leaks_a_reason() {
    assert_eq!(Decision::Allow.body(), "allow");
    for reason in [
        DenyReason::VhostNotDomo,
        DenyReason::ClientIdNotBoundToUsername,
        DenyReason::TokenSubjectMismatch,
        DenyReason::MalformedRequest,
    ] {
        assert_eq!(Decision::Deny(reason).body(), "deny");
    }
}

#[test]
fn the_decision_core_has_no_ambient_dependencies() {
    // Purity is an acceptance criterion, not a style preference: these
    // functions must be callable with no server, no network and no clock
    // beyond an injected instant.
    let src = include_str!("../src/rmq/decide.rs");
    for forbidden in [
        "SystemTime",
        "Instant::now",
        "Utc::now",
        "std::fs",
        "std::net",
        "reqwest",
        "tokio",
        "async fn",
    ] {
        assert!(
            !src.contains(forbidden),
            "decide.rs must not mention {forbidden}"
        );
    }
}

// --- the strict forms -------------------------------------------------------

#[actix_web::test]
async fn a_connect_without_a_password_is_refused_rather_than_treated_as_empty() {
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    let cid = cn(SA);
    let (status, body) = twin
        .post(
            "/rmq/user",
            &[("username", SA), ("vhost", "domo"), ("client_id", &cid)],
        )
        .await;
    assert_eq!(status, 200, "a denial still rides a 200");
    assert_eq!(body, "deny");
}

#[actix_web::test]
async fn an_unexpected_field_is_rejected_by_the_strict_form() {
    // The vhost endpoint, where a live session would otherwise allow: an
    // unknown field must flip the answer, not be silently ignored.
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    twin.sessions.insert(SA, session());

    let (status, body) = twin
        .post("/rmq/vhost", &[("username", SA), ("vhost", "domo")])
        .await;
    assert_eq!((status, body.as_str()), (200, "allow"), "baseline");

    let (status, body) = twin
        .post(
            "/rmq/vhost",
            &[("username", SA), ("vhost", "domo"), ("surprise", "1")],
        )
        .await;
    assert_eq!(status, 200, "a parse failure is a decision, not a 400");
    assert_eq!(body, "deny");
}

#[actix_web::test]
async fn a_missing_required_field_is_a_denial_not_a_400() {
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    twin.sessions.insert(SA, session());
    let (status, body) = twin.post("/rmq/vhost", &[("username", SA)]).await;
    assert_eq!(status, 200);
    assert_eq!(body, "deny");
}

#[actix_web::test]
async fn an_unparseable_body_is_a_denial_not_a_400() {
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    for uri in ["/rmq/user", "/rmq/vhost", "/rmq/resource", "/rmq/topic"] {
        let (status, body) = twin.post_raw(uri, "%%%not=urlencoded%%%").await;
        assert_eq!(status, 200, "{uri} must answer 200 even on garbage");
        assert_eq!(body, "deny", "{uri} must deny on garbage");
    }
}

// --- the cache is the only evidence the token-less endpoints have -----------

#[actix_web::test]
async fn the_vhost_endpoint_denies_on_a_cache_miss() {
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    let (status, body) = twin
        .post("/rmq/vhost", &[("username", SA), ("vhost", "domo")])
        .await;
    assert_eq!((status, body.as_str()), (200, "deny"));
}

#[actix_web::test]
async fn the_vhost_endpoint_denies_once_the_cached_entry_has_expired() {
    let twin = Twin::offline(&[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    twin.sessions.insert(
        SA,
        Session {
            tenant_id: LAKESIDE_ID.into(),
            tenant_slug: LAKESIDE_SLUG.into(),
            // Comfortably past even the verifier's clock-skew allowance.
            exp: now() - (CLOCK_SKEW_SECS * 10),
        },
    );
    let (status, body) = twin
        .post("/rmq/vhost", &[("username", SA), ("vhost", "domo")])
        .await;
    assert_eq!(status, 200);
    assert_eq!(body, "deny", "an expired session must not keep granting");
}

// --- ordering: the cheap checks come first ----------------------------------

#[actix_web::test]
async fn a_refused_identity_costs_no_key_set_work() {
    let axiam = FakeAxiam::start().await;
    let claims = TokenClaims::device(SA, LAKESIDE_ID);
    let token = axiam.key.sign(&claims);
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));

    // A perfectly good token, presented with somebody else's client id.
    let cid = cn(OTHER_SA);
    let (status, body) = twin
        .post(
            "/rmq/user",
            &[
                ("username", SA),
                ("password", &token),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
        )
        .await;
    assert_eq!((status, body.as_str()), (200, "deny"));
    assert_eq!(
        axiam.jwks_hits().await,
        0,
        "the identity half must refuse before any key-set fetch"
    );
}

// --- the response contract, per endpoint ------------------------------------

#[actix_web::test]
async fn every_endpoint_answers_200_for_both_outcomes() {
    let axiam = FakeAxiam::start().await;
    let token = axiam.key.sign(&TokenClaims::device(SA, LAKESIDE_ID));
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));
    let cid = cn(SA);
    let own_queue = format!("mqtt-subscription-{cid}qos0");
    let own_key = format!("domo.{LAKESIDE_SLUG}.{SA}.reported");

    // CONNECT first: it is what fills the cache the other three read.
    let allow_user: &[(&str, &str)] = &[
        ("username", SA),
        ("password", &token),
        ("vhost", "domo"),
        ("client_id", &cid),
    ];
    assert_eq!(twin.post("/rmq/user", allow_user).await, (200, "allow".into()));

    let cases: Vec<(&str, Vec<(&str, &str)>, &str)> = vec![
        (
            "/rmq/user",
            vec![
                ("username", SA),
                ("password", "not-a-token"),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
            "deny",
        ),
        (
            "/rmq/vhost",
            vec![("username", SA), ("vhost", "domo")],
            "allow",
        ),
        ("/rmq/vhost", vec![("username", SA), ("vhost", "/")], "deny"),
        (
            "/rmq/resource",
            vec![
                ("username", SA),
                ("vhost", "domo"),
                ("resource", "queue"),
                ("name", &own_queue),
                ("permission", "read"),
            ],
            "allow",
        ),
        (
            "/rmq/resource",
            vec![
                ("username", SA),
                ("vhost", "domo"),
                ("resource", "queue"),
                ("name", "somebody-elses-queue"),
                ("permission", "read"),
            ],
            "deny",
        ),
        (
            "/rmq/topic",
            vec![
                ("username", SA),
                ("vhost", "domo"),
                ("resource", "topic"),
                ("name", "amq.topic"),
                ("permission", "write"),
                ("routing_key", &own_key),
            ],
            "allow",
        ),
        (
            "/rmq/topic",
            vec![
                ("username", SA),
                ("vhost", "domo"),
                ("resource", "topic"),
                ("name", "amq.topic"),
                ("permission", "write"),
                ("routing_key", "domo.harbour.someone.else"),
            ],
            "deny",
        ),
    ];

    for (uri, form, expected) in cases {
        let (status, body) = twin.post(uri, &form).await;
        assert_eq!(status, 200, "{uri} must answer 200 for {expected}");
        assert_eq!(body, expected, "{uri}");
    }
}

// --- nothing credential-shaped ever reaches a log ---------------------------

#[actix_web::test]
async fn no_log_record_contains_the_password() {
    let axiam = FakeAxiam::start().await;
    // A structurally valid token whose subject names somebody else, so the
    // request travels the whole path — parse, verify, refuse — while logging.
    let token = axiam
        .key
        .sign(&TokenClaims::device(OTHER_SA, LAKESIDE_ID));
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));
    let cid = cn(SA);

    let log = CapturedLog::new();
    let (status, body) = {
        let _guard = log.install();
        twin.post(
            "/rmq/user",
            &[
                ("username", SA),
                ("password", &token),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
        )
        .await
    };
    assert_eq!((status, body.as_str()), (200, "deny"));

    let captured = log.contents();
    assert!(
        !captured.is_empty(),
        "the capture harness itself must work, or this test proves nothing"
    );
    assert!(
        !captured.contains(&token),
        "the whole token must never appear in a log record"
    );
    for part in token.split('.') {
        assert!(
            !captured.contains(part),
            "no segment of the token may appear in a log record"
        );
    }
}
