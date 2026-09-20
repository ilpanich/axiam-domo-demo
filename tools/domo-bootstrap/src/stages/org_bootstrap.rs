//! Stage 1 — create the organization and its super-admin.
//!
//! # The setup-token gate (D-12, P-3, DF-019)
//!
//! AXIAM emits a single-use setup token into its log exactly once per SurrealDB
//! volume. `just` scrapes it from the FULL log history (not `--since`, which
//! misses it after any restart), persists it at mode 0600 the moment it is
//! seen, and passes it here.
//!
//! Three outcomes are all success:
//!
//! * `201` — created by this call.
//! * `409` — already initialised; the existing super-admin stands.
//! * `403` — the gate was already consumed. This is NOT fatal on its own: if
//!   the saved super-admin can still sign in, the bootstrap that created it
//!   already happened and there is nothing to do.
//!
//! Only when the token is gone AND no saved credential signs in is the volume
//! genuinely unrecoverable — that is the one case that demands
//! `just demo-reset`, and it is reported in those words.

use anyhow::{Result, bail};

use domo_common::hand_rolled::{BootstrapOutcome, HandRolled};

use super::{Env, ok, step, super_admin_credentials};

pub async fn run(setup_token: Option<&str>) -> Result<()> {
    let env = Env::load()?;
    let creds = super_admin_credentials(&env.org_slug)?;
    let mut session = HandRolled::new(&env.axiam_url, &env.root_pem)?;

    // Probe first: signing in is the honest question. A 409 from the bootstrap
    // route is only reachable when the gate is an admin email, and inferring
    // "already done" from a status code is what made AXIAM's own script's
    // documented idempotence false for production-shaped stacks.
    step("probing whether the organization is already bootstrapped");
    if session
        .login(&env.org_slug, &creds.email, &creds.password)
        .await
        .is_ok()
    {
        ok("already bootstrapped (super-admin signs in) — nothing to do");
        domo_common::secrets::mark_done("org-bootstrap")?;
        return Ok(());
    }

    // Fall back to the saved token when the caller did not pass one.
    let saved;
    let token = match setup_token {
        Some(t) => Some(t),
        None => {
            saved = domo_common::secrets::read_string("state/setup-token").ok();
            saved.as_deref()
        }
    };

    step("calling /api/v1/admin/bootstrap");
    let outcome = session
        .bootstrap(
            "Domo Demo",
            &env.org_slug,
            &creds.email,
            &creds.username,
            &creds.password,
            token,
        )
        .await?;

    match outcome {
        BootstrapOutcome::Created => ok("organization and super-admin created"),
        BootstrapOutcome::AlreadyInitialised => ok("already initialised — reusing"),
        BootstrapOutcome::GateConsumed => {
            // Last chance: maybe the credentials changed under us.
            if session
                .login(&env.org_slug, &creds.email, &creds.password)
                .await
                .is_ok()
            {
                ok("gate already consumed, but the saved super-admin signs in");
            } else {
                bail!(
                    "reset required: run 'just demo-reset'. The setup-token gate is consumed \
                     and no saved credential can sign in, so this SurrealDB volume can no \
                     longer be bootstrapped (DF-019)."
                );
            }
        }
    }

    // Prove the credentials work before declaring the stage done, so a later
    // stage cannot be the first thing to discover they do not.
    if !session.is_authenticated() {
        session
            .login(&env.org_slug, &creds.email, &creds.password)
            .await?;
    }
    ok("super-admin session verified");
    domo_common::secrets::mark_done("org-bootstrap")?;
    Ok(())
}
