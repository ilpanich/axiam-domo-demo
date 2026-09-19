# Phase 1: Foundation - Pattern Map

**Mapped:** 2026-09-19
**Files analyzed:** 19 new files (this repo is greenfield — zero in-repo analogs)
**Analogs found:** 11 / 19 (all from sibling checkouts under `/home/emanuele/git/priv/`)

> **Greenfield warning for the planner.** `/home/emanuele/git/priv/axiam-domo-demo` contains only
> `.planning/` and `CLAUDE.md`. Every analog below lives in a *different* repository
> (`../axiam`, `../axiam-rust-sdk`). They are **references to copy shape from, never to
> `include:` / `path =` / import**. D-02 explicitly forbids depending on the sibling checkout at
> runtime. All cited paths were verified git-TRACKED in their own repo (`git ls-files`).

## File Classification

| New file (Phase 1) | Role | Data flow | Closest analog | Match quality |
|---|---|---|---|---|
| `deploy/compose.yml` | config (orchestration) | n/a | `../axiam/docker/docker-compose.prod.yml` | exact |
| `deploy/rabbitmq/30-mqtt.conf` | config (broker) | event-driven | `../axiam/docker/rabbitmq-tls.conf` | exact |
| `deploy/rabbitmq/enabled_plugins` | config | n/a | none (image default only) | **no analog** |
| `deploy/caddy/Caddyfile` | config (edge routing) | request-response | `../axiam/docker/nginx.conf.template` | partial (different tool, same route map + upstream-TLS semantics) |
| `deploy/landing/index.html` | static asset | n/a | none | **no analog** |
| `deploy/docker/Dockerfile.rust` | build config | n/a | `../axiam/docker/Dockerfile.server` | role-match (AXIAM builds natively per-arch, not cross) |
| `deploy/postgres/*.conf` | config | n/a | none in siblings | **no analog** |
| root `justfile` (`up`, `demo-reset`, `export-trust`, `smoke`, `pki*`, `preflight`) | orchestration script | batch | `../axiam/justfile` (`prod-up`, `_prod-compose`, `prod-clean`) | exact |
| `scripts/` offline PKI (root + SAN server certs, D-15/PKI-04) | utility script | file-I/O | `../axiam/scripts/gen-broker-tls.sh` | exact |
| root `Cargo.toml` (workspace) | config | n/a | `../axiam/Cargo.toml` | exact |
| `crates/domo-common` | library | n/a | none (SDK-consumer helper crate) | **no analog** |
| `tools/domo-bootstrap` — CLI/stage skeleton | CLI tool | batch | none (no clap-derive CLI exists in either sibling) | **no analog** |
| `tools/domo-bootstrap` — org-scoped SDK calls | service client | request-response | `../axiam-rust-sdk/examples/device_mtls_provisioning.rs` | exact (call-for-call) |
| `tools/domo-bootstrap` — catalog apply | service client | batch/declarative | `../axiam-rust-sdk/examples/management_manifest.rs` | exact |
| `tools/domo-bootstrap` — imperative tree/groups | service client | CRUD | `../axiam-rust-sdk/examples/management_basics.rs` | exact |
| `tools/domo-bootstrap` — hand-rolled tenant-admin + `/admin/bootstrap` | HTTP client | request-response | `../axiam/scripts/e2e-bootstrap.sh` | exact (shell → Rust transliteration) |
| `tools/domo-probe` — device login + MQTT matrix | CLI/e2e harness | request-response + pub-sub | partial: `device_mtls_provisioning.rs` (identity) ; **no analog for `/auth/device` or rumqttc** | partial |
| `services/device-twin` (auth-backend endpoints only) | service (Actix) | request-response | `../axiam-rust-sdk/examples/actix_route_guard.rs` | role-match (same wiring; different guard surface) |
| `authz/catalog.toml` | config (declarative authz) | n/a | `../axiam-rust-sdk/examples/management_manifest.rs` (the `manifest!` target schema) | partial — schema is ours, target type is the SDK's |
| `docs/dogfooding-findings.md` | doc | n/a | none | **no analog** |
| `.gitignore` / `.dockerignore` secrets guard | config | n/a | `../axiam/.gitignore:33-34`, `../axiam/.dockerignore:30` | exact |

---

## Pattern Assignments

### `deploy/compose.yml` (config, orchestration)

**Analog:** `/home/emanuele/git/priv/axiam/docker/docker-compose.prod.yml`

**Image pinning + hard-fail interpolation guard** (line 63; identical form on `axiam-frontend` line ~314):
```yaml
image: ghcr.io/ilpanich/axiam/server:${AXIAM_IMAGE_TAG:?image tag required - run 'just prod-up', which defaults it to the workspace version, or export AXIAM_IMAGE_TAG=1.0.0-alphaNN}
```
Copy the `${VAR:?message}` style for every secret: it turns a missing `.env` into one clear
line instead of a container that boots with a default. Note the analog's own warning comment:
compose expands **every** guard even for `down`, which is why the justfile analog exports
placeholders (see `_prod-compose` below).

**Env-var naming trap to preserve** (comment at lines 93-98):
> `load_config()` … calls `.with_prefix("AXIAM").separator("__")` … Env vars must use a
> DOUBLE underscore after AXIAM (e.g. `AXIAM__DB__URL`), otherwise they are silently ignored
> and in-code defaults win.

**TLS + client-auth env block to adapt** (the vars exist already; Phase 1 flips them on per
RESEARCH Pattern 1):
```yaml
      AXIAM__SERVER__TLS__ENABLED: "${AXIAM_TLS_ENABLED:-false}"     # → "true"
      AXIAM__SERVER__TLS__CERT_PATH: "/etc/axiam/server-tls/fullchain.pem"
      AXIAM__SERVER__TLS__KEY_PATH: "/etc/axiam/server-tls/privkey.pem"
      AXIAM__SERVER__TLS__CLIENT_AUTH: "${AXIAM_TLS_CLIENT_AUTH:-off}" # → "optional" + CLIENT_CA_PATH
```

**AMQP-over-TLS block** (add `CLIENT_CERT_PATH`/`CLIENT_KEY_PATH` per D-37/C-4):
```yaml
      AXIAM__AMQP__URL: "amqps://${RABBITMQ_DEFAULT_USER:?...}:${RABBITMQ_DEFAULT_PASS:?...}@rabbitmq:5671"
      AXIAM__AMQP__TLS__CA_CERT_PATH: "/etc/axiam/broker-tls/ca.pem"
```

**Frontend wiring to reuse verbatim (D-03/D-30)** (lines ~334-336):
```yaml
      AXIAM_BACKEND_ORIGIN: "${AXIAM_BACKEND_ORIGIN:-http://axiam-server:8090}"
      AXIAM_BACKEND_SNI: "${AXIAM_BACKEND_SNI:-axiam-server}"
      AXIAM_BACKEND_CA: "${AXIAM_BACKEND_CA:-/etc/ssl/certs/ca-certificates.crt}"
```
For Phase 1 set `ORIGIN=https://axiam-server:8090` and `CA=<mounted root.pem>`.

**The `tls-init` one-shot pattern (P-6) — copy `surrealdb-init` exactly** (lines ~354-363, and
the `condition: service_completed_successfully` consumer at ~368-370):
```yaml
  surrealdb-init:
    image: busybox
    container_name: axiam-surrealdb-init
    command: ["sh", "-c", "chown -R 65532:65532 /data && chmod -R u+rwX /data"]
    volumes:
      - surrealdb-data:/data
    restart: "no"

  surrealdb:
    depends_on:
      surrealdb-init:
        condition: service_completed_successfully
```
This is the shape `tls-init` must take: copy `.secrets/pki/*` into per-service named volumes
with the right uid (axiam 65532, postgres 70, rabbitmq's own) and 0600/0640.

**RabbitMQ service — conf.d fragment mount + TLS dir mount** (lines ~415-421):
```yaml
    volumes:
      - rabbitmq-data:/var/lib/rabbitmq
      - ./.secrets/broker-tls:/etc/rabbitmq/tls:ro
      - ./rabbitmq-tls.conf:/etc/rabbitmq/conf.d/20-tls.conf:ro
    healthcheck:
      test: ["CMD", "rabbitmq-diagnostics", "check_running"]
```
Add `./rabbitmq/30-mqtt.conf:/etc/rabbitmq/conf.d/30-mqtt.conf:ro`, `enabled_plugins`, and the
8883 publish. **Do not copy AXIAM's `axiam-server` healthcheck** — P-4 says it breaks under TLS.

**Deltas from the analog (do not copy):** the whole `vault` + `vault-data-perms` block
(lines 50-120) — D-02 drops Vault for `AXIAM__AUTH__SECRET_PROVIDER: "env"`; and the
`127.0.0.1:` port bindings — D-05 needs 8090/8883 on the LAN.

---

### `deploy/rabbitmq/30-mqtt.conf` (config, broker)

**Analog:** `/home/emanuele/git/priv/axiam/docker/rabbitmq-tls.conf` (68 lines, read whole)

**Values Phase 1 must override** (lines 46-68 of the analog):
```ini
listeners.tcp = none
listeners.ssl.default = 5671
ssl_options.cacertfile = /etc/rabbitmq/tls/ca.pem
ssl_options.certfile   = /etc/rabbitmq/tls/server.pem
ssl_options.keyfile    = /etc/rabbitmq/tls/server.key
ssl_options.verify = verify_none              # ← 30-mqtt.conf must flip to verify_peer
ssl_options.fail_if_no_peer_cert = false      # ← must flip to true
ssl_options.versions.1 = tlsv1.3
```

**The sysctl-vs-erl-args trap, stated by the analog's own header (lines 15-44) — reproduce the
rationale comment in our file:**
> Because the erl-args form this replaced was never valid… `-rabbit <key> <value>` wants Erlang
> atoms and terms… The node dies during prelaunch, and the symptom is an unrelated-looking
> `Error when reading /var/lib/rabbitmq/.erlang.cookie: eacces`.

**The conf.d numbering rule (lines 3-7):**
> Installed as `/etc/rabbitmq/conf.d/20-tls.conf` — a *fragment*, not a replacement for
> `rabbitmq.conf`… the official image's entrypoint writes `conf.d/10-defaultuser.conf` from
> `RABBITMQ_DEFAULT_USER/PASS`… Numbered 20 so it loads after that one.

Ours is `30-mqtt.conf` for exactly that reason. This also backs D-26's "no `definitions.json`".

**Executor task:** decide whether to ship `20-tls.conf` as a copy with our values merged in
(safer, per assumption A4) or rely on later-file-wins. Verify with `rabbitmq-diagnostics environment`.

---

### `deploy/caddy/Caddyfile` (config, request-response)

**Analog:** `/home/emanuele/git/priv/axiam/docker/nginx.conf.template` — **not a Caddyfile**,
but it is the authoritative statement of the route map and upstream-TLS posture.

**Route set to mirror** (locations at lines 99, 104, 146, 175):
```nginx
    location / { try_files $uri $uri/ /index.html; }   # SPA root-mounted → D-30's separate host
    location /api        { proxy_pass ${AXIAM_BACKEND_ORIGIN}; ... }
    location /oauth2/    { proxy_pass ${AXIAM_BACKEND_ORIGIN}; ... }   # trailing slash load-bearing
    location /.well-known { proxy_pass ${AXIAM_BACKEND_ORIGIN}; ... }
```

**Upstream-TLS verification stanza — the semantics Caddy's `transport http { tls_trust_pool … }`
must reproduce** (lines 118-123, repeated in every proxy block):
```nginx
        proxy_ssl_server_name on;
        proxy_ssl_name ${AXIAM_BACKEND_SNI};
        proxy_ssl_verify on;
        proxy_ssl_verify_depth 3;
        proxy_ssl_trusted_certificate ${AXIAM_BACKEND_CA};
        proxy_ssl_protocols TLSv1.3;
```
→ Caddy equivalent: `tls_trust_pool file /pki/root.pem`, `tls_server_name axiam-server`,
`protocols tls1.3`. Note `verify_depth 3` — matches RESEARCH's `ssl_options.depth = 3` (A9).

**The trailing-slash note (lines 128-135)** is a real prefix-matching hazard: `location /oauth2`
would also swallow `/oauth2-clients`. Caddy's `path /oauth2/*` matcher has the same class of
problem — write the matcher exactly, and keep D-30's ordering rule (`/api/mgmt`, `/api/twin`
matched **before** `/api/v1`).

**CSP to carry to the landing page and, in Phase 5, the portals** (line 69) — `wasm-unsafe-eval`
is required by the WASM SDK:
```
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; form-action 'self'; base-uri 'self'
```

**No analog for:** `auto_https off`, static `tls <cert> <key>`, `header_up -X-Client-Certificate`.
Take those from RESEARCH Code Example 4 + the Caddy docs cited there.

---

### PKI scripts — offline root + SAN server certs (utility, file-I/O)

**Analog:** `/home/emanuele/git/priv/axiam/scripts/gen-broker-tls.sh` (88 lines, read whole).
This is the closest thing to `just pki` / `just pki-server-certs` that exists anywhere in the
sibling tree, and it already encodes idempotency, SAN handling and the permissions problem.

**Idempotency guard to copy (D-10: the root is never rotated by reset)** (lines 36-40):
```bash
if [[ -f "$OUT_DIR/server.pem" && -f "$OUT_DIR/server.key" && -f "$OUT_DIR/ca.pem" ]]; then
  echo "→ Broker TLS material already present in $OUT_DIR — leaving it alone."
  echo "  (Delete the directory and re-run to rotate.)"
  exit 0
fi
```
Ours splits this: the **root** guard is unconditional (`.secrets/state/pki-root.done`), the
**server leaf** guard is cleared by reset.

**Root generation (lines 46-50) — change algorithm params only, keep the shape:**
```bash
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out "$OUT_DIR/ca.key" 2>/dev/null
openssl req -x509 -new -key "$OUT_DIR/ca.key" -sha256 -days "$DAYS" \
  -subj "/CN=AXIAM Broker CA/O=AXIAM" -out "$OUT_DIR/ca.pem" 2>/dev/null
```
**Delta required by P-5:** `genpkey` already emits PKCS#8 (good — AXIAM's BYOK import parses with
`rcgen::KeyPair::from_pem` and rejects PKCS#1). The analog omits `basicConstraints`/`keyUsage`
extensions on the root; our root **must** add `basicConstraints=critical,CA:TRUE`,
`keyUsage=critical,keyCertSign,cRLSign`, `subjectKeyIdentifier=hash`, and **no pathlen**.

**Leaf extension file + signing (lines 59-73) — the exact pattern for D-04's SAN list:**
```bash
cat > "$OUT_DIR/server.ext" <<EOF
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = DNS:${BROKER_HOST}, DNS:localhost, IP:127.0.0.1
EOF
openssl x509 -req -in "$OUT_DIR/server.csr" \
  -CA "$OUT_DIR/ca.pem" -CAkey "$OUT_DIR/ca.key" -CAcreateserial \
  -out "$OUT_DIR/server.pem" -days "$DAYS" -sha256 \
  -extfile "$OUT_DIR/server.ext" 2>/dev/null
rm -f "$OUT_DIR/server.csr" "$OUT_DIR/server.ext" "$OUT_DIR/ca.srl"
```
Deltas: P-256 keys (`-pkeyopt ec_paramgen_curve:P-256`), `-days 397`, add `-copy_extensions none`,
drop `keyEncipherment` (not used by ECDSA/TLS1.3), and the D-04 SAN list (`$DOMO_HOST`,
`axiam.$DOMO_HOST`, LAN IP, localhost, 127.0.0.1, compose service name).

**Permissions block (lines 75-79)** — note the analog's *comment* is the important part:
```bash
chmod 644 "$OUT_DIR/ca.pem" "$OUT_DIR/server.pem"
chmod 644 "$OUT_DIR/server.key"
chmod 600 "$OUT_DIR/ca.key"
```
Ours must be stricter (D-13: 0600 everywhere) — which is precisely why the `tls-init` copy
service exists (P-6). Do **not** loosen `.secrets` to 644 to make containers read it.

**The analog's own header already predicts this project (lines 17-21):**
> **Use AXIAM's own CA.** `axiam-pki` issues X.509 certificates from an organization CA… Issuing
> the broker's cert from the same CA that signs your device and service certificates is good
> dogfooding and gives you one trust root to rotate instead of two.

---

### root `justfile` (orchestration, batch)

**Analog:** `/home/emanuele/git/priv/axiam/justfile` — `prod-up` (lines 349-470+),
`_prod-compose` (≈ line 600), `prod-down`/`prod-clean` (lines 623, 637).

**Secret minting + stale-volume refusal — directly reusable for `just up` preflight/pki
(lines 362-403):**
```bash
    CREDS="$SECRETS_DIR/stack-credentials.env"
    if [[ ! -f "$CREDS" ]]; then
        PROJECT="${COMPOSE_PROJECT_NAME:-docker}"
        STALE=""
        for v in "${PROJECT}_surrealdb-data" "${PROJECT}_rabbitmq-data"; do
            if docker volume inspect "$v" >/dev/null 2>&1; then STALE="$STALE $v"; fi
        done
        if [[ -n "$STALE" ]]; then
            echo "✗ $CREDS is missing, but these data volumes already exist:" >&2
            ...
            echo "      just prod-clean && just prod-up" >&2
            exit 1
        fi
        echo "→ Minting SurrealDB + RabbitMQ credentials in $CREDS (first-run only)"
        ( umask 077 && cat > "$CREDS" <<EOF
    export AXIAM__DB__PASSWORD="$(openssl rand -hex 24)"
    export RABBITMQ_DEFAULT_PASS="$(openssl rand -hex 24)"
    EOF
        )
    fi
    source "$CREDS"
```
Three things to lift verbatim: `umask 077` for the heredoc (D-13's 0600), the
`✗ … / → …` output vocabulary (matches D-34's ✓/✗ checklist and the user's `just up` sketch),
and the **refuse-don't-guess** posture on a mismatch between secrets and volumes.

**Key-material generation guard (lines 424-431)** — the shape for every stage marker:
```bash
    if [[ ! -f "$PRIV" || ! -f "$PUB" ]]; then
        echo "→ Generating Ed25519 JWT keypair in $SECRETS_DIR/ (first-run only)"
        openssl genpkey -algorithm ed25519 -out "$PRIV"
        openssl pkey -in "$PRIV" -pubout -out "$PUB"
        chmod 600 "$PRIV"
    fi
    export AXIAM__AUTH__JWT_PRIVATE_KEY_PEM="$(cat "$PRIV")"
```

**Service-readiness wait loop (lines 446-461)** — the template for D-34's health waits and the
setup-token scrape (P-3), including the "print the container to check" failure line:
```bash
    for _ in $(seq 1 60); do
        if curl -sS --cacert "$VAULT_CACERT" -o /dev/null "$VAULT_ADDR/v1/sys/health" 2>/dev/null; then
            VAULT_UP=1; break
        fi
        sleep 1
    done
    if [[ "$VAULT_UP" -ne 1 ]]; then
        echo "✗ Vault never came up on $VAULT_ADDR. Check: docker logs axiam-vault" >&2
        exit 1
    fi
```

**Don't-infer-state-from-a-file lesson (lines 464-467)** — applies directly to D-34's markers:
> Ask Vault whether it is initialised rather than inferring it from the presence of `$VAULT_STATE`:
> a run that died between the redirect and a successful response leaves a zero-byte file behind,
> and `[[ -f ]]` then skips initialisation forever.

→ Every `domo-bootstrap` stage must re-probe AXIAM for the real state (P-2/P-3/P-8 idempotent
"list by natural key → create if absent"), and treat the marker as a *skip hint*, not as truth.

**Compose-wrapper recipe (`_prod-compose`)** — copy this for `just demo-reset`/`down`:
```just
_prod-compose *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    CREDS="docker/.secrets/stack-credentials.env"
    if [[ -f "$CREDS" ]]; then source "$CREDS"; fi
    export AXIAM__DB__USERNAME="${AXIAM__DB__USERNAME:-unused-for-this-subcommand}"
    ...
    docker compose -f docker/docker-compose.prod.yml {{ARGS}}
```
Rationale, quoted from the analog: "Compose expands EVERY `${VAR:?...}` guard in the file whatever
the subcommand is — so `down` … still fails on all eight of them unless they resolve."

**`prod-clean` comment is the model for D-09 vs D-10** (lines ~637-643):
> Deliberately kept: the JWT keypair, broker TLS and Vault TLS material, which are inputs to the
> stack rather than state produced by it, and are regenerated only when absent.

→ `just demo-reset` keeps `.secrets/pki/root.*` + `pki-root.done`; wipes everything else.
**Also:** wipe only **this project's** volumes (name them explicitly — the analog's
`COMPOSE_PROJECT_NAME` handling shows why), never `docker volume prune`.

---

### `tools/domo-bootstrap` — AXIAM calls

**Analog A (org-scoped SDK sequence):** `/home/emanuele/git/priv/axiam-rust-sdk/examples/device_mtls_provisioning.rs`
It is a *printed* walkthrough (no network I/O), but it names every §27 call and the rules attached:
```rust
// 1. anchor the organization CA for mTLS
//    PUT /api/v1/organizations/{org}/ca-certificates/{ca}/mtls-trust-anchor
let _anchor = models::SetMtlsTrustAnchor { enabled: true };
// 2. service_accounts().create(&CreateServiceAccountRequest { .. })
// 3. certificates().generate(&GenerateCertificateRequest { cert_type: Device, .. })
// 4. service_accounts().bind_certificate(sa_id, &BindCertificateRequest { certificate_id })
// 5. AxiamClient::builder().with_client_cert(cert_pem, key_pem)
```
Rules to obey, straight from the example's comments:
- `SetMtlsTrustAnchor` has a *required* field — "this route is a replacement rather than a patch
  (§27.4 rule 5)".
- Bind is mandatory: "Without this, the certificate is valid TLS material that authenticates as
  nobody: the handshake succeeds and the authorization check finds no subject to check."
- `private_key_pem` is `Sensitive<String>`, returned **once** — `Display` redacts it. D-08's
  tenant-CA key is deliberately discarded (AXIAM keeps custody).
- "A real run authenticates first: §27.4 rule 1 refuses to make a wire call without a session."

**Delta:** Phase 1 uses `certificates().sign_csr(...)`, **not** `generate(...)` — the probe's key
never leaves the probe (D-23). Shape is otherwise identical.

**Analog B (catalog / `authz/catalog.toml` target):** `/home/emanuele/git/priv/axiam-rust-sdk/examples/management_manifest.rs`
```rust
let desired = manifest! {
    resource workspace = "workspace", "collection";
    resource documents = "documents", "collection", under workspace;
    scope    drafts    = "draft", "Unpublished documents", in documents;
    permission read  = "document:read",  "Read a document";
    role editor = "Editor", "Edits drafts, reads everything";
    grant editor, allow read;
    grant editor, allow write, in [drafts];
    group staff = "Staff", "Everyone on the team";
    assign role editor, to group staff;
};
let plan = client.manifest().plan(&desired).await?;   // GETs only — safe against prod
let report = client.manifest().apply(&desired).await?;
assert!(report.is_complete());
```
This is the exact surface `authz/catalog.toml` must deserialize into (permissions + roles +
grants, per D-16). Four properties the example states that the planner should encode as the
verification for PLAT-05 / catalog idempotence:
1. "Nothing is ever deleted… There is no prune option, on purpose."
2. "A field the manifest does not state is never a difference."
3. "Applying twice converges: the second plan is all NoChange." ← the automated check.
4. "There is no transaction across 147 independent HTTP endpoints… Fix the cause and re-apply."
Also: "Broken manifests are refused before the first request: dangling keys, duplicate keys, a
cycle in the resource parents" — so catalog.toml validation gets a free layer from the SDK.
**Confirms DF-011:** no `metadata`, no resource-scoped group→role in the macro — the portfolio/
smoke tree and `roles().assign_to_group(.., resource_id)` stay imperative.

**Analog C (imperative CRUD conventions):** `/home/emanuele/git/priv/axiam-rust-sdk/examples/management_basics.rs`
```rust
let page = client.users().list(PageRequest::first(50).search("ada")).await?;
// page.total is the SET, not the page; page.has_more()
client.users().list_all(PageRequest::first(200)).await?  // walks to exhaustion, carries `search`
```
Use `list_all(...)` + server-side `.search(..)` for every "resolve by natural key before create"
lookup (P-8). Do not filter a single page client-side — the example explains why `total` then
belongs to a different result set.

**Analog D (hand-rolled calls):** `/home/emanuele/git/priv/axiam/scripts/e2e-bootstrap.sh`
This is the transliteration source for `hand_rolled.rs`. Three things the research summary does
**not** capture and that will otherwise cost an executor a debugging round:

1. **Session = cookie jar + CSRF header, not a bearer token** (lines ~205-219):
```bash
LOGIN_STATUS=$(curl -sS -D "${LOGIN_HEADERS}" -c "${JAR}" \
  -H "Content-Type: application/json" \
  -d "{\"org_slug\":\"${ORG_SLUG}\",\"username_or_email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}" \
  "${AXIAM_URL}/api/v1/auth/login")
CSRF=$(grep -i '^x-csrf-token:' "${LOGIN_HEADERS}" | tail -1 | tr -d '\r' | cut -d' ' -f2-)
```
→ the hand-rolled `reqwest` client needs `.cookie_store(true)` **and** must echo `X-CSRF-Token`
on every mutating call. (The SDK's own `reqwest` features include `cookies` for this reason.)
Note the login body uses `org_slug` with **no** tenant — that is what selects organization scope.

2. **The tenant-admin sequence (D-37), four calls, all with `X-Axiam-Tenant`** (lines ~268-345):
```bash
# create
curl -X POST -b "$JAR" -H "X-CSRF-Token: $CSRF" -H "X-Axiam-Tenant: ${TENANT_ID}" \
  -d '{"username":"…","email":"…","password":"…"}' "$AXIAM_URL/api/v1/users"
# activate — users are created PendingVerification
curl -X PUT  … -H "X-Axiam-Tenant: ${TENANT_ID}" -d '{"status":"Active"}' "$AXIAM_URL/api/v1/users/${ID}"
# find the tenant's seeded super-admin role
curl -b "$JAR" -H "X-Axiam-Tenant: ${TENANT_ID}" "$AXIAM_URL/api/v1/roles" \
  | jq -r '(.items // .) | map(select(.name == "super-admin")) | .[0].id // empty'
# assign, with NO resource_id → global within this tenant
curl -X POST … -H "X-Axiam-Tenant: ${TENANT_ID}" -d "{\"user_id\":\"${ID}\"}" \
  "$AXIAM_URL/api/v1/roles/${ROLE_ID}/users"
```
Header semantics, quoted: "`X-Axiam-Tenant` is how an organization-level principal says which
tenant a request is about, and it is honoured only for a principal whose own record lives in the
organization scope, in a tenant of that principal's own organization."

3. **Idempotent status handling — the exact accept-sets to reuse** (lines 184-187, 240-246, 280-296, 338-340):
```bash
if [ "${HTTP_STATUS}" != "201" ] && [ "${HTTP_STATUS}" != "409" ]; then … fi   # bootstrap
case "${TENANT_STATUS}" in 201) … ;; 409) … skipping ;; *) error ;; esac        # tenant
case "${USER_STATUS}"   in 201) id=$(jq .id) ;; 409) id=$(search by username) ;; *) error ;; esac
case "${ASSIGN_STATUS}" in 204|200|201|409) ok ;; *) error ;; esac              # role assign
```
Also `(.items // .)` — list endpoints may or may not be envelope-wrapped; handle both.
**Conflict to flag:** this script's header says a second bootstrap answers **409**; D-12 says
**403 "gate not satisfied"** for a consumed *setup token*. Both paths exist (email-gate vs
token-gate). The `org-bootstrap` stage must accept 201/409 **and** treat 403 as "probe login
instead" (P-3), not as a hard failure.

**No analog for:** the clap-derive stage CLI, `.secrets/state/` markers, the ✓/✗ checklist
renderer, the demo card (D-36). Neither sibling has a clap-based CLI
(`grep -rn "Subcommand" ../axiam/crates ../axiam/tools` → no hits; `tools/surreal-race-probe`
is the only tool crate and is excluded from the workspace). Written from scratch.

---

### `tools/domo-probe` (CLI, request-response + pub-sub)

**Partial analog:** `device_mtls_provisioning.rs` step 5 — the only SDK-sanctioned way to build a
client identity:
```rust
let device = AxiamClient::builder()
    .base_url("https://iam.example.com")?
    .tenant_id(tenant_id)
    .with_client_cert(cert_pem.as_bytes(), key_pem.as_bytes())?
    .build()?;
```

**Confirmed: there is no analog for the device login the probe needs.**
`/home/emanuele/git/priv/axiam-rust-sdk/examples/device_login.rs` (read in full) is the **OAuth 2.0
Device Authorization Grant** (§14) — `client.device_login(DeviceLoginParams{..}, |authorization| …)`
prints a `user_code` and a `verification_uri` for a human to visit. It is **not**
`POST /api/v1/auth/device` mTLS login. This independently confirms research finding C-6 / DF-009.
The executor must **not** mistake `device_login` for the mTLS call.

→ `/api/v1/auth/device` and the rumqttc/rustls connect are written from scratch against RESEARCH
Code Example 5. No sibling code exists for either.

---

### `services/device-twin` (service, request-response)

**Analog:** `/home/emanuele/git/priv/axiam-rust-sdk/examples/actix_route_guard.rs` (125 lines, read whole)

**JwksVerifier construction — the exact C-7 pattern** (lines 76-89):
```rust
let http = reqwest::Client::new();
let tenant_id: uuid::Uuid = std::env::var("AXIAM_TENANT_ID")
    .expect("AXIAM_TENANT_ID is required: the §10 guard asserts it on every token")
    .parse().expect("AXIAM_TENANT_ID must be a UUID");
let jwks_verifier = JwksVerifier::new(http, &base_url_parsed)
    .expect("failed to construct JwksVerifier")
    .expect_tenant_id(tenant_id)
    .expect_audience("axiam:user");     // ← Twin uses "axiam:m2m" (device tokens)
```
The comment states the rule the Twin depends on: "`/oauth2/jwks` is organization-wide, so a valid
signature proves only 'some tenant in this org' — without `expect_tenant_id` the verifier fails
closed on every request." → **the Twin needs one verifier per tenant**, keyed by the tenant it
asserts. That is the concrete shape of `State::verifier(tenant)` in RESEARCH Code Example 6.

**App wiring — `web::Data` is Arc-backed, build once, clone into the factory** (lines 102-121):
```rust
let jwks_data = web::Data::new(jwks_verifier);
let client_data = web::Data::new(client);
HttpServer::new(move || {
    App::new()
        .app_data(jwks_data.clone())
        .app_data(client_data.clone())
        .route("/protected", web::get().to(protected_resource))
})
.bind(&listen_addr)?
.run().await
```
→ Twin: a `web::Data<State>` holding `HashMap<tenant, JwksVerifier>` + the username cache, and
`.route("/rmq/user", web::post().to(user))` etc.

**Deltas (do not copy):** the `#[require_auth]` / `#[require_access]` / `AxiamUser` guards. The
RabbitMQ auth backend is called **by the broker**, which presents no AXIAM session — its endpoints
take `web::Form<T>` and return the plain-text `allow`/`deny` contract. Also the example binds
plain `.bind(addr)`; the Twin needs `bind_rustls_0_23` (TLS 1.3, D-07) since `auth_http.*_path`
uses `https://`.

---

### root `Cargo.toml` (workspace config)

**Analog:** `/home/emanuele/git/priv/axiam/Cargo.toml`
```toml
[workspace]
members = [
    "crates/axiam-core",
    ...
]
# `tools/surreal-race-probe` is a diagnostic, not part of the product: … CI
# should not pay for that dependency tree on every commit.
exclude = ["tools/surreal-race-probe"]
resolver = "3"
```
Copy `resolver = "3"` (edition 2024 / MSRV 1.88, matching the SDK) and the practice of a comment
justifying anything excluded. Also copy the `[workspace.lints]` **ratchet** posture described at
lines ~30-45 ("Opt-in per crate via `[lints] workspace = true`, so the ratchet can move one crate
at a time") — but given D-29's small workspace, opting every crate in from day one is cheaper.
`[workspace.dependencies]` content comes from RESEARCH § Standard Stack, not from this analog
(AXIAM's deps are a server's, not a client's).

---

### `.gitignore` / `.dockerignore` secrets guard

**Analog:** `/home/emanuele/git/priv/axiam/.gitignore:33-34` and `.dockerignore:30`
```gitignore
# Local dev secrets (JWT signing keys, TLS certs) — never commit
docker/.secrets/
```
```dockerignore
# Environment and secrets — NEVER bake into images
```
Ours: `.secrets/` and `dist/` in both files. **No analog** for D-13's pre-commit/pre-build guard
hook — written from scratch; verify with the PKI-06 checks already listed in RESEARCH's
Validation Architecture (`git check-ignore -q`, `docker save … | grep -c "PRIVATE KEY"` == 0).

---

## Shared Patterns

### Pattern S-1: `${VAR:?explanatory message}` for every required compose input
**Source:** `../axiam/docker/docker-compose.prod.yml` (lines 63, 107-108, 415-417)
**Apply to:** `deploy/compose.yml`, every service.
The message names the recipe that fixes it (`run 'just prod-up'`). Ours should say `run 'just up'`.
Paired constraint: the justfile's compose wrapper must export placeholders so `down` still works.

### Pattern S-2: one-shot `*-init` container for volume ownership
**Source:** `../axiam/docker/docker-compose.prod.yml` `surrealdb-init` (lines ~354-363) +
`vault-data-perms` (lines 33-50, with the best explanation of *why*):
> Docker creates a named volume's mount point owned by root when the path is absent from the
> image… In Kubernetes `fsGroup: 1000` does this for us; Compose has no equivalent.
**Apply to:** `tls-init` (P-6) and any new data volume. Consumers use
`condition: service_completed_successfully`.

### Pattern S-3: conf.d fragment, never a config replacement
**Source:** `../axiam/docker/rabbitmq-tls.conf:3-7`
**Apply to:** `deploy/rabbitmq/30-mqtt.conf`. Replacing `rabbitmq.conf` loses the entrypoint's
`10-defaultuser.conf` — exactly the D-26 reason for "no `definitions.json`".

### Pattern S-4: idempotent-by-probe, not idempotent-by-marker
**Source:** `../axiam/justfile` (the `sys/init` comment, lines ~464-467) +
`../axiam/scripts/e2e-bootstrap.sh` (201/409 case blocks throughout)
**Apply to:** every `domo-bootstrap` stage and every `just` stage. Markers are a skip *hint*;
the stage still resolves by natural key. Backed by the SDK manifest's own guarantee
("Applying twice converges: the second plan is all NoChange").

### Pattern S-5: `Sensitive<T>` and the once-only key
**Source:** `../axiam-rust-sdk/examples/device_mtls_provisioning.rs:69-75`
> `GeneratedCertificate::private_key_pem` is `Sensitive<String>` and is returned by this call and
> by no other… Write it to the device now or mint a new certificate later; there is no third option.
**Apply to:** the BYOK import payload, the tenant-CA response, and every log statement in
`domo-bootstrap`/`domo-probe`/the Twin. Never `{:?}` a struct that may contain one — `Display`
redacts, debug formatting of surrounding structs is the leak path. (ASVS V7.)

### Pattern S-6: `umask 077` + `chmod 600` on every minted secret
**Source:** `../axiam/justfile:404-414, 424-431`; `../axiam/scripts/gen-broker-tls.sh:75-79`
**Apply to:** everything written under `.secrets/`. Note the analog relaxes the broker key to 644
to satisfy the container user — **we must not**; that is what `tls-init` (S-2) is for.

### Pattern S-7: the `→ / ✓ / ✗ … Check: docker logs <container>` output vocabulary
**Source:** `../axiam/justfile` throughout; `../axiam/scripts/gen-broker-tls.sh:37-43, 81-88`
**Apply to:** the D-34 staged checklist and D-36's demo card. The analog's closing block
(what each file is, and how to rotate) is a good model for `just export-trust`'s printed steps.

### Pattern S-8: upstream TLS is always verified, depth 3, TLS 1.3, explicit SNI
**Source:** `../axiam/docker/nginx.conf.template:118-123` (repeated in all three proxy blocks)
**Apply to:** Caddy → axiam-server / device-twin, Twin ← broker (`auth_http.ssl_options.*`),
AXIAM → rabbitmq (AMQPS). There is no verification-skip anywhere in the AXIAM stack by design
(`gen-broker-tls.sh:6-7`: "There is deliberately no verification-skip option in AXIAM's AMQP
client"); don't introduce one to get past a SAN mismatch — fix the SAN.

---

## No Analog Found

Written from scratch; planner should use RESEARCH.md's Code Examples / Patterns instead.

| File | Role | Data flow | Reason | Use instead |
|---|---|---|---|---|
| `deploy/caddy/Caddyfile` (Caddy syntax proper) | config | request-response | Neither sibling uses Caddy (`git ls-files \| grep -i caddy` in `../axiam` → empty); AXIAM only *mentions* `caddy reverse-proxy` in a compose comment | RESEARCH Code Example 4 + Pattern 6; route map from `nginx.conf.template` |
| `deploy/rabbitmq/enabled_plugins` | config | n/a | AXIAM never enables MQTT or the HTTP auth backend | RESEARCH Code Ex. 3 footnote; assumption A11 (include management + prometheus explicitly) |
| `deploy/landing/index.html` | static asset | n/a | No static landing page anywhere in the siblings | D-31 + the CSP from `nginx.conf.template:69` |
| `deploy/postgres/*` | config | n/a | AXIAM uses SurrealDB, not PostgreSQL | RESEARCH; P-6 for key ownership |
| `tools/domo-bootstrap` CLI skeleton (clap subcommands, stage markers, checklist, demo card) | CLI | batch | No clap-derive CLI exists in `../axiam` or `../axiam-rust-sdk` | D-34/D-36; RESEARCH Pattern 2 |
| `crates/domo-common` | library | n/a | No SDK-consumer helper crate exists | D-29; RESEARCH's project structure |
| `tools/domo-probe` — `POST /api/v1/auth/device` | HTTP client | request-response | **Verified absent from the SDK.** `examples/device_login.rs` is the OAuth Device Authorization Grant (§14), a different thing entirely | RESEARCH Code Ex. 5; log DF-009 |
| `tools/domo-probe` — rumqttc/rustls MQTT | client | pub-sub | No MQTT client anywhere in the siblings (AXIAM speaks AMQP via lapin) | RESEARCH Code Ex. 5 + Pattern 5 |
| `services/device-twin` `/rmq/*` endpoint bodies | service | request-response | RabbitMQ's HTTP-backend contract has no representation in the siblings | RESEARCH Pattern 5 (verified against `rabbit_auth_backend_http.erl`) + Code Ex. 6 |
| `authz/catalog.toml` (the TOML schema itself) | config | n/a | AXIAM seeds roles in Rust (`axiam-db` seeder), not from a file | D-16; deserialize into the SDK's `ManagementManifest` (`management_manifest.rs`) |
| `docs/dogfooding-findings.md` | doc | n/a | No findings-log format exists in either sibling | D-32/D-33; seed IDs DF-008…DF-020 from RESEARCH |
| `.secrets` pre-commit/pre-build guard | script | n/a | AXIAM relies on `.gitignore` alone | D-13; verify per RESEARCH's PKI-06 row |

---

## Notes for the Planner (found during mapping, not in RESEARCH.md)

1. **Hand-rolled AXIAM calls need a cookie jar + `X-CSRF-Token`, not a bearer header.**
   `e2e-bootstrap.sh:205-219` reads the CSRF token off the login *response headers* and echoes it
   on every mutating request, with `-b/-c` cookie jar. `hand_rolled.rs` must build its `reqwest`
   client with `.cookie_store(true)` and thread the CSRF token. Missing this reads as a 403 that
   looks like an authorization bug. Budget a task for it.
2. **The bootstrap re-run status is ambiguous across gates.** `e2e-bootstrap.sh` documents 409 on
   a second bootstrap (email gate); D-12 documents 403 for a consumed setup token. The
   `org-bootstrap` stage must handle 201 / 409 / 403 and fall through to the login probe (P-3).
3. **`AXIAM__SERVER__TLS__*` and `AXIAM__AMQP__TLS__*` already exist as compose knobs** in the
   prod file — Phase 1 flips values, it does not invent variables. Lowers the risk on RESEARCH's
   MEDIUM-confidence compose rating.
4. **AXIAM's own Dockerfile does *not* cross-compile** (`FROM rust:1.97-bookworm AS builder`, no
   `$BUILDPLATFORM`) — RESEARCH Pattern 8's cargo-zigbuild approach has **no precedent in this
   codebase family**, contrary to the general impression. The transferable parts of
   `Dockerfile.server` are the distroless `nonroot` runtime, `COPY --from=builder --chown=65532:65532`,
   numeric `USER 65532:65532` (with its stated Kubernetes rationale), and the digest-pinned bases.
   Treat the arm64 cross-build as genuinely new work with the "build natively on the Pi" fallback.
5. **`prod-clean`'s keep/discard split is the precedent for D-09 vs D-10** and is worth quoting in
   the justfile: inputs (root key) survive; state (volumes, markers) does not.

## Metadata

**Analog search scope:** `/home/emanuele/git/priv/axiam/{docker,scripts,justfile,Cargo.toml,.gitignore,.dockerignore,crates,tools}`,
`/home/emanuele/git/priv/axiam-rust-sdk/{examples,Cargo.toml,CONTRACT.md}`.
Other SDK checkouts (`axiam-java-sdk`, `axiam-c-sdk`, `axiam-cplusplus-sdk`, `axiam-typescript-sdk`)
were not searched: no Phase 1 file is written in those languages.
**Files read in full:** `rabbitmq-tls.conf`, `gen-broker-tls.sh`, `actix_route_guard.rs`,
`device_login.rs`, `device_mtls_provisioning.rs`, `management_manifest.rs`.
**Files read in targeted ranges:** `docker-compose.prod.yml`, `justfile`, `e2e-bootstrap.sh`,
`Dockerfile.server`, `nginx.conf.template`, `management_basics.rs`, `Cargo.toml`.
**Tracked-source gate:** every cited path verified with `git ls-files` in its own repo. No
gitignored mirror paths emitted.
**Pattern extraction date:** 2026-09-19
