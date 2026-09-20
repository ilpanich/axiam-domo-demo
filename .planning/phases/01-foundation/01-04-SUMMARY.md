---
phase: 01-foundation
plan: 04
subsystem: authz
tags: [authz, rbac, catalog, naming, pki, multi-tenant, axiam-sdk, manifest, tdd]

requires:
  - "01-01: the Cargo workspace, the offline root imported by BYOK, the organization super-admin, and the `tenant_client`/`org_client` SDK factories"
provides:
  - "`authz/catalog.toml` — eight roles and fifteen enumerated actions, applied identically to every tenant"
  - "Both demo tenants (D-19): Lakeside Residences (`lakeside`) and Summit Homes (`summit`)"
  - "A per-tenant admin principal, as its own stage, that every tenant-scoped call logs in as"
  - "`OrgClient` / `TenantClient` — distinct types, so the acting-tenant mistake is a compile error"
  - "One `mgmt@<slug>` and one `twin@<slug>` service account per tenant, each with a certificate issued by its own tenant's CA and bound to it"
  - "`naming.rs` — `resource_name`, `group_name`, `apartment_slug`, `validate_slug`, `structural_groups_for` as pure, tested functions"
  - "`stages/tree.rs` — each tenant's `portfolio` root, and the reusable create-resource-with-its-structural-groups helper"
  - "`just catalog-plan/catalog-apply/tenant-admins/service-certs/tree/authz-verify/authz`"
affects: [01-05, 01-06, 01-07, management-platform, device-twin, simulators]

actuals:
  tokens: 28257
  tasks: 3
  commits: 5
  plan_head_before: 5e23f5f68290adcfd5ab19a10cee2e048cac758d

tech-stack:
  added:
    - "toml 1.1 and rcgen 0.14 added to `domo-bootstrap` (both already pinned in the workspace; no new dependency entered the lockfile's resolution)"
    - "`domo-bootstrap` gains a library target alongside its binary"
  patterns:
    - "Scope-as-a-type: `OrgClient` and `TenantClient` are distinct newtypes, so a silent cross-scope write cannot be expressed"
    - "Declarative where the shape allows it (roles and permissions via the SDK manifest), imperative where it does not (metadata, resource-scoped bindings, service accounts — DF-011)"
    - "Verification by count, not by existence: one signing CA per tenant, one active certificate per service account, one child per name"
    - "Anti-drift tests: one test asserts `naming.rs` and `authz/catalog.toml` agree, because they are two encodings of one decision"

key-files:
  created:
    - "authz/catalog.toml — the authorization model, 250 lines, no wildcard, no deny"
    - "tools/domo-bootstrap/src/lib.rs — library target so the pure parts are testable"
    - "tools/domo-bootstrap/src/catalog.rs — TOML to management-manifest projection plus pre-flight validation"
    - "tools/domo-bootstrap/src/naming.rs — the D-17 scheme as pure functions"
    - "tools/domo-bootstrap/src/stages/catalog.rs — apply per tenant, then re-plan and prove convergence"
    - "tools/domo-bootstrap/src/stages/tenant_admin.rs — the four hand-rolled calls per tenant (DF-008)"
    - "tools/domo-bootstrap/src/stages/service_certs.rs — one account and one certificate per (service, tenant)"
    - "tools/domo-bootstrap/src/stages/tree.rs — portfolio roots and the resource-with-its-groups helper"
    - "tools/domo-bootstrap/tests/catalog.rs — 14 tests"
    - "tools/domo-bootstrap/tests/naming.rs — 12 tests"
    - "just/authz.just — catalog-plan, catalog-apply, tenant-admins, service-certs, tree, authz-verify, authz"
  modified:
    - "tools/domo-bootstrap/Cargo.toml — [lib] target, toml + rcgen"
    - "tools/domo-bootstrap/src/main.rs — tenant-admin, service-certs, tree, catalog, authz-verify subcommands"
    - "tools/domo-bootstrap/src/stages/mod.rs — OrgClient/TenantClient, fail(), public_keys_match, verify_all"
    - "tools/domo-bootstrap/src/stages/tenants.rs — both tenants; admin provisioning delegated; verify()"
    - "tools/domo-bootstrap/src/stages/pki.rs — verify()"
    - "tools/domo-bootstrap/src/stages/device_identity.rs — public_keys_match moved up to `stages`"
    - "Cargo.lock"

key-decisions:
  - "The stable domain identifier lives in `Resource.metadata` under the key `domo_id`, alongside a `kind` echo. This is the field D-17 left to research: `CreateResourceRequest` carries free-form `metadata`, and it is the only place on a resource that takes caller-supplied data."
  - "Ed25519 tenant signing CAs were KEPT. Plan 01-01 confirmed assumption A2 empirically — Erlang/OTP TLS 1.3 accepted the Ed25519-under-Ed25519-under-RSA-4096 chain — so D-11's `KeyAlgorithm::Rsa4096` fallback was not needed and `pki.rs` is unchanged in that respect."
  - "The eight role names and fifteen action names are now a cross-phase contract (see the table below). They are duplicated nowhere: `naming.rs` and the catalog are cross-checked by a test."
  - "Structural groups are NOT projected into the SDK manifest even though it has a `GroupSpec`. A manifest group-to-role binding carries no resource scope, so it would grant tenant-wide — the difference between an installer on one site and an installer everywhere."
  - "A duplicate sibling resource is refused, not resolved. With no uniqueness index on names (P-8), silently picking the first match would make an authorization decision depend on list order."
  - "The `tenants` stage still provisions tenant admins inline, delegating to the new `tenant_admin` stage, so `just tracer`'s existing sequence keeps working untouched while `just authz` can re-run the step alone."

requirements-completed: [AUTHZ-01, AUTHZ-02, PKI-02, PKI-03]

coverage:
  - id: D1
    description: "A checked-in catalog declares the eight roles of D-16, each with a description and an explicitly enumerated permission list"
    requirement: AUTHZ-02
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/catalog.rs#the_shipped_catalog_declares_the_eight_roles_of_d16"
        status: pass
    human_judgment: false
  - id: D2
    description: "No wildcard survives anywhere in the catalog, and a wildcard in an action or a grant is refused before any network call, naming the offending entry"
    requirement: AUTHZ-02
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/catalog.rs#the_shipped_catalog_contains_no_wildcard_anywhere"
        status: pass
      - kind: unit
        ref: "tools/domo-bootstrap/tests/catalog.rs#a_wildcard_permission_is_refused_and_the_error_names_it"
        status: pass
      - kind: command
        ref: "grep -v '^#' authz/catalog.toml | grep -c '[*]' => 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "A broken catalog — dangling action, duplicate role or permission, empty name, undefined template role — is refused before the first HTTP request, with the offending entry named"
    requirement: AUTHZ-02
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/catalog.rs — 6 rejection tests"
        status: pass
    human_judgment: false
  - id: D4
    description: "No staff role (property manager, concierge, installer) grants device:operate directly — the cross-role denial demo moment, asserted against the shipped catalog"
    requirement: AUTHZ-02
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/catalog.rs#no_staff_role_in_the_shipped_catalog_can_operate_a_device"
        status: pass
    human_judgment: false
  - id: D5
    description: "The D-17 naming scheme produces readable type-prefixed resource names and {role}@{type}:{slug} groups, and rejects a slug containing a dot or a colon for the two concrete downstream reasons"
    requirement: AUTHZ-01
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/naming.rs — 12 tests including both rejection reasons"
        status: pass
    human_judgment: false
  - id: D6
    description: "The eager structural group set is identical in naming.rs and authz/catalog.toml for every resource kind, so Phase 1's writer and Phase 2's reader cannot drift"
    requirement: AUTHZ-02
    verification:
      - kind: unit
        ref: "tools/domo-bootstrap/tests/naming.rs#the_group_scheme_agrees_with_the_catalog_templates"
        status: pass
    human_judgment: false
  - id: D7
    description: "Passing the organization client where a tenant client is required fails to compile, so a tenant-scoped write cannot silently land in the reserved organization tenant"
    requirement: AUTHZ-01
    verification:
      - kind: unit
        ref: "cargo test -p domo-bootstrap --doc — compile_fail doctest on stages::OrgClient"
        status: pass
    human_judgment: false
  - id: D8
    description: "Both tenants exist with exactly one signing CA each, a tenant-admin principal, a portfolio root carrying a stable identifier, and four service accounts each with one active certificate issued by its own tenant's CA"
    requirement: PKI-03
    human_judgment: true
    rationale: "Implemented and compiled, but NOT executed against live AXIAM. The parallel worktree carries no `.secrets/` (gitignored, so it is absent by construction) and running the stages from here would have had to either stand up a second stack or mutate the live one while two sibling executors were working against it — and would have written each tenant admin's generated password into a worktree the orchestrator deletes on return. Verifier must run `just authz && just authz-verify` post-merge; see Deferred Verification below for the exact commands and expected output."
  - id: D9
    description: "The catalog applies idempotently: apply then plan reports only no-change, for both tenants"
    requirement: AUTHZ-02
    human_judgment: true
    rationale: "The convergence assertion is built into the apply path itself (`stages/catalog.rs` re-plans and fails on any remaining change), and the SDK's own contract guarantees it, but the round trip has not been executed against live AXIAM for the reason in D8. Verifier must run `just catalog-apply lakeside && just catalog-plan lakeside`."
  - id: D10
    description: "A Lakeside certificate does not verify under Summit's CA, and vice versa"
    requirement: PKI-03
    human_judgment: true
    rationale: "Requires issued certificates on disk, which requires the live run. The code passes the acting tenant's own CA at every signing call and `authz-verify` asserts issuer equality by id; the openssl cross-check is the independent confirmation and is deferred."

duration: 78min
completed: 2026-09-20
status: complete
---

# Phase 1 Plan 04: Authorization Model Foundation Summary

**Eight roles and fifteen enumerated actions in a checked-in `authz/catalog.toml` applied identically to both tenants through the SDK's declarative manifest, a per-tenant admin principal that makes tenant-scoped work possible at all, one signing CA and one `mgmt@`/`twin@` credential set per tenant, each tenant's `portfolio` root, and the naming scheme all three later phases resolve through — with the acting-tenant mistake made a compile error.**

## Performance

- **Duration:** 78 min
- **Tasks:** 3
- **Commits:** 5 (two RED, three implementation)
- **Files created/modified:** 18
- **Tests:** 27 (14 catalog, 12 naming, 1 compile_fail doctest); workspace total 44, all green

## Task Commits

| Task | Phase | Commit | What |
| --- | --- | --- | --- |
| 1 | RED | `49dfee8` | 10 failing catalog tests; library target |
| 1 | GREEN | `bb1de6a` | `authz/catalog.toml`, `catalog.rs`, `stages/catalog.rs`, typed clients, `just/authz.just` |
| 2 | — | `0777644` | Both tenants, `tenant_admin`, `service_certs`, `pki::verify`, `tenants::verify` |
| 3 | RED | `6323cda` | 9 failing naming tests |
| 3 | GREEN | `4825fbb` | `naming.rs`, `stages/tree.rs`, `tree::verify` |

## The contract this plan fixes

These strings are read by Phase 2's Management Platform, Phase 3's Twin and every smoke assertion. Renaming one is a coordinated change in three phases.

### Roles

| Role | Permissions granted |
| --- | --- |
| `property-manager` | `structure:create`, `structure:read`, `structure:update`, `structure:delete`, `member:assign`, `device:read` |
| `installer` | `device:create`, `device:read`, `device:update`, `device:delete`, `device:configure` |
| `concierge` | `structure:read`, `device:read` |
| `resident` | `device:create`, `device:read`, `device:update`, `device:delete`, `device:configure`, `device:operate`, `grant:manage` |
| `granted-operator` | `device:operate` |
| `device-self` | `twin:report`, `command:receive`, `intercom:call` |
| `common-operator` | `device:read`, `device:operate` |
| `common-device-manager` | `device:create`, `device:read`, `device:update`, `device:delete`, `device:configure` |

Note what is absent: no staff role grants `device:operate`. A concierge reaches it only through membership of a `common-operator` group, which is scoped to a common area and therefore cannot reach inside an apartment. That absence *is* the cross-role denial demo moment, and a test asserts it against the shipped file rather than against a fixture.

### Actions

`structure:create`, `structure:read`, `structure:update`, `structure:delete`, `member:assign`, `device:create`, `device:read`, `device:update`, `device:delete`, `device:configure`, `device:operate`, `grant:manage`, `twin:report`, `command:receive`, `intercom:call`.

`structure:*` and `device:manage` from PROJECT.md's roles table are shorthand for a human reader; both are expanded above, and the parser refuses `*`, `?` or `%` anywhere.

### Group templates (D-21)

| Resource type | Eager groups |
| --- | --- |
| `portfolio` | `property-manager` |
| `site` | `installer`, `concierge` |
| `common` | `common-operator`, `common-device-manager` |
| `apartment` | `resident` |
| `building` | none |
| `device` | none |

The two absences are deliberate and now documented in three places that a test keeps in agreement. A building's own devices live under its `common:building` node and an installer reaches its buildings through the site binding, which cascades; a device's `granted-operator` group is created on the first grant in Phase 2 and deleted on revoke, so creating it eagerly would leave an empty group on every device — which reads as a grant that is not there.

## Answers the plan asked for

- **Which metadata field holds the stable identifier:** `Resource.metadata`, under the key `domo_id` (`tools/domo-bootstrap/src/stages/tree.rs::DOMAIN_ID_KEY`), with a `kind` echo beside it. `CreateResourceRequest` carries free-form `metadata: Option<serde_json::Value>` and `Resource` returns it as `metadata: serde_json::Value` — it is the only caller-supplied field on a resource, so D-17's open question resolves to it. The readable name stays the human-facing key; `domo_id` is what Phase 2 joins on, so a site can be renamed without breaking anything pointing at it.
- **Ed25519 or the RSA fallback:** **Ed25519 kept.** Plan 01-01 confirmed assumption A2 against the real broker — the Ed25519 leaf under an Ed25519 tenant CA under the RSA-4096 root was accepted by Erlang/OTP's TLS 1.3 — so D-11's one-field `Rsa4096` fallback was not needed and `pki.rs` is unchanged in that respect.
- **Service-account tokens rejected by REST management routes (C-5):** **carried forward as a code-read finding, NOT confirmed here.** Confirming it needs a live call with an `mgmt@` token, which is part of the deferred verification below. The consequence is already encoded where Phase 2 will read it: `stages/service_certs.rs` and `stages/tenant_admin.rs` both carry doc comments stating that these accounts serve authorization checks and device-style authentication only, and that Phase 2's Management Platform needs the per-tenant admin **user** instead.

## Deviations from Plan

### Auto-fixed issues

**1. [Rule 3 - Blocking] `tests/` cannot import from a binary crate**

- **Found during:** Task 1, writing the RED tests.
- **Issue:** The plan declares `tools/domo-bootstrap/tests/{naming,catalog}.rs`, but `domo-bootstrap` was a bin-only package. An integration test under `tests/` links against a *library* target, so neither test file could compile.
- **Fix:** Added a `[lib]` target and `src/lib.rs` exposing `catalog`, `naming` and `stages`; `main.rs` now uses `domo_bootstrap::stages`.
- **Scope note:** `Cargo.toml` and `src/lib.rs` are outside the plan's declared `files_modified`, though inside `tools/domo-bootstrap/` which this plan owns. Recorded rather than silently absorbed.
- **Commit:** `49dfee8`

**2. [Rule 3 - Blocking] Two workspace dependencies not yet enabled for this package**

- **Found during:** Task 1 (`toml`) and Task 2 (`rcgen`).
- **Issue:** The catalog parser needs `toml` and the service-account CSRs need `rcgen`. Both were already pinned in `[workspace.dependencies]` by plan 01-01 and already in `Cargo.lock` via other members, so nothing new entered dependency resolution and no package-legitimacy gate applies.
- **Fix:** Added both to `tools/domo-bootstrap/Cargo.toml`.
- **Merge hazard:** `Cargo.lock` is touched. Plan 01-05 builds in the same workspace; if it also adds a dependency, this file will conflict at merge. The conflict is textual and trivially resolved by `cargo build` after merging.
- **Commit:** `49dfee8`

**3. [Rule 2 - Missing critical] Certificate-identity check was about to be duplicated**

- **Found during:** Task 2.
- **Issue:** `service_certs.rs` needs exactly the reuse test `device_identity.rs` already had — compare a CSR's SubjectPublicKeyInfo with a certificate's — and plan 01-01 recorded that getting this wrong produces an mTLS identity whose halves disagree, failing far from its cause. Two copies of that check is one too many.
- **Fix:** Moved `public_keys_match` to `stages/mod.rs`; `device_identity.rs` imports it.
- **Scope note:** `device_identity.rs` is outside the declared `files_modified` (two lines changed).
- **Commit:** `0777644`

**4. [Rule 1 - Bug] Three SDK signatures differed from what the plan assumed**

- **Found during:** Task 3, first compile of `tree.rs`.
- **Issue:** `AssignRoleToGroupRequest` has `tenant_scope`, not `tenant_ids`; `roles().list_groups()` takes no `PageRequest`; and it returns `Vec<RoleGroupAssignment>`, not a page.
- **Fix:** Corrected all three. The last one improved the check rather than merely satisfying the compiler: `RoleGroupAssignment` carries `resource_id`, so the post-failure re-check now confirms the binding exists **and is scoped to this resource**. An existing binding at the wrong scope is a worse outcome than a failed call, and the original formulation could not have told them apart.
- **Commit:** `4825fbb`

---

**Total deviations:** 4 auto-fixed (2 blocking, 1 missing-critical, 1 bug). **Impact:** no scope change. Three of the four are consequences of the plan being written before the code existed; the fourth made a verification stricter than planned.

## Deferred Verification

**Every live assertion in this plan is unrun.** This is the one gap, and it is deliberate.

The plan's automated checks — `just catalog-apply`, `just authz-verify`, the `openssl verify` chain and cross-chain tests, the `.secrets` mode check — all need a running stack and a populated `.secrets/`. This worktree has neither: `.secrets/` is gitignored, so a worktree gets none by construction. The two ways to close that from here were both worse than deferring:

- **Stand up a second stack.** Two sibling executors (01-02 and 01-05) were running against the shared Docker host at the time, and 01-02 owns `just demo-reset`. A second `axiam-server`, `rabbitmq` and `surrealdb` would have contended for ports and several GB of RAM on a machine already at 24 GB free.
- **Run against the live stack with a copied `.secrets/`.** This would have created the `summit` tenant, its admin user and four service accounts in live AXIAM while writing each generated password into a `.secrets/` copy inside a worktree the orchestrator force-removes on return. The result would be live accounts whose credentials no longer exist anywhere — a strictly worse state than not having created them.

The project-level precondition itself was **met** and checked read-only before Task 2: `.secrets/axiam/super-admin.json` exists in the main checkout and `https://127.0.0.1:8090/health` returned `{"status":"ok"}`.

**To close the gap post-merge**, from the repository root with the stack up:

```bash
just authz          # tenant-admins → catalog (both tenants) → tree → service-certs → authz-verify
just authz-verify   # idempotence: a second run must create nothing and still pass

# D9 — catalog idempotence, the plan's own formulation
just catalog-apply lakeside && just catalog-plan lakeside | grep -qi "no.change"

# D10 — the cross-tenant negative, which is the assertion that matters
openssl verify -CAfile .secrets/pki/root.pem \
  -untrusted .secrets/axiam/lakeside-ca.pem '.secrets/axiam/service/mgmt@lakeside.pem'   # must PASS
openssl verify -CAfile .secrets/pki/root.pem \
  -untrusted .secrets/axiam/summit-ca.pem   '.secrets/axiam/service/mgmt@lakeside.pem'   # must FAIL

find .secrets/axiam -type f ! -perm 600     # must print nothing (D-13)
```

`authz-verify` prints one `✓` or `✗` per invariant and fails only at the end, so a single run reports everything that is wrong rather than the first thing. Expected lines include `✓ tenants  lakeside, summit`, `✓ signing-cas  lakeside  1`, `✓ signing-cas  organization  0 (reserved tenant, correct)`, `✓ tree  portfolio children: 0` and `✓ service-certs  mgmt@lakeside  1 active certificate, issued by its own tenant's CA`.

**Two things to watch on that first live run**, both plausible and neither knowable from here:

1. **`@` in a service-account name.** D-20 fixes the names as `mgmt@lakeside`; AXIAM may validate service-account names more narrowly than usernames. If it refuses, that is a D-20 naming question for the user, not something to work around locally.
2. **`PageRequest.search` against group and role names.** The SDK documents `search` as a free-text filter over identifying fields; `tree.rs` uses it and then re-checks the name exactly, so a server that matches more loosely costs a wasted page rather than a wrong result — but a server that matches more *strictly* (for instance not matching on `@`) would make the search return nothing and the code would then try to create a group that already exists. The conflict path resolves that correctly, so the outcome should still be right; it is worth watching the first run for repeated "creating group" lines that should have said "resolving".

## Notes for the orchestrator and later plans

- **`just tracer` still works and now does more.** The `tenants` stage creates both tenants, so `pki` mints two signing CAs on a tracer run. The tenant-admin provisioning stayed inline (delegating to the new stage) specifically so the tracer's existing sequence needed no edit — `just/stack.just` is untouched.
- **The plan's `coupling_justified` note does not apply as written.** It reasoned about plans 01-04 and 01-05 sharing one `target/` under the single root workspace. Under worktree isolation each worktree has its own `target/`, so there is no shared mutable build directory and no lock contention. The real coupling is additive **disk**: this worktree's `target/` reached 3.5 GB. Free space went 33 GB → 24 GB on `/home` across this plan and its siblings, comfortably above the 8 GB floor but worth knowing the shape of.
- **`.planning/WINDOWS.md` does not exist**, so the deferred-verification items above were not appended to the broken-windows ledger. They are recorded here and in the `coverage` block with `human_judgment: true`, which is what routes them to `/gsd-verify-work`.
- **Plan 01-06** calls `stages::tree::ensure_resource` to build the smoke branch; it is `pub` and takes `(&TenantClient, Kind, slug, parent)`. The `smoke-` prefix D-18 requires is the caller's to apply — `validate_slug` accepts it, and every naming test uses `smoke-`-prefixed slugs so the shape is exercised.

## Issues Encountered

- **The RED-evidence checker parses node-TAP, not cargo's libtest output.** `gsd_run check tdd-red-evidence` classified a faithful hand-written record as `INVALID_RED (invalid_record)` because it reads camelCase fields and then parses `output` as TAP; a cargo run yields `tests: 0` and trips `zero_tests_discovered`. Resolved by translating each captured cargo run into TAP mechanically with a small script rather than authoring the numbers — names and counts come verbatim from the run. Both RED phases then verified `RED_EVIDENCE_OK`. Worth a GSD-side note: a Rust project cannot satisfy this gate without an adapter.

## Next Phase Readiness

Ready for 01-06, with the live-verification caveat above resolved first — 01-06's smoke branch needs the portfolio roots and the catalog to actually exist in AXIAM, so `just authz` must run successfully before it.

- **01-05** (Device Twin): unaffected. This plan touched no file under `services/device-twin/` or `crates/domo-common/src/topic.rs`.
- **01-06** (smoke branch and negative assertions): `ensure_resource` is the helper it was promised, and every negative case it must invert has a positive counterpart asserted here.
- **01-07** (documentation): the role and action tables above are the authoritative list; the group-name scheme is `{role}@{type}:{slug}` with `portfolio` unslugged.
- **Phase 2**: needs a per-tenant **user** principal for management calls (C-5). The tenant-admin created here is the candidate, and its credentials are at `.secrets/axiam/tenant-admin-<slug>.json`.

---
*Phase: 01-foundation*
*Completed: 2026-09-20*
