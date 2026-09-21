# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

This repo is at the **specification stage**: the only content is `DEFINITIONS.md`, the source of truth for what the demo must do. There is no code, build system, or test suite yet, so there are no build/lint/test commands. When components are added, record their commands here.

Read `DEFINITIONS.md` before any design or implementation work. If an implementation choice contradicts it, update the spec (with the user's agreement) rather than silently diverging.

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
