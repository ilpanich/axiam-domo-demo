# Offline PKI: issuance, whole-chain verification, root rotation and the
# secrets guards that keep the root key out of git and out of image layers.
#
# Ownership split with `just/stack.just` (D-34): that module owns the Docker
# lifecycle, this one owns everything to do with certificates and the secrets
# that back them. `just pki` lives HERE, and `stack.just`'s `up` depends on it
# across the module boundary — `just` flattens imports into one namespace, so a
# recipe only ever needs to exist once.
#
# `pki-rotate-root` is the one COSTLY action in this file and is deliberately
# absent from every automated path, including `just demo-reset` (D-10):
# rotating the root invalidates the trust every machine has already installed.

# --- issuance ----------------------------------------------------------------

# Generate the organization root (once, ever) and re-issue every leaf declared
# in deploy/pki/listeners.conf.
pki:
    @echo "→ offline PKI"
    @set -a; if [ -f .env ]; then . ./.env; fi; set +a; scripts/gen-pki.sh

# --- verification ------------------------------------------------------------

# Live TLS handshakes are checked too when this compose project is running. The
# live section is skipped only when the stack is down, and a running service
# with no published port is skipped by name. Every other way of not asserting a
# published listener is a failure (G-01-3).
#
# Assert every property of every certificate the listener table declares:
# algorithm, extensions, SANs, lifetime bound, and the chain to the one root.
verify-pki:
    @set -a; if [ -f .env ]; then . ./.env; fi; set +a; scripts/verify-pki.sh

# Deliberately outside `just verify` and `just phase-verify`: it saturates every
# core for a minute or two. Each hog runs under `timeout` and is killed on
# exit. It never retries: every iteration counts.
#
# The G-01-3 regression guard: one row's live handshakes under CPU contention.
verify-pki-stress $row="rabbitmq" $iterations="50":
    #!/usr/bin/env bash
    set -uo pipefail
    set -a; if [ -f .env ]; then . ./.env; fi; set +a
    if ! [[ "$row" =~ ^[a-z0-9-]+$ ]]; then
        echo "✗ verify-pki-stress  row must match ^[a-z0-9-]+\$ (got '${row}')"; exit 1
    fi
    if ! [[ "$iterations" =~ ^[0-9]+$ ]] || [ "$iterations" -lt 1 ] || [ "$iterations" -gt 200 ]; then
        echo "✗ verify-pki-stress  iterations must be an integer from 1 to 200 (got '${iterations}')"; exit 1
    fi
    marker="✓ ${row}  TLS 1.3 verified for"
    hogs=(); tmp=""
    cleanup() {
        if [ "${#hogs[@]}" -gt 0 ]; then
            kill -TERM "${hogs[@]}" 2>/dev/null; wait "${hogs[@]}" 2>/dev/null; hogs=()
        fi
        if [ -n "$tmp" ]; then rm -rf "$tmp"; tmp=""; fi
    }
    trap cleanup EXIT
    trap 'cleanup; exit 130' INT TERM
    echo "→ verify-pki-stress  ${row}, ${iterations} iteration(s)"

    # Idle precondition: the row must pass once without load, and fixes how
    # many names every loaded iteration must verify.
    out="$(timeout 60 scripts/verify-pki.sh --live-only "$row" 2>&1)"; rc=$?
    want="$(grep -cF "$marker" <<<"$out")"
    if [ "$rc" -ne 0 ] || [ "$want" -eq 0 ]; then
        printf '%s\n' "$out" | sed 's/^/      /'
        echo "✗ verify-pki-stress  precondition: the idle --live-only ${row} run did not pass"
        exit 1
    fi
    # The count must mean names: it equals the SAN entries the row declares,
    # read from listeners.conf through the script's own expand_sans.
    declared="$( source scripts/verify-pki.sh; set +e
        while IFS='|' read -r n k s; do
            case "${n# }" in ''|\#*) continue ;; esac
            [ "$(echo "$n" | tr -d '[:space:]')" = "$row" ] || continue
            expand_sans "$(echo "${s:-}" | tr -d '[:space:]')" | tr ',' '\n' | grep -c .
            break
        done < "$LISTENERS" )"
    declared="${declared:-0}"
    if [ "$declared" -eq 0 ] || [ "$want" -ne "$declared" ]; then
        printf '%s\n' "$out" | sed 's/^/      /'
        echo "✗ verify-pki-stress  precondition: ${want} name(s) verified but the ${row} row declares ${declared}"
        exit 1
    fi
    echo "  ✓ idle precondition  ${want}/${declared} declared name(s) verified, each an identity check"

    # Negative controls, before any load. Each subshell sources the script, so
    # it calls the REAL predicates; a vacuous one must never yield a green run.
    # They are rabbitmq-specific because rabbitmq is the listener under test.
    tls_ep="$(docker compose -f deploy/compose.yml port rabbitmq 8883 2>/dev/null)"
    http_ep="$(docker compose -f deploy/compose.yml port rabbitmq 15672 2>/dev/null)"
    if [ -z "$tls_ep" ] || [ -z "$http_ep" ]; then
        echo "✗ verify-pki-stress  rabbitmq publishes no 8883 or no 15672 endpoint (got '${tls_ep}' / '${http_ep}')"; exit 1
    fi
    tmp="$(mktemp -d)"
    if ! openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 \
            -subj /CN=verify-pki-negative-control -keyout "${tmp}/nc.key" -out "${tmp}/nc.pem" >/dev/null 2>&1; then
        echo "✗ verify-pki-stress  could not generate the throwaway CA"; exit 1
    fi
    control() { # control LABEL WANT(0|nonzero) FUNCTION ARGS…
        local label="$1" want="$2" rc; shift 2
        ( source scripts/verify-pki.sh; "$@" ) >/dev/null 2>&1; rc=$?
        if { [ "$want" = 0 ] && [ "$rc" -eq 0 ]; } || { [ "$want" != 0 ] && [ "$rc" -ne 0 ]; }; then
            echo "  ✓ ${label}"
        else
            echo "✗ verify-pki-stress  negative control failed: ${label} (exit ${rc})"; exit 1
        fi
    }
    control "NC1 tls_answers refuses plain HTTP on ${http_ep}" nonzero \
        tls_answers "$http_ep"
    control "NC2 tls_handshake_verified refuses a throwaway CA on ${tls_ep}" nonzero \
        tls_handshake_verified "$tls_ep" "${tmp}/nc.pem" DNS:rabbitmq
    control "NC3 tls_handshake_verified accepts the organization root on ${tls_ep}" 0 \
        tls_handshake_verified "$tls_ep" .secrets/pki/root.pem DNS:rabbitmq
    # NC9: the real predicate fails on a name, not only on a chain (CR-01).
    control "NC9 tls_handshake_verified refuses a DNS name the certificate does not carry on ${tls_ep}" nonzero \
        tls_handshake_verified "$tls_ep" .secrets/pki/root.pem DNS:not-a-san.invalid
    control "NC9 tls_handshake_verified refuses an IP the certificate does not carry on ${tls_ep}" nonzero \
        tls_handshake_verified "$tls_ep" .secrets/pki/root.pem IP:10.9.9.9
    control "NC9 tls_handshake_verified accepts a declared IP on ${tls_ep}" 0 \
        tls_handshake_verified "$tls_ep" .secrets/pki/root.pem IP:127.0.0.1
    control "NC9 tls_handshake_verified refuses to judge without an identity on ${tls_ep}" nonzero \
        tls_handshake_verified "$tls_ep" .secrets/pki/root.pem rabbitmq

    # Branch controls: override exactly one function, then every failure branch
    # of verify_live must FAIL with its own message, and never skip.
    branch() { # branch LABEL EXPECTED-TEXT OVERRIDE
        local label="$1" expect="$2" override="$3" out rc
        out="$( source scripts/verify-pki.sh; LIVE_ONLY=rabbitmq; eval "$override"; verify_live 2>&1 )"; rc=$?
        if [ "$rc" -ne 0 ] && grep -qF -- "$expect" <<<"$out" && ! grep -qF '→ skipped' <<<"$out"; then
            echo "  ✓ ${label}"
        else
            printf '%s\n' "$out" | sed 's/^/      /'
            echo "✗ verify-pki-stress  negative control failed: ${label} (exit ${rc})"; exit 1
        fi
    }
    branch "NC4 a listener service that is not running fails" \
        "✗ rabbitmq  the stack is up but rabbitmq is not running" \
        'service_running() { [ "$1" != rabbitmq ]; }'
    branch "NC5 no port answering a TLS handshake fails" \
        "no port answered a TLS handshake" 'tls_answers() { return 1; }'
    branch "NC6 a name that does not verify fails" \
        "no published TLS listener verified for" 'tls_handshake_verified() { return 1; }'
    # NC10: the REAL predicate at the REAL call site (CR-01), unlike NC6's mock. The leading DNS:rabbitmq keeps the probe's SNI a real name, so only the identity check can fail.
    branch "NC10 verify_live fails a row naming a DNS name its served certificate does not carry" \
        "no published TLS listener verified for not-a-san.invalid" \
        'expand_sans() { echo "DNS:rabbitmq,DNS:not-a-san.invalid"; }'
    branch "NC10 verify_live fails a row naming an IP its served certificate does not carry" \
        "no published TLS listener verified for 10.9.9.9" \
        'expand_sans() { echo "DNS:rabbitmq,IP:10.9.9.9"; }'
    control "NC7 --live-only refuses an unknown row" nonzero \
        scripts/verify-pki.sh --live-only no-such-row
    control "NC7 --live-only refuses a client row" nonzero \
        scripts/verify-pki.sh --live-only axiam-amqp-client
    control "NC8 stack_is_up is true on the live stack" 0 stack_is_up
    control "NC8 stack_is_up is false when no listener service runs" nonzero \
        eval 'service_running() { return 1; }; stack_is_up'

    # Compose-failure controls (WR-01, WR-02): a compose ERROR is never "stack
    # down" or "no published port"; a compose SUCCESS with no output still is.
    nc_fail() { # nc_fail LABEL RC OUTPUT
        printf '%s\n' "$3" | sed 's/^/      /'
        echo "✗ verify-pki-stress  negative control failed: ${1} (exit ${2})"; exit 1
    }
    label='NC13 a compose answer that succeeds with no containers is still "stack down"'
    out="$( source scripts/verify-pki.sh; docker() { :; }; LIVE_ONLY=""; verify_live 2>&1 )"; rc=$?
    if [ "$rc" -eq 0 ] && grep -qF '→ skipped (stack down)' <<<"$out"; then echo "  ✓ ${label}"; else nc_fail "$label" "$rc" "$out"; fi
    branch 'NC11 a compose failure is not "stack down"' "docker compose ps failed" \
        'docker() { echo "simulated compose failure" >&2; return 1; }'
    label='NC11 verify-pki run without COMPOSE_PROJECT_NAME fails, never "stack down" (WR-01)'
    out="$(env -i PATH="$PATH" HOME="$HOME" DOMO_HOST="${DOMO_HOST:-}" DOMO_LAN_IP="${DOMO_LAN_IP:-}" bash scripts/verify-pki.sh 2>&1)"; rc=$?
    if [ "$rc" -ne 0 ] && grep -qF 'docker compose ps failed' <<<"$out" \
        && grep -qF 'COMPOSE_PROJECT_NAME is missing a value' <<<"$out" \
        && ! grep -qF 'skipped (stack down)' <<<"$out" && ! grep -qx '✓ verify-pki' <<<"$out" \
        && [ "$(grep -c '^✗' <<<"$out")" -eq 1 ]; then
        echo "  ✓ ${label}"
    else
        nc_fail "$label" "$rc" "$out"
    fi
    branch 'NC12 a compose failure while reading published ports is not "no published port" (WR-02)' \
        "docker compose ps failed" \
        'docker() { case " $* " in *" --format "*) echo "simulated compose failure" >&2; return 1 ;; esac; command docker "$@"; }'

    # Load: 2 × nproc busy loops, each bounded by `timeout`.
    n=$(( 2 * $(nproc) ))
    for _ in $(seq "$n"); do
        timeout 600 sh -c 'while :; do :; done' &
        hogs+=("$!")
    done
    sleep 1

    failures=0; skips=0; first=""
    for i in $(seq "$iterations"); do
        out="$(timeout 60 scripts/verify-pki.sh --live-only "$row" 2>&1)"; rc=$?
        got="$(grep -cF "$marker" <<<"$out")"
        if [ "$rc" -ne 0 ] || [ "$got" -ne "$want" ]; then
            failures=$(( failures + 1 ))
            [ -n "$first" ] || first="iteration ${i}: exit ${rc}, ${got}/${want} verified"$'\n'"${out}"
        fi
        if grep -qF '→ skipped' <<<"$out"; then skips=$(( skips + 1 )); fi
    done

    alive=0
    for p in "${hogs[@]}"; do if kill -0 "$p" 2>/dev/null; then alive=1; fi; done
    load="$(cut -d' ' -f1 /proc/loadavg)"
    if [ "$alive" -eq 0 ]; then
        echo "✗ verify-pki-stress  the CPU load ended before the last iteration; nothing was proved under contention"
        exit 1
    fi
    summary="${row}: $(( iterations - failures ))/${iterations} under ${n} CPU hogs (loadavg ${load}), ${failures} failures, ${skips} skips"
    if [ "$failures" -eq 0 ] && [ "$skips" -eq 0 ]; then
        echo "✓ verify-pki-stress  ${summary}"
    else
        echo "✗ verify-pki-stress  ${summary}"
        if [ -n "$first" ]; then printf '%s\n' "$first" | sed 's/^/      /'; fi
        exit 1
    fi

# --- secrets guards ----------------------------------------------------------

# Three independent layers between the root key and a commit or an image layer:
# staged-path refusal, ignore integrity, build-context integrity (PKI-06, D-13).
# The pre-commit hook runs exactly this, so CI, `just verify` and the hook can
# never disagree about what is safe to commit.
guard-secrets:
    @scripts/guard-secrets.sh

# Point git at the repository's own hooks directory, so the pre-commit guard
# runs for everyone who clones. Git deliberately never does this automatically.
hooks-install:
    @git config core.hooksPath .githooks
    @echo "  ✓ core.hooksPath = $(git config core.hooksPath)"

# --- trust export (PKI-05, D-14) ---------------------------------------------

# Write the PUBLIC root certificate to dist/trust/ as PEM and DER with its
# SHA-256 fingerprint, then print the per-target import steps.
#
# The PRIVATE key is never touched and never copied. Nothing here starts a
# server: the root is deliberately NOT offered over HTTP for one-click
# download — the user considered that and rejected it (D-14). Distribution is
# this recipe plus the documented steps in docs/trust.md, and nothing else.
export-trust:
    @echo "→ exporting the organization root"
    @test -s .secrets/pki/root.pem || { echo "✗ .secrets/pki/root.pem is missing — run 'just pki' first"; exit 1; }
    @mkdir -p dist/trust
    @cp .secrets/pki/root.pem dist/trust/domo-root.pem
    @openssl x509 -in .secrets/pki/root.pem -outform der -out dist/trust/domo-root.der
    @openssl x509 -noout -fingerprint -sha256 -in .secrets/pki/root.pem > dist/trust/domo-root.sha256
    @chmod 644 dist/trust/domo-root.pem dist/trust/domo-root.der dist/trust/domo-root.sha256
    @# Self-check rather than a claim: the DER must round-trip to the in-use
    @# PEM byte for byte, or the two files a reviewer compares disagree.
    @openssl x509 -inform der -in dist/trust/domo-root.der | diff - .secrets/pki/root.pem >/dev/null \
        || { echo "✗ dist/trust/domo-root.der does not round-trip to the in-use root"; exit 1; }
    @test ! -e dist/trust/domo-root.key || { echo "✗ private key material found in dist/trust/ — refusing"; exit 1; }
    @echo "  ✓ dist/trust/domo-root.pem     PEM, for OS trust stores and certutil"
    @echo "  ✓ dist/trust/domo-root.der     DER, same bytes, for tools that want it"
    @echo "  ✓ dist/trust/domo-root.sha256  the fingerprint to compare before trusting"
    @echo ""
    @echo "  Fingerprint — compare this against the landing page before you trust anything:"
    @echo ""
    @sed 's/^/    /' dist/trust/domo-root.sha256
    @echo ""
    @echo "  Import it (full instructions, including how to confirm and how to remove: docs/trust.md):"
    @echo ""
    @echo "    ArchLinux         sudo trust anchor --store dist/trust/domo-root.pem"
    @echo "    Debian / Pi OS    sudo cp dist/trust/domo-root.pem /usr/local/share/ca-certificates/domo-root.crt"
    @echo "                      sudo update-ca-certificates"
    @echo "    Chrome/Chromium   certutil -d sql:\$HOME/.pki/nssdb -A -t \"C,,\" -n \"Domo Demo Root\" -i dist/trust/domo-root.pem"
    @echo "    Firefox           Settings → Privacy & Security → Certificates → View Certificates"
    @echo "                      → Authorities → Import, tick \"Trust this CA to identify websites\""
    @echo ""
    @echo "  Rotating the root ('just pki-rotate-root') invalidates every import above."

# --- rotation (COSTLY — never called by reset) -------------------------------

# Destroy the organization root and start a completely fresh PKI.
#
# D-10: `just demo-reset` re-imports the EXISTING root so browsers and the
# simulator PC stay trusted. This recipe is the opposite of that, and every
# machine listed in docs/trust.md must repeat its import step afterwards.
#
# Non-interactive callers (CI, a scripted teardown) must set
# DOMO_ROTATE_CONFIRM=rotate. There is no unguarded path.
pki-rotate-root:
    @echo ""
    @echo "  ############################################################"
    @echo "  #  ROOT ROTATION — this invalidates installed trust        #"
    @echo "  ############################################################"
    @echo ""
    @echo "  Every machine that has already trusted the current root will"
    @echo "  start showing certificate warnings until it repeats its step"
    @echo "  from docs/trust.md:"
    @echo ""
    @echo "    - the ArchLinux laptop      (trust anchor --store)"
    @echo "    - the Raspberry Pi          (update-ca-certificates)"
    @echo "    - Chromium / Chrome         (certutil, NSS user database)"
    @echo "    - Firefox                   (Authorities → Import)"
    @echo "    - the simulator PC"
    @echo ""
    @echo "  AXIAM's imported trust anchor and every tenant signing CA"
    @echo "  beneath it are re-created by the next 'just tracer'."
    @echo ""
    @if [ "${DOMO_ROTATE_CONFIRM:-}" = "rotate" ]; then \
        echo "  … DOMO_ROTATE_CONFIRM=rotate — proceeding"; \
    elif [ -t 0 ]; then \
        printf "  Type 'rotate' to continue: "; read -r reply; \
        if [ "$reply" != "rotate" ]; then echo "✗ aborted"; exit 1; fi; \
    else \
        echo "✗ refusing to rotate non-interactively; set DOMO_ROTATE_CONFIRM=rotate"; exit 1; \
    fi
    @rm -rf .secrets/pki .secrets/state/pki-root.done
    @echo "  ✓ previous root removed"
    @just pki
    @echo "  → new root fingerprint:"
    @openssl x509 -noout -fingerprint -sha256 -in .secrets/pki/root.pem
    @echo "  Next: 'just export-trust', then re-import on every machine above."
