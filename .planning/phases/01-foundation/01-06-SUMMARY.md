---
phase: 01-foundation
plan: 06
subsystem: smoke
tags: [authorization, mqtt, mtls, pki, multi-tenant, live-verification, dogfooding]

requires:
  - "01-04: tools/domo-bootstrap/src/stages/tree.rs — the resource-with-its-groups helper and the resolve-before-create path"
  - "01-04: tools/domo-bootstrap/src/naming.rs — the D-17 naming scheme and structural_groups_for"
  - "01-05: services/device-twin — the hardened four-endpoint authorization backend"
  - "01-05: crates/domo-common/src/topic.rs — the separator-aware namespace predicate"
  - "01-02: docs/dogfooding-findings.md — the findings log and its DF- sequence"
provides:
  - "One live branch of the resource tree in Lakeside, reserved-prefix, with its structural groups"
  - "Thirteen live authorization assertions over that branch (AUTHZ-01, AUTHZ-02)"
  - "A twelve-case device connect matrix, declared as data and run by a separate runner"
  - "`just smoke`, `smoke-tree`, `smoke-authz`, `smoke-certs`, `smoke-matrix`, `smoke-teardown`, `smoke-slow`"
  - "The reserved fixture prefix `smoke-`, which Phase 2's seed must keep avoiding"
  - "DF-017 confirmed at runtime; DF-025, DF-026, DF-027 recorded"
affects: [01-07, phase-02, phase-03, device-twin, domo-probe]

actuals:
  tokens: 96000
  tasks: 3
  commits: 4
  plan_head_before: 9be236a8fde3a031b5e1d8bc0ba4b5c5ecd40dac

tech-stack:
  added: []
  patterns:
    - "Matrix cases declared as data plus a separate runner, so Phase 3 can reuse the harness as a load driver"
    - "Expected outcomes name the LAYER that refuses, not merely that something failed"
    - "Every mutating assertion restores its starting state, so re-runnability is a property of each case rather than of a cleanup at the end"
    - "Read-only `resolve` as the twin of a creating `build`, so a verification recipe cannot quietly build what it is about to assert"
    - "An experiment whose answer only an admin client can produce records that answer to disk; the matrix reports and fails on it"

key-files:
  created:
    - "tools/domo-bootstrap/src/stages/smoke.rs — the branch builder and its fixtures"
    - "tools/domo-bootstrap/src/stages/smoke/assertions.rs — thirteen live authorization cases"
    - "tools/domo-bootstrap/src/stages/smoke/certs.rs — fixture certificates and the cross-tenant issuance experiment"
    - "tools/domo-bootstrap/src/stages/smoke/support.rs — the access-check and membership helpers"
    - "tools/domo-bootstrap/src/stages/smoke/teardown.rs — prefixed-fixture removal"
    - "tools/domo-probe/src/cases.rs — the connect matrix as data"
    - "tools/domo-probe/src/matrix.rs — the runner and the outcome classifier"
    - "tools/domo-probe/src/fixtures.rs — fixture loading and the hand-rolled device login"
    - "just/smoke.just — the seven recipes"
  modified:
    - "tools/domo-bootstrap/src/main.rs — four new subcommands"
    - "tools/domo-bootstrap/src/stages/mod.rs — the smoke module"
    - "tools/domo-bootstrap/src/stages/tree.rs — ensure_group_binding made public, returns the group name"
    - "tools/domo-probe/src/main.rs — smoke-keys and the two matrix subcommands"
    - "docs/dogfooding-findings.md — DF-017 confirmed; DF-025, DF-026, DF-027 appended"

key-decisions:
  - "Both common areas use the SAME slug under different parents, so the duplicate-sibling question is exercised for real rather than assumed away."
  - "The device self-service group is created through the lazy path, reusing `tree::ensure_group_binding` rather than a second implementation — the eager and lazy paths cannot then drift into binding at different scopes."
  - "The cross-tenant issuance experiment records its answer to disk and the MATRIX fails on it, rather than the provisioning stage failing. Failing at provisioning would stop the other eleven cases from ever running."
  - "The `other-tenant-ca` case uses the STRICT forged-common-name form, which only became producible once DF-017 was confirmed. The weaker form (the other tenant's own device certificate) is the documented fallback if AXIAM ever starts refusing."
  - "Two cases are left FAILING. They are confirmed AXIAM defects; the plan's prohibition against making a negative case pass by weakening it is explicit and flagged."

requirements-completed: [AUTHZ-01, AUTHZ-02, MQTT-01, MQTT-02, PKI-03]

coverage:
  - id: A1
    description: "The resource tree's shape, ordering, empty and duplicate behaviour asserted against live AXIAM"
    requirement: AUTHZ-01
    verification:
      - kind: command
        ref: "just smoke-authz — cases ancestors, common-areas, stable-identifiers, empty-portfolio, empty-tenant, duplicate-create, duplicate-siblings"
        status: pass
      - kind: command
        ref: "mutation check: reversing the expected ancestor order fails the case and prints both sequences"
        status: pass
    human_judgment: false
  - id: A2
    description: "Access genuinely follows group membership, in both binding orders, including the empty-group case"
    requirement: AUTHZ-02
    verification:
      - kind: command
        ref: "just smoke-authz — cases membership (five states), empty-group, membership-before-binding, device-self, group-pattern, group-isolation"
        status: pass
    human_judgment: false
  - id: A3
    description: "No deny rule exists at or above any apartment or device node"
    requirement: AUTHZ-02
    verification:
      - kind: command
        ref: "just smoke-authz — case no-deny-rule, checking reason_code across eight nodes"
        status: pass
    human_judgment: false
  - id: A4
    description: "The device identity chain proven positively: connect, subscribe, publish, round trip on its own namespace"
    requirement: MQTT-01
    verification:
      - kind: command
        ref: "just smoke-matrix — case positive"
        status: pass
    human_judgment: false
  - id: A5
    description: "Credential and token forgeries refused, each with a named signal"
    requirement: MQTT-02
    verification:
      - kind: command
        ref: "just smoke-matrix — cases mismatched-cert, malformed-token, bad-signature, wrong-tenant"
        status: pass
    human_judgment: false
  - id: A6
    description: "The broker's peer-certificate requirement demonstrably in force, distinguishably from an authorization denial"
    requirement: MQTT-01
    verification:
      - kind: command
        ref: "just smoke-matrix — case no-client-cert, observed as a TLS CertificateRequired alert before any CONNACK"
        status: pass
    human_judgment: false
  - id: A7
    description: "A device cannot publish outside its own namespace, including into an adjacent name that extends its own"
    verification:
      - kind: command
        ref: "just smoke-matrix — cases namespace-sibling, namespace-other-tenant, namespace-adjacent"
        status: pass
    human_judgment: false
  - id: A8
    description: "A certificate bound to no service account authenticates as nobody"
    requirement: PKI-03
    verification:
      - kind: command
        ref: "just smoke-matrix — case empty-cert-binding, observed HTTP 403"
        status: pass
    human_judgment: false
  - id: A9
    description: "The cross-tenant certificate-issuance question answered and recorded"
    requirement: PKI-03
    verification:
      - kind: command
        ref: "just smoke-matrix — case cross-tenant-ca-issuance"
        status: fail
    human_judgment: true
    rationale: "The case is RED because AXIAM accepts the request. That is the recorded answer, not a harness defect, and whether the phase proceeds with a permanently-red case or the confirmed behaviour is pinned is a decision for the user — see 'Decision required'."
  - id: A10
    description: "A forged-common-name certificate from another tenant's CA refused at some layer"
    requirement: MQTT-02
    verification:
      - kind: command
        ref: "just smoke-matrix — case other-tenant-ca"
        status: fail
    human_judgment: true
    rationale: "RED because nothing refuses it: the connection succeeds end to end. Confirmed AXIAM gap (DF-025), left failing per the plan's flagged prohibition. Needs the same user decision as A9."
  - id: A11
    description: "The suite is re-runnable against the mechanisms this plan owns"
    verification:
      - kind: command
        ref: "two consecutive `just smoke` runs produce case-for-case identical output; `just smoke-teardown && just smoke` reproduces it again from an emptied tenant"
        status: pass
    human_judgment: false

duration: ~3h
completed: 2026-09-21
status: complete
---

# Phase 1 Plan 06: Live Smoke — Branch, Authorization and the Connect Matrix Summary

**One real branch of the resource tree, thirteen live authorization assertions and a twelve-case device connect matrix — which proved the identity chain positively, refuted it in eight distinct ways, and found two confirmed AXIAM defects that no amount of reading the source had settled.**

## Performance

- **Duration:** ~3h
- **Tasks:** 3
- **Commits:** 4 (measured: `git rev-list --count 9be236a..HEAD`)
- **Files created:** 9 — **modified:** 5
- **Live runs:** the full suite executed end to end at least five times, including two consecutive runs and one after a teardown

## Accomplishments

- **The authorization model meets a real AXIAM and holds.** Thirteen named cases, every one green. The ancestor assertion compares an ordered sequence and was mutation-checked — reversing the expected order fails it and prints both sequences.
- **The device identity chain is proven positively and refuted in eight ways**, each with a named expected signal rather than a generic "it failed". The `no-client-cert` case is observed as a TLS `CertificateRequired` alert *before* any CONNACK exists, which is the difference between the broker's peer-certificate requirement being in force and being silently off.
- **Two confirmed AXIAM defects**, both of which were open questions this plan existed to settle, and neither of which was made to pass by weakening what it tests.
- **Ed25519 held at every hop.** No algorithm change was needed anywhere (D-27); the broker, rustls and AXIAM all accepted the Ed25519 leaf chained to a tenant CA.

## Task Commits

| Task | Commit | What |
|---|---|---|
| 1 | `b2ca0e7` | The branch, its structural groups, the device accounts, `smoke-teardown` |
| 2 | `b5a63fc` | Thirteen live authorization assertions |
| 3 | `3fb194d` | The connect matrix, the certificate stage, the findings |
| — | `81238da` | Splitting three files that exceeded CLAUDE.md's 500-line limit |

## Decision required

**`just smoke` is RED, and it should stay red until someone decides otherwise.**

Ten of twelve matrix cases pass. Two fail, and both failures are confirmed defects in AXIAM rather than in this harness:

| Case | Observed |
|---|---|
| `cross-tenant-ca-issuance` | AXIAM signed a leaf under the **other tenant's** signing CA, at the request of this tenant's admin |
| `other-tenant-ca` | That forged leaf then **connected, published and subscribed** end to end |

The plan's own acceptance criteria cannot both be satisfied, and they say opposite things:

- *"an acceptance fails the matrix"* — satisfied. The matrix is red.
- *"`just smoke` passes twice consecutively"* — **not satisfiable**, because AXIAM accepts.

Where the two conflict the plan is explicit, twice and with `flagged: true`: *"A negative case must never be made to pass by weakening the thing it tests. If a refusal does not happen, record a finding and stop."* So I recorded the findings and stopped, rather than pinning the observed behaviour to get a green run.

What was verified instead is the property PLAT-05 actually names — **the same result twice in a row**. Two consecutive `just smoke` runs produce case-for-case identical output, and so does a third after `just smoke-teardown` empties the tenant. The suite is fully re-runnable; it is simply not green.

**The choice is the user's, and it is not a small one:**

1. **Leave it red** until DF-017 is fixed upstream. Honest, and every other regression in the suite still reports — but plan 01-07's `just phase-verify` will inherit a red gate, and a permanently-red suite stops being read.
2. **Pin the confirmed behaviour** — assert what AXIAM does today and fail if it ever *changes* — so the suite goes green and a future fix is detected. This is normal practice for a confirmed upstream gap, but it does convert a security failure into a green tick, which is exactly what the prohibition exists to prevent. If chosen, the pinned case should keep printing the gap's name loudly.

I did not make this call.

## What the plan asked to be recorded

### The cross-tenant certificate-issuance result

**Accepted.** Recorded in **DF-017**, whose status moves from `reported-from-source-reading` to `confirmed`. The consequence is **DF-025**, a new entry.

The stamping is the detail that makes it more than bookkeeping: the issued leaf carries `tenant_id` = **the acting tenant's** (Lakeside) while `issuer_ca_id` = **the other tenant's** signing CA. AXIAM records the certificate as belonging to one tenant and signs it with another tenant's key.

### The exact signal each negative case produced

Phase 3 can use this table to tell a regression from a new failure mode.

| Case | Expected layer | Observed signal |
|---|---|---|
| `mismatched-cert` | broker, at CONNECT | `CONNACK BadClientId` — `ssl_cert_client_id_from` firing before the Twin is consulted |
| `malformed-token` | authorization backend | `CONNACK BadUserNamePassword` |
| `bad-signature` | authorization backend | `CONNACK BadUserNamePassword` |
| `wrong-tenant` | authorization backend | `CONNACK BadUserNamePassword` |
| `no-client-cert` | TLS handshake | `received fatal alert: CertificateRequired`, **before any CONNACK** |
| `namespace-sibling` | Twin topic check | connection closed after CONNACK, no PUBACK |
| `namespace-other-tenant` | Twin topic check | connection closed after CONNACK, no PUBACK |
| `namespace-adjacent` | Twin topic check | connection closed after CONNACK, no PUBACK |
| `empty-cert-binding` | AXIAM device login | **HTTP 403** (the plan predicted 401 — see below) |
| `other-tenant-ca` | any | **none — it connected** |

The three namespace cases are indistinguishable from each other at the protocol layer, by design: RabbitMQ closes the connection on an unauthorised publish and says no more. The distinction between them lives in which routing key was attempted, which the case name carries.

### Whether the Ed25519 chain held at the broker

**Yes, at every hop.** Ed25519 device keys, leaves signed by an Ed25519 tenant CA under the imported root, accepted by RabbitMQ 4.3.6's Erlang TLS 1.3 for client authentication, by rustls on the client side, and by AXIAM's own mTLS device-login listener. D-27's fallback to RSA-4096 was never needed and no finding was required.

### The reserved fixture prefix Phase 2's seed must avoid

**`smoke-`**, applied to the *slug*, after the type prefix: `site:smoke-park`, `apartment:smoke-tower-a1`, `device:smoke-probe-light`, and service accounts `smoke-probe-device` / `smoke-peer-device` / `smoke-unbound-device` / `smoke-summit-device`, plus the user `smoke-resident`.

This is a **costly** commitment in both directions. `smoke-teardown` decides what to delete by this prefix alone, so a Phase 2 seed name that happened to carry it would be destroyed by a smoke run without a word. `ensure_prefixed` refuses a fixture that lacks it, so the rule cannot rot from this side.

## Deviations from Plan

### Auto-fixed

**1. [Rule 3 — Blocking] `tree::ensure_group_binding` was private**

- **Found during:** Task 1.
- **Issue:** The plan requires the device self-service group to be created through plan 01-04's helper and explicitly forbids reimplementing group creation. That helper was private to `tree.rs`, which is not in this plan's `files_modified`.
- **Fix:** Made it `pub` and had it return the group name. No logic changed. The comment says why both the eager and the lazy path must share it — two implementations of "resolve, create, bind, verify the scope" is how they would drift into binding at different scopes.
- **Commit:** `b2ca0e7`

**2. [Rule 3 — Blocking] The plan's file list had nowhere to sign a certificate**

- **Found during:** Task 3.
- **Issue:** Task 3 must issue fixture certificates and attempt the cross-tenant issuance, both of which need tenant-admin credentials. Its `<files>` lists only probe files, `just/smoke.just` and the findings log — no bootstrap file. The probe is a *device* and must not hold admin credentials.
- **Fix:** `stages/smoke/certs.rs`, inside this plan's own module directory. Same shape as plan 01-05's `tests/common/mod.rs` deviation.
- **Commit:** `3fb194d`

**3. [Rule 1 — Bug] Pre-existing clippy failure in `domo-probe`**

- **Found during:** Task 3 verification. `collapsible_match` in `connect()`, from plan 01-01, blocking the clippy gate on a file this plan owns.
- **Fix:** Collapsed the `if` into a match guard. One line, no behaviour change.
- **Commit:** `3fb194d`

**4. [Rule 2 — CLAUDE.md] Three files exceeded the 500-line limit**

- **Found during:** self-check. `smoke.rs` 555, `assertions.rs` 557, `matrix.rs` 588.
- **Fix:** Split along seams each file already had, into `teardown.rs`, `support.rs` and `fixtures.rs`. Re-verified live: case-for-case identical output.
- **Commit:** `81238da`

### Corrected predictions, recorded rather than silently substituted

**`empty-cert-binding` returns 403, not the predicted 401.** The plan's Task 3 says "expect 401, which is what 'authenticates as nobody' means concretely". AXIAM answers **403**. The security property — refused — holds exactly; only the status differs. The case now accepts 401 or 403 and still fails on a 200, so nothing was weakened, and the observed value is filed as **DF-027** because the distinction matters to a client deciding whether to re-authenticate.

**`wrong-tenant` refuses at subject mismatch, not at the tenant assertion.** The plan notes this "is the case that would pass if the Twin's per-tenant verifier assertion were removed". Live, the refusal comes from the subject-to-user-name equality check, which fires first. The per-tenant assertion is a genuine second barrier, but no live case can isolate it: both tenants' tokens are signed by the same organization key, so a token whose subject *matched* the connecting account would have to belong to an account existing in both tenants, and none does. Plan 01-05 proves the assertion separately at unit level. The code comment says so rather than leaving the plan's claim standing unqualified.

**`other-tenant-ca` was implemented in its strict form, which the plan's wording implies and which only became producible mid-execution.** My first implementation used the other tenant's *own* device certificate, which the broker refuses on `client_id` — a real refusal, but of the weaker claim. Once `cross-tenant-ca-issuance` proved a forged-common-name leaf could be minted, the strict form became testable, and it is what the case now does. The weaker form remains as an announced fallback.

**Total deviations:** 4 auto-fixed (2 blocking, 1 bug, 1 CLAUDE.md), 3 recorded prediction corrections.
**Impact:** No scope creep. Two deviations were structural prerequisites the plan could not be executed without.

## Environment note — how the live runs were performed

This worktree has no `.secrets/` tree: it is gitignored and lives in the main checkout, alongside the running stack. Two consequences worth recording for whoever runs the next worktree plan:

- The recipes were executed as `just --justfile "$PWD/justfile" --working-directory <main checkout> <recipe>` — **this worktree's recipes**, against the main checkout's runtime state and the live stack. This is why plan 01-04's live verification was deferred, and why 01-03 stood up a throwaway stack instead.
- `just build` would have built the *main checkout's* source under that working directory, so the image was built directly from this worktree with `docker build -f deploy/docker/Dockerfile.rust --target tools -t domo-tools:dev .` after every Rust change, before every stage or probe invocation. The dispatch's warning about running a stale binary is real; this is the form of the rebuild that works from a worktree.
- The session's secret-read guard refuses any command naming `.secrets`, including `ln -s`. That is a false positive of the guard's own allowlist (which already exempts `ls`, `mkdir`, `touch`, `rm`), but it was **not worked around** — the `just` route above needs no such command.

## Findings recorded

| Id | What | Status |
|---|---|---|
| DF-017 | Leaf issuance does not bind a tenant signing CA to its tenant | `reported-from-source-reading` → **`confirmed`**, with the stamping evidence |
| DF-025 | A forged-common-name certificate from another tenant's CA authenticates a device end to end | **new**, high |
| DF-026 | The console image's nginx resolves proxy upstreams at config load, so it crash-loops when the API is absent | **new**, medium — observed by plan 01-03, which could not write this file |
| DF-027 | An unbound device certificate is refused with 403, not 401 | **new**, low |

Two items handed to me were deliberately **not** filed:

- **The Rust SDK's missing gRPC token-validation wrapper** (plan 01-05's Deferred Issue #2) is **already filed as DF-012**, with the same `JwksVerifier` workaround and the same reasoning. Plan 01-05 could not see the file to know that. Filing it again would have put two identifiers on one gap, which is exactly what the log's "ids are never reused" discipline is guarding against.
- **`openssl s_client` needing `-servername`** against a host-keyed Caddy is, on my reading, **our probe's limitation and not an AXIAM gap** — `scripts/verify-pki.sh` omits SNI, and a Caddy with `auto_https off` and only host-keyed site blocks correctly has no default site to answer with. The log's own audit rule is about hand-rolled *AXIAM calls*; this is not one. Filing it would have padded the log with a one-word bug in our own script. The fix is already routed to plan 01-07's verify gate.

## Known Stubs

None. Nothing here returns a placeholder or a hardcoded empty value. The two red cases are not stubs — they run fully and report a real, reproducible outcome.

## Threat Flags

None beyond the plan's own register. The new surface is the smoke fixtures themselves, which `smoke-teardown` removes, and the forged certificate the experiment mints, which `smoke-teardown` explicitly revokes because it is unbound and would otherwise outlive the accounts.

The plan's register needs one correction, which belongs to the phase rather than to this file: **T-06-04's disposition of `mitigate` is now wrong.** It reads "Attempted explicitly. A refusal passes; an acceptance fails the matrix" — the acceptance happened, and the compensating controls it assumed (`other-tenant-ca` being refused) do not hold. It is an accepted-and-demonstrated risk, not a mitigated one.

## Issues Encountered

- **The two red cases**, covered under "Decision required" above.
- **The live stack is shared.** `domo-tools:dev` is the image the main checkout's stack uses, and rebuilding it from this worktree replaced it. Nothing else in the wave depends on that image, but a future parallel plan touching the Rust tools would collide here.
- **The tenant was left clean.** The final action was `just smoke-teardown`, so the forged certificate is revoked, every prefixed fixture is gone, and `just authz-verify` passes with 17 green lines. Plan 01-07 starts from the same state plan 01-04 left.

## Next Phase Readiness

Ready for **01-07**, with one condition.

- **`just phase-verify` will inherit a red `just smoke`** unless the decision above is taken first. Plan 01-07 Task 3 builds that gate; it needs to know whether a confirmed-and-recorded AXIAM gap counts as a gate failure.
- **`just demo-reset` can now be written against a working teardown.** `smoke-teardown` is the narrow, prefix-scoped version; a full reset is a superset of it.
- **Phase 2 must avoid the `smoke-` prefix**, on slugs and on account names. This is the plan's one costly commitment.
- **The matrix is ready to become Phase 3's load driver.** Cases are data and the runner is separate, which is why it was built that way rather than as a script.

---
*Phase: 01-foundation*
*Completed: 2026-09-21*

## Self-Check: PASSED

- All 9 claimed created files and all 5 modified files exist on disk (`[ -f ]` on each).
- All 4 claimed commits resolve in `git log 9be236a..HEAD`, in the order claimed.
- `commits: 4` is measured, not narrated: `git rev-list --count 9be236a..HEAD` = 4.
- Plan `<verification>` re-run at close-out, with results reported honestly:
  - `just smoke-tree` twice — second run creates nothing, zero `✗`. **PASS**
  - `just smoke-authz` — every named case green, twice consecutively. **PASS**
  - `just smoke` reports 13 `✓` at column zero (≥ 10 required). **PASS**
  - `just smoke && just smoke` — **FAIL**, by the two recorded AXIAM defects; case-for-case identical across both runs.
  - `just smoke-teardown && just smoke` — **FAIL** identically, i.e. reproducible from an emptied tenant.
  - Cross-tenant issuance outcome in `docs/dogfooding-findings.md`. **PASS** (DF-017 + DF-025)
  - Disk: 27–28 GB free on `/home` and `/`, above the 8 GB floor throughout; `docker build` cache reused rather than accumulating.
- `cargo clippy -p domo-bootstrap -p domo-probe --no-deps --all-targets -- -D warnings` clean.
- The 37 pre-existing unit tests in `domo-bootstrap` and `domo-common` still pass.
- The ancestor-ordering assertion was mutation-checked live: reversing the expected order fails the case.
- Scope respected: no file outside this plan's `files_modified` was touched except `stages/tree.rs` (a documented visibility change) and the three split files inside this plan's own module directories. **None of 01-03's paths were touched.** `STATE.md` and `ROADMAP.md` untouched.
- No negative case was made to pass by changing the thing it tests.
