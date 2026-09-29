---
phase: 01-foundation
reviewed: 2026-09-29T10:48:18Z
depth: standard
files_reviewed: 108
files_reviewed_list:
  - authz/catalog.toml
  - Cargo.lock
  - Cargo.toml
  - CLAUDE.md
  - crates/domo-common/Cargo.toml
  - crates/domo-common/src/axiam.rs
  - crates/domo-common/src/hand_rolled.rs
  - crates/domo-common/src/lib.rs
  - crates/domo-common/src/secrets.rs
  - crates/domo-common/src/tls.rs
  - crates/domo-common/src/topic.rs
  - deploy/caddy/Caddyfile
  - deploy/compose.yml
  - deploy/docker/Dockerfile.rust
  - deploy/landing/index.html
  - deploy/landing/style.css
  - deploy/pki/listeners.conf
  - deploy/postgres/postgresql.tls.conf
  - deploy/rabbitmq/20-tls.conf
  - deploy/rabbitmq/30-mqtt.conf
  - deploy/rabbitmq/enabled_plugins
  - .dockerignore
  - docs/dogfooding-findings.md
  - docs/dogfooding-issues/DF-001.md
  - docs/dogfooding-issues/DF-002.md
  - docs/dogfooding-issues/DF-003.md
  - docs/dogfooding-issues/DF-004.md
  - docs/dogfooding-issues/DF-005.md
  - docs/dogfooding-issues/DF-006.md
  - docs/dogfooding-issues/DF-007.md
  - docs/dogfooding-issues/DF-008.md
  - docs/dogfooding-issues/DF-009.md
  - docs/dogfooding-issues/DF-010.md
  - docs/dogfooding-issues/DF-011.md
  - docs/dogfooding-issues/DF-012.md
  - docs/dogfooding-issues/DF-013.md
  - docs/dogfooding-issues/DF-014.md
  - docs/dogfooding-issues/DF-015.md
  - docs/dogfooding-issues/DF-016.md
  - docs/dogfooding-issues/DF-017.md
  - docs/dogfooding-issues/DF-018.md
  - docs/dogfooding-issues/DF-019.md
  - docs/dogfooding-issues/DF-020.md
  - docs/dogfooding-issues/DF-021.md
  - docs/dogfooding-issues/DF-022.md
  - docs/dogfooding-issues/DF-023.md
  - docs/dogfooding-issues/DF-024.md
  - docs/dogfooding-issues/DF-025.md
  - docs/dogfooding-issues/DF-026.md
  - docs/dogfooding-issues/DF-027.md
  - docs/dogfooding-upstream-status.md
  - docs/setup.md
  - docs/trust.md
  - .env.example
  - .githooks/pre-commit
  - .gitignore
  - just/authz.just
  - just/edge.just
  - justfile
  - just/pki.just
  - just/smoke.just
  - just/stack.just
  - just/twin.just
  - just/verify.just
  - scripts/gen-pki.sh
  - scripts/guard-secrets.sh
  - scripts/preflight.sh
  - scripts/verify-pki.sh
  - services/device-twin/Cargo.toml
  - services/device-twin/src/lib.rs
  - services/device-twin/src/main.rs
  - services/device-twin/src/rmq/decide.rs
  - services/device-twin/src/rmq/forms.rs
  - services/device-twin/src/rmq.rs
  - services/device-twin/src/tenants.rs
  - services/device-twin/tests/common/mod.rs
  - services/device-twin/tests/rmq_resource_topic.rs
  - services/device-twin/tests/rmq_tokens.rs
  - services/device-twin/tests/rmq_user.rs
  - tools/domo-bootstrap/Cargo.toml
  - tools/domo-bootstrap/src/catalog.rs
  - tools/domo-bootstrap/src/checklist.rs
  - tools/domo-bootstrap/src/lib.rs
  - tools/domo-bootstrap/src/main.rs
  - tools/domo-bootstrap/src/naming.rs
  - tools/domo-bootstrap/src/stages/broker.rs
  - tools/domo-bootstrap/src/stages/catalog.rs
  - tools/domo-bootstrap/src/stages/device_identity.rs
  - tools/domo-bootstrap/src/stages/mod.rs
  - tools/domo-bootstrap/src/stages/org_bootstrap.rs
  - tools/domo-bootstrap/src/stages/pki.rs
  - tools/domo-bootstrap/src/stages/service_certs.rs
  - tools/domo-bootstrap/src/stages/smoke/assertions.rs
  - tools/domo-bootstrap/src/stages/smoke/certs.rs
  - tools/domo-bootstrap/src/stages/smoke.rs
  - tools/domo-bootstrap/src/stages/smoke/support.rs
  - tools/domo-bootstrap/src/stages/smoke/teardown.rs
  - tools/domo-bootstrap/src/stages/tenant_admin.rs
  - tools/domo-bootstrap/src/stages/tenants.rs
  - tools/domo-bootstrap/src/stages/tree.rs
  - tools/domo-bootstrap/src/state.rs
  - tools/domo-bootstrap/tests/catalog.rs
  - tools/domo-bootstrap/tests/naming.rs
  - tools/domo-probe/Cargo.toml
  - tools/domo-probe/src/cases.rs
  - tools/domo-probe/src/fixtures.rs
  - tools/domo-probe/src/main.rs
  - tools/domo-probe/src/matrix.rs
findings:
  critical: 3
  warning: 8
  info: 9
  total: 20
status: issues_found
---

# Phase 01: Code Review Report

**Reviewed:** 2026-09-29T10:48:18Z
**Depth:** standard
**Files Reviewed:** 108
**Status:** issues_found

## Narrative Findings (AI reviewer)

## Summary

I reviewed the Phase 1 foundation: the Device Twin's RabbitMQ authorization backend, the bootstrap stages (PKI, service certificates, device identity, tenant admins), the domo-common TLS/secrets/topic helpers, the offline-PKI and guard scripts, the compose/Caddy/RabbitMQ/PostgreSQL configuration, the just recipes, and the probe's negative-case matrix. The security-critical files got the most attention.

The Twin's pure decision core (`decide.rs`, `topic.rs`) holds up. Routing keys are matched level by level, the adjacency case is refused, tenants are asserted per verifier, and every parse failure becomes an HTTP-200 `deny`.

**On the `client_id` fix (f33b589).** When the broker sends `client_id` on `/rmq/vhost` or `/rmq/resource`, it must equal `CN=<username>`, so it can only narrow access. When it is absent, the check falls back to the live session. That is the same behaviour these endpoints had before the field existed, so absence weakens nothing compared with the previous code. The problem is not the field. It is who can send it:
- The Caddy edge forwards `/api/twin/*` to the Twin, including `/rmq/*`.
- The Twin does not authenticate its caller.

So any LAN client can call the broker-only backend with a `client_id` it made up. That breaks D-05 (CR-01).

Two verification controls report a pass without checking what they claim to check:
- `guard-secrets` scans only one image tag per repository (CR-02).
- The probe's tenant-isolation publish cases count a timeout as a refusal, so a real topic-authorization bypass would still pass them (CR-03).

Out of scope, as instructed: the known-red smoke cases DF-017/DF-025, the deliberate rate-limit and trusted-hops settings, offline-signed server certificates, the beta16 pin, and `cargo fmt` drift.

## Critical Issues

### CR-01: The broker-only authorization backend (`/rmq/*`) is reachable from the LAN through the edge, unauthenticated

**File:** `deploy/caddy/Caddyfile:69-77`, `services/device-twin/src/main.rs:14,38`, `services/device-twin/src/rmq.rs:45-51`

**Issue:** `handle_path /api/twin/*` removes the prefix and forwards everything to `device-twin:8443`. The Twin serves `/rmq/user`, `/rmq/vhost`, `/rmq/resource` and `/rmq/topic` at its root, so `POST https://{DOMO_HOST}/api/twin/rmq/user` reaches the RabbitMQ authorization backend from any machine on the LAN.

This contradicts two statements:
- Locked decision D-05: "the Twin's auth-backend endpoint stay[s] internal".
- The Twin's own header comment at `main.rs:14`: "never published outside the compose network (T-01-07)".

The Twin's listener is also `.with_no_client_auth()` (`main.rs:38`), so it cannot tell the broker apart from any other caller.

Consequences:
- **Hop 2 of the identity chain means nothing for these callers.** `client_id == "CN=" + username` is only evidence when the broker sends it, having derived it from a verified certificate DN. Through the edge, the caller writes `client_id` into the form body itself.
- **Unauthenticated cross-tenant disclosure.** For any device UUID, `POST /api/twin/rmq/topic` with `routing_key=domo.<slug>.<uuid>.x` returns `allow` or `deny`. That tells an anonymous LAN client whether the device currently has a live session and which tenant it belongs to. Tenant isolation is one of the four demo moments.
- **Token oracle and session writes.** Anyone holding a device token, even without its certificate, can check it against `/rmq/user` and create or refresh that device's cached session, including its `exp`. The session cache is the only evidence the three token-less checks use (T-05-05). Today this gives no extra broker access, because the broker still requires the certificate. But any later feature that reads the session cache inherits this path.

**Fix:** Keep `/rmq/*` off the edge and make the Twin authenticate the broker. For example:
```caddyfile
# before handle_path /api/twin/*
handle /api/twin/rmq/* {
	respond 404
}
```
and, in the Twin, serve `/rmq/*` on a separate internal listener that requires a client certificate:
```rust
// rmq listener only
let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store)).build()?;
rustls::ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
    .with_client_cert_verifier(verifier)
    .with_single_cert(certs, key)?
```
together with `auth_http.ssl_options.certfile`/`keyfile` in `30-mqtt.conf`, pointing at a root-signed `rabbitmq-authz-client` leaf added to `listeners.conf`. Also add an `edge-verify` assertion that `/api/twin/rmq/user` does not return 200.

### CR-02: `guard-secrets` scans only one tag per image repository, so the running image or the shipped arm64 image is never scanned

**File:** `scripts/guard-secrets.sh:192-194`

**Issue:** `awk -F: -v r="$repo" '$1 == r { print; exit }'` keeps only the first `docker image ls` row for each repository. `just images` produces both `domo-twin:dev` and `domo-twin:dev-arm64`, and likewise for `domo-tools` (`just/stack.just:264-284`); both are present on this machine right now. `docker image ls` lists the newest image first:
- After `just images`, only the `-arm64` image is scanned. The `:dev` image that compose actually runs is not.
- After a `just build`, only `:dev` is scanned. The arm64 image that is archived and loaded onto the Pi is not.

The archived `dist/images/*-arm64.tar` files, which are the artifacts that physically leave the machine, are never scanned at all.

This is the image-layer half of PKI-06/T-01-03 ("the bytes of the images actually produced"), and it prints `✓ … 0 PEM private-key blocks` while skipping at least one image.

**Fix:**
```bash
for repo in $repos; do
    images="$(docker image ls --format '{{.Repository}}:{{.Tag}}' | awk -F: -v r="$repo" '$1 == r')"
    [ -n "$images" ] || { skip "image not built: ${repo}"; continue; }
    for image in $images; do
        blocks="$(docker save "$image" 2>/dev/null | tar -xO 2>/dev/null | count_pem_key_blocks || echo "")"
        # … same assertions as today, per image
    done
done
shopt -s nullglob
for t in dist/images/*.tar; do
    blocks="$(tar -xOf "$t" 2>/dev/null | count_pem_key_blocks || echo "")"
    [ "$blocks" = 0 ] || fail "$t: ${blocks:-unscannable} PEM private-key block(s)"
done
```

### CR-03: The probe's namespace-isolation cases pass when the cross-namespace publish is allowed

**File:** `tools/domo-probe/src/matrix.rs:101`, `tools/domo-probe/src/matrix.rs:453-479`, `tools/domo-probe/src/cases.rs:263-281`

**Issue:** `namespace-sibling`, `namespace-other-tenant` and `namespace-adjacent` subscribe to the device's own `{own}/#` and publish into another namespace (`matrix.rs` builds `subscribe: format!("{own}/#")`, `expect_round_trip: false`). The publish is QoS 1.

Suppose the Twin or the broker wrongly allows that publish. The broker answers with a PUBACK, and the message goes to the victim's namespace, which the prober is not subscribed to. The loop ignores `PubAck`, runs until the deadline, and returns `Observed::Timeout` (`(true, false, false) => Observed::Timeout`). `satisfies()` then accepts `(Expect::PublishRefused, … | Self::Timeout) => true`.

So a real topic-authorization bypass, which is exactly the tenant-isolation failure these cases exist to catch, reports `✓`. The negative cases cannot tell "refused" from "allowed".

**Fix:** Record the PUBACK and treat it as acceptance. Accept only a close after connect as a refusal:
```rust
Event::Incoming(Packet::PubAck(_)) if !a.expect_round_trip => {
    client.disconnect().await.ok();
    return Ok(Observed::PublishAccepted);
}
…
(Expect::PublishRefused, Self::ClosedAfterConnect(_)) => true,
// Timeout no longer satisfies PublishRefused; PublishAccepted never does.
```
If a broker version answers with an MQTT 5 PUBACK carrying a reason code, check that the code is `NotAuthorized` instead.

## Warnings

### WR-01: `verify-pki` "verified for <name>" does not check the host name

**File:** `scripts/verify-pki.sh:355-376`

**Issue:** The live loop runs `openssl s_client -connect … -servername "$host_name" -CAfile root.pem -verify_return_error`. `-servername` only sets SNI. Without `-verify_hostname` (or `-verify_ip` for IP SANs), s_client checks the chain and nothing else. A listener serving any root-signed certificate, for example a stale leaf or another listener's cert, passes every declared name and prints `✓ … TLS 1.3 verified for <name>`. The offline SAN check only inspects the file on disk, not what the live listener presents.

**Fix:**
```bash
case "$entry" in
    IP:*) sni=(-verify_ip "$host_name") ;;
    *)    sni=(-servername "$host_name" -verify_hostname "$host_name") ;;
esac
```

### WR-02: PostgreSQL offers TLS but does not require it, and `pg-verify` reports otherwise

**File:** `deploy/postgres/postgresql.tls.conf:42,87`, `just/edge.just:314-317`

**Issue:** `hba_file` points at the image-generated `pg_hba.conf`. Its entrypoint appends `host all all all scram-sha-256`, which accepts plaintext connections from every container. `ssl = on` only makes TLS available. `pg-verify` asserts `SHOW ssl → on` and the TLS 1.3 floor, which reads as "PostgreSQL is TLS-only". It is not, and Phase 2's clients can silently connect without TLS.

**Fix:** Ship a `pg_hba.conf` (mounted read-only, with `hba_file` pointing at it):
```
local   all all                scram-sha-256
hostssl all all 0.0.0.0/0      scram-sha-256
host    all all 0.0.0.0/0      reject
```
and add a `pg-verify` check that a `sslmode=disable` connection is refused.

### WR-03: The tenant map is written non-atomically, and a torn read replaces the Twin's whole map with an empty one

**File:** `tools/domo-bootstrap/src/stages/tenants.rs:101-104`, `services/device-twin/src/tenants.rs:105-119,234-239`

**Issue:** The bootstrap writes `tenants.json` with `std::fs::write`, which truncates and then writes. On a miss, the Twin checks the file's mtime and re-reads it. `load_tenant_map` maps a truncated or partial file to an empty map, and `*map = fresh` then replaces the whole cached map, including tenants that were resolving fine.

If the truncate and the write share an mtime tick (timestamp granularity is coarse), the Twin records that mtime as seen and keeps the empty map until the file changes again. Every CONNECT is then denied with `UnknownTenant`. The failure is closed, but it is an outage.

**Fix:** Write to `tenants.json.tmp` in the same directory, `fsync`, and `rename` over the target. In `slug()`, keep the previous map when the fresh read fails to parse. Make `load_tenant_map` return `Option` so "unparseable" is distinguishable from "empty", and do not advance `slugs_read_at` in that case.

### WR-04: `device-identity` signs and binds any CSR without checking its subject, and reuses revoked certificates

**File:** `tools/domo-bootstrap/src/stages/device_identity.rs:403-452`

**Issue:**
1. The CSR read from `--csr` is signed and bound to `sa_id` without checking that its CN is `sa_id`. `service_certs.rs` notes that "AXIAM resolves the service account from the certificate's CN", and the whole identity chain assumes CN == service-account UUID. A CSR naming another account's UUID would be signed under the tenant CA and bound to the probe account, producing a credential whose CN names someone else.
2. The reuse branch matches only `bound_service_account_id` and the public key. It does not check `status == CertificateStatus::Active`, which `service_certs.rs:157` does. A revoked certificate is written back to `out_path` as if it were usable.

**Fix:**
```rust
let (_, block) = x509_parser::pem::parse_x509_pem(csr_pem.as_bytes())?;
let (_, csr) = X509CertificationRequest::from_der(&block.contents)?;
let cn = csr.certification_request_info.subject.iter_common_name()
    .next().and_then(|a| a.as_str().ok());
anyhow::ensure!(cn == Some(sa_id.to_string().as_str()), "CSR subject is not CN={sa_id}");
…
.find(|c| c.bound_service_account_id == Some(sa_id) && c.status == CertificateStatus::Active)
```

### WR-05: Rotating the root leaves the old root as an AXIAM mTLS anchor and the tenant CAs under it

**File:** `tools/domo-bootstrap/src/stages/pki.rs:73-105,134-140`, `just/pki.just:114,124`

**Issue:** `pki-rotate-root` deletes `.secrets/pki` and states that AXIAM's trust anchor "and every tenant signing CA beneath it are re-created by the next 'just tracer'". They are not:
- `pki.rs` imports the new root and anchors it, but never un-anchors the old one. The rotated-out root, which may have been rotated precisely because it was exposed, stays an accepted mTLS trust anchor in AXIAM.
- Each tenant's signing CA is reused through `existing.first()` without checking that it chains to the current root. The tenant CAs stay under the old root, and every leaf issued afterwards fails verification against the new root on the broker, the Twin and the probe.

**Fix:** In `pki.rs`, reuse a signing CA only if its `parent_ca_id` is `root_ca.id`, or if its PEM verifies against `root_pem`. Otherwise stop with instructions to run `just demo-reset`, or revoke it and generate a new one. Disable `mtls_trust_anchor` on every org CA other than `root_ca`. Correct the rotation text, or make `pki-rotate-root` chain into `demo-reset` and `export-trust`.

### WR-06: The staged-path layer only catches files under `.secrets/`, not key material committed from anywhere else

**File:** `scripts/guard-secrets.sh:48-76`

**Issue:** Layer 1 exists to catch `git add -f`, but only for paths starting with `.secrets/` or `dist/`. A force-added copy of the root key anywhere else (`tools/x/root.key`, `deploy/caddy/server.key`, or a PEM pasted into a `.md`) passes all three layers. `**/*.key` in `.gitignore` is exactly what `-f` bypasses. In addition, `git diff --cached --name-only` without `-z` C-quotes paths with unusual bytes (`".secrets/\303…"`), so they slip past the `case` match.

**Fix:** Also scan staged content, and use NUL-delimited paths:
```bash
git diff --cached --name-only -z --diff-filter=ACMR | while IFS= read -r -d '' p; do
    if git show ":$p" 2>/dev/null | count_pem_key_blocks | grep -qv '^0$'; then
        fail "staged file $p contains a PEM private key"
    fi
done
```

### WR-07: The simulated device container gets the organization root private key, read-write

**File:** `deploy/compose.yml:566-580`

**Issue:** `domo-probe` mounts the whole `../.secrets` read-write at `/domo/.secrets`. It also mounts `.secrets/pki` separately, which includes `root.key`, `super-admin.json`, tenant-admin passwords, `generated.env` (JWT signing key, peppers) and every service key. The probe stands in for a device. It needs its own `probe/` and `smoke/` material, the tenant CA PEMs and `root.pem`, and nothing else. This goes against the "private keys never leave the simulator host / single root custody" posture, and makes the probe an equivalent of organization-root custody.

**Fix:** Mount only what the probe reads:
```yaml
volumes:
  - ../.secrets/probe:/domo/.secrets/probe
  - ../.secrets/smoke:/domo/.secrets/smoke
  - ../.secrets/axiam:/domo/.secrets/axiam:ro   # tenant CA PEMs only, ideally a narrower dir
  - ../.secrets/pki/root.pem:/etc/domo/pki/root.pem:ro
```
Moving the `*-ca.pem` files to a non-secret directory would narrow this further.

### WR-08: `just twin-run` defaults point at files that are never produced

**File:** `just/twin.just:29-32`

**Issue:** The defaults are `.secrets/pki/twin.pem` / `twin.key`, but `gen-pki.sh` issues `device-twin.pem` / `device-twin.key` (`listeners.conf:123`). The default tenant map, `.secrets/state/tenants.json`, is not written by anything: the bootstrap writes to `DOMO_TWIN_STATE_DIR` (default `/domo/state`). As a result, `just twin-run` fails at startup, and if the certificate is overridden, every CONNECT is denied as `UnknownTenant`.

**Fix:** Default to `.secrets/pki/device-twin.pem` / `.key`, and document or default `DOMO_TENANT_MAP_FILE` to a path the bootstrap actually writes locally (or run the tenants stage with `DOMO_TWIN_STATE_DIR` set to match).

## Info

### IN-01: Secret-bearing structs derive `Debug`

**File:** `tools/domo-bootstrap/src/stages/mod.rs:23-29`, `tools/domo-probe/src/main.rs:136-143`

**Issue:** `Credentials` (plaintext `password`) and `DeviceAuth` (`access_token`) derive `Debug`. No current call site formats them, but one `?creds` in a `tracing` call or a `{:?}` in an error context leaks them. `pki.rs` itself cites T-01-04 against exactly this.

**Fix:** Implement `Debug` by hand with the secret redacted, or wrap the field in `axiam_sdk::Sensitive`.

### IN-02: A token that was revoked, or bound to a certificate, stays usable until `exp`

**File:** `services/device-twin/src/tenants.rs:151-156`, `services/device-twin/src/rmq.rs:123-146`

**Issue:** beta16's `JwksVerifier::verify` checks revocation only when a revocation feed is attached, and none is. It also does not look at `cnf`, so a certificate-bound device token is accepted as a bearer token as long as the CN matches. The Twin cannot see the certificate thumbprint, which is how DF-025's forged leaf is accepted.

Separately, `SessionCache` entries are never evicted except by expiry. A service account that is disabled or revoked in AXIAM keeps its live session and passes CONNECT until the token expires.

**Fix:** Document this in the findings next to DF-025. When the SDK allows it, attach a revocation feed and reject tokens that carry `cnf` here, rather than silently dropping the binding.

### IN-03: The topic check ignores `variable_map.client_id`

**File:** `services/device-twin/src/rmq.rs:210-226`, `services/device-twin/src/rmq/forms.rs:323-324`

**Issue:** `variable_map_client_id` is parsed but never held to the `CN=<username>` binding, unlike `client_id` on the vhost and resource checks. This is harmless today, because the session was bound at CONNECT, but it is an inconsistency that can only narrow access if closed.

**Fix:** Pass it through `client_id_binding` in `decide_topic`.

### IN-04: `DOMO_MQTT_VHOST` is set but never read

**File:** `deploy/compose.yml:360`

**Issue:** The Twin hardcodes `domo_common::DOMO_VHOST`. The environment variable suggests the vhost is configurable when it is not.

**Fix:** Remove it, or read it and assert that it equals `DOMO_VHOST` at startup.

### IN-05: The broker management API is plaintext HTTP carrying the AXIAM broker admin credentials

**File:** `tools/domo-bootstrap/src/stages/broker.rs:16-34`, `deploy/compose.yml:318`

**Issue:** Basic auth for `RABBITMQ_DEFAULT_USER`/`PASS` crosses the compose network in cleartext, and the listener is published on host loopback. This is an exception to D-07 that is not recorded as one.

**Fix:** Enable `management.ssl.*` with a root-signed leaf, or record the exception.

### IN-06: `demo-reset` ignores failed volume removals and continues

**File:** `just/stack.just:700-703`

**Issue:** The recipe runs without `-e`, and `docker volume rm … && …` swallows the failure. The reset then deletes the stage markers and the setup token, and runs `just up` against volumes that survived. The result reports a clean reset that did not happen.

**Fix:** Count failures and exit non-zero before `just up` when any named volume could not be removed.

### IN-07: The tenant-admin 409 path never reconciles the stored password; the search parameter is not encoded

**File:** `crates/domo-common/src/hand_rolled.rs:272-280`

**Issue:** If `.secrets/axiam/tenant-admin-<slug>.json` is lost while the AXIAM volume survives, a new password is minted, the existing user is "provisioned" (activated and given the role), and the failure only shows up later as a misleading login error. `?search={username}` is interpolated without URL encoding.

**Fix:** After a 409, verify that the stored credentials can log in, and fail with a precise message if they cannot. Build the query with `reqwest`'s `.query(&[("search", username)])`.

### IN-08: The `tenants` stage probe does not check that the Twin's tenant map exists

**File:** `tools/domo-bootstrap/src/state.rs` (`probe`, `Stage::Tenants`)

**Issue:** The probe checks tenants, signing CAs and tenant-admin logins, but not `tenants.json` in `twin-state`. If only that volume is lost, the stage is skipped as done and the Twin denies every device.

**Fix:** Also require the map file to exist, parse, and name every demo tenant.

### IN-09: `secrets::write` creates parent directories with the process umask

**File:** `crates/domo-common/src/secrets.rs:66-69`

**Issue:** `create_dir_all` does not apply 0700, so subdirectories created inside the containers (as uid 1000, umask 022) are world-listable. File contents are 0600, but the names of secret files are exposed. The module comment says the tree is 0700.

**Fix:** Use `std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)`.

---

_Reviewed: 2026-09-29T10:48:18Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
