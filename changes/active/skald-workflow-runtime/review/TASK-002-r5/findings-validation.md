# Structured Ponytail validation — TASK-002 R5

## Immutable subject and validation inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2d669917c03699876b3c8926f0de5ac88c578c01`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation tasks: R2, R3, and R4 in their named prior review directories
- Prior verdicts and stable finding ledgers: R1 through R4

`HEAD` resolved to the candidate before validation and again immediately before
this report was written. The repository has no `.codegraph/` directory, so I
used the immutable Git range, current source, callers, tests, and repository
search. I modified no reviewed source.

Every required R5 discovery report was available: both task reviews, repository
standards, maintainer, system resilience, registry durability/concurrency,
tenancy/security, and Workflow runtime/lifecycle. I also read `subject.md`,
`verification.md`, and `claim-comparison.md`. Their proposed finding union is
empty. The comparison records no material conflict, unreviewed reachable path,
or unresolved common source, so the conditional follow-up was not triggered.

## Independent validation of the empty proposal union

Agreement among the discovery reviewers was not treated as proof. I inspected
the complete cumulative change and traced its material producers, owners, and
consumers independently.

| Boundary challenged | Independent source trace | Validation |
|---|---|---|
| Authored loading and provenance | `wyrd_loader::load` remains the parser and sandbox owner. `Workflow::from_path` moves the synchronous load and entry canonicalization to Tokio's installed blocking pool, constructs `Cards` only when `WorkflowBodies::external_refs` finds an external `Ref`, and publishes only after `WorkflowBodies::hydrate`. `WorkflowBodies::body` serves `Sibling` only from the sibling store and external `Ref` only from exact registered Cards, including an authored UID when present. | No missing behavior or reopened `FIND-TASK-002-1`; no second loader, graph, cache, parser, or runtime. |
| Registered graph and lifecycle | `WorkflowCards::load` rejects versionless and wrong-kind selectors before IO. `CardGraphHydrator` reuses the existing `GraphTraversal`, follows only exact UID-bearing Agent/Prompt relationships in runtime scope, asserts selector/response identity through the shared read owner, and requires every returned Card to be Active before Skald hydration. Existing Service bundle hydration retains its broader bundle scope. | Exact versions and UIDs do not float; inactive or mismatched Cards cannot produce a runnable Workflow; the scope enum has two real consumers rather than speculative flexibility. |
| Skald validation and execution | `CardBodyResolver` adapts environment-owned exact bodies to Skald's existing Agent and Prompt resolver seams. `from_card_bodies` performs the shared lowering and resolved validation; `validate_card_bodies` clears tool names only on temporary validation values and performs no IO, provider call, secret resolution, or durable mutation. The client facade delegates execution to the existing Skald Workflow. | One validation/lowering/runtime path remains; no registration-time execution or parallel binder/executor exists. |
| Registration provenance and durability | `EffectiveSpecs` keeps submitted siblings and registry-loaded external bodies in separate stores and carries preflight `(CardRef, CardUid)` pairs into `RegistrationWriter`. `recheck_active_card_refs` matches exact identity plus the expected Active UID under `FOR SHARE`; the caller-owned `TenantConn` retains those locks through binding, Card and relationship writes, audit, replay seed, and the single commit. | `FIND-TASK-002-4`, `-9`, and `-10` remain closed. Replacement, lifecycle change, audit failure, write failure, or pre-commit cancellation cannot substitute an unvalidated body or commit a partial graph. |
| Python and TypeScript boundaries | Python and Node each convert their untyped language inputs at their existing foreign-runtime boundary and delegate transport, traversal, validation, and execution to `wyrd-client`/Skald. Generated declarations expose the approved closed selector and recursive JSON/run types. Python releases the GIL through the shared runtime; Node owns ordinary napi values across awaits. | The language-specific code is boundary conversion, not duplicate durable logic or a new validator framework. Public signatures and generated declarations align. |
| `FIND-TASK-002-11` exact-version producer | `VersionBlock::Deserialize` reads the normal string wire form and delegates to the existing `VersionBlock::parse`. `CardRef` and every sibling consumer therefore receive the same exact-version invariant; `VersionRange` remains the separate range owner. The owning test round-trips exact, prerelease, and build values and rejects ranges, partials, empty strings, and invalid syntax. | Closed at the invalid-state producer. A downstream Workflow-only guard would be duplicate machinery and is neither present nor required. |
| `FIND-TASK-002-11` Node field attribution | `WorkflowSelectorJson` retains the same closed `{ uid }` or `{ space, name, version }` shape but holds raw boundary strings. `parse_workflow_selector` uses the existing `CardUid`, `SpaceName`, `CardName`, and `VersionBlock` constructors, maps value failures to their actual field, and retains `selector` for malformed/unknown/mixed/incomplete shapes. `NativeCards::load_workflow` invokes this before the shared Cards read. | Closed at the existing foreign-runtime producer. The public TypeScript journey uses a caller without Cards read authority, so Workflow-family malformed-field results demonstrate refusal before registry authorization/IO. |
| Error and public-contract projection | The shared/Python/Node boundaries use the derive-backed `WorkflowInvalidCardRef`; registry transport and server defense remain unchanged. Rust, Python, and TypeScript expose the approved authored and registered paths, while the Rust SDK stays a thin re-export. | No parallel catalog, compatibility alias, transport, or language-owned durable contract entered the change. |
| Tests, generated state, and dependency edges | The change uses in-owner unit proof, existing Postgres/server targets, and the three existing language-runtime journey shapes. R4 extended the existing TypeScript journey rather than adding a harness. The Node native crate's direct `wyrd-semver` edge imports the workspace owner it now calls; Hakari and declarations were regenerated through established repository mechanisms. | No test-only scanner, lexical ban, retry layer, allowance, feature flag, configuration option, or new third-party dependency is present. |

## Ponytail ladder and human standing direction

The retained implementation stops at the first established mechanism for each
need: the existing loader and canonical reference visitor, Cards hydrator and
transport, Skald resolvers/runtime, `EffectiveSpecs`, `RegistrationWriter`,
`TenantConn`, domain constructors, Serde, Tokio's blocking pool, PostgreSQL
transactions and row locks, generated declarations, and ordinary owning-runtime
tests. These are established Wyrd owners and conventional mechanisms in
comparable Rust, Python, TypeScript, Cargo, PostgreSQL, and integration-test
projects.

Deleting the thin Workflow facade would remove the required client IO surface;
deleting its typed Cards view would remove the approved registered API; and
deleting provenance-separated bodies or the exact-UID recheck would violate
explicit correctness and durability obligations. Conversely, another loader,
selector framework, cache, repository layer, worker, retry loop, cancellation
shim, check, scanner, allowlist, feature flag, setting, or compatibility path
would be unsupported DRIFT. None was added, and none is required as remediation.

The one extended Vitest timeout is attached to the existing real-server journey
and follows the neighboring server-backed tests after a recorded diagnosis of
Vitest's default five-second budget; it is not production machinery or a hidden
retry. The ignored Rust journey is the repository's explicit environment-gated
test selected by its exact command, not a disabled acceptance path.

## Prior stable finding closure

| Stable finding | Independently validated disposition |
|---|---|
| `FIND-TASK-002-1` | Closed: sibling and external body provenance remains distinct through client hydration and server effective-spec validation. |
| `FIND-TASK-002-2` | Closed: the checked-in bundle contains exact Prompt Cards, and stored Workflow-to-Agent and Agent-to-Prompt refs/relationships are asserted. |
| `FIND-TASK-002-3` | Superseded and closed under approved Revision 12: keyed wrapper normalization and its duplicate slot inventory remain removed. The loader's inline-body discrimination is part of the existing untagged native body/reference syntax, not the removed keyed dialect. |
| `FIND-TASK-002-4` | Closed: the exact preflight UID is rechecked with exact identity and Active status and locked through the registration commit. |
| `FIND-TASK-002-5` and `-6` | Closed: changed loading/registration items carry the required behavior, error, panic, and cancellation documentation, and changed imports/signatures follow repository shape. |
| `FIND-TASK-002-7` | Closed: each real language journey inspects the complete exact Workflow-to-Agent-to-Prompt graph and retains after-v2 pinning and execution proof. |
| `FIND-TASK-002-8` | Closed: TypeScript projects recursive JSON input and the complete native Workflow run/step/error snapshot. |
| `FIND-TASK-002-9` | Closed: `CardGraphHydrator`, `EffectiveSpecs`, and `RegistrationWriter` remain cohesive state-owning structs rather than parallel orchestration layers. |
| `FIND-TASK-002-10` | Closed: graph-ready validation holders do not replace authored version intent used for request hashing, replay, allocation, or persistence. |
| `FIND-TASK-002-11` | Closed: validated `VersionBlock` deserialization prevents invalid exact-version state, and Node field conversion now reports the precise malformed field before Cards IO. |
| `FIND-TASK-002-12` | Closed: generic Python selector failures use request Validation while Workflow-specific selectors and genuine Data validation retain their owning contracts. |
| `FIND-TASK-002-13` | Closed through standard Tokio `spawn_blocking` at the shared client owner; no custom pool or cancellation mechanism was introduced. |
| `FIND-TASK-002-14` | Closed through sanctioned Hakari regeneration and the existing workspace-hack gate. |
| `FIND-TASK-002-15` | Closed: cumulative `git diff --check` passes. |

## Final deduplicated stable ledger

**Explicitly validated empty ledger.** No discovery proposal exists to retain,
revise, or reject, and independent source validation found no reachable
`MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding. No new
stable finding ID is assigned.

The candidate needs no remediation and no specification revision. Available
verification directly exercises the final R4 producer fixes and their public
TypeScript consumer, while the cumulative source and prior candidate-bound
evidence cover the broader task paths. I found no missing source, incomplete
caller trace, unresolved report conflict, or unsupported mechanism that would
block validation.
