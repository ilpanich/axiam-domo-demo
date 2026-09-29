---
phase: 01-foundation
reviewed: 2026-09-29T00:00:00Z
depth: standard
files_reviewed: 2
files_reviewed_list:
  - scripts/verify-pki.sh
  - just/pki.just
findings:
  critical: 0
  warning: 4
  info: 6
  total: 10
status: issues_found
---

# Phase 1: Code Review Report (plan 01-09, gap closure for CR-01, WR-01, WR-02)

**Reviewed:** 2026-09-29
**Depth:** standard
**Files Reviewed:** 2
**Status:** issues_found

## Summary

Scope: the 01-09 changes (diff base `53a2895`) to `scripts/verify-pki.sh` and `just/pki.just`. The earlier report reviewed plan 01-08. This one replaces it.

### Status of the prior findings

| Prior finding | Status | Evidence |
|---|---|---|
| CR-01: the live check never verified the name or IP | **Resolved** | `tls_handshake_verified` now requires a `DNS:`/`IP:` entry and builds `-servername N -verify_hostname N` or `-verify_ip A` from it. An entry with no identity is refused before any handshake. The only call site passes `"$entry"`. Tested live against rabbitmq 8883 with the public root: `DNS:rabbitmq` returned 0, `IP:127.0.0.1` returned 0, `DNS:not-a-san.invalid` returned 1, `IP:10.9.9.9` returned 1, bare `rabbitmq` returned 1, and `DNS:` returned 1. `just verify-pki` returns rc=0 with 14 `TLS 1.3 verified for` lines, two skips (device-twin, postgres) and nothing on stderr. One gap remains: a verdict can still pass with zero identities (WR-01 below). |
| WR-01: a compose error read as "stack down" | **Resolved** | `service_running` now fails the run when compose fails, and it is never called inside a substitution. The `env -i … bash scripts/verify-pki.sh` reproduction now exits rc=1. It shows compose's `COMPOSE_PROJECT_NAME is missing a value`, then exactly one `✗` line, and neither a skip nor `✓ verify-pki`. An unreachable daemon (`DOCKER_HOST=unix:///nonexistent.sock`) also fails instead of skipping. |
| WR-02: a compose error read as "no published port" | **Resolved for the error path; partly open** | `published_endpoints` returns 1 on a compose error, and its caller fails the row. The second half of the earlier fix was not done: the expected ports should come from static config, not from the runtime answer. A compose call that succeeds but reports no publishers is still a green skip (WR-03 below). |
| WR-03 (`just/verify.just` `group()` status) | Not re-reviewed | That file is outside this scope. |
| WR-04, IN-01 … IN-05 | Still open | Carried forward below. 01-09 did not target them. |

No new blocker was found. The four problems below show the gate, or its guard, can still report more than it proved.

Commands run (read-only; `df` showed 32G free on /home): `just verify-pki`, the `env -i` reproduction, single predicate calls against rabbitmq 8883, and three sourced `verify_live` runs with one function overridden (outputs quoted below).

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: `--live-only ROW` passes, printing `✓`, after checking zero identities when the row declares no SAN entries

**File:** `scripts/verify-pki.sh:442-459`, `scripts/verify-pki.sh:462-466`
**Issue:** `checked` counts rows, not verified identities. If the row's expanded SAN field is empty, `_declared` is empty and the per-entry loop runs zero times. Nothing calls `fail`, `checked` is still incremented, and `main` prints `✓ verify-pki --live-only ROW`. In full mode, `verify_listeners` (line 250) stops an empty server row first. `--live-only` skips `verify_listeners`, though, so there the verdict rests on no identity check at all. That is the CR-01 failure class again, now with zero names instead of the wrong ones. The section-4 header (lines 343-348) says "nothing checked at all" is a failure. Reproduced:

```
$ COMPOSE_PROJECT_NAME=domo DOMO_LAN_IP=192.168.144.20 bash -c 'source scripts/verify-pki.sh; set +e;
    LIVE_ONLY=rabbitmq; expand_sans() { echo ""; }; verify_live; echo "rc=$?"'
→ live listeners
rc=0
```

`verify-pki-stress` guards against this only by accident, through its `declared -eq 0` precondition. A direct `scripts/verify-pki.sh --live-only ROW` has no such guard.
**Fix:** Count identities, not rows, and fail a row that verified none:

```bash
        local ids_verified=0
        IFS=',' read -r -a _declared <<< "$sans"
        for entry in "${_declared[@]}"; do
            ...
            [ "$verified" -eq 1 ] || fail "..."
            ids_verified=$(( ids_verified + 1 ))
        done
        [ "$ids_verified" -gt 0 ] \
            || fail "${name}  declares no DNS or IP identity, so nothing was live-verified"
        checked=$(( checked + 1 ))
```

### WR-02: NC12 can pass without ever reaching the published-ports branch it claims to test

**File:** `just/pki.just:180-182`
**Issue:** NC12 expects the text `docker compose ps failed`. `service_running`'s failure message (`scripts/verify-pki.sh:356`) contains that same substring. NC12's mock fails only calls that contain `--format`, so it depends on `service_running` never using `--format`. Suppose a later refactor makes `service_running` use `ps --format '{{.ID}}'`, which is plausible. Then the mock fails `service_running` for `axiam-server` inside `stack_is_up`, the run exits there, and NC12 is still green. `published_endpoints` and its call site at line 412-413 would never be exercised. Reproduced by overriding `service_running` that way:

```
→ live listeners
sim
✗ axiam-server  docker compose ps failed, so whether
```

This output satisfies all three of `branch()`'s conditions (exit non-zero, text present, no skip). The negative control is therefore coupled to an implementation detail, and it would go vacuous without any warning.
**Fix:** Assert the specific branch and the row:

```bash
    branch "NC12 a compose failure while reading published ports is not \"no published port\" (WR-02)" \
        "✗ rabbitmq  docker compose ps failed while reading its published ports" \
        'docker() { case " $* " in *" --format "*) echo "simulated compose failure" >&2; return 1 ;; esac; command docker "$@"; }'
```

### WR-03: Residual of prior WR-02: a successful compose answer with no publishers is still a green skip for a row that publishes

**File:** `scripts/verify-pki.sh:411-417`
**Issue:** The error path is closed. The other half of the prior fix, taking the expectation from static config, was not done. "No published port" is still inferred only from the runtime answer. Three cases produce an empty `ps --format '{{range .Publishers}}…'` result with exit 0:

- a compose change that drops `ports:` from rabbitmq, caddy or axiam-server;
- a compose build whose template renders `.Publishers` as empty rather than failing;
- a container whose port bindings are not reported at that moment.

In each case the row is skipped as `no published port`. `checked` is still satisfied by the other rows, and `just verify-pki` ends `✓ verify-pki`. `just verify` shows the extra skip line but stays green. The plan's must-have ("only device-twin and postgres are skipped") is checked by hand in the SUMMARY, and nothing in the script enforces it.
**Fix:** Make the unpublished set explicit and fail any other row that has no endpoints. For example, add a fourth column to `deploy/pki/listeners.conf` (`published`/`internal`). Or compare with `docker compose -f "$COMPOSE_FILE" config --format json` (`.services[$name].ports | length`):

```bash
        if [ -z "$endpoints" ]; then
            row_is_internal "$name" \
                || fail "${name}  declares published ports but compose reports none; refusing to skip"
            skip "${name}: no published port"
            continue
        fi
```

### WR-04 (carried over, prior WR-04): the CPU hogs' lifetime is still a fixed 600 s

**File:** `just/pki.just:46`, `just/pki.just:186-188`, `just/pki.just:203-209`
**Issue:** Unchanged by 01-09. `iterations` may be as high as 200, each iteration may take up to `timeout 60`, and the hogs die after `timeout 600`. On the Pi 5 at the advertised upper bound, the run can fail with "the CPU load ended before the last iteration" even with zero probe failures. It fails loudly rather than falsely green, but it makes the guard unusable at its own limit. 01-09 also added about 12 controls ahead of the load. They run before the hogs start, so they do not shorten the window.
**Fix:** Set `hog_secs=$(( iterations * 60 + 120 ))`, then run `timeout "$hog_secs" sh -c 'while :; do :; done' &`.

## Info

### IN-01: The compose-failure hint always blames `COMPOSE_PROJECT_NAME`, and "daemon stopped" has changed from skip to fail without a documentation update

**File:** `scripts/verify-pki.sh:355-356`, `just/pki.just:24-27`
**Issue:** With the daemon unreachable, the run prints compose's `failed to connect to the docker API…` and then `✗ axiam-server … (export COMPOSE_PROJECT_NAME, or run 'just verify-pki')`. That hint is wrong for this cause. Failing is the right behaviour, because an unknown state is not a down stack. But the `verify-pki` recipe comment still says the live section is "skipped only when the stack is down", and an operator will read "Docker is not running" as down.
**Fix:** Make the hint neutral ("see compose's error above; a direct run needs COMPOSE_PROJECT_NAME and a reachable daemon"). Add a sentence to the recipe comment saying that an unreachable daemon or a compose error fails the gate.

### IN-02: `_declared` is a global array shared by `verify_leaf` and `verify_live`

**File:** `scripts/verify-pki.sh:254`, `scripts/verify-pki.sh:443`
**Issue:** Neither function declares `_declared` local, so each run overwrites the other's array. Nothing is wrong today, because each function refills it with `read -a` before use. A future caller that reads it after a nested call would see stale entries.
**Fix:** Add `local -a _declared` in both functions.

### IN-03 (carried over, prior IN-01): sourcing the script still leaks `set -euo pipefail` and `cd` into the caller

**File:** `scripts/verify-pki.sh:14-15`, `scripts/verify-pki.sh:26-29`
**Issue:** Unchanged. The header still advertises `source scripts/verify-pki.sh` without saying "only inside a subshell". Every new 01-09 control (the `declared` count, NC13, `branch`) correctly uses a subshell.
**Fix:** Document the subshell requirement, or move `set -euo pipefail` and `cd` into `main`.

### IN-04 (carried over, prior IN-02 and IN-03): the controls are tied to rabbitmq, and NC7 checks only the exit status

**File:** `just/pki.just:90-91`, `just/pki.just:109-157`
**Issue:** Unchanged. `just verify-pki-stress caddy` still runs NC1-NC13 against rabbitmq. `tls_ep`/`http_ep` are not normalised the way `published_endpoints` normalises endpoints. The NC7 `control` calls pass on any non-zero exit (a full disk, stack down, missing openssl), not only on the specific refusal. The new NC9 (4 controls) and NC10 (2) add more rabbitmq-specific cases.
**Fix:** Refuse `row != rabbitmq`, or derive the endpoints through the sourced `published_endpoints`. Run NC7 through a message-checking helper that expects `no server row of that name`.

### IN-05 (carried over, prior IN-04): the failure message names a bogus SNI for a row with no DNS SAN

**File:** `scripts/verify-pki.sh:427-437`
**Issue:** Unchanged. `first_dns` is set from `${sans#*DNS:}` even when `sans` has no `DNS:`. The message then shows `probed with SNI IP:…` although no SNI was sent.
**Fix:** Assign `first_dns` only inside the `*DNS:*` case arm.

### IN-06 (carried over, prior IN-05): the device-twin and postgres TLS listeners are never live-asserted

**File:** `scripts/verify-pki.sh:414-417`
**Issue:** Unchanged. Two of the five server listeners are never handshaken; the static SAN check covers only the files on disk. WR-03 makes this more important: the unpublished exemption is inferred at runtime rather than declared.
**Fix:** Optional. Probe them from inside the compose network (a `domo-probe` one-off running `openssl s_client`), or record the exemption explicitly in `listeners.conf` (see WR-03).

---

_Reviewed: 2026-09-29_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
