# The phase's verification gates.
#
# Two recipes, and the difference between them is cost, not rigour:
#
#   just verify        the fast gate. Everything that can be asserted against the
#                      stack as it stands — seconds to a couple of minutes.
#   just phase-verify  the phase gate. reset → verify → smoke → reset → smoke,
#                      because this phase's own standard is two consecutive green
#                      cycles rather than one. Tens of minutes.
#
# # This file aggregates; it does not reimplement
#
# Every assertion below already exists somewhere, contributed by the plan that
# owned the thing it asserts: `just verify-pki` (01-02), `just guard-secrets`
# (01-02), `just edge-verify` and `just pg-verify` (01-03), `just authz-verify`
# (01-04), `just twin-test` (01-05), `just smoke` (01-06). What this file adds is
# the mapping from those commands to the ROADMAP's four success criteria, so a
# reader can check the run against the phase's own definition of done rather than
# against a list of commands, plus the two gates nobody else owned: the findings
# audit (D-32) and the operator-documentation drift check.
#
# # NOTE
#
# `set shell` is deliberately absent. `just` refuses a setting that two imported
# modules both define, and `just/stack.just` already sets it for the whole import
# tree.

# --- the fast gate ------------------------------------------------------------

# Everything that does not need a reset. One line per group, one final verdict.
#
# Exit code is the whole point: a group that prints ✗ fails the recipe. Nothing
# here is advisory.
verify:
    #!/usr/bin/env bash
    set -uo pipefail

    fail=0
    ok()   { printf '  ✓ %s\n' "$*"; }
    bad()  { printf '  ✗ %s\n' "$*"; fail=1; }
    note() { printf '  … %s\n' "$*"; }
    rule() { local t="$*"; local n=$(( 60 - ${#t} )); [ "$n" -lt 1 ] && n=1
             printf '\n── %s %s\n' "$t" "$(printf '─%.0s' $(seq 1 "$n"))"; }

    # Run a contributed recipe as one named group: stream nothing on success
    # beyond its own name, print its whole output on failure. A green gate that
    # prints four hundred lines stops being read.
    group() {
      local name="$1"; shift
      local out rc
      out="$("$@" 2>&1)"; rc=$?
      if [ "$rc" -eq 0 ]; then
        ok "$name"
      else
        bad "$name"
        printf '%s\n' "$out" | sed 's/^/      /'
      fi
    }

    echo "→ verify  the fast gate"

    # =========================================================================
    rule "the repository"
    # These need no running stack: they are facts about the source tree.
    # =========================================================================

    group "build          cargo build --workspace --all-targets" \
        cargo build --quiet --workspace --all-targets
    group "lint           cargo clippy --workspace --all-targets -- -D warnings" \
        cargo clippy --quiet --workspace --all-targets -- -D warnings
    group "tests          cargo test --workspace" \
        cargo test --quiet --workspace
    group "secrets-guard  staged paths, ignore rules, build context (PKI-06)" \
        just guard-secrets

    # --- findings audit (D-32) ----------------------------------------------
    #
    # The mechanism that keeps the dogfooding log honest as later phases add
    # workarounds, which only works if it runs every time. Four assertions; the
    # contract is written out in the log's own "How this file is audited".
    LOG=docs/dogfooding-findings.md
    ISSUES=docs/dogfooding-issues
    fa=0
    fa_bad() { printf '  ✗ findings-audit  %s\n' "$*"; fa=1; fail=1; }

    if [ ! -s "$LOG" ]; then
      fa_bad "$LOG is missing"
    else
      entries="$(grep -oE '^## DF-[0-9]{3}' "$LOG" | sed 's/^## //' | sort -u)"

      # 1. every cited identifier resolves to an entry
      for id in $(grep -rhoE 'DF-[0-9]{3}' crates tools services 2>/dev/null | sort -u); do
        printf '%s\n' "$entries" | grep -qx "$id" \
          || fa_bad "a hand-rolled call cites ${id}, which has no entry in ${LOG}"
      done

      # 2. every file naming an AXIAM REST path cites at least one identifier.
      #    This is the half that catches the real risk — a NEW hand-rolled call
      #    with no finding written for it. A citation with no entry is caught by
      #    (1); a call with no citation at all is caught here.
      for f in $(grep -rlE '/api/v1/' crates tools services --include='*.rs' 2>/dev/null); do
        grep -qE 'DF-[0-9]{3}' "$f" \
          || fa_bad "${f} calls an AXIAM REST path and cites no DF- identifier"
      done

      # 3. entry ids and upstream-issue files are the same set, in both
      #    directions: an entry with no issue body is an undocumented gap, and an
      #    issue file with no entry is an orphan nobody will ever read.
      files="$(ls "$ISSUES" 2>/dev/null | sed -n 's/\.md$//p' | sort -u)"
      for id in $entries; do
        printf '%s\n' "$files" | grep -qx "$id" \
          || fa_bad "${id} has no upstream issue body at ${ISSUES}/${id}.md (D-33)"
      done
      for id in $files; do
        printf '%s\n' "$entries" | grep -qx "$id" \
          || fa_bad "${ISSUES}/${id}.md has no matching '## ${id}' entry in ${LOG}"
      done

      # 4. the log stays inside the project's own 500-line cap. Splitting the
      #    issue bodies out is what keeps it there; this is what stops the next
      #    twenty entries from quietly breaking the rule again.
      n="$(wc -l < "$LOG" | tr -d ' ')"
      [ "$n" -le 500 ] \
        || fa_bad "${LOG} is ${n} lines, over the project's 500-line cap — split more issue bodies into ${ISSUES}/"

      [ "$fa" -eq 0 ] && ok "findings-audit  $(printf '%s\n' "$entries" | grep -c .) entries, every citation resolves, every entry has its issue body, ${n} lines"
    fi

    # --- operator documentation drift ---------------------------------------
    #
    # An operator discovers a missing key by reading, not by a failed start. The
    # key set is DERIVED from the compose file's own `operator`/`generated`
    # markers rather than listed here, so the gate and the documentation cover
    # one subset by construction rather than two by agreement.
    od=0
    od_bad() { printf '  ✗ operator-docs  %s\n' "$*"; od=1; fail=1; }

    body="$(grep -v '^[[:space:]]*#' deploy/compose.yml)"
    tot="$(printf '%s\n' "$body" | grep -ho '\${[A-Z_][A-Z0-9_]*:?' | wc -l)"
    opn="$(printf '%s\n' "$body" | grep -ho '\${[A-Z_][A-Z0-9_]*:?operator' | wc -l)"
    gen="$(printf '%s\n' "$body" | grep -ho '\${[A-Z_][A-Z0-9_]*:?generated' | wc -l)"
    if [ "$tot" -ne $(( opn + gen )) ]; then
      od_bad "$(( tot - opn - gen )) \${VAR:?} guard(s) in deploy/compose.yml open with neither 'operator' nor 'generated' — nobody has decided who supplies the value. Fix the guard's message, not this gate."
    fi

    if [ ! -s .env.example ]; then
      od_bad ".env.example is missing"
    else
      for k in $(printf '%s\n' "$body" | grep -ho '\${[A-Z_][A-Z0-9_]*:?operator' | sed 's/^\${//;s/:?operator$//' | sort -u); do
        grep -qE "^#*[[:space:]]*${k}=" .env.example \
          || od_bad "env key undocumented: ${k}"
      done
      # The reverse direction: a generated secret set by hand REPLACES a minted
      # value with a weaker one (D-13), so it must never be offered here.
      for k in $(printf '%s\n' "$body" | grep -ho '\${[A-Z_][A-Z0-9_]*:?generated' | sed 's/^\${//;s/:?generated$//' | sort -u); do
        grep -qE "^#*[[:space:]]*${k}=" .env.example \
          && od_bad "generated secret offered to the operator in .env.example: ${k} (D-13)"
      done
    fi

    for s in "cargo test --workspace" "just verify" "just phase-verify"; do
      grep -qF "$s" CLAUDE.md || od_bad "CLAUDE.md missing command: ${s}"
    done

    [ "$od" -eq 0 ] && ok "operator-docs  ${tot} compose guards all marked, $(grep -c '^[A-Z_]*=' .env.example 2>/dev/null) operator keys documented, CLAUDE.md records the gates"

    # --- topology invariants -------------------------------------------------
    tp=0
    tp_bad() { printf '  ✗ topology  %s\n' "$*"; tp=1; fail=1; }

    # D-06: exactly one compose file, and no per-host override. The moment the
    # two reference machines run different topologies, the Pi run stops proving
    # anything about the laptop run.
    nc="$(find . -path ./target -prune -o \( -name 'compose*.y*ml' -o -name 'docker-compose*.y*ml' \) -print 2>/dev/null | wc -l)"
    [ "$nc" -eq 1 ] || tp_bad "${nc} compose files exist; D-06 allows exactly one (deploy/compose.yml)"

    # D-09 / T-07-01: blanket volume pruning must appear nowhere in the
    # orchestration. It would also destroy the SIBLING AXIAM checkout's
    # development volumes, which are not ours to touch. The patterns below are
    # spelled with character classes precisely so this gate does not match its
    # own source — the literal two-word form appears nowhere in the repository,
    # which is what makes the raw grep in `just demo-reset`'s header honest.
    if grep -rnE 'docker[[:space:]]+volume[[:space:]]+prune|prune[^|]*--volumes|--volumes[^|]*prune' \
         just scripts tools services deploy 2>/dev/null | grep -v 'verify\.just'; then
      tp_bad "a blanket volume prune appears in the orchestration (D-09) — reset removes named volumes one by one"
    fi

    # D-10 / T-07-02: the root survives a reset. Asserted statically here (the
    # recipe keeps the root guard) and dynamically by `just phase-verify`, which
    # compares the fingerprint across a real reset.
    grep -q 'pki-root.done' just/stack.just \
      || tp_bad "just demo-reset no longer excludes pki-root.done — a reset would rotate the organization root (D-10)"

    group "topology-compose  docker compose config validates" just _dc config --quiet
    [ "$tp" -eq 0 ] && ok "topology  one compose file, no volume pruning, the root guard survives reset"

    # =========================================================================
    rule "the phase's four success criteria"
    # Named for the ROADMAP's own wording, so this run can be checked against
    # the phase's definition of done rather than against a list of commands.
    # These need the stack up.
    # =========================================================================

    if [ -z "$(just _dc ps -q 2>/dev/null)" ]; then
      bad "the stack is not running — the four criteria below assert against a live stack. Run 'just up' first."
      echo
      echo "✗ verify"
      exit 1
    fi

    # --- SC1 -----------------------------------------------------------------
    printf '\n  SC1  one-command start, and a reset that is idempotent (PLAT-01, PLAT-02, PLAT-05)\n'
    sc=0
    for st in pki axiam-up org-bootstrap tenants catalog service-certs broker platform-up; do
      [ -e ".secrets/state/${st}" ] || [ -e ".secrets/state/${st}.done" ] \
        || { bad "SC1  stage marker missing: ${st} — 'just up' has not completed on this machine"; sc=1; }
    done
    # D-36: a successful run prints a demo card AND writes it, at owner-only
    # mode, because it carries the super-admin login (T-07-04).
    if [ ! -s .secrets/demo-card.txt ]; then
      bad "SC1  .secrets/demo-card.txt is missing — 'just up' did not reach its verify stage"; sc=1
    else
      for k in Portal Console super-admin SHA256 export-trust; do
        grep -q "$k" .secrets/demo-card.txt || { bad "SC1  demo card missing: ${k}"; sc=1; }
      done
      m="$(stat -c '%a' .secrets/demo-card.txt)"
      [ "$m" = 600 ] || { bad "SC1  demo card mode is ${m}, not 600 — it holds the super-admin login (T-07-04)"; sc=1; }
    fi
    # A fresh IMAGE is not a fresh CONTAINER. Every assertion below is only as
    # good as the binaries actually running, so a service still on a superseded
    # image fails the gate here rather than silently validating old code. Read
    # only — the gate reports, `just _containers-fresh` repairs.
    for c in $(just _dc ps -q 2>/dev/null); do
      [ "$(docker inspect --format '{{"{{"}}index .Config.Labels "com.docker.compose.oneoff"{{"}}"}}' "$c" 2>/dev/null)" = True ] && continue
      ref="$(docker inspect --format '{{"{{"}}.Config.Image{{"}}"}}' "$c" 2>/dev/null)" || continue
      cur="$(docker inspect --format '{{"{{"}}.Image{{"}}"}}' "$c" 2>/dev/null)" || continue
      want="$(docker image inspect --format '{{"{{"}}.Id{{"}}"}}' "$ref" 2>/dev/null)" || continue
      if [ "$cur" != "$want" ]; then
        bad "SC1  $(docker inspect --format '{{"{{"}}index .Config.Labels "com.docker.compose.service"{{"}}"}}' "$c") is running ${cur:7:12}, not ${ref} (${want:7:12}) — run 'just _containers-fresh'"
        sc=1
      fi
    done
    [ "$sc" -eq 0 ] && ok "SC1  every stage marker present, every running container on its current image, demo card at 0600 with all five fields"
    note "SC1  the reset half — two consecutive green cycles across a real 'just demo-reset' — is 'just phase-verify'"

    # --- SC2 -----------------------------------------------------------------
    printf '\n  SC2  a single organization root, and server certificates a browser trusts (PKI-01…06)\n'
    group "SC2  verify-pki   the whole chain, every listener, live TLS per row" just verify-pki
    if [ -s dist/trust/domo-root.sha256 ] && [ -s .secrets/pki/root.pem ]; then
      a="$(cut -d= -f2 dist/trust/domo-root.sha256 | tr -d ' ')"
      b="$(openssl x509 -in .secrets/pki/root.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ' ')"
      [ "$a" = "$b" ] \
        && ok "SC2  the exported fingerprint round-trips to the in-use root" \
        || bad "SC2  dist/trust/domo-root.sha256 does not match .secrets/pki/root.pem — run 'just export-trust'"
    else
      bad "SC2  the trust export is missing — run 'just export-trust'"
    fi
    note "SC2  the browser half (Chrome and Firefox, no warning) is a HUMAN check — docs/setup.md § Troubleshooting, and RESEARCH A7 is still [ASSUMED]"

    # --- SC3 -----------------------------------------------------------------
    printf '\n  SC3  a single origin, and a queryable resource tree with the group pattern (PLAT-06, AUTHZ-01, AUTHZ-02)\n'
    group "SC3  edge-verify  single origin, route matchers, TLS 1.3, the AXIAM discovery documents" just edge-verify
    group "SC3  pg-verify    PostgreSQL reachable, both schemas, TLS" just pg-verify
    group "SC3  authz-verify the tree, the catalog and the group-per-(role, resource) pattern" just authz-verify

    # --- SC4 -----------------------------------------------------------------
    printf '\n  SC4  a device reaching the broker through a working authorization backend (MQTT-01, MQTT-02)\n'
    group "SC4  twin-test    the Twin's authorization backend, offline: tokens, tenants, topic scheme" just twin-test
    note "SC4  the live end-to-end matrix is 'just smoke', in the phase gate — and it is DELIBERATELY RED; 'just smoke-gate' explains exactly why"

    # =========================================================================
    echo
    if [ "$fail" -eq 0 ]; then
      echo "✓ verify"
      echo "  The live device-connect matrix is NOT part of this gate. It is red by an"
      echo "  explicit decision, not by oversight — run 'just smoke-gate' to read why."
    else
      echo "✗ verify"
      exit 1
    fi

# --- the smoke gate, and why it is red ----------------------------------------

# `just smoke`, with the one thing a bare invocation cannot give a reader: which
# of its failures are known and attributed, and which would be news.
#
# # The gate is red on purpose
#
# Two of twelve matrix cases fail against the pinned AXIAM build. Plan 01-06
# confirmed both as defects in AXIAM rather than in this harness, and the user
# was asked directly what to do about it and chose: LEAVE IT RED until the defect
# is fixed upstream *and that fix is verified here*.
#
# So nothing in this recipe makes the suite pass. It does not pin the observed
# behaviour, exclude the cases, weaken an assertion or discard an exit status. A
# negative case must never be made to pass by weakening the thing it tests. What
# it adds is attribution, so that a permanently-red gate stays READ rather than
# becoming wallpaper: the two known failures are named with their findings, and
# any THIRD failure is called out as a regression in its own right.
smoke-gate:
    #!/usr/bin/env bash
    set -uo pipefail

    # The two cases plan 01-06 confirmed. Anything else failing is news.
    KNOWN="cross-tenant-ca-issuance other-tenant-ca"

    out="$(just smoke 2>&1)"; rc=$?
    printf '%s\n' "$out"

    failed="$(printf '%s\n' "$out" | sed -n 's/^✗ \([a-z0-9-]*\) .*/\1/p' | sort -u)"
    unexpected=""
    for c in $failed; do
      case " $KNOWN " in *" $c "*) ;; *) unexpected="${unexpected} ${c}" ;; esac
    done
    missing=""
    for c in $KNOWN; do
      printf '%s\n' "$failed" | grep -qx "$c" || missing="${missing} ${c}"
    done

    echo
    echo "─── smoke-gate ───────────────────────────────────────────────────────"

    if [ "$rc" -eq 0 ]; then
      echo "  ! The suite is GREEN, and it was not expected to be."
      echo
      echo "    cross-tenant-ca-issuance and other-tenant-ca have failed against every"
      echo "    build this demo has pinned. If they now pass, the most likely reason is"
      echo "    that AXIAM's beta17 fix (T22.1) is finally running here — which is good"
      echo "    news that has to be CONFIRMED rather than assumed, because a negative"
      echo "    case can also go green by being broken."
      echo
      echo "    Work through 'What to re-run once the images exist' in"
      echo "    docs/dogfooding-upstream-status.md, then move DF-017 and DF-025 to"
      echo "    'resolved' with the runtime evidence recorded in place."
      echo "✓ smoke-gate  (unexpectedly green — confirm before believing it)"
      exit 0
    fi

    echo "  The two cases below FAIL against the pinned AXIAM build, and that is the"
    echo "  demo's own recorded position rather than a broken harness or a regression"
    echo "  in this repository's code:"
    echo
    echo "    ✗ cross-tenant-ca-issuance   DF-017 — AXIAM signs a leaf under ANOTHER"
    echo "                                 tenant's signing CA at this tenant's request"
    echo "    ✗ other-tenant-ca            DF-025 — that forged leaf then connects,"
    echo "                                 publishes and subscribes, end to end"
    echo
    echo "  What it means: the per-tenant CA tier does not currently separate tenants."
    echo "  Certificate issuance is not a tenant boundary on the pinned build."
    echo
    echo "  FIXED UPSTREAM, NOT VERIFIED HERE. AXIAM 1.0.0-beta17 fixes DF-017 as T22.1"
    echo "  ('prepare_leaf_issuance' now reads ca_certificate.tenant_id), and DF-025 had"
    echo "  no cause of its own beyond DF-017, so its precondition should disappear with"
    echo "  it. NONE of that is exercised by this repository: the beta17 container images"
    echo "  are not published, so the demo stays pinned at beta16, server AND SDK"
    echo "  together. See docs/dogfooding-upstream-status.md — it lists exactly what to"
    echo "  re-run the day the images exist."
    echo
    echo "  This gate stays RED by explicit user decision until that re-run happens."
    echo "  Do not pin these cases, exclude them, or soften an assertion to clear it."

    if [ -n "$unexpected" ]; then
      echo
      echo "  ✗✗ REGRESSION — case(s) failing that are NOT the known pair:${unexpected}"
      echo "     These are NEWS. Everything above explains the other two and explains"
      echo "     nothing about these. Diagnose them on their own terms."
    fi
    if [ -n "$missing" ]; then
      echo
      echo "  !  Known case(s) did NOT fail this run:${missing}"
      echo "     Either the build moved, or the case stopped running. Both need an answer"
      echo "     before this gate's attribution can be trusted again."
    fi

    echo
    if [ -n "$unexpected" ]; then
      echo "✗ smoke-gate  known-red PLUS a regression"
    else
      echo "✗ smoke-gate  known-red: exactly the two attributed failures, nothing else"
    fi
    exit 1

# --- the phase gate -----------------------------------------------------------

# reset → verify → smoke → reset → smoke.
#
# Two consecutive cycles, because the phase's own standard is that the second one
# is green too: a failure only on the second cycle points at state the first left
# behind, which one cycle can never distinguish from a clean pass. This is the
# reset-crossed half of PLAT-05, handed over from plan 01-06 — which proved the
# smoke suite re-runnable against the mechanisms available at its own wave, but
# could not cross a reset because `just demo-reset` did not exist yet.
#
# The organization root's fingerprint is captured before the first reset and
# compared after the last (D-10, T-07-02): a reset that rotated the root would
# invalidate the trust every browser and LAN client has already installed, and
# nothing else in the gate would notice.
#
# # This recipe exits non-zero, and that is the correct outcome today
#
# `just smoke` is deliberately red — see `just smoke-gate`. The phase closes with
# a known-red gate by explicit user decision. What this recipe guarantees is that
# the redness is exactly the two attributed cases and nothing else, on both
# cycles, and that everything around them is green.
phase-verify:
    #!/usr/bin/env bash
    set -uo pipefail

    echo "→ phase-verify  reset → verify → smoke → reset → smoke"
    echo

    root_fp() {
      [ -s .secrets/pki/root.pem ] || { echo "<absent>"; return; }
      openssl x509 -in .secrets/pki/root.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ' '
    }
    FP_BEFORE="$(root_fp)"
    echo "  root before: ${FP_BEFORE}"
    echo

    hard=0     # a genuine failure: anything but the attributed smoke pair
    step() {
      local label="$1"; shift
      printf '\n═══ %s ═══\n' "$label"
      "$@"
      local rc=$?
      if [ "$rc" -ne 0 ]; then
        printf '\n✗ phase-verify: %s failed\n' "$label"
        return 1
      fi
      printf '\n✓ %s\n' "$label"
      return 0
    }

    # The smoke step is the one place a non-zero exit is not automatically a
    # stop: known-red is the expected state. `smoke-gate` distinguishes the two,
    # and the marker line it prints is what this loop reads.
    smoke_step() {
      local label="$1"
      printf '\n═══ %s ═══\n' "$label"
      local out rc
      out="$(just smoke-gate 2>&1)"; rc=$?
      printf '%s\n' "$out"
      if [ "$rc" -eq 0 ]; then
        printf '\n! %s: green, which is unexpected — see the note above\n' "$label"
        return 0
      fi
      if printf '%s' "$out" | grep -q 'known-red PLUS a regression'; then
        printf '\n✗ %s: a case failed that is NOT the attributed pair\n' "$label"
        return 1
      fi
      printf '\n✓ %s: known-red, exactly the two attributed cases\n' "$label"
      return 0
    }

    step  "cycle 1 · reset"  just demo-reset      || hard=1
    [ "$hard" -eq 0 ] && { step "cycle 1 · verify" just verify || hard=1; }
    [ "$hard" -eq 0 ] && { smoke_step "cycle 1 · smoke"        || hard=1; }
    [ "$hard" -eq 0 ] && { step "cycle 2 · reset"  just demo-reset || hard=1; }
    [ "$hard" -eq 0 ] && { smoke_step "cycle 2 · smoke"        || hard=1; }

    FP_AFTER="$(root_fp)"
    echo
    echo "  root after:  ${FP_AFTER}"
    if [ "$FP_BEFORE" != "$FP_AFTER" ]; then
      echo "  ✗ the organization root CHANGED across the reset cycles (D-10). Every browser and"
      echo "    LAN client that trusted the old root no longer trusts this demo. Only"
      echo "    'just pki-rotate-root' may do this, and it appears in no automated path."
      hard=1
    else
      echo "  ✓ the organization root is unchanged across both resets (D-10)"
    fi

    echo
    if [ "$hard" -ne 0 ]; then
      echo "✗ phase-verify"
      exit 1
    fi

    echo "✗ phase-verify  known-red"
    echo
    echo "  Both cycles behaved identically and everything except the device-connect"
    echo "  matrix is green. The matrix fails on exactly two cases — DF-017 and DF-025 —"
    echo "  fixed upstream in AXIAM 1.0.0-beta17 as T22.1 but NOT verified here, because"
    echo "  the beta17 images are not published. 'just smoke-gate' spells it out; the"
    echo "  re-run list is in docs/dogfooding-upstream-status.md."
    echo
    echo "  Phase 1 closes with this gate red by explicit user decision. The exit status"
    echo "  is non-zero on purpose: the honest report of a confirmed, unfixed security"
    echo "  defect is a failing gate, not a green one with a footnote."
    exit 1
