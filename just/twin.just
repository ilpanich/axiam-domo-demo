# Device Twin recipes.
#
# The Twin's authorization suite is entirely offline: a `wiremock` stand-in for
# the organization key-set endpoint and locally signed Ed25519 tokens. Nothing
# here needs Docker, a broker, or the real AXIAM — which is the point. The live
# counterpart lives in plan 01-06's smoke.

# NOTE: `set shell` is deliberately absent. `just` refuses a setting that two
# imported modules both define, and `just/stack.just` already sets it for the
# whole import tree. Any module added later must leave it alone too.

# The whole offline authorization suite, plus the shared topic scheme.
twin-test:
    @echo "→ twin-test"
    @cargo test -p device-twin
    @cargo test -p domo-common
    # `--no-deps` keeps the lint scoped to this crate: workspace path
    # dependencies are checked, not linted, so a lint elsewhere in the
    # workspace is that crate's own gate to clear, not this one's.
    @cargo clippy -p device-twin --no-deps --all-targets -- -D warnings
    @echo "✓ twin-test"

# Run the Twin against the local TLS material, outside Docker.
#
# The defaults match `deploy/compose.yml`; override any of them inline, e.g.
#   just twin-run DOMO_TWIN_BIND=127.0.0.1:9443
twin-run *ENV:
    @DOMO_TWIN_BIND="${DOMO_TWIN_BIND:-127.0.0.1:8443}" \
     DOMO_TWIN_CERT="${DOMO_TWIN_CERT:-.secrets/pki/twin.pem}" \
     DOMO_TWIN_KEY="${DOMO_TWIN_KEY:-.secrets/pki/twin.key}" \
     DOMO_ROOT_CA="${DOMO_ROOT_CA:-.secrets/pki/root.pem}" \
     DOMO_TENANT_MAP_FILE="${DOMO_TENANT_MAP_FILE:-.secrets/state/tenants.json}" \
     RUST_LOG="${RUST_LOG:-device_twin=debug,info}" \
     {{ENV}} cargo run -p device-twin --bin domo-twin
