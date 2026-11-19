# TASK-003 maintainer review

Result: **FAIL** — one bounded mandatory rustdoc correction remains. No material duplicate workflow or unearned abstraction was established.

## Immutable subject and scope

- Base: `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`.
- Candidate: `384a0def73ed6eac4041dc1e428dc30e981514fe`.
- Reviewed the complete cumulative range, not the revision-22 subset. This is independent discovery; no other review reports were read.
- Authority: supplied root `AGENTS.md`, SDK-local Python/TypeScript rules, `agent_docs/maintainer-style.md`, `agent_docs/rust.md`, `changes/spec-driven-development.md`, the pinned TASK-003 and spec sections on one execution owner, existing transformations, bounded controls and recovery, and the Verifier/execution sections of `architecture/wyrd-design.md`.
- Used CodeGraph before locating/exploring the orchestration owners. At completion HEAD remained the candidate, assigned tracked source had no working-tree changes, and every entry in `authority-sha256.txt` passed verification.

## Changed-surface coverage

| Surface | Source/callers examined | Assessment |
|---|---|---|
| Engine state and recovery | `engine.rs`: `Checkpointed`, `Budget`, `with_checkpoints`, restored/recorded/frozen/disposition lookups, admission, begun/attempted records, begin/forget/commit/control; server `RunState` implementation and runner call site | State remains on the existing dependency-owning engine and run-state owner. Host supplies the accepted snapshot through the existing checkpoint capability; no competing checkpoint writer/store is introduced here. |
| Ready scheduling and running work | `VerificationEngine::run/drive/prepare/step`, `ready/complete/guard`, `Node`; `walk.rs::RunLedger`, `Prepared`, `Tasks` and panic handling | The previous stage walker is replaced in place. `RunLedger` remains the settlement authority; `Tasks` owns pending futures and drop/drain behavior. No second graph store or parallel execution entry point was found. |
| Fanout, sequential repeat, speculation | `members/member/instance_requirements`, `decide_speculation`, common `requirement/attempt/judge`, aggregate/own/unrun/attempt records | Fanout and repeat compose the existing action/check/settlement path; they do not embed another Agent runtime or retry loop. The member loop adds the required iteration sequencing; action retry stays in `attempt`. |
| Binding and publication selection | Complete `Scope`/`Frame`/`Iteration` resolution, missing causes, collections and attempt records; engine bindings/check bindings/mount/condition helpers; hosted workspace consumer diff | Resolution remains on `Scope`; frame-specific values extend the existing parser and resolver. Collection and named-member selection stay branches of the same owner. Pure preparation remains synchronous. |
| Planner and validation | `Step::prerequisites/guard_steps`, `VerifierSpec::plan/dependencies`, existing `plan_stages`, `Validation` position/reference/control rules; reference parse/display | Runtime readiness needs a different eligibility predicate from conservative cycle validation. Both derive edges from the existing Step owner; the existing SCC/stage planner still rejects cycles. This is not a second graph-validation workflow. |
| Hosted/checkpoint adapters | `BlockCall::producer`, `Checkpoints` signature/control extension, Agent executor change; `RunState::begin/forget/settle/control`, runner snapshot/deadline changes | Existing adapters carry full invocation identities; the old plain-producer adapter is removed. `RunState::change` remains the shared durable mutation path. No extra service or runtime owner is introduced. |
| Public declarations | `Step`, `DependsOn`, `Foreach`, `Repeat`, `VerifierLimits`, `ReferenceRoot`, requirement item/iteration fields, speculation/control shapes; generated TS card declarations, native TS declarations, Python authored stubs | Changed shape projections align in the inspected declarations. Generated TS controls reproduce the Rust owner; Python/TS lifecycle projections do not introduce orchestration. No generator or runtime check was rerun. |
| Changed test support and clarity | Engine `Script`, `Behavior`, `Ledger` and new assertions; Rust orchestration `Desk`, scripted provider and helper methods; shared hosted tools/support changes; Python conftest/support; `VerificationFixture` diff; Scribe conflict test/helper insertion | Tests use the existing injected executors, public SDK fixture/deployment owners, and local provider harness. Outcome assertions distinguish ordering, identity, skipped/error cases, recovery, manifests and bounds rather than only checking response presence. The new `Desk` owns the actual deployment/upstream/state dependencies. Documentation gaps are grouped below. |
| Scribe correction ownership | `ShardOwner::write_group/contradicts_retained`, `Memtable::retained_batch_rows/retained_identity_rows`, contradiction regression | Conflict isolation is added to the existing grouped-write owner using the existing retained-identity check. It is not another retry/write lifecycle. The helper insertion displaces existing rustdoc (finding below). |

## Whole-workflow comparison with BASE

The BASE implementation was inspected directly from Git. BASE `engine.rs::run` (496), `drive` (648), `step` (705), `requirement` (749), `attempt` (832) and `judge` (923) compose `walk.rs::walk` (105), whose `RunLedger` (32) accumulated stage settlements only at a barrier. Candidate `run` (699) enters `drive` (877), which keeps that ledger and immediately releases ready steps. The old `walk` function is removed, not left alongside a competing scheduler. The running-future poll/catch-unwind mechanism moves into `Tasks::next` under the same module.

The missing BASE behavior is real: the stage barrier cannot release a dependent while an unrelated peer remains active, does not represent per-item/per-iteration results, and has no separate guard disposition. Candidate `members` and `member` supply those required control lifetimes and call the same `attempt`, `requirement`, `judge`, checkpoint and Scope owners. Meaningful engine dependencies are not used as an excuse to retain duplicate orchestration; comparison of the complete paths found no surviving duplicate workflow to delete.

The new pure helpers transform caller-owned arguments; retained run state remains on `VerificationEngine`, `RunLedger`, `Tasks`, `Scope`, and `RunState`. Async methods inspected await IO, admission, futures that perform IO, or the pre-existing async capability contract. A test implementation of `Checkpoints` retaining that trait's async signature is not independently removable pure async machinery.

## Findings

### MAINT-003-01 — Required rustdoc is missing or attached to the wrong item

- **Classification:** VIOLATION; explicit mandatory repository gate.
- **Rule:** root `AGENTS.md`: every new or changed Rust item has rustdoc, with `# Panics` when it can panic; missing rustdoc blocks merge. `agent_docs/rust.md` explicitly includes private/test helpers and requires invariants sufficient to change the code safely. The task-review materiality rule preserves this gate even when a panic is an internal invariant.
- **Changed locations and source evidence:**

| Location | Gap |
|---|---|
| `crates/vala/vala-verify/src/blocks/engine.rs:1229` (`members`) | Indexes `spec.steps[index]` while its new rustdoc has no `# Panics` documenting the declaration-index invariant. |
| Same file:1287 (`member`) | Directly indexes the declared step; no `# Panics`. |
| Same file:1422 (`instance_requirements`) | Indexes `spec.steps[index]` to enumerate requirement instances; no `# Panics`. |
| Same file:1929 (`ready`) | Indexes the step carried by `Node::Step`; no `# Panics`. |
| Same file:1990 (`missing_requirements`) | Directly indexes `spec.steps[index].requirements`; no `# Panics`. |
| Same file:2136 (`Node::id`) | Indexes step/requirement declarations using the variant's index; no `# Panics`. |
| `crates/wyrd-spec/src/card/verifier/validate.rs:2648` (`controlled`) | Calls `step.as_object().expect("step fields")`; new helper docs omit the object precondition and `# Panics`. |
| `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:4471`–4500 | The new `contradicts_retained` is inserted after the old `next_prepared_slice` rustdoc. The resulting docs claim the boolean predicate pops a materialized slice, while `next_prepared_slice` now has no attached rustdoc. |

- **Concrete maintenance cost:** reviewers cannot recover the private declaration-key preconditions from these new helper contracts; the Scribe helper insertion additionally documents the wrong operation and leaves an existing changed item undocumented. These are mandatory documentation failures, not evidence that supported callers currently panic.
- **Nearby pattern:** `VerificationEngine::step` (1056), `requirement` (1466), `Judged::report` (2419), and `requirement_at` (2531) already document the corresponding declaration-key panic preconditions; validator `refusal` (2669) documents its test-shape panic conditions.
- **Smallest testable correction:** restore the slice-popping docs to `next_prepared_slice`, leave only the retained-conflict contract on `contradicts_retained`, and add the missing panic/precondition sections to the identified new helpers. Keep code, signatures and validation unchanged. Inspect the resulting attached rustdoc against each body and run the existing narrow formatting/documentation checks; no new test or abstraction is warranted.
- **Correction burden:** documentation-only, bounded to existing touched items; deletion/reuse is unnecessary because these helpers implement required behavior and already sit with their owners. This should be batched with any other confirmed documentation corrections, not made a separate style round.

## Verification limits

This was static source/diff review. No Cargo, tests, generators, commits or implementation edits were run. Recorded task runs were treated as claims; assertions and owner paths above were inspected directly. Runtime behavior, tenancy and failure/recovery acceptance remain with their assigned reviewers and the orchestrator's evidence reconciliation.

No optional redesign or preference-only finding is proposed. Runtime exhaustion, provider behavior and platform suites were not independently checked.
