//! The CONNECT and virtual-host decisions, end to end through the handlers.

use device_twin::rmq::decide::{
    Decision, DenyReason, Session, UserFacts, decide_user, decide_user_identity, decide_vhost,
};

const SA: &str = "01a0be43-4c1e-4f8f-9c0a-2f1d3b5e7a90";
const OTHER_SA: &str = "7f2c9d10-55aa-4b3c-8e21-0d4f6a8b1c33";

fn session() -> Session {
    Session {
        tenant_id: "11111111-1111-1111-1111-111111111111".into(),
        tenant_slug: "lakeside".into(),
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

#[test]
fn connect_allowed_when_every_hop_agrees() {
    let cid = format!("CN={SA}");
    let f = facts(SA, Some("domo"), Some(&cid), Some(SA));
    assert_eq!(decide_user(&f), Decision::Allow);
}

#[test]
fn connect_refused_when_client_id_is_not_the_certificate_subject() {
    // This is the hop that makes a stolen token useless without the cert.
    let cid = format!("CN={OTHER_SA}");
    let f = facts(SA, Some("domo"), Some(&cid), Some(SA));
    assert_eq!(
        decide_user(&f),
        Decision::Deny(DenyReason::ClientIdNotBoundToUsername)
    );
}

#[test]
fn connect_refused_when_token_names_another_account() {
    let cid = format!("CN={SA}");
    let f = facts(SA, Some("domo"), Some(&cid), Some(OTHER_SA));
    assert_eq!(
        decide_user(&f),
        Decision::Deny(DenyReason::TokenSubjectMismatch)
    );
}

#[test]
fn connect_refused_without_a_verified_token() {
    let cid = format!("CN={SA}");
    let f = facts(SA, Some("domo"), Some(&cid), None);
    assert_eq!(decide_user(&f), Decision::Deny(DenyReason::TokenDidNotVerify));
}

#[test]
fn connect_refused_without_a_client_id() {
    let f = facts(SA, Some("domo"), None, Some(SA));
    assert_eq!(decide_user(&f), Decision::Deny(DenyReason::ClientIdMissing));
}

#[test]
fn other_vhosts_are_never_reachable() {
    let cid = format!("CN={SA}");
    let f = facts(SA, Some("/"), Some(&cid), Some(SA));
    assert_eq!(decide_user(&f), Decision::Deny(DenyReason::VhostNotDomo));
    assert_eq!(
        decide_vhost("/", Some(&session())),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
}

#[test]
fn the_vhost_endpoint_denies_without_a_session() {
    assert_eq!(decide_vhost("domo", None), Decision::Deny(DenyReason::NoSession));
    assert_eq!(decide_vhost("domo", Some(&session())), Decision::Allow);
}

#[test]
fn the_wire_body_never_leaks_a_reason() {
    assert_eq!(Decision::Allow.body(), "allow");
    for reason in [
        DenyReason::VhostNotDomo,
        DenyReason::ClientIdNotBoundToUsername,
        DenyReason::TokenSubjectMismatch,
    ] {
        assert_eq!(Decision::Deny(reason).body(), "deny");
    }
}

#[test]
fn the_identity_half_needs_no_token_at_all() {
    let cid = format!("CN={SA}");
    assert_eq!(
        decide_user_identity(SA, Some("domo"), Some(&cid)),
        Decision::Allow
    );
}
