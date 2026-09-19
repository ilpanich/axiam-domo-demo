# Phase 1: Foundation - Context

**Gathered:** 2026-09-19
**Status:** Ready for planning

<domain>
## Phase Boundary

The platform boots from a single trust anchor, AXIAM's tenant and resource/role model is established, and one device proves the broker path end to end. Concretely, Phase 1 delivers:

- the compose stack for AXIAM (server, console, SurrealDB, RabbitMQ), PostgreSQL, Caddy, and a Device Twin **skeleton** that only serves the RabbitMQ HTTP auth backend
- the offline RSA-4096 org root, imported into AXIAM (BYOK); one AXIAM-generated signing CA per tenant; SAN server certs signed offline by the root
- `domo-bootstrap` (Rust): org bootstrap, both tenants, the role/permission catalog, portfolio roots, per-(service, tenant) service accounts and their certs, and the `domo` vhost
- `just up`, `just demo-reset`, `just export-trust`, `just smoke`
- a `just smoke` branch in one tenant that proves the resource tree and the group-per-(role, resource) pattern
- `domo-probe` (Rust): one device that gets a cert from a local CSR, logs in to AXIAM over mTLS, and connects to MQTT on the `domo` vhost with cert + JWT, with automated positive and negative checks
- `docs/dogfooding-findings.md`, started now

Requirements: PLAT-01, PLAT-02, PLAT-05, PLAT-06, PKI-01…06, AUTHZ-01, AUTHZ-02, MQTT-01, MQTT-02.

**Not in this phase:**
- the Management Platform, domain CRUD and the full seed (Phase 2)
- the Twin's shadows, commands and SSE (Phase 3)
- simulators (Phase 4)
- portals (Phase 5)
- Pi memory validation (Phase 6)

PLAT-01/02 are satisfied in Phase 1 for the components that exist by then. Later phases add their services to the same compose file and the same `just up`.

</domain>

<decisions>
## Implementation Decisions

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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project scope and decisions
- `DEFINITIONS.md`: the original spec (source of truth).
- `.planning/PROJECT.md`: architecture, authorization model (resource tree, roles table, group indirection), key flows (device provisioning, device connect), TLS bootstrap stages, resource budget. **Note:** D-20 and D-30 above supersede its org-scope Mgmt service account and its `/axiam` route.
- `.planning/REQUIREMENTS.md`: PLAT-01/02/05/06, PKI-01…06, AUTHZ-01/02, MQTT-01/02, DOC-05 (the seed list of known gaps).
- `.planning/ROADMAP.md` § Phase 1: goal and success criteria.
- `.planning/research/SUMMARY.md`: the corrections (Spring Boot 4.1, `has_role` uniqueness, bind-certificate, no SAN/KU/EKU, broker auth, no device refresh token) and the research flags.
- `.planning/research/ARCHITECTURE.md`, `.planning/research/PITFALLS.md`, `.planning/research/STACK.md`: detailed findings behind SUMMARY.md, especially the TLS bootstrap, MQTT-on-RabbitMQ and demo-reset idempotency pitfalls.

### AXIAM server (sibling checkout)
- `../axiam/docker/docker-compose.prod.yml`: the service definitions to copy and adapt (D-02). Covers the image tag handling, TLS env (`AXIAM__SERVER__TLS__*`, client-cert policy), RabbitMQ credentials and volumes, and the SurrealDB init container.
- `../axiam/docker/rabbitmq-tls.conf`: the AMQPS conf.d fragment pattern and the sysctl-vs-erl-args trap (D-26).
- `../axiam/docker/nginx.conf.template`: the console SPA is root-mounted and expects `/api`, `/oauth2/`, `/.well-known` on its origin; see it for the CSP including `wasm-unsafe-eval` (D-30).
- `../axiam/scripts/e2e-bootstrap.sh`: the first-run shape (`POST /api/v1/admin/bootstrap` → org super-admin → tenant creation with `X-Axiam-Tenant`), the setup-token gate and the idempotency pitfalls (D-12).
- `../axiam/docs/pki/README.md`: CA key algorithms (RSA-4096/Ed25519), BYOK import (`POST /api/v1/organizations/{org_id}/ca-certificates/import`), import validation rules, custody, the tenant signing CA tier (D-10, D-11).
- `../axiam/crates/axiam-api-rest/src/extractors/cert_auth.rs`: native mTLS vs the forwarded `X-Client-Certificate` path (D-05).
- `../axiam/crates/axiam-pki/src/mtls.rs` (`DeviceAuthService::authenticate_der`) and `../axiam/crates/axiam-pki/src/cert.rs` (`leaf_params`): why the bind is mandatory and why AXIAM leaves carry no SAN.
- `../axiam/crates/axiam-auth/src/token.rs`: `AccessTokenClaims`, i.e. `sub`, `tenant_id`, `cnf`; relevant to D-24.

### AXIAM SDKs
- `../axiam-rust-sdk/CONTRACT.md`: the management surface, including `tenants.*`, `service_accounts.bind_certificate`, `certificates.sign_csr`, `ca_certificates.{import_ca, generate_signing_ca, list_signing_cas, set_mtls_trust_anchor}` and the `/admin/bootstrap` notes. Also covers sensitive-field handling (`private_key_pem`).
- `../axiam-rust-sdk/Cargo.toml` and `README.md`: feature flags (`rest`, `grpc`, `actix`, `macros`), rustls-only policy, MSRV 1.88, edition 2024.

### Research flags this phase must resolve
- Does the Rust SDK actually **implement** every management call D-08 needs, or does the contract only list them? Anything missing is hand-rolled and logged.
- Can `rabbitmq_auth_backend_http` on RabbitMQ 4.2 receive the client-cert identity alongside the CONNECT username/password for MQTT? This decides D-24.
- Which key algorithm does AXIAM use for a tenant signing CA under an imported RSA root, and does RabbitMQ's Erlang TLS 1.3 accept the resulting Ed25519/RSA-mixed chain for client auth? (D-11, D-27)
- Do mTLS-login tokens carry `cnf`/`x5t#S256`? (D-24)
- Which resource field holds the domain UUID? (D-17)
- Can an AXIAM tenant signing CA be added as an mTLS trust anchor (`set_mtls_trust_anchor`), so that device login works with certs it issues?

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- There's no code in this repo yet. Phase 1 is greenfield.
- `../axiam/docker/*`: compose services, the RabbitMQ TLS fragment and the nginx template to adapt, not import.
- `../axiam/scripts/e2e-bootstrap.sh` and `../axiam/examples/b1-*`, `b6-*` (curl walkthroughs): templates for the order and shape of bootstrap calls, to re-express through the Rust SDK.
- The AXIAM Rust SDK: management REST client, mTLS device login, gRPC token validation, Actix guards. It's the foundation for `domo-bootstrap`, `domo-probe` and the Twin skeleton.

### Established Patterns
- AXIAM's own stack pins released images via `AXIAM_IMAGE_TAG`, keeps secrets in `docker/.secrets/` generated by `just` recipes, uses conf.d fragments for RabbitMQ, and runs multi-stage `--platform=$BUILDPLATFORM` Rust cross-builds (`Dockerfile.server`). Mirror these conventions.
- AXIAM's first-run flow: the org super-admin signs in at org level, then acts per tenant with an `X-Axiam-Tenant` header.

### Integration Points
- AXIAM REST `/api/v1/*` (management, device login) and gRPC (token validation for the Twin's auth backend).
- The RabbitMQ management API (creating the `domo` vhost) and the HTTP auth backend callbacks into the Twin.
- Caddy → axiam-server, axiam-frontend, device-twin; later management-platform and portals.

</code_context>

<specifics>
## Specific Ideas

- Tenant names **Lakeside Residences** (`lakeside`) and **Summit Homes** (`summit`), to make isolation easy to narrate ("Lakeside's manager can't see Summit").
- The `just up` output should read like a checklist, e.g. `✓ pki  root kept (SHA256 3F:A1:…)` / `✗ catalog  409 on role 'installer' (summit)` / `re-run 'just up' to resume at: catalog`.
- The AXIAM console is part of the pitch: evaluators should be able to open `https://axiam.domo.local` and see the real resource tree and groups the demo created.
- Smoke fixtures use a `smoke-` name prefix so they never collide with Phase 2 seed data.

</specifics>

<deferred>
## Deferred Ideas

- **Sim-host trust/credential bundle** (`just export-sim-bundle`): Phase 4, when the simulator hosts exist.
- **Serving the root cert over plain HTTP** for one-click download: considered and rejected for now in favour of `just export-trust` plus docs.
- **Pi RAM/swap preflight warning:** the user left it out of preflight. Pi memory is validated on real hardware in Phase 6 (PLAT-03).
- **Filing findings upstream:** issue bodies are drafted (D-33), but filing stays a manual decision for the user.

</deferred>

---

*Phase: 01-foundation*
*Context gathered: 2026-09-19*
