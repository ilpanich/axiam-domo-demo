#!/usr/bin/env bash
#
# Secrets guard (PKI-06, D-13, T-02-01, T-02-02).
#
# Three INDEPENDENT layers stand between the organization root private key and
# a commit or a distributable image layer. They are independent on purpose:
# any one of them failing alone is enough to leak the key, so none of them is
# allowed to be the only thing standing there.
#
#   1. staged-path refusal   — `git add -f` deliberately defeats .gitignore.
#                              This is the layer that catches that.
#   2. ignore integrity      — the ignore rules themselves can be weakened, and
#                              nothing else would notice.
#   3. build-context integrity — .dockerignore, the Dockerfiles, and the bytes
#                              of the images actually produced.
#
# Usable two ways, with identical behaviour:
#   - as the pre-commit hook (.githooks/pre-commit, installed by `just hooks-install`)
#   - standalone: `just guard-secrets`, and from CI / `just verify`
#
# Exit: 0 when every layer holds; non-zero on the first violation, naming it.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Paths that must never reach git or a build context.
SECRET_DIRS=(.secrets dist)

# The canary: if THIS file is not ignored, nothing else about the ignore rules
# can be trusted either.
CANARY=".secrets/pki/root.key"

# Escape a literal string for use inside an ERE.
escape_re() { printf '%s' "$1" | sed 's/[][\.^$*+?(){}|\\\/]/\\&/g'; }

group() { printf '→ %s\n' "$*"; }
ok()    { printf '  ✓ %s\n' "$*"; }
skip()  { printf '  → skipped (%s)\n' "$*"; }
fail()  { printf '✗ %s\n' "$*" >&2; exit 1; }

# --- layer 1: staged-path refusal --------------------------------------------
#
# .gitignore is NOT relied on here. `git add -f .secrets/pki/root.key` ignores
# it completely, and that force-add — usually a reflex while chasing something
# else — is the accident this layer exists to catch.
guard_staged_paths() {
    group "staged paths"

    local staged violations=0 path dir
    staged="$(git diff --cached --name-only 2>/dev/null || true)"

    if [ -z "$staged" ]; then
        skip "nothing staged"
        return 0
    fi

    while IFS= read -r path; do
        [ -n "$path" ] || continue
        for dir in "${SECRET_DIRS[@]}"; do
            case "$path" in
                "$dir"|"$dir"/*)
                    printf '✗ refusing to commit secret path: %s\n' "$path" >&2
                    violations=$(( violations + 1 ))
                    ;;
            esac
        done
    done <<< "$staged"

    if [ "$violations" -gt 0 ]; then
        printf '✗ %d secret path(s) staged. Unstage them with: git restore --staged <path>\n' "$violations" >&2
        exit 1
    fi
    ok "no staged path under $(printf '%s/ ' "${SECRET_DIRS[@]}")"
}

# --- layer 2: ignore integrity -----------------------------------------------
guard_ignore_rules() {
    group "ignore rules"

    git check-ignore -q "$CANARY" \
        || fail "${CANARY} is not covered by an ignore rule — .gitignore has been weakened (PKI-06)"

    local tracked
    tracked="$(git ls-files -- "${SECRET_DIRS[@]}" 2>/dev/null || true)"
    if [ -n "$tracked" ]; then
        printf '%s\n' "$tracked" | sed 's/^/    /' >&2
        fail "$(printf '%s ' "${SECRET_DIRS[@]}")is tracked by git — the paths above are in the index"
    fi

    ok "${CANARY} is ignored; nothing under $(printf '%s/ ' "${SECRET_DIRS[@]}")is tracked"
}

# --- layer 3: build-context integrity ----------------------------------------
guard_dockerignore() {
    local dir
    [ -f .dockerignore ] || fail ".dockerignore is missing — every build context would carry ${SECRET_DIRS[*]}"
    for dir in "${SECRET_DIRS[@]}"; do
        # Accept `.secrets`, `.secrets/`, `/.secrets`, `**/.secrets` — all of
        # which exclude the directory — but nothing that merely mentions it.
        grep -qE "^[[:space:]]*/?(\*\*/)?$(escape_re "$dir")/?[[:space:]]*$" .dockerignore \
            || fail ".dockerignore does not exclude ${dir}"
    done
    ok ".dockerignore excludes $(printf '%s ' "${SECRET_DIRS[@]}")"
}

guard_dockerfiles() {
    local df line src dir
    shopt -s nullglob
    local dockerfiles=(deploy/docker/Dockerfile*)
    shopt -u nullglob

    if [ "${#dockerfiles[@]}" -eq 0 ]; then
        skip "no Dockerfile under deploy/docker/"
        return 0
    fi

    for df in "${dockerfiles[@]}"; do
        # A `COPY ../something` escapes the build context entirely — which is
        # also how a sibling checkout's secrets would get pulled in (D-02).
        while IFS= read -r line; do
            case "$line" in
                *"../"*) fail "${df}: '${line}' reaches outside the build context" ;;
            esac
            for dir in "${SECRET_DIRS[@]}"; do
                # As a PATH COMPONENT, so `/usr/lib/distro-tool` is not mistaken
                # for `dist/`. The dot in `.secrets` is escaped so the pattern
                # cannot match `mysecrets` either.
                if printf '%s' "$line" | grep -qE "(^|[[:space:]=\"'/])$(escape_re "$dir")(/|[[:space:]]|\$)"; then
                    fail "${df}: '${line}' names ${dir}, which must never enter a build context"
                fi
            done
        done < <(grep -iE '^[[:space:]]*(COPY|ADD)[[:space:]]' "$df" || true)

        # A bind mount is a second, quieter way into the context.
        while IFS= read -r line; do
            for dir in "${SECRET_DIRS[@]}"; do
                case "$line" in
                    *"source=${dir}"*|*"src=${dir}"*)
                        fail "${df}: '${line}' bind-mounts ${dir} into the build" ;;
                esac
            done
        done < <(grep -E 'mount=type=bind' "$df" || true)
    done

    ok "${#dockerfiles[@]} Dockerfile(s) copy nothing from $(printf '%s ' "${SECRET_DIRS[@]}")or outside the context"
}

# Images this repository BUILDS are exactly the ones the compose file tags with
# ${DOMO_IMAGE_TAG}. Reading them from the compose file rather than hardcoding
# two names means an image added by a later phase is scanned automatically.
local_image_repos() {
    grep -oE 'image:[[:space:]]*[A-Za-z0-9._-]+:\$\{DOMO_IMAGE_TAG' deploy/compose.yml 2>/dev/null \
        | sed -E 's/image:[[:space:]]*//; s/:\$\{DOMO_IMAGE_TAG.*//' \
        | sort -u
}

# Count COMPLETE PEM private-key blocks — a header followed by at least 200
# base64 characters — not bare header occurrences.
#
# This distinction is load-bearing, and was learned the hard way in plan 01-01:
# the header substring is a parser STRING LITERAL inside the distroless base's
# own libcrypto.so.3 and engines-3/loader_attic.so, so a naive header count is
# 3-4 for any image containing OpenSSL and can never reach zero. Counting
# complete blocks measures what the requirement actually means — key material
# present — and correctly reads 0 for those same images.
count_pem_key_blocks() {
    awk '
      /-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----/ { inkey = 1; acc = 0; next }
      inkey {
        if ($0 ~ /-----END [A-Z0-9 ]*PRIVATE KEY-----/) { if (acc >= 200) n++; inkey = 0; next }
        if ($0 ~ /^[A-Za-z0-9+\/=]+$/) { acc += length($0) } else { inkey = 0 }
      }
      END { print n + 0 }
    '
}

guard_image_layers() {
    if ! command -v docker >/dev/null 2>&1; then
        skip "docker is not on PATH"
        return 0
    fi

    local repos scanned=0 repo image blocks
    repos="$(local_image_repos)"
    if [ -z "$repos" ]; then
        skip "the compose file declares no locally built image"
        return 0
    fi

    for repo in $repos; do
        image="$(docker image ls --format '{{.Repository}}:{{.Tag}}' 2>/dev/null \
                 | awk -F: -v r="$repo" '$1 == r { print; exit }' || true)"
        if [ -z "$image" ]; then
            skip "image not built: ${repo}"
            continue
        fi
        blocks="$(docker save "$image" 2>/dev/null | tar -xO 2>/dev/null | count_pem_key_blocks || echo "")"
        [ -n "$blocks" ] || fail "${image}: could not scan the image layers"
        [ "$blocks" -eq 0 ] \
            || fail "${image}: ${blocks} complete PEM private-key block(s) in the image layers — key material has been baked in (PKI-06)"
        ok "${image}  0 PEM private-key blocks in its layers"
        scanned=$(( scanned + 1 ))
    done

    [ "$scanned" -gt 0 ] || skip "no locally built image present to scan"
}

guard_build_context() {
    group "build context"
    guard_dockerignore
    guard_dockerfiles
    guard_image_layers
}

# --- main --------------------------------------------------------------------
guard_staged_paths
guard_ignore_rules
guard_build_context

printf '✓ guard-secrets\n'
