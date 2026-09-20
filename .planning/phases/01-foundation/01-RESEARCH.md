# Phase 1: Foundation - Research

**Researched:** 2026-09-19
**Domain:** Offline PKI + AXIAM BYOK/tenant CAs, AXIAM org/tenant/RBAC bootstrap via the Rust SDK, Docker Compose + Caddy single origin, RabbitMQ MQTT-over-mTLS with an HTTP auth backend
**Confidence:** HIGH for the AXIAM/SDK surface (read from source this session). HIGH for RabbitMQ broker behavior (read from the v4.3.x Erlang source). MEDIUM for browser/OS trust steps and cross-compile details.

## Summary

Phase 1 is feasible as designed, but source reading turned up **seven facts that change how the planner must shape tasks**. Four of them touch locked decisions and need user sign-off (see "Research Conflicts with Locked Decisions" right after User Constraints):

1. **The Rust SDK cannot switch the acting tenant.** An org-level super-admin reaches a tenant only through the `X-Axiam-Tenant` header, and `axiam-sdk` 1.0.0-beta16 never sends it. The practical result: `domo-bootstrap` handles org-scoped work through the SDK as the super-admin. That covers tenants, the root import, signing CAs and the trust anchor. It then hand-rolls about four calls per tenant to create a **tenant-level admin user**, and does all tenant-scoped work (catalog, resources, groups, service accounts, CSR signing, binding) through a second SDK client logged in as that tenant admin. Every hand-rolled call becomes a dogfooding entry.
2. **The Rust SDK has no `POST /api/v1/auth/device` method.** `with_client_cert` exists, but nothing calls the device-login endpoint. `domo-probe` must hand-roll it (reqwest + rustls identity). The C++ SDK does have `authenticate_device()`.
3. **RabbitMQ's HTTP backend never sees the client certificate, but it can be bound to the cert anyway.** With `mqtt.ssl_cert_client_id_from = distinguished_name`, the broker itself rejects a CONNECT whose MQTT client_id differs from the cert's subject DN. The HTTP backend then receives `username`, `password` (the JWT) and that broker-verified `client_id`. AXIAM leaves carry a CN-only subject, so a CSR with `CN=<service-account-uuid>` lets the backend check `jwt.sub == username` and `client_id == "CN=" + username`. This meets MQTT-02, including the mismatched-cert and cross-tenant negative cases. It is stronger than D-24's fallback. Residual risk: an admin of the *other* tenant could mint a cert with a forged CN. The fix would need AXIAM to put `cnf` in device tokens, which it does not do today.
4. **The MQTT TLS listener has no TLS settings of its own.** It uses the broker-wide `ssl_options.*`, the same ones as AXIAM's AMQPS listener. `verify_peer` + `fail_if_no_peer_cert` therefore also forces **AXIAM's own AMQP link to present a client cert**, which AXIAM supports (`AXIAM__AMQP__TLS__CLIENT_CERT_PATH`/`KEY_PATH`). Recommended: set `cacertfile` = the org root, not "the tenant signing CAs" (D-26). Tenant CAs change on every reset and don't exist when the broker first starts.
5. **Service-account tokens (`aud: axiam:m2m`) are rejected by every REST management route.** Management handlers use the `AuthenticatedUser` extractor. It affects Phase 2, and D-20 said the Management Platform would manage AXIAM with an mTLS service account. That cannot work for resources, groups or roles. It still works for `POST /authz/check` (`AuthenticatedPrincipal`).
6. **AXIAM can now generate RSA-4096 CAs.** The generation code landed 2026-08-25 and is in every `v1.0.0-beta*` tag. `docs/pki/README.md` still says it can't. Each tenant signing CA's algorithm is the caller's choice (`key_algorithm: Rsa4096 | Ed25519`).
7. **No two-stage TLS bootstrap is needed.** The root is generated offline *before* AXIAM starts, so every server cert (AXIAM, Caddy, RabbitMQ, Postgres, Twin), plus AXIAM's AMQP client cert, exists at first boot. `ARCHITECTURE.md` §5 (temporary self-signed cert, SIGHUP) is superseded.

**Primary recommendation:** build one Rust workspace (`tools/domo-bootstrap`, `tools/domo-probe`, `services/device-twin`, `crates/domo-common`). Use `axiam-sdk = "=1.0.0-beta16"` for every AXIAM call it covers, and a tiny, logged `hand_rolled` module for the rest (`/admin/bootstrap`, the tenant-admin provisioning calls, `/auth/device`). Put the whole broker trust on the org root. Bind MQTT identity with `ssl_cert_client_id_from = distinguished_name` and `CN=<sa-uuid>`. Prove everything with a `just smoke` positive/negative matrix.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### Stack & networking
- **D-01:** AXIAM runs from the **released multi-arch images** `ghcr.io/ilpanich/axiam/{server,frontend}`, pinned by a single `AXIAM_IMAGE_TAG` in `.env`. Nothing is built from `../axiam`. The pinned tag is the AXIAM version the demo claims to validate. Record it in the dogfooding findings header.
- **D-02:** This repo has **its own compose file**. AXIAM's services are copied from `../axiam/docker/docker-compose.prod.yml` and adapted: TLS on axiam-server, MQTT plugin and `domo` vhost on RabbitMQ, no Vault, and memory limits. It has no `include:` of the sibling checkout, so the Pi needs only this repo. — **Reversibility:** reversible
- **D-03:** The **AXIAM admin console (axiam-frontend) is part of the stack**, behind Caddy, so evaluators can inspect real tenants, resources, groups and memberships.
- **D-04:** The host name is configurable as `DOMO_HOST` (default `domo.local`, advertised via mDNS/Avahi). Server certs carry SANs for `DOMO_HOST`, `axiam.DOMO_HOST`, the detected LAN IP, `localhost`/`127.0.0.1` and the compose service names they're reached by internally.
- **D-05:** The simulator PC reaches AXIAM and the broker **directly with native mTLS**:
  - AXIAM's own TLS listener is exposed on the LAN, with client-cert policy `optional` so one listener serves both device mTLS login and ordinary TLS.
  - RabbitMQ MQTTS (8883) is exposed on the LAN.
  - Nothing else leaves the Docker network: Postgres, AMQPS, SurrealDB and the Twin's auth-backend endpoint stay internal.
  - AXIAM's forwarded `X-Client-Certificate` proxy path (`AXIAM__AUTH__TRUST_FORWARDED_CLIENT_CERT`) is **not** used.
- **D-06:** **One compose file plus a per-host `.env`**, generated or selected by `just` recipes. The XPS and the Pi run the same images and the same topology. There is no Pi override file.
- **D-07:** **TLS 1.3 only** on every listener we configure: Caddy, MQTTS, and later the Twin and Management Platform. This matches AXIAM's own `rabbitmq-tls.conf` stance.

### Bootstrap & reset tooling
- **D-08:** Bootstrap is a **Rust CLI, `domo-bootstrap`**, built on the **AXIAM Rust SDK management client** for every AXIAM call. It covers org bootstrap, tenants, the BYOK import, tenant signing CAs, the catalog, service accounts, certificate sign/bind and the `domo` vhost. Offline PKI (root, SAN server certs) uses rcgen/openssl. Any AXIAM call the SDK doesn't cover is hand-rolled over HTTP **and** gets an entry in the dogfooding findings. `/admin/bootstrap` is the likely example.
- **D-09:** **`just demo-reset` wipes volumes** (SurrealDB, Postgres, RabbitMQ), then re-runs the full bootstrap. It never tears down through the API. This is the path that stays idempotent and survives interruption, and it sidesteps the `tenants.delete` → 409 precondition, which requires `export_audit` within the last 6 h. — **Reversibility:** reversible
- **D-10:** **The org root is stable across resets.** It is generated once and kept in `.secrets/`. Every reset re-imports it and re-issues everything below it: tenant CAs, server certs and client certs. Browsers and the sim PC therefore stay trusted. A separate `just pki-rotate-root` starts completely fresh (new root, re-trust required).
- **D-11:** **The root is RSA-4096** (locked by constraint, not preference):
  - AXIAM CAs can only be RSA-4096 or Ed25519.
  - Chromium and Firefox don't accept Ed25519 in server cert chains.
  - AXIAM can import RSA CAs but not generate them, which fits BYOK.
  - **Tenant signing CAs** are generated by AXIAM (`ca_certificates.generate_signing_ca`), which means Ed25519. Research must confirm the algorithm AXIAM uses for signing CAs under an RSA root.
- **D-12:** The **first-run bootstrap gate uses the one-time setup token**, scraped from axiam-server's first-boot log line (`AXIAM first-run bootstrap setup token minted ... "setup_token":"..."`). This is the production-shaped path; **`AXIAM_BOOTSTRAP_ADMIN_EMAIL` is not set**. The scraper must:
  - wait for the log line with a timeout
  - handle an already-consumed token (a second bootstrap returns 403 "gate not satisfied", not 409) by detecting an initialized system with a probe
  - fail with a clear message
  - persist the super-admin credentials to `.secrets/`
- **D-13:** **Secrets live in `./.secrets/` inside the repo.** The directory is git-ignored and docker-ignored, files are chmod 600, and it is mounted read-only per service. It holds the root key, generated `.env`, service creds and keys, server keys, stage markers and the demo card. The root key is never baked into images (PKI-06). A guard stops `.secrets/` from ever being added to git or a build context.
- **D-14:** **`just export-trust`** writes the root to `./dist/trust/` as PEM and DER, with its SHA-256 fingerprint. It prints per-target import steps: ArchLinux trust anchors, Raspberry Pi OS / Debian `update-ca-certificates`, Firefox NSS, Chromium. The root is **not** served over HTTP. A sim-host bundle recipe (host service creds) comes in Phase 4.
- **D-15:** **Server certs** use **ECDSA P-256** leaf keys signed by the RSA-4096 root, valid for about 397 days (below the browser lifetime cap). Reset re-issues them.

### AuthZ model seeding
- **D-16:** The **role/permission catalog is a checked-in declarative file, `authz/catalog.toml`**. It lists the roles (property-manager, installer, concierge, resident, granted-operator, device-self, common-operator, common-device-manager, as in PROJECT.md), each role's enumerated `resource:verb` permissions (there is no wildcard; `structure:*` must be expanded), and the group templates. `domo-bootstrap` applies it to each tenant idempotently. The Management Platform (Phase 2) only creates resources and groups against it and never defines roles. — **Reversibility:** costly — Phase 2/3 services and the smoke checks depend on role and permission names.
- **D-17:** **Naming: readable, plus a stable ID.**
  - AXIAM resource names are readable and prefixed by type, e.g. `site:{slug}`, `building:{slug}`, `apartment:{building}-{unit}`, `common:site:{slug}`, `device:{slug}`.
  - The stable key is the domain UUID, kept in resource metadata/description or mapped in Postgres. Research picks the field AXIAM offers.
  - Groups follow `{role}@{type}:{slug}`, e.g. `installer@site:lakeside-park`, `granted-operator@device:apt-a1-light-1`.
  - — **Reversibility:** costly — every service and the seed resolve resources and groups through this scheme.
- **D-18:** **Phase 1's tree scope:**
  - `domo-bootstrap` creates the tenants, the catalog and each tenant's `portfolio` root.
  - A separate **`just smoke`** creates **one branch in tenant A** (site → common:site → building → common:building → apartment → device), with its structural groups, and hosts the probe device.
  - The full two-tenant seed is **Phase 2's job via the Management Platform**, so no data ever has two writers.
  - The smoke branch is removed by reset. It must not collide with Phase 2 seed names: use a `smoke-` prefix.
- **D-19:** **Demo tenants are fictional brands:** **Lakeside Residences** (`lakeside`) and **Summit Homes** (`summit`). The names are editable later, but tests and docs refer to these slugs.
- **D-20:** **One service account per (service, tenant)**, e.g. `mgmt@lakeside`, `mgmt@summit`, `twin@lakeside`, `twin@summit`, and sim-host accounts in Phase 4.
  - Each account's mTLS client cert is issued by **that tenant's signing CA** (PKI-03).
  - A service bug can't cross tenants.
  - The org-level super-admin is used **only by `domo-bootstrap`**.
  - **This supersedes PROJECT.md's "org-scope service account over mTLS" for the Management Platform.** Update PROJECT.md at phase transition.
  - — **Reversibility:** costly — the Phase 2/3 services hold one credential set per tenant and route by tenant.
- **D-21:** **Group creation is eager for structural groups and lazy for grants.**
  - The groups for a site, common area or apartment are created together with that resource: `installer@`, `concierge@`, `resident@`, `property-manager@portfolio`, `common-operator@`, `common-device-manager@`.
  - `granted-operator@device:*` groups are created on first grant (Phase 2).
  - Phase 1 establishes the helper and the pattern; the smoke branch exercises it.

### MQTT proof shape
- **D-22:** The **RabbitMQ HTTP auth backend lives in the real Device Twin crate** (`services/device-twin`: Rust/Actix plus the Rust SDK). In Phase 1 it contains only the auth endpoints (user/vhost/resource/topic) and health. Phase 3 grows it into the full Twin, so no spike code is thrown away.
- **D-23:** The **test device is `tools/domo-probe` (Rust)**. It:
  1. generates an Ed25519 key and CSR locally
  2. gets the cert signed by the tenant CA and bound to its device service account (the private key never leaves the probe)
  3. logs in via the Rust SDK's mTLS `auth/device`
  4. connects with rumqttc and rustls
  5. publishes and subscribes on its own `domo/{tenant}/{device}/…` topic

  Phase 3 reuses it as the **114-connection load harness**. In Phase 1, sign and bind are driven by `just smoke` through the bootstrap/admin tooling, since the Management Platform doesn't exist yet.
- **D-24:** **MQTT fallback, locked in advance.** If research shows `rabbitmq_auth_backend_http` can't receive both the client-cert identity and the CONNECT password (the JWT), keep the JWT:
  - The broker enforces the cert at TLS: `verify_peer` + `fail_if_no_peer_cert`, trusting only the tenant signing CAs.
  - username = the device service-account ID, password = the AXIAM JWT.
  - The backend validates the JWT with AXIAM and checks `sub == username`.
  - If AXIAM puts a cert thumbprint (`cnf`/`x5t#S256`) in mTLS-login tokens and any cert info reaches the backend, it binds on that.
  - The weakened subject↔cert check is logged as a dogfooding finding.
  - If the primary design works (the backend sees both), use it as agreed in PROJECT.md.
- **D-25:** **The proof is automated, with positive and negative cases.** `just smoke` asserts:
  - (+) the probe connects and publishes/subscribes on its own topics
  - (−) a JWT presented with a mismatched cert is rejected
  - (−) an expired or garbage JWT is rejected
  - (−) a connection with no client cert is rejected
  - (−) a cert from the *other* tenant's CA presented for this tenant's device is rejected

  It can be re-run after every reset.
- **D-26:** **Broker config:**
  - An `enabled_plugins` file adds `rabbitmq_mqtt` and `rabbitmq_auth_backend_http`.
  - A **`30-mqtt.conf` conf.d fragment** sets: the MQTTS listener on 8883, TLS 1.3, `verify_peer`, `fail_if_no_peer_cert`, CA bundle = the tenant signing CAs, `auth_backends.1 = internal` (AXIAM's own AMQP user keeps working), `auth_backends.2 = http` (devices, pointed at the Twin).
  - It loads alongside AXIAM's `20-tls.conf`. Use sysctl-format keys only; AXIAM's own notes record that the erl-args form breaks prelaunch.
  - `domo-bootstrap` creates the **`domo` vhost** idempotently via the RabbitMQ management API.
  - AXIAM's `/` vhost is untouched. **No `definitions.json`**, because `load_definitions` suppresses the default-user creation AXIAM depends on.
- **D-27:** **Device keys are Ed25519.** The phase must verify that RabbitMQ (Erlang TLS 1.3), rustls, and later OpenSSL/Paho accept the Ed25519 client cert chained to an AXIAM tenant CA. If the broker rejects it, stop and record a finding before switching algorithms.

### Repo layout
- **D-28:** **Monorepo organized by component kind:**
  - `services/{management-platform,device-twin}`
  - `tools/{domo-bootstrap,domo-probe}`
  - `simulators/{lights-c,intercom-cpp,thermostat-rust}`
  - `apps/{staff-console,resident-app,sim-control}` and `packages/shared`
  - `deploy/{compose.yml,caddy/,rabbitmq/,postgres/,landing/}`
  - `authz/catalog.toml`, `docs/`, the root `justfile`, and `.secrets/` (git-ignored)

  Phase 1 creates only what it uses. — **Reversibility:** costly — paths are baked into the justfile, compose, CI and docs.
- **D-29:** **One root Cargo workspace** for all Rust code: device-twin, domo-bootstrap, domo-probe, later thermostat-rust, and a shared **`domo-common`** crate for rustls/mTLS config, the MQTT topic scheme and AXIAM client setup. One `target/` and one lockfile keep disk use down; see the disk-hygiene constraint.

### Caddy route map
- **D-30:** **AXIAM is unprefixed on the portal origin, and the console gets its own host.** This replaces PLAT-06's `/axiam` wording and PROJECT.md's diagram; update both at phase transition.
  - `https://{DOMO_HOST}`:
    - `/` → static landing page
    - `/staff/*`, `/resident/*`, `/sim/*` → portal SPAs (Phase 5)
    - `/api/mgmt/*` → management-platform (Phase 2)
    - `/api/twin/*` → device-twin
    - `/api/v1/*`, `/oauth2/*`, `/.well-known/*` → axiam-server (unprefixed, as the AXIAM SDKs and the WASM SDK expect)
  - `https://axiam.{DOMO_HOST}` → axiam-frontend (console) and its `/api`, `/oauth2/`, `/.well-known` → axiam-server. It needs its own SAN and an mDNS alias.
  - **Why:** the console SPA is root-mounted (`../axiam/docker/nginx.conf.template`), and a separate host gives it its own cookie jar. `axiam_access`/`axiam_refresh`/`axiam_csrf` would otherwise collide between a console super-admin and a portal user in the same browser. Ports don't isolate cookies.
  - **Rule:** `/api/mgmt` and `/api/twin` must be matched **before** AXIAM's `/api` routes.
- **D-31:** **`/` serves a static demo landing page** (`deploy/landing/`). It lists the portals (marked "coming" until Phase 5), links to the AXIAM console, and shows the root cert fingerprint and links to the trust docs. In Phase 1 it proves the Caddy TLS chain in a browser; later it becomes the demo's front door.

### Dogfooding log process
- **D-32:** **Start `docs/dogfooding-findings.md` in Phase 1**, seeded with the 7 known gaps from DOC-05, with a fixed entry format:
  - `DF-NNN`
  - AXIAM image tag and SDK name/version
  - component
  - severity
  - expected vs actual
  - reproduction
  - workaround
  - status

  Every phase adds entries as it hits gaps. **Each phase's verification checks that every hand-rolled AXIAM call has a matching entry.** Phase 6 only polishes.
- **D-33:** **Each finding also carries a ready-to-paste upstream issue body** (title and body for the AXIAM server or SDK repo). **Nothing is filed automatically**; filing is the user's call.

### Operator UX of setup
- **D-34:** **`just up` / `just demo-reset` run a staged, resumable checklist.** The stages are named: preflight → pki → axiam-up → org-bootstrap → tenants → catalog → service-certs → broker → platform-up → verify.
  - Each stage is idempotent and writes a marker in `.secrets/state/`, so a re-run resumes at the failed stage. Reset clears the markers except the root's.
  - Output is a ✓/✗ checklist. A failure prints the stage, the cause, and the last log lines of the relevant container.
- **D-35:** **Preflight checks:**
  - disk space (refuse below about 8 GB free)
  - tools and versions (docker + compose v2, buildx, just, openssl; cargo only when building locally)
  - host and ports (`DOMO_HOST` resolves via mDNS or hosts, the LAN IP is detected, 443/8883 and AXIAM's TLS port are free)

  The user explicitly left out the Pi RAM/swap check.
- **D-36:** **A successful run prints a demo card** and writes it to `.secrets/demo-card.txt`: the portal and console URLs, the super-admin login, the root fingerprint, a `just export-trust` hint and next steps. Later phases append seeded users.

### Claude's Discretion
- The exact ports (other than 443/8883), container names and the resource limits per service.
- Exact stage names and marker format, the catalog file schema details, and whether to use rcgen or openssl for offline PKI.
- Landing page styling (keep it small and static).
- The health-wait timeouts and how the setup-token log line is matched, as long as D-12's failure behaviour holds.
- Twin auth-backend endpoint paths and the response format, following RabbitMQ's documented HTTP backend contract.
- How the Twin validates device JWTs (gRPC `ValidateToken`/introspect via the SDK is the default), and which per-tenant credential it uses for each tenant.

### Deferred Ideas (OUT OF SCOPE)
- **Sim-host trust/credential bundle** (`just export-sim-bundle`): Phase 4, when the simulator hosts exist.
- **Serving the root cert over plain HTTP** for one-click download: considered and rejected for now in favour of `just export-trust` plus docs.
- **Pi RAM/swap preflight warning:** the user left it out of preflight. Pi memory is validated on real hardware in Phase 6 (PLAT-03).
- **Filing findings upstream:** issue bodies are drafted (D-33), but filing stays a manual decision for the user.
</user_constraints>

## Research Conflicts with Locked Decisions (user confirmation needed)

The planner must **not** silently deviate. Each item below needs either a `checkpoint:decision` task early in the phase or a quick discuss-phase follow-up.

| # | Locked decision | What the source shows | Recommended resolution |
|---|---|---|---|
| C-1 | **D-26**: MQTT CA bundle = "the tenant signing CAs" | The MQTT TLS listener takes **only** a port (`mqtt.listeners.ssl.$name` → `{datatype, [integer, ip]}`) and uses the broker-wide `ssl_options.*` that also govern AXIAM's AMQPS 5671 [VERIFIED: rabbitmq_mqtt.schema v4.3.x lines 143-160; CITED: rabbitmq.com/docs/mqtt "The plugin uses core RabbitMQ server certificates and keys"]. Tenant CAs are created by AXIAM *after* the broker starts, and are re-created on every reset (D-10), so a tenant-CA bundle means a broker restart on every reset. It also forces AXIAM's own AMQP client cert to chain to a tenant CA. | `ssl_options.cacertfile` = **org root** (stable, exists before first boot). Tenant separation moves to the HTTP backend (C-2). AXIAM's AMQPS link presents an **offline root-signed client cert** (C-4). |
| C-2 | **D-24**: fallback "backend can't see cert" | True: the HTTP backend's `user_path` receives `username`, `password`, `vhost`, `client_id` and nothing about the certificate [VERIFIED: rabbit_auth_backend_http.erl v4.3.x `user_login_authentication` + `extract_other_credentials`; rabbit_mqtt_processor.erl `check_user_login` AuthProps `[{vhost, VHost}, {client_id, ClientId}, {password, Password}]`]. However, `mqtt.ssl_cert_client_id_from = distinguished_name` makes the **broker** reject a CONNECT whose client_id ≠ the cert subject DN (`extract_client_id_from_certificate` → `RC_CLIENT_IDENTIFIER_NOT_VALID`), and it runs whether or not `ssl_cert_login` is on. | Adopt the **strengthened fallback**: CSR `CN=<device-sa-uuid>`, CONNECT `client_id = "CN=<sa-uuid>"`, `username = <sa-uuid>`, `password = JWT`. Backend: JWT valid ∧ `sub == username` ∧ `client_id == "CN=" + username`. Log the residual (forged-CN cert from another tenant's admin + stolen JWT) as a finding. |
| C-3 | **D-11**: "AXIAM can import RSA CAs but not generate them … tenant signing CAs … means Ed25519" | `generate_keypair` handles `KeyAlgorithm::Rsa4096 => generate_rsa_keypair()` [VERIFIED: axiam/crates/axiam-pki/src/crypto.rs:71-76], added by commit `bacc92a1d` 2026-08-25 (contained in `v1.0.0-beta01`…`beta16`). `CreateIntermediateCaRequest` takes a caller-chosen `key_algorithm` [VERIFIED: axiam-rust-sdk/src/management/models.rs:1019-1028]. | The root stays RSA-4096 + BYOK (still right for browsers). **Tenant signing CA = Ed25519 by choice, not by force** (instant keygen on the Pi, consistent with Ed25519 leaves). RSA-4096 is a one-field fallback if D-27's broker check fails. Log the stale-doc finding. |
| C-4 | **PKI-03**: "every device and service client cert is issued by AXIAM" | With broker-wide `fail_if_no_peer_cert = true`, AXIAM's own AMQP client needs a cert **before AXIAM runs**. AXIAM supports it: `client_cert_path`/`client_key_path`, PKCS#8 PEM [VERIFIED: axiam/crates/axiam-amqp/src/config.rs:58-68; connection.rs:57-69 `OwnedIdentity::PKCS8 { pem, key }`]. | Treat AXIAM's AMQP link cert as **infrastructure**, offline root-signed like the server certs. Record it as a documented exception in the dogfooding file. |
| C-5 | **D-20**: the Management Platform manages AXIAM with per-tenant **service accounts** (Phase 2) | Every REST management handler takes `AuthenticatedUser` (resources 7, groups 12, roles 17, permissions 8, service_accounts 6, certificates 6, users 6 uses; 0 `AuthenticatedPrincipal`), and `check_user_aud_and_parse_jti` rejects `Some(AUD_M2M)` with `"audience mismatch — this route requires axiam:user audience"` [VERIFIED: axiam/crates/axiam-api-rest/src/extractors/auth.rs:951-992, 1080-1113]. `POST /authz/check` accepts service accounts (`AuthenticatedPrincipal`, authz_check.rs:28,170,261). | Phase 1 still creates `mgmt@`/`twin@` service accounts and certs (fine for CheckAccess and device-style auth). **Flag for Phase 2:** the Management Platform needs a per-tenant *user* principal for management calls. The per-tenant admin user Phase 1 creates anyway (Pattern 3) is the natural candidate. Log as a finding. |
| C-6 | **D-23 step 3**: "logs in via the Rust SDK's mTLS `auth/device`" | No such method exists: `grep 'auth/device' axiam-rust-sdk/src` → no hits, and `AxiamClient` has `with_client_cert` but no device-login op [VERIFIED: src/client.rs:262, src/rest/auth.rs pub fns = `login`, `verify_mfa`, `refresh`, `logout`]. | Build the client identity with the SDK's rules, hand-roll `POST /api/v1/auth/device`, and log the finding. |
| C-7 | **Claude's Discretion**: Twin validates JWTs via "gRPC `ValidateToken`/introspect via the SDK" | The SDK's gRPC client wraps only `check_access`/`batch_check` [VERIFIED: src/grpc/client.rs:220,240]. `validate_token` exists only in the raw generated stub `axiam_sdk::grpc::r#gen` (src/gen/axiam.v1.rs:710). The SDK's first-class validator is `token::JwksVerifier`. | Use `JwksVerifier` (local EdDSA verify, `expect_tenant_id`, `expect_audience("axiam:m2m")`). It is SDK-native, so no finding is needed for the choice. Note the missing wrapper as a minor finding. |

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PLAT-01 | One-command start on XPS (amd64) | Compose skeleton (Pattern 1); all images verified multi-arch; staged `just up` (Pattern 2) |
| PLAT-02 | One-command start on Pi 5 (arm64), multi-arch images | `ghcr.io/ilpanich/axiam/{server,frontend}:1.0.0-beta16`, `surrealdb:v3`, `rabbitmq:4-management-alpine`, `postgres:17-alpine`, `caddy:2.11-alpine` all publish linux/arm64 [VERIFIED: `docker buildx imagetools inspect`]; our Rust images need a cross-build (Pattern 8) |
| PLAT-05 | `just demo-reset` idempotent, re-runnable after interruption | Volume wipe + stage markers (Pattern 2); setup-token pitfall (P-3); no resource-name uniqueness (P-8) |
| PLAT-06 | Single HTTPS origin via Caddy | Caddyfile (Code Example 4); D-30 route order; frontend nginx already proxies `/api`,`/oauth2/`,`/.well-known` |
| PKI-01 | Root generated at setup, BYOK import, single anchor | `ca_certificates.import_ca` [VERIFIED]; PKCS#8 key requirement (P-5) |
| PKI-02 | One tenant signing CA per tenant | `ca_certificates.generate_signing_ca(tenant_id, CreateIntermediateCaRequest{key_algorithm, parent_ca_id, subject, validity_days})` [VERIFIED] |
| PKI-03 | Device/service client certs from the tenant CA | `certificates.sign_csr` + `service_accounts.bind_certificate` via the **tenant-admin** SDK client (leaf tenant = acting tenant) [VERIFIED: handlers/certificates.rs sign_csr `tenant_id: user.tenant_id`] |
| PKI-04 | Server certs with SANs, offline-signed by root; browsers trust | openssl recipe (Code Example 1); P-256 leaf, EKU serverAuth, ≤397 d |
| PKI-05 | Export root + documented trust steps | `just export-trust` (PEM/DER/fingerprint) + per-OS/browser steps (Pattern 7) |
| PKI-06 | Root key only in the git/docker-ignored secrets dir | `.gitignore`, `.dockerignore`, pre-commit guard, `tls-init` copy pattern (P-6) |
| AUTHZ-01 | Tree portfolio → site → {common, building} → {common, apartment} → device | `resources.create(CreateResourceRequest{name, resource_type, parent_id, metadata})`; `list_ancestors`/`list_children` for verification [VERIFIED] |
| AUTHZ-02 | Group per (role, resource); access only via membership | `groups.create` + `roles.assign_to_group(role_id, AssignRoleToGroupRequest{group_id, resource_id})` + `groups.add_member`/`add_service_account`; verify with `check_access_as` [VERIFIED] |
| MQTT-01 | Devices connect to the `domo` vhost via MQTT over mTLS | `30-mqtt.conf` (Code Example 3); Ed25519 leaf chain check (D-27) |
| MQTT-02 | JWT as MQTT password; backend rejects a subject/cert mismatch | C-2 binding design; Twin endpoints (Code Example 6) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

From `./CLAUDE.md`, `./.claude/CLAUDE.md`, `~/.claude/CLAUDE.md` and `/home/emanuele/CLAUDE.md`:

- **Use the AXIAM SDKs for every AXIAM interaction.** Hand-rolled HTTP is allowed only where no SDK covers the call, and each such case gets a dogfooding entry.
- **Don't extend AXIAM or its SDKs.** Log gaps and don't fix them.
- **Languages are fixed.** Device Twin and the tools are Rust (edition 2024, MSRV 1.88, matching `axiam-sdk`). Actix-web is required for the SDK's guards.
- **Local, LAN-only.** No internet exposure, no ACME, no `tls internal`.
- **Single trust anchor = the AXIAM org root.** Server certs are offline-signed by it.
- **Disk hygiene (MUST).**
  - Run `df -h /home /` before any build, test or image build.
  - Run `cargo clean`, or at least `rm -rf target/debug/incremental`, when done.
  - Stop if free space drops below 8 GB. No heavy parallel builds unless ≥25 GB is free.
  - Never prune docker volumes; they hold AXIAM dev data.
  - Plans must include a cleanup step after build/test tasks. There is a single root `target/` (D-29).
- **Repo rules.**
  - Keep files under 500 lines.
  - Validate input at system boundaries.
  - Never commit secrets or `.env` files.
  - Read before editing.
  - Don't save working files to the root.
- **GSD workflow.** Edits go through `/gsd-execute-phase`. Record build/test commands in `CLAUDE.md` once the components exist; the repo has none today.
- **Build & test.** Run `npm run build && npm test` or the project equivalent before committing. For this repo that means `cargo build --workspace && cargo test --workspace`, plus `just smoke` for the stack.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Root generation, SAN server certs | Operator host (offline, `just pki`) | — | Must exist before AXIAM boots; root key never enters a container except as a one-shot import payload |
| Root BYOK import, tenant CAs, trust anchor flag | AXIAM (via `domo-bootstrap` SDK org client) | — | AXIAM owns the CA hierarchy and custody (`database`) |
| Tenant/catalog/resource/group/SA/cert provisioning | AXIAM (via `domo-bootstrap` SDK **tenant-admin** client) | Hand-rolled HTTP for acting-tenant bootstrap | SDK lacks `X-Axiam-Tenant`; leaf tenant = acting tenant |
| Browser TLS termination + routing | Caddy (edge) | axiam-frontend nginx (console `/api` proxy) | Single origin; cookies per host |
| Device authN (cert → JWT) | AXIAM native mTLS listener (`/api/v1/auth/device`) | — | D-05: direct, no proxy header path |
| MQTT TLS + cert↔client_id binding | RabbitMQ (broker) | — | Only the broker sees the peer cert |
| MQTT authN/authZ decisions | Device Twin HTTP auth backend | AXIAM JWKS (signature trust) | D-22; stateless JWT verify + tenant/topic namespace rules |
| Stage orchestration, volume wipe, log scraping | `just` on the host | `domo-bootstrap` (API work) | Docker lifecycle belongs to the host; API work runs on the compose network |
| Positive/negative proof | `domo-probe` + `just smoke` | `cargo test` unit tests (Twin) | End-to-end against the real broker plus deterministic unit cases (expiry) |

## Standard Stack

### Core
| Library / Image | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axiam-sdk` (crates.io) | `=1.0.0-beta16` (published 2026-09-19) | AXIAM management client, `JwksVerifier`, client-cert config | Required by the project. Matches the local checkout (`Cargo.toml` `version = "1.0.0-beta16"`, `rust-version = "1.88"`, `edition = "2024"`) [VERIFIED: axiam-rust-sdk/Cargo.toml:18-21; crates.io API]. Pin exactly, since it's a pre-release |
| AXIAM images | `ghcr.io/ilpanich/axiam/server:1.0.0-beta16`, `…/frontend:1.0.0-beta16` | IAM server + console | D-01; amd64 + arm64 manifests [VERIFIED: buildx imagetools]. Tag has **no `v` prefix** |
| `rabbitmq` | `4-management-alpine` → currently **RabbitMQ 4.3.6** (not 4.2.x) | Broker (AXIAM's) + MQTT | [VERIFIED: image env `RABBITMQ_VERSION=4.3.6`]. Recommend pinning `rabbitmq:4.3-management-alpine` for reproducibility |
| `surrealdb/surrealdb` | `v3` | AXIAM datastore | As in AXIAM prod compose; amd64 + arm64 |
| `postgres` | `17-alpine` | Mgmt + Twin DBs (started in Phase 1, schemas later) | Project decision (PG18/Flyway issue) |
| `caddy` | `2.11-alpine` (2.11.4) | Single-origin TLS proxy | Project decision; static `tls cert key` |
| `actix-web` | 4.15.0, feature `rustls-0_23` | Twin HTTPS server (auth backend + health) | Required by the SDK's Actix guards; `rustls-0_23` feature exists [VERIFIED: crates.io features] |
| `rumqttc` | 0.25.1 (default `use-rustls`) | Probe MQTT client | Project decision; pure-Rust TLS |
| `rustls` | 0.23.45 | TLS client configs (probe, bootstrap) | Same major as the SDK's reqwest/tonic |
| `reqwest` | 0.13.5, `default-features=false`, features `rustls`,`json` | Hand-rolled AXIAM calls + RabbitMQ mgmt API | Same major the SDK uses (`reqwest = { version = "0.13", … features = ["json","rustls","cookies","form"] }` [VERIFIED: axiam-rust-sdk/Cargo.toml:386]) |
| `rcgen` | 0.14.10 | Probe: Ed25519 keypair + PKCS#10 CSR | Same major as the SDK dev-dep; ring backend generates Ed25519 |
| `openssl` CLI | 3.x (host 3.6.4) | Offline root (RSA-4096) + ECDSA P-256 server certs | rcgen/ring can't generate RSA (AXIAM's own crypto.rs explains why); openssl is already a preflight tool (D-35) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.53.x (`full` in bins) | Async runtime | All binaries |
| `clap` | 4.6.x (derive) | CLI subcommands for `domo-bootstrap`/`domo-probe` | Stage subcommands |
| `serde`, `serde_json`, `toml` (1.1.x) | current | `authz/catalog.toml`, state files | Catalog parsing |
| `uuid` | 1.26 | IDs, metadata `domo_id` | Everywhere |
| `thiserror`/`anyhow` | current | Errors (lib/bin) | — |
| `tracing`, `tracing-subscriber` | 0.1 / 0.3.23 | Logs (never log JWTs/keys) | — |
| `x509-parser` | 0.18.1 | Probe/verify: read cert subject, fingerprint, SAN checks in `verify` | Verification stage |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `JwksVerifier` in the Twin | raw `axiam_sdk::grpc::r#gen` `TokenService.validate_token` | Makes a network round trip per CONNECT and uses an undocumented generated surface. Only worth it if revocation-on-CONNECT becomes a requirement |
| openssl CLI for offline PKI | rcgen + `rsa` crate (as AXIAM does) | Pure Rust and testable, but more code; the openssl CLI is already a preflight dependency |
| Root as broker `cacertfile` | tenant-CA bundle (D-26 literal) | See C-1: broker restart every reset, and a chicken-and-egg with AXIAM's AMQP client cert |
| Tenant signing CA Ed25519 | RSA-4096 | RSA keygen takes seconds (more on a Pi); pick it only if Erlang rejects the Ed25519 chain |

**Installation (workspace deps, illustrative):**
```toml
# Cargo.toml [workspace.dependencies]
axiam-sdk = { version = "=1.0.0-beta16", default-features = false, features = ["rest"] }
actix-web = { version = "4.15", features = ["rustls-0_23"] }
rumqttc = "0.25"
rustls = "0.23"
reqwest = { version = "0.13", default-features = false, features = ["rustls", "json"] }
rcgen = "0.14"
tokio = { version = "1", features = ["full"] }
clap = { version = "4.6", features = ["derive"] }
toml = "1.1"
uuid = { version = "1", features = ["v4", "serde"] }
x509-parser = "0.18"
```
(Only `rest` is enabled on `axiam-sdk`. Phase 1 needs neither `grpc`/`amqp` nor OPAQUE, and `management` is always compiled [VERIFIED: axiam-rust-sdk/src/lib.rs:88-109].)

**Version verification:** crates.io API queried 2026-09-19. Versions: rumqttc 0.25.1, rustls 0.23.45, rcgen 0.14.10, actix-web 4.15.0, reqwest 0.13.5, tokio 1.53.1, clap 4.6.7, toml 1.1.6, x509-parser 0.18.1, uuid 1.26.1.

## Package Legitimacy Audit

`gsd-tools query package-legitimacy check --ecosystem crates …` was run 2026-09-19:

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| axiam-sdk | crates.io | beta line (beta12 2026-09-06 … beta16 2026-09-19) | 678 total | github.com/ilpanich/axiam-rust-sdk | [SUS] (low-downloads) | **Flagged.** This is the user's own first-party SDK and matches the sibling checkout. Planner adds a `checkpoint:human-verify` that the crates.io `1.0.0-beta16` matches `../axiam-rust-sdk` at tag/commit `44d9a49` (or use a `path =` dependency instead) |
| rumqttc | crates.io | multi-year | 8.3M | github.com/bytebeamio/rumqtt | OK | Approved |
| rustls | crates.io | multi-year | 925M | github.com/rustls/rustls | OK | Approved |
| rcgen | crates.io | multi-year | 102M | github.com/rustls/rcgen | OK | Approved |
| actix-web | crates.io | multi-year | 82M | github.com/actix/actix-web | OK | Approved |
| reqwest | crates.io | multi-year | 727M | github.com/seanmonstar/reqwest | OK | Approved |
| tokio, clap, toml, x509-parser, time, uuid, anyhow, serde, serde_json, thiserror, tracing, tracing-subscriber | crates.io | multi-year | 100M+ | official repos | OK | Approved |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** `axiam-sdk`. It's first-party, so verify its provenance and don't remove it.
*No npm/pnpm packages in Phase 1. Cargo has no postinstall scripts, but `build.rs` of transitive deps (aws-lc-sys, ring) compile C code; see P-10.*

## Architecture Patterns

### System Architecture Diagram

```
                         (operator host: just)
  preflight ─► pki (openssl, offline) ─► writes .secrets/pki/{root.*, server/*, axiam-amqp-client.*}
                                        │
                                        ▼  tls-init (one-shot busybox) copies certs into per-service volumes (right uid/mode)
  ┌──────────────────────────── docker compose network ─────────────────────────────────────────┐
  │  surrealdb ◄── axiam-server (TLS :8090, clientAuth=optional, CA=root) ──AMQPS mTLS──► rabbitmq │
  │                    ▲   ▲                                             (ssl_options: verify_peer, │
  │                    │   │ JWKS /oauth2/jwks                           fail_if_no_peer_cert,      │
  │  domo-bootstrap ───┘   │                                             cacertfile=root)           │
  │  (SDK org client + tenant-admin clients; hand-rolled: /admin/bootstrap, tenant-admin, …)        │
  │        │ RabbitMQ mgmt API: PUT /api/vhosts/domo                         │  MQTTS :8883           │
  │        ▼                                                                  │  client_id must == DN │
  │  rabbitmq ──auth_http (POST, TLS)──► device-twin /rmq/{user,vhost,resource,topic}  (JwksVerifier)│
  │  caddy :443 ──► landing | /api/v1,/oauth2,/.well-known → axiam-server | axiam.HOST → frontend    │
  │  postgres (TLS, idle until Phase 2)                                                             │
  └───────────────────────────────────────────────────────────────────────────────────────────────┘
        ▲ LAN :443 (browsers)        ▲ LAN :8090 (device mTLS login)        ▲ LAN :8883 (MQTTS)
                                     domo-probe: CSR(CN=<sa-uuid>) ─► [smoke signs+binds] ─►
                                     POST /api/v1/auth/device (mTLS) ─► JWT ─► MQTT CONNECT
                                     {client_id:"CN=<sa-uuid>", user:<sa-uuid>, pass:JWT}
```

### Recommended Project Structure (Phase 1 subset of D-28)
```
Cargo.toml                 # workspace root (members below), [workspace.dependencies]
crates/domo-common/        # rustls config builders, topic scheme, AXIAM client factory, hand_rolled.rs
tools/domo-bootstrap/      # stages: org-bootstrap, tenants, catalog, service-certs, broker, verify, smoke
tools/domo-probe/          # keygen+CSR, device login, MQTT +/− matrix
services/device-twin/      # Actix HTTPS: /healthz, /rmq/{user,vhost,resource,topic}
authz/catalog.toml         # D-16
deploy/compose.yml         # D-02/D-06
deploy/caddy/Caddyfile
deploy/rabbitmq/{enabled_plugins,30-mqtt.conf}   # plus AXIAM's 20-tls.conf copied in
deploy/postgres/           # postgresql.conf TLS fragment
deploy/landing/index.html  # D-31
deploy/docker/Dockerfile.rust   # one multi-bin build → twin image + tools image
docs/dogfooding-findings.md     # D-32
justfile
.gitignore .dockerignore        # both list .secrets/ and dist/
```
Note: `crates/domo-common` is not in D-28's list, but D-29 names the crate. `crates/` is the Cargo-conventional home; `tools/domo-common` is an alternative if the user prefers the D-28 top-level set. Treat this as a naming discretion item.

### Pattern 1: Compose adapted from AXIAM prod (D-02)
**What:** Copy `axiam-server`, `axiam-frontend`, `surrealdb-init`, `surrealdb` and `rabbitmq` from `../axiam/docker/docker-compose.prod.yml`, then:
- Drop `vault`/`vault-data-perms`. Set `AXIAM__AUTH__SECRET_PROVIDER: "env"`. Valid values are `env`, `file`, `vault` [CITED: axiam/docs/deployment/vault.md:701].
- Enable TLS: `AXIAM__SERVER__TLS__ENABLED=true`, `…CERT_PATH`, `…KEY_PATH`. For client auth, **set `AXIAM__SERVER__TLS__CLIENT_AUTH=optional` together with `AXIAM__SERVER__TLS__CLIENT_CA_PATH=<root.pem>`**. Optional without a CA path aborts startup: `"server.tls.client_auth is optional/required but server.tls.client_ca_path is not set"` [VERIFIED: axiam/crates/axiam-server/src/tls.rs:258-268]. The root exists before boot, so this is static. There's no restart dance.
- AMQP: `AXIAM__AMQP__URL=amqps://…@rabbitmq:5671`, `AXIAM__AMQP__TLS__CA_CERT_PATH=root.pem`, `AXIAM__AMQP__TLS__CLIENT_CERT_PATH`, `AXIAM__AMQP__TLS__CLIENT_KEY_PATH` (PKCS#8).
- Required secrets, generated by `just` into `.secrets/generated.env`. The list comes from AXIAM's table and `just prod-up` [CITED: axiam/docs/deployment/README.md:207-218; axiam/justfile prod-up]:
  - `AXIAM__DB__USERNAME`, `AXIAM__DB__PASSWORD`
  - `AXIAM__AUTH__JWT_PRIVATE_KEY_PEM`, `AXIAM__AUTH__JWT_PUBLIC_KEY_PEM` (Ed25519)
  - `AXIAM__AUTH__MFA_ENCRYPTION_KEY`, `AXIAM__AUTH__FEDERATION_ENCRYPTION_KEY`, `AXIAM__EMAIL_ENCRYPTION_KEY`, `AXIAM__GDPR_PSEUDONYM_PEPPER`
  - `AXIAM__AUTH__PEPPER`, which is mandatory in release builds
  - the AMQP signing key, and the PKI key (see P-9 on the exact name)
  - `AXIAM__AUTH__OPAQUE_SESSION_KEY` and `AXIAM__AUTH__OPAQUE_SETUP_KEY`. The env provider maps a logical key to `AXIAM__AUTH__<NAME>` [VERIFIED: axiam/crates/axiam-auth/src/secrets.rs:82-86]; the names are `opaque_session_key`/`opaque_setup_key` [VERIFIED: axiam/crates/axiam-core/src/secrets.rs:54,59]. Phase 5 needs OPAQUE, so set both now: with only one set, OPAQUE answers 503.
  - `RABBITMQ_DEFAULT_USER`/`PASS`
- Behind Caddy: `AXIAM__RATE_LIMIT__TRUSTED_HOPS=1`, `AXIAM__AUTH__WEBAUTHN_RP_ID=${DOMO_HOST}`, `AXIAM__AUTH__WEBAUTHN_RP_ORIGIN=https://${DOMO_HOST}`.
- Frontend: `AXIAM_BACKEND_ORIGIN=https://axiam-server:8090`, `AXIAM_BACKEND_SNI=axiam-server`, `AXIAM_BACKEND_CA=<root.pem mount>`. Its nginx already proxies `/api`, `/oauth2/` and `/.well-known` with TLS 1.3 verification [VERIFIED: axiam/docker/nginx.conf.template].
- Ports:
  - LAN `:443` → Caddy
  - LAN `:8090` → axiam-server (device mTLS, D-05)
  - LAN `:8883` → RabbitMQ
  - everything else unpublished, or bound to `127.0.0.1` for operator tools

### Pattern 2: Staged, resumable orchestration (D-34)
- `just up` runs `preflight` → `pki` → `axiam-up` → `org-bootstrap` → `tenants` → `catalog` → `service-certs` → `broker` → `platform-up` → `verify`. Each stage checks `.secrets/state/<stage>.done` and prints `✓`/`✗`.
- **Split the ownership.** `just` (host) owns Docker lifecycle: compose up/down, volume wipe, `docker compose logs axiam-server` setup-token scraping, container log tails on failure. `domo-bootstrap` owns API work and runs as a one-shot compose service (`docker compose run --rm domo-bootstrap <stage>`) on the compose network. That way the Pi needs no cargo (D-35) and it reaches `axiam-server`/`rabbitmq` by service name.
- **Idempotency lives inside each stage, not only in markers.** Every create is "list/get by natural key → create if absent → reuse". Tenants are keyed by slug, roles by name, permissions by action, groups by name, resources by name under a parent, and service accounts by name. The marker only lets a re-run skip work.
- `demo-reset` = `docker compose down` → `docker volume rm` for **this project's** volumes only (surrealdb, rabbitmq, postgres, tls volumes; never AXIAM-repo volumes) → clear markers except `pki-root.done` → `just up`.

### Pattern 3: AXIAM call map: SDK vs hand-rolled
| Step | Call | How | Evidence |
|---|---|---|---|
| First-run bootstrap | `POST /api/v1/admin/bootstrap` `{organization_name, organization_slug, email, username, password, setup_token}` (`tenant_name`/`tenant_slug` are deprecated and ignored) | **Hand-rolled** (excluded from SDKs by §27.0) → finding | [VERIFIED: axiam/crates/axiam-api-rest/src/handlers/bootstrap.rs:53-90; management-registry.json:20] |
| Org login | `AxiamClient::builder().base_url(..)?.org_slug(..).tenant_slug("organization").with_custom_ca(root)?` → `login(email, pw)`; read `LoginResult.org_id` | SDK | [VERIFIED: CONTRACT.md §5.2.1; src/rest/auth.rs LoginResult `org_id: Option<Uuid>`] |
| Create tenants | `tenants().in_org(org_id).create(CreateTenantRequest{name, slug, metadata})` | SDK | [VERIFIED: ops/tenants.rs:88; models.rs:1400-1407] |
| Import root | `ca_certificates().in_org(org).import_ca(ImportCaCertificateRequest{private_key_pem: Some(Sensitive), public_cert_pem})` | SDK | [VERIFIED: ops/ca_certificates.rs:113; models.rs:2120-2131] |
| Flag root as mTLS anchor | `set_mtls_trust_anchor(root_id, &SetMtlsTrustAnchor{enabled: true})` | SDK | [VERIFIED: ops/ca_certificates.rs:198; models.rs:3724-3727] |
| Tenant signing CA | `generate_signing_ca(tenant_id, CreateIntermediateCaRequest{key_algorithm, parent_ca_id: root_id, subject, validity_days})` | SDK | [VERIFIED: ops/ca_certificates.rs:261; models.rs:1019-1028] |
| Tenant-admin user in each tenant | `POST /api/v1/users` → `PUT /api/v1/users/{id}` `{"status":"Active"}` → `GET /api/v1/roles` (find `super-admin`) → `POST /api/v1/roles/{id}/users` `{"user_id"}`, all with `X-Axiam-Tenant: <tenant_id>` | **Hand-rolled** (SDK can't send `X-Axiam-Tenant`) → finding | [VERIFIED: axiam/scripts/e2e-bootstrap.sh:280-345; extractors/auth.rs:200 `pub const ACTIVE_TENANT_HEADER: &str = "X-Axiam-Tenant";`; `grep -rni axiam-tenant axiam-rust-sdk/src` → only a doc comment at rest/auth.rs:213] |
| Tenant-admin login | second `AxiamClient` with `.tenant_slug("lakeside")` (and `.tenant_id(..)`) → `login` | SDK | — |
| Catalog | `management().manifest().apply(&ManagementManifest{permissions, roles(with grants)})` | SDK (declarative, idempotent) | [VERIFIED: manifest/mod.rs:128,139; spec.rs:24-37] |
| Portfolio + smoke tree | `resources().create(CreateResourceRequest{name, resource_type, parent_id, metadata})` **imperatively** (manifest `ResourceSpec` has no metadata) | SDK | [VERIFIED: models.rs:1265-1280; spec.rs:85-95] |
| Groups + scoped role | `groups().create(CreateGroupRequest{name, description, metadata})`, `roles().assign_to_group(role_id, AssignRoleToGroupRequest{group_id, resource_id: Some(res)})` **imperatively** (manifest `GroupSpec.roles` carries no resource scope) | SDK | [VERIFIED: models.rs:105-126, 1005-1015; spec.rs:272-281] |
| Service accounts | `service_accounts().create(CreateServiceAccountRequest{name, description})`; `groups().add_service_account(group_id, AddServiceAccountMemberRequest{service_account_id})` | SDK | [VERIFIED: ops/service_accounts.rs:86; ops/groups.rs:271] |
| Sign CSR + bind | `certificates().sign_csr(SignCertificateCsrRequest{cert_type, csr_pem, issuer_ca_id, metadata, validity_days})` → `service_accounts().bind_certificate(sa, BindCertificate{certificate_id})` | SDK (tenant-admin client: leaf tenant = acting tenant; bind requires same tenant) | [VERIFIED: models.rs:3874-3887, 308-311; handlers/certificates.rs sign_csr uses `tenant_id: user.tenant_id`, bind checks both cert and SA against `user.tenant_id`] |
| Device login | `POST /api/v1/auth/device` over mTLS → `{access_token, token_type: "Bearer", expires_in}` | **Hand-rolled** in `domo-probe` → finding | [VERIFIED: axiam/crates/axiam-api-rest/src/handlers/auth.rs:854-897] |
| Twin JWT verify | `token::JwksVerifier::new(http, &base)?.expect_tenant_id(t).expect_audience("axiam:m2m")` → `verify(jwt)` (fetches `/oauth2/jwks`) | SDK | [VERIFIED: src/token/jwks.rs:33 `pub const JWKS_PATH: &str = "/oauth2/jwks";`, :494-557, :789] |
| Verify tree/groups | `resources().list_ancestors/list_children`, `groups().list_roles`, `rest` `check_access_as(subject_id, action, resource_id)` | SDK | [VERIFIED: ops/resources.rs:146,164; src/rest/authz.rs:130-160] |

`CertificateType` values: `User`, `Service`, `Device` [VERIFIED: models.rs:501-511]. `KeyAlgorithm` values: `Rsa4096`, `Ed25519` [VERIFIED: models.rs:2164-2177].

### Pattern 4: Offline PKI
- **Root:** RSA-4096, **PKCS#8** key (`openssl genpkey`). Extensions:
  - `basicConstraints=critical,CA:TRUE`, with no `pathlen` or `pathlen>=1`
  - `keyUsage=critical,keyCertSign,cRLSign`
  - SKI
  - validity of about 10 y
  - stored in `.secrets/pki/root.{key,pem}`, `pki-root.done`, never rotated by reset (D-10)

  AXIAM import refuses anything that is not PEM X.509, lacks `CA:TRUE`, is expired, is neither Ed25519 nor RSA, or has a key/cert mismatch [CITED: axiam/docs/pki/README.md "Import a CA you already have (BYOK)"]. AXIAM parses the key with `rcgen::KeyPair::from_pem` [VERIFIED: axiam/crates/axiam-pki/src/ca.rs:1296].
- **Server leaves:** ECDSA P-256. Extensions:
  - `basicConstraints=CA:FALSE`
  - `keyUsage=critical,digitalSignature`
  - `extendedKeyUsage=serverAuth`
  - SAN per D-04
  - `-days 397`

  One per listener: `caddy` (DOMO_HOST, axiam.DOMO_HOST, LAN IP, localhost, 127.0.0.1), `axiam-server` (axiam-server, DOMO_HOST, LAN IP, localhost, 127.0.0.1), `rabbitmq` (rabbitmq, DOMO_HOST, LAN IP), `postgres`, `device-twin`.
- **Infra client leaf:** `axiam-amqp-client`, P-256, `extendedKeyUsage=clientAuth`, root-signed (C-4).
- **The LAN IP is baked into SANs.** If the network changes, re-run `just pki-server-certs`.

### Pattern 5: Broker auth design (resolves D-24/D-26 per C-1/C-2)
- Global `ssl_options`: `verify_peer`, `fail_if_no_peer_cert=true`, `cacertfile=root`, TLS 1.3.
- MQTT: `mqtt.listeners.tcp = none`, `mqtt.listeners.ssl.default = 8883`, `mqtt.allow_anonymous = false`, `mqtt.vhost = domo`, `mqtt.ssl_cert_client_id_from = distinguished_name`. Leave **`mqtt.ssl_cert_login` off**: "Clients **must not** supply username and password" under cert login [CITED: rabbitmq.com/docs/mqtt], and CONNECT username/password take priority anyway (`creds/3`: `{true, true, _} -> %% Username and password take priority`) [VERIFIED: rabbit_mqtt_processor.erl v4.3.x].
- Username must not contain `:`. The MQTT plugin splits `vhost:user` at the last colon (`get_vhost_username`) [VERIFIED: same file]. SA UUIDs are safe.
- Leaf subject: AXIAM builds every leaf with `CommonName` only (`params.distinguished_name.push(DnType::CommonName, subject)`) and takes it from the CSR's CN [VERIFIED: axiam/crates/axiam-pki/src/cert.rs:574, 713-719]. The broker-computed DN is therefore `CN=<sa-uuid>` (exact rendering [ASSUMED]; the positive smoke case proves it).
- Backend contract [VERIFIED: rabbit_auth_backend_http.erl v4.3.x]:
  - Requests are **POST** `application/x-www-form-urlencoded` when `auth_http.http_method = post`. With GET the JWT lands in the URL, which RabbitMQ debug-logs ("Intentionally logs the full URL including credentials").
  - The response must be HTTP 2xx with body `allow` / `deny` / `deny <reason>` / `allow <tags>`. Any other status is an error.
  - Parameters per endpoint:
    - user: `username`, `vhost`, `client_id`, `password`
    - vhost: `username`, `vhost`, `ip`, `tags`
    - resource: `username`, `vhost`, `resource`, `name`, `permission`, `tags`
    - topic: `username`, `vhost`, `resource`, `name`, `permission`, `tags`, `routing_key`, `variable_map.username`, `variable_map.vhost`, `variable_map.client_id`
- Resource names to allow per device [VERIFIED: rabbit_mqtt_util.erl v4.3.x `queue_name_bin`]:
  - exchange `amq.topic` (read/write)
  - queues `mqtt-subscription-<client_id>qos0`, `mqtt-subscription-<client_id>qos1`
  - `mqtt-will-<client_id>`
- Topic check: MQTT `/` → AMQP `.`, `+` → `*`, `#` → `#` [CITED: rabbitmq.com/docs/mqtt]. Allow `routing_key` only under `domo.<tenant_slug>.<sa-uuid>.` (reads may end in `#`). Slugs must contain no `.`.
- The Twin keeps an in-memory `username → {tenant_id, tenant_slug, exp}` cache, filled on a successful `/user` call and used by `/vhost`, `/resource` and `/topic`. On a miss it answers deny, so a Twin restart forces devices to reconnect. Acceptable for Phase 1; Phase 3 revisits.
- **Topic `{device}` segment = device SA UUID** in Phase 1. It's self-contained and needs no DB. Confirm with the user (Open Question 2).

### Pattern 6: Caddy (D-30/D-31)
- Global `auto_https off`. Every site block uses `tls /certs/caddy.pem /certs/caddy.key { protocols tls1.3 }` [CITED: caddyserver.com/docs/caddyfile/directives/tls].
- Upstream TLS to AXIAM: `reverse_proxy https://axiam-server:8090 { transport http { tls_trust_pool file /pki/root.pem; tls_server_name axiam-server } header_up -X-Client-Certificate }` [CITED: caddyserver.com/docs/caddyfile/directives/reverse_proxy].
- Order: `/api/mgmt/*`, `/api/twin/*` **before** `/api/v1/*`. Use `handle`, not `handle_path`, for AXIAM routes; they must stay unprefixed.
- `axiam.{DOMO_HOST}` → `reverse_proxy axiam-frontend:8080` for everything (its nginx proxies the API paths itself), **or** split `/api*`,`/oauth2/*`,`/.well-known/*` directly to axiam-server (one proxy hop → `TRUSTED_HOPS=1` stays correct). Recommended: the split, for consistent hop counting.

### Pattern 7: Trust export & import (PKI-05, D-14)
- `just export-trust` writes:
  - `dist/trust/domo-root.pem` and `.der` (`openssl x509 -outform der`)
  - `domo-root.sha256` (`openssl x509 -noout -fingerprint -sha256`)
- ArchLinux: `sudo trust anchor --store domo-root.pem`. On Arch, NSS's built-in trust module is p11-kit, so Chrome and Firefox see it [ASSUMED].
- Debian / Raspberry Pi OS: `sudo cp domo-root.pem /usr/local/share/ca-certificates/domo-root.crt && sudo update-ca-certificates`.
- Chrome/Chromium on Linux (NSS user DB): `certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n "Domo Demo Root" -i domo-root.pem`.
- Firefox: import via Settings → Certificates → Authorities (tick "identify websites"), or `certutil -d sql:<profile> -A -t "C,," …`. Set `security.enterprise_roots.enabled` where supported [ASSUMED].

### Pattern 8: Multi-arch Rust images
- One `Dockerfile.rust` builds all workspace binaries in a `--platform=$BUILDPLATFORM` stage with `cargo-zigbuild --target aarch64-unknown-linux-gnu` (and native x86_64), then copies them into `gcr.io/distroless/cc-debian12:nonroot` per `$TARGETPLATFORM` [ASSUMED toolchain; AXIAM's own Dockerfile builds per-platform natively rather than cross-compiling (FROM rust:1.97-bookworm … AS builder, no `$BUILDPLATFORM`) — [VERIFIED: axiam/docker/Dockerfile.server:16,173]].
- Fallback: build natively on the Pi in Phase 6.
- The images are `domo-twin` and `domo-tools` (domo-bootstrap + domo-probe).

### Anti-Patterns to Avoid
- **Using the SDK org client for tenant-scoped routes.** It silently acts on the reserved `organization` tenant. Resources, roles and SAs get created in the wrong tenant with no error.
- **GET for `auth_http`.** It puts JWTs in URLs and broker debug logs.
- **Enabling `mqtt.ssl_cert_login`.** The username/password then have to be absent, which breaks the JWT flow.
- **Mounting `.secrets` files (0600, host uid) straight into non-root containers.** axiam 65532, postgres 70, rabbitmq's own uid: the read fails. Use the `tls-init` copy pattern (P-6).
- **Treating the manifest as covering groups/resources.** It can't express metadata or resource-scoped group roles.
- **Deny rules anywhere above apartment/device.** This comes from the project research and is out of scope here, but the catalog must not introduce any.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JWT verification in the Twin | custom JWKS fetch + EdDSA verify | `axiam_sdk::token::JwksVerifier` | Pins `alg` to EdDSA before key lookup, enforces tenant, aud, exp and a 60 s skew [VERIFIED: jwks.rs:770-795] |
| Idempotent catalog creation | exists-check loops | `management().manifest().apply()` then `plan()` = all `NoChange` | Contract rule 6 idempotence; ordered, natural-key reconciliation |
| X.509/CSR building | DER by hand | `rcgen` (CSR, Ed25519), openssl CLI (root/server certs) | Extension encoding mistakes are silent and fatal in browsers |
| TLS | anything custom | rustls 0.23 / Caddy / Erlang ssl | — |
| MQTT client | raw sockets | `rumqttc` | Keepalive, QoS, reconnection |
| Form parsing of backend requests | string splitting | `actix_web::web::Form<T>` with `serde` | Strict schema, rejects unknown/missing fields (V5 input validation) |
| Resource-tree walks | recursive list calls | `resources().list_ancestors(id)` | Server-side |

**Key insight:** the risky parts here are PKI extensions, TLS chains and authz semantics. The libraries and AXIAM already encode the edge cases. What must be custom is *only* the tenant/namespace policy in the Twin's four endpoints, and that should be a small, pure, unit-tested function.

## Common Pitfalls

### P-1: Acting tenant silently wrong
**What goes wrong:** An org-level SDK client creates resources, roles and SAs in the reserved `organization` tenant instead of `lakeside`.
**Why:** The SDK never sends `X-Axiam-Tenant`. Tenant-scoped routes use the principal's tenant.
**How to avoid:** Use a per-tenant admin client for all tenant-scoped work. In `verify`, assert each created object's `tenant_id` equals the target tenant.
**Warning signs:** Objects visible in the console under "Organization" rather than under the tenant.

### P-2: Management API rejects service-account tokens
**What goes wrong:** 401 `audience mismatch — this route requires axiam:user audience` when an SA (mTLS or client-credentials) calls `/api/v1/resources` and similar routes.
**How to avoid:** Management uses user principals. SAs are only for authz checks and device identity. Phase 2 input (C-5).

### P-3: Setup token lost forever
**What goes wrong:** The token is minted and logged **once per database**. Later boots are no-ops: "setup token already exists … nothing is minted or re-logged" [VERIFIED: axiam/crates/axiam-db/src/seeder.rs:145-165]. If the axiam-server container is *recreated* (logs gone) before org-bootstrap succeeds, the only recovery is wiping the SurrealDB volume.
**How to avoid:**
- Scrape `docker compose logs --no-color axiam-server` (full history, not `--since`) with the regex `"setup_token":"([A-Za-z0-9_-]+)"`. Logs are JSON: `tracing_subscriber::fmt()...json()` [VERIFIED: axiam-server/src/main.rs:218-221]; the token is 32 random bytes base64url (43 chars).
- Persist the token to `.secrets/state/setup-token` (0600) the moment it is seen.
- On the `org-bootstrap` retry path, first probe `POST /api/v1/auth/login` with the saved super-admin creds (200 ⇒ already bootstrapped). If there is no token and no working login, print: "reset required: `just demo-reset`".
- Keep `RUST_LOG` at info or finer for `axiam`, or the line is suppressed.

### P-4: axiam-server healthcheck breaks under TLS
**What goes wrong:** The shipped `axiam-server healthcheck` probes `AXIAM_HEALTHCHECK_URL`, default `http://127.0.0.1:8090/health`, with a reqwest built on `rustls-tls` (webpki roots) [VERIFIED: main.rs:189-196; axiam/Cargo.toml:135]. Plain HTTP fails against a TLS listener, and HTTPS won't trust the private root. SSL_CERT_FILE is not honored by webpki roots [ASSUMED].
**How to avoid:**
- Drop the compose healthcheck for axiam-server. `just` waits on `curl --cacert root.pem https://127.0.0.1:8090/health`, and dependents use `service_started` plus the stage gate.
- Or run a tiny healthcheck sidecar.
- Log as a finding.

### P-5: Wrong key/cert format on import
**What goes wrong:** An import of a PKCS#1 (`BEGIN RSA PRIVATE KEY`) key, or a root with `pathlen:0`, fails. With `pathlen:0` the import succeeds but every tenant CA chain is invalid to Erlang, browsers and webpki.
**How to avoid:** Use `openssl genpkey` (PKCS#8) and no pathlen on the root. The `verify` stage runs `openssl verify -CAfile root.pem -untrusted tenantCA.pem leaf.pem`.

### P-6: Secrets permissions vs container users
**What goes wrong:** Files in `.secrets` are 0600 and owned by the host uid (D-13). axiam-server runs as `65532:65532` [VERIFIED: image config `"User": "65532:65532"`], and postgres/rabbitmq/distroless use other uids, so the TLS key read fails. Compose (non-swarm) ignores `secrets.*.uid/mode`.
**How to avoid:**
- Add a `tls-init` one-shot (busybox) service mirroring AXIAM's `surrealdb-init` pattern. It mounts `.secrets/pki` read-only and copies each service's cert/key into a dedicated named volume with the right owner and 0600/0640.
- Services mount their volume read-only.
- Reset wipes these volumes.
- Postgres requires the key to be owned by the postgres uid with 0600, or root with 0640 [ASSUMED].

### P-7: Broker-wide TLS change breaks AXIAM's AMQP
**What goes wrong:** Turning on `fail_if_no_peer_cert` for MQTT kills AXIAM's AMQPS connection (C-1). AXIAM may fail to start, or lose events.
**How to avoid:** Issue the `axiam-amqp-client` cert in the `pki` stage and set both `AXIAM__AMQP__TLS__CLIENT_*` vars from day one. The half-set combination "fails closed" [VERIFIED: axiam-amqp/src/config.rs:63-68].

### P-8: Duplicate resources on re-run
**What goes wrong:** Re-running the smoke/portfolio creation creates a second `portfolio`/`site:smoke-…` resource. The schema has unique indexes for role `(tenant_id, name)`, permission `(tenant_id, action)` and group `(tenant_id, name)` [VERIFIED: axiam-db/src/schema.rs:466-467, 480-481, 542-543]. No resource uniqueness index turned up in a grep of the resource table definitions [ASSUMED absence].
**How to avoid:** Resolve by name under the parent before every create. `verify` asserts exactly one child per name.

### P-9: PKI encryption key variable name
**What goes wrong:** CA import/generation is refused ("no CA signing key custodian configured; CA generation and import will be refused until AXIAM__PKI__ENCRYPTION_KEY …" [VERIFIED: main.rs:1108-1112]). The key is read via the secret provider's `read_key("pki_encryption_key")`, and the `env` provider maps that to `AXIAM__AUTH__PKI_ENCRYPTION_KEY` [VERIFIED: secrets.rs:82-86; core/secrets.rs:74, 167-174], while docs and log text say `AXIAM__PKI__ENCRYPTION_KEY`.
**How to avoid:** Set **both** to the same 64-hex value. The `axiam-up` stage asserts the startup log contains `"CA signing key custody resolved"`. Record whichever name the running image honors as a finding.

### P-10: rustls crypto-provider ambiguity and C builds
**What goes wrong:**
- The dependency graph enables both aws-lc-rs (reqwest 0.13 `rustls`, rumqttc `use-rustls` → `tokio-rustls/default`) and ring (tonic `tls-ring`, rcgen). `rustls::ClientConfig::builder()` can then panic with "no process-level CryptoProvider" [ASSUMED].
- aws-lc-sys compiles C, which needs cc/cmake when cross-compiling.

**How to avoid:** Call `rustls::crypto::aws_lc_rs::default_provider().install_default()` first thing in every `main`. Make the build image include clang/cmake (or zig).

### P-11: Leaf issuance under the wrong tenant's CA
**What goes wrong:** `prepare_leaf_issuance` checks that the issuer CA belongs to the **organization**, is active and in-window. From code reading, it does not check that a *tenant signing CA* belongs to the acting tenant [VERIFIED partial read: axiam-pki/src/cert.rs prepare_leaf_issuance comments "Scoped to the organization"]. A Lakeside admin could issue a Lakeside cert under Summit's CA, or directly under the root.
**How to avoid:** `domo-bootstrap` always passes the acting tenant's own CA. `just smoke` adds a negative assertion (expect 4xx). If the server returns 201, log the finding.

### P-12: mDNS names
**What goes wrong:** `domo.local` resolves on the host but not for Chromium on other machines, or `axiam.domo.local` is not published.
**How to avoid:** Preflight checks `getent hosts $DOMO_HOST axiam.$DOMO_HOST` and falls back to `/etc/hosts` guidance. Publish the alias with `avahi-publish -a -R axiam.<host> <ip>` as a user service [ASSUMED]. `domo.local` currently doesn't resolve on this XPS (probe 2026-09-19).

### P-13: Device token lifetime makes the "expired JWT" test slow
**What goes wrong:** Device tokens use the global `access_token_lifetime_secs` (`expires_in: state.auth_config.access_token_lifetime_secs` [VERIFIED: handlers/auth.rs:893-896]), 900 s by default. There's no per-test override.
**How to avoid:** Cover expiry in Twin unit tests (a wiremock JWKS plus a locally signed Ed25519 token with `exp` in the past). The smoke covers garbage, bad-signature and wrong-tenant tokens. An optional `just smoke-slow` waits past `exp`.

## Code Examples

> Values in these examples are verified where the table rows above quote them. Anything else (ports, paths, file names) is a recommendation [ASSUMED].

### 1. Offline PKI (openssl)
```bash
# Root (once; D-10) — PKCS#8 RSA-4096
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out .secrets/pki/root.key
openssl req -x509 -new -key .secrets/pki/root.key -sha256 -days 3650 \
  -subj "/CN=Domo Demo Root CA/O=AXIAM Domo Demo" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign" \
  -addext "subjectKeyIdentifier=hash" -out .secrets/pki/root.pem
# Server leaf (every reset) — ECDSA P-256, ≤397 days
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out caddy.key
openssl req -new -key caddy.key -subj "/CN=${DOMO_HOST}" -out caddy.csr
openssl x509 -req -in caddy.csr -CA .secrets/pki/root.pem -CAkey .secrets/pki/root.key \
  -days 397 -sha256 -copy_extensions none -extfile <(printf '%s\n' \
  "basicConstraints=CA:FALSE" "keyUsage=critical,digitalSignature" \
  "extendedKeyUsage=serverAuth" \
  "subjectAltName=DNS:${DOMO_HOST},DNS:axiam.${DOMO_HOST},DNS:localhost,IP:127.0.0.1,IP:${LAN_IP}") \
  -out caddy.pem
```

### 2. Org-level SDK work in domo-bootstrap
```rust
// Sources: axiam-rust-sdk src/client.rs (builder), ops/ca_certificates.rs, models.rs (verified above)
rustls::crypto::aws_lc_rs::default_provider().install_default().ok();
let org = AxiamClient::builder()
    .base_url("https://axiam-server:8090")?
    .org_slug("domo")                     // [ASSUMED] org slug choice
    .tenant_slug("organization")          // CONTRACT §5.2.1 reserved slug
    .with_custom_ca(&root_pem)?
    .build()?;
let login = org.login(&admin_email, &admin_pw).await?;
let org_id = login.org_id.expect("org-level login returns org_id");
let root = org.ca_certificates().in_org(org_id).import_ca(&ImportCaCertificateRequest {
    private_key_pem: Some(Sensitive::new(root_key_pem)),
    public_cert_pem: root_cert_pem.clone(),
}).await?;
org.ca_certificates().in_org(org_id)
    .set_mtls_trust_anchor(root.id, &SetMtlsTrustAnchor { enabled: true }).await?;
let ca = org.ca_certificates().in_org(org_id).generate_signing_ca(tenant_id,
    &CreateIntermediateCaRequest { key_algorithm: KeyAlgorithm::Ed25519,
        parent_ca_id: root.id, subject: "CN=Lakeside Signing CA".into(), validity_days: 1825 }).await?;
// ca.private_key_pem is returned ONCE (§27.5) — discard deliberately (AXIAM keeps custody), never log.
```

### 3. `deploy/rabbitmq/30-mqtt.conf` (sysctl format)
```ini
# Broker-wide: also applies to AXIAM's AMQPS listener (C-1) — AXIAM presents its client cert (C-4)
ssl_options.cacertfile = /etc/rabbitmq/tls/root.pem
ssl_options.verify = verify_peer
ssl_options.fail_if_no_peer_cert = true
ssl_options.versions.1 = tlsv1.3
ssl_options.depth = 3                       # [ASSUMED] allow root→tenantCA→leaf
mqtt.listeners.tcp = none
mqtt.listeners.ssl.default = 8883
mqtt.allow_anonymous = false
mqtt.vhost = domo
mqtt.exchange = amq.topic
mqtt.ssl_cert_client_id_from = distinguished_name
auth_backends.1 = internal
auth_backends.2 = http                      # [ASSUMED alias]; fallback: rabbit_auth_backend_http
auth_http.http_method = post
auth_http.user_path     = https://device-twin:8443/rmq/user
auth_http.vhost_path    = https://device-twin:8443/rmq/vhost
auth_http.resource_path = https://device-twin:8443/rmq/resource
auth_http.topic_path    = https://device-twin:8443/rmq/topic
auth_http.request_timeout = 5000
auth_http.ssl_options.cacertfile = /etc/rabbitmq/tls/root.pem
auth_http.ssl_options.verify = verify_peer
```
Load it after AXIAM's `20-tls.conf` and override the `ssl_options.verify`/`fail_if_no_peer_cert` values that file sets (`verify_none`/`false` [VERIFIED: axiam/docker/rabbitmq-tls.conf]). Files in `conf.d` load in lexical order, so the later value wins [ASSUMED]; `verify` confirms it with `rabbitmq-diagnostics environment`. `enabled_plugins`: `[rabbitmq_management,rabbitmq_prometheus,rabbitmq_mqtt,rabbitmq_auth_backend_http].` The image default contents are [ASSUMED], so keep management.

### 4. Caddyfile skeleton
```caddyfile
{
	auto_https off
}
{$DOMO_HOST}:443 {
	tls /certs/caddy.pem /certs/caddy.key {
		protocols tls1.3
	}
	handle /api/twin/* { reverse_proxy https://device-twin:8443 { transport http { tls_trust_pool file /pki/root.pem } } }
	@axiam path /api/v1/* /oauth2/* /.well-known/*
	handle @axiam {
		reverse_proxy https://axiam-server:8090 {
			header_up -X-Client-Certificate
			transport http { tls_trust_pool file /pki/root.pem
				tls_server_name axiam-server }
		}
	}
	handle { root * /srv/landing
		file_server }
}
axiam.{$DOMO_HOST}:443 {
	tls /certs/caddy.pem /certs/caddy.key { protocols tls1.3 }
	@api path /api/* /oauth2/* /.well-known/*
	handle @api { reverse_proxy https://axiam-server:8090 { header_up -X-Client-Certificate
		transport http { tls_trust_pool file /pki/root.pem
			tls_server_name axiam-server } } }
	handle { reverse_proxy axiam-frontend:8080 }
}
```

### 5. domo-probe: device login (hand-rolled) + MQTT connect
```rust
// POST /api/v1/auth/device — response {access_token, token_type:"Bearer", expires_in} [VERIFIED handlers/auth.rs:854-897]
let identity = reqwest::Identity::from_pem(&[leaf_pem, tenant_ca_pem, key_pem].concat())?; // chain + PKCS#8
let http = reqwest::Client::builder().use_rustls_tls()
    .add_root_certificate(reqwest::Certificate::from_pem(&root_pem)?)
    .identity(identity).build()?;
let tok: DeviceAuth = http.post(format!("{axiam}/api/v1/auth/device")).send().await?
    .error_for_status()?.json().await?;
// MQTT: client_id MUST equal the cert DN; username = SA uuid; password = JWT
let mut opts = rumqttc::MqttOptions::new(format!("CN={sa_id}"), host, 8883);
opts.set_credentials(sa_id.to_string(), tok.access_token);
opts.set_transport(rumqttc::Transport::tls_with_config(
    rumqttc::TlsConfiguration::Rustls(Arc::new(client_cfg_with_chain_and_key))));
```

### 6. Twin `/rmq/user` decision (pure core, unit-testable)
```rust
#[derive(serde::Deserialize)]
struct UserReq { username: String, password: String, vhost: String, client_id: Option<String> }
async fn user(form: web::Form<UserReq>, st: web::Data<State>) -> impl Responder {
    let r = form.into_inner();
    let deny = || HttpResponse::Ok().content_type("text/plain").body("deny");
    if r.vhost != "domo" { return deny(); }
    if r.client_id.as_deref() != Some(&format!("CN={}", r.username)) { return deny(); } // cert binding
    let Some(tenant) = st.tenant_for_unverified(&r.password) else { return deny(); };   // pick verifier
    match st.verifier(tenant).verify(&r.password).await {       // JwksVerifier: sig, exp, tenant, aud
        Ok(c) if c.sub == r.username => { st.cache(r.username, tenant, c.exp); HttpResponse::Ok().body("allow") }
        _ => deny(),                                              // never log the password
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| AXIAM: RSA CAs import-only | RSA-4096 generation via the `rsa` crate + rcgen signing | 2026-08-25 (`bacc92a1d`, in all v1.0.0-beta tags) | D-11 rationale outdated; the tenant CA algorithm is free |
| Device token `aud: axiam:user` | `aud: axiam:m2m` (both SA paths) | contract "residual-1" flip (CONTRACT.md ~1960) | SAs can't call management/user routes (C-5) |
| Two-stage TLS bootstrap (self-signed → CA-issued) | Offline root first; all server certs final at first boot | this design | No SIGHUP/restart choreography |
| RabbitMQ 4.2.x (project STACK research) | `4-management-alpine` = 4.3.6 today | 2026 | Pin the minor; MQTT/HTTP-backend logic unchanged between 4.2 and 4.3 [VERIFIED diff of `creds/3`, `extract_client_id_from_certificate`] |

**Deprecated/outdated:**
- `tenant_name`/`tenant_slug` in `/admin/bootstrap`: `#[deprecated(note = "bootstrap no longer creates a tenant; this value is ignored")]` [VERIFIED: bootstrap.rs:74-80].
- `docs/pki/README.md` statements "Device-type certificates do not use this bind step" and "`POST .../ca-certificates` with `Rsa4096` fails". Both contradict the code.

## Dogfooding Findings to Add in Phase 1 (new, beyond DOC-05's seven)

| Draft ID | Component | Finding | Evidence tier |
|---|---|---|---|
| DF-008 | axiam-rust-sdk | No acting-tenant (`X-Axiam-Tenant`) support, so an org-level principal cannot administer tenant-scoped resources via the SDK | VERIFIED (grep + server const) |
| DF-009 | axiam-rust-sdk | No `/api/v1/auth/device` operation despite §6.1 (C++ has `authenticate_device()`) | VERIFIED |
| DF-010 | axiam-rust-sdk | `/admin/bootstrap` excluded by §27.0, so first-run is hand-rolled (by design; log for completeness) | VERIFIED |
| DF-011 | axiam-rust-sdk | Manifest lacks resource `metadata`, resource-scoped group→role bindings and `service_accounts` (the contract lists them) | VERIFIED (spec.rs) |
| DF-012 | axiam-rust-sdk | gRPC client lacks `validate_token`/`introspect_token` wrappers (raw stubs only) | VERIFIED |
| DF-013 | axiam-server | REST management routes reject `axiam:m2m`, so service accounts can't manage | VERIFIED |
| DF-014 | axiam-server | Device mTLS tokens carry no `cnf`/`x5t#S256`, so the broker can't bind a token to a cert (the DN-client_id workaround applies) | VERIFIED (`AccessTokenSpec::service_account` sets none) |
| DF-015 | axiam docs | pki/README says RSA generation fails; code generates RSA-4096 | VERIFIED |
| DF-016 | axiam-server | The healthcheck subcommand can't probe a TLS listener with a private CA | VERIFIED code, runtime ASSUMED |
| DF-017 | axiam-server | Leaf issuance doesn't bind a tenant signing CA to its tenant | Code reading; confirm in smoke |
| DF-018 | axiam-server/docs | PKI encryption key env name mismatch (`AXIAM__AUTH__PKI_ENCRYPTION_KEY` vs `AXIAM__PKI__ENCRYPTION_KEY`) | Confirm at runtime |
| DF-019 | axiam-server | The setup token is logged once per DB; a container recreate before bootstrap requires a DB wipe | VERIFIED |
| DF-020 | demo infra (not AXIAM) | AXIAM AMQP client cert offline-root-signed, an exception to PKI-03 (C-4) | Design note |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | RabbitMQ renders a CN-only subject as exactly `CN=<value>` for `ssl_cert_client_id_from = distinguished_name` | Pattern 5 | Positive MQTT case fails; fix the client_id format (the smoke reveals it) |
| A2 | Erlang/OTP TLS 1.3 accepts an Ed25519 leaf under an Ed25519 tenant CA under an RSA root, with no EKU | D-27, C-3 | Switch the tenant CA to RSA-4096 (one field) or record a finding |
| A3 | `auth_backends.2 = http` alias works in sysctl format | Code Ex. 3 | Use the module name `rabbit_auth_backend_http` |
| A4 | A later conf.d file overrides `ssl_options.verify` set in `20-tls.conf` | Code Ex. 3 | Replace AXIAM's fragment with our own merged copy |
| A5 | reqwest `rustls-tls` (0.12) ignores SSL_CERT_FILE (webpki roots) | P-4 | The healthcheck might work with SSL_CERT_FILE; harmless |
| A6 | Both aws-lc-rs and ring providers in graph ⇒ `ClientConfig::builder()` panics without `install_default` | P-10 | None; installing the default is harmless either way |
| A7 | Postgres key-file ownership rules; Arch p11-kit NSS behavior; Firefox enterprise roots on Linux | P-6, Pattern 7 | Trust-doc steps need adjustment; Firefox/Chrome UAT catches it |
| A8 | cargo-zigbuild cross-build works for ring/aws-lc-sys to aarch64-gnu | Pattern 8 | Fall back to native Pi build or QEMU |
| A9 | `ssl_options.depth = 3` is needed/sufficient | Code Ex. 3 | Chain-length rejection at TLS |
| A10 | No uniqueness index on resource names | P-8 | None; idempotent lookups are needed anyway |
| A11 | The default `enabled_plugins` of the management image includes management (+prometheus) | Code Ex. 3 | Management API missing if omitted; include it explicitly |
| A12 | Org slug `domo`, ports 8443 (Twin), 8090 (AXIAM), file paths | Code examples | Cosmetic |

## Open Questions (RESOLVED)

All five were answered before planning closed. Each carries its resolution inline; none is outstanding, and nothing below blocks the phase goal.

1. **C-1..C-7 sign-off** (root as broker CA, DN-bound client_id, Ed25519 tenant CA, infra AMQP cert exception, Phase 2 management principal, hand-rolled device login, JwksVerifier).
   - Recommendation at research time: one `checkpoint:decision` task at the start of the phase, presenting the table above.
   - **RESOLVED 2026-09-19 — by the user, in `01-CONTEXT.md`.** The corrections were presented and signed off directly rather than deferred to a checkpoint, and the amendments are recorded in CONTEXT.md as authoritative: D-11 ← C-3 (Ed25519 tenant signing CA), D-24 ← C-2 (client identifier bound to the certificate's distinguished name), D-26 ← C-1 (the organization root, not a tenant CA, is the broker's trust store), D-20 ← C-5 (the Phase 2 management principal), and the new D-37 ← C-4 (AXIAM's own AMQP client certificate is offline root-signed, the documented PKI-03 exception). C-6 (hand-rolled device login) and C-7 (`JwksVerifier` construction) are implemented as written in plans 01-01 and 01-05. No `checkpoint:decision` remains in any plan, deliberately: re-opening a decision the user already locked would contradict the phase context.
2. **Topic `{device}` segment.**
   - What we know: SA-UUID-based topics make the backend self-contained.
   - What's unclear at research time: readable slugs (`domo/lakeside/smoke-light-1/…`) need a registry lookup, which is Phase 3's DB.
   - **RESOLVED — D-23 as amended.** Phase 1 uses `domo/{tenant_slug}/{sa_uuid}/…`, the service-account UUID form. Implemented in `crates/domo-common/src/topic.rs` (plan 01-01), hardened and unit-tested in plan 01-05, and asserted end to end by the namespace-escape cases in plan 01-06. Readable slugs are explicitly a Phase 3 question to re-open with the user; Phase 4's simulators will inherit whatever Phase 3 settles on.
3. **Where tenant-admin credentials live and who uses them later.**
   - **RESOLVED — D-37 plus plan 01-04 Task 2.** The credentials are persisted to `.secrets/axiam/tenant-admin-<slug>.json` at mode 0600, provisioned by `domo-bootstrap` through four hand-rolled calls carrying the acting-tenant header (the SDK sends none — DF-008). Whether the Management Platform reuses them is C-5's Phase 2 question and is out of this phase's scope.
4. **AXIAM gRPC TLS config** (`50051`, for Phase 3 CheckAccess).
   - Not researched deeply. Only `AXIAM__SERVER__TLS__*` is documented.
   - **RESOLVED — deferred to Phase 3 by decision, not left open.** Port 50051 stays unpublished for the whole of Phase 1; no plan publishes it and no plan makes a gRPC call. Phase 3 verifies the TLS configuration when it first needs `CheckAccess`.
5. **Firefox on Raspberry Pi OS trust path.**
   - **RESOLVED — covered by two `human-check` blocks**, consistent with `human_verify_mode: end-of-phase`: plan 01-02 Task 3 (confirm Chrome and Firefox show no warning once the root is trusted, and record which documented path was actually required, since A7 marks the p11-kit and Firefox behaviour as assumed) and plan 01-07 Task 3 (the same confirmation as part of walking `docs/setup.md` on a machine that has never run the demo). `docs/trust.md` documents both the OS trust-store path and the explicit `certutil` path so either outcome is already written down.

## Environment Availability

Probed 2026-09-19 on the XPS:

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker Engine | everything | ✓ | 29.8.1 (x86_64) | — |
| Docker Compose | `just up` | ✓ | 5.5.1 (v2+ CLI plugin line) | — |
| buildx | multi-arch images | ✓ | 0.37.1 | — |
| just | orchestration | ✓ | 1.58.0 | — |
| openssl | offline PKI | ✓ | 3.6.4 | — |
| cargo/rustc | local builds | ✓ | 1.98.1 (≥ MSRV 1.88) | build in Docker |
| cmake | aws-lc-sys (maybe) | ✓ | — | — |
| cargo-zigbuild / zig / cross | arm64 cross-build | ✗ | — | install in the builder image, or build natively on the Pi |
| avahi (resolve/publish) | mDNS `domo.local` | ✓ (tools) | 0.9-rc5 | `/etc/hosts` entries |
| `domo.local` resolution | D-35 preflight | ✗ (unresolved now) | — | publish via avahi or hosts |
| Ports 443/8883/8090 | listeners | free | — | — |
| google-chrome-stable / firefox / certutil / trust | browser UAT, trust import | ✓ | — | chromium not installed; Chrome suffices |
| jq, curl | smoke helpers | ✓ | 1.8.2 / 8.22.0 | — |
| Disk `/home` | builds | 35 GB free | — | Plans must `cargo clean` after builds; no parallel heavy builds (<25 GB rule borderline) |
| LAN IP | SANs | 192.168.144.20 (detected) | — | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** zig/cargo-zigbuild (builder image), `domo.local` name (avahi/hosts).

## Validation Architecture

`workflow.nyquist_validation` is `false` in config, but the orchestrator requested this section explicitly.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust unit/integration); `just smoke` (stack-level, driven by `domo-bootstrap smoke` + `domo-probe`); shell assertions in `just verify` |
| Config file | none yet (Wave 0) |
| Quick run command | `cargo test -p device-twin -p domo-common` |
| Full suite command | `cargo test --workspace && just demo-reset && just smoke` |

### Success Criteria → Automated Verification
| SC / Req | Behavior | Test Type | Automated Command | File Exists? |
|---|---|---|---|---|
| SC1 / PLAT-01 | One command brings AXIAM, Postgres, Caddy (+ Twin) up healthy | stack | `just up && docker compose -f deploy/compose.yml ps --format json \| jq -e 'all(.[]; .State=="running")'` plus `curl --cacert dist/trust/domo-root.pem https://$DOMO_HOST/` | ❌ Wave 0 |
| SC1 / PLAT-05 | Reset idempotent + resumable | stack | `just demo-reset && just demo-reset` (second run all ✓). Interruption test: `DOMO_FAIL_AT=catalog just demo-reset; just up` → resumes at `catalog`, then `just verify` | ❌ |
| SC1 / PLAT-05 | Catalog idempotence | integration | `domo-bootstrap verify catalog`: manifest `plan()` returns all `NoChange` after `apply()` | ❌ |
| SC1 / PLAT-02 | Multi-arch | build | `docker buildx imagetools inspect <each image>` lists `linux/arm64` (AXIAM/third-party verified now; our images after build) | ❌ |
| SC2 / PKI-01 | Root imported with key, trust anchor | integration | `domo-bootstrap verify pki`: `ca_certificates.list` has one CA whose `fingerprint` equals the local root SHA-256, `mtls_trust_anchor == Some(true)`, `key_custody == "database"` | ❌ |
| SC2 / PKI-02 | One signing CA per tenant | integration | `list_signing_cas(tenant)` returns exactly 1 per tenant; `openssl verify -CAfile root.pem tenantCA.pem` | ❌ |
| SC2 / PKI-03 | Client certs from tenant CA | integration | For each SA cert: `issuer_ca_id == tenant CA id` and `openssl verify -CAfile root.pem -untrusted tenantCA.pem leaf.pem` | ❌ |
| SC2 / PKI-04 | SANs, validity, chain | shell | `openssl x509 -ext subjectAltName,extendedKeyUsage -noout`; check `notAfter - notBefore ≤ 397d`; `openssl s_client -connect $DOMO_HOST:443 -servername axiam.$DOMO_HOST -CAfile root.pem -verify_return_error -tls1_3 </dev/null` for each host/IP | ❌ |
| SC2 / PKI-04 | Browsers trust | UAT (+ optional headless) | Chrome headless with a temp NSS DB (`certutil -d sql:$tmp …; google-chrome --headless --user-data-dir=$tmp --dump-dom https://$DOMO_HOST/`) shows no cert error. Firefox: human-verify | ❌ |
| SC2 / PKI-05 | Export | shell | `just export-trust` → files exist; DER round-trips; fingerprint matches the landing page | ❌ |
| SC2 / PKI-06 | Root key never in git/images | shell | `git check-ignore -q .secrets/pki/root.key`; `git ls-files .secrets` empty; `docker save domo-twin domo-tools \| tar -xO \| grep -c "PRIVATE KEY"` == 0; `.dockerignore` contains `.secrets` | ❌ |
| SC3 / PLAT-06 | Single origin | shell | `curl --cacert root.pem https://$DOMO_HOST/.well-known/openid-configuration` 200; `…/oauth2/jwks` 200; `https://axiam.$DOMO_HOST/` returns console HTML; `/api/twin/healthz` 200 | ❌ |
| SC3 / AUTHZ-01 | Tree exists and is queryable | integration | `domo-bootstrap smoke verify`: `list_ancestors(device:smoke-…)` == [apartment, building, site, portfolio] (common nodes checked via `list_children`); every node carries `metadata.domo_id` | ❌ |
| SC3 / AUTHZ-02 | Group pattern | integration | Groups are named `{role}@{type}:{slug}`; `groups.list_roles` shows the scoped role; `check_access_as(user, "device:operate", device)`: deny → add member → allow → remove → deny; probe SA via `device-self@device:…` → `check_access_as(sa, "twin:report", device)` allow | ❌ |
| SC4 / MQTT-01/02 | +/− MQTT matrix | e2e | `domo-probe matrix --tenant lakeside` exits non-zero on any unexpected outcome. Cases: (+) pub/sub own topic; (−) mismatched cert/JWT; (−) garbage JWT; (−) bad-signature JWT; (−) no client cert (TLS failure); (−) Summit-issued cert + Lakeside creds; (−) publish outside own namespace; (−) cross-tenant CA leaf issuance (P-11) | ❌ |
| SC4 / MQTT-02 | Expired JWT + backend rules | unit | `cargo test -p device-twin rmq::` (wiremock JWKS, locally signed tokens: expired, wrong aud, wrong tenant, sub≠username, client_id≠CN=username, topic prefix rules) | ❌ |
| D-32 | Every hand-rolled call has a finding | shell | `grep -o 'hand_rolled::[a-z_]*' -r crates tools \| sort -u` ⊆ IDs referenced in `docs/dogfooding-findings.md` | ❌ |

### Sampling Rate
- **Per task commit:** `cargo test -p <crate touched>`, then `cargo clean -p` or `rm -rf target/debug/incremental` if disk is tight.
- **Per wave merge:** `cargo test --workspace` + `just verify`.
- **Phase gate:** `just demo-reset && just smoke` green twice in a row, plus human browser checks.

### Wave 0 Gaps
- [ ] Root `Cargo.toml` workspace + `crates/domo-common` (test utils: wiremock JWKS, token signer)
- [ ] `services/device-twin/tests/rmq_*.rs`, `tools/domo-probe` matrix subcommand
- [ ] `justfile` recipes `verify`, `smoke`, `export-trust`, `demo-reset`
- [ ] dev-dependency `wiremock` (0.6, the SDK already uses it)

## Security Domain

`security_enforcement: true`, ASVS level 1.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | AXIAM device mTLS → JWT; broker `verify_peer`/`fail_if_no_peer_cert`; `JwksVerifier` (EdDSA pinned, exp/tenant/aud) |
| V3 Session Management | limited | No sessions in Phase 1 services; console uses AXIAM's own cookies on a separate host (D-30) |
| V4 Access Control | yes | Tenant + namespace checks in the Twin backend; group-per-(role, resource); cross-tenant negative tests |
| V5 Input Validation | yes | `web::Form<T>` strict structs for backend params; UUID parsing of username; slug regex `^[a-z0-9-]+$`; catalog TOML schema validation |
| V6 Cryptography | yes | openssl/rcgen/rustls only; RSA-4096 root, P-256 servers, Ed25519 devices; no custom crypto |
| V7 Error/Logging | yes | Never log JWTs, setup tokens (after capture), or `private_key_pem` (`Sensitive<T>`); POST for auth_http |
| V9 Communications | yes | TLS 1.3 only on Caddy, MQTTS, Twin, AMQPS; internal hops verify against the root |
| V14 Configuration | yes | `.secrets` git- and docker-ignored, 0600, per-service copies; pre-commit guard; no keys in images |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Stolen device JWT replayed from another host | Spoofing | Broker requires a client cert, and client_id must equal the cert DN, which must equal `CN=username`, where `username == jwt.sub` |
| Other-tenant admin mints a forged-CN cert | Spoofing/Elevation | Residual; DF-014/DF-017; mitigation needs AXIAM `cnf` or issuer-tenant binding |
| Forged `X-Client-Certificate` via the edge | Spoofing | Forwarded path disabled (default), Caddy strips the header, nginx sets it empty |
| JWT leakage in broker logs/URLs | Info disclosure | `auth_http.http_method = post`; Twin logs no bodies |
| Rogue caller of the Twin auth endpoint | Spoofing | Internal network only (D-05), TLS; optionally require the broker's client cert (mTLS) on `/rmq/*` |
| Setup token in container logs | Info disclosure | Single-use; consumed at org-bootstrap; `.secrets` copy 0600 |
| Root key exfiltration | Info disclosure | Host-only file, a one-shot import payload over TLS; never in images; `.dockerignore` |
| Cross-tenant data via wrong acting tenant | Tampering | Per-tenant admin clients; `verify` asserts `tenant_id` on every created object |

## Sources

### Primary (HIGH confidence, read this session)
- `../axiam-rust-sdk`: `Cargo.toml`, `CONTRACT.md` (§5.2, §6.1, §27), `management-registry.json`, `src/client.rs`, `src/rest/{auth,authz}.rs`, `src/token/jwks.rs`, `src/grpc/client.rs`, `src/management/{models.rs, ops/*, manifest/spec.rs, manifest/mod.rs}`, `examples/device_mtls_provisioning.rs`
- `../axiam`:
  - crates: `axiam-pki/src/{crypto,ca,cert,mtls}.rs`, `axiam-api-rest/src/handlers/{ca_certificates,certificates,auth,bootstrap}.rs`, `axiam-api-rest/src/extractors/{auth,cert_auth}.rs`, `axiam-auth/src/{token,secrets}.rs`, `axiam-core/src/secrets.rs`, `axiam-db/src/{seeder,schema}.rs`, `axiam-db/src/repository/ca_certificate.rs`, `axiam-server/src/{main,tls,mtls_anchors}.rs`, `axiam-amqp/src/{config,connection}.rs`
  - docker: `docker-compose.{prod,dev}.yml`, `rabbitmq-tls.conf`, `nginx.conf.template`, `Dockerfile.server`
  - `scripts/e2e-bootstrap.sh`
  - docs: `pki/README.md`, `deployment/{README,vault}.md`
- RabbitMQ source, branches v4.2.x and v4.3.x: `deps/rabbitmq_mqtt/src/rabbit_mqtt_processor.erl`, `rabbit_mqtt_util.erl`, `priv/schema/rabbitmq_mqtt.schema`, `deps/rabbitmq_auth_backend_http/src/rabbit_auth_backend_http.erl`, `deps/rabbit/src/rabbit_ssl.erl` (raw.githubusercontent.com)
- Registries: crates.io API (versions, features); `docker buildx imagetools inspect` (platforms, image env/config)

### Secondary (MEDIUM)
- https://www.rabbitmq.com/docs/mqtt (v4.3): cert auth, client_id from cert, vhost selection, topic mapping
- https://www.rabbitmq.com/docs/access-control: backend chaining, variable expansion
- https://github.com/rabbitmq/rabbitmq-server/blob/main/deps/rabbitmq_auth_backend_http/README.md: config keys, request params, responses
- https://caddyserver.com/docs/caddyfile/directives/{tls,reverse_proxy}

### Tertiary (LOW, marked ASSUMED)
- OS/browser trust-store behavior, cargo-zigbuild cross-compiling of ring/aws-lc, Postgres key ownership, rustls provider panic semantics

## Metadata

**Confidence breakdown:**
- AXIAM/SDK surface: HIGH. Every call shape was read from source; the gaps were confirmed by grep plus reading.
- Broker design: HIGH for mechanics (Erlang source). MEDIUM for Ed25519 chain acceptance and the exact DN rendering, which the smoke proves.
- Compose/Caddy/PKI ops: MEDIUM. Standard patterns; env-var naming (P-9) and healthcheck (P-4) need runtime confirmation.
- Pitfalls: HIGH for P-1..P-3, P-5, P-7, P-13; MEDIUM for the rest.

**Research date:** 2026-09-19
**Valid until:** ~2026-10-03. AXIAM/SDK ship a beta every few days (beta12→beta16 in 13 days); re-check `X-Axiam-Tenant`/`auth/device` support if `AXIAM_IMAGE_TAG` or `axiam-sdk` moves past beta16.
