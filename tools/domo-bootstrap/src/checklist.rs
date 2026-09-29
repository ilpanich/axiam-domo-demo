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

use anyhow::{Context, Result};

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

/// The demo card (D-36): everything a presenter needs, on one screen.
///
/// Printed at the end of a successful run AND written to
/// `.secrets/demo-card.txt`, because the terminal it was printed to is usually
/// not the one open when the demo starts.
///
/// # Mode
///
/// It carries the super-admin password, so it is written through
/// `domo_common::secrets::write`, which opens at 0600 and refuses any path
/// outside the git-ignored, docker-ignored `.secrets/` tree. Printing it to the
/// operator's own terminal is intended — it is the presenter's credential.
///
/// # Shape
///
/// The sections are separated by `## ` headings and the seeded-user section is
/// last and empty, so a later phase appends its users by adding lines under
/// that heading rather than by reformatting the card (D-36).
///
/// `host` and `fingerprint` are supplied by `just`, which is the side that
/// knows the operator's `.env` and holds `dist/trust/domo-root.sha256` — the
/// file `just export-trust` has already checked against the in-use root.
pub fn demo_card(host: &str, fingerprint: &str) -> Result<()> {
    let raw = domo_common::secrets::read("axiam/super-admin.json").context(
        "no .secrets/axiam/super-admin.json — the org-bootstrap stage has not run yet",
    )?;
    let creds: serde_json::Value =
        serde_json::from_slice(&raw).context("super-admin.json is malformed")?;
    let email = creds["email"].as_str().unwrap_or("<unknown>");
    let password = creds["password"].as_str().unwrap_or("<unknown>");

    let card = format!(
        "\
╭──────────────────────────────────────────────────────────────────────╮
│  AXIAM Domo Demo — demo card                                         │
╰──────────────────────────────────────────────────────────────────────╯

## Where to go

  Portal       https://{host}/
  Console      https://axiam.{host}/     (the real AXIAM admin console)

## Who to sign in as

  super-admin  {email}
  password     {password}

  This is the organization super-admin. It is used by the bootstrap and by
  the console; no service in the demo holds it.

## The trust anchor

  Root fingerprint (SHA256)
    {fingerprint}

  The same value appears on the portal's front door and in
  dist/trust/domo-root.sha256. All three must agree — if they do not, stop.

  To trust it on this machine or on a laptop you are presenting from:

    just export-trust        then follow the steps it prints, or docs/trust.md

## Next steps

  just checklist     where this machine stands, stage by stage
  just verify        the fast gate: build, tests, PKI, edge, database, authz
  just smoke-gate    the live authorization and device-connect suite, attributed
  just demo-reset    wipe and rebuild everything except the root, between runs

  If {host} does not resolve on the machine you are presenting from, publish
  it (avahi-publish, or /etc/hosts) — `just preflight` prints the command.

## One expected failure, so it does not surprise you on stage

  `just smoke` FAILS two of twelve cases, on purpose. Both are confirmed defects
  in AXIAM itself, not in this demo:

    cross-tenant-ca-issuance   DF-017   AXIAM signs a leaf under another
                                        tenant's signing CA
    other-tenant-ca            DF-025   that leaf then connects end to end

  Together: certificate issuance is not a tenant boundary on the pinned build.
  AXIAM 1.0.0-beta17 fixes the cause (T22.1); that fix is NOT verified here,
  because the beta17 images were never published. Run `just smoke-gate` rather
  than `just smoke` — same suite, but it says which failures are these two and
  which would be news. Full story: docs/dogfooding-upstream-status.md.

## Seeded users

  (none yet — the Management Platform's seed arrives in Phase 2 and appends
  its property managers, installers, concierges and residents here)
"
    );

    print!("{card}");
    let path = domo_common::secrets::write_string("demo-card.txt", &card)?;
    println!("  ✓ written to {} (owner-only)", path.display());
    Ok(())
}
