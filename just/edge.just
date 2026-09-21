# The edge: Caddy's route map, and PostgreSQL's TLS posture.
#
# Ownership split (D-28/D-34): `just/stack.just` owns the Docker lifecycle,
# `just/pki.just` owns certificates, and this module owns the two things that
# only become true once the stack is running — which upstream a request
# actually reaches, and whether the database came up with TLS on.
#
# Every assertion here follows plan 01-02's vocabulary: `→` for a step, `✓` for
# a passing assertion, `✗` for a failure that names BOTH the request and what it
# reached. A check that cannot run prints WHY it was skipped; a silent skip is
# indistinguishable from a pass and is how a broken route map survives a demo.

# --- helpers -----------------------------------------------------------------

# The exported root, not the in-use original. `.secrets/pki/root.pem` is what
# the servers were issued from; `dist/trust/domo-root.pem` is what a CLIENT
# machine is told to trust (D-14), so verifying against it is the check that
# actually matches what a browser will do.
trust_bundle := "dist/trust/domo-root.pem"

# --- the route map -----------------------------------------------------------

# Assert the single origin: matcher ORDER, prefix boundaries, TLS version, the
# absence of any Caddy-minted certificate, and that every HTTPS upstream is
# verified by name against the one root.
#
# Splits into two halves on purpose. The configuration half reads the Caddyfile
# and always runs — it is what catches D-30's ordering rule being broken by an
# edit. The live half needs a running stack and a resolvable DOMO_HOST, and
# skips with a printed reason when it does not have them.
edge-verify:
    #!/usr/bin/env bash
    set -uo pipefail
    cf=deploy/caddy/Caddyfile
    fail=0
    ok()   { printf '  ✓ %s\n' "$1"; }
    bad()  { printf '  ✗ %s\n' "$1"; fail=1; }
    set -a; if [ -f .env ]; then . ./.env; fi; set +a
    host="${DOMO_HOST:-domo.local}"

    echo "→ Caddyfile configuration"
    [ -f "$cf" ] || { echo "  ✗ missing $cf"; exit 1; }

    # --- no second certificate authority (T-03-05) ---------------------------
    if [ "$(grep -c 'tls internal' "$cf")" -eq 0 ]; then
        ok "no 'tls internal' — Caddy mints nothing from its own CA"
    else
        bad "'tls internal' present: Caddy would mint from its OWN CA, a second trust anchor"
    fi
    if [ "$(grep -c 'auto_https off' "$cf")" -ge 1 ]; then
        ok "auto_https off — no ACME, no automatic HTTPS"
    else
        bad "global 'auto_https off' missing: Caddy would try ACME on a LAN-only demo"
    fi
    if grep -qE '^\s*(acme_ca|acme_dns|email)\b' "$cf"; then
        bad "an ACME directive is present; this demo has no internet exposure and one trust anchor"
    else
        ok "no ACME directive anywhere"
    fi

    # --- TLS 1.3 only (D-07) -------------------------------------------------
    tls_blocks=$(grep -c 'tls /etc/caddy/tls/server.pem' "$cf")
    proto_13=$(grep -c 'protocols tls1.3' "$cf")
    if [ "$tls_blocks" -ge 1 ] && [ "$proto_13" -eq "$tls_blocks" ]; then
        ok "all $tls_blocks site block(s) serve a static root-signed certificate, TLS 1.3 only"
    else
        bad "site blocks=$tls_blocks but 'protocols tls1.3' appears $proto_13 time(s) — every block must pin 1.3"
    fi

    # --- matcher ORDER (D-30, T-03-03) ---------------------------------------
    # The whole reason this recipe exists. Byte offsets, not eyeballs.
    mgmt=$(grep -n 'handle /api/mgmt/\*' "$cf" | head -1 | cut -d: -f1)
    twin=$(grep -n 'handle_path /api/twin/\*' "$cf" | head -1 | cut -d: -f1)
    axiam=$(grep -n '@axiam path' "$cf" | head -1 | cut -d: -f1)
    landing=$(grep -n 'root \* /srv/landing' "$cf" | head -1 | cut -d: -f1)
    if [ -n "$mgmt" ] && [ -n "$twin" ] && [ -n "$axiam" ] && [ -n "$landing" ]; then
        if [ "$mgmt" -lt "$axiam" ] && [ "$twin" -lt "$axiam" ] && [ "$axiam" -lt "$landing" ]; then
            ok "order: /api/mgmt(:$mgmt) and /api/twin(:$twin) before AXIAM(:$axiam) before the landing page(:$landing)"
        else
            bad "matcher order violates D-30: mgmt=$mgmt twin=$twin axiam=$axiam landing=$landing — /api/mgmt and /api/twin must come first"
        fi
    else
        bad "could not locate all four handlers (mgmt=$mgmt twin=$twin axiam=$axiam landing=$landing)"
    fi

    # --- prefix boundary (the nginx analog's documented /oauth2-clients bug) --
    if grep -qE "path .*(/oauth2\*|/oauth2 )" "$cf"; then
        bad "a bare /oauth2 matcher would also swallow /oauth2-clients — keep the trailing slash"
    else
        ok "/oauth2/* keeps its trailing slash; /oauth2-clients falls through"
    fi

    # --- upstream verification (T-03-04) -------------------------------------
    # Every https:// upstream must name BOTH a trust pool and a server name.
    up_https=$(grep -c 'reverse_proxy https://' "$cf")
    pools=$(grep -c 'tls_trust_pool file' "$cf")
    names=$(grep -c 'tls_server_name' "$cf")
    if [ "$up_https" -ge 1 ] && [ "$pools" -eq "$up_https" ] && [ "$names" -eq "$up_https" ]; then
        ok "$up_https HTTPS upstream(s), each naming a trust pool AND a server name"
    else
        bad "https upstreams=$up_https but trust pools=$pools and server names=$names — none may omit either"
    fi
    if grep -qE 'tls_insecure_skip_verify|insecure_skip_verify' "$cf"; then
        bad "a verification skip is present — fix the SAN in deploy/pki/listeners.conf instead"
    else
        ok "no verification skip anywhere"
    fi

    # --- forged client certificate at the edge (T-03-01) ---------------------
    strips=$(grep -c 'header_up -X-Client-Certificate' "$cf")
    if [ "$up_https" -ge 1 ] && [ "$strips" -ge "$up_https" ]; then
        ok "$strips proxy hop(s) strip the inbound X-Client-Certificate header"
    else
        bad "only $strips of $up_https proxies strip X-Client-Certificate — a client could forge a device identity"
    fi

    # --- the root is never served (T-03-06, D-14) ----------------------------
    if grep -qE 'dist/trust|domo-root\.(pem|der)' "$cf"; then
        bad "the Caddyfile references the exported root — the certificate is never offered for download"
    else
        ok "no route maps to the exported root certificate"
    fi

    echo "→ live route map"
    if ! command -v docker >/dev/null 2>&1; then
        echo "  → skipped (docker is not on PATH)"
    elif [ -z "$(just _dc ps -q caddy 2>/dev/null)" ]; then
        echo "  → skipped (caddy is not running for this compose project — 'just up' first)"
    elif [ ! -s "{{trust_bundle}}" ]; then
        echo "  → skipped (no {{trust_bundle}} — run 'just export-trust')"
    else
        just _dc exec -T caddy caddy validate --config /etc/caddy/Caddyfile >/dev/null 2>&1 \
            && ok "caddy validate accepts the running configuration" \
            || bad "caddy validate rejected /etc/caddy/Caddyfile"

        if ! getent hosts "$host" >/dev/null 2>&1; then
            echo "  → skipped: $host does not resolve on this machine."
            echo "    Publish it (avahi-publish -a -R $host <LAN_IP>, and the same for axiam.$host)"
            echo "    or add both names to /etc/hosts. Confirm with: getent hosts $host axiam.$host"
        else
            c="curl -sS --cacert {{trust_bundle}} -o /dev/null -w %{http_code}"

            code=$($c "https://$host/api/twin/healthz" 2>/dev/null)
            case "$code" in
              200) ok "/api/twin/healthz → 200 from the Device Twin" ;;
              404) bad "/api/twin/healthz reached axiam-server (matcher order), got 404" ;;
              *)   bad "/api/twin/healthz returned $code — the Twin upstream is unreachable or failed verification" ;;
            esac

            code=$($c "https://$host/.well-known/openid-configuration" 2>/dev/null)
            [ "$code" = "200" ] \
                && ok "/.well-known/openid-configuration → 200 from AXIAM, unprefixed" \
                || bad "/.well-known/openid-configuration returned $code — AXIAM is not reachable unprefixed (PLAT-06)"

            code=$($c "https://$host/oauth2/jwks" 2>/dev/null)
            [ "$code" = "200" ] \
                && ok "/oauth2/jwks → 200 from AXIAM" \
                || bad "/oauth2/jwks returned $code from the AXIAM matcher"

            code=$($c "https://$host/api/mgmt/anything" 2>/dev/null)
            [ "$code" = "503" ] \
                && ok "/api/mgmt/* → 503, the route is reserved ahead of AXIAM" \
                || bad "/api/mgmt/anything returned $code, expected the reserved 503"

            ct=$(curl -sS --cacert {{trust_bundle}} -D- -o /dev/null "https://$host/oauth2-clients" 2>/dev/null | tr -d '\r')
            if printf '%s' "$ct" | grep -qi '^content-type: text/html'; then
                ok "/oauth2-clients was served by the landing handler, NOT captured by /oauth2/*"
            else
                bad "/oauth2-clients was captured by the /oauth2/* matcher (it did not reach the landing page)"
            fi

            ct=$(curl -sS --cacert {{trust_bundle}} -D- -o /dev/null "https://$host/" 2>/dev/null | tr -d '\r')
            printf '%s' "$ct" | grep -qi '^content-type: text/html' \
                && ok "/ → text/html from the static front door" \
                || bad "/ did not return HTML — the landing handler was shadowed by an earlier matcher"

            if getent hosts "axiam.$host" >/dev/null 2>&1; then
                body=$(curl -sS --cacert {{trust_bundle}} "https://axiam.$host/" 2>/dev/null)
                if printf '%s' "$body" | grep -qi '<div id="root"\|<script type="module"\|/assets/'; then
                    ok "https://axiam.$host/ → the console SPA, on its own host and its own cookie jar"
                else
                    bad "https://axiam.$host/ did not return the console SPA"
                fi
                printf '%s' "$body" | grep -qi 'AXIAM Domo Demo' \
                    && bad "https://axiam.$host/ returned the LANDING page — the two hosts are not separated" \
                    || ok "the console host never returns the landing page"
            else
                echo "  → skipped: axiam.$host does not resolve (see the note above)"
            fi

            # D-07, both directions. A 1.3-only listener must ACCEPT 1.3 and
            # REFUSE 1.2; asserting only the first would pass on a listener
            # that quietly accepts everything.
            if openssl s_client -connect "$host:443" -servername "$host" \
                   -CAfile {{trust_bundle}} -verify_return_error -tls1_3 </dev/null >/dev/null 2>&1; then
                if openssl s_client -connect "$host:443" -servername "$host" -tls1_2 </dev/null >/dev/null 2>&1; then
                    bad "a TLS 1.2 handshake was ACCEPTED on :443 — D-07 requires 1.3 only"
                else
                    ok "TLS 1.3 verifies against the exported root; TLS 1.2 is refused"
                fi
            else
                bad "the TLS 1.3 handshake on :443 failed chain verification against {{trust_bundle}}"
            fi
        fi
    fi

    if [ "$fail" -eq 0 ]; then echo "✓ edge-verify"; else echo "✗ edge-verify"; exit 1; fi

# Reload Caddy's configuration and certificates IN PLACE.
#
# `just demo-reset` re-issues every certificate; a container restart would drop
# every connection in flight for the few seconds it takes to come back
# (T-03-09). Caddy's admin API swaps the configuration — new certificate files
# included — with no dropped connection, which is also how AXIAM's own Pi
# deployment guide treats a renewal.
#
# The landing page needs nothing here: its fingerprint is read from
# /srv/landing/fingerprint.txt per request, so a rotated root is live as soon as
# `just export-trust` rewrites that file.
edge-reload:
    @echo "→ reloading caddy"
    @just _dc exec -T caddy caddy reload --config /etc/caddy/Caddyfile --force
    @echo "  ✓ configuration and certificates reloaded, no restart"
