---
phase: 01-foundation
plan: 09
subsystem: pki-verification
status: complete
tags: [pki, tls, verification-gate, gap-closure, CR-01, WR-01, WR-02, rabbitmq, regression-guard]

requires:
  - "01-08: tls_answers / tls_handshake_verified marker verdicts, --live-only, the source guard, just verify-pki-stress (NC1-NC8)"
provides:
  - "tls_handshake_verified ADDR CAFILE ENTRY: every verdict is an identity check (-servername NAME -verify_hostname NAME, or -verify_ip ADDR); no identity, no verdict"
  - "service_running / published_endpoints: a compose error fails the run instead of reading as \"stack down\" or \"no published port\""
  - "verify-pki-stress: idle-precondition count cross-checked against the row's declared SAN entries; NC9-NC13"
affects: [verify, phase-verify, verify-pki, phase-06]

gap_ids: [CR-01, WR-01, WR-02]
requirements-completed: [PKI-01, PKI-04]

actuals:
  tokens: 3900
  tasks: 2
  commits: 4
  plan_head_before: d398d1da6de238d7096b3663a227bd34ceeb3c2b
  plan_head_after: 7cc98d8decef0e8a729e391bd9d37a0cb3d86c14

tech-stack:
  added: []
  patterns:
    - "A TLS verdict takes the identity it vouches for as a required argument; the helper builds the name/IP check itself rather than trusting callers to pass flags"
    - "A tool's exit status separates \"it failed\" from \"it found nothing\"; stderr is let through so the reason is printed before the ✗ line"
    - "A function that may call fail() is never run inside a command substitution or pipeline, so exit ends the script, not a subshell"
    - "Every new negative control was committed RED against the unfixed code before the fix"

key-files:
  created: []
  modified:
    - scripts/verify-pki.sh
    - just/pki.just

key-decisions:
  - "CR-01 fixed in the predicate's signature (ENTRY is required), not at the call site alone, so no future caller can obtain a chain-only verdict"
  - "A compose failure ends verify-pki at its first occurrence (fail → exit 1); a three-valued up/down/failed result was rejected as more lines for the same single ✗"
  - "The script never defaults, derives or reads COMPOSE_PROJECT_NAME from .env; the header documents it as required and a direct run without it fails loudly"

coverage:
  - deliverable: "Every 'TLS 1.3 verified for <name>' line is an OpenSSL identity check for that name or IP (CR-01)"
    human_judgment: false
    verification:
      - kind: command
        ref: "just verify-pki-stress rabbitmq 50 (NC9 x4, NC10 x2, idle 5/5)"
        status: pass
      - kind: command
        ref: "just verify-pki (5 rabbitmq / 14 total identity lines)"
        status: pass
  - deliverable: "A compose failure fails verify-pki; a stopped stack still skips (WR-01, WR-02)"
    human_judgment: false
    verification:
      - kind: command
        ref: "just verify-pki-stress rabbitmq 50 (NC11 branch + direct, NC12, NC13)"
        status: pass
      - kind: command
        ref: "env -i PATH HOME DOMO_LAN_IP bash scripts/verify-pki.sh (rc=1, one ✗ line)"
        status: pass
  - deliverable: "The fast gate stays green with only the two legitimate skips"
    human_judgment: false
    verification:
      - kind: command
        ref: "just verify"
        status: pass

duration: 10min
completed: 2026-09-29
---

# Phase 1 Plan 09: verify-pki identity checks and compose-failure semantics Summary

**`tls_handshake_verified` now requires the SAN entry and passes `-servername NAME -verify_hostname NAME` or `-verify_ip ADDR`, so every live "verified for <name>" line in the phase gate is a real identity check. A `docker compose ps` error now fails verify-pki, naming the service, where it used to read as "stack down" or "no published port". Both fixes are guarded by the new real-predicate controls NC9 to NC13, and each of those controls was committed red first.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-09-29T15:46:33Z
- **Completed:** 2026-09-29T15:56:21Z
- **Tasks:** 2 (each done as a RED commit followed by a GREEN commit)
- **Files modified:** 2

## Accomplishments

- **CR-01 closed.** The predicate signature is now `tls_handshake_verified ADDR CAFILE ENTRY`. `DNS:NAME` becomes `-servername NAME -verify_hostname NAME`, and `IP:ADDR` becomes `-verify_ip ADDR`. Anything else returns 1 before any handshake. `verify_live` passes `"$entry"`, and the ok and fail message texts are unchanged byte for byte.
- **The name-mismatch branch is now asserted by the real code.** NC9 runs the real predicate on rabbitmq 8883: it refuses `DNS:not-a-san.invalid` and `IP:10.9.9.9`, accepts `IP:127.0.0.1`, and refuses an entry with no identity. NC10 runs the real call site and fails a row that declares a name or an IP its certificate does not carry. NC6 (the mock) is kept alongside them.
- **The guard's count now means names.** The idle precondition cross-checks the verified count against the row's declared SAN entries, read through the real `expand_sans`.
- **WR-01 and WR-02 closed.** `service_running` fails on a non-zero compose exit and lets compose's stderr through. `published_endpoints` returns 1 on a compose error, and `verify_live` fails that row. The header now documents `COMPOSE_PROJECT_NAME`.

## Task Commits

1. **Task 1 RED:** `36076b5`, `test(01-09): add NC10, the call-site name-mismatch control, red on the SNI-only verdict (CR-01)`
2. **Task 1 GREEN:** `53fe8bb`, `fix(01-09): verify-pki checks every declared name and IP, not just the chain (CR-01)`
3. **Task 2 RED:** `d3b1cf3`, `test(01-09): add NC11-NC13, compose-failure controls, red on the swallowing helpers (WR-01, WR-02)`
4. **Task 2 GREEN:** `7cc98d8`, `fix(01-09): a compose failure fails verify-pki instead of reading as stack down or no published port (WR-01, WR-02)`

## RED evidence (verbatim)

**Task 1, NC10 on the SNI-only code** (`just verify-pki-stress rabbitmq 1`):

```
        ✓ rabbitmq  TLS 1.3 verified for not-a-san.invalid on 192.168.144.20:8883
✗ verify-pki-stress  negative control failed: NC10 verify_live fails a row naming a DNS name its served certificate does not carry (exit 0)
```

**Task 2, NC11 on the swallowing helpers** (`just verify-pki-stress rabbitmq 1`). NC13 passed first, as the plan expected:

```
  ✓ NC13 a compose answer that succeeds with no containers is still "stack down"
      → live listeners
      ✗ stack down: --live-only rabbitmq asserts a live listener — run 'just up' first
✗ verify-pki-stress  negative control failed: NC11 a compose failure is not "stack down" (exit 1)
```

Also red outside the recipe, before the fix:
- The direct `env -i` run ended `→ skipped (stack down)` / `✓ verify-pki` with `rc=0` (WR-01).
- The NC12 override produced `→ skipped (rabbitmq: no published port)` (WR-02).

## GREEN evidence

`just verify-pki-stress rabbitmq 50`, run after each fix and again after the final label change:

```
  ✓ idle precondition  5/5 declared name(s) verified, each an identity check
  ✓ NC1 … NC8 (unchanged assertions), NC9 x4, NC10 x2, NC13, NC11 (branch), NC11 (direct, WR-01), NC12 (WR-02)
✓ verify-pki-stress  rabbitmq: 50/50 under 24 CPU hogs (loadavg 24.29), 0 failures, 0 skips
```

`just verify-pki`: exit 0, ends `✓ verify-pki`. It printed 5 `✓ rabbitmq  TLS 1.3 verified for` lines and 14 `TLS 1.3 verified for` lines in total (axiam-server 6, rabbitmq 5, caddy 3). It printed exactly two skip lines:

```
  → skipped (device-twin: no published port)
  → skipped (postgres: no published port)
```

Direct `env -i PATH HOME DOMO_LAN_IP bash scripts/verify-pki.sh` with the stack up, after the fix:

```
→ live listeners
error while interpolating name: required variable COMPOSE_PROJECT_NAME is missing a value: operator must set COMPOSE_PROJECT_NAME - see .env.example
✗ axiam-server  docker compose ps failed, so whether axiam-server is running is unknown; a compose error is not a stopped stack (export COMPOSE_PROJECT_NAME, or run 'just verify-pki')
rc=1
```

The plan's `bash -c` assertion chain passed (exit 0): exactly one `✗` line, no `skipped (stack down)`, and no `✓ verify-pki`.

`just verify` (exit 0), the SC2 group and the final line:

```
  ✓ SC2  verify-pki   the whole chain, every listener, live TLS per row
      → skipped (device-twin: no published port)
      → skipped (postgres: no published port)
  ✓ SC2  the exported fingerprint round-trips to the in-use root
…
✓ verify
```

`✓ verify` is the gate's verdict line. The recipe's existing, unchanged footer (the `just smoke-gate` pointer) prints two informational lines after it. There is no `skipped (rabbitmq`.

## Disk and housekeeping

- **`df -h /home /` before `just verify`:** /home 157G, 117G used, 32G free (79%); / 196G, 147G used, 40G free (79%)
- **`df -h /home /` after `rm -rf target/debug/incremental`:** /home 32G free; / 40G free, both unchanged at `-h` resolution
- `target/debug/incremental` is removed (`test -e` returns 1).
- `pgrep -fc 'while :; do :; done'` returns `1`, which is only the self-match.
- `git diff --quiet 9e873b0 -- deploy/ just/smoke.just just/verify.just tools/ services/ crates/ Cargo.toml Cargo.lock` exits 0.
- `just smoke` and `just smoke-gate` were not run or touched. The smoke gate stays deliberately red on DF-017 and DF-025.
- **`wc -l`:** `scripts/verify-pki.sh` is 500 (cap 500), `just/pki.just` is 317.

## Gap coverage

| Gap | What now asserts it |
|---|---|
| VERIFICATION gap 1 / **CR-01**: chain-only verdict per name | `tls_handshake_verified` ENTRY argument, run on every `just verify` (SC2 → `just verify-pki`, 14 identity lines); NC9 (the real predicate refuses a wrong DNS name and IP); NC10 (the real call site); acceptance check `tls_handshake_verified … rabbitmq` → `rc=1` |
| VERIFICATION gap 2: name mismatch only covered by NC6's mock | NC9 and NC10 in `just verify-pki-stress`; the idle precondition's `5/5 declared` cross-check |
| VERIFICATION gap 3 / **WR-01**: compose error read as "stack down" | NC11 (branch) and NC11 (direct `env -i` reproduction) in `just verify-pki-stress`; Task 2's second `<automated>` command |
| **WR-02**: compose error read as "no published port" | NC12 in `just verify-pki-stress` |
| Over-tightening guard: a stopped stack must still skip | NC13 |

## Review findings left open (not verification gaps)

WR-03, WR-04, IN-01, IN-02, IN-03, IN-04, IN-05 (see `01-REVIEW.md`). This plan does not address them.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The NC11–NC13 labels did not match the plan's acceptance grep**
- **Found during:** Task 2 acceptance check
- **Issue:** The labels contain literal double quotes (`"stack down"`), so the first version wrapped them in single quotes. The acceptance check `grep -cE '"NC1[123] '` then counted 0.
- **Fix:** Re-quoted the four labels as double-quoted strings with the inner quotes escaped. The printed labels are unchanged. The stress run was repeated after the change, and all controls passed.
- **Files modified:** just/pki.just
- **Commit:** 7cc98d8

### Notes

- `scripts/verify-pki.sh` ends at exactly 500 lines, the cap. Two edits made room, as step 7 of the plan directs:
  - The header's opening description paragraph was condensed, with every fact kept.
  - The new section-4 sentence was fitted onto two lines.
- The idle-precondition's `declared` count is computed inside a subshell that sources the script and then runs `set +e`. Without `set +e`, a `grep -c .` over 0 entries would stop that subshell before it reported the count. A count of 0, or an unknown row, fails the precondition.

**Total deviations:** 1 auto-fixed (Rule 1). **Impact:** cosmetic quoting only. No assertion changed.

## Issues Encountered

None.

## Next Phase Readiness

The three gaps (CR-01, WR-01, WR-02) are closed, and `just verify` is green on the live stack. Phase 1 is ready for re-verification (`/gsd-verify-work 01`). The Pi 5 and fresh-machine human checks are still open.

## Self-Check: PASSED

- FOUND: scripts/verify-pki.sh (500 lines), just/pki.just (317 lines)
- FOUND commits: 36076b5, 53fe8bb, d3b1cf3, 7cc98d8 (in this order in `git log`, each `test` before its `fix`)
- Task 1 and Task 2 acceptance criteria all PASS: A1–A12 and B1–B13, including the requote fix for B8.
