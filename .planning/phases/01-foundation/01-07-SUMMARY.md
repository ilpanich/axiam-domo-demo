---
phase: 01-foundation
plan: 07
subsystem: operations
tags: [operability, preflight, resumable-bootstrap, reset, arm64, verification-gate, docs, device-twin]

requires:
  - "01-01: the tracer stack, the operator/generated compose guard markers, domo-bootstrap as a one-shot"
  - "01-02: guard-secrets, verify-pki, export-trust, the findings log and its audit contract"
  - "01-03: edge-verify, pg-verify, the Caddy single origin"
  - "01-04: authz-verify and the per-tenant provisioning stages"
  - "01-05: the Twin's strict RabbitMQ auth-backend forms and twin-test"
  - "01-06: the live smoke suite and its two confirmed AXIAM defects (DF-017, DF-025)"
provides:
  - "just up — ten named, resumable stages from a clean machine to a demo card (PLAT-01, PLAT-05)"
  - "just demo-reset — named-volume wipe that keeps the organization root (D-09, D-10)"
  - "just images / images-verify — amd64+arm64 images, asserted down to the ELF header (PLAT-02)"
  - "just verify — the fast gate, grouped by the ROADMAP's four success criteria"
  - "just phase-verify — reset → verify → smoke → reset → smoke, known-red by user decision"
  - "just smoke-gate — the smoke suite with attribution of the known pair and regression detection"
  - "_containers-fresh — no running service may outlive the image its tag names"
  - "docs/setup.md, .env.example, CLAUDE.md build/test commands"
affects: [phase-02, phase-03, phase-06, device-twin, domo-bootstrap, domo-probe]

actuals:
  tokens: 46700
  tasks: 3
  commits: 12
  plan_head_before: 420d0b6bc3c6935cb0972b96b2b2548938f82766
  plan_head_after: 563706a38d907fc9272cdda13c5297096beeeef3

tech-stack:
  added: []
  patterns:
    - "Stage markers are a skip hint, never truth: skip only on marker AND a read-only re-probe"
    - "Gates are generated from the source of truth they check (compose guard markers → .env.example; DF citations → findings entries)"
    - "A known-red gate stays red and is ATTRIBUTED, never pinned; any extra failure is reported as a regression"
    - "A gate detects, a recipe repairs: verify reports a stale container, _containers-fresh recreates it"

key-files:
  created:
    - just/verify.just
    - .env.example
    - scripts/preflight.sh
    - tools/domo-bootstrap/src/state.rs
    - docs/dogfooding-issues/DF-001.md … DF-027.md (27 files)
  modified:
    - just/stack.just
    - just/smoke.just
    - tools/domo-bootstrap/src/checklist.rs
    - tools/domo-bootstrap/src/main.rs
    - deploy/docker/Dockerfile.rust
    - deploy/compose.yml
    - deploy/pki/listeners.conf
    - scripts/verify-pki.sh
    - services/device-twin/src/rmq.rs
    - services/device-twin/src/rmq/decide.rs
    - services/device-twin/src/rmq/forms.rs
    - services/device-twin/tests/rmq_user.rs
    - services/device-twin/tests/rmq_resource_topic.rs
    - docs/setup.md
    - docs/dogfooding-findings.md
    - CLAUDE.md

key-decisions:
  - "The arm64 route that worked is CROSS-COMPILATION on the amd64 host ($BUILDPLATFORM builder → $TARGETPLATFORM distroless runtime, GNU cross toolchain for aws-lc-sys/ring). ~2m30s per image. Native-on-the-Pi remains the documented fallback, unused."
  - "The demo publishes RabbitMQ's management console on HOST port 15673 (container port unchanged) so it coexists permanently with the sibling AXIAM dev stack, which holds 15672. User decision."
  - "The Twin admits the broker's `client_id` on /rmq/vhost and /rmq/resource BY NAME and holds it to the CN=<username> binding when present; deny_unknown_fields stays for everything else."
  - "`just phase-verify` exits non-zero by design: the smoke suite stays red on DF-017/DF-025 until the upstream fix is verified here. User decision, reaffirmed; nothing pinned or weakened."
  - "The findings log's upstream issue bodies live in docs/dogfooding-issues/; the audit gate asserts the two id sets are equal and the log stays under 500 lines."
  - "Plan 01-06's live matrix is INVALIDATED as evidence about the hardened Twin: it ran against a Twin container still on the 01-01 tracer image. This plan's 10/2 result is the first valid one."

requirements-completed: [PLAT-01, PLAT-02, PLAT-05, PLAT-06, PKI-05]

coverage:
  - id: D1
    description: "One command from a clean machine to a running stack, ten named stages, ending in a demo card"
    requirement: PLAT-01
    verification:
      - kind: command
        ref: "just up — ✓ for every stage, `resuming at: axiam-up` after pki was skipped, ends ✓ up (2026-09-29, and again inside both phase-verify resets)"
        status: pass
    human_judgment: false
  - id: D2
    description: "An interrupted run resumes at the failed stage"
    requirement: PLAT-05
    verification:
      - kind: command
        ref: "DOMO_FAIL_AT=catalog just up then just up → `resuming at: catalog` (executed live by the previous executor, recorded in 2db2f7b's message; not re-run this session)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Reset is idempotent across two consecutive cycles, removes only named volumes, and preserves the organization root"
    requirement: PLAT-05
    verification:
      - kind: command
        ref: "just phase-verify (10:14–10:18Z) — both resets ✓, root 63:4D:9F:…:11:61 before and after, cycles case-for-case identical"
        status: pass
      - kind: command
        ref: "just verify topology group — no blanket volume prune anywhere (negative-tested by injecting one)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every image in the stack resolves arm64, ours down to the ELF header"
    requirement: PLAT-02
    verification:
      - kind: command
        ref: "just images && just images-verify (rebuilt 2026-09-29 AFTER the Twin fix) — 7 registry manifests list linux/arm64; domo-twin and domo-bootstrap are real aarch64 ELFs"
        status: pass
    human_judgment: false
  - id: D5
    description: "The same `just up` actually runs on the Raspberry Pi 5"
    requirement: PLAT-02
    human_judgment: true
    rationale: "user_setup — only the operator can reach the second machine. Not run. dist/images/*-arm64.tar are current as of this plan and include the Twin fix."
  - id: D6
    description: "Single origin serves AXIAM's OIDC discovery document and JWKS (closes 01-03's deferred 200s)"
    requirement: PLAT-06
    verification:
      - kind: command
        ref: "just edge-verify — /.well-known/openid-configuration → 200, /oauth2/jwks → 200; issuer https://domo.local, one OKP/Ed25519 key"
        status: pass
    human_judgment: false
  - id: D7
    description: "verify-pki's live branch asserts per listener, SNI included (closes 01-02's deferred verify_live)"
    requirement: PKI-05
    verification:
      - kind: command
        ref: "just verify-pki — 14 TLS 1.3 assertions across axiam-server, rabbitmq and caddy; caddy now asserts instead of skipping"
        status: pass
    human_judgment: false
  - id: D8
    description: "The fast gate, grouped by the four ROADMAP success criteria, with the findings audit and operator-docs drift check"
    requirement: PLAT-01
    verification:
      - kind: command
        ref: "just verify — ✓ verify on the live stack; seven failure modes negative-tested (unresolved DF id, uncited /api/v1 file, entry without issue file, undocumented operator key, generated key offered, volume prune, CLAUDE.md missing a gate)"
        status: pass
    human_judgment: false
  - id: D9
    description: "Demo card printed and written at 0600 with all five fields"
    requirement: PKI-05
    verification:
      - kind: command
        ref: ".secrets/demo-card.txt mode 600; Portal, Console, super-admin, SHA256, export-trust all present"
        status: pass
    human_judgment: false
  - id: D10
    description: "Browsers trust the portal and console without a warning; docs/setup.md followed literally on a fresh machine reaches a demo card"
    requirement: PKI-05
    human_judgment: true
    rationale: "End-of-phase human check. RESEARCH A7 (p11-kit pickup by Chrome/Firefox) remains [ASSUMED]; no agent can sit at a browser or a fresh machine."

duration: "~1h05m this session (09:23–10:28Z); tasks 1–2 on 2026-09-21 in an earlier, interrupted session"
completed: 2026-09-29
status: complete
---

# Phase 1 Plan 7: Operability and the Whole-Phase Gate Summary

**One command to a running stack and a demo card; a reset that keeps the root; arm64 images proven down to the ELF header; and a phase gate that ran twice green around exactly two attributed, deliberately-red AXIAM defects. That gate also found a real bug: the hardened Twin had never met a real broker until this plan.**

## Performance

- **This session:** 2026-09-29, 09:23Z → 10:28Z (~65 min), task 3 plus close-out
- **Tasks 1–2:** 2026-09-21, a previous executor; stopped mid-task-3 for a machine shutdown (WIP `adf734b`)
- **Tasks:** 3 of 3
- **Commits:** 12 in `420d0b6..563706a`. 01-07's own are 10. `f101316` is the orchestrator's beta17 reconciliation and `637188f` is a worktree merge.
- **`just phase-verify`:** 4m46s for both cycles

## Accomplishments

- **`just up` / `demo-reset` / preflight** (task 1, `2db2f7b`): ten stages, resumable, markers as hints only, reset by explicit volume name.
- **arm64** (task 2, `83c0f50`): cross-compiled images, `images-verify` checking the manifest AND the binary's ELF machine.
- **The whole-phase gate** (`3214461`): `verify`, `smoke-gate`, `phase-verify`, all grouped by the ROADMAP's four criteria.
- **The findings audit**, now covering the case that matters (a new hand-rolled call with no finding), and the findings log split back under the 500-line cap (`dcfefa7`).
- **Operator documentation**: `.env.example` generated-and-gated from the compose markers; `docs/setup.md` end to end for both machines, with a troubleshooting section built only from pitfalls this phase hit.
- **Two carried deferred verifications closed on the live stack**: 01-03's OIDC 200s and 01-02's per-listener `verify_live`.
- **A Twin defect that blocked every device, found and fixed** (`f33b589`), and the tooling gap that hid it closed (`563706a`).

## The phase gate result

```
═══ cycle 1 · reset ═══    ✓   (✓ up, root reused — D-10)
═══ cycle 1 · verify ═══   ✓   (every group, all four criteria)
═══ cycle 1 · smoke ═══    matrix: 10 passed, 2 failed, 12 cases
                           ✗ smoke-gate  known-red: exactly the two attributed failures, nothing else
═══ cycle 2 · reset ═══    ✓
═══ cycle 2 · smoke ═══    matrix: 10 passed, 2 failed, 12 cases   — case-for-case identical
  root before = root after = 63:4D:9F:15:9C:13:66:01:A6:59:78:53:82:9C:50:50:5F:B7:FB:5B:EC:FB:F8:14:F0:09:2D:33:D6:A2:11:61
✗ phase-verify  known-red
```

**Did it pass on the first two consecutive cycles, or need cleanup between them?** The first attempt did not reach cycle 2. `smoke-gate` stopped it on cycle 1 as `known-red PLUS a regression` (see the Twin bug below). After the fix, the next run was clean across both cycles **with no state cleanup between them**: two real `demo-reset`s, identical results.

### The two red cases, as the gate states them

`cross-tenant-ca-issuance` (**DF-017**: AXIAM signs a leaf under another tenant's signing CA) and `other-tenant-ca` (**DF-025**: that leaf then connects, publishes and subscribes). Together they mean **certificate issuance is not a tenant boundary on the pinned build**. AXIAM 1.0.0-beta17 fixes DF-017 as **T22.1**, and DF-025 should go with it. **None of that is verified here**: the beta17 images are unpublished, so the demo stays on beta16, server and SDK together. The re-run list is in `docs/dogfooding-upstream-status.md`. Nothing was pinned, excluded, weakened or had its exit status discarded. A third failing case would print `✗✗ REGRESSION`, and on the first run it did.

## The Twin bug the gate found, and why 01-06 never saw it

**Symptom** (first `phase-verify`, cycle 1): `positive` and all three namespace cases failed with `CONNACK NotAuthorized`. Nothing connected, so the namespace cases were no longer testing namespaces at all.

**Cause**, read from the Twin at debug level: RabbitMQ 4.3.6 sends `client_id` on the `/rmq/vhost` and `/rmq/resource` checks of an MQTT connection. The Twin's `VhostReq`/`ResourceReq` are `deny_unknown_fields`, and 01-05's parameter list has no `client_id`, so every request failed to parse and was denied. CONNECT was allowed, which is why the broker only said `access refused … to vhost 'domo'`. The WIP commit's `deny_unparsed` logging is what made the cause readable in one step.

**Fix** (`f33b589`): the field is admitted by name on those two forms and **held to the `CN=<username>` binding when present**, through one shared helper, so it can only narrow access. Strictness is unchanged for any other field, and a test asserts that. The new endpoint tests post the live broker's exact shape and fail on the old code with the live symptom. After the fix, the matrix is 10/2 with only the attributed pair.

**Why 01-06 passed against "the same broker": the broker was not the variable, the Twin container was.**

| Fact | Source |
|---|---|
| Image `028b0a1434f6` exists (untagged), created `2026-09-20T11:54:52+02:00` | **confirmed here** (`docker image inspect`) |
| Strict parsing landed in `d4ceeda` at `2026-09-20 13:25 +02:00`, 90 minutes later | **confirmed here** (`git log`) |
| On 2026-09-21 `domo-twin:dev` was `028b0a1434f6`; the post-wave-2 `just build` re-tagged it; nothing recreated the running container; at shutdown `domo-device-twin-1` was still on `028b0a1434f6` | orchestrator's session evidence. **Not independently confirmable**: `demo-reset`'s `down` has since removed that container |

So **plan 01-06's entire live matrix ran against the tolerant pre-01-05 Twin**. Its SUMMARY's re-observation that the strict 4.3.6 forms "caused no spurious deny" was made against a Twin with no strict forms, and **is invalidated**. 01-06's SUMMARY is not edited. **The 10-passed/2-failed result in this plan, with the rebuilt Twin, is the first valid run of the hardened Twin against a real broker.** It failed on the first request, which is the risk 01-05 flagged in its own `forms.rs` header.

**The tooling gap, closed in code** (`563706a`): `_images-fresh` compared only *tagged* images to source, and a converged `just up` skips the stages that recreate containers. So **a fresh image is not a fresh container**. `_containers-fresh` now compares every running, non-one-off container's image with what its reference resolves to now, and recreates only the mismatches. It runs in `up`'s always-run `verify` stage and in front of `smoke-matrix`. `just verify` *detects* a stale container as a failing SC1 line without repairing it. I mutation-tested it by putting the Twin back on `028b0a1434f6`: the gate went red naming both ids, and the recipe repaired it. It then fired for real minutes later, after `just images` re-tagged `:dev`. This is our tooling, so it's documented in `docs/setup.md` and kept out of the dogfooding log.

## Stage names and marker format (later phases extend these)

`preflight pki axiam-up org-bootstrap tenants catalog service-certs broker platform-up verify`. `preflight` and `verify` are checks and never skipped; the other eight resume.

A marker is `.secrets/state/<stage>`: JSON `{"stage", "completed_at", "produced"}`, mode 0600, and **a skip hint only**. A stage is skipped only when its marker exists AND a read-only re-probe agrees. `pki-root.done` is `gen-pki.sh`'s root guard; `demo-reset` clears every marker except it. The zero-length `<name>.done` files are the per-substage markers `domo-bootstrap` has written since 01-01. `DOMO_FAIL_AT=<stage>` aborts at a stage's entry before any work or marker; `_stage` forwards it into the one-shot.

## The Raspberry Pi

**The arm64 route that worked: cross-compilation** on the amd64 laptop. The builder runs at `$BUILDPLATFORM`, cross-compiles to the target triple with a GNU cross toolchain for `aws-lc-sys`/`ring`, and the runtime stage only `COPY`s into the target's distroless base. About 2m30s per image. The first cut shipped an `arm64` config around an **x86-64 binary**, because `ARG TARGETPLATFORM=linux/amd64` shadowed BuildKit's value. Only the ELF check caught it, which is why `images-verify` checks both. Native build on the Pi stays the documented fallback and was never needed.

**The Pi's first-run behaviour was not observed.** That is `user_setup`, and no agent can reach the machine. `docs/setup.md` gained nothing Pi-specific from observation. `dist/images/domo-{twin,tools}-arm64.tar` **were rebuilt in this plan after `f33b589`**: the 7-day-old ones predated the Twin fix and would have refused every device on the Pi exactly as here.

## Carried items (14)

| # | Item | Where |
|---|---|---|
| 1 | Image-vs-source freshness | `_images-fresh`, task 1; **extended to containers** in `563706a` |
| 2 | `verify-pki` SNI | WIP `adf734b`; **now asserting live** (14 rows, caddy included) |
| 3 | clippy `collapsible_if` | fixed in task 1 (`2db2f7b`); workspace passes `-D warnings` |
| 4 | PEM **block** test, not header count | `guard-secrets` already does it; wired into `verify` as `secrets-guard` |
| 5 | `demo-reset` markers/volumes | task 1; asserted statically in `verify`, dynamically in `phase-verify` |
| 6 | `LOGIN_PER_MIN=120` warning | banner at the top of `docs/setup.md` |
| 7 | `TRUSTED_HOPS` stays 0 | `docs/setup.md` § Notes for the phases that come next |
| 8 | mDNS names | preflight prints `avahi-publish`; guide § Configure |
| 9 | `just authz` does not create tenants | troubleshooting, with the by-hand recovery order |
| 10 | `smoke-` reserved prefix | `docs/setup.md` § Notes for the phases that come next |
| 11 | 01-03 OIDC 200s, 01-02 `verify_live` | **closed live** (coverage D6, D7) |
| 12 | Browser trust check | still human; flagged in `verify`'s SC2 and in D10 |
| 13 | T-06-04 is accepted-and-demonstrated, not mitigated | `docs/setup.md` § Notes; 01-06 not rewritten |
| 14 | Findings log over 500 lines | split to `docs/dogfooding-issues/`, 449 lines, gated |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] preflight missed a published port (15672)** — `acd860d`
- **Found during:** task 3, bringing the stack up
- **Issue:** the port list held the three LAN-facing ports, not every bind that can fail. The sibling AXIAM stack held 15672, so preflight passed and then compose would have failed halfway.
- **Fix:** every published host port is checked, and the refusal names the holding container. Since `ab4ba49` the port is 15673.
- **Files:** `scripts/preflight.sh`

**2. [Rule 2 - Missing critical] the demo card sent presenters to a red suite without a word** — `3c1d090`
- **Fix:** the card points at `just smoke-gate` and names the expected pair and what it means. **Files:** `tools/domo-bootstrap/src/checklist.rs`

**3. [Rule 1 - Bug] the Twin refused every device at the vhost and resource checks** — `f33b589`
- Covered in full above. **Files:** `services/device-twin/src/rmq{.rs,/decide.rs,/forms.rs}`, two test files. Twin tests: `rmq_user` 21 → 24, all green.

**4. [Rule 1 - Bug] a running container could outlive the image its tag names** — `563706a`
- Covered above. **Files:** `just/stack.just`, `just/smoke.just`, `just/verify.just`, `docs/setup.md`

**5. [Rule 2 - Missing critical] the findings audit never checked for an uncited hand-rolled call** — `3214461`
- The planned gate checked only that citations resolve. It now also requires every file naming an `/api/v1` path to cite an id. This is the case that actually rots a dogfooding log.

### User-directed change (checkpoint resolved)

**6. Host port 15673 for the demo broker's management console** — `ab4ba49`. The sibling AXIAM dev stack holds 15672 and stays untouched. Only the host side of the publish changed. Every in-network `rabbitmq:15672` reference was classified and left alone, and the classification is in the commit body. Preflight also found no other collision: 443, 8090 and 8883 are free, and the sibling's 8000 and 5671 are not ports this demo publishes.

---

**Total deviations:** 5 auto-fixed (2 bugs, 3 missing-critical), 1 user-directed. **Impact:** deviations 3 and 4 were necessary for the plan's central claim to mean anything. Without them the phase gate would have passed a smoke run against a Twin that had never been exercised.

## Issues Encountered

- **The live half was blocked** by the 15672 collision and returned as a checkpoint. The user resolved it with the host-port move.
- **`just images` (buildx) and `just build` (compose) give the same source different image ids**, so each re-tags `:dev` over the other. With `_containers-fresh` that is now harmless, and it's how that check fired for real, but it is churn.
- **`_images-fresh` false-positives after an mtime-only change**: a cached rebuild does not refresh the image's `Created`, so it keeps rebuilding (from cache, in seconds) until a real layer changes. Harmless and not fixed.

## Deferred Issues

- **The workspace is not rustfmt-clean.** `cargo fmt --all` rewrites 28 files nobody touched in this plan. I reverted that churn and kept this plan's diff minimal. Adding `cargo fmt --check` to `just verify` should be a deliberate one-commit change, not a side effect.
- **`_images-fresh` mtime heuristic** (above).

## Known Stubs

- `tools/domo-bootstrap/src/checklist.rs`, the demo card's "Seeded users" section reads *"(none yet — … arrives in Phase 2 …)"*. Intentional (D-36): Phase 2's seed appends under that heading. It does not block this plan's goal.

(`.planning/WINDOWS.md` does not exist in this project, so nothing was appended to a broken-windows ledger.)

## Authentication Gates

None.

## User Setup Required

- **Raspberry Pi 5 (PLAT-02 live half):** copy `dist/images/domo-{twin,tools}-arm64.tar` (current, including the Twin fix), `docker load`, re-tag to `:dev`, create `.env` from `.env.example` with the Pi's own `DOMO_LAN_IP`, trust the Pi's root (`docs/trust.md`), run `just up`, and confirm every image reports `arm64`. `docs/setup.md` § Running on the Raspberry Pi 5 has the steps.
- **End-of-phase human check:** walk `docs/setup.md` literally on a machine that has never run the demo, then open `https://domo.local/` and `https://axiam.domo.local/` in Chrome and Firefox with no certificate warning. This settles RESEARCH A7.

## Next Phase Readiness

Phase 1 is complete and closes with a **known-red phase gate by explicit user decision**. Phase 2 inherits:
- a Twin whose strict forms have now actually met RabbitMQ 4.3.6;
- the reserved `smoke-` prefix;
- "service accounts are not management principals" (unless beta17's T22.13 changes it, which is unverified);
- a findings log with a two-file entry shape the gate enforces.

When beta17 images appear, `docs/dogfooding-upstream-status.md` lists the re-run order, and the same `just phase-verify` will say whether T22.1 holds.

## Self-Check: PASSED

- Files: `just/verify.just`, `.env.example`, `docs/setup.md`, `scripts/preflight.sh`, `tools/domo-bootstrap/src/state.rs`, `tools/domo-bootstrap/src/checklist.rs`, and 27 files in `docs/dogfooding-issues/`: all present.
- Commits `2db2f7b 83c0f50 adf734b dcfefa7 acd860d 3214461 3c1d090 ab4ba49 f33b589 563706a`: all in `git log`.
- Plan acceptance commands, re-run at close-out:
  - demo card fields and mode 600 ✓
  - CLAUDE.md commands ✓
  - `.env.example` covers every `operator` guard ✓
  - 0 unmarked compose guards (36 = 11 operator + 25 generated) ✓
  - `just verify` ✓
  - `just phase-verify` = `✗ known-red` with only the attributed pair, as required ✓
