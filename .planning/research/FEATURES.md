# Feature Research

**Domain:** Multi-tenant IoT property-management / smart-building demo, built to showcase an IAM product (AXIAM)
**Researched:** 2026-09-19
**Confidence:** MEDIUM-HIGH — core scope is already fixed by `.planning/PROJECT.md` and `DEFINITIONS.md` (treat those as ground truth); this document adds ecosystem grounding (AWS IoT Device Shadow / Azure Device Twin patterns, commercial multi-tenant intercom/access platforms such as 2N, Ring Intercom, SmartRent, Swiftlane, Tapkey) and turns it into table-stakes / differentiator / anti-feature calls for THIS demo specifically. This is not a general proptech product plan — every recommendation is filtered through "does this make the four key demo moments land?"

The four key demo moments (from PROJECT.md, restated for reference): **(1) cross-role denial, (2) resident→installer grant/revoke, (3) tenant isolation, (4) the live device loop.** Every table below flags which moment(s) a feature serves.

## Feature Landscape

### Table Stakes (Demo Falls Flat Without These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Site/building/apartment CRUD, scoped to the acting tenant | Every proptech platform (SmartRent, DOOR, Tapkey) organizes access around a property hierarchy; without it there's nothing to grant roles or devices onto | LOW | Already specified in PROJECT.md's resource tree. Delete must cascade sensibly (block or cascade-delete devices/members) |
| Hierarchical resource tree mirrored 1:1 into AXIAM on every domain change | This *is* the IAM story — if the Management Platform's DB and AXIAM's resource tree drift, moment 1 and 3 both break silently | MEDIUM | Needs a sync strategy (transactional outbox or synchronous dual-write with reconciliation); PROJECT.md already commits to "every domain change keeps the AXIAM resource tree and role assignments in sync" |
| Role assignment at the right scope (property-manager@portfolio, installer@site-or-apartment, concierge@site, resident@apartment) | Table stakes for any RBAC-over-hierarchy product; this is literally what AXIAM sells | LOW-MEDIUM | Assignment scope is fixed by PROJECT.md's role table — no product decision needed, just correct plumbing |
| Resident → installer device-level delegation grant + revoke | Explicitly one of the four key moments; every commercial multi-tenant access platform (Tapkey, ProdataKey) supports some form of "resident invites a service provider," which validates this is expected, not exotic | MEDIUM | Grant creates a `granted-operator` role assignment scoped to one device resource; revoke deletes it. Must show effect on the *very next* command, not after a cache TTL — that immediacy is the demonstration |
| Tenant isolation (can't see or act cross-tenant) | Table stakes for any multi-tenant SaaS; also one of the four key moments | LOW (mechanism is AXIAM's tenant-bound tokens) but MEDIUM to verify | Requires a negative-test discipline: every list/query endpoint must filter by the caller's tenant, and the demo needs a visible way to *prove* it (e.g., log into tenant B, show tenant A's site 404s or is absent from lists) |
| Device registry + identity per device (device resource in AXIAM + own mTLS cert + own service account) | Standard IoT device lifecycle stage ("Provisioning & Onboarding" — DigiCert, Device Authority); also core to the dogfooding value ("every device has its own AXIAM identity") | MEDIUM-HIGH | CSR flow: simulator generates keypair locally, Management Platform forwards CSR to AXIAM PKI, private key never leaves the simulator host (already decided) |
| Device decommission (delete device → revoke cert / disable service account) | Standard lifecycle stage; without it, deleted devices remain valid credentialed identities — a bad look for an IAM demo specifically | LOW-MEDIUM | AXIAM has cert signing per PROJECT.md; revocation path must exist even if it's "delete the service account, which invalidates its tokens" rather than a full CRL |
| Device shadow with reported + desired state, converging via delta | This is the textbook AWS IoT Device Shadow / Azure Device Twin model — dropping either half (e.g., no offline handling) makes the Twin feel like a dumb command relay, not a "twin" | MEDIUM | Standard pattern: desired set by command, reported set by device ack, delta = diff, cleared on convergence. See device-type table below |
| Offline device handling: shadow retains last known state, commands queue as desired-only until device reconnects | AWS: "shadows work whether the device is online or offline, decoupling backend from device... if offline, the device retrieves changes when it comes back online." This is the single most-cited Device Shadow feature in the ecosystem and it directly powers the "simulator control panel takes devices offline/online" requirement | MEDIUM | UI must visibly distinguish "commanded but not yet applied" (desired ≠ reported) from "confirmed" — this dual-state display is itself a demo asset, not just plumbing |
| Command acknowledgement (device publishes reported state; UI reflects convergence, not just "sent") | Same AWS pattern: a command is "successful" only when reported catches up to desired, not when the command is dispatched | LOW-MEDIUM | Needed so the live device loop (moment 4) reads as *real* control, not fire-and-forget |
| Four device types support their specified commands (see device-type table below) | Fixed by PROJECT.md/DEFINITIONS.md — no product choice here, just completeness | MEDIUM | Each device type needs its own small state machine; details below |
| Intercom ring → answer → unlock flow with timeout and missed-call state | Every commercial video/audio intercom (2N, Ring Intercom, Swiftlane) implements exactly this state machine; it is the recognizable "smart intercom" interaction and validates the cross-resource authorization path (site/building device calling an apartment device) | MEDIUM | State-only per PROJECT.md (no real audio/video) — simplifies enormously versus real intercom products, see device-type detail below |
| Outdoor intercom can only call indoor intercoms within its own site/building | Explicit requirement; also protects tenant isolation (an outdoor intercom in tenant A must never reach an apartment in tenant B) | LOW | Enforced by the resource hierarchy scoping of `intercom:call`, already in the authorization model |
| Live UI updates for shadow state and incoming calls (SSE) | Users expect a "smart home" UI to update without refresh; AWS's own IoT dashboards and every commercial intercom app push instantly. Without this, moment 4 (the live device loop) has to be narrated ("now imagine the light turns on") instead of shown | MEDIUM | SSE is already the architectural choice in PROJECT.md; scope it to per-tenant, per-user-visible-resource channels so it doesn't leak cross-tenant data |
| Live access-decision feed (subject, action, resource, result, reason) | Directly serves moment 1 (cross-role denial) — the denial reason (`no_grant` / `denied_by_rule`) is the entire point of showing AXIAM's authorization model live, not just enforcing it silently | LOW-MEDIUM | Already specified; the feed must be visible in the staff console and toggleable in the resident app per PROJECT.md |
| Seed script producing the exact DEFINITIONS.md minimums (2 tenants, sites/buildings/apartments, users per role, 57 devices/site) | Every one-command demo needs deterministic, restorable data; also the only way the four moments have known credentials to click through | MEDIUM | Must be idempotent and align with `just demo-reset` |
| Reset script (`just demo-reset`) wiping and re-seeding, including re-issuing certs | Explicit requirement; also the safety net for live demos that get into a bad state | MEDIUM-HIGH | Two-stage TLS bootstrap makes this nontrivial (AXIAM bootstrap cert → org CA → reissue all server/service/device certs) — already flagged as a dogfooding-findings risk in PROJECT.md |
| Simulator control panel: ring an intercom, trigger disturbances, take a device online/offline | Explicit requirement; this is the only way to *trigger* the live device loop and the intercom flow without physical hardware | MEDIUM | Needs to target a device by ID/apartment, not just "device type," so the demo script can be deterministic |
| Guided demo script (click-by-click walkthrough of the four moments, with seeded credentials) | Explicit deliverable; without it, presenters improvise live and the "four key moments run reliably" core value has no rehearsed path | LOW (as a document) | This is a documentation deliverable, but the *UI* should be designed so the script's steps map to obvious, discoverable actions (see differentiators) |

### Differentiators (Make the Demo Memorable)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Decision feed surfaced to non-technical viewers, with human-readable denial reasons ("Concierge lacks device:operate on apartment 4B — no grant") | Most commercial products *hide* the authorization engine entirely; making it a first-class, readable UI element is exactly what an IAM vendor demo should differentiate on. It turns an invisible security control into a visible, explainable feature | LOW-MEDIUM (mostly a UI/copy problem once the feed plumbing exists) | Depends on the access-decision feed (table stakes). Map AXIAM's raw reason codes to a short plain-English sentence per (action, resource-type) pair |
| Instant grant/revoke effect demonstrated side-by-side (two browser windows: resident revokes, installer's next click fails live) | Turns an abstract claim ("delegation is revocable") into an unambiguous, timed visual proof — the single most convincing IAM demo pattern (compare to committing a code change and watching a build fail) | LOW (UI choreography, not new backend) | Depends on grant/revoke (table stakes) and the decision feed. This is a *demo script* feature more than a product feature — cheap to add, high payoff |
| Device-level (not device-type or apartment-wide) delegation granularity | Real multi-tenant access platforms (Tapkey, ProdataKey) mostly delegate by *space* (a whole unit or door), not by individual device. Delegating to a specific device (e.g., "the installer can touch this one thermostat, not the lights") showcases AXIAM's fine-grained resource model better than a coarser grant would | LOW (already the chosen design; the "differentiator" is describing/demoing it as such) | Already fixed by PROJECT.md — just worth calling out in the guided script as a talking point, since it's genuinely more granular than the market norm |
| Real per-device mTLS identity, visibly distinct per virtual device (own keypair, own cert, own token) | Most IoT demos fake device identity with a shared API key. Showing 114 devices each with their own AXIAM-issued cert is a strong "this is real PKI, not a toy" moment for technical evaluators, and it is explicit dogfooding value | MEDIUM-HIGH (already required, not optional) | The differentiator is *surfacing* this in the UI (e.g., a device detail panel showing cert serial/expiry/issuer) rather than just doing it invisibly |
| Multi-SDK, multi-language visual banner (Java / Rust / Rust-WASM / C / C++, all hitting the same AXIAM) | Reinforces "one IAM, every stack" — a strong pitch line for a platform team evaluating whether AXIAM fits a polyglot org. Low cost, high narrative value | LOW | Pure presentation layer (e.g., an "under the hood" panel or architecture diagram overlay in the demo script) |
| Simulator-injected faults during a live demo: window-open thermal disturbance, device going offline mid-command | Directly demonstrates the device-shadow offline/desired-vs-reported model (table stakes) *in motion*, which is far more convincing than describing the shadow model in prose. AWS's own docs treat "device retrieves changes on reconnect" as the flagship Device Shadow feature — showing it live is a natural differentiator here | MEDIUM (needs disturbance already implemented for the thermostat sim per PROJECT.md, plus an offline toggle) | Depends on device-twin offline handling (table stakes) and the simulator control panel (table stakes) |
| Guided demo mode as an actual UI overlay/checklist (not just an external doc) pre-selecting the right users/devices for one click each | Removes presenter error risk and cuts moment-to-moment friction during a live pitch; several enterprise demo platforms (e.g., product-led-growth "guided tour" widgets) use this pattern for exactly this reason | MEDIUM | Optional polish on top of the (table stakes) written guided script; only worth building if time remains after the four moments are solid |
| Per-tenant reset (reseed one tenant without wiping the other) | Lets a presenter recover from a mistake in front of tenant-B stakeholders without disturbing a tenant-A environment mid-multi-day-demo | MEDIUM | Not in PROJECT.md's requirements — flag as a nice-to-have research finding, not a commitment |

### Anti-Features (Do Not Build — Or Explicitly Deferred by PROJECT.md)

| Anti-Feature | Why It Looks Appealing | Why It's Wrong Here | Alternative |
|--------------|------------------------|----------------------|-------------|
| Real audio/video for intercom calls | Feels "more real," matches what 2N/Ring actually do | PROJECT.md explicitly scopes calls to state changes only; real AV adds codecs, WebRTC signaling, NAT traversal — none of which touches the IAM story and would dominate the schedule | State-only call flow: `call → ring → answer → unlock`, with clear UI copy ("simulated call") so evaluators don't mistake the omission for a gap |
| Time-bound / expiring installer grants | Feels like a "more secure" delegation model, and is standard in real access-control products (temporary contractor badges) | User explicitly chose revocable-only; AXIAM has no expiry primitive on role assignments, so building it means simulating TTLs in the Management Platform — extra state machine for zero IAM-demo payoff | Revoke-only grants; call out in the guided script that AXIAM's roadmap gap (no time-bound delegation) is itself a dogfooding finding |
| Self-registration / sign-up flows for residents or installers | Common in real proptech onboarding (SmartRent, Tapkey both support self-service) | Explicit out-of-scope; adds an unauthenticated public surface, email verification, invite tokens — a large adjacent feature area not related to demonstrating IAM enforcement | All accounts created by seed scripts or by property managers, exactly as decided |
| Native mobile apps for residents/installers | Real competitors (Ring, Tapkey, SmartRent) ship mobile-first; feels expected for a "resident app" | Explicit out-of-scope (cost of memory/time, doesn't serve the core moments); the demo runs on presenter-controlled machines, not resident phones | Responsive React web app, works fine on a tablet during a live pitch |
| Full observability stack (Prometheus/Grafana/tracing UI) | Standard "production-grade" expectation, and genuinely useful for the Pi resource budget concern | Explicit out-of-scope; costs memory on the 8 GB Pi and doesn't serve any of the four moments — the *access decision feed* already gives IAM-relevant visibility | Structured logs to stdout/files (captured for the dogfooding-findings doc) plus the in-product decision feed, which is the visibility that actually matters for this demo |
| Push/SMS/email notifications for missed calls or grants | Real intercom products (Ring: "missed calls reviewable up to 180 days," push notifications) make this feel table stakes | Requires SMTP/push infrastructure, external service accounts, and delivery reliability work — none of which is IAM-relevant, and PROJECT.md is silent on it, signaling it's not core | In-app missed-call/notification list (via the existing SSE feed and a simple "recent activity" panel), no external delivery channel |
| OTA firmware update pipeline for simulated devices | "Configure devices" is a real requirement, and real IoT platforms treat firmware updates as part of device lifecycle | These are *simulators*; there is no firmware to update, and building a fake OTA pipeline burns effort modeling something with zero observable effect in the demo | "Configure" stays scoped to settings already decided: thermostat limits, intercom→apartment mapping, light groupings — static config changes, not a firmware/version concept |
| A general policy/rule editor exposed in the UI (custom ABAC/ReBAC-style conditions) | Feels like a natural "advanced" feature for an IAM product demo, and some competitors (e.g., attribute-based commercial access platforms) do expose rule builders | AXIAM is RBAC-over-hierarchy with no ABAC/ReBAC (confirmed against `../axiam` in PROJECT.md); building a UI that implies more expressiveness than AXIAM has would misrepresent the product to evaluators — the opposite of the demo's purpose | Show the real model: roles assigned at real resource-tree nodes, with cascading and the common-area trick for the apartment-operate ban. If evaluators want ABAC, that's a dogfooding-findings entry, not a UI feature |
| Billing/subscription/plan management per tenant | Present in almost every real multi-tenant SaaS | Zero relevance to the IAM story; the "tenant" here exists to show isolation, not commerce | None needed — tenants are provisioned by the org-scope service account during seed |
| General-purpose ticketing/helpdesk for installer work orders | Feels like a natural companion to "installer visits apartment to configure a device" | Out of scope; a work-order system is a whole adjacent domain (SLAs, statuses, assignment) with no IAM enforcement surface beyond what device `configure`/`manage` permissions already demonstrate | None — installer's AXIAM-scoped `device:manage`/`device:configure` permissions are the entire story that matters here |

## Feature Dependencies

```
Site/Building/Apartment CRUD (Mgmt Platform)
    └──requires──> AXIAM resource-tree sync on every write
                       └──requires──> Role assignment API (property-manager, installer, concierge, resident)
                                          └──enables──> Cross-role denial demo (moment 1)
                                          └──enables──> Resident→installer grant/revoke (moment 2)
                                                            └──requires──> Device registry (a device resource must exist to grant on)

Device registry + identity (cert issuance, service account)
    └──requires──> AXIAM PKI CSR-signing flow (two-stage TLS bootstrap must exist first)
    └──enables──> Device Twin shadow registration
                      └──enables──> Command dispatch (user command → CheckAccess → MQTT → device)
                                        └──enables──> Shadow convergence (desired/reported/delta)
                                                          └──enables──> Live device loop (moment 4)
                                                          └──enables──> Offline-handling demo (differentiator: simulator fault injection)

Device Twin command dispatch
    └──enables──> Access-decision feed entries
                      └──enables──> Cross-role denial visibility (moment 1)
                      └──enhances──> Instant grant/revoke visual proof (differentiator)

Intercom call flow
    └──requires──> Device Twin command routing (to route `incoming_call` cross-device)
    └──requires──> Resource-hierarchy scoping (outdoor intercom → apartments in its own site/building only)
    └──requires──> Real-time UI (SSE) to ring the resident app live
    └──enhances──> Tenant isolation demo (an outdoor intercom must never reach another tenant's apartment)

Seed script
    └──requires──> All of the above APIs to exist (it is a consumer, not a foundation)
    └──enables──> Guided demo script (needs known, stable credentials/IDs)
    └──enables──> Reset script (`just demo-reset` = wipe + reseed + recert)

Simulator control panel
    └──requires──> Device registry + Device Twin command dispatch
    └──enables──> Intercom ring trigger, offline/online toggle, thermal disturbance
```

### Dependency Notes

- **AXIAM resource-tree sync requires being solved before anything else.** Every other feature (roles, grants, denial, device operate) sits on top of the resource hierarchy being correct and live-synced. If this phase is rushed, moments 1–3 all degrade simultaneously (denials look wrong, grants don't take effect, tenant isolation can't be trusted).
- **Device registry/identity must exist before the Device Twin can register a shadow, and both must exist before the intercom call flow or any command dispatch works.** This chains device provisioning → shadow → command → decision feed into one build order; the roadmap should not attempt device operation before certs and shadow registration are solid.
- **The two-stage TLS bootstrap (bootstrap cert → org CA → reissue everything) is a hard dependency of the reset script and of device provisioning.** It should land early — everything downstream (device certs, service certs, browser trust) depends on the org CA existing.
- **The access-decision feed enhances but does not block the four moments** — moments 1 and 2 are technically demonstrable via UI state changes alone (a denied button, a grant taking effect), but the feed is what makes the *reason* legible, which is why it's table stakes rather than a pure differentiator.
- **Real-time UI (SSE) is a shared dependency of the live device loop and the intercom flow** — building it once, generically (per-tenant, per-resource channels), avoids two bespoke real-time implementations.
- **Anti-feature "policy/rule editor" conflicts with "showing AXIAM as it exists today."** Building a rule editor UI would misrepresent AXIAM's actual RBAC-over-hierarchy model to evaluators — this is a hard conflict, not just wasted effort.

## Device Type Detail: Commands, State, Events

Fixed by PROJECT.md/DEFINITIONS.md; provided here as the concrete shadow-model contract each device type needs, following the AWS IoT Device Shadow desired/reported/delta pattern.

| Device type | User commands (→ desired state) | Reported/shadow state fields | Notable events |
|---|---|---|---|
| **Outdoor intercom** | `unlock` (momentary actuator pulse) | `lock_state` (locked/unlocked, auto-relocks after N seconds), `online` | `call_initiated` (device-originated, via `check_as`, not a user command), `call_timeout`, `unlock_result`, `online`/`offline` |
| **Indoor intercom** | `answer` | `call_state` (idle/ringing/in_call), `online` | `incoming_call` (pushed by the Twin when an outdoor intercom calls), `call_answered`, `call_missed` (ring timeout elapsed), `call_ended` |
| **Lights** | `on`/`off`, `dim` (0-100%), set RGB color | `power`, `brightness`, `color`, `online` | `state_changed` (on ack), `online`/`offline` |
| **Thermostats** | `on`/`off`, `mode` (heat/cool), `target_temperature` | `power`, `mode`, `target_temp`, `current_temp` (simulated, reported-only — never settable by a user command), `online` | `state_changed`, `disturbance_detected` (simulated "window open," reported-only, drives current_temp drift toward ambient), `online`/`offline` |

Notes:
- `current_temp` and `call_state`/`lock_state` transitions are **reported-only** fields — never part of `desired`. Only `power`, `mode`, `target_temperature`, `brightness`, `color`, and the momentary `unlock`/`answer` actuations belong in `desired`. Modeling this distinction correctly in the Twin's schema avoids a common shadow-model bug: treating simulated/sensor values as commandable.
- The outdoor intercom's `unlock` is momentary (auto-reverts to `locked` after a timeout) rather than a persistent desired-state toggle — this matches real intercom/access-control UX ("confirm to unlock," per Ring Intercom's model) and avoids a shadow that claims a door is permanently unlocked.
- Ring/Aiphone/2N-class real products additionally support call *forwarding* to a second resident or a concierge fallback; PROJECT.md does not require this and it is reasonable to treat as future/deferred rather than in-scope — it would add real value (a "missed call escalates to concierge" moment) but isn't part of the four key moments.

## Intercom Call Flow (State Machine)

```
[idle] --(control panel triggers ring, via outdoor intercom sim)--> call_initiated
   check_as: outdoor intercom has intercom:call on its site/building? --deny--> call rejected, logged, back to idle
   --allow--> incoming_call pushed via SSE to indoor intercom / resident app --> [ringing]

[ringing] --(ring timeout elapses, no answer)--> [missed] --> logged as missed call, back to idle
[ringing] --(resident answers, checked as device:operate on indoor intercom)--> [in_call]

[in_call] --(resident unlocks, checked as device:operate on outdoor intercom)--> lock_state=unlocked (momentary)
[in_call] --(resident ends / no further action)--> [idle]
```

Recommended demo-tuned timeout: commercial intercoms use roughly 20-30 second ring timeouts (2N configuration docs expose both a "ring time limit" and a "connecting time limit" as separate settings). For a live demo, a **10-15 second ring timeout** is recommended so a deliberately-triggered "missed call" moment doesn't stall a presentation, while still reading as a real timeout rather than an instant failure. This is a suggestion for requirements definition, not a PROJECT.md commitment.

## Audit & Access-Decision Visibility

- **Table stakes:** every `CheckAccess`/`check_as` call the platform makes is appended to a live feed entry: `{subject, action, resource, result (allow/deny), reason, timestamp}`. Streamed via SSE to the staff console always, and toggleable in the resident app (per PROJECT.md).
- **Differentiator:** translate raw reason codes (`no_grant`, `denied_by_rule`) into a short human sentence keyed by (role, action, resource-type) — e.g., "Concierge cannot operate apartment devices (no grant)." This is cheap (a lookup table) and is what makes moment 1 legible to a non-technical prospect rather than just a red toast notification.
- **Complexity:** LOW-MEDIUM once the underlying CheckAccess calls are already being made for enforcement — this is largely a matter of also logging/streaming every decision, not a new authorization mechanism.
- **Dependency:** requires the resource-tree sync and role model to be correct first, otherwise the feed will surface confusing or wrong reasons.

## Demo Tooling

| Feature | Category | Complexity | Notes |
|---|---|---|---|
| Seed script (idempotent, matches DEFINITIONS.md minimums exactly) | Table stakes | MEDIUM | Must create 2 tenants, sites/buildings/apartments, all role assignments, and 114 devices with certs — this is a lot of AXIAM API calls; consider a declarative seed manifest (YAML/JSON) consumed by a small seeding tool rather than a hand-written imperative script, for maintainability under `demo-reset` |
| Reset script (`just demo-reset`) | Table stakes | MEDIUM-HIGH | Must repeat the two-stage TLS bootstrap; PROJECT.md already flags this as a place where things can go wrong and should be recorded in dogfooding findings |
| Simulator control panel (ring, disturbance, online/offline toggle) | Table stakes | MEDIUM | Needs device-level targeting (by ID/apartment), not just device-type-level, to support a deterministic guided script |
| Guided demo script document | Table stakes | LOW | Documentation deliverable; should reference exact seeded credentials/apartment numbers so it never needs live adjustment |
| Guided demo mode as UI overlay | Differentiator | MEDIUM | Optional; only pursue after the four moments and their manual click-paths are solid |
| Per-tenant reset | Differentiator | MEDIUM | Not required; flag as a nice-to-have if multi-day/multi-audience demos are anticipated |

## MVP Definition

### Launch With (v1) — the four key moments and their direct dependencies

- [ ] Site/building/apartment CRUD with live AXIAM resource-tree sync — foundation for every other moment
- [ ] Role assignment at the correct scopes (property-manager, installer, concierge, resident) — enables moment 1 and moment 3
- [ ] Device registry, identity issuance (cert + service account), decommission — foundation for moment 4
- [ ] Device Twin shadow (reported/desired/delta) for all four device types with their exact specified commands — moment 4
- [ ] Command dispatch through `CheckAccess`, MQTT publish, shadow update, SSE push — moment 4
- [ ] Resident→installer device-level grant + revoke, with immediate effect — moment 2
- [ ] Tenant isolation enforced and demonstrably provable (no cross-tenant visibility or action) — moment 3
- [ ] Intercom call flow (ring/answer/unlock/timeout/missed) scoped to same site/building — validates cross-resource authorization, a strong moment-1/moment-4 hybrid
- [ ] Live access-decision feed with human-readable reasons — makes moment 1 legible
- [ ] Seed script + reset script matching DEFINITIONS.md minimums
- [ ] Simulator control panel (ring, disturbance, online/offline)
- [ ] Guided demo script document

### Add After Validation (v1.x)

- [ ] Instant grant/revoke two-window visual proof choreography in the guided script
- [ ] Human-readable denial reason mapping table (beyond raw AXIAM codes)
- [ ] Device detail panel surfacing cert metadata (serial/issuer/expiry) for technical evaluators
- [ ] Multi-SDK "under the hood" architecture overlay panel
- [ ] Simulator fault injection woven explicitly into the guided script as a scripted beat

### Future Consideration (out of this milestone's scope entirely)

- [ ] Per-tenant reset
- [ ] Guided demo mode as an interactive UI overlay/checklist
- [ ] Call forwarding / concierge fallback on missed calls (real intercom products have this; not in PROJECT.md)
- [ ] Time-bound delegation (blocked on an AXIAM product gap, not a demo choice — log as dogfooding finding instead)

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---|---|---|---|
| Resource-tree CRUD + AXIAM sync | HIGH | MEDIUM | P1 |
| Role assignment at correct scopes | HIGH | LOW-MEDIUM | P1 |
| Device identity/cert issuance + decommission | HIGH | MEDIUM-HIGH | P1 |
| Device Twin shadow (all 4 device types) | HIGH | MEDIUM | P1 |
| Command dispatch + CheckAccess + SSE | HIGH | MEDIUM | P1 |
| Resident→installer grant/revoke | HIGH | MEDIUM | P1 |
| Tenant isolation guarantee | HIGH | LOW-MEDIUM | P1 |
| Intercom call flow (ring/answer/unlock/timeout/missed) | HIGH | MEDIUM | P1 |
| Live access-decision feed (raw) | HIGH | LOW-MEDIUM | P1 |
| Seed + reset scripts | HIGH | MEDIUM-HIGH | P1 |
| Simulator control panel | HIGH | MEDIUM | P1 |
| Guided demo script (document) | HIGH | LOW | P1 |
| Human-readable denial reasons | MEDIUM-HIGH | LOW | P2 |
| Grant/revoke two-window demo choreography | MEDIUM-HIGH | LOW | P2 |
| Device cert detail panel | MEDIUM | LOW-MEDIUM | P2 |
| Multi-SDK architecture overlay | MEDIUM | LOW | P2 |
| Simulator fault injection as scripted beat | MEDIUM | LOW (if fault injection already exists) | P2 |
| Per-tenant reset | LOW-MEDIUM | MEDIUM | P3 |
| Guided demo UI overlay/checklist | LOW-MEDIUM | MEDIUM | P3 |
| Call forwarding / concierge fallback | LOW (not requested) | MEDIUM | P3 |

## Competitor / Adjacent-Product Feature Analysis

| Feature area | AWS IoT Device Shadow / Azure Device Twin | Commercial multi-tenant intercom/access (2N, Ring Intercom, Tapkey, SmartRent) | Our approach |
|---|---|---|---|
| Device state model | Reported/desired/delta JSON documents, convergence on device ack | Mostly opaque to the end product (lock state, online/offline only) | Adopt the reported/desired/delta pattern explicitly for all 4 device types, including reported-only fields for sensor-like values (current_temp) |
| Offline handling | Desired state persists; device fetches delta on reconnect | Devices typically show "offline" and block commands until reconnect | Match AWS pattern (queue desired, apply on reconnect) — this is our differentiator opportunity via simulator fault injection |
| Delegation | N/A (not an access-control product) | Delegate by unit/door (coarse), sometimes time-bound (real access control norm) | Delegate by individual device (finer than market norm), revocable-only (coarser than market norm on expiry, by explicit choice) — worth narrating both differences in the guided script |
| Multi-tenancy | N/A (AWS accounts, not multi-tenant SaaS) | Property-level tenancy, usually with a portfolio admin role | AXIAM org→tenant model maps directly; isolation is a first-class demo moment here, unlike most competitors where it's assumed infrastructure |
| Call flow | N/A | Real audio/video, forwarding, PSTN fallback, 180-day missed call history | State-only (ring/answer/unlock/timeout/missed), no AV, no forwarding — deliberately reduced scope, explained above |
| Access visibility / audit | N/A (shadow service has no authz-decision concept) | Activity logs (who unlocked what, when) but rarely *why a request was denied* | Live decision feed with denial reasons is our clearest point of differentiation versus every competitor in this table — none of them expose an authorization *reasoning* trail to end users |

## Sources

- Primary and authoritative for this project: `.planning/PROJECT.md` and `DEFINITIONS.md` (both read in full; all scope, role, and permission decisions in this document defer to them)
- [Mechanisms for IoT commands, control, and configuration — AWS IoT Blog](https://aws.amazon.com/blogs/iot/mechanisms-for-iot-commands-control-and-configuration/)
- [AWS IoT Device Shadow service — AWS IoT Core docs](https://docs.aws.amazon.com/iot/latest/developerguide/iot-device-shadows.html)
- [Retaining device state while offline with Device Shadows — AWS IoT Core docs](https://docs.aws.amazon.com/iot/latest/developerguide/iot-shadows-tutorial.html)
- [Device Shadows — MQTT Topics, IoT Atlas](https://iotatlas.net/en/implementations/aws/device_state_replica/device_state_replica1/)
- [AWS IoT Device Shadow service — Well-Architected IoT Lens](https://docs.aws.amazon.com/wellarchitected/latest/iot-lens/aws-iot-device-shadow-service.html)
- [Smartphone Intercom App: 2026 Playbook — Forasoft](https://www.forasoft.com/blog/article/smartphone-intercom-app-features-benefits)
- [2N IP intercom configuration manual — call settings/timeouts](https://wiki.2n.com/hip/conf/latest/en/5-konfigurace-interkomu/5-3-volani/5-3-1-obecne-nastaveni)
- [Ring Intercom Audio — product page](https://ring.com/eu/en/products/intercom)
- [Ring Intercom — Intercom Calls support article](https://ring.com/gb/en/support/articles/h0bsk/intercom-calls)
- [Using Remote Unlock with your Ring Intercom device](https://ring.com/support/articles/rsmh2/Using-Remote-Unlock-with-your-Ring-Intercom-device)
- [A Smart Way to Access Multitenant Residential Buildings — Tapkey](https://tapkey.com/en/a-smart-way-to-access-multitenant-residential-buildings/)
- [Multifamily Apartment Access Control Systems: A Complete Guide — ICT](https://www.ict.co/blog/multifamily-apartment-access-control-systems-a-complete-guide/)
- [Multi-Tenant Residential Integrations — ProdataKey](https://www.prodatakey.com/integrations/multi-tenant-residential)
- [Cloud-Based Access Control for Multifamily — DOOR](https://door.com/article/cloud-based-access-control-for-multifamily-a-guide-for-modern-apartment-buildings)
- [Apartment Access Control Systems — SmartRent](https://smartrent.com/products/access-control/)
- [Securing IoT Device Lifecycle Management: Best Practices — Device Authority](https://deviceauthority.com/securing-iot-device-lifecycle-management-best-practices-for-each-stage/)
- [IoT Provisioning Process: Secure Onboarding and Lifecycle Management — Device Authority](https://deviceauthority.com/iot-provisioning-process-secure-onboarding-and-lifecycle-management-of-devices/)
- [IoT & Device Certificates — EverTrust Digital Trust Guide](https://evertrust.io/guide/iot-device-certificates/)
- [Simplify IoT Device Identity and Onboarding at Scale — DigiCert](https://www.digicert.com/blog/simplifying-iot-device-identity-and-onboarding)

---
*Feature research for: AXIAM Domo Demo (multi-tenant IoT property management, IAM showcase)*
*Researched: 2026-09-19*
</content>
