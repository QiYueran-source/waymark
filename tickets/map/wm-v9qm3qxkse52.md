---
schema: 1
id: wm-v9qm3qxkse52
title: Investigate safe reuse of Waymark Web in DSH
category: map
status: closed
owner: frank
created_at: 2026-11-04T02:01:58Z
updated_at: 2026-11-04T11:55:21Z
closed_at: 2026-11-04T11:55:21Z
parent: wm-ztbcsgpkq248
---
## Question

What integration approaches can reuse the existing Waymark Web UI and read-only behavior inside a local DSH sidebar without weakening Host/Origin validation? Compare embedding and constrained alternatives, considering iframe policies, security, asset URLs, storage, narrow layout, workspace switching, and actual browser verification needed.

## Research context

Local research branch: `research/waymark-web-embedding`. Findings are being prepared at `docs/research/waymark-web-embedding.md` on that branch; they are not merged or pushed.

## Resolution

Source investigation on 2026-11-04 against Waymark `5a080b4`/HEAD `6a18981` and the installed DSH packages. Not a validated integration or a security approval: no plugin was installed, no server started, and no real-browser embedding test was performed.

- **Least invasive reuse:** a separate-origin iframe around the existing page is the smallest integration; the shipped DSH Browser tab is a candidate for a manual experiment. It is not proven safe or functional here.
- **The current fence is absent:** Waymark sets no CSP or `X-Frame-Options` in the inspected response code and has no browser-user authentication; loopback is not an authorization boundary against other local processes. Nothing in the current service ensures that only DSH can frame it, and nothing prevents a local page from framing or reading it.
- **The DSH Browser tab is not a least-privilege assurance:** it is disabled by default in Web profiles; its frame defaults to `allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox`; escaped popups retain an opener; a user can disable the sandbox per tab; enabling it needs a profile change. DSH's web server has no server-wide authentication or origin policy and can bind non-loopback.
- **Alternatives:** (1) separate-origin iframe, only if navigation headers, both effective CSPs, endpoint identity, privacy settings and workspace lifecycle are measured and a confidentiality boundary is established; (2) an authorized read-only host-side data adapter with a DSH-native sidebar view, reusing core read behaviour and sanitized rendering rather than the full page — more UI work and a different trust boundary, but it avoids a remotely navigable frame; (3) a DSH-origin reverse proxy — higher risk, root-absolute paths would need redesign, and it loses browser-origin separation; (4) a separate browser tab as the low-integration fallback.
- **Decision gate:** if the embedding/confidentiality boundary cannot be established, prefer the host-side adapter. Do not weaken the current Host/Origin guard to make an experiment pass. Real-browser verification remains required.

Findings: `docs/research/waymark-web-embedding.md` on local branch `research/waymark-web-embedding` (commit `4c20599`).
