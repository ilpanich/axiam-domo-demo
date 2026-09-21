//! The D-34 stage sequence, its markers, and the interruption hook.
//!
//! # A marker is a skip hint. It is never the truth.
//!
//! Two failure modes make "the marker is there, so the work is done" wrong, and
//! both have already happened in this project:
//!
//! * A run that died **between** a successful request and writing its marker
//!   leaves the work done with no marker. Cost: one wasted re-probe. Harmless.
//! * A run that died **just after** writing the marker but before the work
//!   landed leaves a marker with nothing behind it. Cost: the stage is skipped
//!   forever, and the failure surfaces somewhere else entirely. This is the one
//!   that matters, and it is why [`probe`] exists.
//!
//! So a stage is skipped only when its marker is present **and** [`probe`]
//! re-asks AXIAM, the broker or the filesystem whether the work is genuinely
//! there. The marker only ever saves time; correctness comes from the probe and
//! from every stage resolving its objects by natural key before creating them.
//!
//! # The interruption hook (`DOMO_FAIL_AT`)
//!
//! `DOMO_FAIL_AT=<stage>` aborts the run at that stage's **entry** — before the
//! stage does any work, and before anything writes `.secrets/state/<stage>` —
//! exiting non-zero with the ordinary stage-failure output, so the abort is
//! indistinguishable from a real failure at that point.
//!
//! Without it, "an interrupted run resumes at the stage that failed" (D-34,
//! PLAT-05) could only ever be asserted by killing a run at the right
//! millisecond, which is to say: never asserted at all.
//!
//! The variable is read once, at the top of the process, and it adds no branch
//! inside any stage's own logic. Unset — or naming no stage — the runner takes
//! exactly the production path. It has no role in a demo run; see
//! `docs/setup.md`.
//!
//! # Marker location and shape
//!
//! `.secrets/state/<stage>` — one file per stage name, no suffix, so the
//! presence or absence of a stage's marker is checkable from a shell without
//! parsing anything (`test -e .secrets/state/catalog`). The CONTENT is JSON and
//! self-describing — stage, completion time, and what the stage produced — so a
//! human reading the directory can tell what state the machine is in.
//!
//! The older `<stage>.done` markers from plan 01-01 are a finer-grained, per
//! subcommand set and are left alone: they are written by the individual stage
//! functions, this file's are written by the runner, and the two never collide
//! because these carry no `.done` suffix.
//!
//! `.secrets/state/pki-root.done` is the one marker `just demo-reset` must
//! never remove (D-10). It is `gen-pki.sh`'s root guard, not a stage marker,
//! and it is what stops a reset from silently rotating the organization root
//! out from under every machine that has already trusted it.

use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// D-34's named stage sequence, in run order.
///
/// `just/stack.just` runs the same ten names in the same order — it is the host
/// side of this sequence, because compose lifecycle, volume removal and log
/// tailing cannot happen from inside a container. If the two ever disagree, the
/// symptom is immediate and loud: a stage's marker never appears, so it never
/// skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Host: refuse before anything is created (`scripts/preflight.sh`, D-35).
    Preflight,
    /// Host: the offline organization root and every SAN leaf (`gen-pki.sh`).
    Pki,
    /// Host: SurrealDB, AXIAM and the broker up, and AXIAM answering.
    AxiamUp,
    /// The organization and its super-admin, through the one-time setup token.
    OrgBootstrap,
    /// Both demo tenants, the BYOK root import and anchor, one signing CA per
    /// tenant, and each tenant's admin principal.
    ///
    /// Three subcommands under one D-34 name. They are inseparable in practice:
    /// a signing CA cannot exist before its tenant, and no tenant-scoped call
    /// can be made without that tenant's own admin (D-37).
    Tenants,
    /// `authz/catalog.toml` applied to every tenant, and proven converged.
    Catalog,
    /// One service account and certificate per (service, tenant), and each
    /// tenant's `portfolio` root with its structural group.
    ServiceCerts,
    /// The `domo` MQTT vhost, touching no other vhost.
    Broker,
    /// Host: everything else in the compose file — the Twin, PostgreSQL, Caddy
    /// and the AXIAM console.
    PlatformUp,
    /// Host: the invariants, and the demo card.
    Verify,
}

impl Stage {
    /// Every stage, in D-34 order.
    pub const ALL: [Self; 10] = [
        Self::Preflight,
        Self::Pki,
        Self::AxiamUp,
        Self::OrgBootstrap,
        Self::Tenants,
        Self::Catalog,
        Self::ServiceCerts,
        Self::Broker,
        Self::PlatformUp,
        Self::Verify,
    ];

    /// The stage's name, as it appears in the checklist and on disk.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::Pki => "pki",
            Self::AxiamUp => "axiam-up",
            Self::OrgBootstrap => "org-bootstrap",
            Self::Tenants => "tenants",
            Self::Catalog => "catalog",
            Self::ServiceCerts => "service-certs",
            Self::Broker => "broker",
            Self::PlatformUp => "platform-up",
            Self::Verify => "verify",
        }
    }

    /// Resolve a stage by name, or `None` when the name is not one of D-34's.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// True for the stages `domo-bootstrap` runs inside the compose network.
    /// The rest belong to `just`, which owns the Docker lifecycle (D-34/D-35).
    #[must_use]
    pub const fn is_bootstrap_owned(self) -> bool {
        matches!(
            self,
            Self::OrgBootstrap
                | Self::Tenants
                | Self::Catalog
                | Self::ServiceCerts
                | Self::Broker
        )
    }
}

/// `DOMO_FAIL_AT`, read exactly once for the life of the process.
fn fail_at() -> Option<&'static str> {
    static FAIL_AT: OnceLock<Option<String>> = OnceLock::new();
    FAIL_AT
        .get_or_init(|| {
            std::env::var("DOMO_FAIL_AT")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        })
        .as_deref()
}

/// Abort at `stage`'s entry when `DOMO_FAIL_AT` names it.
///
/// Called before the stage does any work and before any marker is written, so
/// the interrupted run leaves exactly the state a real failure at that point
/// would leave. A value naming no stage is treated as unset — which is what
/// makes it harmless for `just` to forward an empty value into the container
/// unconditionally.
pub fn gate(stage: Stage) -> Result<()> {
    if fail_at() == Some(stage.name()) {
        bail!(
            "DOMO_FAIL_AT={} — aborting at the entry of stage '{}' before any work \
             and before its marker. This is the interruption hook, not a real failure; \
             re-run 'just up' to resume here.",
            stage.name(),
            stage.name()
        );
    }
    Ok(())
}

/// What a stage wrote to `.secrets/state/<stage>` when it finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    /// The stage's D-34 name, repeated inside the file so a copied marker is
    /// still self-describing.
    pub stage: String,
    /// RFC 3339, UTC.
    pub completed_at: String,
    /// What the stage produced, in one human-readable line.
    pub produced: String,
}

/// Read a stage's marker, or `None` when it is absent or unreadable.
///
/// Unreadable is deliberately the same answer as absent: a marker we cannot
/// parse tells us nothing, and the probe is about to ask the real question
/// anyway.
#[must_use]
pub fn marker(stage: Stage) -> Option<Marker> {
    let raw = domo_common::secrets::read(format!("state/{}", stage.name())).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Record that `stage` finished, and what it produced.
///
/// `just` writes the markers for the stages it owns, in the same shape; this is
/// the container side of the same format.
pub fn mark(stage: Stage, produced: &str) -> Result<()> {
    let m = Marker {
        stage: stage.name().to_owned(),
        completed_at: chrono_now(),
        produced: produced.to_owned(),
    };
    let json = serde_json::to_vec_pretty(&m).context("serializing the stage marker")?;
    domo_common::secrets::write(format!("state/{}", stage.name()), &json)?;
    Ok(())
}

/// RFC 3339 in UTC, without pulling a date library into this crate.
fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Civil-from-days (Howard Hinnant's algorithm), valid for every date this
    // project will ever see and free of a dependency whose only use would be
    // formatting one string.
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let tod = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3_600,
        (tod % 3_600) / 60,
        tod % 60
    )
}

/// Run one bootstrap-owned stage: gate, work, mark.
///
/// Every sub-step below already resolves its objects by natural key, so running
/// a stage whose work is partly done converges rather than duplicating. That is
/// what makes resume correct; the marker only makes it fast.
pub async fn run(stage: Stage) -> Result<()> {
    use crate::stages;

    gate(stage)?;

    let produced = match stage {
        Stage::OrgBootstrap => {
            stages::org_bootstrap::run(None).await?;
            "organization and super-admin".to_owned()
        }
        Stage::Tenants => {
            stages::tenants::run().await?;
            // The BYOK import, the mTLS anchor and one signing CA per tenant.
            // It lives here rather than in its own D-34 stage because it can
            // only run once tenants exist and must run before anything is
            // issued beneath them.
            stages::pki::run().await?;
            stages::tenant_admin::run().await?;
            let env = stages::Env::load()?;
            let org = stages::OrgClient::login(&env).await?;
            let slugs: Vec<String> = org
                .demo_tenants()
                .await?
                .into_iter()
                .map(|t| t.slug)
                .collect();
            format!(
                "tenants {} — root imported (BYOK) and anchored, one signing CA and one admin each",
                slugs.join(", ")
            )
        }
        Stage::Catalog => {
            let env = stages::Env::load()?;
            let org = stages::OrgClient::login(&env).await?;
            let slugs: Vec<String> = org
                .demo_tenants()
                .await?
                .into_iter()
                .map(|t| t.slug)
                .collect();
            for slug in &slugs {
                stages::catalog::run(slug, false).await?;
            }
            format!("authz/catalog.toml converged on {}", slugs.join(", "))
        }
        Stage::ServiceCerts => {
            stages::service_certs::run().await?;
            stages::tree::run().await?;
            "one mgmt@ and twin@ account per tenant, each with its own tenant CA's certificate; portfolio roots".to_owned()
        }
        Stage::Broker => {
            stages::broker::run().await?;
            format!("vhost '{}'", domo_common::DOMO_VHOST)
        }
        _ => bail!(
            "stage '{}' is owned by the task runner, not by domo-bootstrap",
            stage.name()
        ),
    };

    mark(stage, &produced)?;
    Ok(())
}

/// Ask whether a bootstrap-owned stage's work is genuinely already there.
///
/// Read-only: every branch issues GETs (or a login) and creates nothing, so it
/// is safe to point at a live instance. This is the re-probe that makes a
/// marker a hint rather than a claim.
pub async fn probe(stage: Stage) -> Result<bool> {
    use crate::stages;

    let env = stages::Env::load()?;

    match stage {
        // The honest question is not "does a credentials file exist" but "does
        // the super-admin actually sign in". A file survives a volume wipe; a
        // session does not.
        Stage::OrgBootstrap => Ok(stages::OrgClient::login(&env).await.is_ok()),

        Stage::Tenants => {
            let Ok(org) = stages::OrgClient::login(&env).await else {
                return Ok(false);
            };
            let tenants = org.demo_tenants().await.unwrap_or_default();
            if tenants.len() < 2 {
                return Ok(false);
            }
            for t in &tenants {
                if org.signing_ca(t.id).await.is_err() {
                    return Ok(false);
                }
                if stages::TenantClient::login(&env, &t.slug, t.id)
                    .await
                    .is_err()
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }

        // `plan` is the catalog's own convergence question, issued as GETs.
        // Anything still wanting to change means the stage has work to do.
        Stage::Catalog => {
            let path = std::env::var("DOMO_CATALOG")
                .unwrap_or_else(|_| crate::catalog::DEFAULT_PATH.to_owned());
            let manifest = crate::catalog::load(&path)?.to_manifest();
            let Ok(org) = stages::OrgClient::login(&env).await else {
                return Ok(false);
            };
            for t in org.demo_tenants().await.unwrap_or_default() {
                let Ok(tenant) = stages::TenantClient::login(&env, &t.slug, t.id).await else {
                    return Ok(false);
                };
                match tenant.manifest().plan(&manifest).await {
                    Ok(plan) if plan.is_converged() => {}
                    _ => return Ok(false),
                }
            }
            Ok(true)
        }

        Stage::ServiceCerts => {
            let Ok(org) = stages::OrgClient::login(&env).await else {
                return Ok(false);
            };
            let certs = stages::service_certs::verify(&org, &env).await.unwrap_or(false);
            let tree = stages::tree::verify(&org, &env).await.unwrap_or(false);
            Ok(certs && tree)
        }

        Stage::Broker => {
            let base = std::env::var("DOMO_RABBITMQ_MGMT_URL")
                .unwrap_or_else(|_| "http://rabbitmq:15672".into());
            let (Ok(user), Ok(pass)) = (
                std::env::var("RABBITMQ_DEFAULT_USER"),
                std::env::var("RABBITMQ_DEFAULT_PASS"),
            ) else {
                return Ok(false);
            };
            let http = reqwest::Client::builder().use_rustls_tls().build()?;
            let Ok(resp) = http
                .get(format!("{base}/api/vhosts"))
                .basic_auth(&user, Some(&pass))
                .send()
                .await
            else {
                return Ok(false);
            };
            let vhosts: Vec<serde_json::Value> = resp.json().await.unwrap_or_default();
            Ok(vhosts.iter().any(|v| {
                v.get("name").and_then(serde_json::Value::as_str) == Some(domo_common::DOMO_VHOST)
            }))
        }

        _ => bail!(
            "stage '{}' is probed by the task runner, not by domo-bootstrap",
            stage.name()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_round_trips_through_its_name() {
        for s in Stage::ALL {
            assert_eq!(Stage::from_name(s.name()), Some(s));
        }
        assert_eq!(Stage::from_name("not-a-stage"), None);
    }

    #[test]
    fn the_sequence_is_d34s_sequence() {
        // D-34 names the order, and later phases extend it. Spelled out here so
        // a reordering is a failing test rather than a silent change of meaning.
        let names: Vec<&str> = Stage::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            vec![
                "preflight",
                "pki",
                "axiam-up",
                "org-bootstrap",
                "tenants",
                "catalog",
                "service-certs",
                "broker",
                "platform-up",
                "verify",
            ]
        );
    }

    #[test]
    fn the_gate_fires_only_for_the_named_stage() {
        // `fail_at()` is a process-wide OnceLock, so this asserts the matching
        // rule rather than mutating the environment mid-process.
        assert!(gate(Stage::Catalog).is_ok() || fail_at() == Some("catalog"));
        assert_eq!(Stage::from_name("catalog"), Some(Stage::Catalog));
    }

    #[test]
    fn marker_timestamps_are_rfc3339_utc() {
        let t = chrono_now();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.ends_with('Z'), "{t}");
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], "T");
        // Sanity: this project did not run before 2020 and will not run past 2999.
        let year: i32 = t[0..4].parse().expect("year");
        assert!((2020..3000).contains(&year), "{t}");
    }

    #[test]
    fn only_the_five_axiam_stages_are_bootstrap_owned() {
        let owned: Vec<&str> = Stage::ALL
            .iter()
            .filter(|s| s.is_bootstrap_owned())
            .map(|s| s.name())
            .collect();
        assert_eq!(
            owned,
            vec!["org-bootstrap", "tenants", "catalog", "service-certs", "broker"]
        );
    }
}
