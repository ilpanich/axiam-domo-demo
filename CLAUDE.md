# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

Phase 1 (Foundation) is built: the offline PKI, AXIAM bootstrap and tenant model, the Caddy single origin, the Device Twin's RabbitMQ authorization backend, and a device proving the mTLS → JWT → MQTT chain end to end. Phases 2–6 (Management Platform, Twin + MQTT, simulators, portals, hardening) are not started.

`DEFINITIONS.md` remains the source of truth for what the demo must do. Read it before any design or implementation work. If an implementation choice contradicts it, update the spec (with the user's agreement) rather than silently diverging.

## Build & Test

The task runner is [`just`](https://just.systems) (`justfile` at the root, modules under `just/`). `just` alone lists every recipe.

| Command | What it does |
|---|---|
| `just up` | One command from a clean machine to a running stack. Ten named stages; an interrupted run resumes at the stage that failed. Ends by printing the demo card. |
| `just preflight` | Disk, tools and versions, host-name resolution, port availability. Refuses rather than failing halfway. Runs first inside `just up`. |
| `just demo-reset` | Wipe this project's named volumes and stage markers and rebuild, keeping the organization root (D-09, D-10). Never prunes. |
| `cargo build --workspace --all-targets` | The Rust workspace: `crates/domo-common`, `services/device-twin`, `tools/domo-bootstrap`, `tools/domo-probe`. |
| `cargo test --workspace` | The whole offline test suite. `cargo test -p <crate>` for one crate while iterating. |
| `cargo clippy --workspace --all-targets -- -D warnings` | The lint gate. Warnings are errors. |
| `just verify` | **The fast gate.** Build, lint, tests, the secrets guard, the findings audit, the operator-documentation drift check, the topology invariants, then the phase's four success criteria as named groups (PKI, edge, database, authorization, the Twin's backend). Needs the stack up. |
| `just phase-verify` | **The phase gate.** `reset → verify → smoke → reset → smoke` — two consecutive cycles, because a failure only on the second points at state the first left behind. Tens of minutes. |
| `just smoke` / `just smoke-gate` | The live authorization and device-connect matrix. **`just smoke` is deliberately RED** — see below. `just smoke-gate` runs it and attributes the failures. |
| `just images` / `just images-verify` | Build both images for amd64 and arm64; assert every image in the stack resolves the Pi's architecture, down to the ELF header of the binary inside ours. |

Run `cargo test --workspace` (or `cargo test -p <crate touched>`) after code changes, and `just verify` before considering a unit of work done.

**`just smoke` is red on purpose.** Two of twelve matrix cases fail against the pinned AXIAM build — `cross-tenant-ca-issuance` (DF-017) and `other-tenant-ca` (DF-025) — and both are confirmed defects in AXIAM, not in this repository. The user decided to leave the gate red until the defect is fixed upstream *and that fix is verified here*. **Do not pin the observed behaviour, mark the cases expected-failure, exclude them from the matrix, weaken any assertion, or discard the exit status.** A negative case must never be made to pass by weakening the thing it tests. `just smoke-gate` prints the full explanation; `docs/dogfooding-upstream-status.md` lists what to re-run.

**Disk hygiene.** This machine has run out of space mid-session before, and the resulting errors look like compiler bugs. Run `df -h /home /` before any build; clean build outputs after (`cargo clean`, or at least `rm -rf target/debug/incremental`); never run two heavy builds in parallel below 25 GB free. `just images` and `just up` enforce the 8 GB floor themselves and `just _disk-guard` clears incremental output when it is tight.

**Documentation that must stay true.** `docs/setup.md` is the first-run guide for both reference machines. `.env.example` documents the five operator-set compose keys and is gate-checked against `deploy/compose.yml`'s own `operator`/`generated` guard markers — add a key there and `just verify` fails until it is documented. Every hand-rolled AXIAM call needs an entry in `docs/dogfooding-findings.md` with a `DF-` identifier cited in its doc comment; `just verify`'s findings audit fails the gate otherwise.

## What is being built

A demo showing **AXIAM** (the IAM product) handling multi-tenant IoT home automation ("domotic") properties.

### Domain hierarchy
`Tenant (one per property manager) → Site → Building → Apartment`. Devices can attach at site, building, or apartment level.

### Planned components
| Component | Language | Role |
|---|---|---|
| Management Platform | Java microservice | CRUD for sites/buildings/apartments, user assignment, device registry |
| Device Twin | Rust microservice | Shadow copy of device state and command endpoints toward devices |
| Frontend | React | UI for all user types. Use the AXIAM SDK via WASM where possible |
| Device simulators | C, C++, Rust | Simulate intercoms, lights, and thermostats (including physical behavior such as room heating), and keep state in sync with the Device Twin |

Integration rules from the spec:
- All components talk to AXIAM **through the AXIAM SDKs**, over **gRPC**, with **mTLS** where possible.
- Devices authenticate as **service accounts** over mTLS.

### Authorization model: the key constraint
The permission rules are what the demo exists to show, so enforce them through AXIAM policies, not ad-hoc checks in service code:
- Property managers administer sites, buildings, apartments and their users, and manage devices at site and building level. They **cannot operate apartment devices**.
- Installers add, edit, delete and configure devices at site and building level. They can reach apartment devices **only when an apartment's resident explicitly grants it**.
- Concierges operate site and building devices for their assigned sites.
- Local users (residents) manage devices in their own apartment. They can operate devices in their apartment and in the building and site it belongs to.
- No staff role (property manager, concierge, installer) may operate apartment devices, except an installer with a resident's grant.

### Seed data minimums (deliverable)
- At least 2 tenants.
- Each site has at least 2 buildings, and each building has 4 apartments.
- Users: at least 1 property manager and 1 installer per tenant, 1 concierge per site, and 2 residents per apartment.
- Devices:
  - Per site: 1 outdoor intercom.
  - Per building: 1 outdoor intercom and 3 lights.
  - Per apartment: 1 indoor intercom, 3 lights, and 2 thermostats.

Other deliverables: code, tests and documentation for each component, setup scripts and instructions, and deploy scripts for the simulator fleet.

## Related local repositories

The AXIAM server and its SDKs are sibling checkouts under `/home/emanuele/git/priv/`. Consult them for real APIs instead of guessing:
- `axiam/`: the AXIAM server (Rust workspace, protos, docker, and its own `CLAUDE.md`)
- `axiam-java-sdk/`: for the Management Platform
- `axiam-rust-sdk/`: for the Device Twin and Rust simulators. Contains `axiam-sdk-wasm/` for the Frontend
- `axiam-typescript-sdk/`: Frontend fallback where WASM doesn't fit
- `axiam-c-sdk/`, `axiam-cplusplus-sdk/`: for the C/C++ simulators

Each SDK has a `CONTRACT.md`, `openapi.json`, and `proto/` that define the API surface.

## Known gaps in the spec

Settle these with the user before building on them:
- The IoT devices section says "3 categories" but lists 4 (outdoor intercom, indoor intercom, lights, thermostats).
- The spec does not say how a resident grants an installer access to apartment devices (scope, expiry, revocation).
- No transport is specified between Device Twin and devices (only AXIAM calls are specified as gRPC).

## SAGE — Persistent Memory

You have persistent institutional memory via SAGE MCP.

### Boot Sequence (IMPORTANT)
1. Call `sage_inception` as your first action in every new conversation, before responding to the user
2. This loads the context stored in previous sessions, so it must run first
3. Follow the instructions returned by inception (they adapt to the user's settings)

### If SAGE MCP is not connected
Start the node: `sage-gui serve`
MCP config is in `.mcp.json` at project root. Restart your session after starting.
