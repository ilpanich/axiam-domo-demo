---
phase: 01-foundation
reviewed: 2026-09-29T00:00:00Z
depth: standard
files_reviewed: 3
files_reviewed_list:
  - just/pki.just
  - just/verify.just
  - scripts/verify-pki.sh
findings:
  critical: 1
  warning: 4
  info: 5
  total: 10
status: issues_found
---

# Phase 1: Code Review Report (plan 01-08, gap G-01-3)

**Reviewed:** 2026-09-29
**Depth:** standard
**Files Reviewed:** 3
**Status:** issues_found

## Summary

Scope: the 01-08 changes (diff base `78745c1`) that close G-01-3. These are the marker-based TLS verdicts (`tls_answers`, `tls_handshake_verified`), `--live-only` and the source guard in `scripts/verify-pki.sh`, the `verify-pki-stress` regression guard in `just/pki.just`, and skip-surfacing in `just/verify.just`'s `group()`.

The race fix itself holds up. Judging by `-state` markers and not by `s_client`'s exit status is sound. The three-marker conjunction rejects a refused chain: NC2, and a throwaway-CA probe against caddy, both returned 1. With `-CAfile` given, OpenSSL 3.6.4's `s_client` did not fall back to the system trust store; a throwaway CAfile against a public site failed.

The live suite still reports assurance it never produces, which is the failure class G-01-3 was meant to remove. Every `✓ <row>  TLS 1.3 verified for <name>` line is printed without the name ever being checked against the certificate the listener serves. This was proven live: a nonsense SNI "verifies" against axiam-server. That makes the plan's first must-have truth false. A second fail-open path turns a compose error into `skipped (stack down)` plus `✓ verify-pki` while the stack is running; this was also reproduced live.

Commands run (read-only; `df` showed 32G free on /home): `just verify-pki`, `scripts/verify-pki.sh` without an environment, one idle `--live-only rabbitmq` run (0.35 s), and single `openssl s_client` probes against the running stack.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: The live check never verifies the declared name or IP, so "TLS 1.3 verified for <name>" is false assurance

**File:** `scripts/verify-pki.sh:149-157`, `scripts/verify-pki.sh:435-455`
**Issue:** `tls_handshake_verified` passes `-CAfile … -verify_return_error -tls1_3` and, for DNS entries, `-servername <name>`. `-servername` only sets SNI. `openssl s_client` does not check the hostname unless `-verify_hostname` is given, or the IP unless `-verify_ip` is given. The per-name loop therefore repeats the same chain-only handshake once per SAN entry and prints a per-name ✓ each time.

For IP entries it is worse. The probe connects to the published address (for example `127.0.0.1:8090`) with no SNI and no IP check, then reports `verified for 192.168.144.20 on 127.0.0.1:8090`.

Reproduced against the running stack:

```
tls_handshake_verified 127.0.0.1:8090 "$PKI_DIR/root.pem" -servername definitely-not-a-san.example   -> rc=0
tls_handshake_verified 127.0.0.1:443  "$PKI_DIR/root.pem" -servername localhost -verify_hostname definitely-not-a-san.example -> rc=1
tls_handshake_verified 127.0.0.1:8090 "$PKI_DIR/root.pem" -verify_ip 10.9.9.9                          -> rc=1
```

Consequence: a listener serving any leaf that chains to the root passes every name of its row. Examples are rabbitmq mounting `caddy.pem`, or a stale leaf issued before a SAN was added. The static SAN check (lines 236-257) only inspects the file under `.secrets/pki/`, not what the container actually serves. So nothing asserts that the served certificate covers the names. Plan 01-08's must-have truth ("asserted every declared name of the rabbitmq 8883 row") does not hold. The same applies to the `verify-pki-stress` success marker it counts: `want`/`got` count repetitions of the chain check, not names. None of NC1-NC8 exercises a name mismatch, so the guard cannot catch this. The gap predates 01-08, but 01-08 rewrote this predicate and its call site and kept it.
**Fix:** Make the expected identity part of the verdict, and add a negative control for it:

```bash
# verify_live, inside the per-entry loop
case "$entry" in
    IP:*) sni=(-verify_ip "$host_name") ;;
    *)    sni=(-servername "$host_name" -verify_hostname "$host_name") ;;
esac
```

```bash
# just/pki.just, alongside NC2/NC3
control "NC9 tls_handshake_verified refuses a name the certificate does not carry on ${tls_ep}" nonzero \
    tls_handshake_verified "$tls_ep" .secrets/pki/root.pem -servername rabbitmq -verify_hostname not-a-san.invalid
```

With `-verify_return_error`, a name mismatch aborts before `write finished`, so the existing marker conjunction rejects it; `caddy -verify_hostname bogus` returned 1 above. Update the comment at lines 146-148 to say "and the certificate covers NAME".

## Warnings

### WR-01: Stack detection fails open on a compose error: a live stack is reported as "skipped (stack down)" and ✓ verify-pki

**File:** `scripts/verify-pki.sh:344-346`, `scripts/verify-pki.sh:351-360`, `scripts/verify-pki.sh:377-381`
**Issue:** `service_running` discards compose's stderr and treats any failure as "not running". `deploy/compose.yml` opens with `name: ${COMPOSE_PROJECT_NAME:?operator …}`, and `verify-pki.sh` never loads `.env` or `.secrets/generated.env` itself. It relies on its caller having exported `COMPOSE_PROJECT_NAME`: `just verify-pki` does, and compose then falls back to name-only mode. The script's own documented usage (`scripts/verify-pki.sh`, header line 15) does not.

Reproduced with the stack up: `env -i PATH=… HOME=… DOMO_LAN_IP=192.168.144.20 scripts/verify-pki.sh` ends with `→ skipped (stack down)` and `✓ verify-pki`. Meanwhile `docker compose ps` prints `required variable COMPOSE_PROJECT_NAME is missing a value` to /dev/null. So a compose error, a Docker permission error or a daemon hiccup all become a green "stack down". Only `just verify`'s own separate `_dc ps -q` precheck masks this, and only inside that gate.
**Fix:** Tell "compose failed" apart from "nothing running", and fail loudly on the former:

```bash
service_running() {
    local out
    out="$(docker compose -f "$COMPOSE_FILE" ps -q "$1" 2>&1)" \
        || fail "docker compose ps ${1} failed: ${out} (is COMPOSE_PROJECT_NAME exported? run via 'just verify-pki')"
    [ -n "$out" ]
}
```

If that is too strict, at least have `main()` require `COMPOSE_PROJECT_NAME` whenever `docker` is on PATH. Also make `stack_is_up` treat "docker reachable, compose errors" as a failure, not as down.

### WR-02: "No published port" is inferred from a command whose failure is swallowed, so a compose error on a publishing row becomes a green skip

**File:** `scripts/verify-pki.sh:365-372`, `scripts/verify-pki.sh:398-402`
**Issue:** `published_endpoints` sends compose's stderr to /dev/null, and its exit status is lost: it is the `sort -u` status, and the caller adds `|| true`. Any failure of `docker compose ps --format '{{range .Publishers}}…'` yields empty output, and the row is then skipped as `no published port`. That failure could be transient, or a compose build whose `ps` template does not expose `.Publishers` (the Pi's distro plugin, for example). This applies to rabbitmq, caddy and axiam-server, which do publish. In full mode the result is `✓ verify-pki` and a surfaced but green skip. The comment at lines 336-340 ("The only per-row skip left is a running service that publishes no port") assumes this command cannot fail.
**Fix:** Take the expectation from static config rather than the runtime probe. Read the service's declared `ports:` from `docker compose config --format json`, or add a published/unpublished column to `listeners.conf`. Then fail when ports are declared but none are observed, and propagate compose's exit status:

```bash
published_endpoints() {
    local raw
    raw="$(docker compose -f "$COMPOSE_FILE" ps --format '…' "$1")" || return 1
    printf '%s' "$raw" | tr ' ' '\n' | sed '…' | sort -u
}
endpoints="$(published_endpoints "$name")" || fail "${name}  could not read published ports"
```

### WR-03: `group()` in `just verify` now returns non-zero on a clean pass

**File:** `just/verify.just:54-56`
**Issue:** The line added by this diff, `grep -F '→ skipped' <<<"$out" | sed …`, is the last command of the success branch. When there are no skip lines, `grep` exits 1, and under `set -o pipefail` the pipeline, and so `group`, returns 1. The failure branch ends in `printf | sed` and returns 0. So `group`'s status is now inverted exactly when the group passed cleanly. No current caller reads it (the verdict comes from `$fail`), but any future `group … || …` or `if group …` gets the opposite answer. Before this diff the branch ended in `ok` and returned 0.
**Fix:**

```bash
        ok "$name"
        grep -F '→ skipped' <<<"$out" | sed 's/^[[:space:]]*/      /' || true
      else
        bad "$name"
        printf '%s\n' "$out" | sed 's/^/      /'
        return 1
      fi
```

### WR-04: The CPU hogs' lifetime is a fixed 600 s, not tied to the iteration count, so large runs fail spuriously

**File:** `just/pki.just:46`, `just/pki.just:129-133`, `just/pki.just:147-153`
**Issue:** `iterations` may be as high as 200 and each iteration may take up to 60 s (`timeout 60`). The hogs die after a hard-coded `timeout 600`. On a host where one loaded `--live-only` run takes more than about 3 s, 200 iterations outlast the load. The run then fails with "the CPU load ended before the last iteration", even with zero probe failures. The idle cost here is 0.35 s on 12 cores; the Pi 5 has 4 slower cores and runs under 8 hogs. This fails loudly, not falsely green, but it makes the guard unusable at its own advertised upper bound.
**Fix:** Derive the lifetime from the bound instead of a magic number, for example `hog_secs=$(( iterations * 60 + 120 ))` and then `timeout "$hog_secs" sh -c 'while :; do :; done' &`.

## Info

### IN-01: The advertised `source scripts/verify-pki.sh` usage leaks `set -euo pipefail` and `cd` into the caller

**File:** `scripts/verify-pki.sh:18-19`, `scripts/verify-pki.sh:27-30`, `scripts/verify-pki.sh:59`
**Issue:** Sourcing runs `set -euo pipefail` and `cd "$REPO_ROOT"` in the caller's shell, and `fail()` calls `exit 1`. The header offers this as a general usage. In an interactive shell, one failing predicate closes the terminal. `just verify-pki-stress` is safe only because it always sources inside a subshell.
**Fix:** Document "source only inside a subshell: `( source scripts/verify-pki.sh; … )`", or apply `set -euo pipefail` and `cd` only inside `main`.

### IN-02: The negative controls are fixed to rabbitmq whatever `row` is, and `tls_ep` is not normalised

**File:** `just/pki.just:75-126`
**Issue:** `just verify-pki-stress caddy` still runs NC1-NC8 against rabbitmq only, so the row under stress has no controls of its own. `tls_ep`/`http_ep` come raw from `docker compose port`, without the `0.0.0.0` / `[::]` / `:::` rewriting that `published_endpoints` applies. If the binding changes to a wildcard, the NCs would probe a different address form than the suite itself.
**Fix:** Refuse `row != rabbitmq` until the controls are made row-generic, or build `tls_ep` through `published_endpoints` from the sourced script.

### IN-03: The NC7 controls accept any non-zero exit, not the specific refusal

**File:** `just/pki.just:120-123`
**Issue:** `control` checks only the exit status. `--live-only no-such-row` also exits non-zero for unrelated reasons, such as `stack down`, `openssl not on PATH` or a disk-gate failure. In any of those cases NC7 passes without proving that an unknown row or a client row is refused. `branch()` already checks the message; NC7 should too.
**Fix:** Run NC7 through a message-checking helper that expects `no server row of that name`.

### IN-04: The failure message names a bogus SNI for a row with no DNS SAN

**File:** `scripts/verify-pki.sh:420-430`
**Issue:** When `sans` has no `DNS:` entry, `${sans#*DNS:}` leaves `first_dns` set to the first entry (for example `IP:10.0.0.1`). The message then says `probed with SNI IP:10.0.0.1` even though `probe_sni` was empty.
**Fix:** Set `first_dns=""` unless `sans` contains `DNS:`.

### IN-05: The device-twin and postgres TLS listeners are never live-asserted

**File:** `scripts/verify-pki.sh:398-402`
**Issue:** Both are server rows with no published port (D-05), so every run skips them. The skips are now visible in `just verify`, but the gate is still green with two of five server listeners never handshaken.
**Fix:** Optional: probe them from inside the compose network (for example `docker compose run --rm domo-probe` or `exec` into a sibling container running `openssl s_client`), or record the exemption explicitly in `listeners.conf`.

---

_Reviewed: 2026-09-29_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
