# Pitfalls Research

**Domain:** Multi-tenant IoT home-automation demo on a private IAM (AXIAM), Docker Compose on amd64 + Raspberry Pi 5 arm64
**Researched:** 2026-09-19
**Confidence:** HIGH (PKI, RBAC, token and cache claims are verified against `axiam` source and docs; RabbitMQ MQTT-plugin and browser-TLS claims are verified against current vendor documentation; SDK claims are verified against the SDK CONTRACT.md/README files)

## Critical Pitfalls

### Pitfall 1: AXIAM's PKI cannot issue a SAN — every AXIAM-issued server cert will fail modern browser TLS

**What goes wrong:**
The plan anchors every certificate — including Caddy's and AXIAM's own server TLS — in the AXIAM organization CA. AXIAM's leaf-certificate issuance (`axiam-pki::cert::leaf_params`, shared by both `CertService::generate` and `CertService::sign_csr`) hard-codes **no `subjectAltName`, no `keyUsage`, no `extendedKeyUsage`, on either path, ever**: the source comment says so explicitly ("No subjectAltName, no key usage, no extended key usage — deliberately, and identically on both leaf paths"). `sign-csr` additionally *refuses with 400* any CSR that even requests those three extensions, on every CA custodian — this isn't a gap that Vault custody can route around. `CreateCertificateRequest` (the server-generates-the-key path) likewise has only a `subject: String` field; there is no SAN parameter to pass.
Chrome removed CN-fallback matching in v58 (2017), Firefox followed (v48/101); neither browser will accept a certificate that has no SAN extension, no matter how correctly the CA is imported into the OS/browser trust store. Reaching Caddy at the Pi's LAN IP or a `.local` hostname over HTTPS in Chrome/Firefox/Chromium will show `NET::ERR_CERT_COMMON_NAME_INVALID` even with a perfectly-trusted root.

**Why it happens:**
The constraint "every certificate is issued by the AXIAM CA" was set before checking what AXIAM's issuance code actually emits. It's an easy trap because CN-only certs still work for rustls-to-rustls and libcurl-to-OpenSSL mTLS pairs where a custom or lenient verifier is used (device auth checks a SHA-256 fingerprint, not the hostname) — so the gap only surfaces at the one connection that matters most for a "prospect" demo: a human opening a browser.

**How to avoid:**
Do not treat "issued by AXIAM's CA" and "usable by a browser" as the same requirement. Options, in order of fit for this project (SDK/AXIAM extension is out of scope):
- Terminate browser-facing TLS (Caddy) with a certificate whose hostname verification the browser can actually pass. Since AXIAM cannot add a SAN, the only way to keep "every cert issued by the AXIAM CA" literally true is to accept that Caddy's cert will be **CN-only and browsers will reject it** — so budget time in the PKI phase to test this on both real target browsers (Chromium on the Pi's own desktop if any, and the evaluator's laptop browser) before promising it works, and have a documented, demoed workaround ready (e.g., a one-time per-browser exception click-through, which still shows a scary interstitial and needs to be in the demo script so it isn't a surprise on stage).
- For **non-browser** TLS clients that this project's own code controls (Rust Twin, Java Management Platform, Rust/C/C++ simulators, RabbitMQ AMQPS/MQTT), you are not stuck with the OS/browser default verifier: rustls (`ServerCertVerifier`), Java (`HostnameVerifier`/`TrustManager`), and libcurl (`CURLOPT_SSL_VERIFYHOST` combined with a custom verify callback or fingerprint pinning) can all be configured to validate the AXIAM leaf by CN or by SHA-256 fingerprint instead of by SAN. This is legitimate here because every peer's identity is already known out-of-band (it's a fixed demo topology, not the open internet) — but it must be deliberate, documented per service, and never silently done by disabling verification (`verify_none`/`InsecureSkipVerify`) since that also defeats the mTLS client-auth AXIAM performs.
- Confirm this finding against the actual AXIAM PKI code again after any AXIAM upgrade — this is exactly the kind of gap that belongs in the dogfooding-findings document with a reproduction (a `sign-csr` call with a SAN-bearing CSR returns 400).

**Warning signs:**
- `openssl x509 -in server.pem -noout -text | grep -A1 "Subject Alternative Name"` on any AXIAM-issued leaf comes back empty.
- Chrome/Firefox show `ERR_CERT_COMMON_NAME_INVALID` against Caddy even after importing the AXIAM root.
- A `sign-csr` request that includes a `subjectAltName` extension in the CSR returns HTTP 400.

**Phase to address:**
The PKI/TLS bootstrap phase, before any browser-facing service is stood up. This must be resolved (with an explicit, demoed decision) before the portal phases start, because it changes how Caddy's server certificate is obtained and how every browser in the demo room needs to be prepped beforehand.

---

### Pitfall 2: A single deny rule anywhere in the hierarchy silently kills the resident→installer grant flow

**What goes wrong:**
AXIAM's authorization engine is deny-override "in one pass": `engine.rs` implements "deny wins over every allow, matched or not, at any depth" (verified directly in `axiam-authz::engine`). The project's own design deliberately avoids deny rules by keeping devices under common-area nodes rather than directly under sites/buildings, specifically because "a deny would also override resident→installer grants, since deny wins at any depth." That's correct today — but it's a structural invariant, not a one-time decision. Any later feature (e.g., "let a property manager block a specific installer entirely," "quarantine a compromised device," "an emergency site lockdown") that reaches for a deny rule anywhere at or above an apartment/device node will silently and permanently block every `granted-operator` assignment under it, and the failure mode is *not* an error — the installer's command is just denied with no obvious link back to the new deny rule.

**Why it happens:**
Deny-override is the correct default-secure choice, and reaching for "just add a deny" feels like the natural way to implement any future exclusion feature. The cascading interaction with delegated grants is non-obvious unless someone has read the engine's precedence rules.

**How to avoid:**
Treat "no deny rules are used, anywhere, ever, in this resource tree" as a hard architectural constraint alongside the common-area-node design, and enforce it structurally: model every future exclusion as removing/not-granting an allow (or scoping a `granted-operator` assignment more narrowly) rather than adding a deny. Add this constraint to whatever ADR/decision log the project keeps, not just to institutional memory.

**Warning signs:**
- Any `RequirePermission`/grant-creation code path in the Management Platform that creates a `Deny` role or rule.
- An installer's previously-working grant starts returning `denied_by_rule` (not `no_grant`) after an unrelated feature ships — `denied_by_rule` vs `no_grant` is exactly the AXIAM-provided signal that distinguishes "a deny fired" from "there's no grant," and the demo's own decision feed already surfaces this reason code, so this should never go unnoticed in testing.

**Phase to address:**
The RBAC/authorization-model phase (resource tree + role design), and re-verified in any phase that adds a new authorization rule (installer suspension, device quarantine, etc.).

---

### Pitfall 3: There is no refresh token for device mTLS login — 114 devices must fully re-authenticate on a fixed cycle, and naive implementations create a reconnect storm

**What goes wrong:**
`POST /api/v1/auth/device` (`device_auth` handler) returns only `access_token` + `expires_in`; the `DeviceAuthResponse` type carries no `refresh_token` field. Unlike the browser/user login flow (which does get a refresh token and cookie-based session), a device's "refresh" is a full mTLS re-handshake against `/api/v1/auth/device`, not a cheap `grant_type=refresh_token` call. The default `access_token_lifetime_secs` is **900 seconds (15 minutes)**, configurable per tenant. With 114 devices split across 3 simulator processes, a naive "check token age, re-auth when it's stale" loop that starts all devices at once (e.g., right after `just demo-reset`) will re-authenticate all ~114 clients within the same few-second window every 15 minutes for the life of the demo — a burst of simultaneous TLS 1.3 + mTLS handshakes against a single AXIAM instance on the same Pi that's also serving every other request.

**Why it happens:**
"Refresh" reads like an OAuth2 refresh-token exchange because that's the vocabulary AXIAM uses everywhere else; the device path quietly isn't that. Simulator code that copies the pattern from the user-login flow, or that just does "re-call login when a 401 arrives," will not proactively refresh and will drop the in-flight command/state-report that triggered the 401 instead of retrying it after re-auth.

**How to avoid:**
- In each simulator host, track each device's token expiry locally and re-authenticate proactively (e.g., at 80% of `expires_in`), not reactively on 401.
- Jitter the re-auth schedule per device (e.g., `expires_in * 0.8 + random(0, expires_in * 0.15)`) so 114 devices don't converge on the same instant, especially right after a `demo-reset` where every device started its clock at the same moment.
- On a 401 mid-command, re-authenticate and retry the specific request rather than dropping it — this is directly relevant to the "flaky timing in live demos" risk.
- Read `access_token_lifetime_secs` from tenant config rather than hard-coding 900s, since a tenant admin (or the seed script) may change it.

**Warning signs:**
- CPU/latency spikes on the Pi at regular intervals matching the token TTL.
- Devices going transiently "offline" in the Twin's shadow view on a period matching `expires_in`.
- Command failures that correlate with the 15-minute mark rather than with anything a user did.

**Phase to address:**
The simulator-host and Device-Twin phases (device auth/session lifecycle design), verified during the live-device-loop demo-moment phase.

---

### Pitfall 4: AXIAM's RabbitMQ has zero MQTT history — enabling the plugin on AXIAM's own shared broker is genuinely new, higher-risk territory

**What goes wrong:**
A full search of the `axiam` repository (source, docs, k8s manifests, Docker configs) turns up **no reference to MQTT anywhere** — no `rabbitmq_mqtt` plugin enablement, no MQTT listener, no MQTT-related code in any SDK. AXIAM's own RabbitMQ StatefulSet also documents, in its own comments, exactly why vhost isolation matters on a shared broker: "`T-131`: a dedicated vhost rather than the default `/`... On the default `/` any other user with default-vhost permissions shares AXIAM's namespace — which matters most on a broker AXIAM does not have to itself." AXIAM already dedicates a vhost (`axiam`) to itself for this reason. This project reuses that same broker instance for device MQTT traffic on a second vhost (`domo`) — which is the right mitigation, but it means: (a) the MQTT plugin, its listener (1883/8883), and its TLS config are being turned on for the first time in this whole codebase's history, with no precedent to compare against if something misbehaves; (b) AXIAM's own control-plane traffic (the `axiam.authz.cache.invalidate` fanout, audit events, webhook retries — all on the `axiam` vhost) shares the same Erlang node, and a misconfigured MQTT listener or a resource exhaustion in the `domo` vhost (e.g., unbounded retained-message growth, a runaway publish loop from a buggy simulator) can still starve the whole node's CPU/memory/file descriptors even though vhosts are a permission boundary, not a resource-isolation boundary.

**Why it happens:**
"No extra broker on the Pi" (an explicit key decision) is the right memory-budget call, but it converts what would otherwise be an isolated blast radius (a dedicated MQTT broker) into a shared one.

**How to avoid:**
- Enable `rabbitmq_mqtt` explicitly (`rabbitmq-plugins enable rabbitmq_mqtt`) as part of setup, never assume it's on by default in the `rabbitmq:4-management-alpine` image AXIAM's own manifests use.
- Set `mqtt.vhost = domo` (or equivalent) so all device MQTT connections land on the dedicated vhost without relying on per-cert `mqtt_default_vhosts` mapping (which needs a Distinguished-Name pattern, since AXIAM certs carry no SAN to key off of — see Pitfall 1).
- Set resource limits that protect the shared node: per-vhost `max-connections`, `max-queues`, and message/queue TTLs on `domo`, so a runaway simulator can't starve AXIAM's own queues.
- Load-test the MQTT plugin's interaction with AXIAM's own AMQP traffic (cache invalidation fanout, audit events) under the full ~115-connection device load before treating "one broker, two vhosts" as validated, since this exact combination has never run before in this codebase.

**Warning signs:**
- AXIAM AMQP cache-invalidation health-check messages ("`axiam.authz.cache.invalidate` self-echo") slow down or drop once MQTT connections ramp up.
- `rabbitmqctl list_connections` shows connections/channels approaching Erlang process or file-descriptor ceilings.

**Phase to address:**
The infra/messaging phase that stands up the `domo` vhost and MQTT plugin — this is exactly the item PROJECT.md already flags as "decided in phase research," and this pitfall is the reason to timebox that research early rather than late.

---

### Pitfall 5: Per-device RabbitMQ authorization at 114+ devices is a real provisioning system, not a config file

**What goes wrong:**
RabbitMQ's MQTT plugin supports certificate-based login (`EXTERNAL` mechanism, `ssl_cert_login_from = common_name`) — and because AXIAM's `sign-csr` preserves whatever CN the CSR asked for ("the subject is kept, not restated"), each device's CN survives onto its MQTT client certificate. But RabbitMQ's *internal* auth backend still needs each of those CN-derived identities to resolve to something it recognizes for topic-permission checks (`rabbitmqctl set_topic_permissions` per user, or a DN-pattern via `mqtt_default_vhosts`/`mqtt_port_to_vhost_mapping`). With 114 devices — added and removed live from the UI, "picked up... automatically... without restarts" per the requirements — this is a nontrivial provisioning surface: every device create/delete in the Management Platform must also create/delete a matching RabbitMQ user (or a pattern-based permission keyed to a stable per-device identifier) and its topic-scoped ACL (publish only to its own `domo/{tenant}/{device}/...` topic, subscribe only to its own command topic), kept in lockstep with the AXIAM resource lifecycle.

**Why it happens:**
It's easy to prototype topic auth with 2-3 hand-created RabbitMQ users and forget that the real system needs 114+, created/destroyed dynamically, with no restart — RabbitMQ's management API (or `rabbitmqctl`) calls for this need to be wired into whatever service handles device provisioning (see PROJECT.md's device-provisioning flow), not left as a manual setup-script step.

**How to avoid:**
Design topic authorization as part of the device-provisioning flow from the start (likely in the Management Platform or Twin, whichever owns "device added → cert issued → broker access granted"), using RabbitMQ's HTTP Management API to create the vhost-scoped user/permission atomically with cert issuance. Prefer a topic-pattern scheme with a stable per-device path segment (device UUID, not something that can collide) so ACLs are mechanical to generate.

**Warning signs:**
- A device added through the UI shows up in the Twin's device registry but never publishes/receives on MQTT because no RabbitMQ permission was ever created for it.
- Manual `rabbitmqctl` invocations creep into the setup scripts as a substitute for automated per-device provisioning — a sign this was deferred rather than designed.

**Phase to address:**
The device-provisioning phase (device add/remove flow), in the same pass as CSR issuance — these two steps should be transactionally close together.

---

### Pitfall 6: Raspberry Pi 5 / 8 GB is tight even before this project's own services, and the wrong storage medium makes it worse

**What goes wrong:**
AXIAM's own Raspberry Pi 5 deployment guide treats 8 GB as "required, **not recommended** — required," even for a leaner k3s-based topology than this project's Compose stack, and separately calls out that SurrealDB's `surrealkv` storage engine "on an SD card is miserable" — the guide requires booting from NVMe/SSD. This project stacks a JVM (Management Platform), Rust Twin, SurrealDB, RabbitMQ, and PostgreSQL on the same 8 GB, with a stated ≤4 GB resident-memory budget. Any one of these growing beyond its expected footprint (a JVM heap that isn't actually capped the way `-Xmx512m` implies once metaspace/threads/GC overhead are counted, a RabbitMQ node with 115 MQTT connections plus AXIAM's own AMQP load, SurrealDB's cache) risks OOM-killing a container mid-demo, and a Pi under memory pressure degrades unpredictably rather than failing cleanly.

**Why it happens:**
Memory budgets are usually estimated from idle/cold-start footprints, not from the full four-key-moment demo load (115 device connections, live SSE streams to multiple browser tabs, concurrent CheckAccess calls). The SD-card-vs-NVMe distinction is easy to miss if the Pi is set up quickly from a pre-imaged SD card for convenience.

**How to avoid:**
- Boot the Pi from NVMe/SSD, not SD card, for anything touching SurrealDB or Postgres storage — treat this as non-negotiable given AXIAM's own team already hit this.
- Set explicit container memory limits for every service (not just a JVM heap flag) and test the full four-demo-moment sequence under `docker stats`/`free -h` on the actual Pi hardware, not just on the amd64 dev machine, before declaring the resource budget met.
- Measure RabbitMQ's actual RSS with ~115 MQTT connections plus AXIAM's own AMQP connections live, not just at idle — AXIAM's own k8s manifest budgets RabbitMQ at only 512 Mi under a much lighter (k3s-internal, no MQTT) load, which is a useful floor but not a ceiling for this project's usage.
- Confirm arm64 images exist (or can be built) for every component: SurrealDB, RabbitMQ, PostgreSQL, and Caddy all publish official `linux/arm64` images; AXIAM's own images are already documented as multi-arch. The gap to actually verify is this project's *own* images (Management Platform JVM image, Device Twin Rust image) — building and running those under real arm64, not just declaring a `linux/arm64` platform target in a Dockerfile that was only ever tested on amd64.

**Warning signs:**
- `dmesg`/`journalctl` on the Pi showing OOM killer activity during a demo run-through.
- SurrealDB or Postgres query latency spiking under concurrent load if still on an SD card.
- A container that runs fine on the XPS but crashes or throttles only on the Pi.

**Phase to address:**
The deployment/infra phase (Compose + resource limits), verified explicitly on real Pi hardware before any phase is marked done — "runs on my amd64 laptop" must never stand in for "runs on the Pi" in this project.

---

### Pitfall 7: The C SDK is synchronous/blocking with one client per identity, and AXIAM's SDKs carry no MQTT support at all — each simulated device needs two independent client stacks

**What goes wrong:**
The C SDK's own contract states "Synchronous canonical calls only (`axiam_*`); no async surface" for v1.0, and the C SDK's transport is one `libcurl` easy-handle per client identity with per-client cookie/mTLS state — meaning a C process hosting ~38 virtual lights needs 38 independent client objects, and a naive single-threaded loop calling blocking `axiam_*` functions for each device in turn will serialize all of them (a slow AXIAM response for device #1 stalls devices #2-38's token refresh/report calls too). Separately, **no AXIAM SDK (C, C++, Rust, Java, WASM) has any MQTT support** — MQTT pub/sub for device state/commands is necessarily a *second*, unrelated client library (e.g., Paho MQTT C/C++, or an MQTT crate for Rust) alongside the AXIAM SDK used only for REST mTLS login. Every simulated device therefore juggles two independent TLS stacks (one for AXIAM auth over libcurl/OpenSSL, one for MQTT, possibly over a different TLS backend) and, in C particularly, needs its own concurrency model (thread-per-device or a work queue) since the AXIAM SDK gives none for free.

**Why it happens:**
It's tempting to assume "the SDK handles the network" covers the whole device story, when in this architecture the SDK only ever covers login/cert issuance — the actual device runtime traffic (state, commands) goes over MQTT entirely outside any AXIAM SDK.

**How to avoid:**
- Design each simulator host's concurrency model explicitly: one thread (or a small thread pool with a work queue) per virtual device is the straightforward fit for the C SDK's blocking model; the C++ and Rust hosts can use their own idioms (Rust: async is a real option there) but should be deliberate about it too.
- Pick MQTT client libraries per language up front (e.g., Paho for C/C++, a Rust MQTT crate for the thermostat host) as part of the tech-stack decision, and confirm each one supports mTLS with the AXIAM-issued Ed25519 client certificates and TLS 1.3, since that combination is less common than RSA+TLS1.2 in older MQTT client examples.
- Keep the AXIAM-auth TLS stack and the MQTT TLS stack's certificate/key loading paths consistent (both should use the same in-memory PEM blobs the C SDK already prefers — "no temp files" — rather than writing private keys to disk for one stack and not the other).

**Warning signs:**
- One virtual device's slow AXIAM re-auth visibly delays another device's command response in the Twin's decision feed.
- Simulator process CPU/thread count balloons or, conversely, throughput is capped far below what 38-servers-per-process should support.

**Phase to address:**
The simulator-host phase (per language), specifically the concurrency/connection-model design step, before device count is scaled from a handful up to the full 114.

---

### Pitfall 8: The PKI bootstrap chicken-and-egg and certificate expiry are demo-day failure modes, not one-time setup problems

**What goes wrong:**
PROJECT.md already identifies the two-stage bootstrap (AXIAM starts on a temporary localhost cert, then mints the org CA and reissues everything). The risks that remain even with that design: (1) AXIAM's leaf-certificate validity is capped to its issuing CA's remaining validity — a CA created once with a fixed `validity_days` and never rotated means every future `demo-reset` re-issues leaves whose validity silently shrinks each time, and eventually a `validity_days` request for a new leaf will be rejected outright if it would outlive the CA; (2) `just demo-reset` re-runs the entire bootstrap sequence including CA regeneration — if it partially fails (e.g., crashes after regenerating the CA but before reissuing RabbitMQ's server cert), the system is left in a mixed-trust state that isn't obviously broken until a specific connection is attempted; (3) a tenant's `max_certificate_validity_days` setting can silently cap requested validity below what setup scripts assume.

**Why it happens:**
Bootstrap sequences are usually tested end-to-end exactly once, in a clean environment, and re-tested rarely — but this project explicitly wants `demo-reset` to be routine, which means the bootstrap path runs far more often than in a typical deployment, and each run is an opportunity for a half-finished state.

**How to avoid:**
- Make `demo-reset` idempotent and resumable: check what already exists (CA, per-service certs) before regenerating, and fail loudly with a clear "reset didn't complete" state rather than leaving a partially-reissued trust chain that looks like a random TLS error later.
- Pick a CA `validity_days` generously long relative to the expected project lifetime (AXIAM's own root-CA example uses 3650 days) so day-to-day resets never approach the ceiling.
- Record, in the dogfooding findings, exactly what breaks (and how) if `demo-reset` is interrupted mid-run — this is a near-certain real occurrence given how often the command will be used during development.

**Warning signs:**
- A `demo-reset` run that succeeds but leaves one service (often the one restarted last) still presenting its previous certificate.
- `sign-csr`/`certificates` calls starting to fail with a validity-related 400 after many reset cycles.

**Phase to address:**
The setup/bootstrap phase, with a specific "kill `demo-reset` halfway through and rerun it" test added to that phase's verification criteria.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|-----------------|
| Disabling TLS hostname verification everywhere instead of per-service fingerprint/CN pinning (Pitfall 1) | Fast path around the no-SAN problem | Loses MITM protection on links you actually control; masks future real cert mismatches | Never for browser-reachable services; only ever for internal service-to-service links, and only with an explicit fingerprint/CN check substituted in, not a bare "skip verification" |
| Hand-creating a handful of RabbitMQ users/topic-permissions for early MQTT testing instead of building the provisioning integration | Gets the first device talking over MQTT quickly | Doesn't scale to 114 devices or to live add/remove without restart; becomes a rewrite, not an extension | Only for the very first spike proving the MQTT plugin works at all |
| Turning on `decision_cache_enabled` for perceived performance without re-verifying every grant-mutation path calls `invalidate_*` | Lower CheckAccess latency under load | Reintroduces a bounded (TTL) but real stale-allow window if any mutation path is missed; also changes the demo's "revoke takes effect immediately" guarantee from exact to "within default TTL" | Only after profiling shows CheckAccess latency is an actual demo-visible problem, and only with a test that revokes a grant and asserts the next check denies with zero delay |
| Single-threaded/sequential device loop in a simulator host to get something running first | Simple first implementation | Serializes token refresh/report/command latency across every device sharing that thread once device count grows (Pitfall 7) | Acceptable for an early single-digit-device smoke test only |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|-----------------|-------------------|
| RabbitMQ MQTT plugin | Assuming it's enabled by default on the `rabbitmq:4-management-alpine` image AXIAM already runs | Explicitly `rabbitmq-plugins enable rabbitmq_mqtt` and expose 1883/8883 in the container/compose definition; verify with `rabbitmq-diagnostics list_mqtt_connections` |
| RabbitMQ MQTT vhost | Trying to route different tenants/device types to different vhosts via the MQTT client's username, expecting AXIAM certs' SAN to drive `mqtt_default_vhosts` | Since AXIAM certs carry no SAN (Pitfall 1), use the CN/DN-based `mqtt_default_vhosts` mapping or, simpler for this design (one shared `domo` vhost, tenant isolation via topic scoping), just set a static `mqtt.vhost = domo` |
| RabbitMQ MQTT QoS | Assuming QoS 2 end-to-end delivery guarantees for intercom "call" or command events | RabbitMQ's MQTT plugin auto-downgrades QoS 2 publishes and subscriptions to QoS 1 — design command/state delivery assuming at-least-once QoS 1, with idempotent command handling on the device side |
| RabbitMQ retained messages | Assuming a retained "last known device state" survives a broker restart by default | The default retained-message store is ETS (in-memory, gone on restart); if last-known-state-on-reconnect matters for the demo, explicitly configure the DETS-backed retained store (2 GB/vhost cap, node-local — fine for a single-node demo) |
| OPAQUE via `axiam-sdk-wasm` | Treating it as a drop-in, small addition to the portal bundles | It ships at ~540 KB gzipped versus ~40 KB for the plain TypeScript SDK, is one non-tree-shakeable blob, and is still pre-1.0 (API can change between prereleases) — pin the exact version and budget bundle-size/first-paint accordingly on Pi-hosted static assets over LAN Wi-Fi |
| `axiam-sdk-wasm` under a CSP | Assuming the default portal CSP just works once the wasm file is served | WebAssembly instantiation typically needs `script-src` to include `'wasm-unsafe-eval'` (or an equivalent explicit allowance) under a strict CSP, and the `.wasm` asset must be served with `Content-Type: application/wasm` by Caddy or the bundler's runtime instantiation will fail |
| AXIAM "device" terminology | Implementing the RFC 8628 OAuth2 Device Authorization Grant (`/oauth2/device_authorization`, user-code polling) by mistake when building simulator auth | This project's devices need the mTLS certificate login (`POST /api/v1/auth/device`), a completely different, unrelated mechanism that happens to share the word "device" in AXIAM's docs |
| AXIAM decision cache | Assuming `CheckAccess` always hits a fresh authorization query | The decision cache is **off by default**; if a future perf pass turns it on, the worst-case revocation latency becomes `decision_cache_ttl_secs` (default 5 s, hard ceiling 300 s) rather than immediate, on every replica including the one that performed the mutation if invalidation is misconfigured |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Un-jittered device token refresh (Pitfall 3) | Periodic CPU/latency spikes on a clean 15-minute cadence | Jitter each device's re-auth timer independently | As soon as more than a handful of devices boot in lockstep (true from day one at 114 devices) |
| Sequential/blocking per-device loops in the C simulator (Pitfall 7) | Per-device latency grows linearly with device count on one host | Thread-per-device or a bounded worker pool | Once a single C process hosts more than a few devices — this project's target is ~38-57 per host |
| Unbounded MQTT retained-message growth in the `domo` vhost | RabbitMQ memory/disk creeping up over a long-running demo/dev cycle | Cap retained-message TTLs or don't retain on high-churn topics (e.g., transient intercom "ringing" events) | Noticeable after days of `demo-reset` cycles without a broker restart, sooner if a simulator bug republishes at high frequency |
| Cross-compiling arm64 images for Rust/C++ under amd64+QEMU | CI/build times balloon (Rust and C++ compilation under emulation is known to be dramatically slower than native) | Use `docker buildx` with native arm64 runners where available, or build the Twin/simulators' arm64 images directly on Pi-class hardware for the final artifact, keeping amd64-only builds fast in the everyday dev loop | As soon as arm64 images are built in CI/dev on amd64 hosts via QEMU rather than natively |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Disabling TLS hostname verification broadly to route around Pitfall 1 instead of pinning by CN/fingerprint | Silently reopens MITM on links that should be protected; a future real CA compromise or cert-swap goes unnoticed | Pin by CN or SHA-256 fingerprint explicitly per service, never a blanket "skip verification" flag |
| Tenant isolation enforced only by per-item `CheckAccess` calls in listing/SSE endpoints, not by scoping the underlying query to `tenant_id` first | A listing endpoint that fetches "all sites" then filters in application code can leak tenant B's existence (counts, IDs, timing) to tenant A even if the actual records are filtered out before response | Every listing/SSE query must be scoped to the caller's `tenant_id` at the data-access layer, with `CheckAccess`/decision-feed logging layered on top, not substituting for it |
| Treating an access token as carrying baked-in permissions | An app-layer cache of "what this token can do," refreshed only at login, would make revoke ineffective until the token's own expiry | Regular AXIAM access tokens are documented to carry only subject/scope (the `permissions` claim is UMA-RPT-only) — every authorization decision must go through a live `CheckAccess`/`check_as` call, never a locally cached permission snapshot from token-issue time |
| Adding a deny rule anywhere in the resource hierarchy for any future exclusion feature (Pitfall 2) | Silently blocks all delegated `granted-operator` grants beneath it | Model every exclusion as an allow that is withheld/removed, never as a deny, anywhere at or above an apartment/device node |
| Per-device private keys ever touching a shared/logged filesystem path (e.g., a simulator writing a device key to a temp file for the MQTT client library when the AXIAM SDK already keeps it in-memory) | A key readable by another process/user on the simulator PC, or accidentally captured in a log/backup | Keep private keys in memory only, end to end, matching the C SDK's own "no temp files" pattern; make sure the MQTT client library chosen doesn't force a file path for the client certificate |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-------------------|
| Browser cert-trust interstitial appears mid-demo because of Pitfall 1 | Breaks the "polished, guided storyline" for prospects at the worst possible moment | Pre-provision every demo browser/machine with the trust exception (or the workaround chosen) well before the demo, and rehearse the exact click sequence so it reads as intentional rather than broken if it must appear at all |
| Access-decision feed shows a denial reason (`no_grant`/`denied_by_rule`) that doesn't match what a non-technical prospect expects (e.g., `denied_by_rule` from an unrelated deny rule per Pitfall 2) | Confusing or embarrassing on stage — looks like a bug rather than "the policy engine working correctly" | Keep the demo's authorization model simple enough (no deny rules) that every denial in the four key moments is `no_grant`, which reads clearly as "access removed," and rehearse what each reason code should say before a live run |
| Device "goes offline" transiently every 15 minutes due to un-jittered token refresh (Pitfall 3) | Undermines the "live device loop" demo moment with unexplained flicker | Jitter and proactively refresh well before expiry so this is invisible during any single demo run (typically well under 15 minutes) |

## "Looks Done But Isn't" Checklist

- [ ] **Browser HTTPS to the Pi:** Often missing an actual test in a real browser (Chrome/Firefox/Chromium) against the Pi's LAN IP/hostname — verify by opening the staff console from a laptop on the LAN, not just `curl -k` or a same-machine test that never triggers real hostname verification.
- [ ] **mTLS chain delivery:** Often missing the intermediate CA in what's actually installed on each service — verify with `openssl verify -CAfile root.pem -untrusted intermediate.pem leaf.pem`, not just "the leaf loads."
- [ ] **Grant revoke "takes effect immediately":** Often missing verification of the exact code path — confirm the Management Platform's revoke handler actually deletes the `granted-operator` role assignment (and, if the decision cache is ever turned on, that the deletion path also triggers `invalidate_*`) by testing a live command from the installer's already-issued token immediately after revoke, not just checking the AXIAM resource state.
- [ ] **Device online after UI add, no restart:** Often missing the simulator host's polling loop actually being verified end-to-end — add a device through the UI while a simulator host is already running and confirm it comes online without any restart, not just that the cert/service-account got created.
- [ ] **`demo-reset` idempotency:** Often missing a test of interrupting it mid-run — kill it partway through and rerun it; verify it reaches a consistent state rather than assuming the happy path is the only path.
- [ ] **Tenant isolation on listing/SSE endpoints:** Often missing a check on the underlying query, not just the per-item CheckAccess — verify tenant B's data never appears (not even filtered client-side) in tenant A's listing/SSE payloads.
- [ ] **RabbitMQ topic ACLs at scale:** Often missing beyond the first 2-3 hand-provisioned devices — verify the full 114-device provisioning path (add via UI → cert issued → RabbitMQ user/permission created) works, not just a manually seeded subset.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|----------------|-----------------|
| Browser rejects AXIAM-issued server cert (Pitfall 1) | LOW (per-machine, once configured) | Pre-provision the demo browser with a trust exception or a CN/fingerprint-pinning proxy config ahead of time; this is a setup-time fix, not a code fix, given AXIAM cannot be extended in scope |
| Deny rule breaks installer grants (Pitfall 2) | LOW once diagnosed, but diagnosis can be slow | Check the decision feed for `denied_by_rule` on an otherwise-working grant, locate and remove the deny rule; add a regression test asserting no deny rule exists anywhere at/above apartment nodes |
| Device reconnect storm from un-jittered refresh (Pitfall 3) | LOW | Add jitter to the refresh schedule; no data is lost, only a transient load spike |
| MQTT/RabbitMQ misconfiguration destabilizes AXIAM's own AMQP traffic (Pitfall 4) | MEDIUM | Isolate by vhost (already planned) and by resource limits per vhost; if it recurs, consider a rate limit or connection cap on the `domo` vhost specifically, short of a second broker |
| Pi OOM mid-demo (Pitfall 6) | HIGH (mid-demo) / MEDIUM (pre-demo) | Pre-demo: profile full four-moment load on real Pi hardware and set explicit container memory limits well under the 8 GB ceiling; mid-demo, have the amd64 XPS as a fallback host ready to go, since the platform is designed to run on both |
| Partially-completed `demo-reset` (Pitfall 8) | MEDIUM | Make reset idempotent/resumable (preferred) or, as a stopgap, keep a known-good pre-reset snapshot/backup of volumes to restore from if a reset run fails partway |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| No SAN on any AXIAM cert breaks browser TLS (P1) | PKI/TLS bootstrap phase | Open the staff console from a real browser over the LAN against the Pi; document the chosen workaround in the demo script |
| Deny-override breaks delegated grants (P2) | RBAC/authorization-model phase | Automated test: grant, revoke, and re-grant an installer's device access with no deny rule anywhere in the tree; a lint/check that no deny rule exists at/above apartment nodes |
| No refresh token / reconnect storm (P3) | Simulator-host + Device-Twin phase | Run all 114 simulated devices for at least 2x the access-token TTL and confirm no synchronized reconnect spike and no dropped commands across a token boundary |
| Untested MQTT plugin on AXIAM's shared broker (P4) | Messaging/infra phase (the one PROJECT.md already flags as "decided in phase research") | Load the `domo` vhost with the full device count while exercising AXIAM's own AMQP traffic (cache invalidation, audit events) and confirm no cross-vhost degradation |
| Per-device RabbitMQ ACL provisioning at scale (P5) | Device-provisioning phase | Add and remove devices through the UI across the full 114-device seed and confirm each gets/loses correct MQTT topic access with no restart and no manual `rabbitmqctl` step |
| Pi memory/storage pressure (P6) | Deployment/infra phase | Full four-demo-moment run-through on real Pi hardware booted from NVMe/SSD, with `docker stats`/OOM-killer logs checked clean |
| Simulator concurrency + dual TLS stacks (P7) | Simulator-host phase, per language | Single-host load test at target device count (up to ~57) with per-device latency remaining flat as count increases |
| PKI bootstrap chicken-and-egg / cert expiry (P8) | Setup/bootstrap phase | Interrupt `demo-reset` mid-run and rerun it; run enough reset cycles to confirm CA validity headroom is not a near-term concern |

## Sources

- `/home/emanuele/git/priv/axiam/crates/axiam-pki/src/cert.rs` (leaf-certificate parameters: no SAN/keyUsage/EKU on either issuance path) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/crates/axiam-pki/src/ca.rs` (CSR extension refusal logic) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/crates/axiam-api-rest/src/handlers/certificates.rs` (`CreateCertificateRequest` schema, no SAN field) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/docs/pki/README.md` — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/crates/axiam-authz/src/engine.rs` and `decision_cache.rs` and `config.rs` (deny-override semantics; decision cache default-off, 5s default TTL, invalidation model) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/crates/axiam-api-rest/src/handlers/auth.rs` (`device_auth` handler, no refresh token; `access_token_lifetime_secs` default 900s in `axiam-core/src/models/settings.rs`) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/crates/axiam-auth/src/token.rs` (regular access tokens carry no baked-in `permissions` claim; that claim is UMA-RPT-only) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/docs/api/device-flow.md` (RFC 8628 device-authorization grant, distinct from mTLS device login) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/k8s/rabbitmq/statefulset.yml` (AXIAM's own dedicated-vhost rationale; resource limits) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/docker/rabbitmq-tls.conf` — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam/docs/deployment/rpi5-k3s.md` (8 GB "required not recommended"; SurrealDB on SD card) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam-rust-sdk/axiam-sdk-wasm/README.md` (bundle size ~540 KB gzipped vs ~40 KB TypeScript SDK; pre-1.0 API) — HIGH confidence, primary source
- `/home/emanuele/git/priv/axiam-c-sdk/README.md` and CONTRACT.md (synchronous-only calls; in-memory PEM blobs; no MQTT anywhere in any AXIAM SDK, confirmed by repo-wide search) — HIGH confidence, primary source
- Repo-wide `grep` for "mqtt"/"MQTT" across the `axiam` repository and all `axiam-*-sdk` repositories returning zero hits — HIGH confidence, primary source (absence verified directly)
- [MQTT Plugin | RabbitMQ](https://www.rabbitmq.com/docs/mqtt) — MEDIUM/HIGH confidence, official vendor docs (vhost mapping, QoS downgrade, retained-message stores, `ssl_cert_login_from`)
- [rabbitmq/rabbitmq-mqtt issue #73 — client cert vhost mapping](https://github.com/rabbitmq/rabbitmq-mqtt/issues/73) — MEDIUM confidence, vendor issue tracker
- [Chrome Deprecates Subject CN Matching – text/plain](https://textslashplain.com/2017/03/10/chrome-deprecates-subject-cn-matching/) and [Mozilla bug 1245280](https://bugzilla.mozilla.org/show_bug.cgi?id=1245280) — HIGH confidence, corroborated browser-vendor behavior (Chrome 58, Firefox 48/101 SAN-only hostname matching)

---
*Pitfalls research for: multi-tenant IoT home-automation demo on AXIAM IAM*
*Researched: 2026-09-19*
