# Registry durability and persistent-state review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

The candidate commit resolved to the same identity before and after this review.

## Reviewed boundary

I traced composite Workflow registration from external-reference preflight through resolved-graph validation, write-time dependency recheck and row locking, UID binding, canonical hashing, relationship persistence, idempotency reservation, and transaction commit. The review covered sibling and external Agent/Prompt resolution, transitive Prompt discovery, lookup failures, lifecycle replacement between preflight and write, and rollback/no-partial-write behavior.

## Authority and source coverage

| Boundary | Authority | Source and tests inspected | Result |
|---|---|---|---|
| Resolved Workflow validation during composite registration | Revision 11 REQ-013A, REQ-014, REQ-028; AC-002, AC-006; task Scenario 2 | `components/cards/resolve.rs` (`resolve_card_references`, `EffectiveSpecs::{new,load,validate_workflows}`), `wyrd-client/src/workflow_loader.rs` (`WorkflowGraph`) | FAIL: DUR-001 |
| Exact sibling/external identity binding before hash and persistence | `architecture/wyrd-design.md` reference rule 19 and Registry lifecycle; INV-005 | `resolve.rs` (`bind_card_references`), `service.rs` (`write_registration`, `persist_node`), `wyrd-sql` relationship queries | PASS absent the preflight/write replacement race in DUR-001 |
| Caller-owned tenant transaction and tenant isolation | `AGENTS.md` §§3, 9; `architecture/agent-rules.md` TenantConn rules | `service.rs` (`resolve_external`, `write_registration`), `resolve.rs`, `wyrd-sql/src/queries/cards/{register,relationships,get,delete}.rs` | PASS: callees do not commit or roll back; the service owns both transactions; all registry reads/writes use `TenantConn` and RLS |
| Atomic composite persistence/no partial graph | `architecture/wyrd-design.md` Composite transaction; task Scenario 2 | `service.rs` (`write_registration`, `persist_node`), `pg_workflow_registration.rs` no-write assertions | PASS for validation and write errors: idempotency, nodes, edges, and audit share the write transaction |
| Dependency lifecycle stability through commit | Registry relationship lock contract; task requirement that only a valid resolved graph be accepted | `recheck_active_card_refs`, `write_registration`, soft-delete and identity uniqueness migrations/tests | FAIL: DUR-001 |
| Sibling/external parity and declarative tool acceptance | REQ-014, REQ-052, AC-006 | `validate_workflows`; `registers_only_valid_explicit_workflow_graphs` | PASS for a stable registry snapshot; sibling/external invalid matrices compare codes and assert no durable rows |

## Material finding

### DUR-001 — INCORRECT: write-time recheck can bind a different graph than preflight validated

- **Violated obligation:** REQ-014 requires resolved-graph validation at composite registration, and TASK-002 requires registration to accept only a valid resolved graph with no partial durable graph. The write-time dependency authority must preserve the exact graph that supplied that validation.
- **Locations:**
  - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:388-439`
  - `crates/wyrd/wyrd-server/src/components/cards/service.rs:995-1004`
  - `crates/wyrd/wyrd-server/src/components/cards/service.rs:1067-1104`
  - `crates/wyrd/wyrd-sql/src/queries/cards/relationships.rs:17-54`
- **Evidence:** `validate_workflows` loads and validates exact external Agent/Prompt bodies in the preflight `TenantConn`, after which `resolve_external` commits that transaction. `write_registration` opens a new transaction and calls `recheck_active_card_refs`, but that query resolves only `(kind, space, name, version)` and returns whichever Active UID currently owns the identity. It does not compare that UID with the UID whose body `WorkflowGraph::validate` consumed. `write_registration` replaces `plan.external_refs` with those new results, binds them into the Workflow, hashes the bound spec, and persists its relationship edges without rerunning resolved-graph validation.
- **Reachable state path:** an unreferenced external Agent version A is valid for the submitted Workflow during preflight. Before the write transaction starts, another request soft-deletes A and re-registers the same `(kind, space, name, version)` as UID B; the partial unique index intentionally permits reuse after deletion. B can carry a different but independently valid inline Prompt whose variables or request dialect are incompatible with the Workflow. The write recheck locks B, `bind_card_references` stamps B into the Workflow, and persistence commits a graph that was never validated. The old transitive Prompt refs discovered from A may also be rechecked, but that does not validate B's different body.
- **Observable consequence:** composite registration can return success and durably store a Workflow relationship to Agent B even though the resulting Workflow/Agent/Prompt graph violates Prompt-binding or route-dialect rules that stable-snapshot registration rejects. Registered loading later refuses the stored graph, so the registry contains an accepted but unrunnable Workflow.
- **Smallest safe correction:** preserve the preflight UID as part of the write-time authority. When `recheck_active_card_refs` locks each exact identity, require its current UID to equal the UID returned by preflight; if any identity was replaced, refuse and roll back the attempt so a retry preflights the new graph. This reuses the existing preflight validation and `FOR SHARE` lifecycle lock, avoids duplicate graph validation in the write path, and still permits ordinary lifecycle changes only after registration commits. Do not merely recheck status by identity and silently substitute a new UID.
- **Focused closure proof:** add a Postgres integration case that preflights a Workflow against external Agent UID A, transitions A to deleted and establishes a different Active UID B at the same exact identity, then exercises the write-time authority and proves it refuses the stale plan with no registration operation, Workflow row, or relationship committed. A subsequent fresh registration must preflight B and either reject its incompatible graph or register it only when valid. Retain the existing lock test proving lifecycle mutation waits once the matching UID is locked.

## Passing durability properties

- `EffectiveSpecs` seeds sibling specs and discovers transitive external Prompt refs from registered Agents; stable sibling and external graphs therefore use the same `WorkflowGraph` validation.
- Missing external dependencies fail before the write transaction; write-time disappearance also fails under `recheck_active_card_refs`.
- `bind_card_references` executes before `persist_node`; canonical spec hashing and outbound relationship derivation therefore see UID-bearing durable refs rather than `Sibling` values.
- The composite write transaction owns audit append, idempotency reservation, all node rows, relationship rows, binding projection, operation completion, and commit. Any returned error rolls the transaction back.
- `resolve_card_references` and SQL helpers respect caller-owned `TenantConn`; no reviewed callee commits or rolls back its caller's transaction.

## Verification limits

- I inspected the complete base-to-candidate diff and the full surrounding registration, lifecycle, SQL locking, identity-index, WorkflowGraph, and integration-test paths.
- The task records green results for `registers_only_valid_explicit_workflow_graphs`, `fetches_and_executes_locked_workflow_graph`, `test:cards:integration`, and the repository boundary gates. I did not rerun commands in this domain pass.
- The new PG journey proves stable-snapshot sibling/external parity, exact relationship persistence, inactive dependency refusal at later load, and absence of partial writes after ordinary validation failures. It does not interleave identity replacement between preflight validation and write-time recheck, so it cannot close DUR-001.

## Overall result

**FAIL**

The normal transaction and relationship ordering is sound, but DUR-001 permits a different external graph to be durably bound than the graph that passed resolved validation.
