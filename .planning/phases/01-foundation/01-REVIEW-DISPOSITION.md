---
phase: 01
review: 01-REVIEW.md
titles: json
findings:
  - id: WR-01
    severity: warning
    disposition: open
    title: "`--live-only ROW` passes, printing `✓`, after checking zero identities when the row declares no SAN entries"
  - id: WR-02
    severity: warning
    disposition: open
    title: "NC12 can pass without ever reaching the published-ports branch it claims to test"
  - id: WR-03
    severity: warning
    disposition: open
    title: "Residual of prior WR-02: a successful compose answer with no publishers is still a green skip for a row that publishes"
  - id: IN-01
    severity: info
    disposition: open
    title: "The compose-failure hint always blames `COMPOSE_PROJECT_NAME`, and \"daemon stopped\" has changed from skip to fail without a documentation update"
  - id: IN-02
    severity: info
    disposition: open
    title: "`_declared` is a global array shared by `verify_leaf` and `verify_live`"
  - id: CR-01
    severity: critical
    disposition: open
    title: "The live check never verifies the declared name or IP, so \"TLS 1.3 verified for <name>\" is false assurance"
  - id: WR-04
    severity: warning
    disposition: open
    title: "The CPU hogs' lifetime is a fixed 600 s, not tied to the iteration count, so large runs fail spuriously"
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
unparsed: 5
recorded: 2026-09-29T16:04:44.162Z
---

# Phase 01: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| WR-03 | warning | open | - |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| CR-01 | critical | open | - (not in the current review) |
| WR-04 | warning | open | - (not in the current review) |
| IN-03 | info | open | - (not in the current review) |
| IN-04 | info | open | - (not in the current review) |
| IN-05 | info | open | - (not in the current review) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
