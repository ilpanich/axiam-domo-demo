# Project Research Summary

**Project:** AXIAM Domo Demo — multi-tenant IoT home-automation showcase
**Domain:** IoT platform with embedded IAM (AXIAM), Docker Compose, amd64 + Raspberry Pi 5 arm64
**Researched:** 2026-09-19
**Confidence:** HIGH overall. Findings were verified against the AXIAM source, the AXIAM SDKs and vendor docs.

## Executive Summary

The architecture in PROJECT.md is feasible. The research found six places where AXIAM's real behavior differs from PROJECT.md's assumptions. All six are now resolved, either by the user or by a forced correction, and PROJECT.md has been updated to match (see "Corrections to PROJECT.md" below).

Build in strict dependency order:
1. AXIAM bootstrap and PKI (imported root, tenant signing CAs, server certs).
2. A minimal MQTT-on-RabbitMQ spike with one device.
3. Management Platform domain sync. This is the riskiest authorization-model piece because of the group indirection.
4. The Device Twin.
5. The simulators and portals.

This order proves the novel integrations before scaling to 114 devices and before UI polish.

The main risks are:
- memory pressure on the Pi (8 GB, needs NVMe/SSD boot)
- reconnect storms from un-jittered device re-authentication
- a partially completed TLS bootstrap leaving the trust state inconsistent

## Corrections to PROJECT.md (all resolved)

### 1. Spring Boot must be 4.1.x, not 3.x (forced)

The AXIAM Java SDK's Spring integration is compiled against Spring Boot 4.1.1, Spring Framework 7.0.9 and Spring Security 7.1 (`axiam-java-sdk/pom.xml`). On Boot 3.x it would fail with a classpath mismatch. **Resolution:** the Management Platform uses Spring Boot 4.1.x on Java 21.

### 2. `has_role` is UNIQUE(subject, role) → group indirection (forced)

A subject can hold a given named role at **only one resource scope** (`crates/axiam-db/src/schema.rs`, `idx_has_role_unique`; a second scope returns 409). This breaks:
- installers assigned to several sites
- concierges assigned to several sites
- an installer who receives `granted-operator` grants from two apartments

**Resolution:** create one AXIAM **group per (role, resource)** pair (e.g. `installer@site:123`, `granted-operator@device:xyz`) and assign the role to the group at that scope. Users join and leave through `member_of`, which has no such limit. Grant = add membership (creating the group if needed). Revoke = remove membership.

### 3. Device certs need `bind-certificate` (forced; dogfooding finding)

`docs/pki/README.md` says Device certs skip the bind step. However, `DeviceAuthService::authenticate_der` (`crates/axiam-pki/src/mtls.rs`) requires a `cert_bound_to` edge and fails closed without one. **Resolution:** provisioning always calls `POST /service-accounts/{id}/bind-certificate` right after `sign-csr`. The doc/code mismatch goes into the dogfooding findings.

### 4. AXIAM leaves carry no SAN/KU/EKU (user decision)

`leaf_params` in `crates/axiam-pki/src/cert.rs` emits no subjectAltName, keyUsage or extendedKeyUsage on any leaf, and `sign-csr` refuses CSRs that request them. Browsers and rustls match hostnames only against the SAN, so AXIAM-issued server certs cannot pass hostname verification.

**User decision:**
- Setup generates the root, which AXIAM then imports with its private key (BYOK, `POST /organizations/{org_id}/ca-certificates/import`).
- AXIAM issues a **tenant signing CA** (intermediate) per tenant under that root.
- Device and service client certs are **issued by AXIAM from the tenant CAs**.
- Server certs that need SANs are signed by the same imported root, offline at setup. That keeps a single trust anchor.
- The SAN/KU/EKU gap is logged in the dogfooding findings as an AXIAM improvement.

### 5. Broker auth: HTTP backend in the Twin; devices present cert + AXIAM JWT (user decision)

`rabbitmq_auth_backend_oauth2` is not viable. AXIAM's `scope` claim is free-form (`crates/axiam-auth/src/token.rs`) and not in RabbitMQ's permission grammar.

**User decision:**
1. Each device authenticates to AXIAM with the SDK's mTLS login (`POST /api/v1/auth/device`).
2. It connects to MQTT over mTLS, with the **AXIAM JWT as the MQTT password**.
3. `rabbitmq_auth_backend_http`, served by the Device Twin, validates the JWT with AXIAM, checks that its subject matches the cert identity, and authorizes topics against the tenant/device namespace.

### 6. Device tokens have no refresh token (constraint)

`/auth/device` returns only `access_token` (900 s by default). Simulator hosts must **re-authenticate each device proactively, with jitter**, well before expiry, to avoid a reconnect storm every 15 minutes. The broker checks the token only at CONNECT, so an established connection survives token expiry. Reconnects need a fresh token.

## Key Findings

### Recommended Stack

| Component | Choice | Why |
|---|---|---|
| Management Platform | Java 21, **Spring Boot 4.1.x**, jOOQ 3.19+, Flyway (Boot 4 starter) | Matches the SDK. jOOQ keeps the heap small. A bare `flyway-core` silently stops auto-configuring on Boot 4 |
| Device Twin | Rust edition 2024 (MSRV 1.88), Actix-web 4.x, sqlx 0.8, rumqttc + rustls | The AXIAM Rust SDK's guards are Actix-only. Pure-Rust TLS cross-compiles cleanly to arm64 |
| gRPC | Bundled in the Java and Rust SDKs (grpc-stub / tonic 0.14) | No separate gRPC libraries needed |
| C / C++ simulators | paho.mqtt.c / paho.mqtt.cpp via Conan or vcpkg | Same package-manager story as the AXIAM C/C++ SDKs; not libmosquitto |
| Database | **PostgreSQL 17** | Avoids an open Flyway/Boot 4 bug on PostgreSQL 18 (spring-boot#49012) |
| Broker | AXIAM's RabbitMQ 4.2.x, MQTT plugin, `domo` vhost | No extra broker. MQTT is net-new: no AXIAM or SDK precedent |
| Portals | React 19, Vite 8, pnpm 10 workspaces, TanStack Router/Query, shadcn/ui + Tailwind, vite-plugin-wasm + top-level-await | `axiam-sdk-wasm` is about 540 KB gzipped; plan for CSP `wasm-unsafe-eval` |
| Proxy | Caddy 2.11, static `tls` files, `auto_https off` | Single origin is **required**: the OPAQUE session uses httpOnly `axiam_access`/`axiam_refresh` cookies plus the `axiam_csrf` double-submit cookie |
| AXIAM stack | Existing multi-arch images (axiam-server, SurrealDB, RabbitMQ); **skip Vault** | Protects the 4 GB budget |
| Builds | buildx; cross-compile Rust with the `$BUILDPLATFORM` stage pattern (no QEMU); Java and portals are arch-independent; `just` runner | QEMU Rust builds are prohibitively slow |

### Expected Features

- **Table stakes:**
  - structure CRUD with live AXIAM sync
  - scoped role assignment
  - device registry with per-device identity
  - shadow (reported/desired/delta) for the 4 device types
  - command dispatch through CheckAccess → MQTT → convergence → SSE
  - grant/revoke with immediate effect
  - provable tenant isolation
  - decision feed with human-readable reasons
  - intercom ring/answer/unlock/timeout/missed
  - seed/reset
  - simulator control panel
  - guided demo script
- **Differentiators:**
  - the access-decision feed. No surveyed commercial intercom or access-control product exposes its authorization reasoning.
  - visible per-device mTLS identity
  - fault injection showing offline shadows
  - side-by-side browsers for grant/revoke
- **Shadow modelling:**
  - Reported-only fields: `current_temp`, `call_state`, `lock_state`.
  - Commandable fields: `power`, `mode`, `target_temperature`, `brightness`, `color`.
  - Momentary actions: `unlock`, `answer`.
- **Ring timeout:** 10–15 s, tuned for live demos.
- **Anti-features:**
  - a policy/rule editor UI, which would misrepresent AXIAM's pure RBAC-over-hierarchy model
  - OTA firmware updates
  - notification/paging infrastructure

### Architecture Approach

- **Components:** AXIAM, Management Platform, Device Twin, three simulator hosts, PostgreSQL, RabbitMQ and Caddy.
- **Patterns:**
  - Synchronous, idempotent Management Platform → AXIAM sync backed by an outbox and reconciled on `demo-reset`.
  - Group-per-(role, resource) role assignments.
  - A tenant-scoped MQTT topic tree `domo/{tenant}/{device}/…`.
  - Last Will and Testament (LWT) for offline detection.
  - SSE fan-out filtered per user.
  - Decision-feed capture at every CheckAccess.
- **Authorization verified:**
  - `parent_id` cascade and `resource:verb` action names.
  - Deny wins at any depth.
  - `structure:*` is **not** a wildcard; the permissions must be enumerated.
- **TLS reload:**
  - AXIAM hot-reloads its own server cert on SIGHUP, and its mTLS trust anchors at runtime.
  - Caddy reloads gracefully.
  - RabbitMQ, PostgreSQL and the Java/Rust listeners need a restart.

### Critical Pitfalls

1. **Missing SAN on AXIAM leaves.** Resolved by the offline-signed server certs (see correction 4). Validate in Chromium and Firefox on both machines.
2. **A deny anywhere above apartments or devices silently blocks every delegated grant beneath it.** Never use deny rules in that part of the tree.
3. **Device re-auth storms.** Use jittered, proactive re-authentication (see correction 6).
4. **MQTT on AXIAM's shared broker has no precedent.** Spike it early and load-test with AXIAM's own AMQP traffic running.
5. **Per-device broker authorization is a provisioning system.** Cert issuance, binding and broker identity must be one atomic provisioning step.
6. The authorization decision cache is **off by default** and regular access tokens carry no permissions, so revoke is immediate. That holds only if every grant-mutation path is wired correctly.
7. **Pi memory.** Measure on real hardware booted from NVMe, not only on amd64.
8. **`demo-reset` idempotency.** Test killing it mid-run and re-running.

## Implications for Roadmap

The suggested order is below. The roadmapper applies the coarse granularity setting, so phases may be merged.

1. **AXIAM bootstrap + PKI.** Imported root, tenant signing CAs, offline SAN server certs, bind-certificate verification, two-stage TLS bootstrap, and Caddy as the single origin.
2. **MQTT spike.** `domo` vhost, MQTT plugin, the Twin's HTTP auth backend, and one device connecting with cert + JWT.
3. **Management Platform.** Domain CRUD plus AXIAM sync with group indirection, and seeding.
4. **Device provisioning.** CSR → cert → bind → broker identity, in one step.
5. **Device Twin.** Shadow, command dispatch, SSE, decision feed.
6. **Simulator hosts** (C, C++, Rust), one language at a time.
7. **Portals.** Staff console, Resident app, Sim control.
8. **Demo hardening.** Guided script, reset, and validation on the real Pi.

### Research Flags

- **Phase 1:** TLS bootstrap sequence and browser trust on both machines.
- **Phase 2:** MQTT plugin behavior on RabbitMQ 4.2, QoS 2 → QoS 1 downgrade, and the retained-message store.
- **Phase 3:** group-indirection design with multiple assignments.
- **Phase 5:** cost of per-event SSE filtering (BatchCheckAccess).
- **Final phase:** full load on real Pi hardware.

## Confidence Assessment

| Area | Confidence | Notes |
|---|---|---|
| Stack | HIGH / MEDIUM | SDK pins read from source; third-party versions from 2026-dated vendor docs |
| Features | HIGH | Scope fixed by PROJECT.md and DEFINITIONS.md |
| Architecture | HIGH | Validated against AXIAM source; Twin/MQTT patterns are standard |
| Pitfalls | HIGH | Verified in AXIAM source, RabbitMQ docs and the AXIAM Pi deployment doc |

### Gaps to Address

- Group-indirection behavior at scale (multi-site installers, many grants).
- Pi memory headroom under full load (needs real hardware).
- MQTT plugin stability under mixed AXIAM and device load.
- Simulator discovery polling with devices being added and removed live.
- Whether `demo-reset` recovers from interruption.

## Sources

- **AXIAM source (HIGH):**
  - `axiam-java-sdk/pom.xml`, `axiam-rust-sdk/Cargo.toml`, and the C/C++ SDK READMEs and CONTRACT.md files
  - `axiam/crates/{core,authz,db,pki,auth,api-rest}`
  - `axiam/docs/{pki,deployment}`
- **Vendor docs (MEDIUM-HIGH):** RabbitMQ MQTT and auth-backend docs, Spring Boot 4.1 release notes, PostgreSQL docs, the browser TLS requirements of Chromium and Firefox.
- **Detail files:** `STACK.md`, `FEATURES.md`, `ARCHITECTURE.md`, `PITFALLS.md` in this directory.

---
*Research completed: 2026-09-19. SUMMARY.md written by the orchestrator (#222 self-heal): the synthesizer returned the summary inline without writing the file. The user's decisions on corrections 4 and 5 were added.*
