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
/// translation could not carry across unambiguously — the translation is
/// **total**: every input either round-trips or is refused.
///
/// Refused, and why each one has to be:
///
/// - a level containing the routing separator, which would read as two levels
///   on the far side and make the key mean something the topic did not;
/// - a level containing the broker's single-level wildcard, which would read
///   as a wildcard rather than as the literal character;
/// - a wildcard sharing a level with anything else — MQTT gives a wildcard a
///   whole level or none of it;
/// - a multi-level wildcard anywhere but the final level, where MQTT is the
///   one refusing, not us;
/// - an empty level, and the empty topic.
///
/// ```
/// # use domo_common::topic::to_routing_key;
/// assert_eq!(to_routing_key("domo/lakeside/dev/#").as_deref(), Some("domo.lakeside.dev.#"));
/// assert_eq!(to_routing_key("domo/lakeside/dev.ice/x"), None);
/// ```
#[must_use]
pub fn to_routing_key(topic: &str) -> Option<String> {
    if topic.is_empty() {
        return None;
    }
    let levels: Vec<&str> = topic.split(MQTT_SEPARATOR).collect();
    let last = levels.len() - 1;
    let mut out: Vec<&str> = Vec::with_capacity(levels.len());
    for (i, level) in levels.iter().enumerate() {
        out.push(match *level {
            "" => return None,
            MQTT_SINGLE => ROUTING_SINGLE,
            MULTI if i == last => MULTI,
            // A `#` anywhere but the end is not a legal MQTT filter.
            MULTI => return None,
            other => {
                if other.contains(ROUTING_SEPARATOR)
                    || other.contains(ROUTING_SINGLE)
                    || other.contains(MULTI)
                    || other.contains(MQTT_SINGLE)
                {
                    return None;
                }
                other
            }
        });
    }
    Some(out.join(&ROUTING_SEPARATOR.to_string()))
}

/// True when `routing_key` names a topic the given device owns.
///
/// **Separator-aware by construction, not by a prefix comparison.** A prefix
/// comparison accepts `domo.lakeside.dev2.x` for device `dev` — the adjacency
/// bug, and the difference between a device seeing only its own topics and a
/// device seeing a neighbour whose identifier happens to extend its own. This
/// walks levels, so the boundary can only fall on a separator.
///
/// The comparison assumes the tenant slug and the account identifier contain
/// no separator themselves — guaranteed upstream by the slug validator and by
/// UUIDs. If one ever did, no level would match it and this would refuse
/// everything, which is the safe direction.
#[must_use]
pub fn owns_routing_key(tenant_slug: &str, sa_uuid: &str, routing_key: &str) -> bool {
    let mut levels = routing_key.split(ROUTING_SEPARATOR);
    if levels.next() != Some(TOPIC_ROOT)
        || levels.next() != Some(tenant_slug)
        || levels.next() != Some(sa_uuid)
    {
        return false;
    }
    let beneath: Vec<&str> = levels.collect();
    // The bare prefix names no topic: a device publishes *under* its
    // namespace, and a subscription to all of it carries the `#`.
    if beneath.is_empty() || beneath.iter().any(|l| l.is_empty()) {
        return false;
    }
    // `#` matches everything after it, so it is only meaningful last. One in
    // the middle either widens past what the level structure says or names a
    // scheme this one does not understand.
    !beneath[..beneath.len() - 1].contains(&MULTI)
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
    fn ownership_requires_the_full_level() {
        assert!(owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1.reported"));
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-12.reported"));
        assert!(!owns_routing_key("lakeside", "u-1", "domo.harbour.u-1.reported"));
        // The prefix alone, with nothing beneath it, is not a publishable key.
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1"));
        // Nor is it with an empty level beneath it.
        assert!(!owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1."));
        // A subscription to all of this device's own topics, however, is.
        assert!(owns_routing_key("lakeside", "u-1", "domo.lakeside.u-1.#"));
    }

    #[test]
    fn a_tenant_slug_carrying_a_separator_refuses_everything() {
        // The slug validator forbids this upstream. If it ever stopped, the
        // failure has to be closed rather than open.
        assert!(!owns_routing_key("lake.side", "u-1", "domo.lake.side.u-1.x"));
    }

    #[test]
    fn translation_refuses_what_it_cannot_carry() {
        assert_eq!(to_routing_key(""), None);
        assert_eq!(to_routing_key("a//b"), None);
        assert_eq!(to_routing_key("a/b.c/d"), None);
        assert_eq!(to_routing_key("a/b*c/d"), None);
        assert_eq!(to_routing_key("a/#/b"), None);
        assert_eq!(to_routing_key("a/b+/c"), None);
    }
}
