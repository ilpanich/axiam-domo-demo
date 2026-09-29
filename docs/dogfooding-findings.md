# AXIAM dogfooding findings

This demo exists partly to exercise AXIAM and its SDKs for real and write down
what it finds. Every gap, every place the demo had to hand-roll a call the SDK
does not cover, and every documented exception to the project's own rules lands
here.

## Which AXIAM this validates

| | |
|---|---|
| **Server image** | `ghcr.io/ilpanich/axiam/server:1.0.0-beta16` |
| **Image digest** | `sha256:2e6be78c13840cd98330ccc1e30e96cc539b227ee0d072094fb24c6b4bf7f97d` |
| **Rust SDK** | `axiam-sdk` `=1.0.0-beta16` (crates.io, pinned exactly, `features = ["rest"]`) |
| **Broker** | `rabbitmq:4.3-management-alpine` → RabbitMQ 4.3.6 |

Both versions were resolved by plan 01-01: the image tag is what was actually
pulled, and the SDK version is what the package-legitimacy gate cleared
(published 2026-09-19, not yanked, sole owner `ilpanich`, metadata identical to
the local checkout). Every finding below is a claim about **these builds** and
nothing else — each entry's `Build` field says so explicitly. When the pinned
tag moves, re-check every `confirmed` entry before assuming it still holds.

## Entry format (D-32)

Each entry carries the same eight fields: the `DF-NNN` id, the AXIAM build and
SDK version, the component, a severity, expected vs actual, a reproduction, the
workaround the demo uses, and a status.

**Status vocabulary** — the honesty dial, and the thing to read first.
`confirmed` = observed at runtime against the pinned build.
`reported-from-source-reading` = read in AXIAM's or the SDK's source, not yet
exercised here. `resolved` = was open, the runtime answer is now known and
recorded in place. `design-exception` = not an AXIAM gap, but a documented
exception to one of the demo's own rules. `proposed-improvement` = a capability
that does not exist; not a defect. `fixed-upstream` = AXIAM has shipped a fix in
a **later** release than the one pinned above; the finding remains true of the
pinned build and the fix is **not** verified here.

**Upstream status lives in `docs/dogfooding-upstream-status.md`**, not in this
file. A finding is a fact about the build it was observed on and does not change;
what AXIAM has since done about it changes every release. Eight entries below are
`fixed-upstream` as of AXIAM 1.0.0-beta17 — that file says which, as what, and
what has to be re-run before any of them can be called `resolved`.

## How this file is audited

**Every hand-rolled AXIAM call must have a matching entry here.** A workaround
with no finding is indistinguishable from ordinary code six months later, which
is exactly how a dogfooding deliverable quietly rots.

The rule is mechanical, so it is checked mechanically. `just verify`'s
`findings-audit` group asserts four things, and a failure names which:

1. **Every `DF-` identifier cited under `crates/`, `tools/` or `services/`
   resolves** to a `## DF-NNN — ` heading here. A citation with no entry fails.
2. **Every source file that names an AXIAM REST path** (`/api/v1/…`) **cites at
   least one `DF-`.** This is the half that catches the real risk: a *new*
   hand-rolled call added by a later phase with no finding written for it. The
   file-level granularity is deliberate — a hand-rolled call is always
   accompanied by a module- or function-level doc comment explaining why it is
   hand-rolled, and that comment is where the identifier belongs.
3. **Every entry has its upstream issue file** under `docs/dogfooding-issues/`,
   and every issue file has an entry here. The two sets must be equal.
4. **This file stays under the project's 500-line cap.** Splitting the issue
   bodies out (below) is what keeps it there; the gate is what stops the next
   twenty entries from quietly breaking the rule again.

## Filing upstream

Every entry carries a ready-to-paste issue title and body (D-33), in its own
file under [`docs/dogfooding-issues/`](dogfooding-issues/) — one per `DF-`
identifier, linked from the entry. They live outside this file so the ledger
stays readable at a glance and inside the project's 500-line cap; the issue
bodies are long-form prose for a different audience (the AXIAM maintainers) and
are read one at a time, not scanned.

**Nothing is filed automatically** — opening an issue against the AXIAM server
or SDK repository is the user's call, not this repository's.

## Appending

Ids are sequential and never reused. The highest allocated id is **DF-027**; the
next entry is DF-028. Plan 01-06 recorded the cross-tenant certificate-issuance
outcome — in DF-017, whose question it answers, rather than in a new entry — and
appended DF-025 through DF-027. Every later phase appends its own.

A new entry is two files, not one: the `## DF-NNN — <title>` section here with
its eight fields and a closing `**Upstream issue** — ready to paste:` link, and
`docs/dogfooding-issues/DF-NNN.md` holding the issue title and body. The audit
gate asserts both exist and that the two id sets are equal, so an entry that
invents its own layout, or forgets its issue file, stops the gate rather than
going unnoticed.

---

## DF-001 — No SAN, keyUsage or extendedKeyUsage on AXIAM-issued leaves

**Component** axiam-server (`axiam-pki`) · **Severity** high · **Status** fixed-upstream (beta17 T22.14) · **Build** as pinned above

**Expected vs actual** — A certificate issued from a tenant signing CA should be able to terminate TLS: the CSR's requested `subjectAltName`, `keyUsage` and `extendedKeyUsage` honoured, or passable explicitly. **Actual:** issued leaves carry none of the three, and every current browser rejects a server certificate with no `subjectAltName` — so AXIAM-issued certificates cannot front a listener at all.

**Reproduction** — Sign any CSR through AXIAM, then `openssl x509 -noout -ext subjectAltName -in <leaf>`; the extension is absent regardless of what the CSR asked for.

**Workaround** — Every **server** certificate in the demo is signed offline by the same organization root (`scripts/gen-pki.sh`, driven by `deploy/pki/listeners.conf`). That keeps the single-trust-anchor rule intact — one root, no second CA — at the cost of AXIAM not issuing the certificates it otherwise could. Client certificates, which need no SAN, are AXIAM-issued as intended.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-001.md`](dogfooding-issues/DF-001.md)

## DF-002 — Device certificate binding: the docs and the code disagree

**Component** axiam-server + docs · **Severity** medium · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — The documented device-provisioning flow should match what the handlers require, so an integrator can follow the docs and succeed. **Actual:** the documentation describes certificate binding differently from the implemented `bind_certificate` path, leaving the correct call order and required fields ambiguous.

**Reproduction** — Follow the device-provisioning section of the PKI docs against `1.0.0-beta16` and compare with `certificates::sign_csr` + `service_accounts::bind_certificate` as implemented.

**Workaround** — `domo-bootstrap` follows the **code**: create the service account first (the CSR subject must be `CN=<service-account UUID>`, which cannot exist before the account does), then sign, then bind. Plan 01-01 split its `device-identity` stage in two for exactly this reason.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-002.md`](dogfooding-issues/DF-002.md)

## DF-003 — `has_role` uniqueness

**Component** axiam-server (`axiam-db`) · **Severity** low · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — Assigning the same role to the same group at the same resource twice should be idempotent, or rejected with a clear conflict. **Actual:** the `has_role` edge has no uniqueness constraint, so a repeated assignment silently produces a duplicate edge, and idempotent provisioning cannot rely on "create and ignore the conflict".

**Reproduction** — Call `roles.assign_to_group(role, {group, resource})` twice with identical arguments and list the group's roles.

**Workaround** — `domo-bootstrap` resolves by natural key before every create and never relies on a uniqueness error to signal "already there" (Pattern S-4: idempotent by probe, not by marker).

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-003.md`](dogfooding-issues/DF-003.md)

## DF-004 — No gRPC management API

**Component** axiam-server · **Severity** medium · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — A service that already holds a gRPC channel for `CheckAccess` should be able to manage resources, groups and roles over the same channel. **Actual:** gRPC exposes authorization checks and token operations only; all management is REST, so a service needs two transports, two auth shapes and two client configurations.

**Reproduction** — Inspect the `axiam.v1` service definition: no resource, group, role or service-account management RPCs.

**Workaround** — The demo uses REST for provisioning and gRPC only for access checks, exactly as the project constraints already state. Costed as two client stacks per service rather than one.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-004.md`](dogfooding-issues/DF-004.md)

## DF-005 — gRPC listener performs no client-certificate verification

**Component** axiam-server · **Severity** high · **Status** fixed-upstream (beta17 T22.12) · **Build** as pinned above

**Expected vs actual** — The gRPC listener should be able to require and verify a client certificate, so a service can be authenticated by mTLS the way a device is on REST. **Actual:** only `AXIAM__SERVER__TLS__*` is documented for the gRPC port and there is no client-certificate verification setting, so callers are authenticated by bearer token alone.

**Reproduction** — Search the server configuration surface for a gRPC client-CA or peer-verification option.

**Workaround** — The demo publishes **no** gRPC port in Phase 1 (50051 stays internal to the compose network) and defers the question to Phase 3, where the first `CheckAccess` call is made. Token-only authentication on an internal network is acceptable; exposing it on the LAN would not be.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-005.md`](dogfooding-issues/DF-005.md)

## DF-006 — No refresh token for device principals

**Component** axiam-server · **Severity** medium · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — A device should be able to renew its access token without re-running a full mTLS login, as user principals do via `refresh`. **Actual:** `POST /api/v1/auth/device` returns an access token only, so devices repeat the mTLS login on every expiry.

**Reproduction** — Inspect the device-login response: no `refresh_token`.

**Workaround** — Simulated devices re-run the mTLS login on expiry. Cheap here (the certificate and key are already on disk) but it means every device holds a long-lived credential and performs a full handshake on a fixed interval.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-006.md`](dogfooding-issues/DF-006.md)

## DF-007 — OAuth2 scopes are not shaped for RabbitMQ's permission grammar

**Component** axiam-server · **Severity** low · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — `rabbitmq_auth_backend_oauth2` should be able to consume an AXIAM token directly. **Actual:** that backend requires the `scope` claim pre-shaped as RabbitMQ permission grammar (`read:vhost/topic`, `id.configure:*/*`), while AXIAM's `scope` is an application-defined OAuth2 scope string — and it is **not populated at all** on the mTLS device-login path the architecture uses.

**Reproduction** — Read `AccessTokenClaims.scope` and compare with the OAuth2 backend's expected format; then inspect any token minted by `/api/v1/auth/device`.

**Workaround** — RabbitMQ authenticates devices by TLS client certificate (`verify_peer` + `fail_if_no_peer_cert`, with `mqtt.ssl_cert_client_id_from = distinguished_name`) and authorizes them through the Device Twin's own `rabbitmq_auth_backend_http` endpoints, which validate the JWT with the SDK's `JwksVerifier`. Proven end to end by `just tracer`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-007.md`](dogfooding-issues/DF-007.md)

## DF-008 — Rust SDK sends no acting-tenant header

**Component** axiam-sdk (Rust) · **Severity** high · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — An organization-level principal should be able to act inside a tenant through the SDK by naming the tenant. **Actual:** the SDK never sends `X-Axiam-Tenant` (the server's `ACTIVE_TENANT_HEADER`) and tenant-scoped routes use the principal's own tenant, so an org-level client creates resources, roles and service accounts in the reserved `organization` tenant **with no error** — they simply appear in the wrong place.

**Reproduction** — `grep -r 'X-Axiam-Tenant' axiam-rust-sdk/src` → no hits. Then create a resource with an org-level SDK client and observe which tenant owns it.

**Workaround** — `domo-bootstrap` does org-scoped work (tenants, BYOK import, signing CAs) with the SDK as super-admin, then hand-rolls the four calls per tenant that create a tenant-admin user, carrying the header explicitly, and does all tenant-scoped work through a second SDK client logged in as that admin. Cited at `crates/domo-common/src/hand_rolled.rs` and `tools/domo-bootstrap/src/stages/tenants.rs`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-008.md`](dogfooding-issues/DF-008.md)

## DF-009 — Rust SDK has no device mTLS login operation

**Component** axiam-sdk (Rust) · **Severity** high · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — Parity with the C++ SDK's `authenticate_device()`; the Rust SDK should be able to perform `POST /api/v1/auth/device`. **Actual:** `rest::auth` exposes `login`, `verify_mfa`, `refresh` and `logout` only. The client can be built with a client certificate (`with_client_cert`) but has no device-login call to make with it; `examples/device_login.rs` is the OAuth Device Authorization Grant, a different thing entirely.

**Reproduction** — `grep -r 'auth/device' axiam-rust-sdk/src` → no hits.

**Workaround** — `domo-probe` builds its mTLS client with the SDK and hand-rolls this one call. Cited at `tools/domo-probe/src/main.rs`. Confirmed working against the pinned build by `just tracer`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-009.md`](dogfooding-issues/DF-009.md)

## DF-010 — `/admin/bootstrap` is excluded from the SDKs

**Component** axiam-sdk (all) · **Severity** low · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — First-run bootstrap should be reachable from the SDK, or its exclusion stated where an integrator will look. **Actual:** the contract excludes it by design, so every automated first run hand-rolls the bootstrap call.

**Reproduction** — Look for a bootstrap operation in the SDK's management surface; there is none.

**Workaround** — Hand-rolled with the setup-token gate. Cited at `crates/domo-common/src/hand_rolled.rs`. Logged for completeness rather than as a defect — but the exclusion is the first thing an automated deployment hits, so it deserves to be visible.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-010.md`](dogfooding-issues/DF-010.md)

## DF-011 — Manifest cannot express resource metadata, resource-scoped role bindings or service accounts

**Component** axiam-sdk (Rust) · **Severity** medium · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — The management manifest should cover what the contract lists for it, so declarative provisioning is complete. **Actual:** the manifest spec has no resource `metadata`, no resource-scoped group→role bindings and no `service_accounts` — the three things a hierarchical authorization model needs most cannot be declared.

**Reproduction** — Read the manifest spec types and compare with the contract's description.

**Workaround** — Only the role/permission catalog goes through `manifest().apply()`. Resources, resource-scoped group→role bindings and service accounts are created imperatively with natural-key idempotency.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-011.md`](dogfooding-issues/DF-011.md)

## DF-012 — gRPC client exposes no token-validation wrapper

**Component** axiam-sdk (Rust) · **Severity** low · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — The gRPC client should wrap `validate_token` / `introspect_token` the way it wraps `check_access`. **Actual:** only `check_access` and `batch_check` are wrapped; `validate_token` exists solely in the raw generated stub.

**Reproduction** — Compare the public methods on the SDK's gRPC client with the generated service stub.

**Workaround** — None needed: the Device Twin uses the SDK-native `token::JwksVerifier` (local EdDSA verification against `/oauth2/jwks`, with `expect_tenant_id` and `expect_audience`), which is the better choice anyway — it pins `alg` before key lookup and avoids a network round trip per message. Recorded so the missing wrapper is not mistaken for a missing capability.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-012.md`](dogfooding-issues/DF-012.md)

## DF-013 — REST management routes reject the machine-to-machine audience

**Component** axiam-server · **Severity** high · **Status** fixed-upstream (beta17 T22.13) · **Build** as pinned above

**Expected vs actual** — A service account should be able to manage the objects it is responsible for. **Actual:** every REST management handler takes `AuthenticatedUser`, and the audience check rejects `axiam:m2m` with "audience mismatch — this route requires axiam:user audience". Service accounts can call `POST /authz/check` and nothing else, so a service that manages AXIAM must hold **user** credentials.

**Reproduction** — Authenticate a service account (mTLS or client-credentials) and call `/api/v1/resources`; observe 401 with that message.

**Workaround** — Phase 1 still creates `mgmt@`/`twin@` service accounts and certificates — they are valid for access checks and device-style authentication. The Management Platform (Phase 2) will use the per-tenant **admin user** that bootstrap creates anyway (see DF-008).

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-013.md`](dogfooding-issues/DF-013.md)

## DF-014 — Device mTLS tokens carry no certificate-thumbprint confirmation claim

**Component** axiam-server · **Severity** medium · **Status** fixed-upstream (beta17 T22.3) · **Build** as pinned above

**Expected vs actual** — A token issued in exchange for a client certificate should carry `cnf` / `x5t#S256` (RFC 8705), so a downstream consumer can bind the token to the certificate that obtained it. **Actual:** the device access-token spec sets no confirmation claim, so a stolen token is usable from any TLS session and nothing downstream can prove the presenter is the certificate holder.

**Reproduction** — Decode a token from `/api/v1/auth/device`: no `cnf` claim.

**Workaround** — The binding is reconstructed at the broker instead. `mqtt.ssl_cert_client_id_from = distinguished_name` makes RabbitMQ reject any CONNECT whose `client_id` differs from the certificate subject DN, and the Twin's auth backend additionally requires `sub == username` and `client_id == "CN=" + username`. Plan 01-01 confirmed at runtime that the broker renders a CN-only subject as exactly `CN=<value>`, which is what makes the reconstruction exact rather than approximate. **Residual risk:** a forged-CN certificate issued by another tenant's admin, combined with a stolen JWT — see DF-017.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-014.md`](dogfooding-issues/DF-014.md)

## DF-015 — PKI docs say RSA CA generation fails; the code generates RSA-4096

**Component** axiam docs · **Severity** low · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — The PKI documentation should reflect what the code does. **Actual:** `pki/README.md` states that RSA CA generation is unsupported, while `generate_keypair` has handled `KeyAlgorithm::Rsa4096` since 2026-08-25 — present in every `v1.0.0-beta*` tag — and `CreateIntermediateCaRequest` takes a caller-chosen `key_algorithm`.

**Reproduction** — Compare the README's statement with `axiam-pki`'s `generate_keypair`.

**Workaround** — None needed, but the stale doc changed a real decision: tenant signing CAs are Ed25519 **by choice** (instant keygen on the Pi, consistent with Ed25519 leaves), not because RSA was unavailable. Plan 01-01 confirmed at runtime that Erlang/OTP TLS 1.3 accepts an Ed25519 leaf under an Ed25519 tenant CA under the RSA-4096 root, so the RSA-4096 fallback was never needed.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-015.md`](dogfooding-issues/DF-015.md)

## DF-016 — The healthcheck subcommand cannot probe a TLS listener behind a private CA

**Component** axiam-server · **Severity** medium · **Status** reported-from-source-reading · **Build** as pinned above

**Expected vs actual** — The shipped `healthcheck` subcommand should work against a server configured for TLS. **Actual:** it probes `AXIAM_HEALTHCHECK_URL` (default `http://127.0.0.1:8090/health`) with a client built on webpki roots — plain HTTP fails against a TLS listener, HTTPS is not trusted because the CA is private, and there is no way to point it at a custom trust anchor.

**Reproduction** — Enable `AXIAM__SERVER__TLS__*` and run the container's own healthcheck.

**Workaround** — No compose healthcheck for `axiam-server`. `just` polls `curl --cacert .secrets/pki/root.pem https://127.0.0.1:8090/health` instead, and dependents gate on `service_started` plus the stage marker. Documented in `just/stack.just`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-016.md`](dogfooding-issues/DF-016.md)

## DF-017 — Leaf issuance does not bind a tenant signing CA to its tenant

**Component** axiam-server (`axiam-pki`) · **Severity** high · **Status** confirmed · fixed-upstream (beta17 T22.1) · **Build** as pinned above

**Expected vs actual** — A tenant administrator should be able to issue leaves only from their **own** tenant's signing CA. **Actual:** `prepare_leaf_issuance` checks that the issuing CA belongs to the organization, is active and in-window; it does **not** check that a *tenant* signing CA belongs to the acting tenant. Confirmed at runtime in plan 01-06: the Lakeside tenant admin signed a certificate request under **Summit's** signing CA and AXIAM returned a certificate.

The stamping is the part that makes this more than bookkeeping. The issued leaf carries `tenant_id` = **Lakeside's** (the acting tenant's) while `issuer_ca_id` = **Summit's** signing CA. AXIAM therefore records the certificate as belonging to one tenant and signs it with another tenant's key, and nothing downstream can tell the two apart from the certificate alone.

**Reproduction** — As a tenant admin, sign a CSR naming another tenant's signing CA as the issuer:

```
just smoke-certs        # the attempt runs as its last step
just smoke-matrix       # reports it as the `cross-tenant-ca-issuance` case
```

Observed on the pinned build: certificate issued, `tenant_id` = acting tenant, `issuer_ca_id` = the other tenant's CA. The matrix fails on this case by design, so the result cannot be absorbed into a green run.

**Workaround** — None available to a consumer; this is enforcement that only AXIAM can perform. `domo-bootstrap` always passes the acting tenant's own CA, so the demo never exercises the gap by accident, and `just smoke-teardown` revokes the certificate the experiment mints. That is containment, not mitigation: any tenant administrator can still do this deliberately. **See DF-025 for what the confirmed gap makes reachable end to end** — it is no longer the defence-in-depth concern this entry originally described.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-017.md`](dogfooding-issues/DF-017.md)

## DF-018 — PKI encryption key environment-variable name mismatch — RESOLVED

**Component** axiam-server + docs · **Severity** high · **Status** resolved · **Build** as pinned above

**Expected vs actual** — The environment variable the documentation and the startup error message name should be the one the server reads. **Actual:** startup refuses CA import and generation with a message naming `AXIAM__PKI__ENCRYPTION_KEY`, while the env secret provider resolves the logical key `pki_encryption_key` to **`AXIAM__AUTH__PKI_ENCRYPTION_KEY`**. Setting the documented name alone leaves CA operations refused, with an error pointing at the variable you just set.

**Reproduction** — Plan 01-01 resolved this by setting the two spellings to **different** values. Signing a CSR — which must decrypt the tenant CA's private key — still succeeded while `AXIAM__PKI__ENCRYPTION_KEY` held a wrong value, proving the unprefixed spelling is ignored entirely on `1.0.0-beta16`.

**Workaround** — `just secrets` mints both spellings with the same value, so a future image that switches cannot fail with a decryption error. Recorded as resolved rather than deleted: the mismatch still exists upstream, and the next reader needs to know which name won.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-018.md`](dogfooding-issues/DF-018.md)

## DF-019 — The setup token is logged once per database

**Component** axiam-server · **Severity** high · **Status** confirmed · fixed-upstream (beta17 T22.7) · **Build** as pinned above

**Expected vs actual** — The first-run setup token should be recoverable if it is lost before bootstrap completes. **Actual:** it is minted and logged exactly once per database and later boots are silent no-ops, so if the container is **recreated** — which discards its logs — before org-bootstrap succeeds, the only recovery is wiping the datastore volume.

**Reproduction** — Confirmed at runtime in plan 01-01: adding a port mapping recreated `axiam-server`, which discarded the first-boot log carrying the token. Recovery required deleting the SurrealDB volume by explicit name.

**Workaround** — `just _setup-token` scrapes the **full** log history (never `--since`) and persists the token to `.secrets/state/setup-token` at mode 0600 the moment it is seen. The `org-bootstrap` stage handles 201/403/409 and falls through to a login probe with the saved super-admin credentials, so an already-bootstrapped system is detected rather than misread as a token failure. Cited at `tools/domo-bootstrap/src/stages/org_bootstrap.rs`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-019.md`](dogfooding-issues/DF-019.md)

## DF-020 — AXIAM's own AMQP client certificate is offline root-signed (design exception)

**Component** demo infrastructure — **not an AXIAM gap** · **Severity** informational · **Status** design-exception · **Build** as pinned above

**Expected vs actual** — Per PKI-03, every device and service client certificate in this demo is issued by AXIAM from a tenant signing CA. **Actual:** exactly one client certificate is not — AXIAM's own AMQP client certificate. This is a **documented exception, not a defect**, and it is forced by ordering: the MQTT listener inherits the broker-wide `ssl_options`, so `fail_if_no_peer_cert` applies to AXIAM's own AMQPS 5671 link too, meaning AXIAM must present a client certificate before AXIAM exists to issue one.

**Reproduction** — Not a bug to reproduce. The ordering constraint follows from RabbitMQ applying `ssl_options` broker-wide rather than per listener.

**Workaround** — `axiam-amqp-client` is issued offline by the organization root, alongside the server certificates (`deploy/pki/listeners.conf`, `kind=client`). It is the single documented exception, it is asserted as such by `just verify-pki`, and the single-trust-anchor rule is unbroken: the certificate still chains to the same root as everything else. Plan 01-01 confirmed AXIAM's AMQP connection stays up under broker-wide `fail_if_no_peer_cert`.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-020.md`](dogfooding-issues/DF-020.md)

## DF-021 — An allow/deny grant cannot be marked non-inheritable

**Component** axiam-server (authorization model) · **Severity** low · **Status** fixed-upstream (beta17 T22.11) · **Build** as pinned above

**Raised by the user during Phase 1 execution (2026-09-20); not discovered by the tracer.** It is a proposed capability, not an observed defect, and it is recorded here so the log keeps the two apart.

**Expected vs actual** — A grant should be attachable to a node and declarable **not** to descend to that node's subtree, so "operate at this tier and no lower" is expressible directly. **Actual:** grants always inherit down the subtree; there is no way to say "here and no further".

**Reproduction** — Not a defect. The absence is visible in the grant model: no inheritance flag exists.

**Workaround** — `authz/catalog.toml` expresses the model without it, either by layering deny rules at the apartment tier or by never granting at an ancestor and enumerating per resource. **Why this demo cares:** the authorization model is the thing the demo exists to show, and its sharpest rule is a negative one — *no staff role may operate apartment devices*. Property managers, concierges and installers all hold grants at ancestor nodes of the apartments, so under inherit-by-default every one of those grants reaches apartment devices unless something stops it. The headline constraint ends up encoded as a workaround rather than as a statement of intent.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-021.md`](dogfooding-issues/DF-021.md)

## DF-022 — Two documented secrets are accepted and then silently ignored

**Component** axiam-server · **Severity** medium · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — A secret named exactly as the documentation names it should be read, or its absence reported accurately. **Actual:** the env secret provider resolves **every** logical key under the `AXIAM__AUTH__` prefix, so `AXIAM__EMAIL_ENCRYPTION_KEY` and `AXIAM__GDPR_PSEUDONYM_PEPPER` are accepted into the container, ignored, and then warned about as *missing* while being demonstrably present in the environment. This generalises DF-018 beyond the PKI key: it is a prefix rule, not a one-off typo.

**Reproduction** — Confirmed at runtime in plan 01-01. Set both variables in the container environment and read the startup log: AXIAM reports them missing. Adding the `AXIAM__AUTH__` prefix silences the warnings.

**Workaround** — `just secrets` mints **both** spellings of every affected key with the same value. Neither of these two is on the tracer's critical path, but a secret that is present, ignored and reported as missing is a trap for whoever sets one that *is*.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-022.md`](dogfooding-issues/DF-022.md)

## DF-023 — `generate_signing_ca` takes a bare common name, not a DN

**Component** axiam-server · **Severity** low · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — The `subject` field should either take a distinguished name or reject one with a clear error. **Actual:** it takes a **bare common name** and builds the DN itself, so passing the conventional `CN=<name>` form produces a subject of `CN=CN=<name>` — with no error, no warning, and a certificate that looks right until someone reads its subject.

**Reproduction** — Confirmed at runtime in plan 01-01. Call `generate_signing_ca` with `subject: "CN=Lakeside Residences Signing CA"` and read the resulting certificate's subject.

**Workaround** — `domo-bootstrap` passes the bare common name. The doubled subject was caught by reading the issued certificate, not by any error from the call.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-023.md`](dogfooding-issues/DF-023.md)

## DF-024 — Default login rate limit is below what a staged bootstrap needs

**Component** axiam-server · **Severity** medium · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — Scripted provisioning should be able to complete, and repeat, without tripping a rate limit intended for interactive logins. **Actual:** the default login limit is 10/min, while a staged, multi-process bootstrap performs roughly eight logins per run (each stage is a separate one-shot process with its own client), so a second consecutive run fails part-way — and it fails in a manner that reads as an authentication bug, not as a rate limit.

**Reproduction** — Confirmed at runtime in plan 01-01: two consecutive `just tracer` runs against the default limit; the second fails mid-way.

**Workaround** — `AXIAM__RATE_LIMIT__LOGIN_PER_MIN=120`, set in `deploy/compose.yml` with a comment stating it is a **deliberate LAN-only-demo relaxation**. It must not be copied into anything internet-reachable, and plan 01-07's documentation repeats that where an operator will see it.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-024.md`](dogfooding-issues/DF-024.md)

## DF-025 — A forged-common-name certificate from another tenant's CA authenticates a device end to end

**Component** axiam-server (`axiam-pki`) + the demo's broker chain · **Severity** high · **Status** confirmed · cause fixed-upstream (beta17 T22.1, via DF-017) · **Build** as pinned above

**Expected vs actual** — A certificate signed by tenant B's authority should not be able to speak for a device of tenant A, at any layer. **Actual:** it can, and the connection is fully functional — CONNECT accepted, subscribe acknowledged, publish acknowledged, message delivered.

This is DF-017's consequence rather than a separate defect, and it is worth its own entry because the two were previously believed to be separated by compensating controls. They are not. The chain, hop by hop:

1. DF-017 lets tenant B's administrator mint a leaf whose subject is `CN=<tenant A device's service-account UUID>` — tenant A's device — signed by **tenant B's** signing CA.
2. The broker's TLS trust bundle is the **organization root** (D-26, forced: the MQTT listener inherits broker-wide `ssl_options` shared with AXIAM's own AMQPS listener, so a per-tenant bundle is not expressible). Both tenant CAs chain to that root, so the forged leaf is trusted.
3. `mqtt.ssl_cert_client_id_from = distinguished_name` compares `client_id` against the certificate subject. The forged subject **is** the impersonated device's, so this check has nothing to object to.
4. The Device Twin checks `client_id == "CN=" + username` and `token.sub == username`. With a token for the impersonated device, both hold.

The net effect: **at the broker, the client certificate contributes nothing to tenant separation.** Separation rests entirely on the bearer token. That is a narrower guarantee than the architecture's "every device certificate is issued by its own tenant's CA" was taken to provide, and it is the residual risk D-24 named — now demonstrated rather than hypothesised.

**Reproduction** — `just smoke-matrix`, case `other-tenant-ca`. It presents a forged-common-name leaf (minted by the `cross-tenant-ca-issuance` experiment) with the impersonated device's own credentials, and observes a successful publish/subscribe round trip. The case fails by design, so the result cannot be absorbed into a green run.

**Workaround** — None at the broker. Three things bound it in this demo, none of which is a fix:

- The attacker must already be a tenant administrator of some tenant in the same organization — a privileged insider, not an outsider.
- A valid token for the impersonated device is still required; the forged certificate alone opens nothing.
- `just smoke-teardown` revokes the forged certificate the experiment mints.

Closing DF-017 closes this. Failing that, tenant separation at the broker would have to move to something the broker can see per-tenant, which the MQTT plugin's inherited `ssl_options` currently prevents.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-025.md`](dogfooding-issues/DF-025.md)

## DF-026 — The console image cannot start when its upstream is merely absent

**Component** axiam-frontend (published console image) · **Severity** medium · **Status** confirmed · **Build** as pinned above

**Expected vs actual** — A console container whose backend is not up yet should start and serve an error, or wait. **Actual:** nginx resolves its `proxy_pass` upstreams at configuration load time, so with no `axiam-server` container present it exits immediately with `[emerg] host not found in upstream` and enters a restart loop.

This is a startup-ordering fragility in the published image rather than an operator misconfiguration: a consumer composing the console alone, or with the server starting later, gets a crash-looping container instead of a console. It is also the kind of failure that reads as a networking problem for some time before the log is examined.

**Reproduction** — Observed in plan 01-03 while composing the console without the server. Start the `axiam-frontend` image with no resolvable `axiam-server` host on its network.

**Workaround** — `depends_on: axiam-server` in the compose file, which makes the ordering explicit. It does not help a restart in which the server is slower to come back, so it is an ordering fix rather than a robustness one.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-026.md`](dogfooding-issues/DF-026.md)

## DF-027 — An unbound device certificate is refused with 403, not 401

**Component** axiam-server (`POST /api/v1/auth/device`) · **Severity** low · **Status** confirmed · fixed-upstream (beta17 T22.4) · **Build** as pinned above

**Expected vs actual** — A certificate that is valid TLS material but bound to no service account "authenticates as nobody", which reads as an authentication failure: 401. **Actual:** AXIAM answers **403**.

Recorded because the distinction matters to a client deciding what to do next. A 401 says *these credentials did not identify you* — re-authenticate. A 403 says *you are identified and not permitted* — do not retry. Here the cause is the former (no binding, therefore no subject) while the status says the latter, so a client implementing the conventional reaction to each will do the wrong thing.

**Reproduction** — Issue a certificate for a service account and do **not** call `bind_certificate`, then `POST /api/v1/auth/device` with it over mTLS. `just smoke-matrix`, case `empty-cert-binding`, asserts this; the demo's fixture `smoke-unbound-device` exists precisely to hold that state.

**Workaround** — None needed; the demo asserts 401 or 403 and fails on a 200, so the security property is pinned without depending on which of the two AXIAM returns.

**Upstream issue** — ready to paste: [`docs/dogfooding-issues/DF-027.md`](dogfooding-issues/DF-027.md)
