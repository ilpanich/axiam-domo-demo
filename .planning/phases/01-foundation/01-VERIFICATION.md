---
phase: 01-foundation
verified: 2026-09-29T15:20:00Z
status: gaps_found
score: 7/10 must-haves verified
covered_files:
  - .planning/phases/01-foundation/01-01-PLAN.md
  - .planning/phases/01-foundation/01-01-SUMMARY.md
  - .planning/phases/01-foundation/01-02-PLAN.md
  - .planning/phases/01-foundation/01-02-SUMMARY.md
  - .planning/phases/01-foundation/01-03-PLAN.md
  - .planning/phases/01-foundation/01-03-SUMMARY.md
  - .planning/phases/01-foundation/01-04-PLAN.md
  - .planning/phases/01-foundation/01-04-SUMMARY.md
  - .planning/phases/01-foundation/01-05-PLAN.md
  - .planning/phases/01-foundation/01-05-SUMMARY.md
  - .planning/phases/01-foundation/01-06-PLAN.md
  - .planning/phases/01-foundation/01-06-SUMMARY.md
  - .planning/phases/01-foundation/01-07-PLAN.md
  - .planning/phases/01-foundation/01-07-SUMMARY.md
  - .planning/phases/01-foundation/01-08-PLAN.md
  - .planning/phases/01-foundation/01-08-SUMMARY.md
  - just/pki.just
  - just/verify.just
  - scripts/verify-pki.sh
covered_digest: "v2:sha256:4927b19b6b206f4111be982119037ea4c3bd66b14156cd81707b4cf504b0bca0"
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "`just verify` ends `✓ verify` on the live stack, and on every run its SC2 group has asserted every declared name of the rabbitmq 8883 row against the one organization root over TLS 1.3 (01-08 must-have 1)"
    status: failed
    reason: "tls_handshake_verified passes -servername (SNI only) and never -verify_hostname / -verify_ip, so each per-name '✓ TLS 1.3 verified for <name>' line repeats the same chain-only handshake. Reproduced live with the script's own predicate: 192.168.144.20:8883 with -servername definitely-not-a-san.invalid returns 0. IP entries are worse: the suite prints '✓ axiam-server TLS 1.3 verified for 192.168.144.20 on 127.0.0.1:8090' without any IP check. Review finding CR-01 is CONFIRMED. The served certificates do in fact carry every declared name (verified separately with -verify_hostname/-verify_ip, all 14 rc=0), so the PKI is correct; the gate simply does not prove it."
    artifacts:
      - path: "scripts/verify-pki.sh"
        issue: "verify_live per-entry loop (around lines 435-455) builds sni=(-servername NAME) for DNS and sni=() for IP; no identity check reaches tls_handshake_verified"
      - path: "just/pki.just"
        issue: "verify-pki-stress 'N name(s) verified' counts repetitions of the chain check, not names; no negative control exercises a name mismatch"
    missing:
      - "DNS entries: pass -servername NAME -verify_hostname NAME; IP entries: pass -verify_ip ADDR (live probes show both reject a wrong identity with rc=1 and accept every declared one with rc=0)"
      - "A real-predicate negative control (e.g. NC9) asserting tls_handshake_verified refuses -verify_hostname not-a-san.invalid and -verify_ip 10.9.9.9 on the rabbitmq 8883 endpoint"
      - "Update the helper comment to say the verdict includes 'the certificate covers NAME'"
  - truth: "The verdict can still fail: ... a row whose names do not verify fails verify-pki. Each of these is asserted by the guard's negative controls on every run, not inferred (01-08 must-have 3)"
    status: partial
    reason: "Plain HTTP (NC1) and a foreign chain (NC2) are refused by the real predicates. But 'names do not verify' is only exercised by NC6, which replaces tls_handshake_verified with 'return 1'. The real predicate cannot fail on a name mismatch (same root cause as the gap above), so this branch is inferred from a mock, not asserted."
    artifacts:
      - path: "just/pki.just"
        issue: "NC6 overrides tls_handshake_verified entirely"
    missing:
      - "Covered by the NC9 real-predicate name-mismatch control listed above"
  - truth: "While the stack is up, a server row whose service publishes a port is never skipped ... a run that checked nothing is a failure (01-08 must-have 4)"
    status: partial
    reason: "Holds on the gate path (`just verify-pki`, `just verify`: only device-twin and postgres skip). Fails open on the script's own documented direct usage: with the stack up, `env -i PATH=... HOME=... DOMO_LAN_IP=192.168.144.20 bash scripts/verify-pki.sh` prints '→ skipped (stack down)' and '✓ verify-pki', exit 0, because service_running swallows compose's 'required variable COMPOSE_PROJECT_NAME is missing a value' error. Review finding WR-01 CONFIRMED live. WR-02 (published_endpoints swallows compose failure and turns it into a 'no published port' skip) is the same failure class, confirmed by code reading."
    artifacts:
      - path: "scripts/verify-pki.sh"
        issue: "service_running and published_endpoints discard compose stderr and exit status (around lines 344-372)"
    missing:
      - "Distinguish 'compose failed' from 'nothing running' in service_running/stack_is_up and fail loudly on the former (or require COMPOSE_PROJECT_NAME in main when docker is on PATH)"
      - "Propagate published_endpoints' compose exit status instead of '|| true', so a compose error on a publishing row fails rather than skips"
human_verification:
  - test: "Run `just up` from a clean checkout on the Raspberry Pi 5 (8 GB, arm64, Raspberry Pi OS)"
    expected: "All ten stages print ✓, the run ends `✓ up` with the demo card; `just verify` is green there"
    why_human: "Needs the physical arm64 host; UAT test 11 was skipped. Multi-arch images were verified on amd64 (UAT 9, `just images-verify`), not the run itself (PLAT-02)"
  - test: "Follow docs/setup.md on a fresh machine to a demo card"
    expected: "No undocumented step; ends with the demo card"
    why_human: "Needs a fresh machine; UAT test 12 was skipped"
---

# Phase 1: Foundation Verification Report

**Phase Goal:** The platform boots from a single trust anchor, AXIAM's tenant and resource/role model is established, and a device can already prove it can reach the broker end-to-end.
**Verified:** 2026-09-29T15:20:00Z
**Status:** gaps_found
**Re-verification:** No — initial VERIFICATION.md (plans 01-01..01-07 were UAT'd in 01-UAT.md, 57 pass / 1 issue G-01-3; this run verifies the phase after gap-closure plan 01-08)

## Summary verdict

The phase goal itself is achieved. The four roadmap success criteria hold on the live stack: `just verify` is green, the served certificates are correct, the tree and group pattern are queryable, and the Twin backend is allowing real device CONNECTs. What fails is gap-closure plan 01-08's own contract. G-01-3 existed to stop the phase gate from reporting assurance it never produced. The race is fixed (50/50 under 24 CPU hogs), but the gate still prints "TLS 1.3 verified for <name>" without checking the name. Code review finding CR-01 is confirmed from the source and live against rabbitmq 8883. That is a BLOCKER on 01-08's first must-have, and it is not deferred to any later phase.

## Goal Achievement

### Observable Truths

Roadmap success criteria (contract):

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC1 | One command brings up AXIAM, PostgreSQL and Caddy; `just demo-reset` wipes and rebuilds idempotently (PLAT-01, PLAT-02, PLAT-05) | ✓ VERIFIED | `just verify` (run by verifier, rc=0): "SC1 every stage marker present, every running container on its current image, demo card at 0600". `.secrets/state/` holds all stage markers. UAT 1 (cold demo-reset), 2 (resume at failed stage) and 8 (two-cycle `phase-verify`) passed. The Pi half is human-only; see Human Verification. |
| SC2 | Single org root, BYOK import, one tenant signing CA per tenant, browsers trust Caddy's offline-signed SAN certs (PKI-01..06) | ✓ VERIFIED | `just verify-pki` green: root PKCS#8, RSA-4096, no pathlen; lakeside-ca and summit-ca chain to the root. Verifier's own live probes: the served leaves on 8883, 8090 and 443 are issued by "Domo Demo Root CA" and carry the listeners.conf SANs, and every declared name/IP verifies with `-verify_hostname`/`-verify_ip` (14/14 rc=0). The secrets guard is green. UAT 10 (Chromium/Firefox trust) and UAT 13-16 passed. |
| SC3 | Single Caddy origin; resource tree and group-per-(role, resource) pattern exist and are queryable (PLAT-06, AUTHZ-01, AUTHZ-02) | ✓ VERIFIED | Live `just verify`: edge-verify (single origin, route matchers, TLS 1.3, AXIAM discovery docs), pg-verify, and authz-verify (the tree, the catalog, the group pattern) are all ✓. UAT 31-47 passed. |
| SC4 | A test device does mTLS login, gets a JWT, and CONNECTs to `domo` with cert + JWT, validated by a working HTTP auth backend (MQTT-01, MQTT-02) | ✓ VERIFIED | Live container logs from 14:44: the device-twin logged `CONNECT allowed account=01a0ed16-dc50-… tenant=01a0ed16-5ad0-…`, and rabbitmq logged `Accepted MQTT connection … :8883 for client ID CN=01a0ed16-dc50-…` followed by the backend's cross-tenant topic refusals. twin-test is green. UAT 17-19 and 48-60 passed. `just smoke` stays red only on DF-017/DF-025 (upstream AXIAM defects, by user decision). |

Plan 01-08 must-haves (gap closure G-01-3):

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `just verify` ends ✓ and SC2 "has asserted every declared name of the rabbitmq 8883 row" | ✗ FAILED | First half holds (`just verify` rc=0). Second half is false: `tls_handshake_verified 192.168.144.20:8883 root.pem -servername definitely-not-a-san.invalid` → rc=0. With `-verify_hostname definitely-not-a-san.invalid` → rc=1; with `-verify_ip 10.9.9.9` → rc=1. The script passes neither flag, so the five "✓ rabbitmq TLS 1.3 verified for …" lines are one chain check repeated five times. CR-01 is confirmed. |
| 2 | Verdict independent of the post-handshake alert 116; rabbitmq under 2×nproc hogs: zero failures, zero skips | ✓ VERIFIED | Verifier ran `just verify-pki-stress`: "rabbitmq: 50/50 under 24 CPU hogs (loadavg 22.36), 0 failures, 0 skips". The helpers judge by `write finished` + `New, TLSv1.3` + `Verify return code: 0 (ok)` and discard the exit status. |
| 3 | Plain HTTP not counted as TLS, foreign chain refused, not-running / no-TLS / name-not-verifying rows fail, each asserted by negative controls | ✗ PARTIAL | NC1 (plain HTTP) and NC2 (throwaway CA) exercise the real predicates and pass. NC4/NC5 are fine as branch tests. NC6 ("name does not verify") mocks `tls_handshake_verified` with `return 1`; the real predicate can never fail on a name. |
| 4 | While the stack is up, a publishing row is never skipped; only unpublished rows skip; `just verify` prints skip lines | ✗ PARTIAL | Gate path OK: `just verify` shows exactly `device-twin: no published port` and `postgres: no published port`. Direct documented usage fails open: without COMPOSE_PROJECT_NAME and with the stack up, the run prints `→ skipped (stack down)` then `✓ verify-pki`, rc=0 (WR-01 reproduced). WR-02 is the same class. |
| 5 | No retries; one handshake per name per endpoint; no client cert or key | ✓ VERIFIED | The helper bodies make one `openssl s_client` call each. The loop in `verify_live` iterates distinct `tls_addrs` and breaks on the first success. No `-cert`/`-key` appears anywhere in `scripts/verify-pki.sh`. |
| 6 | `deploy/rabbitmq/` byte-identical (verify_peer, fail_if_no_peer_cert, TLS 1.3); `just smoke` untouched | ✓ VERIFIED | `git diff --quiet 8a6c4ba -- deploy/rabbitmq/` passes. `git diff --quiet 8a6c4ba -- just/smoke.just tools/domo-probe/ services/` passes. `20-tls.conf:47-48` still sets verify_peer and fail_if_no_peer_cert. The 01-08 diff touches only verify-pki.sh, pki.just and verify.just. |

**Score:** 7/10 truths verified (0 present, behavior-unverified)

### Prohibitions (01-08)

| Prohibition | Tier | Verifier disposition |
|---|---|---|
| Retries are not the fix | judgment | Resolved. Code read, no re-run of a failed handshake (truth 5). Non-authoritative LLM judgment. |
| `deploy/rabbitmq/` must not change | test | Resolved. `git diff --quiet 8a6c4ba -- deploy/rabbitmq/` exit 0. |
| `just smoke` stays red on DF-017/DF-025, untouched | test | Resolved by diff: smoke.just, domo-probe and services are unchanged. `just smoke-gate` was not re-run (it mutates AXIAM state); UAT 6 recorded exactly the two known reds. |
| No client certificate presented by the probe | judgment | Resolved. No `-cert`/`-key` in the script. Non-authoritative LLM judgment. |

### Required Artifacts (01-08)

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `scripts/verify-pki.sh` | Marker-based `tls_answers`/`tls_handshake_verified`, `service_running`, no jq, `--live-only`, source guard | ⚠️ VERIFIED with defect | All present and wired. The predicate lacks the identity check (gap 1). Compose errors are swallowed (gap 3). |
| `just/pki.just` | `verify-pki-stress` recipe | ✓ VERIFIED | Runs, sources the real script (`branch()` does `source scripts/verify-pki.sh`), 50/50 green. Missing a name-mismatch control. |
| `just/verify.just` | `group()` surfaces `→ skipped` from passing recipes | ✓ VERIFIED | Observed in the live `just verify` output. WR-03 (group returns 1 on a clean pass) confirmed by reading; no caller reads the status. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `verify_live` | `tls_answers` / `tls_handshake_verified` | direct calls | WIRED | Every live handshake goes through the two helpers. |
| `just verify` SC2 | `just verify-pki` → `scripts/verify-pki.sh` | `group` | WIRED | Skip lines are visible in the live output. |
| `just verify-pki-stress` | `scripts/verify-pki.sh --live-only` + sourced functions | source guard | WIRED | Real functions, not a copy. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Phase fast gate | `just verify` (once, 32G free before, incremental cleaned after) | `✓ verify`, all groups ✓ | ✓ PASS |
| G-01-3 race gone | `just verify-pki-stress` | 50/50, 0 failures, 0 skips, NC1-NC8 ✓ | ✓ PASS |
| Gate checks names | `tls_handshake_verified …:8883 -servername definitely-not-a-san.invalid` | rc=0 (should be 1) | ✗ FAIL |
| Correct predicate is feasible | same with `-verify_hostname` bogus / `-verify_ip 10.9.9.9` | rc=1 / rc=1; declared names rc=0 (14/14) | ✓ PASS |
| Stack-up detection fails closed | `env -i … bash scripts/verify-pki.sh` with stack up | `skipped (stack down)`, `✓ verify-pki`, rc=0 | ✗ FAIL |

### Probe Execution

No `scripts/*/tests/probe-*.sh` are declared by this phase. The phase's runnable checks are the `just` recipes above, which the verifier ran itself.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| PLAT-01 | 01-01, 01-03, 01-07 | ✓ SATISFIED | SC1; UAT 1, 22 |
| PLAT-02 | 01-07 | ? NEEDS HUMAN | arm64 images verified (UAT 9); the Pi run was skipped (UAT 11) |
| PLAT-05 | 01-07 | ✓ SATISFIED | UAT 1, 2, 8 (two-cycle phase-verify) |
| PLAT-06 | 01-03, 01-07 | ✓ SATISFIED | edge-verify live ✓ |
| PKI-01 | 01-01, 01-02, 01-08 | ✓ SATISFIED | verify-pki root group; UAT 13-14 |
| PKI-02 | 01-01, 01-04 | ✓ SATISFIED | lakeside/summit CAs chain to root; authz-verify signing-cas; UAT 15 |
| PKI-03 | 01-01, 01-04, 01-06 | ✓ SATISFIED | UAT 16; service accounts with a certificate from their own tenant CA (UAT 4) |
| PKI-04 | 01-01, 01-02, 01-03, 01-08 | ✓ SATISFIED (the gate's assurance of it is the gap) | Served SANs checked by the verifier directly; UAT 10 browsers |
| PKI-05 | 01-02, 01-07 | ✓ SATISFIED | fingerprint round-trip ✓ in `just verify`; UAT 27 |
| PKI-06 | 01-01, 01-02 | ✓ SATISFIED | secrets-guard ✓; UAT 20, 29 |
| AUTHZ-01 | 01-04, 01-06 | ✓ SATISFIED | authz-verify tree ✓ |
| AUTHZ-02 | 01-04, 01-06 | ✓ SATISFIED | authz-verify group pattern ✓; UAT 45-46 |
| MQTT-01 | 01-01, 01-05, 01-06 | ✓ SATISFIED | live `Accepted MQTT connection … :8883`; UAT 18 |
| MQTT-02 | 01-01, 01-05, 01-06 | ✓ SATISFIED | twin-test ✓; live `CONNECT allowed`; UAT 19, 48 |

Every roadmap requirement ID is claimed by at least one plan, and there are no orphans. **Bookkeeping drift (warning):** the traceability table in `.planning/REQUIREMENTS.md` still shows PKI-02, PKI-03, PKI-06, AUTHZ-01, AUTHZ-02, MQTT-01 and MQTT-02 as `Pending`, with unchecked boxes, even though the evidence above satisfies them for Phase 1's scope.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| scripts/verify-pki.sh | ~149-157, ~435-455 | Per-name ✓ printed without an identity check (CR-01) | 🛑 Blocker | False assurance in the phase gate, which is the failure class G-01-3 was opened to remove |
| scripts/verify-pki.sh | ~344-346 | `service_running` swallows compose errors → "stack down" green (WR-01) | ⚠️ Warning | False green on direct invocation (reproduced) |
| scripts/verify-pki.sh | ~365-372, ~398-402 | `published_endpoints` failure → "no published port" skip (WR-02) | ⚠️ Warning | Surfaced but green skip on a compose error |
| just/verify.just | 54-56 | `group` returns 1 on a clean pass under pipefail (WR-03) | ⚠️ Warning | Latent: no caller reads it today |
| just/pki.just | 131 | Hog lifetime fixed at 600 s (WR-04) | ⚠️ Warning | Fails loudly at high iteration counts, not falsely green |
| scripts/verify-pki.sh | ~398-402 | device-twin/postgres never live-handshaken (IN-05) | ℹ️ Info | Visible skip, by design (D-05) |

There are no TBD/FIXME/XXX markers in the three 01-08 files.

### Human Verification Required

1. **Raspberry Pi 5 one-command run (PLAT-02).** Run `just up` on the Pi. Expected: `✓ up`, the demo card, and a green `just verify`. Why human: needs the arm64 hardware.
2. **Fresh-machine setup guide.** Follow docs/setup.md on a clean machine. Expected: it reaches the demo card with no undocumented step. Why human: needs a fresh host.

(Browser trust in Chromium and Firefox was already passed by a human in UAT 10. Plan 01-08 changed no certificate or Caddy config, so it is not re-requested.)

### Gaps Summary

All three gaps share one root cause and one file. `scripts/verify-pki.sh`'s live section still reports a pass it has not proven:

1. **Blocker (CR-01):** the per-name live check never checks the name. Fix: `-verify_hostname` for DNS SANs and `-verify_ip` for IP SANs, plus a real-predicate name-mismatch negative control in `verify-pki-stress`. The verifier confirmed live that this fix is correct and does not regress: all 14 declared names across rabbitmq, axiam-server and caddy verify with it, and a wrong name or IP is refused.
2. **Partial:** NC6 asserts the name-failure branch only through a mock. It is closed by the control in (1).
3. **Partial (WR-01/WR-02):** compose errors in `service_running` and `published_endpoints` become "stack down" or "no published port" skips. They should fail loudly.

The underlying PKI is correct and the phase goal is met on the live stack, so a narrow gap-closure plan (`/gsd-plan-phase 01 --gaps`) against `scripts/verify-pki.sh` and `just/pki.just` should be enough to close the phase.

---

_Verified: 2026-09-29T15:20:00Z_
_Verifier: Claude (gsd-verifier)_
