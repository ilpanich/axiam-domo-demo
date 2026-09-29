---
phase: 01-foundation
plan: 08
subsystem: pki-verification
status: complete
tags: [pki, tls, verification-gate, gap-closure, G-01-3, rabbitmq, regression-guard]

requires:
  - "01-02: scripts/verify-pki.sh, just verify-pki, the listener table"
  - "01-07: just verify and its group() runner, the RabbitMQ 8883 mTLS listener in the live stack"
provides:
  - "tls_answers / tls_handshake_verified: TLS verdicts from s_client -state markers, never from its exit status"
  - "scripts/verify-pki.sh --live-only ROW, plus a source guard so the real functions can be loaded"
  - "just verify-pki-stress ROW ITERATIONS: the G-01-3 regression guard (NC1-NC8, then one row under 2 x nproc CPU hogs)"
  - "Strict live semantics: while the stack is up, a published server row that cannot be asserted fails"
  - "just verify surfaces every `→ skipped` line from a passing group"
affects: [phase-verify, verify, verify-pki, phase-06]

gap_ids: [G-01-3]
requirements-completed: [PKI-01, PKI-04]

actuals:
  tokens: 5000
  tasks: 3
  commits: 4
  plan_head_before: 6ab1b8109ab0fa4adb15ef12508e520d0e696088
  plan_head_after: 9e873b0a027f72ee025d364681748121180da439

tech-stack:
  added: []
  patterns:
    - "Judge a TLS handshake by what only a completed, verified handshake prints (write finished + New, TLSv1.3 + Verify return code: 0), never by the client's exit status"
    - "A regression guard loads the gate's own functions through a source guard; it never tests a copy of the predicate"
    - "Every guard run proves the predicate can still say no (negative controls) before it is allowed to say yes under load"
    - "A skipped assertion is surfaced, never folded into a pass"

key-files:
  created: []
  modified:
    - scripts/verify-pki.sh
    - just/pki.just
    - just/verify.just

key-decisions:
  - "G-01-3 fixed by construction (three handshake markers, exit status discarded), not by retries; each declared name gets exactly one handshake"
  - "The broker posture (deploy/rabbitmq/, verify_peer + fail_if_no_peer_cert + TLS 1.3 only) is unchanged; the probe presents no client certificate"
  - "published_endpoints uses compose's Go template instead of jq; the optional-tool skip path is gone"
  - "stack_is_up is keyed on the listener services, so a lone tools-profile one-off reads as stack down"
  - "verify-pki-stress stays out of just verify and just phase-verify: it saturates every core"

metrics:
  duration: 11min
  completed: 2026-09-29
---

# Phase 1 Plan 08: verify-pki handshake-marker verdict and strict live semantics Summary

verify-pki now decides every live TLS check from s_client `-state` and summary markers instead of its exit status. This removes the RabbitMQ 8883 scheduling race (the TLS 1.3 `certificate_required` alert that arrives after the handshake). Published rows can no longer be skipped silently while the stack is up, and `just verify` shows every skip. A regression guard, `just verify-pki-stress`, reproduced the defect on the old code and passes on the new code.

## What was built

- **Verdict helpers** (`scripts/verify-pki.sh`). `tls_answers` returns true only if the whole line `SSL_connect:SSLv3/TLS read server certificate` appears. `tls_handshake_verified` returns true only if all three of these appear: `SSL_connect:SSLv3/TLS write finished`, `^New, TLSv1\.3, Cipher is `, and `Verify return code: 0 (ok)`. Each helper makes one handshake under a 20 s `timeout` hang guard. Neither retries or presents a client credential. `verify_live` no longer calls `s_client` directly.
- **`--live-only ROW` and the source guard.** `main()` now runs only when the file is executed, not when it is sourced. `LIVE_ONLY` is assigned unconditionally, so a value inherited from the environment is ignored. In live-only mode each of these fails: stack down, an unknown row, or a row that was not verified.
- **Strict live semantics.** The check no longer uses jq, because `published_endpoints` now uses compose's Go template. `service_running` and a listener-keyed `stack_is_up` were added. While the stack is up, each of these is a failure: a service that is not running, no port that answers TLS, a name that does not verify, or nothing verified at all. Only two skips remain: the stack is down, or a running service publishes no port.
- **`just verify-pki-stress`** (`just/pki.just`). It validates its inputs, then does an idle run as a precondition, then runs the negative controls NC1-NC8 against the real functions. Next it starts 2 × nproc `timeout 600` CPU hogs and repeats the row N times without retrying. It fails if the load ended early. A trap stops and reaps the hogs and removes the throwaway CA's temp directory.
- **`just verify` `group()`.** It now prints a passing recipe's `→ skipped` lines, indented, under that recipe's ✓ line.

## Evidence

**RED on the old exit-status verdict**, Task 1 Step A (commit d29e098), verbatim:

```
✗ verify-pki-stress  rabbitmq: 0/20 under 24 CPU hogs (loadavg 10.49), 20 failures, 18 skips
```

**GREEN guard lines:**

```
✓ verify-pki-stress  rabbitmq: 50/50 under 24 CPU hogs (loadavg 20.70), 0 failures, 0 skips   (Task 1, NC1-NC3)
✓ verify-pki-stress  rabbitmq: 50/50 under 24 CPU hogs (loadavg 23.46), 0 failures, 0 skips   (Task 2, NC1-NC8)
✓ verify-pki-stress  rabbitmq: 50/50 under 24 CPU hogs (loadavg 23.67), 0 failures, 0 skips   (final code)
```

In each run every control printed ✓: NC1 (plain HTTP on 127.0.0.1:15673 refused), NC2 (throwaway CA refused on 192.168.144.20:8883), NC3 (organization root accepted), NC4 (listener service not running), NC5 (no TLS answer), NC6 (name does not verify), NC7 (unknown row and client row refused by `--live-only`), and both NC8 checks (`stack_is_up` is true on the live stack and false when no listener service runs). `pgrep -fc 'while :; do :; done'` returned 1, which is only the self-match, after every run.

**`just verify-pki`:** exits 0 with `✓ verify-pki`. It prints 5 `✓ rabbitmq  TLS 1.3 verified for` lines and exactly 2 skip lines, `device-twin: no published port` and `postgres: no published port`. No skip line names rabbitmq.

**`just verify` final lines (live stack):**

```
  ✓ SC2  verify-pki   the whole chain, every listener, live TLS per row
      → skipped (device-twin: no published port)
      → skipped (postgres: no published port)
...
✓ verify
```

`grep -c 'skipped (rabbitmq'` on the log: 0.

**smoke-gate marker line** (exit 1, expected by user decision, untouched):

```
✗ smoke-gate  known-red: exactly the two attributed failures, nothing else
```

**Disk:**

| When | /home | / |
|---|---|---|
| before (start of plan, and before `just verify`) | 157G, 118G used, 31G free (80%) | 196G, 147G used, 40G free (79%) |
| after `rm -rf target/debug/incremental` | 157G, 117G used, 32G free (79%) | 196G, 147G used, 40G free (79%) |

`target/debug/incremental` no longer exists.

**Prohibitions:** `git diff --quiet 8a6c4ba -- deploy/rabbitmq/ just/smoke.just tools/domo-probe/ services/` exits 0. The region-scoped greps return 0 loops and 0 `-cert/-key/-pass` in the two helper bodies. Line counts: `scripts/verify-pki.sh` 497 and `just/verify.just` 493, both 500 or fewer.

## Coverage

| Gap | Truth | Asserted by |
|---|---|---|
| G-01-3 | `just verify` is green on the live stack, and SC2 verify-pki asserts every declared name of the rabbitmq 8883 row on every run, independent of CPU load | `just verify-pki-stress rabbitmq 50` (regression guard, NC1-NC8 + 50 loaded iterations) and `just verify` (SC2 group, skips surfaced) |

## Task commits

| Task | Commit | Message |
|---|---|---|
| 1 (RED) | d29e098 | test(01-08): add the G-01-3 regression guard, red on the exit-status verdict |
| 1 (GREEN) | 8e12d3e | fix(01-08): judge verify-pki handshakes by markers, not s_client's exit status (G-01-3) |
| 2 | 0f765bc | fix(01-08): never skip a published server row while the stack is up (G-01-3) |
| 3 | 9e873b0 | fix(01-08): surface skip lines from passing groups in just verify (G-01-3) |

## TDD Gate Compliance

For Task 1, the RED commit (`test(01-08)`, d29e098) comes before the GREEN commit (`fix(01-08)`, 8e12d3e). The guard failed on the unmodified verdict (20/20 failures) before the verdict changed. Tasks 2 and 3 extended the same guard (NC4-NC8) and the `just verify` grep check in the same commit as their implementation. Their behaviour could only be exercised against the new code paths.

## Deviations from Plan

**1. [Rule 3 - Blocking] Newline separator in the compose Go template**
- **Found during:** Task 2
- **Issue:** `docker compose ps --format '…{{"\n"}}…'` fails with `template parsing error: unterminated quoted string`.
- **Fix:** The template emits `URL:PublishedPort ` separated by spaces, and `tr ' ' '\n'` splits it into lines. The output contract (one `host:port` per line, sorted, unique, wildcard and empty hosts mapped to 127.0.0.1, blank lines dropped) is unchanged.
- **Files modified:** scripts/verify-pki.sh
- **Commit:** 0f765bc

**2. [Cosmetic] `just --list` shows the last line of a recipe's doc comment**
- The `verify-pki` and `verify-pki-stress` doc comments are ordered so that their last line is a meaningful one-line summary. The content matches what the plan specifies.

**Observation, not a change:** now that `group()` surfaces skips, `just verify` also shows `→ skipped (nothing staged)` from a passing repository-group recipe. It is a legitimate skip and was hidden before. No action was taken.

## Known Stubs

None.

## Threat Flags

None. The only new surface is the stress recipe (T-08-05). Its hogs are bounded by `timeout 600`, stopped by the trap, and confirmed gone by `pgrep`. The throwaway CA lives in `mktemp -d` and is removed by the trap (T-08-04).

## Self-Check: PASSED
