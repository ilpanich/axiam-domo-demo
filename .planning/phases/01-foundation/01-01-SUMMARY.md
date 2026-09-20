---
phase: 01-foundation
plan: 01
subsystem: infra
tags: [pki, mtls, rustls, rabbitmq, mqtt, actix-web, axiam-sdk, docker-compose, just, byok]

requires: []
provides:
  - "Offline RSA-4096 organization root, generated once and imported into AXIAM with its private key (BYOK), anchored as the single mTLS trust anchor"
  - "One Ed25519 tenant signing CA per tenant, issued by AXIAM beneath that root"
  - "A device service account whose locally-generated Ed25519 CSR is signed by its tenant CA and bound to the account"
  - "Device mTLS login at POST /api/v1/auth/device returning a Bearer JWT"
  - "MQTT CONNECT on the `domo` vhost authorized by the Device Twin's RabbitMQ HTTP auth backend"
  - "Cargo workspace (domo-common, domo-bootstrap, domo-probe, device-twin) with axiam-sdk pinned"
  - "deploy/compose.yml stack: surrealdb(+init), tls-init, axiam-server, rabbitmq, device-twin, one-shot tools"
  - "`just tracer` — one command from an empty machine to an accepted MQTT CONNECT"
affects: [01-02, 01-03, 01-04, 01-05, 01-06, 01-07, device-twin, simulators, management-platform]

actuals:
  tokens: 38598
  tasks: 2
  commits: 7
  plan_head_before: d10918a06d423b1d9db06ba402542761bf5c9026

tech-stack:
  added:
    - "axiam-sdk =1.0.0-beta16 (crates.io, pinned exactly, features=[rest])"
    - "actix-web 4.15 (rustls-0_23), rumqttc 0.25, rustls 0.23, reqwest 0.13, rcgen 0.14, x509-parser 0.18"
    - "ghcr.io/ilpanich/axiam/server:1.0.0-beta16, rabbitmq:4.3-management-alpine (4.3.6), surrealdb/surrealdb:v3"
    - "gcr.io/distroless/cc-debian12:nonroot runtime; rust:1.98-bookworm builder"
  patterns:
    - "Four-hop identity chain: cert DN -> MQTT client_id -> MQTT username -> JWT sub, each hop enforced by a different component"
    - "Idempotency by probe, not by marker: every stage resolves by natural key first; .done files are skip hints only"
    - "Pure decision core: authorization logic as free functions, unit-tested without a server or broker"
    - "One-shot compose tooling on the compose network, so the demo host needs no cargo toolchain"
    - "tls-init publishes 0600 host key material into per-service named volumes (Compose ignores secrets uid/mode outside swarm)"

key-files:
  created:
    - "scripts/gen-pki.sh — offline root (once, guarded) + SAN leaves per run"
    - "deploy/pki/listeners.conf — listener -> SAN table, server vs client rows"
    - "deploy/compose.yml — the whole tracer stack"
    - "deploy/rabbitmq/{enabled_plugins,20-tls.conf,30-mqtt.conf}"
    - "deploy/docker/Dockerfile.rust — distroless twin + tools images"
    - "crates/domo-common/src/{tls,axiam,hand_rolled,topic,secrets}.rs"
    - "tools/domo-bootstrap/src/stages/{org_bootstrap,tenants,pki,device_identity,broker}.rs"
    - "tools/domo-probe/src/main.rs — gen-csr and connect"
    - "services/device-twin/src/{main,rmq}.rs — the RabbitMQ HTTP auth backend"
    - "justfile + just/stack.just — the `tracer` recipe"
  modified: []

key-decisions:
  - "axiam-sdk pinned to crates.io =1.0.0-beta16 (disposition (a)); no path dependency, so the Dockerfile keeps a repo-root-only build context and D-02 holds"
  - "device-identity split into two phases (device-account, then device-identity): the CSR subject must be CN=<service-account UUID>, which cannot exist before the account does"
  - "Tenant map published through a shared named volume rather than a bind mount of .secrets/state, which is 0700 because it holds the root key"
  - "AXIAM__RATE_LIMIT__LOGIN_PER_MIN raised to 120: the staged bootstrap logs in ~8 times per run against a default of 10/min, which made a second consecutive run impossible"
  - "Certificate reuse is gated on public-key equality with the CSR, not merely on account binding"

patterns-established:
  - "Natural-key idempotency: list/find/create-if-absent for every AXIAM object"
  - "Secrets never logged: identity fields at debug, tokens never; Sensitive values dropped undisplayed"
  - "Guard markers in compose (`operator` vs `generated`) so plan 01-07 can derive .env.example from the compose file alone"

requirements-completed: [PLAT-01, PKI-01, PKI-02, PKI-03, PKI-04, PKI-06, MQTT-01, MQTT-02]

coverage:
  - id: D1
    description: "Offline organization root generated once, never rotated by a reset, and byte-identical across consecutive runs"
    requirement: PKI-04
    verification:
      - kind: integration
        ref: "sha256sum .secrets/pki/root.{key,pem} compared before and after two `just tracer` runs"
        status: pass
      - kind: integration
        ref: "openssl x509 -noout -text -in root.pem — 4096 bit, basicConstraints critical CA:TRUE, no pathlen"
        status: pass
    human_judgment: false
  - id: D2
    description: "AXIAM holds exactly one mTLS trust anchor and it is the offline-generated root"
    requirement: PKI-01
    verification:
      - kind: integration
        ref: "GET /api/v1/organizations/{org}/ca-certificates — 1 CA with mtls_trust_anchor==true, SHA-256 fingerprint equals local root.pem"
        status: pass
    human_judgment: false
  - id: D3
    description: "Exactly one AXIAM-issued signing CA per demo tenant, chaining to the offline root"
    requirement: PKI-02
    verification:
      - kind: integration
        ref: "GET .../tenants/{id}/signing-cas — lakeside: 1, organization: 0; openssl verify -CAfile root.pem lakeside-ca.pem"
        status: pass
    human_judgment: false
  - id: D4
    description: "Device leaf signed by the tenant CA, bound to its service account, verifying root -> tenant CA -> leaf"
    requirement: PKI-03
    verification:
      - kind: integration
        ref: "openssl verify -CAfile root.pem -untrusted lakeside-ca.pem probe/leaf.pem => OK"
        status: pass
    human_judgment: false
  - id: D5
    description: "Device mTLS login returns a Bearer token whose sub is the device's service-account UUID"
    requirement: MQTT-02
    verification:
      - kind: e2e
        ref: "just _probe connect — POST /api/v1/auth/device over mTLS, token_type Bearer, non-empty access_token"
        status: pass
    human_judgment: false
  - id: D6
    description: "MQTT CONNECT accepted on the domo vhost with cert + JWT, with publish and subscribe round trip"
    requirement: MQTT-01
    verification:
      - kind: e2e
        ref: "just tracer — CONNACK Success, SubAck, round trip on domo/lakeside/<sa>/reported"
        status: pass
      - kind: integration
        ref: "rabbitmq log: \"Accepted MQTT connection ... for client ID CN=<sa-uuid>\""
        status: pass
    human_judgment: false
  - id: D7
    description: "The four-hop identity chain refuses a mismatched client_id, token subject, vhost, queue or topic"
    verification:
      - kind: unit
        ref: "services/device-twin/src/rmq.rs — 10 tests covering every deny path"
        status: pass
    human_judgment: false
  - id: D8
    description: "No key material in git or in any produced image layer"
    requirement: PKI-06
    verification:
      - kind: integration
        ref: "git check-ignore .secrets/pki/root.key; git ls-files .secrets (empty); docker save | scan for complete PEM key blocks => 0"
        status: pass
    human_judgment: false
  - id: D9
    description: "AXIAM's own `/` vhost and default user survive; its AMQPS link stays up under broker-wide fail_if_no_peer_cert"
    verification:
      - kind: integration
        ref: "rabbitmqctl list_vhosts lists / and domo; GET /api/connections shows AXIAM's AMQP 0-9-1 connection on /"
        status: pass
    human_judgment: false
  - id: D10
    description: "One command takes an empty machine to an accepted MQTT CONNECT, repeatably"
    requirement: PLAT-01
    verification:
      - kind: e2e
        ref: "just tracer run consecutively — both report `✓ tracer`, second creates no new AXIAM object"
        status: pass
    human_judgment: false

duration: 72min
completed: 2026-09-20
status: complete
---

# Phase 1 Plan 01: Trust-Anchor Tracer Summary

**One offline RSA-4096 root imported into AXIAM by BYOK, anchoring an Ed25519 tenant CA that signs a device certificate whose CN carries through MQTT client_id, username and JWT `sub` to an accepted CONNECT on the `domo` vhost — reproducible with `just tracer`.**

## Performance

- **Duration:** 72 min
- **Started:** 2026-09-20T11:07:27+02:00
- **Completed:** 2026-09-20T12:19:45+02:00
- **Tasks:** 2
- **Files modified:** 33

## Accomplishments

- **The architectural bet holds.** The whole chain was proven against a real AXIAM (`1.0.0-beta16`) and a real RabbitMQ (4.3.6), not a mock: offline root → BYOK import → mTLS trust anchor → tenant signing CA → device CSR signed and bound → mTLS device login → MQTT CONNECT → Twin authorization decision.
- **`just tracer` runs green twice in a row**, the second run creating no new AXIAM object — every stage resolves by natural key and reuses.
- **Both research assumptions resolved empirically** (A1 and A2 below), plus P-9, which the plan explicitly asked to determine.
- The Device Twin's authorization core is a set of pure functions with 10 unit tests covering every deny path, callable without a server — which is what plan 01-05 was promised.

## Task Commits

1. **Task 1: Package legitimacy gate (`axiam-sdk` `[SUS]`)** — `d667f3e` (chore)
2. **Task 2: End-to-end tracer** — `f4255d7`, `9233563`, `b49397a`, `7b48b9f`, `1bcb051` (feat/docs)

Task 2 is one logical task committed in four working increments plus a documentation commit, so that a failure partway through the live-stack work could not lose the layers already proven.

## Files Created/Modified

See `key-files.created` in the frontmatter. Notable:

- `scripts/gen-pki.sh` — the only place the root is generated; guarded by `.secrets/state/pki-root.done` so a reset never rotates it.
- `services/device-twin/src/rmq.rs` — hops 3 and 4 of the identity chain, as pure functions.
- `tools/domo-bootstrap/src/stages/pki.rs` — the three calls (import, anchor, generate) that are the point of the whole plan.
- `just/stack.just` — the `tracer` recipe and the readiness/scrape logic `just` owns on the host.

## Decisions Made

- **`axiam-sdk` disposition (a)** — pinned `=1.0.0-beta16` from crates.io. Provenance cleared at the Task 1 human gate: published 2026-09-19, not yanked, sole owner `ilpanich`, sha256 `5a97ee60…74a9083b`, metadata identical to the local checkout at `44d9a49`. The `[SUS]` flag is explained entirely by a new first-party pre-release line (679 downloads), not by a provenance problem. Keeping crates.io (rather than a `path` dependency) is what lets `Dockerfile.rust` keep a repo-root-only build context, so D-02 and the "no build context outside the repository" criterion both hold unchanged.
- **`AXIAM_IMAGE_TAG` actually pulled:** `ghcr.io/ilpanich/axiam/server:1.0.0-beta16`, digest `sha256:2e6be78c13840cd98330ccc1e30e96cc539b227ee0d072094fb24c6b4bf7f97d`. Broker: `rabbitmq:4.3-management-alpine` resolving to RabbitMQ **4.3.6**.
- **Device identity is two-phase.** The plan described `device-identity` as taking a CSR, but the CSR's subject must be `CN=<service-account UUID>` — which cannot exist until the account does. Split into `device-account` (create/resolve the account) and `device-identity` (sign + bind).
- **Login rate limit raised to 120/min.** Defensible only because this is a LAN-only demo; recorded as such in the compose comment.

## Assumptions Resolved

| Assumption | Outcome |
|---|---|
| **A1** — RabbitMQ renders a CN-only subject as exactly `CN=<value>` for `ssl_cert_client_id_from = distinguished_name` | **CONFIRMED.** The Twin observed `client_id=Some("CN=01a0be43-…")`. No change to `domo-probe`'s `client_id` format was needed. |
| **A2** — Erlang/OTP TLS 1.3 accepts an Ed25519 leaf under an Ed25519 tenant CA under an RSA-4096 root with no EKU | **CONFIRMED.** The chain was accepted; the broker reached the HTTP auth backend and later logged `Accepted MQTT connection`. D-11's `KeyAlgorithm::Rsa4096` fallback was **not** needed. |
| **A4** — later `conf.d` fragment overrides an earlier `ssl_options.verify` | Neutralised as planned: `20-tls.conf` is a merged file stating the final value directly, so override order is never relied on. |
| **A11** — the image's default `enabled_plugins` | Neutralised as planned: the file is written explicitly with all four plugins. |
| **P-9** — which PKI encryption key variable name the image honours | **RESOLVED: `AXIAM__AUTH__PKI_ENCRYPTION_KEY`.** Proven by setting the two spellings to *different* values: signing a CSR (which must decrypt the tenant CA's private key) still succeeded while `AXIAM__PKI__ENCRYPTION_KEY` held a wrong value, so the unprefixed spelling is ignored entirely. Both remain set to the same value so a future image that switches would not fail with a decryption error. |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Signing CA minted for the reserved `organization` tenant**
- **Found during:** Task 2 (pki stage, first live run)
- **Issue:** The stage looped over every tenant AXIAM returned, including the reserved `organization` tenant, creating a CA nothing would ever issue from and breaking the "one signing CA per tenant, no more and no fewer" count.
- **Fix:** Skip `ORG_TENANT_SLUG`.
- **Verification:** `GET .../tenants/{id}/signing-cas` reports `lakeside: 1`, `organization: 0`.
- **Committed in:** `7b48b9f`

**2. [Rule 1 - Bug] Doubled `CN=` in the tenant CA subject**
- **Found during:** Task 2 (pki stage)
- **Issue:** Passing `subject: "CN=<name>"` produced a subject of `CN=CN=Lakeside Residences Signing CA` — AXIAM builds the DN itself.
- **Fix:** Pass the bare common name.
- **Verification:** Re-issued on a clean volume; `openssl x509 -subject` reads correctly.
- **Committed in:** `7b48b9f`

**3. [Rule 1 - Bug] Certificate reuse ignored key identity**
- **Found during:** Task 2 (second tracer run)
- **Issue:** `device-identity` reused any certificate bound to the account, while `gen-csr` minted a fresh keypair each run. The result was an mTLS identity whose leaf and key disagreed, surfacing far away as `building the device HTTP client`.
- **Fix:** Reuse only when the certificate's SubjectPublicKeyInfo matches the CSR's; and make `gen-csr` idempotent per account.
- **Verification:** Four consecutive `just tracer` runs, all green, with full reuse.
- **Committed in:** `7b48b9f`

**4. [Rule 1 - Bug] Publish asserted on PubAck rather than on the round trip**
- **Found during:** Task 2 (first successful CONNECT)
- **Issue:** The probe required a PubAck *before* the echoed message, but the broker may deliver the subscribed copy first; the loop exited on the round trip and then failed a run that had in fact proven more than the PubAck could.
- **Fix:** The round trip is the assertion; the PubAck is diagnostic.
- **Committed in:** `7b48b9f`

**5. [Rule 3 - Blocking] Tenant map unreadable by the Twin**
- **Found during:** Task 2 (first CONNECT reaching the Twin)
- **Issue:** The map was written into `.secrets/state` at 0600 under the host uid; the Twin runs as 65532 and `.secrets` is 0700 because it holds the root key, so every tenant lookup failed closed with a reason that pointed at the tenant rather than at the file mode.
- **Fix:** Publish it through a shared named volume (`twin-state`) chowned by `tls-init`. `.secrets` permissions were deliberately **not** weakened.
- **Committed in:** `7b48b9f`

**6. [Rule 3 - Blocking] Login rate limit made a second run impossible**
- **Found during:** Task 2 (two consecutive runs)
- **Issue:** The staged bootstrap performs ~8 logins per run (each stage is a separate one-shot process), against AXIAM's default 10/min. The second run failed mid-way in a manner that reads as an authentication bug.
- **Fix:** `AXIAM__RATE_LIMIT__LOGIN_PER_MIN` raised to 120, documented as LAN-only-demo scope.
- **Committed in:** `7b48b9f`

**7. [Rule 2 - Missing Critical] Silently-ignored secrets**
- **Found during:** Task 2 (startup log review)
- **Issue:** `AXIAM__EMAIL_ENCRYPTION_KEY` and `AXIAM__GDPR_PSEUDONYM_PEPPER` were demonstrably present in the container yet AXIAM logged them as missing — the env secret provider resolves logical keys under `AXIAM__AUTH__<NAME>`.
- **Fix:** Set both spellings. Neither is on the tracer's path, but a silently-ignored secret is a trap for the next person.
- **Verification:** Both warnings gone after recreate.
- **Committed in:** `7b48b9f`

**8. [Rule 3 - Blocking] Preflight port check blocked repeat runs**
- **Found during:** Task 2
- **Issue:** Preflight failed when 8090/8883 were held by *our own* running stack — the normal state of a second `just tracer`.
- **Fix:** Skip the port check when this compose project already has containers.
- **Committed in:** `7b48b9f`

---

**Total deviations:** 8 auto-fixed (4 bugs, 3 blocking, 1 missing-critical).
**Impact on plan:** No scope creep. Every fix was required to make the plan's own acceptance criteria reachable; six of the eight were only discoverable by running the stack for real, which is exactly what a tracer is for.

## Acceptance Criteria Notes

Two criteria were satisfied in substance but needed a refined instrument, recorded here rather than quietly reinterpreted:

- **"`docker save` yields a count of zero for the standard PEM private-key header."** As written this is unsatisfiable by *any* image containing OpenSSL: the distroless base's `libcrypto.so.3` and `engines-3/loader_attic.so` contain the header as a parser *string literal*. Measured instead as **complete PEM private-key blocks** (header + ≥200 base64 chars): **0 in both images**. The header-substring count is 3 (twin) and 4 (tools), all inside base-image OpenSSL, none in our binaries. Plan 01-07's verify gate should adopt the block-level test.
- **"The probe's MQTT session appears in `GET /api/connections`."** The probe's session is sub-second, so a polling snapshot is a race. Used the broker's own durable record instead: `Accepted MQTT connection … for client ID CN=<sa-uuid>`, observed on every run. A `--linger` flag on the probe would make the snapshot form testable if a later plan wants it.

## Issues Encountered

- **The setup token was lost to a container recreate (DF-019/P-3 reproduced exactly).** Adding a port mapping recreated `axiam-server`, discarding the first-boot log that carries the single-use token. Recovery was a SurrealDB volume wipe — by explicit name, never `docker volume prune`. This is inherent to the design and is why `just demo-reset` (plan 01-02) matters.
- **The root-generation guard fired correctly during that recovery.** A careless `rm` of `.done` markers removed `pki-root.done`; `gen-pki.sh` refused to overwrite the existing root rather than silently regenerating it. `demo-reset` must clear every marker *except* `pki-root.done`.

## User Setup Required

None beyond the one-time network access the plan already notes: the pinned AXIAM image is pulled from `ghcr.io` on first run (no login needed for public packages). After that the demo is LAN-only. The five operator keys (`AXIAM_IMAGE_TAG`, `DOMO_HOST`, `DOMO_LAN_IP`, `DOMO_ORG_SLUG`, `COMPOSE_PROJECT_NAME`) live in a gitignored `.env`; plan 01-07 Task 3 documents them in `.env.example`.

## Next Phase Readiness

Ready. The skeleton plans 01-02…01-07 extend is in place and proven:

- **01-02** (`demo-reset`, `pki-rotate-root`, findings log): the marker scheme, the named volumes and the `pki-root.done` guard are all in place. Task 4's findings log should record `AXIAM_IMAGE_TAG=1.0.0-beta16` and `axiam-sdk =1.0.0-beta16`, and can take the four new findings below.
- **01-05**: the Twin's decision core is already pure and unit-tested.
- **01-06**: every negative case has a positive counterpart proven here to invert.
- **01-07**: the compose guard markers are in place, so `.env.example` can be derived from `deploy/compose.yml` alone; the operator list is exactly the five keys.

**New dogfooding findings for plan 01-02 Task 4** (beyond the drafted DF-008…DF-020):

1. The env secret provider resolves logical keys under `AXIAM__AUTH__<NAME>`; `AXIAM__EMAIL_ENCRYPTION_KEY` and `AXIAM__GDPR_PSEUDONYM_PEPPER` are accepted into the container and then ignored, warning as "missing". Generalises DF-018 beyond the PKI key.
2. DF-018 resolved: `AXIAM__AUTH__PKI_ENCRYPTION_KEY` is the honoured spelling on `1.0.0-beta16`.
3. `generate_signing_ca` takes a bare common name in `subject`, not a `CN=`-prefixed DN; passing the prefixed form yields `CN=CN=<name>` with no error.
4. The default login rate limit (10/min) is below what a staged, multi-process bootstrap needs; any scripted provisioning of more than ~10 stages per minute must raise it.

**Concern to carry forward:** `AXIAM__RATE_LIMIT__LOGIN_PER_MIN=120` is a deliberate relaxation. It is correct for a LAN-only demo and must not be copied into anything internet-reachable; plan 01-07's documentation should say so where an operator will see it.

---
*Phase: 01-foundation*
*Completed: 2026-09-20*

## Self-Check: PASSED

- All 15 claimed key files exist on disk.
- All 6 claimed task commits resolve in `git log` (`d667f3e`, `f4255d7`, `9233563`, `b49397a`, `7b48b9f`, `1bcb051`), plus this summary's own metadata commit.
- Plan `<verification>` re-run at close-out: `just tracer` green twice consecutively; `cargo build --workspace --locked && cargo test --workspace` green (17 tests, 0 failures); `openssl verify` passes at every tier; both `/` and `domo` vhosts present; nothing under `.secrets/` tracked by git and no complete PEM key block in either produced image.
- Disk hygiene: build outputs trimmed (3.6G to 2.2G); 33G free on `/home`, 35G on `/`.
