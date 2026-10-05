# Independent behavior review — TASK-002 R5

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2d669917c03699876b3c8926f0de5ac88c578c01`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation inputs: the R2, R3, and R4 remediation tasks named by the caller,
  plus prior verdicts and validated ledgers R1–R4 as closure hypotheses

`HEAD` matched the candidate before and after source inspection. The repository
has no `.codegraph/` index, so I traced the complete cumulative diff and current
owners and callers directly. I did not modify reviewed implementation source.

The realistic paths inspected were:

- authored entry file -> `wyrd_loader::load` -> provenance-separated
  `WorkflowBodies` -> lazy ambient `Cards` construction only for external refs
  -> exact `CardGraphHydrator` reads -> Skald hydration and resolved validation
  -> the Rust, Python, and TypeScript run projections;
- registered exact/UID selector -> typed Cards Workflow view -> locked
  relationship traversal -> Active exact body set -> Skald hydration -> run;
- composite registration -> graph planning -> tenant-scoped `EffectiveSpecs`
  body resolution -> declaration-only Skald validation -> exact expected-UID
  write recheck -> one audited registration transaction;
- TypeScript selector JSON -> field-owned domain constructors -> shared
  `WorkflowCards::load` -> stable public error projection; and
- `VersionBlock` construction by parsing and Serde -> `CardRef` and named
  selectors -> registry request boundary.

The standing human direction was applied throughout. The implementation and
the R4 correction use ordinary validated-newtype Serde, existing domain
constructors, existing workspace dependencies, Tokio's installed blocking
pool, existing PostgreSQL locking, and the repository's established SDK test
lanes. I found no task-added mechanism, check, file, setting, or option that is
unsupported by repository practice or comparable widely used projects, and no
remediation-only machinery is required.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002/003, AC-001: one normal Workflow Card, Agent steps, and the existing Prompt binder | `wyrd-loader/src/lib.rs`; `skald-workflow/src/bodies.rs::Workflow::{from_card_bodies,validate_card_bodies}`; `wyrd-client/src/workflow.rs::Workflow::from_path`; checked-in `examples/workflows/code-review` | Shared-client focused test loads and runs the actual example through an injected deterministic gateway; three SDK journeys run the task's explicit Native variant | PASS |
| REQ-013/013A/014, AC-006: pure and resolved validation occur at the earliest informed boundary and refusals dispatch nothing | loader validation; Skald body hydration/validation; `EffectiveSpecs::validate_workflows`; registered hydration before publication | shared-client graph/binding/dialect negatives and `pg_workflow_registration::registers_only_valid_explicit_workflow_graphs` | PASS |
| REQ-025/054/055, AC-029/030: all three SDKs expose the approved authored and registered loading paths; local bundles need no client and external refs resolve lazily | shared `Workflow::from_path` constructs `Cards` only when `external_refs` is nonempty; Rust re-export; `PyWorkflow::from_path`/`PyWorkflowCards`; TypeScript `Workflow.fromPath`/`WorkflowCards.load` | Rust, Python, and TypeScript real-runtime journeys cover local, mixed, registered, credential, denial, and missing/inactive cases | PASS |
| REQ-056, INV-005: sibling/external provenance, exact versions, locked UIDs, Active state, and read authority remain distinct | `cards/hydrate/workflow.rs::WorkflowBodies` keeps separate sibling and registered stores; `GraphTraversal` follows exact relationships; `extend_registered` enforces Active | all three journeys exercise the same-identity shadow case, after-v2 pinning, exact/UID loads, denial, and deleted dependency | PASS |
| REQ-028, AC-002: composite registration validates and writes the same exact Workflow/Agent/Prompt graph without provider dispatch or partial persistence | `EffectiveSpecs` resolves external bodies by tenant and UID while retaining sibling provenance; `RegistrationWriter::write` performs audited atomic persistence | registration journey proves exact Workflow-to-Agent and Agent-to-Prompt relationships, invalid graph no-write, and no dispatch | PASS |
| Preserved FIND-TASK-002-4: preflight body/UID cannot be replaced before relationship persistence | `RegistrationWriter::write` invokes `recheck_active_card_refs`; SQL matches exact identity plus expected UID and Active status under `FOR SHARE` | `refuses_stale_preflight_after_dependency_replacement`; `relationship_recheck_blocks_target_lifecycle_race` | PASS |
| REQ-052/INV-008: registration is declaration-only while execution uses the existing caller/runtime tool owner | `Workflow::validate_card_bodies` suppresses tool binding only on temporary validation values; `Workflow::from_card_bodies` uses the existing registry at execution | registration proof plus Native execution journeys | PASS |
| Exact Rust public contract | `wyrd_client::Workflow::{from_path,run}` and `Cards::workflow().load(&CardSelector)`; SDK re-exports shared types | ignored Rust SDK journey run explicitly; wrong-kind/versionless/missing/denied/inactive cases included | PASS |
| Exact Python public contract | `PyWorkflow::from_path`; `PyWorkflowCards::load`; generated public exports/stubs; old static `Workflow.load` absent | Python integration journey, unit typing/export evidence, codegen check | PASS |
| Exact TypeScript public contract and portable run snapshot | native `NativeWorkflow`/`NativeWorkflowLoad`; public closed `WorkflowSelector`, recursive `JsonValue`, complete `WorkflowRun`/step/error types | TypeScript type/unit proof and real Node/Postgres journey | PASS |
| INV-007 and FIND-TASK-002-11: malformed Workflow selectors refuse locally with the Workflow-owned code and the offending field | `native/src/workflow.rs::parse_workflow_selector` keeps values as strings and applies `CardUid`, `SpaceName`, `CardName`, and `VersionBlock` constructors; combinations remain `field=selector` | TypeScript journey asserts code/status/details for mixed shape and malformed uid/space/name/version using a caller whose registry read would be denied | PASS |
| REQ-056 and FIND-TASK-002-11: an exact `VersionBlock` cannot be populated by Serde with a range, partial, empty, or invalid value | `wyrd-semver/src/block.rs::Deserialize` delegates to `VersionBlock::parse`; `VersionRange` remains the separate range owner | `block::tests::serde_preserves_the_exact_version_invariant`; server regression now proves ranged `CardRef` decoding fails before loading | PASS |
| REQ-057/059, AC-031: reuse existing owners and remove obsolete WorkflowLoader/WorkflowGraph and keyed normalization without collateral regression | thin facade owns no graph/parser/executor; Cards hydrator and Skald remain owners; obsolete symbols and compatibility alias are absent; WyrdState remains Service-rooted | source/diff audit, shared loader/Service hydration evidence, client-tier checks | PASS |
| Async and cancellation boundary | synchronous filesystem load/canonicalization runs in `tokio::task::spawn_blocking`; registry work remains async; load paths publish only complete values and write nothing | shared focused load test and SDK journeys; boundary documentation matches behavior | PASS |
| Prior findings FIND-TASK-002-1 through -10 and -12 through -15 remain closed | provenance stores, versioned Prompt example, UID recheck, owner methods, exact SDK relationship assertions, generic Python Validation owner, blocking-pool use, generated Hakari state, and clean cumulative patch are all present in the candidate | prior focused closure evidence plus R4 candidate-bound focused/boundary runs | PASS |
| Non-goals: no Workflow principal/WyrdState root, public loader/hydrator, duplicate transport/parser/cache/executor, compatibility alias, registration-time execution/secret resolution, or TASK-003/004/005 gateway/server-job/CLI implementation | cumulative production diff stays within the approved loader/client/Skald/registration/SDK projection boundaries | source and complete-diff audit | PASS |
| Human standard-mechanism direction | all retained mechanisms are existing repository owners or ordinary Rust/Serde/Tokio/PostgreSQL/SDK patterns; R4 adds only the already-used workspace `wyrd-semver` dependency to the native projection that consumes its type | source/manifest audit and boundary checks | PASS |

## Proposed findings

None. I found no reachable task-acceptance failure, regression, violation, or
unsupported drift in the cumulative candidate.

## Prior-finding closure

- `FIND-TASK-002-1` through `-10` remain closed at the provenance, example,
  canonical-owner, exact-UID, documentation/import, SDK journey, TypeScript
  snapshot, owner-shape, and registration-version-intent boundaries recorded by
  the prior ledgers.
- `FIND-TASK-002-12` through `-15` remain closed at generic Python selector
  error ownership, the Tokio blocking boundary, generated workspace feature
  state, and cumulative patch hygiene.
- `FIND-TASK-002-11` is now fully closed. `VersionBlock` deserialization
  preserves its exact-version invariant, and TypeScript malformed selector
  values use the established field constructors and Workflow error owner before
  registry IO. The R4 correction introduces no parallel validator or check.

## Verification evidence and limits

I independently ran:

```text
mise exec -- cargo nextest run --locked -p wyrd-semver --lib \
  -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'
```

Result: **PASS**, 1 selected / 1 passed.

The committed R4 remediation evidence records PASS for the TypeScript journey,
all three SDK journeys, the three Workflow-registration tests, the SQL
relationship-race test, shared/client tests, codegen, Python and TypeScript
typing, N-API, client-tier/SDK-tier/PyO3/transaction boundary checks, formatting,
lints, and cumulative diff hygiene. I treated those results as evidence claims
and checked their relevant assertions against current source; I did not rerun
every broad lane. No missing broad rerun masks a source-observable failure in
this behavior review.

## Overall result

**PASS.** The cumulative candidate satisfies the original TASK-002 cleanup and
all three bounded remediation tasks exactly. The behavior finding ledger is
empty.
