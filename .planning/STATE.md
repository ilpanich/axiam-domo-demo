---
gsd_state_version: "1.0"
current_phase: 01
current_phase_name: Foundation
status: executing
stopped_at: Completed 01-09-PLAN.md
last_updated: "2026-09-29T15:57:33.002Z"
last_activity: 2026-09-29
last_activity_desc: Phase 01 execution started
state_head: 7cc98d8decef0e8a729e391bd9d37a0cb3d86c14
progress:
  total_phases: 6
  completed_phases: 0
  total_plans: 9
  completed_plans: 9
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-19)

**Core value:** Every user and device action goes through AXIAM, and the four key demo moments (cross-role denial, resident→installer grant/revoke, tenant isolation, live device loop) run reliably on a single small machine.
**Current focus:** Phase 01 — Foundation

## Current Position

Phase: 01 (Foundation) — EXECUTING
Plan: 2 of 9
Status: Ready to execute
Last activity: 2026-09-29 — Phase 01 execution started

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**

- Total plans completed: 0
- Average duration: - min
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**

- Last 5 plans: none yet
- Trend: -

*Updated after each plan completion*
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 01 P07 | ~65 min (this session; tasks 1-2 on 2026-09-21) | 3 tasks | 48 files |
| Phase 01 P08 | 11min | 3 tasks | 3 files |
| Phase 01 P09 | 10min | 2 tasks | 2 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: horizontal-layer structure (Foundation → Management Platform → Device Twin+MQTT → Simulators → Portals → Hardening/E2E/Docs) per user's explicit PROJECT_MODE=standard choice.
- Roadmap: MQTT-over-mTLS with the Twin's HTTP auth backend is proven with one device in Phase 1 (Foundation), before Phase 3 scales it to 114 devices — per research build-order warning.
- Roadmap: real Raspberry Pi 5 hardware validation (memory budget, one-command start) is an explicit success criterion of the final phase (Phase 6), not assumed from amd64 testing.
- [Phase 01]: 01-07: arm64 images are cross-compiled on the amd64 host (~2m30s/image); native-on-the-Pi stays an unused fallback
- [Phase 01]: 01-07: the demo publishes RabbitMQ's management console on host port 15673 (container 15672) to coexist with the sibling AXIAM dev stack (user decision)
- [Phase 01]: 01-07: the Twin admits the broker's client_id on /rmq/vhost and /rmq/resource by name and holds it to the CN=<username> binding; deny_unknown_fields otherwise unchanged
- [Phase 01]: 01-07: just phase-verify exits non-zero by user decision until the DF-017 upstream fix (beta17 T22.1) is verified here; smoke-gate attributes the pair and flags any third failure as a regression
- [Phase 01]: 01-07: plan 01-06's live matrix is invalidated as evidence about the hardened Twin (it ran on the 01-01 tracer image 028b0a1434f6); 01-07's 10/2 is the first valid run; _containers-fresh now prevents a container outliving its image
- [Phase 01]: G-01-3 fixed by construction: TLS verdicts come from s_client -state/summary markers, never its exit status; no retries, no client cert
- [Phase 01]: While the stack is up, verify-pki fails (never skips) a published server row it cannot assert; stack_is_up keyed on listener services
- [Phase 01]: just verify surfaces skip lines from passing groups; verify-pki-stress stays outside verify/phase-verify
- [Phase 01]: 01-09: CR-01 fixed in tls_handshake_verified signature (SAN ENTRY required, builds -verify_hostname / -verify_ip); no caller can get a chain-only verdict
- [Phase 01]: 01-09: a docker compose failure ends verify-pki at the first occurrence; the script never defaults or reads COMPOSE_PROJECT_NAME itself

### Pending Todos

- [2026-09-20] [authz] Non-inheritable grants must be resource-scoped — [todo file](.planning/todos/pending/2026-09-20-non-inheritable-grants-must-be-resource-scoped.md) — Needs Record as a dogfooding finding, not as demo code. Concretely:.

### Blockers/Concerns

- Phase 3 (Device Twin + MQTT) carries the highest technical risk in this roadmap: group-indirection at scale, per-event SSE tenant filtering cost, and 114-device MQTT load alongside AXIAM's own AMQP traffic all land here. Plan it with extra scrutiny.
- Phase 6's Pi hardware validation (PLAT-03) can only be truly proven on real hardware — budget time for on-device testing, not just amd64 simulation.

## Deferred Items

Items acknowledged and deferred at milestone close, most recent first:

| Category | Item | Status | Deferred At | Milestone |
|----------|------|--------|-------------|-----------|
| *(none)* | | | | |

## Session Continuity

Last session: 2026-09-29T15:57:32.964Z
Stopped at: Completed 01-09-PLAN.md
Resume file: None
