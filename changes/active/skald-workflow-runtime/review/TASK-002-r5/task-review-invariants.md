# TASK-002 R5 invariant review

## Result

**PASS.** The cumulative candidate satisfies the original cleanup task and all
three remediation tasks under approved Revision 12. I found no remaining
`MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2d669917c03699876b3c8926f0de5ac88c578c01`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation tasks: R2, R3, and R4 in their named prior review directories

`HEAD` remained exactly the candidate before source inspection, after focused
verification, and immediately before this report was written. The repository
has no `.codegraph/` directory, so direct repository navigation was used.

## Invariant trace and changed-owner coverage

| Producer / owner | State or value followed | Consumers and failure boundary inspected | Assessment |
|---|---|---|---|
| `wyrd-loader::load`, its canonical reference visitor, and `WorkflowBodies::authored` | Authored `path`, inline, sibling, and external provenance; canonical entry path | `Workflow::from_path`, lazy external discovery, Skald body lookup, checked-in bundle tests | One loader remains. Local siblings and external refs retain distinct lookup stores; wholly local loading constructs no client. |
| `CardGraphHydrator`, `GraphTraversal`, and `WorkflowBodies` | Exact selected root, UID-bearing outbound refs, Active state, aliases, graph scope, and fetched Card bodies | Registered Workflow load, authored external closure, existing Service bundle hydration | Runtime traversal reads only Agent/Prompt Cards and no artifacts; bundle traversal retains its broader inventory behavior. Selector and response identities are asserted by the shared Cards read owner. |
| `skald_workflow::Workflow::{from_card_bodies,validate_card_bodies}` and `CardBodyResolver` | Resolved Agent/Prompt bodies, authored reference provenance, Prompt variables, route/request dialect, and tool names | Client hydration, registration preflight, native execution | One Skald lowering/validation/runtime path remains. Registration suppresses tool binding without executing or resolving secrets; execution binds through the existing registry. |
| `EffectiveSpecs` | Submitted sibling specs, tenant-read external specs, and preflight `(CardRef, CardUid)` pairs | Binding/baseline/Workflow validation and `RegistrationWriter` | Sibling and external sources remain separate. Graph-only holders allow omitted/scoped root version intent without replacing authored submissions used for hashing, replay, allocation, or persistence. |
| `RegistrationWriter` and `recheck_active_card_refs` | Preflight exact UID pairs, tenant transaction, audit events, idempotency, version allocation, and persisted relationships | Composite registration response and registered graph reads | Write-time SQL matches identity plus expected Active UID under `FOR SHARE`; refusal rolls back audit, operation bookkeeping, Cards, and relationships. Workflow creates no principal. |
| `VersionBlock` | Exact semantic-version invariant | `CardRef` Serde, Rust and TypeScript selectors, server query defense, sorting and version consumers | Custom `Deserialize` now delegates to the existing parser. Ranges, partials, and empty values cannot inhabit the exact type, while valid prerelease/build values preserve their original wire string. |
| Python `PyWorkflow` / `PyWorkflowCards` | Path and closed registered selector input | Shared client facade, runtime bridge, public stubs and journeys | Conversion stays at the language boundary; generic selector errors, Workflow-specific errors, and genuine Data validation retain their distinct owners. |
| TypeScript native `parse_workflow_selector` and public `Workflow` | Closed UID or exact named selector; recursive JSON input; canonical run snapshot | Shared `WorkflowCards::load`, public `WyrdError`, generated declarations, Node journey | Shape errors name `selector`; malformed values name `uid`, `space`, `name`, or `version` before Cards IO. No TypeScript graph, transport, validator framework, or executor was introduced. |
| Rust SDK re-export and three SDK journeys | Local/mixed loading, Native run, apply, exact/UID reload, exact relationships, after-v2 pinning, and refusal paths | Public Rust, Python, and TypeScript callers | The journeys inspect all three Workflow-to-Agent and Agent-to-Prompt pairs and preserve distinct body-source output through registered reloads. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002/003/040, INV-002/003/014: one declarative Workflow Card model using existing Agent-to-Prompt references, native Prompt envelopes, pure contracts, and the existing binder | `examples/workflows/code-review/`; `wyrd-loader`; `skald-workflow/src/bodies.rs`; `Workflow::from_card_bodies` | Loader/client focused tests and the three SDK journeys recorded by the task; inspected source preserves the one contract and binder | PASS |
| REQ-013/013A/014, AC-006/013: pure and resolved validation runs at the earliest complete boundary and refuses before dispatch/write | `WorkflowBodies::hydrate`; `EffectiveSpecs::validate_workflows`; `Workflow::validate_card_bodies` | Client bundle negatives, `registers_only_valid_explicit_workflow_graphs`, and SDK refusal paths | PASS |
| REQ-024/025/054/055, AC-001: authored loading uses existing loader semantics, is registry-free when local, lazily reads exact external refs, and runs through one Skald runtime in all three SDKs | `wyrd-client/src/workflow.rs`; Python and TypeScript wrappers; Rust re-export | Rust/Python/TypeScript workflow journeys plus `workflow::tests::from_path_uses_existing_loader` | PASS |
| REQ-028/056/059, AC-002/030: registration remains declarative and atomic; provenance, Active state, exact relationships, and preflight UID replacement refusal are preserved | `EffectiveSpecs`; `RegistrationWriter`; `recheck_active_card_refs`; relationship persistence | Three server Workflow registration scenarios and SQL lifecycle-race proof recorded for this candidate | PASS |
| REQ-052, INV-008: registration does not bind tools, while execution preserves each Agent's declared tools through the existing runtime registry | `Workflow::validate_card_bodies` clears only transient validation copies; `from_card_bodies` binds through `ToolResolver` | Registration negatives and successful credential-free Native journeys | PASS |
| REQ-054/056, INV-005/007, AC-003/029: public registered selectors are exact or UID-based, loaded closures stay pinned after later versions, and all SDKs project the same run/error contract | `WorkflowCards::load`; `CardGraphHydrator::load_workflow`; Python/TS projections | All three language journeys inspect spec refs and server-derived relationships and rerun after v2 | PASS |
| R2 FIND-5/-6/-8/-9/-10: loading docs/imports, TypeScript JSON/run shape, struct ownership, and root version intent | Item-local rustdoc; recursive `JsonValue` and canonical snapshot types; hydrator/`EffectiveSpecs`/`RegistrationWriter`; graph-only preflight holders | Codegen, type, N-API, lint, server registration, and journey evidence recorded in R2/R3 | PASS |
| R3 FIND-7/-12/-13/-14/-15: complete relationship proof, generic Python selector ownership, blocking filesystem strategy, generated Hakari state, and cumulative patch hygiene | SDK journey assertions; Python `invalid_selector`; `spawn_blocking(load_bundle)`; generated workspace-hack manifest | Candidate-bound prior evidence; independent `git diff --check` passed; source inspection found no reopened state | PASS |
| R4 FIND-11: malformed Workflow selector values refuse locally with Workflow-owned code and field detail; exact-version invalid state is unrepresentable through Serde | `wyrd-semver/src/block.rs:153-166`; `sdks/wyrd-sdk-ts/native/src/workflow.rs:53-106`; updated server and TS journey assertions | Independently ran the exact `wyrd-semver` test (1 passed) and exact public TypeScript journey (1 passed) | PASS |
| REQ-057/059, AC-031 and task non-goals: no Workflow principal/root state, public loader/hydrator, second parser/graph/validator/executor, registry cache, compatibility alias, registration execution, secret resolution, or TASK-003/004/005 behavior | Cumulative source/diff audit; obsolete-name search; ownership trace above | Static inspection and boundary/codegen evidence | PASS |
| Human standing direction: no unsupported bespoke mechanism, check, file, setting, or option | The correction uses ordinary validated-newtype Serde, existing domain constructors, the existing Cards/Skald/PostgreSQL owners, a normal direct workspace dependency, and existing language tests. The per-test timeout follows neighboring server-backed Vitest tests. | Source comparison with existing repository mechanisms; no new validator framework, gate, scanner, allowlist, feature flag, compatibility path, or harness | PASS |

## Prior-finding closure

- `FIND-TASK-002-1` through `FIND-TASK-002-10` and
  `FIND-TASK-002-12` through `FIND-TASK-002-15` remain closed at the owners and
  proofs recorded by R2 through R4.
- `FIND-TASK-002-3` remains superseded by Revision 12; keyed normalization and
  its obsolete machinery remain removed.
- `FIND-TASK-002-11` is now closed. `VersionBlock` has one validated
  deserialization path, and TypeScript's boundary reports the actual malformed
  selector field before registry IO.

## Proposed findings

None.

I specifically attempted to falsify provenance separation, exact UID pinning,
Active-state closure, transaction rollback, legal root version intent,
cross-language selector equivalence, async filesystem isolation, and removal of
parallel loading machinery. No reachable violation remained.

## Verification notes

Independently executed against the immutable candidate:

- `mise exec -- cargo nextest run --locked -p wyrd-semver --lib -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'` — 1 passed.
- The exact repository-managed TypeScript Workflow journey from R4 — 1 file,
  1 test passed; it covered the public server-backed loading path and malformed
  selector field assertions.
- `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c 2d669917c03699876b3c8926f0de5ac88c578c01` — passed.

I did not independently rerun every broad aggregate listed in the original
task. Their candidate-bound results and exact focused runs are recorded in the
task and R2-R4 evidence; the inspected state and the two independently rerun
R4 closure paths are consistent with those results. No live provider or cloud
credential was used; deterministic local mock/provider and repository-managed
Postgres are the intended proof seams for this task.

## Overall verdict

**PASS**
