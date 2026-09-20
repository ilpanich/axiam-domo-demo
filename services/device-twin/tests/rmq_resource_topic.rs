//! The resource and topic decisions — a device's exchange, queues and
//! namespace, and nothing beyond them.

use device_twin::rmq::decide::{
    Decision, DenyReason, Session, decide_resource, decide_topic,
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

#[test]
fn the_shared_topic_exchange_is_reachable() {
    let s = session();
    assert_eq!(
        decide_resource("domo", SA, "exchange", "amq.topic", Some(&s)),
        Decision::Allow
    );
}

#[test]
fn another_exchange_is_not() {
    let s = session();
    assert_eq!(
        decide_resource("domo", SA, "exchange", "amq.fanout", Some(&s)),
        Decision::Deny(DenyReason::ResourceNotOwned)
    );
}

#[test]
fn a_device_owns_exactly_its_own_derived_queues() {
    let s = session();
    for name in [
        format!("mqtt-subscription-CN={SA}qos0"),
        format!("mqtt-subscription-CN={SA}qos1"),
        format!("mqtt-will-CN={SA}"),
    ] {
        assert_eq!(
            decide_resource("domo", SA, "queue", &name, Some(&s)),
            Decision::Allow,
            "{name} should be this device's own"
        );
    }
}

#[test]
fn a_queue_derived_from_another_client_identifier_is_denied() {
    let s = session();
    let name = format!("mqtt-subscription-CN={OTHER_SA}qos0");
    assert_eq!(
        decide_resource("domo", SA, "queue", &name, Some(&s)),
        Decision::Deny(DenyReason::ResourceNotOwned)
    );
}

#[test]
fn the_resource_endpoint_checks_the_vhost_first() {
    let s = session();
    assert_eq!(
        decide_resource("/", SA, "exchange", "amq.topic", Some(&s)),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
}

#[test]
fn a_device_publishes_and_subscribes_only_inside_its_own_namespace() {
    let s = session();
    assert_eq!(
        decide_topic("domo", SA, &format!("domo.lakeside.{SA}.reported"), Some(&s)),
        Decision::Allow
    );
    assert_eq!(
        decide_topic("domo", SA, &format!("domo.lakeside.{SA}.#"), Some(&s)),
        Decision::Allow
    );
}

#[test]
fn another_device_in_the_same_tenant_is_out_of_reach() {
    let s = session();
    assert_eq!(
        decide_topic(
            "domo",
            SA,
            &format!("domo.lakeside.{OTHER_SA}.reported"),
            Some(&s)
        ),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
}

#[test]
fn the_same_identifier_in_another_tenant_is_out_of_reach() {
    let s = session();
    assert_eq!(
        decide_topic("domo", SA, &format!("domo.harbour.{SA}.reported"), Some(&s)),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
}

#[test]
fn everything_denies_without_a_session() {
    assert_eq!(
        decide_resource("domo", SA, "exchange", "amq.topic", None),
        Decision::Deny(DenyReason::NoSession)
    );
    assert_eq!(
        decide_topic("domo", SA, &format!("domo.lakeside.{SA}.x"), None),
        Decision::Deny(DenyReason::NoSession)
    );
}
