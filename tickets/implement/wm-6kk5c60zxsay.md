---
schema: 1
id: wm-6kk5c60zxsay
title: Validate and doctor cover the local document
category: implement
status: closed
owner: frank
created_at: 2026-11-05T09:02:46Z
updated_at: 2026-11-05T11:53:46Z
closed_at: 2026-11-05T11:53:46Z
---
## Parent

[Split shared and local Waymark configuration](wm-qkssvky32ptz)

## What to build

Problems in a collaborator's local preferences are found before a mediated write, and they stay local.

`validate` and `doctor` inspect both configuration documents. A finding that belongs to the local document is located at that document, so a person repairs the right file without confusing it with the shared configuration. Local findings never change the shared configuration revision and never affect another clone: one collaborator's broken preferences are reported to that collaborator only.

Whatever a local finding covers, it covers consistently: a malformed document, an unknown or shared project key, an invalid time zone, an invalid cache value, and an empty operator value when the key is present. The existing shared-configuration findings keep their current behaviour.

## Acceptance criteria

- [ ] `validate` reports local-document findings located at the local document, using the same registry and finding shape as every other diagnostic.
- [ ] `doctor` reports the same local findings and still treats them as diagnostics rather than business-data damage.
- [ ] An invalid time zone, an invalid cache value, an empty operator value, an unknown key, and a shared project key each produce a named local finding.
- [ ] Local findings leave the shared configuration revision unchanged and report nothing about other clones.
- [ ] An absent local document produces no findings and no warning that anything is missing.
- [ ] Core and CLI tests cover each local finding, its location, revision stability, and the absent-document case.

**Blocked by:** wm-k3netb2kb3a4 — route local scalars to the local document.

**Status:** ready-for-agent
