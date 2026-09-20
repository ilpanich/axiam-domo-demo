---
created: 2026-09-20T09:08:44.117Z
title: Non-inheritable grants must be resource-scoped
area: authz
severity: minor
files:
  - docs/dogfooding-findings.md
  - authz/catalog.toml
---

## Problem

AXIAM improvement / dogfooding finding, raised by the user during Phase 1 execution
(2026-09-20). Verbatim:

> an allow/deny grant could be set as non-inheritable but if set in this way it must be
> scoped at least to one resource

Today an allow/deny grant applies down the resource subtree it is attached to. There is
no way to say "this grant applies at *this* node and does not descend". The proposed
addition is a non-inheritable flag on a grant, paired with a validation rule: a grant
marked non-inheritable **must** name at least one resource, because a non-inheritable
grant with no resource scope would apply to nothing and is almost certainly an authoring
mistake rather than an intent.

**Why this matters to this demo specifically.** The authorization model is the thing the
demo exists to show, and its sharpest rule is a negative one: no staff role may operate
apartment devices. Property managers administer sites, buildings and apartments and
operate devices at site and building level; concierges operate site and building devices;
installers configure devices at site and building level. All three hold grants at
ancestor nodes of the apartments — so under inherit-by-default, every one of those grants
reaches apartment devices unless something stops it. Expressing the rule today means
either layering deny rules at the apartment tier or deliberately never granting at an
ancestor node and instead enumerating the grants per resource, both of which encode the
demo's headline constraint as a workaround rather than as a direct statement of intent.

A non-inheritable grant would state it directly: grant the operate permission on the site
and building nodes, non-inheritable, and apartment devices are simply out of scope by
construction. The resident's own grant on their apartment stays inheritable and continues
to cover the devices inside it.

This does not block Phase 1 — `authz/catalog.toml` can express the model without it —
which is why the severity is minor. It is a simplification, not a missing capability.

## Solution

Record as a dogfooding finding, not as demo code. Concretely:

1. Add it as a `## DF-NNN` entry in `docs/dogfooding-findings.md` in D-32's eight-field
   format with a D-33 upstream issue title and body. That file is created by
   **plan 01-02 Task 4** (wave 2), which seeds the log with at least 20 entries — this
   one belongs in that seeding pass, so it does not need a separate edit later.
2. Frame the upstream issue as two coupled changes: a `non_inheritable` (or
   `inherit = false`) flag on a grant, and a server-side validation rule rejecting a
   non-inheritable grant whose resource scope is empty.
3. When `authz/catalog.toml` is written (plan 01-04), note in a comment which rules
   would collapse if this existed, so the finding stays evidence-backed rather than
   speculative — that is the standard this log is held to.

Source: user, unprompted, during Phase 1 wave 1. Not discovered by the tracer.
