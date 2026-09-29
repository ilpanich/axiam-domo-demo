# Upstream status of the dogfooding findings

`docs/dogfooding-findings.md` is a record of what this demo observed against **the
build it pins**. This file is the other half: what AXIAM has since done about those
observations. The two are deliberately separate — a finding is a historical fact about
a build and does not change, while its upstream status changes with every release.

**Read this before acting on any `confirmed` entry in the findings log.** An entry can
be simultaneously true of the pinned build and already fixed upstream.

## Reconciliation — AXIAM 1.0.0-beta17 (released 2026-09-25)

Source: `axiam` CHANGELOG `[1.0.0-beta17]` and the commits behind it, read from the
sibling checkout at `887b5c3a1`. **AXIAM's changelog cites our `DF-` identifiers
directly**, so the mapping below is the upstream project's own, not our inference.

| Finding | Was | Fixed upstream as | What changed |
|---|---|---|---|
| DF-001 | reported-from-source-reading | **T22.14** (+ T22.14b console) | `cert_type: Server` carrying `subject_alt_names`, a `server_cert_allowed_names` fence, and a leaf usage profile |
| DF-005 | reported-from-source-reading | **T22.12** | the gRPC listener verifies client certificates |
| DF-013 | reported-from-source-reading | **T22.13** | service accounts admitted on the management routes |
| DF-014 | reported-from-source-reading | **T22.3** | a device's access token is bound via `cnf` to the certificate that obtained it |
| DF-017 | confirmed | **T22.1** | `prepare_leaf_issuance` now reads `ca_certificate.tenant_id`; a signing CA issues only for the tenant it signs for |
| DF-019 | confirmed | **T22.7** | `axiam-server setup-token --remint`, gated on a deployment nobody has bootstrapped |
| DF-021 | proposed-improvement | **T22.11** (+ T22.11b console) | a role assignment can stop at its resource — `inherit: false` |
| DF-027 | confirmed | **T22.4** | an unbound certificate is refused with `401`, not `403` |

**DF-025 is not in the table on purpose.** It has no fix of its own because it had no
cause of its own: a forged-common-name leaf from another tenant's CA authenticated a
device end to end *because* DF-017 let that leaf be issued at all. With T22.1 the leaf
should no longer be obtainable, which would remove DF-025's precondition rather than
its symptom. That is a reasoned expectation, **not an observation** — see below.

## Nothing here is verified against this demo yet

Every row above is read from AXIAM's source and changelog. **None of it has been
exercised by this repository**, for one blocking reason:

> The beta17 **container images are not published**.
> `ghcr.io/ilpanich/axiam/server:1.0.0-beta16` pulls; `:1.0.0-beta17` returns
> `manifest unknown`, as does `frontend:1.0.0-beta17`. The git tag exists and
> `axiam-sdk 1.0.0-beta17` is on crates.io (2026-09-25, not yanked), but the images
> were never built or pushed.

So the demo stays pinned at **beta16** — server *and* SDK together. Bumping the SDK
alone would break D-01, the property that this demo names the exact AXIAM version it
validates; a beta17 SDK against a beta16 server validates neither.

### What to re-run once the images exist

In this order, against a stack rebuilt from the new tag:

```
just build && just up
just authz && just authz-verify      # second run must create nothing
just smoke                           # the question this file exists to answer
```

Then settle these, in the findings log itself:

1. **Do `cross-tenant-ca-issuance` and `other-tenant-ca` now refuse?** If they do,
   DF-017 and DF-025 become `resolved` with the runtime evidence recorded in place,
   and the phase's deliberately-red smoke gate goes green on its own merits — not by
   being weakened. If they do **not**, that is a far more serious finding than the
   original: a fix that did not hold.
2. **DF-027** — confirm the status is now `401`.
3. **DF-014** — confirm the device token carries `cnf`, and decide whether the Twin's
   verification should assert it.
4. **DF-013** — this was plan 01-04's unconfirmed C-5. If management routes now admit
   service accounts, the per-(service, tenant) account design gains a capability it was
   written without; Phase 2 should know before it builds on the tenant-admin *user*.

### One fix is an architecture question, not a version bump

**DF-001 (T22.14) invalidates a standing project constraint.** `.claude/CLAUDE.md`
states that server certificates need SANs, *which AXIAM cannot issue*, and are
therefore signed offline by the same root at setup. That premise is now false: AXIAM
issues `Server` leaves with a SAN list and fences the names it will accept.

Acting on it would touch `scripts/gen-pki.sh`, `deploy/pki/listeners.conf`,
`deploy/compose.yml` and `docs/trust.md`, and would remove one of the demo's few
remaining hand-rolled steps — a genuine dogfooding win. It is **not** a patch to any
Phase 1 plan: it is a design change, and it belongs to an explicit decision with the
user, not to a version bump.

## Adding a release to this file

Append a new `## Reconciliation — AXIAM <version>` section above the older ones. Keep
the table shape. State plainly what was read versus what was observed; the distinction
between the two is the only thing that makes this file worth keeping.
