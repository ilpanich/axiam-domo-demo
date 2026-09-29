---
status: diagnosed
trigger: "G-01-3 (phase 01-foundation, UAT test 3, minor): first `just verify` after a container-recreating `just up` failed SC2 verify-pki: `✗ rabbitmq  no published TLS listener verified for rabbitmq against .secrets/pki/root.pem (tried: 192.168.144.20:8883)`; re-runs green."
created: 2026-09-29T00:00:00Z
updated: 2026-09-29T13:55:00Z
goal: find_root_cause_only
symptoms_prefilled: true
---

## Current Focus

bug_class: Mandelbug (deterministic trigger — a cert-less TLS 1.3 client against a fail_if_no_peer_cert listener — with a non-deterministic, load-dependent outcome in the client's exit status)
hypothesis: CONFIRMED H2 — see Resolution.root_cause
test: done (differential loops vs rabbitmq 8883 and vs two control listeners, idle and under CPU contention)
expecting: n/a
next_action: return ROOT CAUSE FOUND to orchestrator (goal: find_root_cause_only); fix is for /gsd-plan-phase --gaps
known_pattern_candidate: none (no knowledge-base.md exists)

reasoning_checkpoint:
  hypothesis: "verify-pki's rabbitmq row flakes because the broker requires a client certificate (verify_peer + fail_if_no_peer_cert, TLS 1.3 only). In TLS 1.3 the server rejects the empty client Certificate with a certificate_required alert (116) AFTER the client's handshake is already complete, so `openssl s_client ... </dev/null` exits 0 if it processes stdin EOF first and 1 if the alert has already arrived — a scheduling race that CPU load pushes toward failure. The chain itself always verifies."
  confirming_evidence:
    - "7/200 idle verified probes against 192.168.144.20:8883 failed; every failure's stdout shows 'Verify return code: 0 (ok)' and TLSv1.3, stderr shows 'tlsv13 alert certificate required ... SSL alert number 116'"
    - "Under host CPU contention: 265/300 and 93/100 rabbitmq probes failed, all 93 classified 'Verify return code: 0 (ok) | alert certificate required | 116'; caddy :443 control under the same load 100/100 pass"
    - "Idle controls: axiam-server :8090 200/200 and caddy :443 200/200 pass with the identical probe command — only the mTLS-required listener flakes"
    - "Broker log records a certificate_required alert for each cert-less probe (tls_handshake_1_3.erl:452, state wait_cert)"
  falsification_test: "Probes against 8883 that fail with a non-zero Verify return code, a connection refusal/reset, or no alert 116 — or failures of the same probe against caddy/axiam-server under the same load. None observed."
  fix_rationale: "(diagnose-only) The fix must make the probe verdict independent of the post-handshake certificate_required alert (judge chain verification + negotiated protocol from s_client's output, or complete mTLS with a client cert chaining to the root, or deterministically wait for and assert the expected alert 116) — not retry, because retries only lower the odds and under load the per-probe loss rate is ~90%."
  blind_spots: "The exact host load at the moment of the UAT failure is unknown (the container and its logs were removed by phase-verify's resets). OpenSSL s_client's internal select ordering was inferred from behaviour, not read from source."
  candidate_causes:
    - "code: verify-pki.sh uses s_client's exit status as the verdict, which is racy for a listener that requires a client cert (CONFIRMED)"
    - "config: rabbitmq ssl_options.fail_if_no_peer_cert=true + tlsv1.3 only — the correct, intended security posture; it is the trigger, not the defect"
    - "environment: host CPU contention after `just up` / cargo build+clippy+test widens the race window (CONFIRMED as amplifier, 1-3% idle -> ~90% loaded)"
    - "environment: broker restarting / MQTT TLS listener not yet up after container recreation (ELIMINATED)"
  and_gate: "yes — failure needs the racy exit-status check (code) AND a listener that sends a post-handshake certificate_required alert (config); load (environment) only raises the probability. Removing either the code or config condition removes the flake; the config condition is intentional, so the code is the defect."

## Symptoms

expected: `just verify` ends ✓ verify; verify-pki asserts every published TLS listener row, including rabbitmq 8883.
actual: First `just verify` after `just up` recreated containers failed SC2 verify-pki with `✗ rabbitmq  no published TLS listener verified for rabbitmq against .secrets/pki/root.pem (tried: 192.168.144.20:8883)`. Unverified probe on 8883 succeeded (port in tls_addrs); only the verified handshake failed, only for first declared name "rabbitmq". Other rabbitmq SAN names not reported failing. Container ~7 min old and healthy afterwards.
errors: `✗ rabbitmq  no published TLS listener verified for rabbitmq against .secrets/pki/root.pem (tried: 192.168.144.20:8883)`; `error: recipe verify-pki failed on line 29 with exit code 1`
reproduction: UAT test 3; seen once, right after `just up` recreated containers. Subsequent manual openssl, 3x `just verify-pki`, 1x `just verify`, and `just phase-verify` all green on SC2.
started: Discovered during UAT, 2026-09-29.

## Eliminated

- hypothesis: H1 — startup/restart race; RabbitMQ's MQTT TLS listener (or the broker) was restarting or not yet serving after `just up` recreated containers
  evidence: (a) in the failing run the unverified probe had just completed a TLS handshake on the same 8883 port, so the listener was serving a certificate; (b) the identical failure reproduces against a broker up ~1.5 h with RestartCount=0 (7/200 idle, 93/100 under load); (c) every reproduced failure shows 'Verify return code: 0 (ok)' — the listener served the correct chain — and fails only on the post-handshake alert 116; (d) `just verify` never recreates containers (`_containers-fresh` runs only from `just up`'s verify stage and `smoke-matrix`), and rabbitmq's image is upstream so it has no locally-built tag to go stale. The post-`just up` timing mattered only as a source of host load.
  timestamp: 2026-09-29T13:52:00Z

## Evidence

- timestamp: 2026-09-29
  checked: .planning/debug/knowledge-base.md
  found: does not exist
  implication: no known-pattern candidate

- timestamp: 2026-09-29
  checked: scripts/verify-pki.sh verify_live() (lines ~296-383)
  found: Unverified probe = `openssl s_client -connect ADDR -servername <first DNS SAN> </dev/null`; verified probe per SAN = same plus `-CAfile root.pem -verify_return_error -tls1_3`. No client certificate in either. Both treat s_client exit status as the verdict. `fail()` does `exit 1` on the FIRST failure, so the other rabbitmq SAN names were never tried after "rabbitmq" failed.
  implication: "only the first name failed" is not a signal — the script aborts at the first failing name. Any per-probe random failure would surface as the first name that loses the race.

- timestamp: 2026-09-29
  checked: deploy/rabbitmq/20-tls.conf, 30-mqtt.conf
  found: ssl_options.verify = verify_peer; ssl_options.fail_if_no_peer_cert = true; ssl_options.versions.1 = tlsv1.3; MQTT listener ssl 8883 uses the broker-wide ssl_options.
  implication: every verify-pki probe (no client cert) is a connection the broker will REJECT with a TLS 1.3 certificate_required alert, sent after the client's Finished — i.e. after s_client considers the handshake complete.

- timestamp: 2026-09-29
  checked: deploy/compose.yml rabbitmq healthcheck
  found: `rabbitmq-diagnostics check_running` (interval 10s) — does not check the MQTT TLS listener
  implication: healthy != 8883 serving; but in the failing run the unverified probe on 8883 had just completed a handshake, so the listener WAS up moments before the failing probe.

- timestamp: 2026-09-29
  checked: just/verify.just `verify` recipe
  found: groups run strictly sequentially (build, clippy, test, guards, SC1, then SC2 verify-pki). `verify` itself never recreates containers (SC1 only reports stale images; `_containers-fresh` is only in `just up`). rabbitmq image is the upstream rabbitmq:4.3-management-alpine (not locally built), so `_containers-fresh` has no new-tag reason to recreate it.
  implication: no container recreation happens *inside* `just verify`, so a mid-run restart of rabbitmq by the gate itself is ruled out as a mechanism.

- timestamp: 2026-09-29
  checked: docker ps / docker inspect / docker logs domo-rabbitmq-1 (read-only)
  found: current rabbitmq container created 14:13:56 local (phase-verify's last reset), RestartCount=0, json-file logging. The container that saw the original failure was removed by demo-reset, so its logs are gone. In the current log, the only certificate_required alert is from `just smoke`'s no-client-cert case at 12:15:08Z (`{tls_alert,{certificate_required,"TLS server: In state wait_cert at tls_handshake_1_3.erl:452 generated SERVER ALERT: Fatal - Certificate Required`). No verify-pki probes have hit this container.
  implication: original-failure logs unrecoverable; must reproduce the race directly. The broker does generate the certificate_required alert for cert-less TLS 1.3 clients, in state wait_cert, i.e. after receiving the client's second flight.

- timestamp: 2026-09-29T13:45:00Z
  checked: one verified probe `openssl s_client -connect 192.168.144.20:8883 -servername rabbitmq -CAfile .secrets/pki/root.pem -verify_return_error -tls1_3 </dev/null`
  found: rc=0, 'DONE', TLSv1.3, 'Verify return code: 0 (ok)'. Broker log at the same instant: `{tls_alert,{certificate_required,"TLS server: In state wait_cert at tls_handshake_1_3.erl:452 generated SERVER ALERT: Fatal - Certificate Required`.
  implication: the broker rejects EVERY verify-pki probe with alert 116, even the ones s_client reports as success; the success depends on s_client exiting on stdin EOF before it reads the alert.

- timestamp: 2026-09-29T13:46:00Z
  checked: 200x the verified probe (idle host); failures' stdout/stderr kept
  found: ok=193 bad=7. All 7: stdout 'Verify return code: 0 (ok)', 'Protocol: TLSv1.3'; stderr `ssl3_read_bytes:tlsv13 alert certificate required ... SSL alert number 116` then `SSL_shutdown:shutdown while in init`.
  implication: failure reproduced on a long-running broker; the chain verifies every time; the non-zero exit is caused solely by the post-handshake certificate_required alert.

- timestamp: 2026-09-29T13:47:00Z
  checked: 200x unverified reachability probe (`-servername rabbitmq`, no CA) and 200x IP-SAN form (no SNI, verified)
  found: unverified ok=197 bad=3; IP-SAN form ok=192 bad=8
  implication: the reachability probe that builds tls_addrs is equally racy. When IT loses, verify-pki SKIPS the rabbitmq row ("published, but no port answered a TLS handshake") and exits 0 — a false green. `just verify`'s group() discards a passing recipe's output, so that skip is invisible in the gate.

- timestamp: 2026-09-29T13:48:00Z
  checked: controls — the same verified probe 200x each against axiam-server 192.168.144.20:8090 (SNI axiam-server) and caddy 192.168.144.20:443 (SNI localhost), idle
  found: 200/200 and 200/200 pass
  implication: the flake is specific to the listener that requires a client certificate; not a generic docker-proxy / network / openssl issue.

- timestamp: 2026-09-29T13:50:00Z
  checked: 100 simulated verify-pki rabbitmq-row evaluations (1 unverified + 5 verified probes, same commands as verify_live, DOMO_HOST=domo.local), idle
  found: pass=95 FAIL=4 silently-SKIPPED=1; first failing names: localhost x3, 127.0.0.1 x1
  implication: at idle roughly 4-5% of verify-pki runs fail and ~1% silently skip the row; the failing name is whichever probe loses the race first — in UAT that happened to be "rabbitmq" (the first name). A separate 300-probe idle batch had 0 failures: the idle rate is itself variable with host activity.

- timestamp: 2026-09-29T13:51:00Z
  checked: the verified probe under brief host CPU contention (24 busy loops on 12 cores, bounded by `timeout`)
  found: 300 probes: ok=35 bad=265. Second window: rabbitmq 100 probes ok=7 bad=93, all 93 = 'Verify return code: 0 (ok) | tlsv13 alert certificate required | SSL alert number 116'; caddy :443 control in the same window ok=100 bad=0.
  implication: load is a strong amplifier: when s_client is descheduled between completing the handshake and its first select(), the alert arrives first and the probe fails. This explains why the failure appeared on the first `just verify` right after a container-recreating `just up` (busy host) and not on idle re-runs. Under heavy load the most likely outcome is actually the silent SKIP (the unverified probe loses first).

## Resolution

root_cause: "scripts/verify-pki.sh verify_live() decides both TLS reachability (line ~344) and chain verification (lines ~367-369) from `openssl s_client ... </dev/null`'s exit status, with no client certificate. RabbitMQ's 8883 listener requires one (deploy/rabbitmq/20-tls.conf: ssl_options.verify=verify_peer, fail_if_no_peer_cert=true, versions=tlsv1.3). In TLS 1.3 the broker rejects the empty client Certificate with a fatal certificate_required alert (116) only AFTER the client's handshake has completed, so s_client exits 0 if it handles stdin EOF first and 1 if the alert is already readable. The chain always verifies (Verify return code: 0 (ok)); the verdict is a scheduling race, ~1-4% per probe idle and ~90% under CPU contention. The first `just verify` after `just up` ran on a busy host, so a verified probe lost the race; idle re-runs won it. The same race in the reachability probe can make verify-pki silently SKIP the rabbitmq row and pass (false green), invisible under `just verify` because group() hides a passing recipe's output."
fix: (not in scope — goal: find_root_cause_only)
verification: (diagnosis only) reproduced 7/200 idle and 93/100 loaded, all with 'Verify return code: 0 (ok)' + alert 116; controls caddy/axiam-server 0/600 failures including 0/100 under the same load
files_changed: []
