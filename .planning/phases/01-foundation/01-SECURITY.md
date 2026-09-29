---
phase: "01"
slug: "foundation"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-09-29"
---

# Phase 01 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| LAN → `rabbitmq:8883` | Untrusted device MQTT. TLS 1.3, `verify_peer`, `fail_if_no_peer_cert`, CA bundle = org root. | Device cert, JWT as MQTT password (secret) |
| LAN → `axiam-server:8090` | Untrusted device and browser traffic; native mTLS with `client_auth = optional`. | Device certs, credentials, tokens (secret) |
| LAN browser → `caddy:443` | Only browser-facing entry point. Untrusted requests and headers. | HTTP requests, session cookies |
| `{DOMO_HOST}` ↔ `axiam.{DOMO_HOST}` | Cookie boundary: AXIAM session cookies are host-scoped. | Super-admin vs portal sessions (secret) |
| `caddy` → `axiam-server` / `device-twin` | Internal hops, TLS-verified against the offline root with explicit server name. | Proxied requests |
| `rabbitmq` → `device-twin:8443` | Every CONNECT/publish/subscribe decision; caller holds no AXIAM session. | JWT (secret), identity fields, decisions |
| `axiam-server` → `rabbitmq:5671` | AXIAM's own AMQPS link, forced to present a client cert. | AXIAM events |
| device token → verified claims | Only identity assertion the Twin sees; arrives from an untrusted device. | JWT (secret) |
| tenant `lakeside` ↔ tenant `summit` | The isolation the demo exists to show: separate admins, signing CAs, service credentials, verifiers, topic namespaces. | Tokens, certs, topics |
| organization scope ↔ tenant scope | Organization principal acting inside a tenant (acting-tenant header). | Tenant-scoped writes |
| `domo-bootstrap` → AXIAM REST | Privileged: users, super-admin roles, CA import, certificate signing. | Super-admin/tenant-admin creds, root key once (BYOK) (critical) |
| operator host → container network | Root private key crosses exactly once, as the BYOK import over TLS. | `root.key` (critical) |
| `.secrets/` → git index / Docker build context | Most likely escape routes for the root key. | Private keys, passwords (critical) |
| `.secrets/` → filesystem / demo card | Tenant-admin passwords, service keys, super-admin login at mode 0600. | Credentials (secret) |
| `dist/trust/` → operator trust stores | Human action granting the root authority over every TLS connection. | Root cert + fingerprint (public) |
| operator host → Docker daemon | Volume removal and image builds; blast radius includes the sibling checkout's data. | Volumes, images |
| stage markers → stage execution | Markers decide what is skipped; a trusted-as-truth marker is a correctness boundary. | Stage state |
| verify-pki / compose → phase gate | Gate exit status is treated as proof PKI-01/PKI-04 hold; compose output is untrusted input. | Verdicts |
| smoke fixtures ↔ Phase 2 seed | Two writers in one tenant, separated by a naming prefix. | Fixture resources |
| build machine → Raspberry Pi | Images cross the architecture boundary. | Images |

---

## Threat Register

81 threats from the `<threat_model>` blocks of plans 01-01 … 01-09. Plans 01-01 and 01-09 both use the ID `T-01-SC`; the 01-09 one is written `T-01-SC (01-09)` here. Evidence is from the 2026-09-29 static audit (paths relative to the repo root, or to the crate/directory named earlier in the same cell).

| Threat ID | Category | Component | Severity | Disposition | Mitigation / Evidence | Status |
|-----------|----------|-----------|----------|-------------|-----------------------|--------|
| T-01-01 | Spoofing | MQTT CONNECT path (broker + `/rmq/user`) | high | mitigate | `deploy/rabbitmq/20-tls.conf:47-48`, `30-mqtt.conf:77`, `services/device-twin/src/rmq/decide.rs:240-256` | closed |
| T-01-02 | Spoofing / Elevation of privilege | AXIAM leaf issuance | medium | accept | accepted in plan; DF-014, DF-017, DF-025 in `docs/dogfooding-findings.md` (DF-025 shows the residual realised end to end) | closed |
| T-01-03 | Information disclosure | `.secrets/pki/root.key` | critical | mitigate | `.gitignore:4`, `.dockerignore:4`, `scripts/gen-pki.sh:24,72`, `stages/pki.rs:81-89`, `scripts/guard-secrets.sh:79-93,179-208` | closed |
| T-01-04 | Information disclosure | broker logs, container logs, `.secrets/state/setup-token` | high | mitigate | `30-mqtt.conf:91` (POST), `rmq.rs:90-96`, `just/stack.just:174-185`, `stages/pki.rs:162` | closed |
| T-01-05 | Spoofing | `axiam-server` client-certificate extraction | high | mitigate | `deploy/compose.yml:162-171` (flag absent; AXIAM default off) | closed |
| T-01-06 | Denial of service | AXIAM ↔ RabbitMQ AMQPS link | high | mitigate | `compose.yml:201-203`, `deploy/pki/listeners.conf:46` | closed |
| T-01-07 | Spoofing | `device-twin` `/rmq/*` endpoints | medium | mitigate | `compose.yml:345-374` (no ports), `30-mqtt.conf:99-100`, `main.rs:37`, `rmq.rs:107-130` | closed |
| T-01-08 | Tampering | build inputs | high | mitigate | `compose.yml:154,490`, `.env.example:41`, `Dockerfile.rust:120` (`--locked`), Cargo.lock tracked | closed |
| T-01-SC | Tampering | cargo installs | high | mitigate | `Cargo.toml:27-33` (`=1.0.0-beta16`, provenance from 2026-09-20 human gate) | closed |
| T-02-01 | Information disclosure | `.secrets/pki/root.key` reaching a commit | critical | mitigate | `.githooks/pre-commit:16` → `guard-secrets.sh:48-76,79-93`; `core.hooksPath=.githooks` | closed |
| T-02-02 | Information disclosure | root key baked into an image layer | critical | mitigate | `guard-secrets.sh:96-106,159-208`, run from `just/verify.just:76-77` | closed |
| T-02-03 | Spoofing | a wrong or substituted root installed on the operator's machine | high | mitigate | `just/pki.just:249,258`, `deploy/landing/index.html:85-89`, `docs/trust.md:42-64`, `just/edge.just:220-224` | closed |
| T-02-04 | Elevation of privilege | a demo root left permanently trusted on a reviewer's machine | medium | mitigate | `docs/trust.md:98,135,185,242`, `edge.just:117-119` | closed |
| T-02-05 | Tampering | a certificate with a weakened or missing extension passing unnoticed | high | mitigate | `scripts/verify-pki.sh:183-190,229-288` | closed |
| T-02-06 | Repudiation | a rotated root leaving machines silently trusting a dead anchor | low | accept | accepted in plan; `pki.just:285-317`, `docs/trust.md:255-272` | closed |
| T-02-07 | Repudiation | a hand-rolled AXIAM workaround with no recorded finding | medium | mitigate | `verify.just:79-129` (findings audit) | closed |
| T-02-SC | Tampering | package installs | low | accept | accepted in plan (no installs) | closed |
| T-03-01 | Spoofing | forged forwarded-client-certificate header at the edge | high | mitigate | `deploy/caddy/Caddyfile:71,98,185`, `edge.just:110-114` | closed |
| T-03-02 | Elevation of privilege | console super-admin session colliding with a portal user session | high | mitigate | `Caddyfile:171`, `edge.just:237-243` | closed |
| T-03-03 | Tampering | a matcher written in the wrong order routing Phase 2/3 API calls into AXIAM | high | mitigate | `Caddyfile:59,69` before `:89`, `edge.just:75-76,189` | closed |
| T-03-04 | Spoofing | a substituted upstream impersonating `axiam-server` | high | mitigate | `Caddyfile:73-74,100-101,187-188`, `edge.just:95-106` | closed |
| T-03-05 | Spoofing | Caddy minting its own certificate and presenting an unrelated chain | high | mitigate | `Caddyfile:43`, `edge.just:45-48` | closed |
| T-03-06 | Information disclosure | the root certificate or key offered for download from the front door | medium | mitigate | `edge.just:117-119,227-232` | closed |
| T-03-07 | Information disclosure | PostgreSQL reachable from the LAN | high | mitigate | `compose.yml:382-386`, `edge.just:330-336` | closed |
| T-03-08 | Tampering | injected script on the landing page becoming an XSS foothold for Phase 5 | medium | mitigate | `Caddyfile:116,155` (CSP); landing page has no script | closed |
| T-03-09 | Denial of service | a certificate re-issue during `just demo-reset` dropping live connections | low | mitigate | `edge.just:275-278` (`caddy reload`); not called automatically — demo-reset does a full down/up | closed |
| T-03-SC | Tampering | image installs | medium | mitigate | `compose.yml:387,434,490` (tag-pinned) | closed |
| T-04-01 | Tampering | tenant-scoped writes landing in the reserved organization tenant | high | mitigate | `tools/domo-bootstrap/src/stages/mod.rs:127,215`, `tenants.rs:152`, `service_certs.rs:255`, `verify.just:268` | closed |
| T-04-02 | Elevation of privilege | a certificate for one tenant issued under another tenant's signing CA | high | accept (was mitigate) | **re-dispositioned to accept (AR-01).** Our side: `service_certs.rs:73,173,247-253`. AXIAM still issues across tenants (DF-017); `cross-tenant-ca-issuance` red | closed |
| T-04-03 | Elevation of privilege | the organization super-admin credential leaking into a service | high | mitigate | `stages/mod.rs:60-65`; device-twin has no `.secrets` mount | closed |
| T-04-04 | Information disclosure | tenant-admin passwords or service private keys at loose permissions | high | mitigate | `crates/domo-common/src/secrets.rs:67-86`; the ≤0600 check was a one-off, no recurring gate | closed |
| T-04-05 | Elevation of privilege | a deny rule created above an apartment or device node | medium | mitigate | `authz/catalog.toml:15-20`, `tree.rs:24-28,228`, `smoke/assertions.rs:448-473` | closed |
| T-04-06 | Tampering | duplicate sibling resources making an authorization decision ambiguous | medium | mitigate | `tree.rs:96-108,138-146`, `smoke.rs:225-228`, `assertions.rs:209` | closed |
| T-04-07 | Spoofing | a group name collision granting unintended scope | medium | mitigate | `naming.rs:118-150`, `tests/naming.rs` | closed |
| T-04-08 | Repudiation | a hand-rolled call with no corresponding finding entry | low | mitigate | `crates/domo-common/src/hand_rolled.rs`: `new`, `is_authenticated`, `get`, `post`, `put`, `items`, `find_by` cite no DF- id; file-level audit (`verify.just:100-106`) partly covers it | open — below high threshold (non-blocking) |
| T-04-SC | Tampering | cargo installs | low | accept | accepted in plan | closed |
| T-05-01 | Spoofing | a stolen token replayed with an unrelated certificate | high | mitigate | `decide.rs:240-256`, `tests/rmq_user.rs:49,59,68` | closed |
| T-05-02 | Elevation of privilege | a token valid for one tenant accepted for another | high | mitigate | `tenants.rs:140-161`, `rmq.rs:107-121`, `tests/rmq_tokens.rs:140,157,194,212` | closed |
| T-05-03 | Elevation of privilege | a device reaching another device's queues or topics | high | mitigate | `decide.rs:296-338`, `crates/domo-common/src/topic.rs:112-130`, `tests/rmq_resource_topic.rs:159` | closed |
| T-05-04 | Elevation of privilege | reaching AXIAM's own virtual host through the device path | high | mitigate | `decide.rs:232-236,277,305,349` | closed |
| T-05-05 | Spoofing | fail-open on a cache miss, a parse failure or an internal error | high | mitigate | `decide.rs:167-173`, `rmq.rs:85-130`, `rmq_user.rs:354,363` | closed |
| T-05-06 | Tampering | a non-2xx response being read by the broker as an error rather than a denial | medium | mitigate | `rmq.rs:65-72,85-87,167-170`, `rmq_user.rs:333,342,414` | closed |
| T-05-07 | Information disclosure | the device token appearing in a log record or a denial reason | high | mitigate | `decide.rs:35-90`, `rmq_user.rs:512` | closed |
| T-05-08 | Denial of service | an unbounded cache or unbounded verification work from hostile connects | medium | mitigate | cheap checks first (`rmq.rs:98-103`); entries carry exp (`decide.rs:133-148`) but `SessionCache` (`tenants.rs:170-199`) never evicts — expired entries are denied, not removed | open — below high threshold (non-blocking) |
| T-05-09 | Tampering | hand-rolled verification drifting from the SDK's guarantees | medium | mitigate | `tenants.rs:20,151-155` (SDK `JwksVerifier`); jsonwebtoken is dev-only | closed |
| T-05-SC | Tampering | cargo installs | low | mitigate | `services/device-twin/Cargo.toml:41-44` (dev-dependencies only) | closed |
| T-06-01 | Spoofing | a device connecting with someone else's token or certificate | high | accept (was mitigate) | **re-dispositioned to accept (AR-03).** 4 of 5 refusals hold (`tools/domo-probe/src/cases.rs:186,201,207,213`); `other-tenant-ca` (`cases.rs:237`) connects (DF-025) | closed |
| T-06-02 | Spoofing | the broker's peer-certificate requirement silently not in force | high | mitigate | `cases.rs:230-235`, `matrix.rs:88` | closed |
| T-06-03 | Elevation of privilege | a device reading or writing another device's or tenant's topics | high | mitigate | `cases.rs:264-282` | closed |
| T-06-04 | Elevation of privilege | a certificate issued under another tenant's signing CA | high | accept (was mitigate) | **re-dispositioned to accept (AR-02).** Attempt runs and fails the matrix (`smoke/certs.rs:166-213`, `matrix.rs:79`); AXIAM issued the leaf (DF-017); `docs/setup.md:536-542` | closed |
| T-06-05 | Elevation of privilege | group membership not actually gating access | high | mitigate | `smoke/assertions.rs:171,310-333,347` | closed |
| T-06-06 | Tampering | a negative case quietly relaxed to make a run green | high | mitigate | `verify.just:292-303,363`, `cases.rs:237-261`, `verify.just:493` | closed |
| T-06-07 | Tampering | smoke fixtures colliding with the Phase 2 seed | medium | mitigate | `smoke.rs:57,102-110`, `smoke/teardown.rs:93,107,119-122` | closed |
| T-06-08 | Information disclosure | a device token printed by the harness | medium | mitigate | `tools/domo-probe/src/main.rs:250`; note `DeviceAuth` (`main.rs:136-139`) derives Debug over the token | closed |
| T-06-09 | Repudiation | a case skipped rather than run, reported as success | medium | mitigate | `matrix.rs:143-186` | closed |
| T-06-SC | Tampering | cargo installs | low | accept | accepted in plan | closed |
| T-07-01 | Denial of service | reset destroying data outside this project | high | mitigate | `stack.just:699-702`, `verify.just:186-188` | closed |
| T-07-02 | Tampering | the organization root removed by reset | high | mitigate | `stack.just:705-708`, `verify.just:195-196,415-472` | closed |
| T-07-03 | Tampering | a stage skipped on a marker with no work behind it | high | mitigate | `stack.just:477-483,489-516,597` | closed |
| T-07-04 | Information disclosure | the super-admin login on a demo card | medium | mitigate | `secrets.rs:67-86`, `verify.just:229-230` | closed |
| T-07-05 | Information disclosure | the one-time bootstrap token in container logs | medium | mitigate | `stack.just:174-185`, `org_bootstrap.rs:73-88` | closed |
| T-07-06 | Denial of service | a full disk producing failures that look like compiler bugs | high | mitigate | `scripts/preflight.sh:46,92-94`, `stack.just:240-259,290-305,805-811` | closed |
| T-07-07 | Spoofing | an image built for the wrong architecture failing only on the second machine | medium | mitigate | `stack.just:320-383` | closed |
| T-07-08 | Repudiation | a hand-rolled workaround with no recorded finding | medium | mitigate | `verify.just:79-129` | closed |
| T-07-09 | Tampering | the two reference machines silently diverging | medium | mitigate | `verify.just:177-178` | closed |
| T-07-SC | Tampering | image and toolchain installs | medium | mitigate | `deploy/docker/Dockerfile.rust:49` (`rust:1.98-bookworm`), `:135,147` (`distroless/cc-debian12:nonroot`) not digest-pinned (sibling pins both); `compose.yml:40,58` untagged `busybox` | open — below high threshold (non-blocking) |
| T-08-01 | Spoofing | verify_live / `tls_handshake_verified` | medium | mitigate | `verify-pki.sh:150-165,375-383`, `pki.just:111` | closed |
| T-08-02 | Tampering | the SC2 verdict of `just verify` (false green) | high | mitigate | `verify-pki.sh:337-465`, `verify.just:46-56` | closed |
| T-08-03 | Tampering | broker posture (`deploy/rabbitmq/`) | high | mitigate | `git diff --quiet 8a6c4ba -- deploy/rabbitmq/` exit 0; `cases.rs:230` | closed |
| T-08-04 | Information Disclosure | probe helpers | medium | mitigate | `verify-pki.sh:139-165`, `pki.just:51-58,95` | closed |
| T-08-05 | Denial of Service | `verify-pki-stress` CPU hogs | low | mitigate | `pki.just:44-47,51-58,187` | closed |
| T-08-06 | Denial of Service | a listener that stalls mid-handshake hangs the gate | low | mitigate | `verify-pki.sh:48,142,159` | closed |
| T-08-07 | Repudiation | a narrowed gate via environment | low | mitigate | `verify-pki.sh:52,472` | closed |
| T-08-SC | Tampering | npm/pip/cargo installs | high | accept | accepted in plan | closed |
| T-01-09 | Spoofing | `tls_handshake_verified` / `verify_live` per-name check | high | mitigate | `verify-pki.sh:150-165`, `pki.just:115-122,145-150` | closed |
| T-01-10 | Tampering | SC2 verdict of `just verify` when compose fails (false green) | high | mitigate | `verify-pki.sh:353-357,378-383,414-415`, `pki.just:161-177` | closed |
| T-01-11 | Repudiation | a narrowed or guessed gate | medium | mitigate | `verify-pki.sh:52` (never defaults COMPOSE_PROJECT_NAME or reads `.env`) | closed |
| T-01-12 | Information Disclosure | compose stderr now surfaced; probe helpers | low | mitigate | `verify-pki.sh:139-165`, `pki.just:171` | closed |
| T-01-13 | Tampering | broker posture and the deliberately red smoke gate | high | mitigate | `git diff --quiet 9e873b0 -- deploy/ just/smoke.just just/verify.just tools/ services/ crates/ Cargo.toml Cargo.lock` exit 0 | closed |
| T-01-14 | Denial of Service | new controls and the stress recipe | low | accept | accepted in plan | closed |
| T-01-SC | Tampering | npm/pip/cargo installs | high | mitigate | `Cargo.toml:27-33` (`=1.0.0-beta16`, provenance from 2026-09-20 human gate) | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Open, non-blocking (tracked)

| Threat | Severity | Gap | Suggested fix |
|--------|----------|-----|---------------|
| T-05-08 | medium | `SessionCache` never evicts; expired entries are denied but kept. Growth is bounded in practice (entries are written only after a token verifies with `sub == username`). | Evict on expiry or on read, or correct the plan wording. |
| T-07-SC | medium | `Dockerfile.rust` base images (`rust:1.98-bookworm`, `distroless/cc-debian12:nonroot`) are tag-pinned, not digest-pinned as in the sibling; `busybox` in `compose.yml:40,58` has no tag. | Digest-pin both bases; tag and digest-pin `busybox`. |
| T-04-08 | low | Seven public functions in `crates/domo-common/src/hand_rolled.rs` carry no `DF-` citation. | Add the citing doc comments (or tighten the findings audit to per-function). |

### Observations outside the register (not scored)

- `domo-probe` (`compose.yml:580`) and `domo-bootstrap` (`:551`) mount the whole `.secrets` tree read-write; `tls-init` mounts `.secrets/pki` read-only including `root.key` (`:119`). The key still leaves only through BYOK, so T-01-03/T-04-03 hold, but narrowing the probe's mount would make the claim literal.
- The RabbitMQ management UI (plain HTTP) is published on `127.0.0.1:15673` (`compose.yml:318`). Loopback only, but absent from T-03-07's "only 443, 8090, 8883" list.
- The `docker save` key scan in `guard-secrets.sh` assumes uncompressed image layers (true on this host in 01-01); on a compressed-layer store it would silently read 0.
- `DeviceAuth` in `tools/domo-probe/src/main.rs:136-139` derives `Debug` over a plain-`String` token; nothing formats it today.

---

## Accepted Risks Log

All three blocking threats share one root cause: AXIAM issues a leaf under another tenant's signing CA (DF-017), and that leaf then authenticates and round-trips end to end (DF-025). This is an AXIAM defect, not one in this repository. The acceptance does **not** relax any gate: `just smoke` stays red on `cross-tenant-ca-issuance` and `other-tenant-ca`, and no case is pinned, marked expected-failure or excluded.

**Expiry:** each acceptance lapses when the upstream fix (AXIAM T22.1, `prepare_leaf_issuance` checks `ca_certificate.tenant_id`) is verified here using the re-run list in `docs/dogfooding-upstream-status.md`. At that point, re-run `/gsd-secure-phase 01`; the threats return to `mitigate` and must close on evidence.

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-04-02 (high) | The caller-side control holds (every signing call passes the acting tenant's own CA), so the demo never issues across tenants by accident. Deliberate cross-tenant issuance by a tenant admin can only be refused by AXIAM (DF-017). LAN-only demo; residual already named in T-01-02. | Emanuele Panigati (user) | 2026-09-29 |
| AR-02 | T-06-04 (high) | The attempt runs and correctly fails the matrix, but AXIAM issued the certificate and the named compensating control (`other-tenant-ca` refused) does not hold (DF-025). Already recorded as accepted-and-demonstrated in the 01-06 SUMMARY and `docs/setup.md:536-542`. | Emanuele Panigati (user) | 2026-09-29 |
| AR-03 | T-06-01 (high) | Four of the five promised refusals hold (mismatched cert, wrong-tenant token, bad signature, malformed token). The fifth, a certificate from the other tenant's CA, connects because of DF-017 → DF-025. | Emanuele Panigati (user) | 2026-09-29 |
| — | T-01-02, T-02-06, T-02-SC, T-04-SC, T-06-SC, T-08-SC, T-01-14 | Accepted at plan time; rationale in each plan's `<threat_model>`. | Plan authors (user-approved plans) | 2026-09-20 … 2026-09-29 |

*Accepted risks do not resurface in future audit runs — except AR-01…AR-03, which expire as stated above.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-29 | 81 | 75 | 6 (3 high: T-04-02, T-06-01, T-06-04; 3 non-blocking) | gsd-security-auditor (static inspection, ASVS L1) |
| 2026-09-29 | 81 | 78 | 3 (non-blocking only; AR-01…AR-03 accepted by user) | /gsd-secure-phase orchestrator |

Audit limits: static inspection only — no builds, containers or `just` gates were run; the only commands beyond file reads were the two `git diff --quiet` checks cited for T-08-03 and T-01-13. `.secrets/` is hook-protected from reads, so live file modes (T-04-04, T-07-04) were judged from code and recorded outputs.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-29 (with AR-01…AR-03 pending the upstream DF-017 fix)
