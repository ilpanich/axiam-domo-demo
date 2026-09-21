//! How a staged run looks from the outside (D-34).
//!
//! The user's own sketch of the wanted output is the specification:
//!
//! ```text
//! ✓ pki       root kept (SHA256 3F:A1:…)
//! ✗ catalog   409 on role 'installer' (summit)
//! re-run 'just up' to resume at: catalog
//! ```
//!
//! Three things make that read well, and all three are easy to lose:
//!
//! 1. **One line per stage**, with the marker first, so the eye scans a column
//!    rather than a paragraph.
//! 2. **A short status that says what happened**, not that something happened.
//!    "root kept" and "root generated" are different facts; "pki ok" is neither.
//! 3. **A failure names the stage, the cause and the way back.** A stack trace
//!    with no resume hint makes the reader guess whether re-running is safe.
//!
//! The vocabulary (`→` running, `✓` succeeded, `✗` failed, `…` a note) is the
//! one `just/stack.just`, `scripts/verify-pki.sh` and `just edge-verify`
//! already use, because the user's sketch uses it and because one run of
//! `just up` crosses all four.
//!
//! # Where the halves meet
//!
//! `just` owns the Docker lifecycle, so it prints the host-side stages and
//! tails the failing container's log (D-34/D-35 — a container cannot read its
//! neighbour's logs). This module owns the vocabulary both halves share, the
//! whole-sequence status a human can ask for at any time, and, from Task 3, the
//! demo card.

use anyhow::Result;

use crate::state::{Marker, Stage, marker};

/// A stage has started, and what it is about to do.
pub fn running(stage: Stage) {
    println!("→ {}  {}", stage.name(), headline(stage));
}

/// One line saying what a stage is for.
///
/// Printed while the stage runs, so a slow stage says what it is waiting on
/// rather than leaving a bare name on the screen. `just` prints the same text
/// for the stages it owns.
#[must_use]
pub const fn headline(stage: Stage) -> &'static str {
    match stage {
        Stage::Preflight => "disk, tools, host and ports",
        Stage::Pki => "the offline organization root and every SAN leaf",
        Stage::AxiamUp => "SurrealDB, AXIAM and the broker",
        Stage::OrgBootstrap => "the organization and its super-admin",
        Stage::Tenants => "tenants, the BYOK root import, signing CAs and tenant admins",
        Stage::Catalog => "the role and permission catalog, applied and converged",
        Stage::ServiceCerts => "service accounts, their certificates and the portfolio roots",
        Stage::Broker => "the 'domo' MQTT vhost",
        Stage::PlatformUp => "the Twin, PostgreSQL, Caddy and the AXIAM console",
        Stage::Verify => "the invariants, and the demo card",
    }
}

/// A stage finished, and what it produced.
pub fn succeeded(stage: Stage, status: &str) {
    println!("✓ {}  {}", stage.name(), status);
}

/// A stage failed.
///
/// Prints the stage, the cause and the way back. The caller — `just`, which is
/// the only side that can — follows this with the last lines of
/// [`log_container`]'s log.
pub fn failed(stage: Stage, cause: &str) {
    println!("✗ {}  {}", stage.name(), cause);
    if let Some(c) = log_container(stage) {
        println!("  the container to look at: {c}  (just logs --tail 40 {c})");
    }
    println!("  re-run 'just up' to resume at: {}", stage.name());
}

/// The container whose log explains a failure in `stage`, when there is one.
///
/// A stage that talks to AXIAM fails for reasons AXIAM logged; a broker stage
/// fails for reasons the broker logged. Naming the wrong one sends the reader
/// to a quiet log and teaches them the hint is useless.
#[must_use]
pub const fn log_container(stage: Stage) -> Option<&'static str> {
    match stage {
        Stage::AxiamUp
        | Stage::OrgBootstrap
        | Stage::Tenants
        | Stage::Catalog
        | Stage::ServiceCerts => Some("axiam-server"),
        Stage::Broker => Some("rabbitmq"),
        Stage::PlatformUp | Stage::Verify => Some("caddy"),
        // Neither runs against a container: preflight refuses before anything
        // exists, and the PKI is generated offline on the host.
        Stage::Preflight | Stage::Pki => None,
    }
}

/// Print where this machine stands: one line per stage, from its marker.
///
/// Answers the question an operator actually has in front of a machine someone
/// else left — "what has been done here?" — from the self-describing markers
/// rather than from a run log that has scrolled away.
pub fn status() {
    println!("→ stages");
    let mut done = 0;
    for stage in Stage::ALL {
        match marker(stage) {
            Some(Marker {
                completed_at,
                produced,
                ..
            }) => {
                done += 1;
                println!("  ✓ {:<14} {completed_at}  {produced}", stage.name());
            }
            None => println!("  … {:<14} not done", stage.name()),
        }
    }
    println!(
        "  {done}/{} stage(s) complete. A marker is a skip hint: every stage re-probes \
         before it decides.",
        Stage::ALL.len()
    );
}

/// Print the whole-sequence status, for `domo-bootstrap checklist`.
pub fn run() -> Result<()> {
    status();
    Ok(())
}
