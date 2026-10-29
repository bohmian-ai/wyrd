# TASK-010 — Ask and saved-view interaction (deferred)

Spec: [revision 11, approved](../spec.md). Status: Superseded; deferred from this change.
Dependencies: None in this delivery. Original task remains available in Git history.
Mapped disposition: REQ-030 use case 4 and REQ-031 are deferred; no active implementation obligation.

## Outcome and Value

The caller prioritizes conventional UI and MCP App investigation inside an
existing agent harness. Neither requires Wyrd to operate an Ask service or save
views. This task creates no implementation work and blocks no active task.

## Owners, Scope, Consumers, and Prohibited Changes

Do not implement AskPanel, a Find or ask entry, an embedded conversation UI,
Ask endpoint, agent runtime, saved-view store or save/share controls under this
packet. A future approved change may define them independently. Keep the
shared renderer, catalog and BFF loading boundary owned by TASK-003 and the
MCP App interaction owned by TASK-011.

## Proof Strategy

TDD does not apply: this is a scope/dependency disposition only. Check that
TASK-011/TASK-012 and the plan have no active dependency on TASK-010, the
shipping inventory excludes AskPanel, and remaining live platform prerequisites
and per-component human acceptance have not been weakened. No runtime pass or
historical review verdict is implied.

## Authority Links

- [Approved spec](../spec.md), [task order](../plan.md), [verification](../references/verification.md).
