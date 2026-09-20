# Roadmap: AXIAM Domo Demo

## Overview

The demo is built as six horizontal technical layers, in dependency order: a foundation that establishes the single trust anchor, the AXIAM tenant/resource model and a proven MQTT-over-mTLS path; the Management Platform that owns tenant structure, staff, devices and installer grants; the Device Twin that turns authorized commands into real device state with tenant isolation and a decision feed; the three simulator hosts that bring all 114 devices online; the React portals that give every role a real, AXIAM-authenticated UI; and a final hardening pass that proves the whole thing on real Raspberry Pi hardware and ships the documentation and dogfooding findings. Each layer proves its own risk before the next one builds on it — most notably, MQTT-over-mTLS against AXIAM's own broker is proven with a single device in the foundation phase, well before 114 devices depend on it.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [ ] **Phase 1: Foundation** - Single trust anchor, AXIAM tenants/resource model and groups, Caddy single origin, and a proven single-device MQTT-over-mTLS path
- [ ] **Phase 2: Management Platform** - Tenant structure, staff, device provisioning and installer grants, all mirrored into AXIAM, plus seed data
- [ ] **Phase 3: Device Twin + MQTT** - Authorized command dispatch, live shadows, tenant-isolated SSE, intercom routing and the decision feed, holding at 114-device MQTT scale
- [ ] **Phase 4: Simulators** - All 114 devices running as real virtual devices across the C, C++ and Rust simulator hosts
- [ ] **Phase 5: Portals** - Staff console, Resident app and sim control page, with real AXIAM logins and role-scoped views
- [ ] **Phase 6: Demo Hardening, E2E & Docs** - Full-load validation on real Pi hardware, automated tests, and the demo/docs deliverables

## Phase Details

### Phase 1: Foundation
**Goal**: The platform boots from a single trust anchor, AXIAM's tenant and resource/role model is established, and a device can already prove it can reach the broker end-to-end.
**Depends on**: Nothing (first phase)
**Requirements**: PLAT-01, PLAT-02, PLAT-05, PLAT-06, PKI-01, PKI-02, PKI-03, PKI-04, PKI-05, PKI-06, AUTHZ-01, AUTHZ-02, MQTT-01, MQTT-02
**Success Criteria** (what must be TRUE):
  1. Operator brings up AXIAM, PostgreSQL and Caddy with one command, and `just demo-reset` wipes and rebuilds the PKI, AXIAM tenants/roles/groups and certificates idempotently (PLAT-01, PLAT-02, PLAT-05).
  2. A single organization root is generated at setup, AXIAM imports it (BYOK), issues one tenant signing CA per tenant, and Chromium/Firefox trust Caddy's offline-signed SAN certs without warnings once the root is imported (PKI-01, PKI-02, PKI-03, PKI-04, PKI-05, PKI-06).
  3. All browser-facing traffic is reachable through a single Caddy origin proxying to AXIAM, and the AXIAM resource tree (portfolio → site → common/building → apartment → device) plus the group-per-(role, resource) pattern exist and are queryable via the AXIAM API (PLAT-06, AUTHZ-01, AUTHZ-02).
  4. A single test device authenticates to AXIAM over mTLS, receives a JWT, and connects to the `domo` MQTT vhost using cert + JWT, validated end-to-end by a working RabbitMQ HTTP auth backend (MQTT-01, MQTT-02).
**Plans**: 7 plans (4 waves)
Plans:
- [ ] 01-01-PLAN.md — Tracer: offline root → AXIAM BYOK import → tenant CA → device cert → mTLS login → accepted MQTT CONNECT (wave 1)
- [ ] 01-02-PLAN.md — Whole-chain PKI verification, root export and trust docs, secrets guard, dogfooding findings log (wave 2)
- [ ] 01-03-PLAN.md — Caddy single origin, AXIAM console host, landing page, PostgreSQL (wave 3)
- [ ] 01-04-PLAN.md — AuthZ catalog, per-tenant admin, signing CAs, service credentials, resource tree and group pattern (wave 2)
- [ ] 01-05-PLAN.md — Device Twin RabbitMQ auth backend: full four-endpoint contract with offline test suite (wave 2)
- [ ] 01-06-PLAN.md — Smoke: live authorization assertions and the positive/negative device connect matrix (wave 3)
- [ ] 01-07-PLAN.md — Preflight, staged resumable checklist, demo-reset, arm64 images, phase verification and demo card (wave 4)

### Phase 2: Management Platform
**Goal**: Property managers can fully manage their tenant's structure, staff, devices and installer grants, with every change mirrored into AXIAM, and seed data stands up two realistic tenants.
**Depends on**: Phase 1
**Requirements**: MGMT-01, MGMT-02, MGMT-03, MGMT-04, MGMT-05, MGMT-06, MGMT-07, DEV-01, DEV-02, DEV-03, DEV-04, DEV-05, DEV-07, AUTHZ-08, AUTH-06, GRANT-01, GRANT-03, GRANT-04, GRANT-05, SEED-01, SEED-02
**Success Criteria** (what must be TRUE):
  1. A property manager can create, edit and delete sites, buildings and apartments, create staff/resident accounts, and assign or remove installers/concierges/residents, with every change reflected in AXIAM resources and groups before the API reports success, and AXIAM failures surfaced and retried idempotently (MGMT-01…07).
  2. A property manager or installer can add, edit, delete and configure site-, building- and apartment-level devices; adding a device creates its AXIAM resource and service account, and issues, signs and binds its certificate as one atomic provisioning step (DEV-01…05, DEV-07).
  3. A resident can grant an assigned installer access to selected apartment devices and revoke it; any resident of the apartment can revoke, AXIAM's CheckAccess reflects the revoke immediately, and both the resident and the installer can see the current grants (GRANT-01, GRANT-03, GRANT-04, GRANT-05).
  4. Seeding creates 2 tenants (1 site each, 2 buildings per site, 4 apartments per building) with the required property managers, installers, concierges and residents, and no deny rule exists above any apartment or device node (SEED-01, SEED-02, AUTHZ-08).
  5. Every Management Platform request's AXIAM token is validated server-side; a request with a missing or invalid token is rejected even if a UI would have allowed it (AUTH-06).
**Plans**: TBD

### Phase 3: Device Twin + MQTT
**Goal**: Authorized users and devices operate lights, thermostats and intercoms end-to-end through AXIAM-checked commands, with live state, verified tenant isolation and a decision feed, and the messaging layer holds at full 114-device scale.
**Depends on**: Phase 2
**Requirements**: TWIN-01, TWIN-02, TWIN-03, TWIN-04, TWIN-05, TWIN-06, TWIN-07, TWIN-08, TWIN-09, MQTT-03, MQTT-04, MQTT-05, AUTHZ-03, AUTHZ-04, AUTHZ-05, AUTHZ-06, AUTHZ-07, GRANT-02, ICOM-02, ICOM-05, FEED-01
**Success Criteria** (what must be TRUE):
  1. A command to a light, thermostat or intercom is checked with AXIAM `CheckAccess` using the caller's own token before dispatch, a denied command never reaches the device, and the resulting state converges back through the shadow within 2 seconds (TWIN-01…06).
  2. Residents operate their own apartment plus their building's and site's common areas; concierges and property managers operate only common areas of their assigned scope; a granted installer operates exactly the granted devices and nothing else in the apartment; and all are denied on apartment devices without a grant (AUTHZ-03, AUTHZ-04, AUTHZ-05, AUTHZ-06, GRANT-02).
  3. A device's online/offline status tracks via MQTT LWT and is shown in the shadow; a command to an offline device is kept as pending desired state and applied on reconnect (TWIN-07, TWIN-08).
  4. No SSE stream, and no CheckAccess-gated response from the Twin, ever exposes another tenant's users, resources or devices (AUTHZ-07, TWIN-09).
  5. An outdoor intercom can only route a call to indoor intercoms in its own site or building (checked via `check_as`/`intercom:call`), an unanswered call times out and appears missed, every decision is recorded with subject/action/resource/result/reason, and device connections stay authenticated via jittered proactive re-auth while all 114 devices coexist with AXIAM's own AMQP traffic (ICOM-02, ICOM-05, FEED-01, MQTT-03, MQTT-04, MQTT-05).
**Plans**: TBD

### Phase 4: Simulators
**Goal**: All 114 seeded devices run as real virtual devices, each with its own AXIAM identity, across three language-specific simulator hosts on a Linux PC.
**Depends on**: Phase 3
**Requirements**: PLAT-04, SIM-01, SIM-02, SIM-03, SIM-04, SIM-05, SIM-06, SEED-03, DEV-06
**Success Criteria** (what must be TRUE):
  1. The three simulator hosts (C for lights, C++ for intercoms, Rust for thermostats) run on a Linux x86_64 PC and reach the platform over the LAN or on the same host, each virtual device carrying its own AXIAM identity, certificate, token and MQTT connection (PLAT-04, SIM-01, SIM-02, SIM-03).
  2. Thermostats simulate realistic thermal behavior: temperature moves toward the target while heating/cooling, drifts toward ambient when off, and drops during a simulated window-open disturbance (SIM-04).
  3. A device added or removed in the domain is picked up by the right simulator host and comes online within 30 seconds of being added, with no host restart (SIM-05, DEV-06).
  4. Each simulator host exposes a local control API for ring, disturbance and offline/online actions (SIM-06).
  5. Seeding brings all 114 devices online, each with a valid, bound certificate (SEED-03).
**Plans**: TBD

### Phase 5: Portals
**Goal**: Property managers, installers, concierges and residents each get a role-appropriate web app, backed by real AXIAM logins, that exposes every action, denial reason and the live decision feed.
**Depends on**: Phase 4
**Requirements**: AUTH-01, AUTH-02, AUTH-03, AUTH-04, AUTH-05, DEV-08, ICOM-01, ICOM-03, ICOM-04, SIM-07, UI-01, UI-02, UI-03, UI-04, UI-05, UI-06, UI-07, UI-08, FEED-02, FEED-03
**Success Criteria** (what must be TRUE):
  1. A user logs into the Staff console or Resident app via OPAQUE through `axiam-sdk-wasm` (the password never leaves the browser), stays logged in across a page refresh, can log out from any page, and is rejected with a clear message on the wrong portal for their role (AUTH-01…05).
  2. Property managers, installers, concierges and residents each see and can act only on what their role permits in the Staff console or Resident app, including managing installer grants and opening a device's identity panel (certificate, issuer, expiry) (UI-01, UI-02, UI-03, UI-04, UI-05, DEV-08).
  3. A denied action shows a clear, human-readable message including AXIAM's reason code, and an "under the hood" overlay shows which AXIAM SDK and language handled each hop of the action (UI-06, UI-07).
  4. A presenter can enable an in-app guided-demo checklist that walks through the four key moments step by step (UI-08).
  5. The sim control page rings intercoms, triggers window-open disturbances and takes devices online/offline; the Resident app shows an incoming call in real time and lets the resident answer then unlock; and both portals show a live decision feed, scoped to the viewer's tenant in the Staff console and toggleable and self-scoped in the Resident app (SIM-07, ICOM-01, ICOM-03, ICOM-04, FEED-02, FEED-03).
**Plans**: TBD
**UI hint**: yes

### Phase 6: Demo Hardening, E2E & Docs
**Goal**: The full demo runs reliably and within budget on both real reference machines, every component is tested and documented, and the AXIAM dogfooding findings are recorded.
**Depends on**: Phase 5
**Requirements**: PLAT-03, QUAL-01, QUAL-02, DOC-01, DOC-02, DOC-03, DOC-04, DOC-05
**Success Criteria** (what must be TRUE):
  1. The whole platform starts with one command on both the Dell XPS 9570 (amd64) and a real Raspberry Pi 5 (arm64, 8 GB), staying at or below 4 GB total resident memory with all 114 devices connected and both portals in use (PLAT-03).
  2. Each component has automated tests for its own logic, the Management Platform, Twin and simulators have integration tests against a running AXIAM, and an automated end-to-end suite verifies all four key moments (cross-role denial, grant/revoke, tenant isolation, live device loop) against the live stack (QUAL-01, QUAL-02).
  3. Every component (Management Platform, Twin, portals, each simulator host) has a README, both reference machines have a setup guide, and the simulator PC has a deployment guide (DOC-01, DOC-02, DOC-03).
  4. A written guided demo script walks click-by-click through the four key moments with seeded credentials, and the dogfooding findings document lists every AXIAM/SDK gap hit, each with a reproduction and the workaround used (DOC-04, DOC-05).
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation | 0/7 | Planned | - |
| 2. Management Platform | 0/TBD | Not started | - |
| 3. Device Twin + MQTT | 0/TBD | Not started | - |
| 4. Simulators | 0/TBD | Not started | - |
| 5. Portals | 0/TBD | Not started | - |
| 6. Demo Hardening, E2E & Docs | 0/TBD | Not started | - |
