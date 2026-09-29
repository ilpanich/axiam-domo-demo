---
phase: 01-foundation
verified: 2026-09-29T16:11:24Z
status: human_needed
score: 17/17 must-haves verified
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
  - .planning/phases/01-foundation/01-09-PLAN.md
  - .planning/phases/01-foundation/01-09-SUMMARY.md
  - just/pki.just
  - just/verify.just
  - scripts/verify-pki.sh
covered_digest: "v2:sha256:85cdd71eb3d99c825dfcc2476d1537d0630e7234b240f72619896084d411539f"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 7/10
  gaps_closed:
    - "01-08 must-have 1: every `✓ <row>  TLS 1.3 verified for <name>` line now follows an identity-checked handshake (-verify_hostname / -verify_ip); CR-01 closed"
    - "01-08 must-have 3: the name-mismatch branch is asserted with the REAL predicate (NC9) and at the REAL call site (NC10), not only through NC6's mock"
    - "01-08 must-have 4: a compose failure fails the run instead of reading as \"stack down\" (WR-01) or \"no published port\" (WR-02); NC11-NC13"
  gaps_remaining: []
  regressions: []
advisory:
  - finding: "`scripts/verify-pki.sh --live-only ROW` prints ✓ after zero identity checks when the row's SAN field expands to empty (review WR-01). `checked` counts rows, not verified identities."
    category: other
    reason: "Reproduced only by overriding expand_sans; no row in deploy/pki/listeners.conf has an empty server SAN field. Full mode refuses such a row first (verify_listeners line ~248) and verify-pki-stress refuses it through its declared-count precondition, so no gate path and no phase must-have is affected. Resolve by counting verified identities per row and failing a row with zero."
    evidence_status: "fixture-only reproduction; none on the real config"
  - finding: "NC12's expected text `docker compose ps failed` is also a substring of service_running's message, so a future refactor of service_running to use `--format` would make NC12 vacuous (review WR-02)."
    category: other
    reason: "Today NC12 does reach the published-ports branch: the verifier ran its exact override and got `✗ rabbitmq  docker compose ps failed while reading its published ports; …`. The weakness is future fragility. Resolve by matching the branch-specific text `while reading its published ports`."
    evidence_status: "current behaviour verified correct; risk is prospective"
  - finding: "A compose call that SUCCEEDS but reports no publishers still skips a row as `no published port` (review WR-03). The unpublished set (device-twin, postgres) is inferred at runtime, not declared."
    category: other
    reason: "01-09's must-have covers compose FAILURES only, and that holds (verified live). A row that truly stops publishing is shown as an extra skip line in `just verify`, not silently dropped. No reproduction on the current stack. Resolve by declaring internal rows (listeners.conf column or compose config) and failing any other row with no endpoints."
    evidence_status: "none provided (code reading only)"
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
**Verified:** 2026-09-29T16:11:24Z
**Status:** human_needed
**Re-verification:** Yes, after gap-closure plan 01-09 (previous: gaps_found, 7/10)

## Summary verdict

All three gaps from the previous report are closed, and the verifier confirmed each one with its own live runs rather than from the 01-09 SUMMARY. No regression was found. The four roadmap success criteria still hold on the live stack, and `just verify` is green. The phase still needs two human checks that need hardware or a fresh machine: the Raspberry Pi 5 run and the fresh-machine setup guide. They were skipped in UAT and cannot be automated.

1. **CR-01 closed.** `tls_handshake_verified ADDR CAFILE ENTRY` now builds `-servername N -verify_hostname N` from a `DNS:` entry, or `-verify_ip A` from an `IP:` entry, and returns 1 with no handshake for anything else. Live, against the real served certificates with the public root, every declared name passes and every foreign name or IP is refused (table below). A chain-only mutant of the predicate, injected in a subshell, lets NC10's scenario pass with rc=0. So NC10 really does discriminate: it would go red if the identity check were removed.
2. **Real-predicate name controls.** NC9 (4 controls) and NC10 (2 controls) run on every guard run and are green. NC6 is kept unchanged alongside them.
3. **WR-01/WR-02 closed.** Running `env -i … bash scripts/verify-pki.sh` with the stack up now exits rc=1. It prints compose's `COMPOSE_PROJECT_NAME is missing a value` and exactly one `✗` line, with no `skipped (stack down)` and no `✓ verify-pki`. An unreachable Docker daemon also fails. A `--format` failure fails the row with `while reading its published ports`, both in `--live-only` and in full mode. A compose call that succeeds with no output still reads as "stack down" (NC13).

The three new warnings from the post-01-09 review do not make any must-have false. They are recorded as advisory (see the Advisory table).

## Goal Achievement

### Observable Truths

Roadmap success criteria (contract):

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC1 | One command brings up AXIAM, PostgreSQL and Caddy; `just demo-reset` wipes and rebuilds idempotently (PLAT-01, PLAT-02, PLAT-05) | ✓ VERIFIED | Verifier's `just verify` (rc=0): "✓ SC1 every stage marker present, every running container on its current image, demo card at 0600 with all five fields". All 7 `domo` services Up 4 h. UAT 1, 2 and 8 (two-cycle phase-verify) passed. The Pi half is human-only (see below). |
| SC2 | Single org root, BYOK import, one tenant signing CA per tenant, browsers trust Caddy's offline-signed SAN certs (PKI-01..06) | ✓ VERIFIED | `just verify-pki` rc=0: the root is PKCS#8 RSA-4096 with no pathlen; lakeside-ca and summit-ca chain to the root. **14 identity-checked live lines**: axiam-server 6, rabbitmq 5, caddy 3. The fingerprint round-trip is ✓. The secrets guard is ✓. UAT 10 (browsers) and UAT 13-16 passed. |
| SC3 | Single Caddy origin; resource tree and group-per-(role, resource) pattern exist and are queryable (PLAT-06, AUTHZ-01, AUTHZ-02) | ✓ VERIFIED | `just verify`: edge-verify, pg-verify and authz-verify are all ✓. |
| SC4 | A test device does mTLS login, gets a JWT, and CONNECTs to `domo` with cert + JWT, validated by a working HTTP auth backend (MQTT-01, MQTT-02) | ✓ VERIFIED | `just verify`: twin-test ✓. Live logs: device-twin shows 10 `CONNECT allowed` lines, and rabbitmq shows 10 `Accepted MQTT connection` lines. `just smoke` stays red only on DF-017/DF-025 (upstream AXIAM defects, by user decision). It was not re-run because it mutates AXIAM state, and its inputs are byte-identical to 9e873b0. |

Plan 01-08 must-haves (re-checked):

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `just verify` ends ✓, and SC2 has asserted every declared name of the rabbitmq 8883 row | ✓ VERIFIED (was FAILED) | `just verify` rc=0. `just verify-pki` prints 5 rabbitmq lines, each following a `-verify_hostname`/`-verify_ip` handshake. Real predicate: `DNS:not-a-san.invalid`, `DNS:definitely-not-a-san.invalid` and `IP:10.9.9.9` each rc=1. |
| 2 | Verdict independent of alert 116; 2×nproc hogs: 0 failures, 0 skips | ✓ VERIFIED | `just verify-pki-stress rabbitmq 50`: "50/50 under 24 CPU hogs (loadavg 22.47), 0 failures, 0 skips". |
| 3 | Plain HTTP, foreign chain, not running, no TLS, and a name that does not verify all fail, each asserted by negative controls | ✓ VERIFIED (was PARTIAL) | NC1 and NC2 run on the real predicates. NC9 runs on the real predicate, NC10 at the real call site, and NC4-NC6 are branch controls. All are ✓ in the stress run. |
| 4 | Stack up: a publishing row is never skipped; a run that checked nothing fails; `just verify` surfaces skip lines | ✓ VERIFIED (was PARTIAL) | Gate path: the only skips are device-twin and postgres. Direct `env -i` run: rc=1, no "stack down". A compose failure on ports fails the row. Residual edge cases are advisory (review WR-01, WR-03). |
| 5 | No retries; one handshake per name per endpoint; no client cert or key | ✓ VERIFIED | The helper bodies make one `openssl s_client` call each. No `-cert`/`-key`/`-pass` appears in the script. The only "retry" match is the comment "NOT a retry mechanism". |
| 6 | `deploy/rabbitmq/` byte-identical; `just smoke` untouched | ✓ VERIFIED | `git diff --quiet 9e873b0 -- deploy/ just/smoke.just just/verify.just tools/ services/ crates/ Cargo.toml Cargo.lock` exits 0. 01-09 touched only `scripts/verify-pki.sh` and `just/pki.just` (+106/−47). |

Plan 01-09 must-haves:

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Each `✓ <row> TLS 1.3 verified for <name>` follows an identity-checked handshake; 5 rabbitmq lines, 14 total; a wrong name or IP is refused | ✓ VERIFIED | Live: 14 lines (6+5+3). Wrong identities are refused on all three endpoints: 8883 (`DNS:axiam.domo.local` rc=1), 443 (`DNS:rabbitmq`, `IP:192.168.144.20` rc=1), and 8090 (`DNS:rabbitmq`, `IP:10.1.1.1` rc=1). |
| 2 | `tls_handshake_verified` takes the entry as a required third argument, refuses without an identity, and its comment says "the certificate covers NAME" | ✓ VERIFIED | `scripts/verify-pki.sh:147-161`: the `case` falls through `*) return 1` before any `openssl`. Live: `rabbitmq`, `DNS:`, `IP:` and `""` each give rc=1. The comment reads "the certificate covers NAME". |
| 3 | NC9 refuses a wrong name and IP and accepts `rabbitmq`/`127.0.0.1`; NC10 fails and never skips; NC1-NC8 unchanged | ✓ VERIFIED | Stress run: all ✓. NC3 accepts `DNS:rabbitmq` and NC9 accepts `IP:127.0.0.1`. `git diff 9e873b0 -- just/pki.just` removes only NC2/NC3's old `-servername rabbitmq` argument, which was replaced by `DNS:rabbitmq`, plus one echo line. Mutation check: with a chain-only predicate, NC10's scenario exits 0, so NC10 would go red. |
| 4 | The stress count means names: the idle precondition requires verified lines = declared SAN entries (5/5); 50/50 under load | ✓ VERIFIED | "✓ idle precondition 5/5 declared name(s) verified, each an identity check"; "50/50 … 0 failures, 0 skips". `declared` is computed through the script's own `expand_sans`. |
| 5 | A compose failure is never "stack down" or "no published port"; the `env -i` run exits non-zero with exactly one `✗`; NC11-NC13 | ✓ VERIFIED | Verifier's own `env -i PATH HOME DOMO_LAN_IP bash scripts/verify-pki.sh`: rc=1, compose's `COMPOSE_PROJECT_NAME is missing a value`, one `✗`, zero `skipped (stack down)` and zero `✓ verify-pki`. `DOCKER_HOST=unix:///nonexistent.sock`: rc=1 with `✗ axiam-server docker compose ps failed…`. The NC12 override fails in both `--live-only` and full mode. NC13 (empty success) gives rc=0 with `skipped (stack down)`. |
| 6 | Stack up: only device-twin and postgres skip; `just verify` shows exactly those two under SC2 | ✓ VERIFIED | `just verify` shows exactly those two under SC2; none names rabbitmq. The other skip line is secrets-guard's "nothing staged". |
| 7 | No retries, no client credential; the protected paths are byte-identical to 9e873b0; `just smoke` untouched | ✓ VERIFIED | See 01-08 #5 and #6. |

**Score:** 17/17 truths verified (0 present, behavior-unverified). The compose-failure truths depend on runtime behaviour. They are counted as VERIFIED because the verifier exercised the failure paths directly (the `env -i` run, the unreachable daemon, the NC12 override) and through the NC11-NC13 runs, not from the code alone.

### Prohibitions (01-09)

| Prohibition | Verifier disposition |
|---|---|
| No control weakened or removed; NC6 stays | Resolved. The diff removes no assertion. NC1-NC8 labels are present and ✓ in the stress run, and NC6 is unchanged. (Partly judgment; non-authoritative for "weakened".) |
| The script never guesses the compose project | Resolved. `COMPOSE_PROJECT_NAME` appears only in the header comment and the failure hint, with no default and no `.env` read. The `env -i` run fails. |
| `deploy/`, `just/smoke.just`, `just/verify.just`, `tools/`, `services/`, `crates/` unchanged; smoke stays red on DF-017/DF-025 | Resolved. `git diff --quiet 9e873b0 -- …` exits 0. |
| No retries; no client cert, key or passphrase | Resolved by grep of the script (non-authoritative judgment on "retry"). |

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `scripts/verify-pki.sh` | Identity-bearing predicate; `verify_live` passes the entry; compose failure fails; header documents `COMPOSE_PROJECT_NAME`; ≤500 lines | ✓ VERIFIED | 500 lines exactly. The only predicate call site passes `"$entry"` (line ~452). `service_running` does `ids=$(…) \|\| fail`. `published_endpoints` does `\|\| return 1` and its caller calls `fail`. |
| `just/pki.just` | Count cross-check; NC2/NC3 with an entry; NC9-NC13 | ✓ VERIFIED | All present, all sourced from the real script, all green live. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `verify_live` | `tls_handshake_verified` | `"$addr" "${PKI_DIR}/root.pem" "$entry"` | WIRED | The single call site. NC10 plus the mutation check prove the identity reaches OpenSSL. |
| `stack_is_up` → `service_running` | `docker compose ps -q` | exit status | WIRED | Compose errors call `fail` (exit), never "down". Never called inside a substitution. |
| `verify_live` → `published_endpoints` | `docker compose ps --format` | exit status | WIRED | The error fails the row (observed live via override). |
| `just verify` SC2 | `just verify-pki` → script | `group` | WIRED | Skip lines are visible in the live output. |
| `just verify-pki-stress` | script functions | source guard | WIRED | Real functions, not a copy. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Phase fast gate | `just verify` (run once; 32G free before and after; `target/debug/incremental` removed) | `✓ verify`; build, lint, tests, secrets, findings, operator-docs, topology, SC1-SC4 all ✓ | ✓ PASS |
| PKI suite with identity checks | `just verify-pki` | rc=0, 14 `TLS 1.3 verified for` lines, 2 skips | ✓ PASS |
| Predicate refuses wrong identities | sourced `tls_handshake_verified` on 8883/443/8090 with `dist/trust/domo-root.pem` | declared rc=0; foreign DNS/IP, bare name and empty entry rc=1 | ✓ PASS |
| Regression guard | `just verify-pki-stress rabbitmq 50` | 5/5 idle; NC1-NC13 ✓; 50/50 under 24 hogs, 0 failures, 0 skips | ✓ PASS |
| WR-01 repro | `env -i … bash scripts/verify-pki.sh` (stack up) | rc=1, one `✗`, no stack-down, no `✓ verify-pki` | ✓ PASS |
| Daemon unreachable | `DOCKER_HOST=unix:///nonexistent.sock` + `verify_live` | rc=1, `✗ axiam-server docker compose ps failed` | ✓ PASS |
| WR-02 path | NC12 override, `--live-only` and full mode | rc=1, `while reading its published ports` | ✓ PASS |
| NC10 discriminates | chain-only mutant predicate + NC10 SANs | rc=0 (so NC10 would go red on a regression) | ✓ PASS |

### Probe Execution

This phase declares no `scripts/*/tests/probe-*.sh`. Its runnable checks are the `just` recipes above, which the verifier ran itself.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| PLAT-01 | 01-01, 01-03, 01-07 | ✓ SATISFIED | SC1; UAT 1, 22 |
| PLAT-02 | 01-07 | ? NEEDS HUMAN | arm64 images verified (UAT 9); the Pi run was skipped (UAT 11) |
| PLAT-05 | 01-07 | ✓ SATISFIED | UAT 1, 2, 8 |
| PLAT-06 | 01-03, 01-07 | ✓ SATISFIED | edge-verify ✓ |
| PKI-01 | 01-01, 01-02, 01-08, 01-09 | ✓ SATISFIED | Root group ✓. Every live verdict is anchored in the root, and NC2 refuses a foreign CA. The executor's "Complete" mark is justified. |
| PKI-02 | 01-01, 01-04 | ✓ SATISFIED | Tenant CAs chain to the root; UAT 15 |
| PKI-03 | 01-01, 01-04, 01-06 | ✓ SATISFIED | UAT 16, UAT 4 |
| PKI-04 | 01-01, 01-02, 01-03, 01-08, 01-09 | ✓ SATISFIED (Phase 1 scope) | All 14 served identities are now proven by the gate itself; UAT 10 covers the browsers. The executor's "Complete" mark is justified for every listener that exists. The requirement text also names a Management Platform server certificate. That service arrives in Phase 2, so `listeners.conf` has no row for it yet. The table-driven issuance and verification will cover it once the row is added (info). |
| PKI-05 | 01-02, 01-07 | ✓ SATISFIED | Fingerprint round-trip ✓; UAT 27 |
| PKI-06 | 01-01, 01-02 | ✓ SATISFIED | secrets-guard ✓; UAT 20, 29 |
| AUTHZ-01 | 01-04, 01-06 | ✓ SATISFIED | authz-verify tree ✓ |
| AUTHZ-02 | 01-04, 01-06 | ✓ SATISFIED | authz-verify group pattern ✓; UAT 45-46 |
| MQTT-01 | 01-01, 01-05, 01-06 | ✓ SATISFIED | Live `Accepted MQTT connection`; UAT 18 |
| MQTT-02 | 01-01, 01-05, 01-06 | ✓ SATISFIED | twin-test ✓; live `CONNECT allowed`; UAT 19, 48 |

All 14 IDs are claimed by at least one plan, and there are no orphans. **Bookkeeping (warning):** `.planning/REQUIREMENTS.md` still shows PLAT-01/05/06 and PKI-05 as `Gaps Found`, and PKI-02/03/06, AUTHZ-01/02 and MQTT-01/02 as `Pending` with unchecked boxes. They should move to Complete at phase close. PLAT-02 should wait for the Pi run.

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | Review WR-01: `--live-only ROW` can print ✓ after zero identity checks if the row's SANs expand to empty | other | Fixture-only (needs an `expand_sans` override). No real row has empty SANs. The full gate refuses such a row in `verify_listeners`, and the stress guard refuses it in its precondition. No must-have is made false. Cheap hardening: count identities per row. |
| 2 | Review WR-02: NC12's expected text is shared with `service_running`'s message | other | The verifier confirmed that NC12 reaches the published-ports branch today. The risk only applies after a future refactor. Tighten the expected text to `while reading its published ports`. |
| 3 | Review WR-03: a successful compose answer with no publishers is still a green `no published port` skip | other | 01-09's contract covers compose failures, and that holds. An unexpected skip shows up in `just verify`. There is no reproduction on the stack. Hardening: declare the internal rows statically. |

None of the three is a gap. The first two cost only a few lines each and could be folded into Phase 6 hardening, or into a quick task now. Review WR-04 (hog lifetime) and the IN items remain informational.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| scripts/verify-pki.sh | ~442-466 | `checked` counts rows, not identities (review WR-01) | ⚠️ Warning (advisory) | Only in `--live-only` with an empty-SAN row, which does not exist |
| just/pki.just | ~180-182 | NC12 expected text is not branch-specific (review WR-02) | ⚠️ Warning (advisory) | Prospective vacuity |
| scripts/verify-pki.sh | ~411-417 | Runtime-inferred unpublished set (review WR-03) | ⚠️ Warning (advisory) | Visible extra skip, not silent |
| just/pki.just | ~186 | Hog lifetime fixed at 600 s (WR-04, carried) | ℹ️ Info | Fails loudly at high iteration counts |
| scripts/verify-pki.sh | 355-356 | Compose-failure hint always blames COMPOSE_PROJECT_NAME (IN-01) | ℹ️ Info | Misleading hint when the daemon is down; the verdict is correct |

There are no TBD/FIXME/XXX/TODO/HACK markers in `scripts/verify-pki.sh` or `just/pki.just`.

### Human Verification Required

1. **Raspberry Pi 5 one-command run (PLAT-02).** Run `just up` on the Pi. Expected: `✓ up`, the demo card, and a green `just verify`. Why human: needs the arm64 hardware (UAT 11 skipped).
2. **Fresh-machine setup guide.** Follow docs/setup.md on a clean machine. Expected: it reaches the demo card with no undocumented step. Why human: needs a fresh host (UAT 12 skipped).

(Browser trust in Chromium and Firefox was passed by a human in UAT 10. Plans 01-08 and 01-09 changed no certificate and no Caddy config, so it is not re-requested.)

### Gaps Summary

There are none. CR-01, WR-01 and WR-02 are closed, each with live evidence the verifier produced itself. The phase gate now proves what it prints. Each of the 14 live lines follows a hostname or IP check, and wrong identities are refused by the real predicate both at the predicate itself and at the call site. A compose error can no longer turn into a green "stack down" or "no published port". The remaining status is `human_needed` because of the two hardware/fresh-machine checks carried over from UAT. `just smoke` stays deliberately red on DF-017/DF-025, and it is not counted as a gap.

---

_Verified: 2026-09-29T16:11:24Z_
_Verifier: Claude (gsd-verifier)_
