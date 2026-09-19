# Requirements: AXIAM Domo Demo

**Defined:** 2026-09-19
**Core Value:** Every user and device action goes through AXIAM, and the four key demo moments run reliably on a single small machine. The moments are cross-role denial, resident→installer grant/revoke, tenant isolation, and the live device loop.

## v1 Requirements

### Platform & Deployment (PLAT)

- [ ] **PLAT-01**: Operator can start the whole platform with one command on the Dell XPS 9570 (amd64, ArchLinux). The platform is AXIAM, PostgreSQL, the Management Platform, the Device Twin, both portals, the sim control page and Caddy.
- [ ] **PLAT-02**: Operator can start the same platform with one command on a Raspberry Pi 5 with 8 GB (arm64, Raspberry Pi OS), using multi-arch images.
- [ ] **PLAT-03**: The platform's total resident memory stays at or below 4 GB on the Pi with the full demo running (114 devices connected, both portals in use).
- [ ] **PLAT-04**: Operator can run the three simulator hosts on a Linux x86_64 PC. They connect to a platform on the LAN (Pi) or on the same host (XPS).
- [ ] **PLAT-05**: Operator can wipe and fully rebuild the demo state with `just demo-reset`: PKI, AXIAM tenants/roles/groups, domain data, device certs. The command is idempotent and can be re-run after an interruption.
- [ ] **PLAT-06**: All browser traffic goes through a single HTTPS origin (Caddy), which serves the portals and proxies `/api/mgmt`, `/api/twin` and `/axiam`.

### PKI & Trust (PKI)

- [ ] **PKI-01**: Setup generates the organization root. AXIAM imports it with its key (BYOK), and it is the only trust anchor in the demo.
- [ ] **PKI-02**: AXIAM issues one tenant signing CA (intermediate) per tenant under the imported root.
- [ ] **PKI-03**: Every device and service client certificate is issued by AXIAM from the owning tenant's signing CA.
- [ ] **PKI-04**: Every server certificate (Caddy, AXIAM, RabbitMQ, PostgreSQL, Management Platform, Twin) carries correct SANs and is signed by the same root at setup. Chromium and Firefox trust the portals without warnings once the root is imported.
- [ ] **PKI-05**: Operator can export the root and follow documented steps to trust it on the presenting machine (browser and OS) and on the simulator PC.
- [ ] **PKI-06**: The root private key lives only in a setup-owned secrets directory that is git-ignored and never baked into images.

### Authentication (AUTH)

- [ ] **AUTH-01**: A user can log into the Staff console with username and password via OPAQUE (`axiam-sdk-wasm`); the password never leaves the browser.
- [ ] **AUTH-02**: A user can log into the Resident app via OPAQUE (`axiam-sdk-wasm`).
- [ ] **AUTH-03**: A user's session persists across page refresh and is silently refreshed before expiry. It uses AXIAM's httpOnly session cookies and CSRF cookie.
- [ ] **AUTH-04**: A user can log out from any page, which ends the AXIAM session.
- [ ] **AUTH-05**: Each portal admits only its own roles:
  - Staff console: property managers, installers, concierges.
  - Resident app: residents.
  - A user with the wrong role gets a clear message, not a broken page.
- [ ] **AUTH-06**: The Management Platform and Twin validate every request's AXIAM token server-side and never trust UI-side checks.

### Authorization Model (AUTHZ)

- [ ] **AUTHZ-01**: Each tenant's domain is mirrored in AXIAM as the resource tree portfolio → site → {site common area, building} → {building common area, apartment} → device.
- [ ] **AUTHZ-02**: Roles are granted through one AXIAM group per (role, resource), and users gain or lose access only through group membership.
- [ ] **AUTHZ-03**: A resident can operate devices in their own apartment and in the common areas of their building and site.
- [ ] **AUTHZ-04**: A concierge can operate common-area devices of their assigned sites only.
- [ ] **AUTHZ-05**: A property manager can operate common-area devices in their tenant.
- [ ] **AUTHZ-06**: Property managers, concierges and installers are denied on apartment devices unless an explicit per-device grant exists. The UI shows the AXIAM reason.
- [ ] **AUTHZ-07**: A user of tenant A cannot list, view, stream or act on any resource, user or device of tenant B, whether through the Management Platform or the Twin, including SSE.
- [ ] **AUTHZ-08**: No deny rule exists above apartment or device nodes; a seed/test check verifies this.

### Structure & Membership Management (MGMT)

- [ ] **MGMT-01**: A property manager can create, edit and delete sites in their tenant.
- [ ] **MGMT-02**: A property manager can create, edit and delete buildings in a site.
- [ ] **MGMT-03**: A property manager can create, edit and delete apartments in a building.
- [ ] **MGMT-04**: A property manager can create user accounts (installer, concierge, resident) in their tenant.
- [ ] **MGMT-05**: A property manager can assign installers to sites or apartments, concierges to sites, and residents to apartments, and can remove those assignments.
- [ ] **MGMT-06**: Every structure or membership change is reflected in AXIAM (resources, groups, memberships) before the UI reports success. AXIAM failures are surfaced and retried idempotently.
- [ ] **MGMT-07**: Deleting a site, building or apartment removes its devices, AXIAM resources and groups, and the affected users lose access immediately.

### Devices & Provisioning (DEV)

- [ ] **DEV-01**: A property manager or installer can add, edit and delete site- and building-level devices, of all four types.
- [ ] **DEV-02**: An installer or a resident of the apartment can add, edit and delete apartment-level devices.
- [ ] **DEV-03**: An installer can configure device settings, as distinct from operating the device:
  - thermostat limits
  - the apartments an outdoor intercom may call
  - light groups
- [ ] **DEV-04**: Adding a device creates its AXIAM resource and service account, assigns `device-self` permissions, and makes it available to the right simulator host.
- [ ] **DEV-05**: A new device's certificate is issued from a CSR generated on the simulator host (the private key never leaves it), signed by the tenant CA, and bound to the device's service account. This happens as one provisioning step.
- [ ] **DEV-06**: A new device comes online within 30 seconds of being added in the UI, without restarting any simulator host.
- [ ] **DEV-07**: Deleting a device revokes its access. Its service account is disabled, its broker connection is closed, and it cannot reconnect.
- [ ] **DEV-08**: A user can open a device identity panel showing:
  - the device's AXIAM service account
  - certificate serial, issuer (tenant CA) and expiry
  - current token expiry

### Device Twin (TWIN)

- [ ] **TWIN-01**: The Twin keeps a shadow per device with reported state, desired state, version and online status, persisted in PostgreSQL.
- [ ] **TWIN-02**: A user can turn a light on or off, dim it, and set its RGB color.
- [ ] **TWIN-03**: A user can turn a thermostat on or off, switch heat/cool mode, and set the target temperature.
- [ ] **TWIN-04**: A resident can answer a ringing indoor intercom and unlock the calling outdoor intercom.
- [ ] **TWIN-05**: Every user command is checked with AXIAM `CheckAccess` over gRPC, using the user's token, before dispatch. Denied commands never reach the device.
- [ ] **TWIN-06**: Device state changes appear in every authorized open UI within 2 seconds via SSE.
- [ ] **TWIN-07**: A device's online/offline status is tracked through MQTT Last Will and Testament (LWT) and shown in the UI.
- [ ] **TWIN-08**: A command sent to an offline device is kept as desired state and applied when the device reconnects. The UI shows it as pending, then converged.
- [ ] **TWIN-09**: Each SSE stream delivers only devices the connected user is authorized to see.

### Device Messaging (MQTT)

- [ ] **MQTT-01**: Devices connect to AXIAM's RabbitMQ `domo` vhost via the MQTT plugin over mTLS with their AXIAM-issued certificates.
- [ ] **MQTT-02**: A device authenticates to the broker with its AXIAM JWT, obtained via SDK mTLS login, as the MQTT password. The Twin's HTTP auth backend rejects a JWT whose subject doesn't match the connecting cert.
- [ ] **MQTT-03**: A device can publish and subscribe only on its own topics under `domo/{tenant}/{device}/…`, and never on another device's or tenant's topics.
- [ ] **MQTT-04**: Simulator hosts re-authenticate each device proactively, with per-device jitter, before its 900 s token expires, so there's no synchronized reconnect storm.
- [ ] **MQTT-05**: AXIAM's own AMQP traffic keeps working while all 114 devices are connected.

### Delegation (GRANT)

- [ ] **GRANT-01**: A resident can grant an installer assigned to their site operate access to selected devices of their apartment.
- [ ] **GRANT-02**: A granted installer can operate exactly the granted devices, and still nothing else in the apartment.
- [ ] **GRANT-03**: Any resident of the apartment can revoke a grant. The installer's next command is denied (`no_grant`).
- [ ] **GRANT-04**: A resident can see the current grants for their apartment: installer, devices, and who granted.
- [ ] **GRANT-05**: An installer can see which apartment devices they currently hold grants on.

### Intercom (ICOM)

- [ ] **ICOM-01**: The operator can ring a chosen apartment from a chosen outdoor intercom via the sim control panel.
- [ ] **ICOM-02**: An outdoor intercom can only call indoor intercoms within its own site or building. The Twin checks this with AXIAM (`check_as`, `intercom:call`) and denies it otherwise.
- [ ] **ICOM-03**: The target apartment's residents see an incoming call in the Resident app in real time.
- [ ] **ICOM-04**: A resident can answer, then unlock. The outdoor intercom reports unlocked for a few seconds, then relocks.
- [ ] **ICOM-05**: An unanswered call ends after a 10–15 s ring timeout and appears as a missed call.

### Simulators (SIM)

- [ ] **SIM-01**: A C simulator host runs all light devices, each with its own identity, cert, token and MQTT connection.
- [ ] **SIM-02**: A C++ simulator host runs all outdoor and indoor intercom devices, each with its own identity, cert, token and MQTT connection.
- [ ] **SIM-03**: A Rust simulator host runs all thermostat devices, each with its own identity, cert, token and MQTT connection.
- [ ] **SIM-04**: Thermostats simulate room temperature: it moves toward the target when heating or cooling, drifts toward ambient when off, and drops during a "window open" disturbance.
- [ ] **SIM-05**: Each simulator host discovers newly added and removed devices of its type without restarting.
- [ ] **SIM-06**: Each simulator host exposes a local control API for the sim control panel: ring, disturbance, offline/online.
- [ ] **SIM-07**: The operator can use a sim control page to ring intercoms, trigger window-open disturbances, and take any device offline or online.

### Portals (UI)

- [ ] **UI-01**: A property manager can manage sites, buildings, apartments, users, assignments and common-area devices in the Staff console.
- [ ] **UI-02**: An installer can manage and configure devices on their assigned sites or apartments, and operate granted devices, in the Staff console.
- [ ] **UI-03**: A concierge can see and operate common-area devices of their sites in the Staff console.
- [ ] **UI-04**: A resident can see and operate their apartment's devices and their building's and site's common-area devices in the Resident app.
- [ ] **UI-05**: A resident can manage installer grants in the Resident app (GRANT-01…04).
- [ ] **UI-06**: Denied actions show a clear, human-readable explanation that includes AXIAM's reason code.
- [ ] **UI-07**: A user can open an "under the hood" overlay showing which AXIAM SDK and language handles each hop of the current action (Java, Rust, WASM, C, C++).
- [ ] **UI-08**: A presenter can enable an in-app guided-demo checklist that walks through the four key moments step by step.

### Access Decision Feed (FEED)

- [ ] **FEED-01**: Every AXIAM access decision made by the Management Platform or the Twin is recorded with timestamp, subject, action, resource, result and reason.
- [ ] **FEED-02**: The Staff console shows a live decision feed, scoped to the viewer's tenant.
- [ ] **FEED-03**: The Resident app has a toggleable decision feed showing only the resident's own decisions.

### Seed Data & Demo Tooling (SEED)

- [ ] **SEED-01**: Seeding creates 2 tenants, each with 1 site. Each site has 2 buildings, and each building has 4 apartments.
- [ ] **SEED-02**: Seeding creates at least:
  - 1 property manager and 1 installer per tenant
  - 1 concierge per site
  - 2 residents per apartment
  - All accounts have documented demo credentials.
- [ ] **SEED-03**: Seeding creates 57 devices per site (114 in total), all with certificates, all coming online:
  - per site: 1 outdoor intercom
  - per building: 1 outdoor intercom and 3 lights
  - per apartment: 1 indoor intercom, 3 lights and 2 thermostats

### Documentation (DOC)

- [ ] **DOC-01**: Each component has a README with build, test and run instructions: Management Platform, Twin, portals, each simulator host.
- [ ] **DOC-02**: Setup guides cover both reference machines: prerequisites, root trust, first run and troubleshooting.
- [ ] **DOC-03**: A simulator deployment guide covers the Linux PC.
- [ ] **DOC-04**: A written guided demo script walks click by click through the four key moments with the seeded credentials.
- [ ] **DOC-05**: A dogfooding findings document lists every AXIAM or SDK gap with a reproduction and the workaround. It already includes:
  - no SAN/KU/EKU on leaf certificates
  - device cert binding: the docs and the code disagree
  - `has_role` uniqueness
  - no gRPC management API
  - gRPC with no client-cert verification
  - no refresh token for devices
  - OAuth2 scopes not usable by RabbitMQ

### Quality (QUAL)

- [ ] **QUAL-01**: Each component has automated tests for its own logic. The Management Platform, Twin and simulators also have integration tests against a running AXIAM.
- [ ] **QUAL-02**: An automated end-to-end suite verifies the four key moments against the running stack: cross-role denial, grant/revoke, tenant isolation, live device loop.

## v2 Requirements

- **V2-01**: Per-tenant reset (reset one tenant without touching the other).
- **V2-02**: Intercom call forwarding to a concierge when a resident doesn't answer.
- **V2-03**: Time-bound installer grants (would need app-side expiry; AXIAM has none).
- **V2-04**: Observability stack (metrics dashboards) on the XPS only.

## Out of Scope

| Feature | Reason |
|---|---|
| Extending AXIAM or its SDKs | The user's choice: gaps are logged in DOC-05, not fixed |
| Policy/rule editor UI | Would misrepresent AXIAM's RBAC-over-hierarchy model to evaluators |
| Real hardware, intercom audio/video | Simulated state only |
| HA, clustering, backups, internet exposure | Local, LAN-only demo |
| Kubernetes/cloud deployment | Docker Compose only |
| Simulators on the Pi or non-Linux | Simulators target a Linux x86_64 PC |
| Mobile apps, i18n | Not needed for the four moments |
| User self-registration | Accounts come from the seed or from property managers |
| OTA firmware, notifications/paging | Not part of the showcase |
| Vault | Memory budget; AXIAM's default CA key custody is enough |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|---|---|---|
| PLAT-01 | Phase 1 | Pending |
| PLAT-02 | Phase 1 | Pending |
| PLAT-03 | Phase 6 | Pending |
| PLAT-04 | Phase 4 | Pending |
| PLAT-05 | Phase 1 | Pending |
| PLAT-06 | Phase 1 | Pending |
| PKI-01 | Phase 1 | Pending |
| PKI-02 | Phase 1 | Pending |
| PKI-03 | Phase 1 | Pending |
| PKI-04 | Phase 1 | Pending |
| PKI-05 | Phase 1 | Pending |
| PKI-06 | Phase 1 | Pending |
| AUTH-01 | Phase 5 | Pending |
| AUTH-02 | Phase 5 | Pending |
| AUTH-03 | Phase 5 | Pending |
| AUTH-04 | Phase 5 | Pending |
| AUTH-05 | Phase 5 | Pending |
| AUTH-06 | Phase 2 | Pending |
| AUTHZ-01 | Phase 1 | Pending |
| AUTHZ-02 | Phase 1 | Pending |
| AUTHZ-03 | Phase 3 | Pending |
| AUTHZ-04 | Phase 3 | Pending |
| AUTHZ-05 | Phase 3 | Pending |
| AUTHZ-06 | Phase 3 | Pending |
| AUTHZ-07 | Phase 3 | Pending |
| AUTHZ-08 | Phase 2 | Pending |
| MGMT-01 | Phase 2 | Pending |
| MGMT-02 | Phase 2 | Pending |
| MGMT-03 | Phase 2 | Pending |
| MGMT-04 | Phase 2 | Pending |
| MGMT-05 | Phase 2 | Pending |
| MGMT-06 | Phase 2 | Pending |
| MGMT-07 | Phase 2 | Pending |
| DEV-01 | Phase 2 | Pending |
| DEV-02 | Phase 2 | Pending |
| DEV-03 | Phase 2 | Pending |
| DEV-04 | Phase 2 | Pending |
| DEV-05 | Phase 2 | Pending |
| DEV-06 | Phase 4 | Pending |
| DEV-07 | Phase 2 | Pending |
| DEV-08 | Phase 5 | Pending |
| TWIN-01 | Phase 3 | Pending |
| TWIN-02 | Phase 3 | Pending |
| TWIN-03 | Phase 3 | Pending |
| TWIN-04 | Phase 3 | Pending |
| TWIN-05 | Phase 3 | Pending |
| TWIN-06 | Phase 3 | Pending |
| TWIN-07 | Phase 3 | Pending |
| TWIN-08 | Phase 3 | Pending |
| TWIN-09 | Phase 3 | Pending |
| MQTT-01 | Phase 1 | Pending |
| MQTT-02 | Phase 1 | Pending |
| MQTT-03 | Phase 3 | Pending |
| MQTT-04 | Phase 3 | Pending |
| MQTT-05 | Phase 3 | Pending |
| GRANT-01 | Phase 2 | Pending |
| GRANT-02 | Phase 3 | Pending |
| GRANT-03 | Phase 2 | Pending |
| GRANT-04 | Phase 2 | Pending |
| GRANT-05 | Phase 2 | Pending |
| ICOM-01 | Phase 5 | Pending |
| ICOM-02 | Phase 3 | Pending |
| ICOM-03 | Phase 5 | Pending |
| ICOM-04 | Phase 5 | Pending |
| ICOM-05 | Phase 3 | Pending |
| SIM-01 | Phase 4 | Pending |
| SIM-02 | Phase 4 | Pending |
| SIM-03 | Phase 4 | Pending |
| SIM-04 | Phase 4 | Pending |
| SIM-05 | Phase 4 | Pending |
| SIM-06 | Phase 4 | Pending |
| SIM-07 | Phase 5 | Pending |
| UI-01 | Phase 5 | Pending |
| UI-02 | Phase 5 | Pending |
| UI-03 | Phase 5 | Pending |
| UI-04 | Phase 5 | Pending |
| UI-05 | Phase 5 | Pending |
| UI-06 | Phase 5 | Pending |
| UI-07 | Phase 5 | Pending |
| UI-08 | Phase 5 | Pending |
| FEED-01 | Phase 3 | Pending |
| FEED-02 | Phase 5 | Pending |
| FEED-03 | Phase 5 | Pending |
| SEED-01 | Phase 2 | Pending |
| SEED-02 | Phase 2 | Pending |
| SEED-03 | Phase 4 | Pending |
| DOC-01 | Phase 6 | Pending |
| DOC-02 | Phase 6 | Pending |
| DOC-03 | Phase 6 | Pending |
| DOC-04 | Phase 6 | Pending |
| DOC-05 | Phase 6 | Pending |
| QUAL-01 | Phase 6 | Pending |
| QUAL-02 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 93 total
- Mapped to phases: 93/93 (100%)

---
*Requirements defined: 2026-09-19*
*Last updated: 2026-09-19 after roadmap creation (6-phase horizontal-layer structure)*
