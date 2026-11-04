# Agent instructions

## Waymark design work

The tracker is Waymark itself; tickets live under `tickets/` and are managed by the `waymark` CLI,
not by Clove. The design map is [tickets/map/wm-y14hs95k8wct.md](tickets/map/wm-y14hs95k8wct.md).

When grilling, domain-modeling, or otherwise clarifying Waymark behavior, record every settled feature decision in that design map as soon as it is resolved. Do not defer, batch, or compress settled decisions into a few vague summary bullets: capture the concrete rules, constraints, exceptions, and examples needed to guide the functional specification and implementation tickets. Keep unresolved questions in the unresolved section; update the map inline after each confirmed round, before asking the next round. Do not update the functional spec or implementation tickets until the user asks for that synchronization or confirms the design is ready. After completing requested file changes, commit them; inspect the staged diff and use a concise commit message that describes the change.

Hand-edit work-item frontmatter only through the CLI (`waymark create`, `waymark body`, `waymark status`, …). A document written without the required `created_at`/`updated_at` keys makes the projection incomplete and breaks the read-only Web view with `INCOMPLETE_PROJECTION`.
