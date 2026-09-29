#!/usr/bin/env bash
#
# Preflight for the AXIAM Domo Demo (D-35).
#
# The first stage of `just up`. It asks the three questions that decide whether
# a run can possibly succeed, and refuses BEFORE anything is created rather
# than failing halfway through with the machine in a half-provisioned state:
#
#   1. disk      — is there room? (this machine has run out mid-session, and the
#                  resulting failures read as compiler or linker bugs, not as a
#                  full disk)
#   2. tools     — are docker + the compose v2 plugin, buildx, just and openssl
#                  present, and at what versions? cargo ONLY when a local build
#                  is requested, because the Pi never needs it (D-01/D-35).
#   3. host      — does the configured name resolve, what is the LAN address,
#                  and are the three published ports free?
#
# # What is deliberately NOT here
#
# There is no memory or swap check. The user removed it from D-35 explicitly;
# Pi memory is validated on real hardware in Phase 6 (PLAT-03). Adding one here
# would re-open a question that was closed.
#
# # This script reports and refuses. It never fixes.
#
# Every failure names the actual figure and what to do about it. Nothing here
# installs a package, frees a port, publishes a name or deletes a file — a
# preflight that repairs the machine is a preflight whose next run tells you
# nothing.
#
# Usage: scripts/preflight.sh        (normally: `just preflight`)
#
# Env:
#   DOMO_HOST           the portal host name           (default: domo.local)
#   DOMO_LAN_IP         the LAN address to advertise   (default: detected)
#   DOMO_STACK_RUNNING  1 when THIS compose project already holds the ports, so
#                       the port check is skipped. `just` decides this from
#                       `docker compose ps -q`; a port held by our own stack is
#                       the normal state of a second `just up`, not a conflict.
#   DOMO_NEED_CARGO     1 when a local cargo build is about to happen.
#
# Exit: 0 when every check passes. 1 on the first refusal, naming the shortfall.

set -uo pipefail

MIN_FREE_GB="${DOMO_MIN_FREE_GB:-8}"
DOMO_HOST="${DOMO_HOST:-domo.local}"

# Every port `deploy/compose.yml` publishes to the host. The list is not "the
# LAN-facing ports" — it is "the ports a bind can fail on", which is a strictly
# larger set, and the difference is what this check exists to catch.
#
#   443    Caddy, the single browser-facing origin      (LAN + loopback, D-05)
#   8090   AXIAM's own TLS listener, devices reach it directly (LAN + loopback, D-05)
#   8883   RabbitMQ MQTTS                               (LAN, D-05)
#   15672  RabbitMQ's management API                    (loopback only)
#
# 15672 was missing until plan 01-07, and its absence was not theoretical: the
# SIBLING AXIAM checkout's own development stack publishes 15672 on 0.0.0.0,
# which occupies our loopback bind too. Preflight passed, `just up` then died
# inside `docker compose up` with a bind error — precisely the "fails halfway"
# this script exists to prevent. A loopback-only publication is still a
# publication.
PUBLISHED_PORTS="443 8090 8883 15672"

fail_count=0

group() { printf '→ %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
note()  { printf '  … %s\n' "$*"; }
bad()   { printf '  ✗ %s\n' "$*"; fail_count=$(( fail_count + 1 )); }

# -----------------------------------------------------------------------------
# 1. Disk
#
# Checked FIRST, before anything slow, because it is the failure that disguises
# itself as something else. A cargo build that dies with a linker error and a
# docker pull that dies with "unexpected EOF" are the same bug when the disk is
# full, and neither message says so.
# -----------------------------------------------------------------------------
check_disk() {
    group "disk (need ${MIN_FREE_GB}G free)"
    local mp avail short
    for mp in /home /; do
        [ -d "$mp" ] || continue
        avail=$(df -BG --output=avail "$mp" 2>/dev/null | tail -1 | tr -dc '0-9')
        if [ -z "$avail" ]; then
            note "could not read free space on ${mp} — skipping"
            continue
        fi
        if [ "$avail" -lt "$MIN_FREE_GB" ]; then
            short=$(( MIN_FREE_GB - avail ))
            bad "${mp}: ${avail}G free, need ${MIN_FREE_GB}G — short by ${short}G. Reclaim space first: a build that runs under this line fails with errors that look like compiler bugs."
        else
            ok "${mp}: ${avail}G free"
        fi
    done
}

# -----------------------------------------------------------------------------
# 2. Tools and versions
#
# The version is PRINTED, not asserted against a floor. A floor here would be a
# second place to update every time a tool moves, and the demo has never been
# broken by a tool being too new — it has been broken by one being absent.
# -----------------------------------------------------------------------------
version_of() {
    case "$1" in
        docker)  docker --version 2>/dev/null | head -1 ;;
        just)    just --version 2>/dev/null | head -1 ;;
        openssl) openssl version 2>/dev/null | head -1 ;;
        cargo)   cargo --version 2>/dev/null | head -1 ;;
        *)       "$1" --version 2>/dev/null | head -1 ;;
    esac
}

check_tools() {
    group "tools"
    local t v
    for t in docker just openssl; do
        if command -v "$t" >/dev/null 2>&1; then
            v="$(version_of "$t")"
            ok "${t}  ${v:-present}"
        else
            bad "${t} is not on PATH"
        fi
    done

    # The compose v2 PLUGIN, not the retired `docker-compose` script. Every
    # recipe in this repository calls `docker compose`, so the standalone
    # binary being present would not help.
    if docker compose version >/dev/null 2>&1; then
        ok "docker compose  $(docker compose version --short 2>/dev/null || echo present) (v2 plugin)"
    else
        bad "the docker compose v2 plugin is missing — 'docker-compose' the standalone script is not a substitute; every recipe here calls 'docker compose'"
    fi

    # buildx is what produces the arm64 variants PLAT-02 needs. It is checked
    # even on the Pi, where nothing is built: a missing buildx there means
    # `just images-verify` cannot inspect a manifest either.
    if docker buildx version >/dev/null 2>&1; then
        ok "docker buildx  $(docker buildx version 2>/dev/null | awk '{print $2}')"
    else
        bad "docker buildx is missing — the multi-architecture build and 'just images-verify' both need it"
    fi

    if docker info >/dev/null 2>&1; then
        ok "the Docker daemon is reachable"
    else
        bad "the Docker daemon is not reachable — is it running, and is this user in the docker group?"
    fi

    # cargo ONLY when a local build is requested. The demo machine runs the
    # published images and the one-shot tools inside the compose network
    # (D-35), which is precisely what lets a Raspberry Pi with no Rust
    # toolchain bring the whole stack up.
    if [ "${DOMO_NEED_CARGO:-0}" = "1" ]; then
        if command -v cargo >/dev/null 2>&1; then
            ok "cargo  $(version_of cargo)  (a local build was requested)"
        else
            bad "cargo is not on PATH and a local build was requested — either install Rust or drop the local build; the stack itself never needs it"
        fi
    else
        note "cargo not checked (no local build requested — the Pi never needs it)"
    fi
}

# -----------------------------------------------------------------------------
# 3. Host name, LAN address, ports
#
# Resolution is REPORTED, never failed on. `domo.local` is an mDNS name the
# operator publishes, and on a machine where they have not yet, the demo still
# works over the LAN address — `just edge-verify` pins the name to it. What a
# browser on another machine needs is a different question, and that is what the
# printed instruction is for.
# -----------------------------------------------------------------------------
detect_lan_ip() {
    if [ -n "${DOMO_LAN_IP:-}" ]; then
        printf '%s' "$DOMO_LAN_IP"
        return
    fi
    # The address this host would use to reach the LAN: ask the routing table
    # rather than guessing from an interface name, which differs per machine.
    ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src") {print $(i+1); exit}}'
}

check_host() {
    group "host and ports"

    local lan
    lan="$(detect_lan_ip)"
    if [ -n "$lan" ]; then
        ok "LAN address ${lan}"
    else
        bad "could not detect a LAN address, and DOMO_LAN_IP is unset — set it in .env (see .env.example); every server certificate carries it as a SAN"
    fi

    local n
    for n in "$DOMO_HOST" "axiam.${DOMO_HOST}"; do
        if getent hosts "$n" >/dev/null 2>&1; then
            ok "${n} resolves here"
        else
            note "${n} does NOT resolve here. The stack still comes up and 'just edge-verify' pins the name to ${lan:-the LAN address}, but a browser on another machine cannot reach the demo until it is published:"
            note "    avahi-publish -a -R ${n} ${lan:-<LAN_IP>}      (or add it to /etc/hosts)"
        fi
    done

    if [ "${DOMO_STACK_RUNNING:-0}" = "1" ]; then
        note "ports not checked — this compose project already holds them, which is the normal state of a second run"
        return
    fi

    if ! command -v ss >/dev/null 2>&1; then
        note "ss is not on PATH — cannot check whether the published ports are free"
        return
    fi

    local p holder
    for p in $PUBLISHED_PORTS; do
        if ss -ltn 2>/dev/null | grep -qE "[:.]${p}[[:space:]]"; then
            bad "port ${p} is already in use — the stack will fail to publish it."
            # Naming the holder is the difference between a refusal a reader can
            # act on and one they have to investigate. A container is by far the
            # likeliest holder here, and the likeliest container is the SIBLING
            # AXIAM checkout's own development stack, which is not ours to stop.
            holder="$(docker ps --format '{{.Names}}\t{{.Ports}}' 2>/dev/null \
                      | grep -E "[:.]${p}->" | cut -f1 | paste -sd', ' -)"
            if [ -n "$holder" ]; then
                note "    held by container: ${holder}"
                case "$holder" in
                    axiam-*)
                        note "    that is the SIBLING AXIAM checkout's development stack, not this demo's."
                        note "    Stop it there (its own 'just'/compose), or free the port — do not 'docker stop' it blindly;" ;;
                    *)
                        note "    if it is a previous run of THIS demo, 'just down' first;" ;;
                esac
                note "    its volumes are not ours to touch."
            else
                note "    no container publishes it — a host process holds it. 'ss -ltnp' names which."
            fi
        else
            ok "port ${p} is free"
        fi
    done
}

check_disk
check_tools
check_host

if [ "$fail_count" -gt 0 ]; then
    printf '✗ preflight: %d check(s) refused. Nothing was started or changed.\n' "$fail_count" >&2
    exit 1
fi
printf '  ✓ preflight\n'
