//! The MQTT topic scheme and its AMQP routing-key translation.
//!
//! Every device topic is `domo/{tenant_slug}/{sa_uuid}/…`. The tenant slug and
//! the service-account UUID are both in the path so that an authorization
//! decision can be made from the topic alone, without a lookup: the Twin allows
//! a routing key only under the connecting device's own prefix.
//!
//! RabbitMQ's MQTT plugin publishes into `amq.topic`, translating MQTT topic
//! separators to AMQP ones. The Twin's `/rmq/topic` callback is handed the
//! AMQP form, so both spellings have to be derivable from one place.

/// Root segment of every topic in this demo.
pub const TOPIC_ROOT: &str = "domo";

/// The MQTT topic prefix owned by one device.
///
/// ```
/// # use domo_common::topic::device_prefix;
/// assert_eq!(device_prefix("lakeside", "abc"), "domo/lakeside/abc");
/// ```
#[must_use]
pub fn device_prefix(tenant_slug: &str, sa_uuid: &str) -> String {
    format!("{TOPIC_ROOT}/{tenant_slug}/{sa_uuid}")
}

/// The AMQP routing-key prefix owned by one device, trailing dot included.
///
/// The trailing dot is load-bearing: without it `domo.lakeside.abc` would also
/// prefix-match `domo.lakeside.abcdef`, letting one device publish under
/// another's identity.
#[must_use]
pub fn device_routing_prefix(tenant_slug: &str, sa_uuid: &str) -> String {
    format!("{TOPIC_ROOT}.{tenant_slug}.{sa_uuid}.")
}

/// Translate an MQTT topic (or filter) to its AMQP routing-key form.
///
/// `/` → `.`, `+` → `*`, `#` → `#`.
#[must_use]
pub fn mqtt_to_amqp(topic: &str) -> String {
    topic
        .split('/')
        .map(|seg| match seg {
            "+" => "*",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// True when `routing_key` names a topic the given device owns.
#[must_use]
pub fn owns_routing_key(tenant_slug: &str, sa_uuid: &str, routing_key: &str) -> bool {
    routing_key.starts_with(&device_routing_prefix(tenant_slug, sa_uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_built_from_tenant_and_account() {
        assert_eq!(device_prefix("lakeside", "u-1"), "domo/lakeside/u-1");
        assert_eq!(device_routing_prefix("lakeside", "u-1"), "domo.lakeside.u-1.");
    }

    #[test]
    fn mqtt_separators_translate_to_amqp() {
        assert_eq!(mqtt_to_amqp("domo/lakeside/u-1/reported"), "domo.lakeside.u-1.reported");
        assert_eq!(mqtt_to_amqp("domo/lakeside/u-1/#"), "domo.lakeside.u-1.#");
        assert_eq!(mqtt_to_amqp("domo/+/u-1/state"), "domo.*.u-1.state");
    }

    #[test]
    fn ownership_requires_the_full_segment() {
        assert!(owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1.reported"));
        // The trailing dot stops a prefix collision with a longer UUID.
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-12.reported"));
        assert!(!owns_routing_key("lakeside", "u-1", "domo.harbour.u-1.reported"));
        // The prefix alone, with nothing beneath it, is not a publishable key.
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1"));
    }
}
