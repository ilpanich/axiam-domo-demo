# Architecture Research

**Domain:** Multi-tenant IoT home-automation demo built on the AXIAM IAM server
**Researched:** 2026-09-19
**Confidence:** HIGH for AXIAM behavior (verified against `/home/emanuele/git/priv/axiam` source, docs and schema on this date). MEDIUM for Device Twin / MQTT / frontend patterns, which are general IoT/web design applied to this project rather than AXIAM-verified facts.

This document validates the architecture already proposed in `.planning/PROJECT.md`, corrects one load-bearing mismatch found in the AXIAM authorization schema, and fills in the flow-level detail the roadmap needs. It does not re-litigate decisions already made in PROJECT.md's Key Decisions table.

---

## 1. AXIAM resource and role model — verified against source

Verified against `crates/axiam-core/src/models/{resource,role,permission}.rs`, `crates/axiam-authz/src/{engine,types}.rs`, `crates/axiam-db/src/schema.rs`, `crates/axiam-db/src/repository/role.rs`, `docs/pki/README.md`, `crates/axiam-pki/src/mtls.rs`.

### 1.1 Cascading — confirmed, mechanism is ancestor-walk, not database triggers

`Resource.parent_id` forms a tree (`child_of` edges). `AuthorizationEngine::evaluate` (`crates/axiam-authz/src/engine.rs`) resolves cascading like this on every check:

1. Fetch the subject's `has_role` edges (direct + group-inherited), each carrying its own optional `resource_id`.
2. Fetch `get_ancestors(tenant_id, resource_id)` for the resource being checked.
3. `applicable_role_ids()` keeps an assignment if: the **role** is `is_global`, OR the assignment names **no** resource (tenant-wide), OR the assignment's `resource_id` equals the target resource **or any of its ancestors**.
4. Grants (permission + effect) are fetched only for the resulting role IDs and matched against the requested action/scope.

So: **a role assignment scoped to a resource cascades to every descendant of that resource**, confirming PROJECT.md's resource-tree design. This is computed per-request (no materialized closure table), which is fine at demo scale (resource trees are ≤5 levels deep, 114 devices/tenant) but means every `CheckAccess` call does 3–4 sequential round-trips unless batched — see §1.5.

Deny-override is real and unconditional: a matched `Deny` grant short-circuits and wins over any `Allow` at any depth or specificity (`evaluate_grants`, extensively fuzz-tested in `engine.rs`'s own test suite, including an exhaustive "adding a deny can never widen access" property test). This confirms PROJECT.md's reasoning for keeping the design deny-free (denies would also block the resident→installer grant flow, since deny wins at any depth).

### 1.2 Action/permission naming — confirmed

`Permission.action` is a free-form string, and `AccessRequest.action` doc comment gives the convention explicitly: **`resource:verb`**, e.g. `"users:create"`. PROJECT.md's `device:operate`, `device:manage`, `device:configure`, `structure:*`, `member:assign`, `grant:manage`, `twin:report`, `command:receive`, `intercom:call` all follow this convention correctly. `structure:*` as literally written is **not** a wildcard the engine understands — `grant_applies` does exact string equality on `action`. Either register `structure:*` as one literal permission action name (fine, if every "structure" verb is checked with that exact string) or, more likely, register one permission per real verb (`structure:create`, `structure:update`, `structure:delete`) and grant all of them to `property-manager`. Flag this ambiguity for the phase that implements the seed/permission registry.

Scopes (`PermissionGrant.scope_ids`) are a second, optional axis — sub-resource-level grants (e.g. limiting an allow to a named scope within one resource) that inherit down the hierarchy exactly like resource-scoped roles do. Nothing in PROJECT.md's design currently needs scopes; permissions are resource-scoped at the granularity of the resource tree itself, which is enough. Keep scopes as a later lever (e.g. per-thermostat-setting scopes) rather than using them for anything in v1.

### 1.3 Same role at several resources — **NOT directly possible; this is a real mismatch**

This is the most important finding in this document.

`has_role` (SurrealDB `RELATE subject -> has_role -> role`) carries index `idx_has_role_unique UNIQUE(in, out)` — unique on **(subject, role)**, not on **(subject, role, resource_id)**. `resource_id` is only a field on that one edge. The role repository's own doc comment says it outright: *"`has_role` carries a `UNIQUE(in, out)` index, so a subject holds a given role at most once."*

Consequence: **a single user (or service account) can hold a given named role at only one resource scope at a time.** A second `RELATE ... -> has_role -> role:X` for the same (subject, role) pair returns `409 Conflict` (`classify_write_error`, pinned by `idx_has_role_unique`).

This directly conflicts with several assumptions embedded in PROJECT.md's role table if read literally:

- **`installer`**, described as "assigned on a site or an apartment (per assignment)": if one installer is assigned to two sites (or a site and an apartment), a second `installer` assignment for that same installer collides.
- **`concierge`**, "assigned on common nodes of assigned sites" (plural sites for one concierge): same collision.
- **`granted-operator`**, assigned "to an installer when a resident grants access": if the *same* installer is granted device access by residents of **two different apartments** (plausible in the demo — one installer, multiple grants over the course of the guided script), the second grant's `RELATE` collides with the first.

The last one is the one most likely to actually bite during the demo, because "resident grants an installer" is one of the four key moments and is very likely to be exercised more than once against the same installer across the seeded scenario.

**The correct AXIAM-native pattern, and it already exists in the schema:** groups. `member_of` (`user/service_account -> member_of -> group`) is a *separate* relation, also `UNIQUE(in, out)` but keyed on **(subject, group)**, so one subject can join many different groups. A group is itself just another `has_role` subject, and `get_user_role_assignments` walks group-inherited roles and preserves each edge's own `resource_id` (confirmed in `crates/axiam-db/src/repository/role.rs`), so a role assigned to a group scoped to resource A behaves identically, at check time, to the same role assigned directly to a user scoped to resource A.

**Recommended fix for the Management Platform's sync design:** for every role that a subject might need at more than one resource — `installer`, `concierge`, and critically `granted-operator` — create **one group per (role, resource) pair** rather than assigning the role to the user directly:

- `installer@site:{id}` group holds `has_role → installer` scoped to `site:{id}`; the installer user is added via `member_of` to one such group per site/apartment they're assigned to.
- `concierge@site:{id}` group, same pattern.
- `granted-operator@device:{id}` group per grant (or, more simply, keep `granted-operator` a **global role definition** and create one group per apartment-device grant, adding the installer as a member — revoking the grant removes the membership, not the role edge, so it never touches other grants held by the same installer).

This adds one extra AXIAM entity type to provision and clean up (groups), and reconciliation on `demo-reset` must delete groups, not just role assignments. It also means the Management Platform's domain model needs a `resource_type + resource_id → axiam_group_id` mapping table (or a deterministic naming scheme it can reconstruct), which is exactly the kind of thing the sync design in §2 should carry. This should be logged as a dogfooding finding: assigning the same role to the same subject at multiple resources is not supported directly and requires the group indirection, which is not what a first read of the resource/role docs suggests.

**Roles that are naturally single-resource per subject don't need this workaround**: `property-manager` (assigned once, at the tenant/portfolio root — `is_global`-style tenant-wide assignment) and `resident` (assigned once, at their one apartment — a resident with two apartments is out of scope for the seed data) are fine as direct role assignments.

### 1.4 Device service account + Device cert — verified, with one doc/code discrepancy to flag

- A device is provisioned as a `service_account` in AXIAM (same principal kind as any other machine identity — `get_user_role_assignments` explicitly resolves both `user` and `service_account` tables for the same subject id, and `RequirePermission` treats them identically for RBAC purposes).
- The Management Platform (holding a CSR-signing / cert-issuing service account with `certificates:generate`) calls `POST /api/v1/certificates/sign-csr` with `cert_type: "Device"` and the device's self-generated CSR. AXIAM verifies the CSR's self-signature, reads subject/key straight off the CSR (refusing any SAN/keyUsage/EKU extensions in the request), and returns a plain `Certificate` with **no private key** — the device's private key never left the device/simulator host.
- **Binding the cert to the service account is a separate, required step**, `POST /api/v1/service-accounts/{sa_id}/bind-certificate {certificate_id}`. `docs/pki/README.md` states this bind step is *not* needed for `Device`-type certificates ("IoT device (Device-type) certificates do not use this bind step... looks it up directly [by fingerprint]"), but the actual code disagrees: `DeviceAuthService::authenticate_der` (`crates/axiam-pki/src/mtls.rs`) calls `cert_repo.get_bound_service_account(cert.id)` and returns `"certificate is not bound to a service account"` if that lookup is empty — reading the *same* `cert_bound_to` edge that `bind_to_service_account` writes for `Service`-type certs. **Treat the doc as wrong and always call `bind-certificate` for Device certs too**, immediately after `sign-csr`, or `POST /api/v1/auth/device` will fail for every provisioned device. Record this as a dogfooding finding (doc/code mismatch, with a concrete repro: sign a Device CSR, skip the bind step, attempt `/auth/device` mTLS login — 401/"certificate is not bound to a service account").
- At login (`POST /api/v1/auth/device`, mTLS, or the reverse-proxy `X-Client-Certificate` header path per `AXIAM__AUTH__TRUST_FORWARDED_CLIENT_CERT` — see §5), AXIAM computes the SHA-256 fingerprint of the presented cert, looks it up globally (fingerprint is unique across tenants), checks `Active` + unexpired, cryptographically verifies the chain to the issuing CA, requires that chain reach a CA explicitly flagged `mtls_trust_anchor`, then resolves the bound service account and mints an access token scoped to that service account's tenant. This is the identity a device uses for `twin:report`/`command:receive` grants (§1 role table) and for the Twin's `check_as` calls.
- **Native mTLS vs. proxy-forwarded identity**: `AXIAM__AUTH__TRUST_FORWARDED_CLIENT_CERT` (default `false`) lets a reverse proxy that itself terminated the client TLS handshake pass the verified certificate via `X-Client-Certificate`. The docs are explicit that this is unsafe unless the proxy is the *only* thing that can reach the listener and always overwrites the header. **Given Caddy is a single origin fronting browser traffic in this design, do not route device MQTT/mTLS traffic through Caddy at all** — devices should reach AXIAM's REST listener (for `/auth/device`) and the RabbitMQ MQTT+TLS listener directly on the LAN, terminating their own TLS with rustls/OpenSSL/libcurl mTLS, never through the browser-facing proxy. This matches PROJECT.md's diagram (Simulator PC → MQTT over mTLS, not through Caddy) — good, no change needed, just confirmed as the *safer* of two options AXIAM itself warns about.

### 1.5 Practical engine notes worth carrying into the roadmap

- `CheckAccess` is 3–4 sequential DB round-trips per call; `BatchCheckAccess` (gRPC) coalesces shared lookups and is meaningfully faster when checking several device commands at once (e.g. a "turn off all lights" bulk action) — prefer batching in the Twin wherever a single user gesture fans out to multiple device checks.
- `AccessDecision` is 3-valued at the wire level (`Allow` / `Deny("no roles assigned"/"no_grant"/...)` / `DeniedByRule(...)`), with two distinct reason codes `no_grant` and `denied_by_rule`. PROJECT.md already calls these out for the denial UI — confirmed as the exact reason codes the SDKs surface (`reason_code()` in `axiam-authz/src/types.rs`).
- Tenant isolation is structural, not just a filter: `AccessRequest.tenant_id` decides which resource tree is even visible, and a cross-tenant assignment (organization-scope principal) only carries **global** (unscoped) role assignments across the tenant boundary — a resource-scoped assignment never crosses tenants. This is exactly the guarantee the demo's tenant-isolation moment needs, and it is enforced in the engine, not left to the app.

---

## 2. Sync strategy — Management Platform PostgreSQL ↔ AXIAM resources/roles

AXIAM exposes no domain-specific hooks for this; this section is general distributed-systems design applied to the constraint set (local demo, single Postgres, single AXIAM, must survive `demo-reset`, must not deadlock the four key moments).

### 2.1 Recommendation: synchronous calls with a local outbox for retry, not full async messaging

At demo scale (single machine, 114 devices, a handful of concurrent admin actions) a message queue/CDC pipeline is over-engineering and adds a whole failure mode (a stuck outbox worker silently leaving AXIAM out of sync) that is worse for a *demo* than a slightly slower synchronous path. Recommended pattern:

1. **Every domain mutation that has an AXIAM side-effect is a single `@Transactional` unit** in the Management Platform: write the Postgres row(s) and record an **outbox row** (`axiam_sync_outbox`: operation type, target AXIAM entity type, payload, status, attempts, last_error) in the same DB transaction.
2. **Immediately after commit, attempt the AXIAM call synchronously**, in the same request, using the org-scope service account over mTLS. On success, mark the outbox row `done` in a second, tiny transaction. This keeps the common path simple (one request, done) and gives the caller a real error if AXIAM rejects it (e.g., the 409 from §1.3) rather than papering over it with "eventually consistent."
3. **On failure** (AXIAM unreachable, 5xx, or a benign 409 from a double-submit), leave the outbox row `pending`/`failed` and let a lightweight background retry loop (a single scheduled task, not a broker) sweep it with backoff. Because REST calls are POST/PUT/DELETE on identity-bearing paths (create-or-bind a specific resource/role/cert), make every outbox handler **idempotent**: check-then-act ("does resource X already exist under this parent?" before creating), and treat AXIAM's 409 on a duplicate create/RELATE as success, not failure, when retrying. This matters doubly given §1.3 — a retry that blindly re-POSTs a role assignment must not be surprised by a 409.
4. **Ordering**: the outbox is per-aggregate-ordered (e.g. all outbox rows for one apartment are applied in creation order) but cross-aggregate operations can run concurrently — there is no global ordering requirement because AXIAM's resource tree only ever grows top-down (site before building before apartment before device) and the Management Platform's own foreign keys already enforce that a child domain row cannot be created before its parent, so the outbox rows it produces are naturally emitted in a safe order already.
5. **Deletes go through the same outbox**, but resource deletion in AXIAM should be modeled as **archive-then-delete-on-reset** rather than eager delete, because deleting a resource with a live cascading role assignment on it is exactly where an inconsistency would show up mid-demo (a lingering session still holding a decision cached — see the decision cache/invalidation section of `axiam-authz`, which the app does not control directly but which is why role revocation handlers should call `invalidate_subject`/`invalidate_tenant` via AXIAM's own mutation endpoints rather than assuming propagation is instant across processes if the decision cache is ever turned on for the demo. Recommendation: **leave `AXIAM__AUTHZ__DECISION_CACHE_ENABLED` off** for the demo — it defaults to off, and turning it on trades a small perf win for a real risk of "I revoked and it's still allowed for a few seconds" during a live demo of the revoke moment).

### 2.2 Reconciliation on `demo-reset`

`demo-reset` is specified as wipe-everything-and-reseed, which is the easy case for consistency (no partial-state reconciliation needed) but the *hard* case for TLS bootstrap (§5) and for AXIAM entity IDs (they will be fresh UUIDs every reset, so nothing in the Management Platform's schema should hardcode an AXIAM ID; always store the mapping in its own tables and rebuild it).

Recommended sequence for `just demo-reset`:

1. Stop the Management Platform and Device Twin so nothing writes mid-reset.
2. Truncate the Management Platform's and Twin's own Postgres schemas.
3. Re-run the AXIAM-side bootstrap from scratch (org, tenants, org CA, PKI trust anchor, seed roles/permissions, org-scope service account) — do **not** attempt to diff against AXIAM's existing state; wipe AXIAM's own datastore (SurrealDB volume) too, since PROJECT.md already treats `demo-reset` as re-issuing every certificate, which implies a fresh org CA, which invalidates every previously issued cert anyway.
4. Re-run the Management Platform's seed script, which walks the domain seed data top-down (tenants → sites → buildings → apartments → devices → memberships → grants) and, for each entity, performs the AXIAM-side create/assign **synchronously inline** (this is a batch job, not a live request path, so pure synchronous calls with the outbox purely as an audit/retry log are fine and simpler to reason about for a from-scratch reset).
5. Re-run device provisioning (CSR generation + `sign-csr` + `bind-certificate`) for every simulated device, so simulator hosts pick up fresh certs (see §4).
6. Bring the Management Platform and Twin back up.

Because this is a destructive from-scratch reset rather than incremental reconciliation, there is no need for a generic "diff AXIAM state against Postgres and fix drift" reconciler in v1 — that would be real engineering effort spent on a failure mode (partial drift during normal operation) that the chosen sync strategy (synchronous + idempotent outbox retries) already prevents from accumulating. Flag a generic reconciliation job as an explicit **out-of-scope / future work** item rather than silently omitting it.

---

## 3. Device Twin design

Nothing here is AXIAM-specific except the two integration points (gRPC `CheckAccess` for user commands, REST `check_as` for device-originated actions); the rest is standard IoT-shadow/pub-sub design sized to the demo's scale (114 devices, one Twin process, LAN-local).

### 3.1 Shadow model

Keep it a single Postgres table per device (`device_shadow`), not a document store — the demo doesn't need schema flexibility across device types beyond a JSONB payload column, and Postgres is already the chosen store:

```
device_shadow(
  device_id UUID PK,
  tenant_id UUID,
  device_type TEXT,           -- light | thermostat | indoor_intercom | outdoor_intercom
  reported JSONB NOT NULL,    -- last state the device told us
  desired  JSONB NOT NULL,    -- last state a user asked for
  version  BIGINT NOT NULL,   -- monotonic, incremented on every write to reported OR desired
  reported_at TIMESTAMPTZ,    -- last time the device published a report
  online BOOLEAN NOT NULL,
  updated_at TIMESTAMPTZ
)
```

- **`version`** increments on any change to `reported` or `desired`, independent of each other — a classic AWS-IoT-shadow-style single version counter is enough at this scale; there's no need for separate reported/desired version numbers.
- **Delta** is not stored — compute it on demand as `desired \ reported` (JSONB key diff) whenever a command is issued or a UI subscribes, and publish it to the device as the command payload alongside (or instead of) the specific verb. For the demo's simple state machines (on/off, dim %, RGB, target temp, mode) a delta is trivially small; don't build a generic JSON-patch delta engine, just diff the handful of known keys per device type.
- Command dispatch flow: user command → Twin validates via `CheckAccess` → Twin updates `desired` (version++) → Twin publishes the resulting delta on the device's command topic → device applies it and publishes its own `reported` update → Twin updates `reported` (version++) and clears the acknowledged part of the delta.

### 3.2 MQTT topic layout

Namespace by tenant first (tenant isolation must be visible in the topic tree, not just enforced by ACLs, so a broker-side authorization bug fails closed rather than silently leaking across tenants):

```
domo/{tenant_id}/{device_id}/reported     # device → Twin, retained
domo/{tenant_id}/{device_id}/desired      # Twin → device, retained (last-desired, for a reconnecting device)
domo/{tenant_id}/{device_id}/cmd          # Twin → device, NOT retained (point-in-time command + a correlation id)
domo/{tenant_id}/{device_id}/cmd/ack      # device → Twin, NOT retained (correlation id + result)
domo/{tenant_id}/{device_id}/status       # LWT target, retained (see §3.4)
domo/{tenant_id}/{device_id}/event        # device → Twin, NOT retained (intercom `call` events, disturbances)
```

- Retained `reported` and `desired` topics mean a Twin restart (or a late-subscribing UI) can rebuild current state from the broker without replaying history — useful given the "no per-device containers, ≤4GB total" resource budget makes a full event-sourced history store unattractive.
- Client identity for ACL purposes should be the device's mTLS cert CN/fingerprint (RabbitMQ's MQTT plugin can gate topic access per authenticated connection); PROJECT.md already defers the exact broker-authorization mechanism (OAuth2 JWT backend vs. an HTTP auth backend the Twin implements) to phase research — this document doesn't resolve it either, but notes that whichever is chosen, **the topic namespace above should be enforced by the broker itself** (pattern-matched ACLs scoped to `domo/{own_tenant_id}/{own_device_id}/*`), not only by the Twin trusting whatever tenant/device id is embedded in a message — a compromised or misconfigured simulator device should not be able to publish into another tenant's topic space even if the Twin would otherwise ignore it.

### 3.3 Command acknowledgement and timeouts

- Every command the Twin sends on `cmd` carries a `correlation_id` (UUID) and the command payload.
- The Twin tracks in-flight commands in memory (a small map keyed by `correlation_id`, TTL-bounded) and marks a command `timed_out` if no `ack` (or no corresponding `reported` delta collapse) arrives within a fixed window — 5s is generous for LAN-local simulators; make it configurable per device type since thermostats simulate gradual physical change and intercoms should ack near-instantly.
- On timeout, the Twin does **not** retry automatically (a stale retried command against a device that's simply slow, not offline, could double-apply, e.g. toggling a light back off) — it surfaces `timed_out` to the SSE stream and lets the user re-issue.
- A command against a device already known `offline` (§3.4) should fail fast with a `device_offline` reason without ever publishing to MQTT.

### 3.4 Offline detection via MQTT Last Will and Testament

- Each device connects with an LWT message on `domo/{tenant_id}/{device_id}/status` set to `{"online": false}` (or a bare `offline` payload), retained, delivered by the broker if the device's connection drops uncleanly (crash, network loss, `kill -9` in the sim-control "take offline" action).
- On clean connect, the device immediately publishes `{"online": true}` to the same topic (also retained), overwriting the LWT payload the broker would otherwise still be holding from a *previous* session's will.
- The Twin subscribes to `domo/+/+/status` (or per-tenant `domo/{tenant_id}/+/status`) and updates `device_shadow.online` on every message — this is the single source of truth for the UI's online/offline badge, and it is a lot more reliable at demo scale than the Twin trying to infer liveness from a reported-heartbeat timeout, given the sim-control panel needs to be able to force a device offline (which for the simulator is easiest to implement as literally closing the MQTT connection without a clean DISCONNECT, exercising the exact LWT path a real device dropout would).
- Keep the broker's `keepalive`/session-expiry short (well under the demo's attention span, e.g. 15–30s) so an unclean disconnect is reflected as offline quickly rather than only after a long MQTT keepalive grace period — this is a broker/client config knob, not something AXIAM touches.

### 3.5 SSE fan-out filtered by what each user may see

- One SSE endpoint per portal session (`GET /api/twin/stream`), authenticated with the same JWT as REST calls (SSE over the reverse-proxied HTTPS connection, no separate auth mechanism).
- On connect, the Twin does **not** try to precompute "every resource this user can see" as a static list — instead, it filters the fan-out per event using the same `CheckAccess`/`BatchCheckAccess` the command path uses: when a shadow update or decision-feed entry is about to be pushed to a connection, check whether that connection's user still has (at minimum) an implicit "can see" grant on the device/resource in question. For the demo's scale (single-digit concurrent portal sessions, 114 devices), doing a live `BatchCheckAccess` per event batch is cheap and — importantly — it means a resident→installer grant/revoke change takes effect on the *feed* immediately and consistently with the command path, without maintaining a second, hand-rolled "visibility" cache that could drift from AXIAM's own answer.
- Practical implementation: batch shadow-update events by (subject, resource) per SSE tick (e.g. every 250ms) rather than checking per-event-per-connection synchronously on the hot path, to avoid one `CheckAccess` round-trip per device per connected user per update. Reuse `device:operate`-style checks (a user who can operate a device can certainly see it) as the visibility gate — don't invent a separate `device:view` permission unless a phase specifically needs "read but not operate," which nothing in PROJECT.md's requirements currently needs.

### 3.6 Access decision feed capture and streaming

- The natural capture point is the Twin's own `CheckAccess`/`check_as` call sites (every user command, every device-originated `check_as`) — wrap those calls in a small helper that, regardless of outcome, appends a `{timestamp, subject, action, resource, tenant, decision, reason_code}` row to an in-memory ring buffer (bounded, e.g. last 500 entries) **and** pushes it onto the same SSE fan-out as shadow updates, on a dedicated `event: decision` SSE event type so the frontend can route it to the Access-decision-feed component distinctly from shadow updates.
- Persisting the feed to Postgres is optional for the demo (a ring buffer survives a Twin restart poorly, but the feed's job is live storytelling during the guided demo, not an audit trail — AXIAM's own audit log, if enabled, is the durable record). Recommend: **in-memory ring buffer only for v1**, revisit persistence only if the dogfooding findings need a durable decision trail.
- The Management Platform also calls `CheckAccess`/AXIAM REST endpoints for its own domain mutations (e.g., `grant:manage` before creating a `granted-operator` assignment) — those decisions belong on the same feed conceptually, but since the feed is currently scoped to the Twin (which is what streams to the UI), either (a) have the Management Platform push its own decision events to the Twin over a small internal REST call (`POST /api/twin/internal/decision-events`) for the feed to relay, or (b) keep the feed device-operation-only in v1 and note staff-console grant-management decisions are visible via their own success/failure UI feedback rather than the shared feed. (a) is more faithful to "every AXIAM decision" in the requirement; note it as the richer option for the roadmap to choose from rather than deciding it here.

---

## 4. Device provisioning flow (CSR via AXIAM PKI) and simulator discovery

Confirmed end-to-end against `docs/pki/README.md` and `crates/axiam-pki/src/mtls.rs` (§1.4 above covers the AXIAM-side mechanics). The full flow:

1. A property manager/installer/resident adds a device in a portal → Management Platform creates the domain row, the AXIAM `resource` (`device:{id}`, parented under the right common-area or apartment node), and an AXIAM `service_account` for the device (plus whichever `device-self`/`intercom:call` role assignment per §1.1's role table — direct assignment is fine here since a device service account only ever needs one role at its own single resource).
2. The Management Platform records the new device (tenant, type, assigned simulator host) so the right simulator host can discover it. **Simulator discovery is polling, not push**, given PROJECT.md's constraint that simulator hosts run on a separate machine on the LAN and devices "come online without restarts": each simulator host (C/lights, C++/intercoms, Rust/thermostats) runs with its own AXIAM service account (`fleet-manager` style, scoped by device type) and periodically (e.g. every 5–10s) calls a Management Platform REST endpoint `GET /api/mgmt/devices?type=light&simulator_host=lights-c&since=<cursor>` to fetch devices assigned to it that it doesn't yet have a live process for. A short poll interval is fine at this scale and avoids building a push/notification channel to a possibly-offline simulator host just for this.
3. For each newly discovered device, the simulator host generates its own keypair locally (never sent anywhere) and a PKCS#10 CSR for `CN=device:{id}` (or an equivalent identifying subject), then calls the Management Platform (not AXIAM directly — the simulator host's own credentials are a fleet-level service account, not one with `certificates:generate`) with the CSR.
4. The Management Platform, using its own org/tenant-scope PKI service account, calls `POST /api/v1/certificates/sign-csr {issuer_ca_id, csr_pem, cert_type: "Device", validity_days, metadata}` against the tenant's signing CA, then **immediately** `POST /api/v1/service-accounts/{sa_id}/bind-certificate` (§1.4 — required despite the doc's claim otherwise) to bind the returned certificate to the device's service account, then returns the signed certificate (public only) to the simulator host.
5. The simulator host now holds: its own private key (never transmitted), the AXIAM-signed certificate, and the exported org root CA (fetched once at simulator-host startup, or bundled at bootstrap time — see §5). It calls `POST /api/v1/auth/device` over mTLS to get an access token, then connects to the MQTT broker using the same client certificate for the MQTT TLS handshake (RabbitMQ's MQTT listener, `domo` vhost).
6. From this point the device behaves as a normal `device-self` principal: it publishes `reported`/`status`, subscribes to `cmd`/`desired`, and (for outdoor intercoms) publishes `event` calls that the Twin authorizes via `check_as`.

**Revocation/decommission**: deleting a device in a portal should revoke its certificate (`POST /api/v1/certificates/{id}/revoke`) as part of the same outbox-driven flow (§2), not just delete the AXIAM resource — a device whose resource was deleted but whose cert is still `Active` could otherwise still authenticate (fingerprint lookup doesn't check whether the bound service account's resource still exists) and would then simply fail every `CheckAccess` downstream, which is a confusing failure mode to hit live during a demo compared to a clean, immediate 401 at `/auth/device`.

---

## 5. TLS bootstrap — feasible, and AXIAM's own reload behavior changes the recommended sequence

PROJECT.md's two-stage bootstrap (temporary localhost cert → real org-CA-issued certs, restart everything) is **feasible and is a documented pattern AXIAM itself uses on its own Raspberry Pi k3s runbook** (`docs/deployment/rpi5-k3s.md` §4.2: `axiam-selfsigned → axiam-ca`, a self-signed bootstrap issuer for the very first cert AXIAM's own listener presents, before any org CA exists to issue a "real" one). Confirmed against `docs/deployment/README.md`'s TLS-termination section and `crates/axiam-server/src/tls.rs`.

Key facts that refine the sequence:

- **AXIAM can terminate TLS itself** (`AXIAM__SERVER__TLS__ENABLED=true` + `CERT_PATH`/`KEY_PATH`), which the demo needs anyway since "every certificate in the demo is issued by the AXIAM organization CA" includes AXIAM's own server cert.
- **AXIAM reloads its own server leaf without a restart.** It re-reads the cert/key pair on `SIGHUP` and on a poll (`AXIAM__SERVER__TLS__RELOAD_INTERVAL_SECS`, default 3600s, configurable down for the demo — e.g. 5s during the bootstrap window only, or just rely on `SIGHUP` from the bootstrap script) and swaps it behind an `ArcSwap` that rustls consults per handshake — no restart, no dropped connections, and a malformed/mismatched pair (e.g. caught mid-write) is logged and ignored, keeping the previous cert live. **This means the "AXIAM starts with a bootstrap cert, then gets its real cert" step does not require restarting the AXIAM container** — the bootstrap script can drop the new org-CA-issued cert+key into the mounted volume and send `SIGHUP` to the `axiam-server` process (or `docker kill -s HUP <container>`), and it takes effect immediately.
- **AXIAM's client-certificate trust store (for verifying devices/services over native mTLS) also reloads without restart**, via a different, database-driven mechanism: flagging an org CA `mtls-trust-anchor` (`PUT /api/v1/organizations/{org_id}/ca-certificates/{id}/mtls-trust-anchor`) rebuilds the client-CA bundle from the DB and hot-swaps a `ReloadableClientCertVerifier` — again no restart, and it sets `CLIENT_AUTH=optional` automatically (never `required`, so password/passkey/browser logins keep working even after devices start authenticating by cert on the same listener).
- **This reload story does NOT extend to AXIAM's dependencies.** `docs/deployment/rpi5-k3s.md` §4.3 states plainly that RabbitMQ and Vault "need a pod restart" to pick up a renewed certificate — they read `tls_cert_file`/equivalent once at listener startup with no reload path. So in this demo: after the org CA issues RabbitMQ's broker cert, **RabbitMQ's container must be restarted** to pick it up (this is a Compose `docker compose restart rabbitmq`, cheap and expected). The same is true for PostgreSQL (reads its TLS material at startup) and, absent custom code, for the Java (Spring Boot/Tomcat) and Rust (Actix) service TLS listeners in the Management Platform and Twin — neither framework hot-reloads a server keystore/cert out of the box, so those two also need a restart (or at minimum a graceful re-bind) after their AXIAM-issued certs arrive. **Caddy is the one other component that does hot-reload gracefully** (`caddy reload` re-parses config and swaps certs with zero dropped connections), so if Caddy's own cert is AXIAM-issued, the bootstrap script should use `caddy reload` rather than restarting the Caddy container.

Revised bootstrap sequence for `just demo-reset` / first-run:

1. Generate a throw-away, self-signed, localhost-only bootstrap keypair for `axiam-server` (openssl one-liner, or reuse AXIAM's own dev-mode default if it ships one — check at implementation time; otherwise a `just` recipe generates it). Start AXIAM with `TLS_ENABLED=true` pointed at this pair, `CLIENT_AUTH=off`. Every *other* service starts plaintext or with its own throwaway/dev cert for now.
2. Bootstrap AXIAM's data plane over this bootstrap TLS connection (still fine — it's the AXIAM API talking to itself over localhost, not yet exposed to browsers/devices): create the organization, the two tenants, seed roles/permissions/resources per tenant, and generate the **organization root CA** (`POST /organizations/{org_id}/ca-certificates`) plus per-tenant signing CAs if the design wants that extra tier (optional; PROJECT.md doesn't require it, a single org CA issuing tenant-scoped leaves directly is simpler and sufficient at this scale).
3. Flag the org CA `mtls-trust-anchor: true` — AXIAM immediately starts trusting client certs chaining to it (no restart).
4. Issue a server leaf for `axiam-server` itself under the org CA (subject matching whatever hostname/SAN the demo uses — likely just `localhost`/the LAN IP for a local demo, no real DNS needed), drop it into the mounted cert path, `SIGHUP` the AXIAM process. AXIAM is now serving its **real**, org-CA-chained certificate, without a restart.
5. Issue server leaves for Caddy, RabbitMQ, PostgreSQL, the Management Platform and the Twin, the same way. `caddy reload` for Caddy; `docker compose restart <service>` for RabbitMQ, PostgreSQL, the Management Platform and the Twin (all four need a restart per the framework limitations above — batch these restarts together at the end of bootstrap rather than one at a time, since the Management Platform/Twin aren't serving real traffic yet at this point in first-run bootstrap anyway).
6. Export the org root's public certificate and distribute it to: the browser trust story (documented manual "add this root" step for the demo machines, since there's no public CA involved and this is explicitly LAN-only), and the simulator PC (so its mTLS clients can validate AXIAM's/RabbitMQ's presented server certs).
7. Proceed with device provisioning (§4) — every device cert issued from this point is already under the trusted, reloaded-in org CA.

This sequence is fully compatible with `just demo-reset` re-running it from scratch each time (a fresh org CA each reset, hence PROJECT.md's own note that reset re-issues every certificate) — nothing here assumes cert stability across resets. It is also the section most likely to generate real dogfooding findings (the RabbitMQ/Postgres/JVM/Actix restart requirement is annoying friction worth recording, and the doc/code mismatch on `SIGHUP` vs. documented poll-only behavior for other services, if any is found during implementation, should be logged too).

---

## 6. Frontend architecture

Verified against `sdks/CONTRACT.md` §3–§5 and §23 (cookie/CSRF/tenant-context contract every AXIAM SDK — including `axiam-sdk-wasm` — must follow) and PROJECT.md's own architecture diagram.

### 6.1 Two portals + sim control in a pnpm workspace

PROJECT.md's shape (Staff console, Resident app, Sim control page, all React+TS+Vite in a pnpm workspace with a shared package) is a standard, low-risk monorepo layout. Recommended package split:

```
frontend/
├── packages/
│   ├── axiam-client/       # thin wrapper around axiam-sdk-wasm: login, refresh, can(), tenant context
│   ├── twin-client/        # typed REST + SSE client for the Device Twin API
│   ├── mgmt-client/        # typed REST client for the Management Platform API
│   ├── ui/                 # shared components: device tiles, decision-feed panel, grant manager
│   └── device-icons/       # (optional) shared device-type visuals
└── apps/
    ├── staff-console/      # property manager / installer / concierge
    ├── resident-app/       # resident
    └── sim-control/        # simulator control panel
```

Three independent Vite apps sharing packages via pnpm workspace `link:` resolution is enough; there's no need for a meta-framework (Next/Remix) — this is a LAN demo with no SSR/SEO requirement, and PROJECT.md already commits to static-served SPAs behind Caddy.

### 6.2 Token handling after OPAQUE login, including refresh

The SDK contract (§3–§5, §23–24) fixes this precisely, and it is **cookie-based, not the app manually storing bearer tokens** — this simplifies the frontend a lot:

- `POST /api/v1/auth/login` (and the OPAQUE/passkey finish equivalents) sets three cookies: `axiam_access` and `axiam_refresh` (both `httpOnly`) and `axiam_csrf` (readable, for double-submit), and returns the same fields in the body too, but for a browser SDK the *cookies* are the source of truth for auth — the WASM SDK's `loginOpaque()` should be treated as "log the browser in," not "return a token the app must store."
- Every request the app makes to the Management/Twin APIs must run with `credentials: "include"` (fetch) so the browser attaches `axiam_access` automatically; for state-changing requests (`POST`/`PUT`/`PATCH`/`DELETE`) the app must read the non-httpOnly `axiam_csrf` cookie via a small regex against `document.cookie` (per §3, exactly as `frontend/src/lib/api.ts` in the AXIAM repo itself does) and echo it as an `X-CSRF-Token` header.
- **Refresh**: `POST /api/v1/auth/refresh` reads the refresh token from the `axiam_refresh` cookie directly — there is no body/param to pass a token. The app's HTTP layer should implement the standard "on 401, attempt one silent refresh, then retry the original request once" interceptor pattern, with a single-flight guard (the SDK contract has its own §9 "Single-Flight Refresh Guard" requirement for non-browser SDKs; for the browser case, a simple in-memory promise-dedup in the shared `axiam-client` package achieves the same thing so concurrent 401s from several in-flight requests don't each trigger their own refresh call).
- Because tokens live in cookies scoped to AXIAM's own origin, **this is precisely why the single-origin Caddy reverse proxy matters technically, not just for CORS convenience**: the cookies must be visible to `/api/mgmt`, `/api/twin` and `/axiam` under the same registrable domain/port for the Management Platform and Twin to see `axiam_access` on incoming requests. If any of those three paths were served from a different origin, the cookie would not be sent and the whole cookie-based auth model would break. Caddy terminating everything under one origin (`/staff`, `/resident`, `/api/mgmt`, `/api/twin`, `/axiam`) is a hard requirement, not a nicety, given this contract — confirm the Caddyfile routes all four paths under one scheme+host+port with no `Access-Control-*` exposure needed at all (no cross-origin requests exist in this design).
- Tenant/org context (§5, §5.1) is a **required constructor parameter** for every AXIAM client, forwarded as `X-Tenant-ID` on every request and, for login/refresh specifically, as `org_id`/`org_slug` in the body. Each portal knows its tenant at build/config time (or resolves it from the logged-in user's session on first load) — there is no tenant-switcher UI needed for this demo (each user belongs to exactly one tenant; only an org-scope service account crosses tenants, and that's a backend-only concern, never exposed to a browser session).
- `can()` in the WASM SDK is a client-side convenience for **UI gating only** (show/hide a button) — PROJECT.md is already correct that real enforcement is server-side (`CheckAccess` in the Management Platform/Twin). Don't let `can()` become a second source of truth the UI trusts for anything beyond hiding controls a server call would reject anyway.

### 6.3 Calling Management and Twin APIs through Caddy

Route table (mirrors PROJECT.md's diagram, made explicit as Caddyfile-shaped intent):

```
https://demo.local {
    handle /staff/*      { reverse_proxy staff-console-static }
    handle /resident/*   { reverse_proxy resident-app-static }
    handle /sim/*         { reverse_proxy sim-control-static }
    handle /api/mgmt/*   { reverse_proxy management-platform:8080 }
    handle /api/twin/*   { reverse_proxy device-twin:8081 { flush_interval -1 } }  # SSE needs unbuffered proxying
    handle /axiam/*      { reverse_proxy axiam-server:8090 }
}
```

- SSE (`/api/twin/stream`) needs `flush_interval -1` (or Caddy's equivalent no-buffering directive) so events aren't batched by the proxy — a common SSE-through-reverse-proxy pitfall worth calling out explicitly for the phase that builds this.
- Static portal bundles are served by Caddy directly (`file_server` off a build output directory) rather than proxied to a Node dev server in production/demo mode — matches PROJECT.md's "static portals served by Caddy" and the ≤4GB Pi budget (no Node runtime needed at demo time, only at build time).
- Caddy itself needs a server cert from the AXIAM CA (§5) for the public HTTPS listener; that's the one edge of this whole system actually facing "users" (browsers on the LAN), so it's the cert operators/browsers on the demo machines need to trust.

---

## 7. Suggested build order and component boundaries

### 7.1 Component boundaries (summary table)

| Component | Owns | Never does |
|---|---|---|
| AXIAM | Identity, tenants, resource tree, roles/grants, PKI, OPAQUE/passkey auth | Domain knowledge (sites/buildings/apartments/devices are just typed resources to it) |
| Management Platform | Domain CRUD, AXIAM resource/role/cert sync (outbox), device provisioning orchestration | Device runtime state (that's the Twin's), MQTT |
| Device Twin | Shadow state, command dispatch, MQTT bridge, SSE fan-out, decision feed | Domain CRUD, device provisioning, cert issuance |
| Simulator hosts | Virtual device behavior, local keypair generation, CSR submission | Talking to AXIAM directly for provisioning (goes through the Management Platform) |
| Portals | Presentation, OPAQUE login via WASM SDK, `can()` UI gating | Authorization decisions (always re-checked server-side) |
| Caddy | Single-origin TLS termination for browser traffic, static asset serving, SSE-aware proxying | Device/mTLS traffic (that goes direct to AXIAM/RabbitMQ, never through Caddy) |
| PostgreSQL | Management + Twin schemas | AXIAM's own state (SurrealDB, untouched) |
| RabbitMQ (`domo` vhost) | Device↔Twin MQTT, TLS+client-cert transport | AXIAM's own AMQP traffic (separate vhost, untouched) |

### 7.2 Build order

The dependency chain is dictated by what each later piece needs to already exist and be provable in isolation:

1. **AXIAM bootstrap + PKI, headless (no other services yet).** Stand up AXIAM alone (Docker Compose, plaintext or its own dev TLS), script org/tenant/CA creation, verify `sign-csr` + `bind-certificate` + `/auth/device` mTLS login work end-to-end against a hand-crafted CSR (no simulator yet — a `curl`/`openssl` script is enough). This is the foundation everything else assumes, and it is also where the §1.3 and §1.4 findings need to be re-verified against a live instance before committing to the group-based role-assignment workaround in code.
2. **TLS bootstrap sequence, end to end (§5).** Prove the self-signed → org-CA-issued → `SIGHUP`-reload path for AXIAM's own listener, and the restart-required path for RabbitMQ/Postgres, on both target machines (XPS and Pi) — this is infrastructure every other component depends on and the biggest source of "works on my machine" risk (arm64 vs amd64 image builds, k3s-flavored docs not directly transferable to Compose).
3. **Riskiest integration slice #1 — MQTT mTLS end to end with exactly one simulated device.** Before building three simulator hosts and a full Twin, prove: RabbitMQ `domo` vhost + MQTT plugin + TLS listener, one device cert issued and bound per §4, one minimal script/binary that connects over MQTT+mTLS, publishes a `reported` message, and a minimal Twin stub that receives it and answers a `CheckAccess`-gated command back. This is the demo's most novel integration (AXIAM doesn't ship MQTT support itself — the PROJECT.md correctly identifies this as needing its own vhost/plugin work) and the one most likely to surface topology or cert-chain problems that are cheap to fix with one device and expensive to debug across 114.
4. **Riskiest integration slice #2 — OPAQUE WASM login through Caddy, cookie round-trip.** In parallel with (3) if capacity allows: stand up Caddy fronting AXIAM alone, get a trivial static page calling `loginOpaque()` via `axiam-sdk-wasm`, confirm the `axiam_access`/`axiam_refresh`/`axiam_csrf` cookies land correctly under the single origin and that a subsequent authenticated call (even just `GET /axiam/api/v1/auth/me` proxied through Caddy) succeeds. This validates the single-origin cookie assumption in §6.2 before any portal UI is built on top of it, and is the other genuinely novel integration (WASM+OPAQUE+reverse-proxy cookie scoping) worth de-risking early rather than discovering a cookie-domain mismatch after two portals are built.
5. **Management Platform core: domain CRUD + AXIAM sync outbox (§2), without device provisioning yet.** Sites/buildings/apartments/memberships, mirrored into AXIAM resources and role assignments (including the group-based workaround from §1.3 for installer/concierge/granted-operator). This is where tenant isolation and the resident→installer grant/revoke key moment become provable.
6. **Device provisioning end-to-end (§4), now integrated into the Management Platform**, replacing the hand-rolled script from step 1/3 — device add in the UI → resource+service-account creation → simulator discovery poll → CSR → sign → bind → mTLS login → MQTT connect, with one real simulator host (pick the simplest, e.g. Rust/thermostats, since it's the same language as the Twin and lowest integration friction) before building the other two.
7. **Device Twin: shadow store, command dispatch, SSE fan-out, decision feed (§3)**, now against the real provisioned device(s) from step 6.
8. **Remaining two simulator hosts (C/lights, C++/intercoms)**, since by now the provisioning and MQTT contract is proven and adding languages is comparatively low-risk, mechanical work.
9. **Portals**, built against the now-stable Management/Twin/AXIAM APIs — Staff console first (more roles to exercise, cross-role denial moment), then Resident app (grant management, intercom answer/unlock), then Sim control page last (it's the least demo-critical and easiest once the simulator hosts expose a local control API).
10. **`demo-reset` and the four key moments as an explicit, scripted end-to-end pass**, plus the dogfooding findings document, once every component exists — this is validation, not new architecture, but it belongs in the roadmap as its own late phase because it is the acceptance gate for the whole project.

Phases most likely to need deeper phase-specific research when the roadmap reaches them: the RabbitMQ MQTT+TLS broker-authorization mechanism (step 3, explicitly deferred by PROJECT.md), the exact group-per-resource provisioning code shape for §1.3, and the arm64/Pi image-build + resource-budget validation (steps 2 and 9's tail) — everything else in this document is either a confirmed AXIAM capability or a standard pattern with low architectural risk.

---

## Sources

- `crates/axiam-core/src/models/{resource,role,permission,certificate}.rs` (AXIAM repo, read 2026-09-19)
- `crates/axiam-authz/src/{engine,types}.rs` — cascading, deny-override, batch evaluation, tests
- `crates/axiam-db/src/schema.rs`, `crates/axiam-db/src/repository/role.rs` — `has_role`/`member_of`/`grants` relation definitions and uniqueness constraints
- `crates/axiam-pki/src/mtls.rs` — `DeviceAuthService`, fingerprint lookup, chain verification, trust-anchor walk
- `docs/pki/README.md` — CA/leaf issuance, CSR signing, mTLS trust-anchor flagging and its hot-reload behavior
- `docs/deployment/README.md` (TLS termination section) — in-process TLS, `SIGHUP`/poll reload, `TRUST_FORWARDED_CLIENT_CERT`
- `docs/deployment/rpi5-k3s.md` §2–§4 — the `axiam-selfsigned → axiam-ca` bootstrap pattern, per-consumer renewal table (server vs. Vault/RabbitMQ restart-required)
- `docker/docker-compose.dev.yml`, `docker/docker-compose.prod.yml`, `docker/rabbitmq-tls.conf` — confirms no MQTT plugin/vhost shipped today; AMQPS-only broker config as the starting point
- `examples/b2-iot-device-quickstart/README.md` — confirms RFC 8628 device-grant flow is a distinct, unused-by-this-project path from mTLS device auth
- `sdks/CONTRACT.md` §3 (CSRF), §4 (cookie jar), §5/§5.1 (tenant/org context) — cookie/CSRF/tenant contract every AXIAM client (including `axiam-sdk-wasm`) follows
- `.planning/PROJECT.md` (this project) — baseline architecture validated/extended by this document

---
*Architecture research for: multi-tenant IoT home-automation demo on AXIAM*
*Researched: 2026-09-19*
