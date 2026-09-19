# Phase 1: Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-19
**Phase:** 01-foundation
**Areas discussed:** Stack & networking, Bootstrap & reset tooling, AuthZ model seeding, MQTT proof shape, Repo layout, Caddy route map, Dogfooding log process, Operator UX of setup

---

## Stack & networking

**AXIAM image source**

| Option | Description | Selected |
|--------|-------------|----------|
| Released ghcr, pinned | ghcr.io/ilpanich/axiam/* pinned to AXIAM_IMAGE_TAG; already multi-arch | ✓ |
| Build from ../axiam | Always matches local source; slow or cross-built on the Pi | |
| Released + local override | ghcr by default, plus an override that builds from ../axiam | |

**Compose organization**

| Option | Description | Selected |
|--------|-------------|----------|
| Own compose, adapted | Copy AXIAM's services and adapt them (TLS, MQTT, no Vault, limits) | ✓ |
| include: AXIAM's compose | Less copying; needs the sibling checkout on the Pi, and upstream edits can break it | |

**AXIAM admin console**

| Option | Description | Selected |
|--------|-------------|----------|
| Include it behind Caddy | Evaluators see the real tenants, resources and groups; about 10 MB | ✓ |
| Skip it | Leaner | |
| XPS-only profile | Compose profile, off on the Pi | |

**Hostname / SANs**

| Option | Description | Selected |
|--------|-------------|----------|
| Configurable name + IP SANs | DOMO_HOST (default domo.local via mDNS) + LAN IP + localhost + service names | ✓ |
| /etc/hosts name only | Fixed name; every client edits its hosts file | |
| IP address only | Breaks when the DHCP address changes | |

**LAN access for the simulator PC**

| Option | Description | Selected |
|--------|-------------|----------|
| Direct native mTLS | Expose AXIAM's TLS listener (client cert optional) + MQTTS 8883; everything else internal | ✓ |
| Via Caddy + forwarded cert | X-Client-Certificate with TRUST_FORWARDED_CLIENT_CERT (legacy path) | |

**XPS vs Pi**

| Option | Description | Selected |
|--------|-------------|----------|
| One compose + per-host .env | Same images and topology on both | ✓ |
| Base + Pi override file | Explicit Pi limits in a second file | |

**TLS floor**

| Option | Description | Selected |
|--------|-------------|----------|
| TLS 1.3 only | Matches AXIAM's own standard | ✓ |
| TLS 1.2+ | More permissive | |

---

## Bootstrap & reset tooling

**Bootstrap implementation**

| Option | Description | Selected |
|--------|-------------|----------|
| Rust CLI on the Rust SDK | domo-bootstrap in the Cargo workspace; SDK for every AXIAM call; gaps logged | ✓ |
| Java CLI on the Java SDK | Shared with the Management Platform; slow JVM start on the Pi | |
| bash + curl + openssl | Fastest to write; breaks the SDK rule | |

**Reset strategy**

| Option | Description | Selected |
|--------|-------------|----------|
| Wipe volumes, re-bootstrap | Truly idempotent; avoids the tenant-delete preconditions | ✓ |
| API-level teardown | Fragile after partial runs; export_audit precondition | |

**Root lifecycle**

| Option | Description | Selected |
|--------|-------------|----------|
| Keep root; re-issue below it | No re-trust after a reset; separate pki-rotate-root | ✓ |
| New root every reset | Forces re-trusting everywhere | |

**First-run bootstrap gate**

| Option | Description | Selected |
|--------|-------------|----------|
| AXIAM_BOOTSTRAP_ADMIN_EMAIL | Email-match gate, as in AXIAM's E2E stack | |
| Scrape one-time setup token | Production-shaped; parse the token from first-boot logs | ✓ |

**User's choice:** the setup-token path. **Notes:** the scraper must be robust (wait for the log line, detect an already-consumed token, fail clearly).

**Secrets location**

| Option | Description | Selected |
|--------|-------------|----------|
| ./.secrets/ in repo, git-ignored | AXIAM's convention; easy read-only mounts | ✓ |
| Outside the repo | Can't be committed by accident; absolute paths | |

**Trust export**

| Option | Description | Selected |
|--------|-------------|----------|
| just export-trust + docs | PEM/DER in ./dist/trust/ plus per-OS/browser steps | ✓ |
| Serve root over HTTP too | One-click download on stage | |

**Server cert key type and validity**

| Option | Description | Selected |
|--------|-------------|----------|
| ECDSA P-256, ~1 year | Fast on the Pi; ~397 days | ✓ |
| RSA-2048, ~1 year | Conservative, slower | |
| You decide | | |

**Constraint surfaced:** AXIAM CAs are RSA-4096 or Ed25519 only, and browsers reject Ed25519 server chains, so the root is RSA-4096 (via BYOK import).

---

## AuthZ model seeding

**Catalog owner**

| Option | Description | Selected |
|--------|-------------|----------|
| Declarative file, applied by bootstrap | authz/catalog.toml, idempotent per tenant | ✓ |
| Management Platform at startup | Policy model hidden in Java code | |

**Naming**

| Option | Description | Selected |
|--------|-------------|----------|
| Readable + stable ID | site:{slug}, {role}@{type}:{slug}; UUID kept as the key | ✓ |
| Pure UUIDs | Opaque in the AXIAM console | |

**Phase 1 tree scope**

| Option | Description | Selected |
|--------|-------------|----------|
| Catalog + roots + smoke branch | just smoke creates one branch in tenant A; full seed in Phase 2 | ✓ |
| Full seed tree now | Two writers of the same data | |
| Catalog + roots only | Proven only by a create/delete test fixture | |

**Tenant names**

| Option | Description | Selected |
|--------|-------------|----------|
| Fictional brands | Lakeside Residences / Summit Homes | ✓ |
| Generic tenant-a / tenant-b | | |

**Service accounts**

| Option | Description | Selected |
|--------|-------------|----------|
| Per (service, tenant) accounts | Certs from each tenant's CA; strong isolation | ✓ |
| One org-scope account per service | Simpler; unclear which tenant CA issues the cert | |
| You decide after research | | |

**Notes:** supersedes PROJECT.md's org-scope Management Platform account.

**Group creation timing**

| Option | Description | Selected |
|--------|-------------|----------|
| Eager for structure, lazy for grants | Full model visible in the console | ✓ |
| Always lazy | Fewer objects; model invisible until used | |

---

## MQTT proof shape

**Auth backend home**

| Option | Description | Selected |
|--------|-------------|----------|
| Real Twin skeleton | device-twin crate with /mq-auth endpoints only; grows in Phase 3 | ✓ |
| Throwaway spike | Duplicated work | |

**Test device**

| Option | Description | Selected |
|--------|-------------|----------|
| Rust probe in the workspace | domo-probe; reused as the Phase 3 load harness | ✓ |
| C SDK + paho probe | Hardest toolchain first; not reusable | |
| Both Rust and C | Double the work | |

**Fallback if the broker can't pass both the cert identity and the JWT**

| Option | Description | Selected |
|--------|-------------|----------|
| Keep JWT, cert enforced at TLS | verify_peer against tenant CAs; username = SA id, password = JWT; sub check | ✓ |
| Keep cert identity only | ssl_cert_login; no JWT on MQTT | |
| Stop and ask me | | |

**Proof depth**

| Option | Description | Selected |
|--------|-------------|----------|
| Automated positive + negative | Mismatched cert/JWT, bad JWT, no cert, other-tenant CA | ✓ |
| Happy path only | | |

**Broker configuration**

| Option | Description | Selected |
|--------|-------------|----------|
| conf.d fragment + bootstrap API | 30-mqtt.conf; auth_backends internal then http; vhost via management API | ✓ |
| definitions.json at boot | Suppresses the default user AXIAM relies on | |

**Device key algorithm**

| Option | Description | Selected |
|--------|-------------|----------|
| Ed25519 | Matches the AXIAM-generated tenant CAs; verify RabbitMQ acceptance | ✓ |
| RSA-2048 | Most compatible fallback | |
| You decide after research | | |

---

## Repo layout

| Option | Description | Selected |
|--------|-------------|----------|
| By component kind | services/ tools/ simulators/ apps/ packages/ deploy/ authz/ docs/ | ✓ |
| By language | rust/ java/ web/ c/ cpp/ | |

| Option | Description | Selected |
|--------|-------------|----------|
| One root Cargo workspace | Shared target/ and lockfile; domo-common crate | ✓ |
| Separate crates per component | Several multi-GB target dirs | |

---

## Caddy route map

**Constraint surfaced:** the AXIAM console SPA is root-mounted and expects /api, /oauth2/ and /.well-known on its origin. The SDKs call /api/v1 unprefixed, and AXIAM session cookies would collide on a shared host.

| Option | Description | Selected |
|--------|-------------|----------|
| AXIAM unprefixed + console on own host | /api/v1, /oauth2, /.well-known → AXIAM on the portal origin; console at axiam.{DOMO_HOST} | ✓ |
| Console on a separate port | Cookies not port-isolated | |
| Keep /axiam prefix | Path rewriting AXIAM doesn't support | |

| Option | Description | Selected |
|--------|-------------|----------|
| Static demo landing page | Links, root fingerprint, trust docs | ✓ |
| Redirect to /staff/ | 404 until Phase 5 | |

---

## Dogfooding log process

| Option | Description | Selected |
|--------|-------------|----------|
| Start now, fixed format | docs/dogfooding-findings.md seeded with the 7 known gaps; checked in each phase's verification | ✓ |
| Scratch notes, write up in Phase 6 | Risk of losing reproductions | |

| Option | Description | Selected |
|--------|-------------|----------|
| Local doc only | | |
| Also draft issue texts | Ready-to-paste upstream issue body per finding; never auto-filed | ✓ |

---

## Operator UX of setup

| Option | Description | Selected |
|--------|-------------|----------|
| Staged, resumable checklist | Named idempotent stages, markers, ✓/✗ output, resume at the failed stage | ✓ |
| Always run everything, plain logs | | |

**Preflight (multi-select):** Disk space ✓, Tools & versions ✓, Host & ports ✓, Memory (Pi) ✗

| Option | Description | Selected |
|--------|-------------|----------|
| Demo card | URLs, super-admin login, root fingerprint, next steps; saved to .secrets/demo-card.txt | ✓ |
| Just 'done' | | |

---

## Claude's Discretion

- Exact ports (other than 443/8883), container names, per-service resource limits
- Stage names and marker format, the catalog schema details, rcgen vs openssl
- Landing page styling
- Health-wait timeouts and setup-token log matching
- Twin auth-backend endpoint paths and response format
- How the Twin validates device JWTs and which per-tenant credential it uses

## Deferred Ideas

- Sim-host credential bundle (Phase 4)
- Serving the root cert over HTTP (rejected for now)
- Pi RAM/swap preflight (left out; Phase 6 validates Pi memory)
- Filing findings upstream (manual, user's call)
