//! AXIAM calls the Rust SDK does not cover.
//!
//! Every public function here exists because the SDK cannot make the call, and
//! each one names the dogfooding finding that records why. Plan 01-02 Task 4
//! writes `docs/dogfooding-findings.md`; plan 01-07's `verify` gate checks that
//! every `DF-` id cited in this module resolves there.
//!
//! # This is a cookie-jar session, not a bearer token
//!
//! `POST /api/v1/auth/login` sets a session cookie and returns an
//! `X-CSRF-Token` header. Every subsequent *mutating* call must echo that token
//! back. Forgetting it does not look like a missing-CSRF error — it surfaces as
//! a bare `403`, which reads like an authorization bug and sends you looking in
//! the policy model. The token is captured once at login and echoed by
//! [`HandRolled::post`], [`HandRolled::put`] and [`HandRolled::delete`].

use anyhow::{Context, Result, bail};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

/// How an org-level principal names the tenant a request is about.
const ACTING_TENANT_HEADER: &str = "X-Axiam-Tenant";
const CSRF_HEADER: &str = "X-CSRF-Token";

/// A cookie-jar AXIAM session for the calls the SDK does not expose.
pub struct HandRolled {
    http: Client,
    base: String,
    csrf: Option<String>,
}

/// Outcome of the one-shot `/admin/bootstrap` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapOutcome {
    /// 201 — the organization and super-admin were created by this call.
    Created,
    /// 409 — already initialised; the existing super-admin stands.
    AlreadyInitialised,
    /// 403 — the setup-token gate was already consumed. Not fatal on its own:
    /// the caller falls through to probing `auth/login` with saved credentials.
    GateConsumed,
}

impl HandRolled {
    /// Build a session against `base_url`, trusting only the organization root.
    pub fn new(base_url: &str, root_pem: &[u8]) -> Result<Self> {
        let ca = reqwest::Certificate::from_pem(root_pem)
            .context("organization root is not valid PEM")?;
        let http = Client::builder()
            .use_rustls_tls()
            .add_root_certificate(ca)
            // The session lives in this jar; without it every call after login
            // is anonymous and the CSRF token alone will not save you.
            .cookie_store(true)
            .build()
            .context("building the hand-rolled HTTP client")?;
        Ok(Self {
            http,
            base: base_url.trim_end_matches('/').to_owned(),
            csrf: None,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn headers(&self, tenant: Option<Uuid>, mutating: bool) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        if mutating {
            if let Some(csrf) = &self.csrf {
                h.insert(
                    CSRF_HEADER,
                    HeaderValue::from_str(csrf).context("malformed CSRF token")?,
                );
            }
        }
        if let Some(t) = tenant {
            h.insert(
                ACTING_TENANT_HEADER,
                HeaderValue::from_str(&t.to_string()).context("malformed tenant id")?,
            );
        }
        Ok(h)
    }

    /// `POST /api/v1/admin/bootstrap` — first-run organization creation.
    ///
    /// DF-010: `/admin/bootstrap` is excluded from the SDK's management surface
    /// by CONTRACT §27.0, so first-run setup is necessarily hand-rolled.
    ///
    /// The deprecated `tenant_name`/`tenant_slug` fields are deliberately
    /// omitted: tenants are created explicitly by the `tenants` stage.
    ///
    /// DF-019: the setup token is emitted once per SurrealDB volume. A 403 here
    /// means the gate was already consumed — recoverable only by signing in
    /// with the saved super-admin credentials, or by wiping the volume.
    pub async fn bootstrap(
        &self,
        org_name: &str,
        org_slug: &str,
        email: &str,
        username: &str,
        password: &str,
        setup_token: Option<&str>,
    ) -> Result<BootstrapOutcome> {
        let mut body = json!({
            "organization_name": org_name,
            "organization_slug": org_slug,
            "email": email,
            "username": username,
            "password": password,
        });
        if let Some(tok) = setup_token {
            body["setup_token"] = json!(tok);
        }
        let resp = self
            .http
            .post(self.url("/api/v1/admin/bootstrap"))
            .json(&body)
            .send()
            .await
            .context("POST /api/v1/admin/bootstrap")?;

        match resp.status() {
            StatusCode::CREATED | StatusCode::OK => Ok(BootstrapOutcome::Created),
            StatusCode::CONFLICT => Ok(BootstrapOutcome::AlreadyInitialised),
            StatusCode::FORBIDDEN => Ok(BootstrapOutcome::GateConsumed),
            other => {
                // Never echo the body: it contains the password we just sent.
                bail!("POST /api/v1/admin/bootstrap returned {other}")
            }
        }
    }

    /// `POST /api/v1/auth/login` — establish the cookie-jar session.
    ///
    /// DF-008: an organization-level principal is the only identity that can
    /// administer tenant-scoped resources via `X-Axiam-Tenant`, and the Rust
    /// SDK sends no such header — which is why this session exists alongside
    /// the SDK client rather than instead of it.
    pub async fn login(
        &mut self,
        org_slug: &str,
        username_or_email: &str,
        password: &str,
    ) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/api/v1/auth/login"))
            .json(&json!({
                "org_slug": org_slug,
                "username_or_email": username_or_email,
                "password": password,
            }))
            .send()
            .await
            .context("POST /api/v1/auth/login")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("login as '{username_or_email}' returned {status}");
        }
        let csrf = resp
            .headers()
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .context("login succeeded but returned no X-CSRF-Token header")?;
        self.csrf = Some(csrf);
        Ok(())
    }

    /// True when this session has authenticated.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.csrf.is_some()
    }

    /// `GET` a JSON document, optionally acting on a tenant.
    pub async fn get(&self, path: &str, tenant: Option<Uuid>) -> Result<Value> {
        let resp = self
            .http
            .get(self.url(path))
            .headers(self.headers(tenant, false)?)
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("GET {path} returned {status}");
        }
        resp.json().await.with_context(|| format!("decoding GET {path}"))
    }

    /// `POST` a JSON body, echoing the CSRF token. Returns `(status, body)`.
    ///
    /// The body is returned rather than error-checked here because several
    /// callers treat `409` as success (the object already exists).
    pub async fn post(
        &self,
        path: &str,
        body: &Value,
        tenant: Option<Uuid>,
    ) -> Result<(StatusCode, Value)> {
        let resp = self
            .http
            .post(self.url(path))
            .headers(self.headers(tenant, true)?)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        let status = resp.status();
        let value = resp.json::<Value>().await.unwrap_or(Value::Null);
        Ok((status, value))
    }

    /// `PUT` a JSON body, echoing the CSRF token.
    pub async fn put(
        &self,
        path: &str,
        body: &Value,
        tenant: Option<Uuid>,
    ) -> Result<(StatusCode, Value)> {
        let resp = self
            .http
            .put(self.url(path))
            .headers(self.headers(tenant, true)?)
            .json(body)
            .send()
            .await
            .with_context(|| format!("PUT {path}"))?;
        let status = resp.status();
        let value = resp.json::<Value>().await.unwrap_or(Value::Null);
        Ok((status, value))
    }

    /// Provision a tenant administrator and grant it `super-admin` in that
    /// tenant.
    ///
    /// DF-008: four hand-rolled calls, each carrying `X-Axiam-Tenant`, because
    /// the SDK cannot address a tenant from an organization-level session. The
    /// resulting account is what [`crate::axiam::tenant_client`] logs in as, so
    /// that leaf issuance is signed by the right tenant's CA (P-11).
    ///
    /// Idempotent by probe: a `409` on creation falls back to a search by
    /// username rather than failing.
    pub async fn provision_tenant_admin(
        &self,
        tenant_id: Uuid,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<Uuid> {
        let t = Some(tenant_id);

        // 1. Create (or find) the user.
        let (status, body) = self
            .post(
                "/api/v1/users",
                &json!({ "username": username, "email": email, "password": password }),
                t,
            )
            .await?;
        let user_id = match status {
            StatusCode::CREATED | StatusCode::OK => body
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
                .context("user creation returned no usable id")?,
            StatusCode::CONFLICT => {
                let found = self
                    .get(&format!("/api/v1/users?search={username}"), t)
                    .await?;
                find_by(&found, "username", username)
                    .and_then(|u| u.get("id").and_then(Value::as_str))
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .context("tenant admin exists but could not be resolved by username")?
            }
            other => bail!("creating tenant admin returned {other}"),
        };

        // 2. Activate. Users are created `PendingVerification` and there is no
        //    mailbox to click a link in.
        let (status, _) = self
            .put(
                &format!("/api/v1/users/{user_id}"),
                &json!({ "status": "Active" }),
                t,
            )
            .await?;
        if !status.is_success() {
            bail!("activating the tenant admin returned {status}");
        }

        // 3. Find the tenant's seeded `super-admin` role.
        let roles = self.get("/api/v1/roles", t).await?;
        let role_id = find_by(&roles, "name", "super-admin")
            .and_then(|r| r.get("id").and_then(Value::as_str))
            .and_then(|s| Uuid::parse_str(s).ok())
            .context("tenant has no super-admin role")?;

        // 4. Assign it with no resource_id — a global grant *within this
        //    tenant*, and nothing outside it.
        let (status, _) = self
            .post(
                &format!("/api/v1/roles/{role_id}/users"),
                &json!({ "user_id": user_id.to_string() }),
                t,
            )
            .await?;
        match status {
            StatusCode::NO_CONTENT
            | StatusCode::OK
            | StatusCode::CREATED
            | StatusCode::CONFLICT => Ok(user_id),
            other => bail!("assigning super-admin returned {other}"),
        }
    }
}

/// Tolerate both `{"items": [...]}` and a bare `[...]` list.
///
/// AXIAM's list routes are not uniform about the envelope, and a caller that
/// assumes one shape silently finds nothing against the other.
#[must_use]
pub fn items(value: &Value) -> &[Value] {
    value
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .map_or(&[], Vec::as_slice)
}

/// Find the first list element whose `field` equals `wanted`.
#[must_use]
pub fn find_by<'a>(value: &'a Value, field: &str, wanted: &str) -> Option<&'a Value> {
    items(value)
        .iter()
        .find(|e| e.get(field).and_then(Value::as_str) == Some(wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_tolerates_both_envelopes() {
        let wrapped = json!({"items": [{"name": "a"}]});
        let bare = json!([{"name": "a"}]);
        assert_eq!(items(&wrapped).len(), 1);
        assert_eq!(items(&bare).len(), 1);
        assert_eq!(items(&json!({})).len(), 0);
    }

    #[test]
    fn find_by_matches_on_the_named_field() {
        let list = json!({"items": [{"name": "x", "id": "1"}, {"name": "y", "id": "2"}]});
        assert_eq!(
            find_by(&list, "name", "y").unwrap().get("id").unwrap(),
            "2"
        );
        assert!(find_by(&list, "name", "z").is_none());
    }
}
