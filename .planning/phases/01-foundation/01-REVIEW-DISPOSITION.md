---
phase: 01
review: 01-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: open
    title: "The live check never verifies the declared name or IP, so \"TLS 1.3 verified for <name>\" is false assurance"
  - id: WR-01
    severity: warning
    disposition: open
    title: "Stack detection fails open on a compose error: a live stack is reported as \"skipped (stack down)\" and ✓ verify-pki"
  - id: WR-02
    severity: warning
    disposition: open
    title: "\"No published port\" is inferred from a command whose failure is swallowed, so a compose error on a publishing row becomes a green skip"
  - id: WR-03
    severity: warning
    disposition: open
    title: "`group()` in `just verify` now returns non-zero on a clean pass"
  - id: WR-04
    severity: warning
    disposition: open
    title: "The CPU hogs' lifetime is a fixed 600 s, not tied to the iteration count, so large runs fail spuriously"
  - id: IN-01
    severity: info
    disposition: open
    title: "The advertised `source scripts/verify-pki.sh` usage leaks `set -euo pipefail` and `cd` into the caller"
  - id: IN-02
    severity: info
    disposition: open
    title: "The negative controls are fixed to rabbitmq whatever `row` is, and `tls_ep` is not normalised"
  - id: IN-03
    severity: info
    disposition: open
    title: "The NC7 controls accept any non-zero exit, not the specific refusal"
  - id: IN-04
    severity: info
    disposition: open
    title: "The failure message names a bogus SNI for a row with no DNS SAN"
  - id: IN-05
    severity: info
    disposition: open
    title: "The device-twin and postgres TLS listeners are never live-asserted"
open: 10
total: 10
recorded: 2026-09-29T14:54:53.150Z
---

# Phase 01: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | open | - |
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| WR-03 | warning | open | - |
| WR-04 | warning | open | - |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| IN-03 | info | open | - |
| IN-04 | info | open | - |
| IN-05 | info | open | - |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
