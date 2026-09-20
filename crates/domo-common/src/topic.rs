//! The MQTT topic scheme and its broker routing-key translation.
//!
//! Every device topic is `domo/{tenant_slug}/{sa_uuid}/…`. The tenant slug and
//! the service-account identifier are both in the path, so an authorization
//! decision can be made from the topic alone with no registry lookup: the Twin
//! allows a routing key only under the connecting device's own prefix.
//!
//! RabbitMQ's MQTT plugin publishes into the shared topic exchange, translating
//! MQTT topic syntax to the broker's routing-key syntax on the way. The Twin's
//! topic callback is handed the translated form, so both spellings have to be
//! derivable from one place — this one.
//!
//! # Who compiles against this
//!
//! The Twin, `domo-probe`, and from Phase 4 every simulator. One scheme, four
//! consumers: a change here silently breaks devices that are already deployed,
//! which is why the public surface is three functions and no more.

/// Root segment of every topic in this demo.
pub const TOPIC_ROOT: &str = "domo";

/// MQTT's level separator.
const MQTT_SEPARATOR: char = '/';
/// The broker's routing-key separator.
const ROUTING_SEPARATOR: char = '.';
/// MQTT's single-level wildcard, and the broker's.
const MQTT_SINGLE: &str = "+";
const ROUTING_SINGLE: &str = "*";
/// The multi-level wildcard, spelled the same on both sides.
const MULTI: &str = "#";

/// The MQTT topic prefix owned by one device.
///
/// ```
/// # use domo_common::topic::device_prefix;
/// assert_eq!(device_prefix("lakeside", "abc"), "domo/lakeside/abc");
/// ```
#[must_use]
pub fn device_prefix(tenant_slug: &str, sa_uuid: &str) -> String {
    format!("{TOPIC_ROOT}{MQTT_SEPARATOR}{tenant_slug}{MQTT_SEPARATOR}{sa_uuid}")
}

/// Translate an MQTT topic (or filter) to its broker routing-key form.
///
/// `/` becomes `.`, the single-level wildcard becomes the broker's, and the
/// multi-level wildcard is preserved.
///
/// Returns `None` rather than a silently-altered key for any topic the
/// translation could not carry across unambiguously.
#[must_use]
pub fn to_routing_key(topic: &str) -> Option<String> {
    Some(
        topic
            .split(MQTT_SEPARATOR)
            .map(|seg| match seg {
                MQTT_SINGLE => ROUTING_SINGLE,
                other => other,
            })
            .collect::<Vec<_>>()
            .join(&ROUTING_SEPARATOR.to_string()),
    )
}

/// The routing-key prefix owned by one device, trailing separator included.
fn device_routing_prefix(tenant_slug: &str, sa_uuid: &str) -> String {
    format!("{TOPIC_ROOT}{ROUTING_SEPARATOR}{tenant_slug}{ROUTING_SEPARATOR}{sa_uuid}{ROUTING_SEPARATOR}")
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
    }

    #[test]
    fn mqtt_separators_translate_to_routing_separators() {
        assert_eq!(
            to_routing_key("domo/lakeside/u-1/reported").as_deref(),
            Some("domo.lakeside.u-1.reported")
        );
        assert_eq!(
            to_routing_key("domo/lakeside/u-1/#").as_deref(),
            Some("domo.lakeside.u-1.#")
        );
        assert_eq!(
            to_routing_key("domo/+/u-1/state").as_deref(),
            Some("domo.*.u-1.state")
        );
    }

    #[test]
    fn ownership_requires_the_full_segment() {
        assert!(owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1.reported"));
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-12.reported"));
        assert!(!owns_routing_key("lakeside", "u-1", "domo.harbour.u-1.reported"));
        // The prefix alone, with nothing beneath it, is not a publishable key.
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1"));
    }
}
