# AXIAM Domo Demo

## What This Is

A self-contained, locally-run demo of **AXIAM** acting as the single IAM for a multi-tenant IoT home-automation platform.
Property-management companies (one AXIAM tenant each) manage sites, buildings and apartments. Installers, concierges and residents operate a fleet of simulated intercoms, lights and thermostats. Every action by a person or device is authenticated and authorized by AXIAM through its official SDKs.

It serves three audiences at once:
- **Prospects:** a polished, guided storyline.
- **Technical evaluators:** real SDK integration code, policy model and mTLS.
- **Internal dogfooding:** exercises five AXIAM SDKs (Java, Rust, Rust-WASM, C, C++) and records the gaps it finds.

## Core Value

**Every user and device action goes through AXIAM, and the four key demo moments run reliably on a single small machine.**
The four moments are cross-role denial, resident→installer grant/revoke, tenant isolation, and the live device loop.
If anything else slips, these must still work on both reference machines.

## Requirements

### Validated

(None yet — ship to validate)

### Active

**Platform & deployment**
- [ ] The whole platform (AXIAM stack, Management Platform, Device Twin, both portals, reverse proxy, database) runs with Docker Compose on **Dell XPS 9570 / 32 GB / ArchLinux (amd64)** and on **Raspberry Pi 5 / 8 GB / Raspberry Pi OS (arm64)**.
- [ ] Device simulators run on a Linux x86_64 PC. They connect to the platform over the LAN or on the same host.
- [ ] A one-command setup brings up the platform, bootstraps AXIAM (org, tenants, PKI, roles, service accounts) and seeds demo data.
- [ ] A one-command reset (`just demo-reset`) wipes everything, re-seeds, and re-issues certificates.

**Domain & management (Management Platform, Java)**
- [ ] Property managers create, edit and delete sites, buildings and apartments inside their own tenant.
- [ ] Property managers assign installers to sites or apartments, concierges to sites, and residents to apartments.
- [ ] Property managers and installers add, edit and delete site- and building-level devices. Installers and residents do the same for apartment devices.
- [ ] Installers configure devices (settings, as distinct from runtime operation).
- [ ] Every domain change keeps the AXIAM resource tree and role assignments in sync.

**Device operation (Device Twin, Rust)**
- [ ] The Twin keeps a shadow copy of every device's reported and desired state, updated live from devices.
- [ ] Authorized users send commands:
  - lights: on/off, dim, RGB
  - thermostats: on/off, heat/cool mode, target temperature
  - indoor intercom: answer
  - outdoor intercom: unlock
- [ ] Commands reach devices, and the resulting state flows back to the UIs in real time.
- [ ] Every command is authorized by AXIAM before dispatch. Denials show the AXIAM reason (`no_grant` / `denied_by_rule`).

**Authorization rules**
- [ ] Residents operate devices in their own apartment and in the common areas of their building and site.
- [ ] Concierges operate common-area devices of their assigned sites only.
- [ ] Property managers, concierges and installers can never operate apartment devices.
- [ ] A resident grants a specific installer access to **selected devices** of their apartment. Any resident of that apartment can revoke the grant, which takes effect immediately. Grants do not expire.
- [ ] A user of tenant A can neither see nor act on anything in tenant B.

**Intercom flow**
- [ ] A visitor rings an apartment from an outdoor intercom (triggered from the simulator control panel).
- [ ] The indoor intercom rings in the resident app. The resident answers, then unlocks the gate or door.
- [ ] An outdoor intercom may only call indoor intercoms within its own site or building.

**Device simulators**
- [ ] There are three multi-device simulator hosts: **C → lights**, **C++ → intercoms (outdoor + indoor)**, **Rust → thermostats**.
- [ ] Each virtual device has its own AXIAM identity and its own mTLS client certificate.
- [ ] Thermostats simulate room thermal behavior: heating and cooling toward the target, drift toward ambient, and "window open" disturbances.
- [ ] Devices added in the UI are picked up by the right simulator host automatically and come online without restarts.
- [ ] A simulator control panel (web) rings intercoms, triggers disturbances, and takes devices offline or online.

**Portals (React)**
- [ ] **Staff console:** property managers, installers and concierges, with role-based views.
- [ ] **Resident app:** apartment and common-area devices, intercom answer/unlock, installer grant management.
- [ ] Login uses **OPAQUE via the AXIAM WASM SDK**, so the password never leaves the browser.
- [ ] A live **Access decision feed** shows every AXIAM decision (subject, action, resource, result, reason). It is visible in the staff console and toggleable in the resident app.

**Seed data** (minimums from DEFINITIONS.md)
- [ ] 2 tenants, each with 1 site. Each site has 2 buildings, and each building has 4 apartments.
- [ ] Per tenant: at least 1 property manager and at least 1 installer. Per site: at least 1 concierge. Per apartment: at least 2 residents.
- [ ] Devices:
  - per site: 1 outdoor intercom
  - per building: 1 outdoor intercom and 3 lights
  - per apartment: 1 indoor intercom, 3 lights and 2 thermostats
  - That is **57 devices per site, 114 in total**.

**Deliverables & documentation**
- [ ] Code, automated tests and a README for each component (Management Platform, Device Twin, portals, each simulator host).
- [ ] Setup and deployment instructions for both reference machines, plus simulator deployment instructions.
- [ ] A **guided demo script**: a click-by-click walkthrough of the four key moments using the seeded credentials.
- [ ] A **dogfooding findings** document listing every AXIAM or SDK gap, bug or friction point hit, each with a reproduction and the workaround used.

### Out of Scope

- Extending AXIAM or its SDKs. Gaps are **logged, not fixed**; the demo uses what exists today, and a workaround is recorded wherever one is needed.
- Time-bound or expiring installer grants: the user chose revocable-only. AXIAM has no expiry on role assignments either.
- Real hardware devices, real audio or video for intercom calls. Calls are state changes only.
- Production hardening: HA, clustering, backups, secret rotation schedules, internet exposure. This is a local, LAN-only demo.
- Cloud or Kubernetes deployment. Docker Compose only; AXIAM's k3s guide is not used.
- Running simulators on the Raspberry Pi or on non-Linux OSes.
- Mobile apps, i18n, and an observability stack (Prometheus/Grafana). They cost memory on the Pi and don't serve the core moments.
- Self-registration of users. All accounts are created by seed scripts or by property managers.

## Context

### AXIAM capabilities this design relies on (verified against `../axiam` on 2026-09-19)

- **Tenancy:** Organization → Tenant. One demo organization holds two isolated tenants. An org-scope service account can administer both.
- **Authorization:** RBAC over a **resource hierarchy** (`parent_id`; a grant on a parent cascades to its children), with groups and deny-override.
  - There is no ABAC or ReBAC.
  - There is no user-to-user or time-bound delegation. Delegation is modelled as resource-scoped role assignments that the app creates and deletes.
- **gRPC** (`proto/axiam/v1`) offers only:
  - `CheckAccess` / `BatchCheckAccess`, which check the token's own subject
  - token validate and introspect
  - user info
  - Management (users, groups, roles, resources, grants, PKI) is **REST only**. Checking access on behalf of another subject needs REST plus `authz:check_as`.
- **mTLS:** the gRPC listener does **not** verify client certs. Device mTLS login works over REST: `POST /api/v1/auth/device` with a tenant-PKI `Device` cert, which returns an access token. Certs can be issued from a CSR (`/certificates/sign-csr`).
- **SDKs:**
  - Java: REST + gRPC, mTLS, Spring filter plus `@AxiamRequireAccess`.
  - Rust: REST + gRPC, mTLS, Actix guards/macros.
  - Rust-WASM (`axiam-sdk-wasm`): browser `loginOpaque` and `can()`.
  - C and C++: REST only via libcurl, mTLS device login.
  - There is no gRPC-web, so browsers talk REST.
- **Dev stack:** SurrealDB, RabbitMQ 4 (AMQPS), optional Vault, `axiam-server` on :8090. AXIAM already documents a Raspberry Pi 5 deployment (`docs/deployment/rpi5-k3s.md`, which says 8 GB is needed with k3s).
- **Seed tooling:** `scripts/e2e-bootstrap.sh` and the curl walkthroughs in `examples/b1-*` and `examples/b6-*` are templates. There is no demo seed tool.

### Architecture

```
                 Browser (Staff console / Resident app / Sim control)
                        │ HTTPS (single origin via reverse proxy)
                        ▼
        ┌──────────── Caddy reverse proxy (TLS) ────────────┐
        │ /staff  /resident  /api/mgmt  /api/twin  /axiam    │
        └───┬───────────────┬───────────────┬────────────────┘
            │ REST+JWT      │ REST+JWT, SSE │ REST (OPAQUE login via WASM SDK)
            ▼               ▼               ▼
   Management Platform   Device Twin      AXIAM server ── SurrealDB
   (Java/Spring Boot)    (Rust/Actix)     (REST :8090, gRPC)
     │  REST: provisioning   │  gRPC: CheckAccess / ValidateToken
     │  gRPC: CheckAccess    │  REST: check_as for device-originated actions
     │                       │
     └──── PostgreSQL ───────┘            RabbitMQ (AXIAM's broker)
                             │              ├─ vhost axiam  (AXIAM internal, AMQPS)
                             └── MQTT ──────┤
                                            └─ vhost domo   (MQTT plugin, TLS + client certs)
                                                   ▲
                          LAN / same host          │ MQTT over mTLS
                                                   │
             Simulator PC (Linux x86_64): C lights host · C++ intercom host · Rust thermostat host
             each host: N virtual devices × (own keypair, AXIAM Device cert, AXIAM token)
```

**Component responsibilities**

| Component | Language / stack | Owns | Talks to AXIAM via |
|---|---|---|---|
| Management Platform | Java 21, Spring Boot 3, Java SDK | Tenant domain (sites, buildings, apartments, device registry, memberships, installer grants). Mirrors the domain into AXIAM resources and role assignments. Device cert issuance (signs CSRs through AXIAM PKI) | REST for provisioning, using an org-scope service account over mTLS. gRPC `CheckAccess` on every user request, forwarding the user's token |
| Device Twin | Rust (edition 2024), Actix-web, Rust SDK | Device shadows (reported/desired), command dispatch, MQTT bridge, SSE streams for UIs, access decision feed | gRPC `CheckAccess` with the user's token for user commands. REST `check_as` (Twin service account) for device-originated actions such as intercom calls |
| Staff console / Resident app / Sim control | React + TypeScript + Vite, pnpm workspace with a shared package | UI only | `axiam-sdk-wasm`: `loginOpaque` and `can()` for UI gating. Real enforcement is always server-side |
| Simulator hosts | C (lights), C++ (intercoms), Rust (thermostats) | Virtual device behavior and a local control API | REST: mTLS `auth/device` per virtual device. The host itself uses a service account to fetch its device assignments |
| PostgreSQL | 16+ | Management and Twin databases (one instance, two schemas) | — |
| RabbitMQ | AXIAM's instance | The `domo` vhost with the MQTT plugin for device traffic | — |

### Authorization model (AXIAM resources and roles, per tenant)

```
portfolio:{tenant}                          (root)
└─ site:{id}
   ├─ common:site:{id}                      site-level devices live here
   │   └─ device:{id}
   └─ building:{id}
      ├─ common:building:{id}               building-level devices live here
      │   └─ device:{id}
      └─ apartment:{id}
          └─ device:{id}
```

Devices sit under a **common-area** node or an apartment node, never directly under a site or building. Operate permissions are granted only on common-area nodes and on individual apartments or devices. As a result, cascading can never leak "operate" into apartments, and **no deny rules are needed**. A deny would also block installer grants, because deny wins at any depth.

| Role | Assigned on | Permissions (actions) |
|---|---|---|
| `property-manager` | portfolio | `structure:*`, `member:assign`. `device:manage` and `device:operate` on common areas: the Management Platform assigns `common-operator` and `common-device-manager` on every common node |
| `installer` | a site or an apartment (per assignment) | `device:manage`, `device:configure` (cascades to apartments: allowed by the spec). Plus `common-operator` on the site's common nodes |
| `concierge` | common nodes of assigned sites | `device:operate` (via `common-operator`) |
| `resident` | an apartment | `device:manage`, `device:operate`, `grant:manage`. Plus `common-operator` on its building's and site's common nodes |
| `granted-operator` | **individual device** | `device:operate`. Assigned to an installer when a resident grants access, and deleted on revoke |
| `device-self` | its own device resource (device service account) | `twin:report`, `command:receive`. Outdoor intercoms also get `intercom:call` on their building or site |

Tenant isolation comes from AXIAM tenants: each property-management company is its own AXIAM tenant, and user tokens are tenant-bound.

### Key flows

1. **User command:**
   1. The UI calls `POST /api/twin/devices/{id}/commands` with the user's JWT.
   2. The Twin calls gRPC `CheckAccess(device:operate, device:{id})` with that token.
   3. If allowed, the Twin publishes on MQTT `domo/{tenant}/{device}/cmd`, and the device applies the change and reports its new state.
   4. The Twin updates the shadow and pushes it over SSE.
   5. Every decision is appended to the decision feed.
2. **Resident grants an installer:**
   1. The resident app calls the Management Platform `POST /apartments/{id}/grants {installerId, deviceIds[]}`.
   2. The Management Platform checks `grant:manage` on the apartment and creates `granted-operator` role assignments in AXIAM.
   3. The installer's next command succeeds. On revoke the assignments are deleted and the next command is denied (`no_grant`).
3. **Device provisioning:**
   1. A property manager, installer or resident adds a device, and the Management Platform creates the AXIAM resource and service account.
   2. The simulator host polls its assignments, generates a keypair and CSR, and gets the cert signed through the Management Platform → AXIAM PKI. **Private keys never leave the simulator host.**
   3. The device logs in via mTLS `auth/device` and connects to MQTT.
4. **Intercom call:**
   1. The control panel tells the outdoor intercom host to ring apartment X, and the outdoor device publishes a `call` event.
   2. The Twin checks `intercom:call` for that device via `check_as` and routes an `incoming_call` command to apartment X's indoor intercom.
   3. The resident app rings (SSE). The resident answers (operating the indoor intercom) and then unlocks (operating the outdoor intercom). Both are checked with the resident's token.

### Resource budget

The platform must fit comfortably in **8 GB on the Raspberry Pi 5**. The target is **≤ 4 GB** total resident memory with the demo running:
- JVM heap capped at about 512 MB.
- A single PostgreSQL instance and AXIAM's single RabbitMQ.
- Static portals served by Caddy.
- No per-device containers.
- All images are built multi-arch (`linux/amd64`, `linux/arm64`).

## Constraints

- **Target hardware:**
  - The platform runs on amd64 (XPS 9570, 32 GB, ArchLinux) and arm64 (Raspberry Pi 5, 8 GB, Raspberry Pi OS).
  - Simulators run on Linux x86_64 only.
  - Reason: the user's actual demo machines.
- **Languages are fixed by the spec:**
  - Management Platform: Java
  - Device Twin: Rust
  - Portals: React
  - Simulators: C, C++ and Rust
- **Use the AXIAM SDKs for every AXIAM interaction.** Hand-rolled HTTP is allowed only where no SDK covers the call, and each such case is recorded in the dogfooding findings. Reason: dogfooding is a goal.
- **Use AXIAM as it exists today:**
  - gRPC for access checks and token validation (Java and Rust services).
  - REST for provisioning, device login and all browser traffic.
  - mTLS wherever AXIAM verifies it (REST device and service login; MQTT to the broker).
  - Reason: the user chose "use what exists + log gaps".
- **Local, LAN-only:** there is no internet exposure.
- **All trust is anchored in the AXIAM organization CA.** Every certificate in the demo is issued by the AXIAM PKI: server certs for Caddy, AXIAM, RabbitMQ, PostgreSQL, the Management Platform and the Device Twin; client certs for services, simulator hosts and devices. No second CA exists. Browsers and client machines trust the exported AXIAM root. Reason: the user wants everything bound to the AXIAM CA, which also means more PKI dogfooding.
- **Disk hygiene:** clean build outputs after each component build. Reason: the user's machine has run out of disk mid-session before.

## Key Decisions

| Decision | Rationale | Outcome |
|---|---|---|
| Use existing AXIAM APIs and log gaps instead of extending AXIAM | Keeps scope to the demo; gaps become useful product feedback | — Pending |
| Device ↔ Twin over **MQTT on AXIAM's RabbitMQ** (separate `domo` vhost, MQTT plugin, TLS with client certs) | IoT-standard protocol with mature C, C++ and Rust clients; no extra broker on the Pi | — Pending |
| Portal login via **OPAQUE with `axiam-sdk-wasm`** | Matches "prefer WASM"; the password never leaves the browser; shows off an AXIAM strength | — Pending |
| **Two portals:** Staff console (property manager, installer, concierge) and Resident app. Plus a small Sim control page | The user's choice; separates operator and resident experiences | — Pending |
| Simulators split **by device type**: C = lights, C++ = intercoms, Rust = thermostats | Every SDK is exercised without writing each device three times; the physics sim sits in Rust | — Pending |
| Multi-device simulator hosts (one process per language) | Fits a small PC; each virtual device still has its own identity and cert | — Pending |
| Enforce "staff never operate apartment devices" with **common-area resource nodes**, not deny rules | A deny would also override resident→installer grants, since deny wins at any depth | — Pending |
| Installer grants are **per selected device, revocable, no expiry**, implemented as device-scoped `granted-operator` role assignments | The user's choice; AXIAM has no native delegation or expiry | — Pending |
| Simulator hosts generate device keys locally; certs are signed via CSR | Private keys never travel; realistic device provisioning | — Pending |
| PostgreSQL (one instance) for the Management Platform and Twin; AXIAM keeps its own SurrealDB | Mature Java and Rust drivers; lean enough for the Pi; avoids coupling to AXIAM's storage | — Pending |
| Docker Compose with multi-arch images; Caddy as a single-origin TLS reverse proxy | Same deployment on both machines; a single origin avoids CORS and cookie issues | — Pending |
| Four device categories (outdoor intercom, indoor intercom, light, thermostat) | DEFINITIONS.md says "3 categories" but lists 4. This treats that as a typo | — Pending |
| Property managers **can operate** common-area devices, and never apartment devices | The spec only states manage rights and the apartment ban; the user confirmed this reading | — Pending |
| *Configure* = device settings (thermostat limits, intercom → apartment mapping, light groups). *Operate* = runtime commands | Separates installer setup work from the operate ban on apartment devices; the user confirmed this | — Pending |
| A resident may grant access only to **installers assigned to the resident's site** | Keeps grants within the property's own staff; the user confirmed this | — Pending |
| **Every certificate is issued by the AXIAM organization CA**, server TLS included. No separate demo CA | The user's choice: everything is bound to AXIAM trust, and the PKI gets more dogfooding | — Pending |
| Broker topic authorization (RabbitMQ OAuth2 backend with AXIAM JWTs, or an HTTP auth backend in the Twin) is **decided in phase research** | Depends on AXIAM token claims; whichever works without changing AXIAM wins. The user agreed to defer it | — Pending |

### TLS bootstrap implication

AXIAM must be running before its CA exists, so setup has two stages:
1. AXIAM starts with a temporary bootstrap certificate, bound to localhost only.
2. Setup creates the org CA and issues server certs for AXIAM, Caddy, RabbitMQ, PostgreSQL, the Management Platform and the Twin. It then restarts those services on the AXIAM-issued certs and exports the root for browsers and the simulator PC.

`just demo-reset` repeats the whole sequence. Anything that goes wrong in this process is recorded in the dogfooding findings.

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-09-19 after initialization (approved by user)*
