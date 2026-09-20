//! The resource and topic decisions — a device's own exchange, its own queues
//! and its own topic namespace, and nothing one character beyond them.

use device_twin::rmq::decide::{
    Decision, DenyReason, SHARED_TOPIC_EXCHANGE, Session, decide_resource, decide_topic,
};
use domo_common::topic::{device_prefix, owns_routing_key, to_routing_key};

const SA: &str = "01a0be43-4c1e-4f8f-9c0a-2f1d3b5e7a90";
const OTHER_SA: &str = "7f2c9d10-55aa-4b3c-8e21-0d4f6a8b1c33";
const TENANT: &str = "lakeside";
const OTHER_TENANT: &str = "harbour";

/// An injected instant comfortably inside the fixture session's lifetime.
/// The decision core never reads a clock, so this is the whole of "now".
const NOW: i64 = 4_000_000_000;

fn session() -> Session {
    Session {
        tenant_id: "11111111-1111-1111-1111-111111111111".into(),
        tenant_slug: TENANT.into(),
        exp: 4_102_444_800,
    }
}

/// The routing-key prefix this device owns, trailing separator included.
fn own_prefix() -> String {
    format!("domo.{TENANT}.{SA}")
}

// --- resources --------------------------------------------------------------

#[test]
fn the_shared_topic_exchange_is_reachable_for_read_and_write() {
    let s = session();
    for permission in ["read", "write", "configure"] {
        assert_eq!(
            decide_resource("domo", SA, "exchange", SHARED_TOPIC_EXCHANGE, Some(&s), NOW),
            Decision::Allow,
            "the shared exchange must be reachable to {permission}"
        );
    }
}

#[test]
fn another_exchange_is_not_reachable() {
    let s = session();
    for name in ["amq.fanout", "amq.direct", "domo.private", ""] {
        assert_eq!(
            decide_resource("domo", SA, "exchange", name, Some(&s), NOW),
            Decision::Deny(DenyReason::ResourceNotOwned),
            "{name:?} is not the shared topic exchange"
        );
    }
}

#[test]
fn a_device_owns_exactly_the_three_derived_queue_forms() {
    let s = session();
    for name in [
        format!("mqtt-subscription-CN={SA}qos0"),
        format!("mqtt-subscription-CN={SA}qos1"),
        format!("mqtt-will-CN={SA}"),
    ] {
        assert_eq!(
            decide_resource("domo", SA, "queue", &name, Some(&s), NOW),
            Decision::Allow,
            "{name} is derived from this device's own client identifier"
        );
    }
}

#[test]
fn a_queue_derived_from_another_client_identifier_is_denied() {
    let s = session();
    for name in [
        format!("mqtt-subscription-CN={OTHER_SA}qos0"),
        format!("mqtt-subscription-CN={OTHER_SA}qos1"),
        format!("mqtt-will-CN={OTHER_SA}"),
    ] {
        assert_eq!(
            decide_resource("domo", SA, "queue", &name, Some(&s), NOW),
            Decision::Deny(DenyReason::ResourceNotOwned),
            "{name} belongs to another device"
        );
    }
}

#[test]
fn a_queue_matching_none_of_the_derived_forms_is_denied() {
    let s = session();
    for name in [
        format!("mqtt-subscription-CN={SA}qos2"),
        format!("mqtt-subscription-CN={SA}"),
        format!("CN={SA}"),
        format!("mqtt-will-CN={SA}-backup"),
        "amq.gen-whatever".to_string(),
    ] {
        assert_eq!(
            decide_resource("domo", SA, "queue", &name, Some(&s), NOW),
            Decision::Deny(DenyReason::ResourceNotOwned),
            "{name} is not one of the broker's three derived forms"
        );
    }
}

#[test]
fn the_resource_endpoint_checks_the_virtual_host_before_any_name_matching() {
    let s = session();
    assert_eq!(
        decide_resource("/", SA, "exchange", SHARED_TOPIC_EXCHANGE, Some(&s), NOW),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
}

// --- topics -----------------------------------------------------------------

#[test]
fn a_device_reaches_its_own_namespace_for_publish_and_subscribe() {
    let s = session();
    for key in [
        format!("{}.reported", own_prefix()),
        format!("{}.desired", own_prefix()),
        format!("{}.telemetry.temperature", own_prefix()),
        format!("{}.#", own_prefix()),
        format!("{}.*", own_prefix()),
    ] {
        assert_eq!(
            decide_topic("domo", SA, &key, Some(&s), NOW),
            Decision::Allow,
            "{key} lies inside this device's own namespace"
        );
    }
}

#[test]
fn another_device_in_the_same_tenant_is_out_of_reach() {
    let s = session();
    let key = format!("domo.{TENANT}.{OTHER_SA}.reported");
    assert_eq!(
        decide_topic("domo", SA, &key, Some(&s), NOW),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
}

#[test]
fn the_same_identifier_in_another_tenant_is_out_of_reach() {
    // The cross-tenant case. Remove the tenant segment from the comparison and
    // this is the test that notices.
    let s = session();
    let key = format!("domo.{OTHER_TENANT}.{SA}.reported");
    assert_eq!(
        decide_topic("domo", SA, &key, Some(&s), NOW),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
}

#[test]
fn an_adjacent_namespace_whose_name_merely_extends_this_one_is_out_of_reach() {
    // The adjacency case. A prefix comparison passes every one of these,
    // which is the difference between a device seeing only its own topics and
    // a device seeing a neighbour whose identifier happens to extend its own.
    let s = session();
    for suffix in ["x", "0", "-2", "extra.segments"] {
        let key = format!("{}{suffix}.reported", own_prefix());
        assert_eq!(
            decide_topic("domo", SA, &key, Some(&s), NOW),
            Decision::Deny(DenyReason::RoutingKeyOutsideNamespace),
            "{key} is a sibling namespace, not this one"
        );
    }
}

#[test]
fn the_bare_namespace_prefix_is_not_publishable() {
    let s = session();
    assert_eq!(
        decide_topic("domo", SA, &own_prefix(), Some(&s), NOW),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
    // Nor is the prefix with an empty level under it.
    assert_eq!(
        decide_topic("domo", SA, &format!("{}.", own_prefix()), Some(&s), NOW),
        Decision::Deny(DenyReason::RoutingKeyOutsideNamespace)
    );
    // The subscribe form of "everything mine", however, is exactly how the
    // broker presents a subscription to `domo/<tenant>/<device>/#`.
    assert_eq!(
        decide_topic("domo", SA, &format!("{}.#", own_prefix()), Some(&s), NOW),
        Decision::Allow
    );
}

#[test]
fn a_multi_level_wildcard_may_not_hide_in_the_middle_of_a_key() {
    // `#` matches everything after it, so a non-terminal one is either a
    // broker the scheme does not understand or an attempt to widen the
    // subscription past the namespace. Either way: deny.
    let s = session();
    for key in [
        format!("{}.#.reported", own_prefix()),
        format!("domo.#.{SA}.reported"),
        "domo.#".to_string(),
        "#".to_string(),
    ] {
        assert_eq!(
            decide_topic("domo", SA, &key, Some(&s), NOW),
            Decision::Deny(DenyReason::RoutingKeyOutsideNamespace),
            "{key} must not be accepted"
        );
    }
}

#[test]
fn a_wildcard_in_the_tenant_or_device_position_is_out_of_reach() {
    let s = session();
    for key in [
        format!("domo.*.{SA}.reported"),
        format!("domo.{TENANT}.*.reported"),
        "*.*.*.reported".to_string(),
    ] {
        assert_eq!(
            decide_topic("domo", SA, &key, Some(&s), NOW),
            Decision::Deny(DenyReason::RoutingKeyOutsideNamespace),
            "{key} widens past this device"
        );
    }
}

#[test]
fn the_topic_endpoint_denies_on_a_cache_miss_and_on_the_wrong_virtual_host() {
    let key = format!("{}.reported", own_prefix());
    assert_eq!(
        decide_topic("domo", SA, &key, None, NOW),
        Decision::Deny(DenyReason::NoSession)
    );
    assert_eq!(
        decide_topic("/", SA, &key, Some(&session()), NOW),
        Decision::Deny(DenyReason::VhostNotDomo)
    );
}

// --- the shared scheme ------------------------------------------------------

#[test]
fn the_prefix_builder_and_the_translator_agree() {
    // The probe publishes on the MQTT spelling; the Twin authorizes the
    // routing spelling. They have to be the same place.
    let mqtt = format!("{}/reported", device_prefix(TENANT, SA));
    let routing = to_routing_key(&mqtt).expect("a plain topic must translate");
    assert_eq!(routing, format!("{}.reported", own_prefix()));
    assert!(owns_routing_key(TENANT, SA, &routing));
}

#[test]
fn translation_is_total_rather_than_silently_altering() {
    // These characters are legal in an MQTT topic level but meaningful in the
    // broker's routing-key syntax. Carrying them across unchanged would
    // produce a key that means something different from the topic that
    // produced it, so the translator refuses instead.
    for ambiguous in [
        "domo/lakeside/dev.ice/reported",
        "domo/lakeside/dev/repo.rted",
        "domo/lake*side/dev/reported",
        "domo/lakeside//reported",
        "",
    ] {
        assert_eq!(
            to_routing_key(ambiguous),
            None,
            "{ambiguous:?} cannot cross unambiguously and must be refused"
        );
    }
}

#[test]
fn translation_carries_the_wildcards_across() {
    assert_eq!(
        to_routing_key("domo/lakeside/dev/#").as_deref(),
        Some("domo.lakeside.dev.#")
    );
    assert_eq!(
        to_routing_key("domo/+/dev/state").as_deref(),
        Some("domo.*.dev.state")
    );
    // A multi-level wildcard is only legal as the final level in MQTT.
    assert_eq!(to_routing_key("domo/#/dev"), None);
}
