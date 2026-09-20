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

# Assert every property of every certificate the listener table declares:
# algorithm, extensions, SANs, lifetime bound, and the chain to the one root.
# Live TLS handshakes are checked too when this compose project is running, and
# skipped — not failed — when it is not.
verify-pki:
    @set -a; if [ -f .env ]; then . ./.env; fi; set +a; scripts/verify-pki.sh

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
