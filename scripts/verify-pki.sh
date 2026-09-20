#!/usr/bin/env bash
#
# Whole-chain PKI assertion suite for the AXIAM Domo Demo (PKI-01, PKI-04).
#
# The tracer in plan 01-01 proved that ONE chain works. This suite asserts that
# EVERY chain is correct: the right algorithm, the right extensions, the right
# SANs, inside the browser lifetime cap, anchored in the one offline root.
#
# It is driven by `deploy/pki/listeners.conf` — the same table `gen-pki.sh`
# issues from — rather than by a hardcoded list of listener names. A listener
# added by a later plan is therefore both issued AND verified with no edit here.
# A row whose leaf is missing is a hard failure naming that row, never a silent
# pass.
#
# Usage: scripts/verify-pki.sh        (normally: `just verify-pki`)
# Env:   DOMO_HOST (default: domo.local), DOMO_LAN_IP (default: 127.0.0.1)
#        — must match the values `gen-pki.sh` issued with, or the SAN
#          comparison will correctly report the certificates as stale.
#
# Exit: 0 when every assertion passes; non-zero on the FIRST failure, with the
#       listener and the failing property named.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PKI_DIR=".secrets/pki"
AXIAM_DIR=".secrets/axiam"
LISTENERS="deploy/pki/listeners.conf"
COMPOSE_FILE="deploy/compose.yml"

DOMO_HOST="${DOMO_HOST:-domo.local}"
DOMO_LAN_IP="${DOMO_LAN_IP:-127.0.0.1}"

# CA/Browser Forum maximum for a server certificate. INCLUSIVE: exactly 397
# days passes, 398 fails.
MAX_LEAF_DAYS=397
MAX_LEAF_SECS=$(( MAX_LEAF_DAYS * 86400 ))

MIN_FREE_GB=8

# --- output vocabulary (Pattern S-7) -----------------------------------------
group() { printf '→ %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
skip()  { printf '  → skipped (%s)\n' "$*"; }
fail()  { printf '✗ %s\n' "$*" >&2; exit 1; }

# --- disk hygiene gate (project CLAUDE.md) -----------------------------------
check_disk() {
    local mp avail
    for mp in /home /; do
        [ -d "$mp" ] || continue
        avail=$(df -BG --output=avail "$mp" 2>/dev/null | tail -1 | tr -dc '0-9')
        [ -n "$avail" ] || continue
        if [ "$avail" -lt "$MIN_FREE_GB" ]; then
            fail "only ${avail}G free on ${mp}; need ${MIN_FREE_GB}G before running openssl."
        fi
    done
}

# --- helpers -----------------------------------------------------------------

# Body of one X.509v3 extension, or empty when the extension is absent.
# `openssl x509 -ext` exits 0 and prints a "No extensions" banner for a missing
# extension, so emptiness has to be decided here rather than from the status.
ext() {
    local cert="$1" name="$2" out
    out="$(openssl x509 -noout -ext "$name" -in "$cert" 2>/dev/null || true)"
    case "$out" in
        *"No extensions"*|'') printf '' ;;
        *) printf '%s' "$out" ;;
    esac
}

# Seconds between notBefore and notAfter.
cert_lifetime_secs() {
    local cert="$1" nb na
    nb="$(openssl x509 -noout -startdate -in "$cert" | cut -d= -f2-)"
    na="$(openssl x509 -noout -enddate  -in "$cert" | cut -d= -f2-)"
    echo $(( $(date -u -d "$na" +%s) - $(date -u -d "$nb" +%s) ))
}

# How openssl RENDERS a SAN entry declared in listeners.conf's openssl syntax.
# `IP:` is emitted as `IP Address:`; `DNS:` is emitted unchanged.
san_rendered_form() {
    case "$1" in
        IP:*)  printf 'IP Address:%s' "${1#IP:}" ;;
        *)     printf '%s' "$1" ;;
    esac
}

# Expand and de-duplicate a listeners.conf SAN field exactly as gen-pki.sh does,
# so the comparison is against what was actually issued rather than against a
# second, subtly different reading of the same row.
expand_sans() {
    local sans="$1"
    sans="${sans//\$\{DOMO_HOST\}/$DOMO_HOST}"
    sans="${sans//\$\{DOMO_LAN_IP\}/$DOMO_LAN_IP}"
    echo "$sans" | tr ',' '\n' | awk 'NF && !seen[$0]++' | paste -sd, -
}

# --- 1. the organization root ------------------------------------------------
verify_root() {
    group "root CA"

    [ -f "${PKI_DIR}/root.pem" ] || fail "root  missing ${PKI_DIR}/root.pem — run 'just pki' (or 'just tracer') first"
    [ -f "${PKI_DIR}/root.key" ] || fail "root  missing ${PKI_DIR}/root.key — run 'just pki' (or 'just tracer') first"

    # P-5: AXIAM's BYOK import parses the key with `rcgen::KeyPair::from_pem`,
    # which accepts PKCS#8 and rejects PKCS#1. `openssl pkey` parses BOTH, so
    # the parse alone does not discriminate — the PEM banner is what does.
    openssl pkey -in "${PKI_DIR}/root.key" -noout 2>/dev/null \
        || fail "root  private key does not parse"
    grep -q -- '-----BEGIN PRIVATE KEY-----' "${PKI_DIR}/root.key" \
        || fail "root  private key is not PKCS#8 (AXIAM's BYOK import rejects the PKCS#1 'BEGIN RSA PRIVATE KEY' form — P-5)"
    ok "key parses and is PKCS#8"

    local bc ku ski text
    bc="$(ext "${PKI_DIR}/root.pem" basicConstraints)"
    [ -n "$bc" ] || fail "root  no basicConstraints extension"
    printf '%s' "$bc" | grep -q 'critical'  || fail "root  basicConstraints is not critical"
    printf '%s' "$bc" | grep -q 'CA:TRUE'   || fail "root  basicConstraints is not CA:TRUE"
    # P-5 again: pathlen:0 imports cleanly and then invalidates every tenant-CA
    # chain for Erlang, browsers and webpki. There must be no pathlen at all.
    printf '%s' "$bc" | grep -qi 'pathlen'  && fail "root  basicConstraints carries a pathlen constraint; AXIAM issues tenant signing CAs beneath this root and a pathlen would forbid that tier (P-5)"
    ok "basicConstraints critical, CA:TRUE, no pathlen"

    ku="$(ext "${PKI_DIR}/root.pem" keyUsage)"
    [ -n "$ku" ] || fail "root  no keyUsage extension"
    printf '%s' "$ku" | grep -q 'critical'           || fail "root  keyUsage is not critical"
    printf '%s' "$ku" | grep -q 'Certificate Sign'   || fail "root  keyUsage does not contain keyCertSign"
    printf '%s' "$ku" | grep -q 'CRL Sign'           || fail "root  keyUsage does not contain cRLSign"
    ok "keyUsage critical, keyCertSign + cRLSign"

    ski="$(ext "${PKI_DIR}/root.pem" subjectKeyIdentifier)"
    [ -n "$ski" ] || fail "root  no subjectKeyIdentifier extension"
    ok "subjectKeyIdentifier present"

    text="$(openssl x509 -noout -text -in "${PKI_DIR}/root.pem")"
    printf '%s' "$text" | grep -q 'Public Key Algorithm: rsaEncryption' \
        || fail "root  public key is not RSA (D-11: AXIAM CAs may be RSA-4096 or Ed25519; browsers reject Ed25519 in a server chain)"
    printf '%s' "$text" | grep -q 'Public-Key: (4096 bit)' \
        || fail "root  public key is not 4096 bit"
    ok "RSA-4096 public key"
}

# --- 2. every row of the listener table --------------------------------------
verify_leaf() {
    local name="$1" kind="$2" sans="$3"
    local crt="${PKI_DIR}/${name}.pem"
    local key="${PKI_DIR}/${name}.key"

    [ -f "$crt" ] || fail "${name}  missing ${crt} — the listener table declares this row but no certificate was issued for it; run 'just pki'"
    [ -f "$key" ] || fail "${name}  missing ${key}"

    local text bc ku eku san
    text="$(openssl x509 -noout -text -in "$crt")"

    # D-15: ECDSA P-256 leaves under the RSA-4096 root.
    printf '%s' "$text" | grep -q 'Public Key Algorithm: id-ecPublicKey' \
        || fail "${name}  leaf key is not ECDSA (D-15 requires ECDSA P-256)"
    printf '%s' "$text" | grep -q 'ASN1 OID: prime256v1' \
        || fail "${name}  leaf curve is not prime256v1 (D-15)"

    bc="$(ext "$crt" basicConstraints)"
    [ -n "$bc" ] || fail "${name}  no basicConstraints extension"
    printf '%s' "$bc" | grep -q 'CA:FALSE' || fail "${name}  basicConstraints is not CA:FALSE — a leaf must not be a CA"

    ku="$(ext "$crt" keyUsage)"
    [ -n "$ku" ] || fail "${name}  no keyUsage extension"
    printf '%s' "$ku" | grep -q 'critical'          || fail "${name}  keyUsage is not critical"
    printf '%s' "$ku" | grep -q 'Digital Signature' || fail "${name}  keyUsage does not contain digitalSignature"

    eku="$(ext "$crt" extendedKeyUsage)"
    [ -n "$eku" ] || fail "${name}  no extendedKeyUsage extension"

    san="$(ext "$crt" subjectAltName)"

    case "$kind" in
        server)
            printf '%s' "$eku" | grep -q 'TLS Web Server Authentication' \
                || fail "${name}  extendedKeyUsage does not contain serverAuth"

            # The empty-input edge: a SAN-less server certificate is useless to
            # every modern browser and must be a hard error, never a warning.
            [ -n "$sans" ] || fail "${name}  no SAN entries for ${name} — the listener table row declares none, and a server certificate without a subjectAltName is rejected by every current browser"
            [ -n "$san"  ] || fail "${name}  no SAN entries for ${name} — the certificate carries no subjectAltName extension"

            local entry want
            IFS=',' read -r -a _declared <<< "$sans"
            for entry in "${_declared[@]}"; do
                [ -n "$entry" ] || continue
                want="$(san_rendered_form "$entry")"
                # Bounded on both sides so `DNS:domo.local` cannot be satisfied
                # by `DNS:axiam.domo.local` sitting elsewhere in the list. SAN
                # ORDER is deliberately not asserted — only membership.
                printf '%s' "$san" | tr ',' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' \
                    | grep -qxF "$want" \
                    || fail "${name}  SAN does not contain ${entry} (declared in ${LISTENERS}); the certificate is stale — re-run 'just pki' with the same DOMO_HOST/DOMO_LAN_IP"
            done
            ;;
        client)
            # D-37 / C-4: the documented PKI-03 exception. AXIAM must present a
            # client certificate to RabbitMQ before AXIAM exists to issue one,
            # so this one client leaf is root-signed offline. A client leaf
            # carries no SAN, and must not.
            printf '%s' "$eku" | grep -q 'TLS Web Client Authentication' \
                || fail "${name}  extendedKeyUsage does not contain clientAuth"
            [ -z "$san" ] || fail "${name}  client leaf carries a subjectAltName; the listener table declares none"
            ;;
        *)
            fail "${name}  unknown kind '${kind}' in ${LISTENERS}"
            ;;
    esac

    local life
    life="$(cert_lifetime_secs "$crt")"
    if [ "$life" -gt "$MAX_LEAF_SECS" ]; then
        fail "${name}  validity $(( life / 86400 ))d exceeds ${MAX_LEAF_DAYS}d (the CA/Browser Forum cap; Chrome and Firefox reject anything longer)"
    fi

    openssl verify -CAfile "${PKI_DIR}/root.pem" "$crt" >/dev/null 2>&1 \
        || fail "${name}  does not verify against ${PKI_DIR}/root.pem — this leaf is not anchored in the one organization root"

    ok "${name}  (${kind}) ECDSA P-256, extensions, $(( life / 86400 ))d, chains to the root"
}

verify_listeners() {
    group "listener table (${LISTENERS})"
    [ -f "$LISTENERS" ] || fail "missing ${LISTENERS}"

    local rows=0 name kind sans
    while IFS='|' read -r name kind sans; do
        case "${name# }" in ''|\#*) continue ;; esac
        name="$(echo "$name" | tr -d '[:space:]')"
        kind="$(echo "$kind" | tr -d '[:space:]')"
        sans="$(expand_sans "$(echo "${sans:-}" | tr -d '[:space:]')")"
        [ -n "$name" ] || continue
        verify_leaf "$name" "$kind" "$sans"
        rows=$(( rows + 1 ))
    done < "$LISTENERS"

    [ "$rows" -gt 0 ] || fail "${LISTENERS} declares no listeners"
    ok "${rows} row(s) verified"
}

# --- 3. AXIAM-issued tenant signing CAs --------------------------------------
verify_tenant_cas() {
    group "tenant signing CAs (${AXIAM_DIR})"

    local root_subject found=0 ca subject
    root_subject="$(openssl x509 -noout -subject -in "${PKI_DIR}/root.pem")"

    shopt -s nullglob
    for ca in "${AXIAM_DIR}"/*-ca.pem; do
        found=$(( found + 1 ))
        openssl verify -CAfile "${PKI_DIR}/root.pem" "$ca" >/dev/null 2>&1 \
            || fail "$(basename "$ca")  does not verify against the organization root — PKI-01 requires a single trust anchor"
        subject="$(openssl x509 -noout -subject -in "$ca")"
        [ "$subject" != "$root_subject" ] \
            || fail "$(basename "$ca")  has the same subject as the root; this is the root, not an AXIAM-issued signing CA"
        ok "$(basename "$ca")  chains to the root, distinct subject"
    done
    shopt -u nullglob

    if [ "$found" -eq 0 ]; then
        # Legitimate before the bootstrap has run: AXIAM issues the tenant CAs,
        # so there are none on a machine that has only run `just pki`.
        skip "no AXIAM-issued signing CAs present — run 'just tracer' to provision them"
    fi
}

# --- 4. live listeners (only when OUR stack is up) ---------------------------
#
# Deliberately scoped to this compose project rather than to a raw port probe:
# a port being open says nothing about WHOSE listener answers it, and asserting
# our root against a stranger's listener would be a false failure.
stack_is_up() {
    command -v docker >/dev/null 2>&1 || return 1
    docker compose -f "$COMPOSE_FILE" ps -q >/dev/null 2>&1 || return 1
    [ -n "$(docker compose -f "$COMPOSE_FILE" ps -q 2>/dev/null)" ]
}

published_endpoints() {
    local service="$1"
    docker compose -f "$COMPOSE_FILE" ps --format json "$service" 2>/dev/null \
        | jq -r '(if type=="array" then .[] else . end)
                 | .Publishers[]?
                 | select(.PublishedPort > 0)
                 | "\(if (.URL // "") == "" then "127.0.0.1" else .URL end):\(.PublishedPort)"' 2>/dev/null \
        | sed 's/^0\.0\.0\.0:/127.0.0.1:/; s/^\[::\]:/127.0.0.1:/' \
        | sort -u
}

verify_live() {
    group "live listeners"

    if ! command -v jq >/dev/null 2>&1; then
        skip "jq is not on PATH"
        return 0
    fi
    if ! stack_is_up; then
        skip "stack down"
        return 0
    fi

    local name kind sans checked=0
    while IFS='|' read -r name kind sans; do
        case "${name# }" in ''|\#*) continue ;; esac
        name="$(echo "$name" | tr -d '[:space:]')"
        kind="$(echo "$kind" | tr -d '[:space:]')"
        sans="$(expand_sans "$(echo "${sans:-}" | tr -d '[:space:]')")"
        [ -n "$name" ] || continue
        [ "$kind" = "server" ] || continue

        local endpoints
        endpoints="$(published_endpoints "$name" || true)"
        if [ -z "$endpoints" ]; then
            skip "${name}: no published port"
            continue
        fi

        # A service may publish more than one port and only some of them speak
        # TLS (RabbitMQ publishes the plain-HTTP management UI alongside MQTTS).
        # Narrow to the ports that complete a handshake at all, so a non-TLS
        # port is never mistaken for a broken TLS listener.
        local addr tls_addrs=""
        for addr in $endpoints; do
            if openssl s_client -connect "$addr" </dev/null >/dev/null 2>&1; then
                tls_addrs="${tls_addrs} ${addr}"
            fi
        done
        if [ -z "$tls_addrs" ]; then
            skip "${name}: published, but no port answered a TLS handshake"
            continue
        fi

        # Every declared NAME must verify, so the check cannot depend on that
        # name's position inside the SAN extension.
        local entry host_name verified sni
        IFS=',' read -r -a _declared <<< "$sans"
        for entry in "${_declared[@]}"; do
            [ -n "$entry" ] || continue
            host_name="${entry#DNS:}"; host_name="${host_name#IP:}"
            # SNI carries host NAMES only; an IP SAN is checked without it.
            case "$entry" in
                IP:*) sni=() ;;
                *)    sni=(-servername "$host_name") ;;
            esac
            verified=0
            for addr in $tls_addrs; do
                if openssl s_client -connect "$addr" "${sni[@]}" \
                        -CAfile "${PKI_DIR}/root.pem" -verify_return_error -tls1_3 \
                        </dev/null >/dev/null 2>&1; then
                    verified=1
                    ok "${name}  TLS 1.3 verified for ${host_name} on ${addr}"
                    break
                fi
            done
            [ "$verified" -eq 1 ] \
                || fail "${name}  no published TLS listener verified for ${host_name} against ${PKI_DIR}/root.pem (tried:${tls_addrs})"
        done
        checked=$(( checked + 1 ))
    done < "$LISTENERS"

    [ "$checked" -gt 0 ] || skip "no published TLS listener to check"
}

# --- main --------------------------------------------------------------------
check_disk
command -v openssl >/dev/null 2>&1 || fail "openssl not on PATH"

verify_root
verify_listeners
verify_tenant_cas
verify_live

printf '✓ verify-pki\n'
