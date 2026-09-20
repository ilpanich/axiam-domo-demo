//! Apply `authz/catalog.toml` to one tenant, or show what applying it would
//! do (D-16).
//!
//! # Why there is no exists-check loop here
//!
//! The SDK's management manifest is declarative and idempotent by
//! construction, and this stage leans on that rather than reimplementing it:
//!
//! - **It deletes nothing.** §27.6 rule 4 forbids deleting without a
//!   per-namespace opt-in, and the SDK offers none — so a catalog that omits
//!   an existing role leaves that role alone. The catalog is a statement about
//!   the roles the demo needs, not about the roles a tenant may have.
//! - **A field the catalog does not state is never a difference**, which keeps
//!   `apply` safe against a tenant that also holds hand-made state.
//! - **Applying twice converges**: the second `plan` is all no-change. That is
//!   the PLAT-05 idempotence check, asserted below on every apply rather than
//!   taken on trust.
//! - **A broken manifest is refused before the first request.** Our own rules
//!   — no wildcard, no dangling action — are checked first by
//!   [`crate::catalog::Catalog::validate`], which names the offending entry.
//!
//! # What the manifest cannot carry (DF-011)
//!
//! Resource metadata, a resource-scoped group-to-role binding, and service
//! accounts are all outside its expressive range. The resource tree, the
//! structural groups and the service accounts are therefore created
//! imperatively, in `stages/tree.rs` and `stages/service_certs.rs`. If a later
//! SDK release closes that gap, this split becomes redundant rather than
//! wrong.

use anyhow::{Context, Result, bail};

use super::{Env, OrgClient, TenantClient, ok, step};
use crate::catalog;

/// Apply the catalog to `tenant_slug`, or only report what would change.
///
/// `plan_only` issues GETs and nothing else — safe to point at a live tenant.
pub async fn run(tenant_slug: &str, plan_only: bool) -> Result<()> {
    let env = Env::load()?;
    let path = std::env::var("DOMO_CATALOG").unwrap_or_else(|_| catalog::DEFAULT_PATH.to_owned());

    // Validation happens here, before anything is logged in: a wildcard or a
    // dangling action must never cost a network round trip to discover.
    let catalog = catalog::load(&path)?;
    let manifest = catalog.to_manifest();

    let org = OrgClient::login(&env).await?;
    let tenant_id = org.tenant_id(tenant_slug).await?;
    let tenant = TenantClient::login(&env, tenant_slug, tenant_id).await?;

    if plan_only {
        step(&format!("planning the catalog for '{tenant_slug}'"));
        let plan = tenant
            .manifest()
            .plan(&manifest)
            .await
            .with_context(|| format!("planning the catalog for '{tenant_slug}'"))?;
        report(&plan, tenant_slug);
        return Ok(());
    }

    step(&format!(
        "applying {} permissions and {} roles to '{tenant_slug}'",
        catalog.permissions.len(),
        catalog.roles.len()
    ));
    let applied = tenant
        .manifest()
        .apply(&manifest)
        .await
        .with_context(|| format!("applying the catalog to '{tenant_slug}'"))?;
    if let Some((action, message)) = applied.failure() {
        // There is no transaction across AXIAM's management endpoints, so an
        // apply that stops part-way leaves what it already did in place. Fix
        // the cause and re-apply: the steps that landed become no-change.
        bail!(
            "applying the catalog to '{tenant_slug}' failed at {:?} '{}': {message}",
            action.target,
            action.key
        );
    }
    ok(&format!(
        "catalog applied to '{tenant_slug}' ({} item(s) changed)",
        applied.changed()
    ));

    // The convergence assertion, not a formality: a re-plan that still wants
    // to change something means the catalog and AXIAM disagree about what the
    // catalog says, and every later phase resolves through these names.
    step(&format!("re-planning '{tenant_slug}' to prove convergence"));
    let replan = tenant
        .manifest()
        .plan(&manifest)
        .await
        .with_context(|| format!("re-planning the catalog for '{tenant_slug}'"))?;
    if !replan.is_converged() {
        for a in replan.changes() {
            println!("  ✗ catalog  {:?} {:?} {}", a.change, a.target, a.key);
        }
        bail!(
            "the catalog did not converge for '{tenant_slug}': {} item(s) still differ \
             after a successful apply (D-16, PLAT-05)",
            replan.change_count()
        );
    }
    ok(&format!(
        "catalog converged for '{tenant_slug}': {} items, all no-change",
        replan.actions.len()
    ));

    domo_common::secrets::mark_done(&format!("catalog-{tenant_slug}"))?;
    Ok(())
}

/// Print a plan.
///
/// A converged plan prints ONE line and names no verb. That is deliberate:
/// `just catalog-plan` is grepped for the absence of create/update/delete, so
/// a summary that said "0 created" would fail its own check.
fn report(plan: &axiam_sdk::management::manifest::ManagementPlan, tenant_slug: &str) {
    if plan.is_converged() {
        ok(&format!(
            "catalog converged for '{tenant_slug}': {} items, all no-change",
            plan.actions.len()
        ));
        return;
    }
    for a in plan.changes() {
        println!("  ! catalog  {:?} {:?} {}", a.change, a.target, a.key);
    }
    println!(
        "  ! catalog  {} of {} item(s) differ for '{tenant_slug}'",
        plan.change_count(),
        plan.actions.len()
    );
}
