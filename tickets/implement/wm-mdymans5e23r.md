---
schema: 1
id: wm-mdymans5e23r
title: Report embedded asset provenance in doctor
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T15:38:15Z
updated_at: 2026-11-04T16:52:21Z
closed_at: 2026-11-04T16:52:21Z
parent: wm-4125jxq7bpq8
dependencies:
  - wm-vkasp2ade4j7
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Frontend assets and provenance" decision (the `waymark doctor` sentence) and "Testing Decisions".

## What to build

`waymark doctor` reports a disagreement between the embedded frontend manifest and the build it is running in, so a stale, tampered, or mismatched Web binary is visibly unhealthy without being unstartable.

## Acceptance criteria

- [ ] `ASSETS_VERSION_MISMATCH` joins the frozen core error registry with its finding severity (unhealthy/error), non-fixable, and the details keys the finding reports; the golden snapshot and registry-consistency tests change in the same commit.
- [ ] In a Web build, `doctor` compares the embedded manifest `version` with the workspace package version and the `index.html` manifest hash with the embedded `index.html` bytes; a mismatch produces one `ASSETS_VERSION_MISMATCH` finding naming expected and actual.
- [ ] A matching manifest produces no asset finding; a basic build produces none because it embeds no manifest.
- [ ] The finding never blocks `serve` startup; it follows the existing `doctor`/`validate` exit rules, including `--strict`.
- [ ] A test seam feeds a mismatched manifest without editing embedded or on-disk asset files.
- [ ] The check lives where the manifest lives (adapter code may call it); it adds no new diagnosis machinery to core if a finding can be attached through the existing report path.

## Blocked by

"Add `waymark serve` and `waymark version` build variants" — the `web` feature, manifest accessor wiring, and CLI adapter must exist first.
