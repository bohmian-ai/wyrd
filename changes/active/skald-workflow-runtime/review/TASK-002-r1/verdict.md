# TASK-002 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Review attempt: `changes/active/skald-workflow-runtime/review/TASK-002-r1/`

The candidate and `HEAD` remained
`e165360b1264d3628b13b02c41567c047bf96930` throughout discovery,
follow-up, validation, and final evidence collection. Reviewed source was not
modified.

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior implementation | PASS | No proposed findings; all four task-named focused tests passed independently. |
| Invariant implementation | FAIL | Proposed external-ref provenance loss and missing Prompt identities in the checked-in acceptance bundle. |
| Repository standards | FAIL | Proposed mandatory rustdoc and import/signature-shape violations. |
| Maintainer | FAIL | Proposed a duplicated, incomplete keyed-reference slot inventory. |
| System resilience | PASS | No finding; it initially concluded that the write-time lock closed dependency lifecycle races. |
| Tenancy/security domain | FAIL | Independently corroborated the external-ref provenance bypass. |
| Registry durability domain | FAIL | Proposed a preflight-to-write UID replacement race. |
| Focused follow-up | RESOLVED | Source tracing established that the write-time lock protects only the row selected in the second transaction and does not preserve the UID/body validated in preflight. |
| Structured Ponytail validation | COMPLETE | Retained six findings as `FIND-TASK-002-1` through `FIND-TASK-002-6`; no specification revision is required. |

The behavior review's empty proposal set does not override independently
validated reachable paths. The system/durability disagreement was resolved by
the required focused follow-up before validation.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| REQ-001/002/003, REQ-040, INV-014: one native Workflow model, Agent steps, and the existing Prompt binder | The checked-in bundle uses ordinary Cards and native Prompt request bodies; local hydration reaches `Workflow::from_card_with_agent_resolver`. Focused loader/client tests pass. | PASS |
| REQ-013: pure Workflow validation at local load | `wyrd-loader::validate_card` invokes the existing pure Workflow validator; the checked-in bundle mutation test proves cycle refusal. | PASS |
| REQ-013A/014: resolved validation at every boundary and only the validated graph is accepted | Local and stable-snapshot registration cases run resolved validation, but registration can validate a sibling body for an external slot and can later bind a replacement UID/body not examined by preflight. | FAIL — `FIND-TASK-002-1`, `FIND-TASK-002-4` |
| REQ-024/025: local bundle and registered-local Rust loading with exact `path`/`inline`/`ref` semantics | Normal paths pass, but a same-identity local sibling satisfies an authored external `ref`, skipping the required exact registry read and no-client refusal. | FAIL — `FIND-TASK-002-1` |
| REQ-028 and AC-002: the same checked-in bundle registers exact Workflow, Agent, and Prompt versions and relationships | The bundle registers four Cards and three Workflow-to-Agent relationships because all three Prompts are anonymous inline bodies. The separate temporary Prompt fixture is not the named acceptance bundle. | FAIL — `FIND-TASK-002-2` |
| REQ-029 and INV-005: registered loading remains pinned | For a stable registered graph, exact reads, UID checks, Active-state checks, and non-floating execution pass. Registration can nevertheless persist an unvalidated replacement graph. | FAIL — `FIND-TASK-002-4` |
| REQ-052 and INV-008: declarative built-in tool names register and local tools use the caller registry | Existing tool ownership is preserved; registration validation does not reject all nonempty tool lists. | PASS |
| INV-002/003: pure contracts remain IO-free and Cards remain declarative | IO stays in loader/client/server owners; `wyrd-spec` and Card envelopes gain no runtime dependencies. | PASS |
| AC-001/003: actual bundle loads and executes locally, and a stable registered graph executes equivalently | The focused client and real server/Postgres tests pass for the exercised identities. | PASS within TASK-002's Rust slice |
| AC-006/013: negative graph proof and credible focused/integration verification | Existing matrices are green but omit same-identity provenance collision and preflight/write UID replacement. | FAIL — `FIND-TASK-002-1`, `FIND-TASK-002-4` |
| Canonical reference-slot inventory and loader authoring contract | Keyed-form normalization hard-codes three slots and omits canonical `runs_on` and `on_failure[]` slots. | FAIL — `FIND-TASK-002-3` |
| Mandatory Rust documentation | Materially changed items omit required behavior, `# Errors`, `# Panics`, and cancellation/partial-progress documentation. | FAIL — `FIND-TASK-002-5` |
| Module-top imports and bare signature types | Changed modules contain function-scoped imports and qualified signature types prohibited by repository rules. | FAIL — `FIND-TASK-002-6` |
| Non-goals and regression boundary | No second executor/schema, WyrdState expansion, Skald registry dependency, registration-time execution/secret resolution, compatibility path, or new third-party dependency entered the diff. | PASS |

## Validated finding ledger

The decision-complete evidence, correction boundaries, and focused closure
proofs are authoritative in `findings-validation.md`.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-002-1` | REVISED | INCORRECT | Preserve `Sibling` versus external `Ref` provenance through `WorkflowGraph` and the server effective-spec body source so each slot consumes only its declared source. |
| `FIND-TASK-002-2` | CONFIRMED | MISSING | Give the three checked-in reviewer Prompts exact versioned Prompt Cards and prove their relationships in the same acceptance bundle. |
| `FIND-TASK-002-3` | CONFIRMED | VIOLATION | Delete the loader's second field-name inventory and project keyed-form recognition from the canonical reference-slot owner. |
| `FIND-TASK-002-4` | CONFIRMED | INCORRECT | Make write-time recheck lock and require the preflight UID instead of silently substituting the current Active UID at the same identity. |
| `FIND-TASK-002-5` | REVISED | VIOLATION | Complete mandatory rustdoc for all added or materially changed items without behavioral refactoring. |
| `FIND-TASK-002-6` | CONFIRMED | VIOLATION | Move imports to module scope and use imported or aliased bare types in changed signatures. |

## Follow-up decision

A focused follow-up was required because the system and durability reviews made
opposite claims about the preflight/write window. It inspected both transaction
boundaries, the Active identity lookup, deletion and partial uniqueness rules,
the write-time `FOR SHARE` query, UID binding, and the existing lifecycle-lock
test. The conflict is **RESOLVED**: deletion of preflight UID A and activation
of UID B at the same identity can complete before the write transaction; the
write recheck selects B by identity, then locks and persists B without
revalidating its body. This confirms `FIND-TASK-002-4` and narrows the fix to
the existing write-time authority.

## Verification evidence and limits

The orchestrator independently ran, with the required shared target directory:

- the exact `wyrd-loader::tests::load_explicit_workflow_bundle` test;
- the exact `wyrd-client::workflow_loader::tests::hydrate_local_workflow_graph` test;
- both exact `pg_workflow_registration` tests through the repository-managed Postgres wrapper;
- `mise run test:shared` (705 passed);
- `mise run test:skald` (337 passed);
- `mise run test:cards:integration`;
- `mise run codegen:check`;
- `mise run check:client-tier`;
- `mise run check:pyo3-scope`;
- `mise run check:registry-tx-coupling`;
- `mise run fmt`;
- `mise run lints`; and
- `git diff --check`.

All exited successfully. These green lanes establish the exercised paths and
repository baseline, but they do not cover the two newly validated collision
and inter-transaction race paths and cannot waive the static repository-rule
violations.

## Prior-finding closure

This is the first TASK-002 review. There are no prior `FIND-TASK-002-*` IDs or
remediation tasks to close.

## Verdict

**FIX_REQUIRED**

The candidate does not yet satisfy TASK-002 exactly. All six retained findings
are bounded corrections within approved Revision 11 and existing owners; none
requires a new product, public API, architecture, security, compatibility,
concurrency-semantics, resource-ownership, or persistent-data decision.

Remediation task:
`changes/active/skald-workflow-runtime/review/TASK-002-r1/TASK-002-R1-close-validated-graph-gaps.md`.
