# The authorization model: the role catalog, the resource tree and the
# per-tenant credentials the rest of the demo resolves through.
#
# Same ownership split as `stack.just` (D-34/D-35): `just` drives the host,
# `domo-bootstrap` does the AXIAM API work from inside the compose network, so
# the demo machine needs no cargo toolchain. Every recipe here reuses
# `_stage`, defined in `stack.just`.

set shell := ["bash", "-euo", "pipefail", "-c"]

# --- the catalog -------------------------------------------------------------

# Show what applying `authz/catalog.toml` to a tenant WOULD do. Issues GETs
# only — safe to point at a live tenant.
catalog-plan slug:
    @just _stage catalog --tenant {{slug}} --plan-only

# Apply `authz/catalog.toml` to a tenant, then re-plan and prove convergence.
# Fails if the second plan still wants to change anything (D-16, PLAT-05).
catalog-apply slug:
    @just _stage catalog --tenant {{slug}}

# Apply the catalog to every demo tenant. The same file, applied identically:
# the two tenants differ in their data, never in their authorization model.
catalog-apply-all:
    @echo "→ catalog"
    @for t in lakeside summit; do just catalog-apply "$t"; done
    @echo "✓ catalog"

# --- per-tenant principals and credentials ------------------------------------

# Provision each tenant's admin user. The organization super-admin cannot act
# inside a tenant through the SDK (no acting-tenant header, D-37), so these
# four hand-rolled calls per tenant are what makes every tenant-scoped stage
# below possible at all.
tenant-admins:
    @just _stage tenant-admin

# One service account per (service, tenant) — `mgmt@<slug>`, `twin@<slug>` —
# each with a keypair generated locally, a CSR signed by ITS OWN tenant's CA,
# and the mandatory bind without which the certificate authenticates as nobody.
service-certs:
    @just _stage service-certs

# --- the tree -----------------------------------------------------------------

# Each tenant's `portfolio` root plus its `property-manager@portfolio` group.
# The branches below it are built by `just smoke` (Phase 1) and by the
# Management Platform (Phase 2), never here — no data ever has two writers.
tree:
    @just _stage tree

# --- verification --------------------------------------------------------------

# Assert the invariants this phase exists to establish. Every line is a `✓` or
# a `✗`; a `✗` names the tenant and the count that disagreed.
authz-verify:
    @just _stage authz-verify

# Everything above, in dependency order. Assumes the stack is up and the
# organization is bootstrapped (`just tracer`, or `just up` + org-bootstrap).
authz: tenant-admins catalog-apply-all tree service-certs
    @just authz-verify
