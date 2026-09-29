---
status: complete
phase: 01-foundation
source: [01-01-SUMMARY.md, 01-02-SUMMARY.md, 01-03-SUMMARY.md, 01-04-SUMMARY.md, 01-05-SUMMARY.md, 01-06-SUMMARY.md, 01-07-SUMMARY.md]
started: 2026-09-29T11:02:37Z
updated: 2026-09-29T13:41:18Z
---

## Current Test
<!-- OVERWRITE each test - shows where we are -->

[testing complete]

## Tests

### 1. Cold Start Smoke Test
covers: cold-start injection, 01-07 D1, 01-07 D9
expected: Run `just demo-reset` (it asks for confirmation) — it wipes only this project's named volumes and stage markers, keeps the organization root, and rebuilds. Every stage prints ✓, the run ends with `✓ up` and prints the demo card. `ls -l .secrets/demo-card.txt` shows mode `-rw-------` and the card holds Portal, Console, super-admin, the root SHA256 fingerprint and the export-trust hint.
result: pass

### 2. Interrupted run resumes at the failed stage
covers: 01-07 D2
expected: `DOMO_FAIL_AT=catalog just up` stops with a failure at the catalog stage. Running `just up` again prints `resuming at: catalog` (earlier stages are skipped, not redone) and ends `✓ up`.
result: pass
source: claude-run
note: "Verified by removing .secrets/state/catalog first. On a converged stack DOMO_FAIL_AT=catalog is a silent no-op (stage skipped before the in-container gate; exit 0)."

### 3. Fast gate is green on the live stack
covers: 01-07 D8, 01-07 D7, 01-07 D6, 01-02 D10, 01-03 D12, 01-05 D14, 01-04 D2
expected: `just verify` ends with `✓ verify`. Output shows named groups for PKI, edge, database, authorization and the Twin backend, plus the secrets guard, findings audit and operator-docs drift check. verify-pki asserts 14 TLS 1.3 rows (caddy asserts, does not skip); edge-verify shows `/.well-known/openid-configuration` and `/oauth2/jwks` → 200 with issuer `https://domo.local`, and `/api/twin/healthz → 200 from the Device Twin`.
result: issue
reported: "(claude-run) First `just verify`, run right after a `just up` that recreated the containers, failed SC2 verify-pki: `✗ rabbitmq  no published TLS listener verified for rabbitmq against .secrets/pki/root.pem (tried: 192.168.144.20:8883)`, even though the no-CA probe handshake had just succeeded on that port. openssl s_client by hand verified OK with TLS 1.3; three `just verify-pki` re-runs and a second full `just verify` were green. The gate is flaky and races the broker's MQTT TLS listener coming up after a container recreate."
severity: minor
source: claude-run

### 4. Both tenants provisioned and the catalog is idempotent
covers: 01-04 D8, 01-04 D9
expected: `just authz-verify` prints only ✓ lines, including `✓ tenants  lakeside, summit`, `✓ signing-cas  lakeside  1` (and summit 1), `✓ signing-cas  organization  0 (reserved tenant, correct)`, and each service account with `1 active certificate, issued by its own tenant's CA`. Then `just catalog-apply lakeside && just catalog-plan lakeside` (and the same for summit) reports only no-change.
result: pass
source: claude-run

### 5. Tenant CAs do not cross-verify
covers: 01-04 D10
expected: `openssl verify -CAfile .secrets/pki/root.pem -untrusted .secrets/axiam/lakeside-ca.pem '.secrets/axiam/service/mgmt@lakeside.pem'` prints OK; the same command with `summit-ca.pem` as `-untrusted` FAILS (unable to get local issuer). The reverse pair (a summit cert under lakeside's CA) also fails. `find .secrets/axiam -type f ! -perm 600` prints nothing.
result: pass
source: claude-run

### 6. Smoke gate: 10 pass, exactly the 2 known upstream defects red
covers: 01-06 A1–A10
expected: `just smoke-authz` is all green. `just smoke-gate` runs the 12-case matrix: 10 pass (positive round trip, forgeries refused with named signals, no-client-cert refused as a TLS CertificateRequired before CONNACK, namespace sibling/other-tenant/adjacent denied, empty-cert-binding → 403), and exactly `cross-tenant-ca-issuance` (DF-017) and `other-tenant-ca` (DF-025) fail, attributed as known AXIAM defects, with no third failure called out.
result: pass
source: claude-run

### 7. Smoke suite is re-runnable
covers: 01-06 A11
expected: Two consecutive `just smoke` runs print case-for-case identical results; `just smoke-teardown && just smoke` reproduces the same outcome again from an emptied tenant.
result: pass
source: claude-run

### 8. Two-cycle phase gate
covers: 01-07 D3
expected: `just phase-verify` (tens of minutes) runs reset → verify → smoke → reset → smoke. Both resets ✓, the root fingerprint is the same before and after, the two smoke cycles are case-for-case identical, and it ends known-red only on DF-017/DF-025.
result: pass
source: claude-run
note: "4m02s; both resets ✓, root 63:4D:9F:…:11:61 unchanged, both cycles known-red only on DF-017/DF-025 (exit 1 by design); cycle 1 verify green straight after reset."

### 9. Every image resolves arm64
covers: 01-07 D4
expected: `just images-verify` ends ✓: every image in the stack lists linux/arm64 in its manifest, and domo-twin / domo-bootstrap binaries inside are aarch64 ELF.
result: pass
source: claude-run

### 10. Browsers trust the single origin, and trust can be removed
covers: 01-02 D8, 01-03 D13, 01-07 D10 (browser half)
expected: Following docs/trust.md on this machine (import root, confirm), opening https://domo.local (front door/portal) and the AXIAM console in Chromium/Chrome and Firefox shows a padlock with no certificate warning. Following the removal section brings the warning back.
result: pass
note: "Needed domo.local/axiam.domo.local published first (no /etc/hosts or mDNS entry on this machine)."

### 11. Same `just up` runs on the Raspberry Pi 5
covers: 01-07 D5
expected: On the Pi 5 (Raspberry Pi OS, 8 GB), `just up` completes every stage and prints the demo card; `just verify` there is green.
result: skipped
reason: "Pi not available right now"

### 12. docs/setup.md on a fresh machine reaches a demo card
covers: 01-07 D10 (setup half)
expected: Following docs/setup.md literally on a machine that has never run the stack gets to `✓ up` and a demo card with no undocumented step.
result: skipped
reason: "No fresh machine available"

### 13. Offline organization root generated once, never rotated by a reset, and byte-identical across consecutive runs
expected: Offline organization root generated once, never rotated by a reset, and byte-identical across consecutive runs
result: pass
source: automated
coverage_id: 01-01-D1

### 14. AXIAM holds exactly one mTLS trust anchor and it is the offline-generated root
expected: AXIAM holds exactly one mTLS trust anchor and it is the offline-generated root
result: pass
source: automated
coverage_id: 01-01-D2

### 15. Exactly one AXIAM-issued signing CA per demo tenant, chaining to the offline root
expected: Exactly one AXIAM-issued signing CA per demo tenant, chaining to the offline root
result: pass
source: automated
coverage_id: 01-01-D3

### 16. Device leaf signed by the tenant CA, bound to its service account, verifying root -> tenant CA -> leaf
expected: Device leaf signed by the tenant CA, bound to its service account, verifying root -> tenant CA -> leaf
result: pass
source: automated
coverage_id: 01-01-D4

### 17. Device mTLS login returns a Bearer token whose sub is the device's service-account UUID
expected: Device mTLS login returns a Bearer token whose sub is the device's service-account UUID
result: pass
source: automated
coverage_id: 01-01-D5

### 18. MQTT CONNECT accepted on the domo vhost with cert + JWT, with publish and subscribe round trip
expected: MQTT CONNECT accepted on the domo vhost with cert + JWT, with publish and subscribe round trip
result: pass
source: automated
coverage_id: 01-01-D6

### 19. The four-hop identity chain refuses a mismatched client_id, token subject, vhost, queue or topic
expected: The four-hop identity chain refuses a mismatched client_id, token subject, vhost, queue or topic
result: pass
source: automated
coverage_id: 01-01-D7

### 20. No key material in git or in any produced image layer
expected: No key material in git or in any produced image layer
result: pass
source: automated
coverage_id: 01-01-D8

### 21. AXIAM's own `/` vhost and default user survive; its AMQPS link stays up under broker-wide fail_if_no_peer_cert
expected: AXIAM's own `/` vhost and default user survive; its AMQPS link stays up under broker-wide fail_if_no_peer_cert
result: pass
source: automated
coverage_id: 01-01-D9

### 22. One command takes an empty machine to an accepted MQTT CONNECT, repeatably
expected: One command takes an empty machine to an accepted MQTT CONNECT, repeatably
result: pass
source: automated
coverage_id: 01-01-D10

### 23. Every certificate the listener table declares is asserted for algorithm, extensions, SANs, lifetime and chain
expected: Every certificate the listener table declares is asserted for algorithm, extensions, SANs, lifetime and chain
result: pass
source: automated
coverage_id: 01-02-D1

### 24. The 397-day bound is inclusive at 397 and a 398-day leaf fails naming the listener
expected: The 397-day bound is inclusive at 397 and a 398-day leaf fails naming the listener
result: pass
source: automated
coverage_id: 01-02-D2

### 25. A SAN-less server certificate is a hard failure naming the listener, never a SAN-less pass
expected: A SAN-less server certificate is a hard failure naming the listener, never a SAN-less pass
result: pass
source: automated
coverage_id: 01-02-D3

### 26. Coverage follows the listener table, so a row added without regenerating fails naming that row
expected: Coverage follows the listener table, so a row added without regenerating fails naming that row
result: pass
source: automated
coverage_id: 01-02-D4

### 27. The root exports as PEM, DER and a fingerprint file, and the DER round-trips byte-for-byte to the in-use root
expected: The root exports as PEM, DER and a fingerprint file, and the DER round-trips byte-for-byte to the in-use root
result: pass
source: automated
coverage_id: 01-02-D5

### 28. Root rotation produces a different fingerprint, and a plain `just pki` afterwards leaves the new root untouched
expected: Root rotation produces a different fingerprint, and a plain `just pki` afterwards leaves the new root untouched
result: pass
source: automated
coverage_id: 01-02-D6

### 29. Three independent guards each fail loudly on their own, and the pre-commit hook refuses a force-staged secret
expected: Three independent guards each fail loudly on their own, and the pre-commit hook refuses a force-staged secret
result: pass
source: automated
coverage_id: 01-02-D7

### 30. The dogfooding log defines every finding the demo's workarounds cite, with a ready-to-paste upstream issue
expected: The dogfooding log defines every finding the demo's workarounds cite, with a ready-to-paste upstream issue
result: pass
source: automated
coverage_id: 01-02-D9

### 31. One origin routes /api/mgmt, /api/twin and AXIAM's unprefixed paths in D-30's order, and the landing page catches the rest
expected: One origin routes /api/mgmt, /api/twin and AXIAM's unprefixed paths in D-30's order, and the landing page catches the rest
result: pass
source: automated
coverage_id: 01-03-D1

### 32. The /oauth2/* matcher does not over-capture at its boundary
expected: The /oauth2/* matcher does not over-capture at its boundary
result: pass
source: automated
coverage_id: 01-03-D2

### 33. The console host and the portal origin never return each other's content
expected: The console host and the portal origin never return each other's content
result: pass
source: automated
coverage_id: 01-03-D3

### 34. Every Caddy listener is TLS 1.3 only, with a static certificate that chains to the one offline root
expected: Every Caddy listener is TLS 1.3 only, with a static certificate that chains to the one offline root
result: pass
source: automated
coverage_id: 01-03-D4

### 35. Every upstream hop is TLS-verified by name against the offline root, with no verification skip and no forwardable client-certificate header
expected: Every upstream hop is TLS-verified by name against the offline root, with no verification skip and no forwardable client-certificate header
result: pass
source: automated
coverage_id: 01-03-D5

### 36. A configuration reload during live traffic drops no connection
expected: A configuration reload during live traffic drops no connection
result: pass
source: automated
coverage_id: 01-03-D6

### 37. The front door renders the root fingerprint, equal to dist/trust/domo-root.sha256, and never offers the certificate
expected: The front door renders the root fingerprint, equal to dist/trust/domo-root.sha256, and never offers the certificate
result: pass
source: automated
coverage_id: 01-03-D7

### 38. The front door carries the AXIAM Content-Security-Policy verbatim, including wasm-unsafe-eval, plus referrer, nosniff and frame-ancestors
expected: The front door carries the AXIAM Content-Security-Policy verbatim, including wasm-unsafe-eval, plus referrer, nosniff and frame-ancestors
result: pass
source: automated
coverage_id: 01-03-D8

### 39. PostgreSQL 17 runs with TLS 1.3 against the one root, unpublished, owning no schema
expected: PostgreSQL 17 runs with TLS 1.3 against the one root, unpublished, owning no schema
result: pass
source: automated
coverage_id: 01-03-D9

### 40. The caddy and postgres listener rows are covered by the existing PKI suite without editing it
expected: The caddy and postgres listener rows are covered by the existing PKI suite without editing it
result: pass
source: automated
coverage_id: 01-03-D10

### 41. caddy, axiam-frontend and postgres are part of the same single compose file and the same `just up`
expected: caddy, axiam-frontend and postgres are part of the same single compose file and the same `just up`
result: pass
source: automated
coverage_id: 01-03-D11

### 42. A checked-in catalog declares the eight roles of D-16, each with a description and an explicitly enumerated permission list
expected: A checked-in catalog declares the eight roles of D-16, each with a description and an explicitly enumerated permission list
result: pass
source: automated
coverage_id: 01-04-D1

### 43. A broken catalog — dangling action, duplicate role or permission, empty name, undefined template role — is refused before the first HTTP request, with the offending entry named
expected: A broken catalog — dangling action, duplicate role or permission, empty name, undefined template role — is refused before the first HTTP request, with the offending entry named
result: pass
source: automated
coverage_id: 01-04-D3

### 44. No staff role (property manager, concierge, installer) grants device:operate directly — the cross-role denial demo moment, asserted against the shipped catalog
expected: No staff role (property manager, concierge, installer) grants device:operate directly — the cross-role denial demo moment, asserted against the shipped catalog
result: pass
source: automated
coverage_id: 01-04-D4

### 45. The D-17 naming scheme produces readable type-prefixed resource names and {role}@{type}:{slug} groups, and rejects a slug containing a dot or a colon for the two concrete downstream reasons
expected: The D-17 naming scheme produces readable type-prefixed resource names and {role}@{type}:{slug} groups, and rejects a slug containing a dot or a colon for the two concrete downstream reasons
result: pass
source: automated
coverage_id: 01-04-D5

### 46. The eager structural group set is identical in naming.rs and authz/catalog.toml for every resource kind, so Phase 1's writer and Phase 2's reader cannot drift
expected: The eager structural group set is identical in naming.rs and authz/catalog.toml for every resource kind, so Phase 1's writer and Phase 2's reader cannot drift
result: pass
source: automated
coverage_id: 01-04-D6

### 47. Passing the organization client where a tenant client is required fails to compile, so a tenant-scoped write cannot silently land in the reserved organization tenant
expected: Passing the organization client where a tenant client is required fails to compile, so a tenant-scoped write cannot silently land in the reserved organization tenant
result: pass
source: automated
coverage_id: 01-04-D7

### 48. A CONNECT whose token subject differs from the supplied user name is denied, with the reason naming the mismatch and never echoing the token
expected: A CONNECT whose token subject differs from the supplied user name is denied, with the reason naming the mismatch and never echoing the token
result: pass
source: automated
coverage_id: 01-05-D1

### 49. A CONNECT whose client identifier is not the marker-prefixed user name is denied, independently of the token
expected: A CONNECT whose client identifier is not the marker-prefixed user name is denied, independently of the token
result: pass
source: automated
coverage_id: 01-05-D2

### 50. Any virtual host other than `domo` is denied at every endpoint, checked before any token or name work
expected: Any virtual host other than `domo` is denied at every endpoint, checked before any token or name work
result: pass
source: automated
coverage_id: 01-05-D3

### 51. Expired, unknown-key, malformed, wrong-audience and wrong-tenant tokens are each denied as a separate asserted case
expected: Expired, unknown-key, malformed, wrong-audience and wrong-tenant tokens are each denied as a separate asserted case
result: pass
source: automated
coverage_id: 01-05-D4

### 52. The signature algorithm is pinned before key lookup, so an unexpected algorithm is rejected without the unexpected key type being tried
expected: The signature algorithm is pinned before key lookup, so an unexpected algorithm is rejected without the unexpected key type being tried
result: pass
source: automated
coverage_id: 01-05-D5

### 53. Every endpoint answers HTTP 200 with a plain-text decision, for allow and for deny, including a request that failed to parse
expected: Every endpoint answers HTTP 200 with a plain-text decision, for allow and for deny, including a request that failed to parse
result: pass
source: automated
coverage_id: 01-05-D6

### 54. A request missing a required field, or carrying an unexpected one, is rejected by the strict form and decides deny
expected: A request missing a required field, or carrying an unexpected one, is rejected by the strict form and decides deny
result: pass
source: automated
coverage_id: 01-05-D7

### 55. The virtual-host, resource and topic endpoints deny on a cache miss and on an expired entry
expected: The virtual-host, resource and topic endpoints deny on a cache miss and on an expired entry
result: pass
source: automated
coverage_id: 01-05-D8

### 56. The resource endpoint allows only the shared topic exchange and the three queue names the broker derives from the connecting client identifier
expected: The resource endpoint allows only the shared topic exchange and the three queue names the broker derives from the connecting client identifier
result: pass
source: automated
coverage_id: 01-05-D9

### 57. A routing key is allowed only under the device's own namespace, with the boundary falling on a separator — the adjacency and cross-tenant cases a prefix comparison would pass
expected: A routing key is allowed only under the device's own namespace, with the boundary falling on a separator — the adjacency and cross-tenant cases a prefix comparison would pass
result: pass
source: automated
coverage_id: 01-05-D10

### 58. MQTT-to-routing-key translation is total: every topic either round-trips unambiguously or is refused, never silently altered
expected: MQTT-to-routing-key translation is total: every topic either round-trips unambiguously or is refused, never silently altered
result: pass
source: automated
coverage_id: 01-05-D11

### 59. A token is routed to exactly one tenant's verifier and never tried against the others; an unregistered tenant resolves to no verifier at all
expected: A token is routed to exactly one tenant's verifier and never tried against the others; an unregistered tenant resolves to no verifier at all
result: pass
source: automated
coverage_id: 01-05-D12

### 60. No handler writes a request body, a password field, a token, or a reason containing either, to a log at any level
expected: No handler writes a request body, a password field, a token, or a reason containing either, to a log at any level
result: pass
source: automated
coverage_id: 01-05-D13

## Summary

total: 60
passed: 57
issues: 1
pending: 0
skipped: 2
blocked: 0

## Notes

- Coverage blocks in 01-03, 01-04, 01-06 and 01-07 SUMMARYs use `kind: command` / `kind: manual`, which the classifier rejects (valid: unit, integration, e2e, automated_ui, manual_procedural, other). Their entries were kept as human checkpoints (fail-safe), grouped above by the single command that exercises them.
- No SUMMARY carries `commits:` / `plan_head_before:` — legacy, commit-claim reconciliation skipped (warning only).
- No UI-SPEC and no browser automation enabled: UI checkpoints 0 auto-verified, 1 queued for manual review (test 10).

## Gaps

- gap_id: G-01-3
  truth: "`just verify` is green on the live stack, including SC2 verify-pki's RabbitMQ 8883 row"
  status: failed
  reason: "User reported (delegated run): first `just verify` after `just up` failed `rabbitmq no published TLS listener verified ... (tried: 192.168.144.20:8883)`; passes on re-run. Intermittent, likely a startup race with the MQTT TLS listener."
  severity: minor
  test: 3
  artifacts: []
  missing: []
