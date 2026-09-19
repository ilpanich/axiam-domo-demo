# API Coverage — Phase 1 Foundation

> Full coverage by default. Opt-outs are explicit, reasoned decisions.

Phase 1 integrates four external surfaces:

1. **AXIAM REST API** (`ghcr.io/ilpanich/axiam/server:${AXIAM_IMAGE_TAG}`) — through `axiam-sdk = "=1.0.0-beta16"`, plus three hand-rolled calls the SDK does not cover (each logged as a dogfooding finding per D-08/D-32).
2. **AXIAM gRPC API** (`:50051`).
3. **AXIAM JWKS** (`/oauth2/jwks`) — through `axiam_sdk::token::JwksVerifier`.
4. **RabbitMQ Management HTTP API** (`:15672`) and the **RabbitMQ HTTP auth-backend contract** (inbound — Phase 1 *implements* the contract in `services/device-twin`, it does not call it).

`INTEGRATE` below means "a Phase 1 task exercises this capability against the live service".
`OPT-OUT` means "deliberately not exercised in Phase 1", with the reason.

---

## AXIAM — authentication

| capability | decision | reason |
|---|---|---|
| `POST /api/v1/admin/bootstrap` (first-run org + super-admin, setup-token gate) | INTEGRATE | Hand-rolled — excluded from every SDK by CONTRACT §27.0. Logged as DF-010. |
| `POST /api/v1/auth/login` (org scope, `org_slug` + no tenant) | INTEGRATE | SDK `AxiamClient::login`. |
| `POST /api/v1/auth/login` (tenant scope, `tenant_slug`) | INTEGRATE | SDK, second client per D-37 tenant-admin. |
| `POST /api/v1/auth/device` (mTLS device login → JWT) | INTEGRATE | Hand-rolled: no SDK method exists (C-6). Logged as DF-009. |
| `POST /api/v1/auth/verify-mfa` | OPT-OUT | No MFA enrolment anywhere in the demo; the bootstrap super-admin and tenant admins are password-only. |
| `POST /api/v1/auth/refresh` | OPT-OUT | `domo-bootstrap` is a short-lived one-shot process; device tokens have no refresh path at all (DOC-05 known gap). Proactive re-auth is MQTT-04, Phase 3. |
| `POST /api/v1/auth/logout` | OPT-OUT | The CLI process exits and its cookie jar dies with it; no long-lived session to end. |
| OAuth 2.0 Device Authorization Grant (`client.device_login`, CONTRACT §14) | OPT-OUT | A different capability entirely from mTLS device login — a human-in-the-browser flow. Not needed by any of the four demo moments. |
| OPAQUE login (`axiam-sdk-wasm`) | OPT-OUT | Browser-side; AUTH-01/AUTH-02, Phase 5. Phase 1 only sets `AXIAM__AUTH__OPAQUE_SESSION_KEY` / `OPAQUE_SETUP_KEY` so Phase 5 is not blocked. |
| WebAuthn / passkeys | OPT-OUT | Not in scope for v1 (no requirement references it). |
| Federation (OIDC/SAML) | OPT-OUT | Explicitly out of scope: local, LAN-only demo. |

## AXIAM — organization, tenants, PKI

| capability | decision | reason |
|---|---|---|
| `tenants.create` | INTEGRATE | Lakeside + Summit (D-19). |
| `tenants.list` / `tenants.get` | INTEGRATE | Idempotent resolve-by-slug before create (P-8, Pattern S-4). |
| `tenants.update` | OPT-OUT | Phase 1 never renames a tenant; slugs are fixed by D-19. |
| `tenants.delete` | OPT-OUT | D-09: reset wipes volumes rather than tearing down through the API — delete carries a 409 precondition requiring `export_audit` within 6 h. |
| `ca_certificates.import_ca` (BYOK) | INTEGRATE | PKI-01. |
| `ca_certificates.set_mtls_trust_anchor` | INTEGRATE | Required for device mTLS login to accept our leaves. |
| `ca_certificates.generate_signing_ca` | INTEGRATE | PKI-02, one per tenant, `KeyAlgorithm::Ed25519` (C-3). |
| `ca_certificates.list_signing_cas` | INTEGRATE | `just verify` asserts exactly one per tenant. |
| `ca_certificates.list` / `get` | INTEGRATE | Fingerprint + custody assertions in `just verify`. |
| `ca_certificates.generate` (AXIAM-generated **org** CA) | OPT-OUT | PKI-01 mandates a single BYOK-imported root. Generating a second org CA would create a second trust anchor. |
| `ca_certificates.revoke` | OPT-OUT | Nothing is revoked in Phase 1; `just demo-reset` re-issues the whole tier below the root instead. |
| `certificates.sign_csr` | INTEGRATE | PKI-03; the private key never leaves the holder (D-23). |
| `certificates.generate` (server-side keygen) | OPT-OUT | Contradicts D-23/DEV-05 — the key would leave AXIAM over the wire. `sign_csr` is the only issuance path used. |
| `certificates.list` / `get` | INTEGRATE | `just verify` asserts `issuer_ca_id` == the acting tenant's CA. |
| `certificates.revoke` | OPT-OUT | DEV-07 (device deletion) is Phase 2. |
| `service_accounts.create` | INTEGRATE | `mgmt@{slug}`, `twin@{slug}`, and the probe device SA (D-20, D-23). |
| `service_accounts.bind_certificate` | INTEGRATE | Mandatory — without it a valid cert authenticates as nobody. |
| `service_accounts.list` / `get` | INTEGRATE | Idempotent resolve-by-name. |
| `service_accounts.delete` / `disable` | OPT-OUT | DEV-07, Phase 2. |
| `service_accounts.rotate_secret` / client-credentials secret | OPT-OUT | Every Phase 1 service account authenticates by mTLS certificate, not by a client secret. |

## AXIAM — users, roles, permissions, resources, groups

| capability | decision | reason |
|---|---|---|
| `POST /api/v1/users` (with `X-Axiam-Tenant`) | INTEGRATE | Hand-rolled tenant-admin provisioning (D-37). Logged as DF-008. |
| `PUT /api/v1/users/{id}` (`status: Active`) | INTEGRATE | Hand-rolled — users are created `PendingVerification`. |
| `users.list` (search by username) | INTEGRATE | 409-recovery path on re-run. |
| `users.delete` | OPT-OUT | Reset wipes volumes (D-09); MGMT-04/07 user lifecycle is Phase 2. |
| `roles.list` | INTEGRATE | Find the tenant's seeded `super-admin` role. |
| `POST /api/v1/roles/{id}/users` (assign role to user, tenant-global) | INTEGRATE | Hand-rolled, part of the D-37 sequence. |
| `roles.assign_to_group` (with `resource_id`) | INTEGRATE | AUTHZ-02 — the group-per-(role, resource) pattern. Imperative: the manifest cannot express a resource-scoped binding (DF-011). |
| `roles.create` / `permissions.create` | INTEGRATE | Via `management().manifest().apply()` from `authz/catalog.toml` (D-16). |
| `management().manifest().plan()` | INTEGRATE | Idempotence proof: the second plan is all `NoChange`. |
| `roles.delete` / `permissions.delete` | OPT-OUT | The manifest has no prune, on purpose; reset wipes volumes instead. |
| `resources.create` | INTEGRATE | AUTHZ-01 — portfolio roots and the smoke branch. |
| `resources.list_children` / `list_ancestors` | INTEGRATE | AUTHZ-01 verification, server-side tree walk. |
| `resources.update` / `resources.delete` | OPT-OUT | MGMT-07 (structure editing and cascade delete) is Phase 2. |
| `groups.create` | INTEGRATE | AUTHZ-02, eager structural groups (D-21). |
| `groups.add_member` / `remove_member` | INTEGRATE | The deny → allow → deny assertion in `just verify`. |
| `groups.add_service_account` | INTEGRATE | `device-self@device:…` for the probe. |
| `groups.list_roles` / `list_members` | INTEGRATE | Verification. |
| `groups.delete` | OPT-OUT | Reset wipes volumes; MGMT-07 is Phase 2. |

## AXIAM — authorization and tokens

| capability | decision | reason |
|---|---|---|
| `POST /authz/check` (REST) | INTEGRATE | Accepts service-account principals (C-5); used by `just verify`. |
| `check_access_as(subject, action, resource)` (REST) | INTEGRATE | The AUTHZ-02 group-membership proof. |
| `authz.batch_check` (REST) | OPT-OUT | Nothing in Phase 1 checks more than one decision at a time; batching is a Phase 3 scale concern. |
| gRPC `check_access` / `batch_check` | OPT-OUT | TWIN-05 is Phase 3. Port 50051 stays unpublished in Phase 1 (its TLS configuration is unresearched — RESEARCH Open Question 4). |
| gRPC `validate_token` / `introspect_token` | OPT-OUT | C-7: the SDK wraps neither (raw generated stubs only, DF-012). `JwksVerifier` is the SDK-native path and costs no network round trip per CONNECT. |
| `token::JwksVerifier` (`GET /oauth2/jwks`) | INTEGRATE | The Twin's device-JWT validation, with `expect_tenant_id` + `expect_audience("axiam:m2m")`. |
| `GET /.well-known/openid-configuration` | INTEGRATE | Proves the Caddy single origin reaches AXIAM unprefixed (PLAT-06). |
| OAuth2 client registration / token endpoint | OPT-OUT | No third-party OAuth client in the demo; devices and services use mTLS. |
| Audit export (`export_audit`) | OPT-OUT | Only needed as the precondition of `tenants.delete`, which is itself opted out. |
| GDPR / pseudonymisation endpoints | OPT-OUT | Out of scope for the demo (no real personal data). |

## RabbitMQ Management HTTP API

| capability | decision | reason |
|---|---|---|
| `GET /api/overview` | INTEGRATE | Readiness + version assertion in the `broker` stage. |
| `GET /api/vhosts` | INTEGRATE | Idempotent resolve before create. |
| `PUT /api/vhosts/domo` | INTEGRATE | D-26 — the `domo` vhost, created idempotently. |
| `DELETE /api/vhosts/{name}` | OPT-OUT | AXIAM's `/` vhost must never be touched, and `domo` is recreated by the volume wipe in `just demo-reset`. |
| `GET /api/connections` | INTEGRATE | `just smoke` asserts the probe's MQTT connection is present on the `domo` vhost. |
| `GET /api/nodes` | INTEGRATE | Asserts `rabbitmq_mqtt` and `rabbitmq_auth_backend_http` are running plugins. |
| `PUT /api/users/{name}` | OPT-OUT | Device identity is the HTTP backend's job (`auth_backends.2`); creating internal broker users would create a second, unauthenticated identity source. |
| `PUT /api/permissions/{vhost}/{user}` | OPT-OUT | Same reason — `auth_backends.1 = internal` exists solely so AXIAM's own entrypoint-created default user keeps working; no internal permission is ever added by this project. |
| `POST /api/definitions` (declarative import) | OPT-OUT | D-26: a loaded definitions file suppresses the official image's default-user creation, which AXIAM's AMQP link depends on. |
| `GET /api/exchanges`, `/api/queues`, `/api/bindings` | OPT-OUT | Nothing in Phase 1 inspects broker topology; shadow/queue behaviour is Phase 3. |
| Policies, shovels, federation, quorum-queue admin | OPT-OUT | Broker tuning is out of scope: the broker is AXIAM's, inherited as-is (D-02). |

## RabbitMQ HTTP auth-backend contract (inbound — implemented, not called)

| capability | decision | reason |
|---|---|---|
| `auth_http.user_path` → `POST /rmq/user` | INTEGRATE | MQTT-02 — JWT verify + `sub == username` + `client_id == "CN=" + username`. |
| `auth_http.vhost_path` → `POST /rmq/vhost` | INTEGRATE | Rejects any vhost other than `domo`. |
| `auth_http.resource_path` → `POST /rmq/resource` | INTEGRATE | Allows only `amq.topic` and the device's own `mqtt-subscription-<client_id>qos{0,1}` / `mqtt-will-<client_id>` names. |
| `auth_http.topic_path` → `POST /rmq/topic` | INTEGRATE | MQTT-03 groundwork — routing keys confined to `domo.<tenant_slug>.<sa-uuid>.`. |
| `allow <tags>` response form (broker tags) | OPT-OUT | No device is ever a broker administrator/monitor; the backend returns bare `allow` so no tag is ever granted. |
| `deny <reason>` response form | INTEGRATE | The reason string is what makes a denied CONNECT diagnosable in `just smoke`. |
| `auth_http.http_method = get` | OPT-OUT | GET puts the JWT in the URL, which RabbitMQ debug-logs. `post` is mandatory here (ASVS V7). |
| `rabbitmq_auth_backend_oauth2` | OPT-OUT | AXIAM's `scope` claim is not RabbitMQ permission grammar and is absent entirely on the mTLS device-login path; making it work would mean reconfiguring AXIAM's OAuth2 scope issuance — forbidden by the project's out-of-scope list. |
| `mqtt.ssl_cert_login` | OPT-OUT | Under cert login the client must not supply username/password, which is exactly the JWT-as-password flow MQTT-02 requires. `ssl_cert_client_id_from = distinguished_name` gives the cert binding without it. |

---

*Produced at plan time for Phase 1 (2026-09-19). Re-decide every row for any new integration surface; do not inherit these opt-outs silently.*
