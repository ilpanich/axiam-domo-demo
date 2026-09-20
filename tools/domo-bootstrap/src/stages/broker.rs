//! Stage 5 — create the `domo` MQTT vhost.
//!
//! D-26 is a prohibition as much as a task: AXIAM's own `/` vhost, its
//! entrypoint-created default user and its `auth_backends.1 = internal` path
//! must survive untouched. This stage therefore does exactly two things —
//! `GET /api/vhosts` to see what exists, and `PUT /api/vhosts/domo` if ours is
//! missing. It never enumerates-and-reconciles, never deletes, and never
//! imports a declarative definitions file, because every one of those would put
//! AXIAM's vhost at risk.

use anyhow::{Context, Result, bail};

use super::{ok, step};

pub async fn run() -> Result<()> {
    let base = std::env::var("DOMO_RABBITMQ_MGMT_URL")
        .unwrap_or_else(|_| "http://rabbitmq:15672".into());
    let user = std::env::var("RABBITMQ_DEFAULT_USER")
        .context("RABBITMQ_DEFAULT_USER is not set (generated — run 'just up')")?;
    let pass = std::env::var("RABBITMQ_DEFAULT_PASS")
        .context("RABBITMQ_DEFAULT_PASS is not set (generated — run 'just up')")?;

    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .context("building the RabbitMQ management client")?;

    step("checking the broker's vhosts");
    let resp = http
        .get(format!("{base}/api/vhosts"))
        .basic_auth(&user, Some(&pass))
        .send()
        .await
        .context("GET /api/vhosts on the RabbitMQ management API")?;
    if !resp.status().is_success() {
        bail!("GET /api/vhosts returned {}", resp.status());
    }
    let vhosts: Vec<serde_json::Value> =
        resp.json().await.context("decoding the vhost list")?;
    let names: Vec<&str> = vhosts
        .iter()
        .filter_map(|v| v.get("name").and_then(serde_json::Value::as_str))
        .collect();

    // A sanity check, not a repair: if AXIAM's own vhost has gone missing,
    // something upstream destroyed it and creating ours would paper over that.
    if !names.contains(&"/") {
        bail!(
            "AXIAM's default vhost '/' is missing from the broker; refusing to continue (D-26)"
        );
    }

    if names.contains(&domo_common::DOMO_VHOST) {
        ok("vhost 'domo' already exists — reusing");
    } else {
        step("creating vhost 'domo'");
        let put = http
            .put(format!("{base}/api/vhosts/{}", domo_common::DOMO_VHOST))
            .basic_auth(&user, Some(&pass))
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("PUT /api/vhosts/domo")?;
        if !put.status().is_success() {
            bail!("PUT /api/vhosts/domo returned {}", put.status());
        }
        ok("vhost 'domo' created");
    }

    // The device's MQTT permissions are decided per operation by the Twin's
    // HTTP backend, so no static per-user permission grant is written here.
    domo_common::secrets::mark_done("broker")?;
    Ok(())
}
