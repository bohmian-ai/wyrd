---
id: TASK-005-R1
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 8
parent_task: TASK-005
remediates: [FIND-005-1]
---

# Complete required documentation on extraction proof helpers

Route directly to `$wyrd-implement`.

Approved specification: `changes/active/github-verifier-pipelines/spec.md`, revision 8.
Original task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.
Review: `changes/active/github-verifier-pipelines/review/005-pull-request-extraction-codex-r1/verdict.md` and `findings-validation.md` beside it.
Base: `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`.
Reviewed candidate: `1f798d132c03cbc2474e1bad4bc36763e8047848`.
This remediation belongs to this independent review; it neither carries forward nor rewrites the separate non-standard r1/r2 reviews.

## Diagnosis — FIND-005-1

The implementation's connected behavior passes the recorded source review and focused verification, but newly authored proof helpers omit mandatory rustdoc. Root `AGENTS.md:152–154` requires every new/changed Rust item to have rustdoc and a `# Panics` section when it can panic; `agent_docs/rust.md:233–241` explicitly includes fields and private/test helpers. The review skill preserves this merge gate regardless of runtime probability or green tests.

In `crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs:756–757`, `HeldBatch.lock` and `HeldBatch.write` have no field rustdoc. `HeldBatch::start` retains a transaction holding the snapshot-record row lock and an in-flight metadata write; `release` commits the lock transaction before awaiting that write. The batching visibility and failure-rollback tests consume this owner. The enclosing struct's scenario description does not document each field as required.

In `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4607–4615`, `source_extracted` reads the execution's coordination fact using `fetch_one(...).await.expect("execution reads")`. A query failure or missing execution row panics, but its documentation lacks `# Panics`. Its sibling callers verify single/batched completion, identical replay and stale/rolled-back/cancelled completion behavior. The issue is missing documentation at the proof owners, not invalid production state or a missing runtime guard.

The observable acceptance failure is the explicit mandatory repository documentation gate. No production failure, missing feature or specification change is asserted.

## Intended correction and selected approach

Preserve the existing test owners. Add rustdoc to the two HeldBatch fields explaining their retained lock-transaction and pending-write roles, and document source_extracted's concrete panic condition. Limit the correction to comments in the two named test files.

This closes the omissions where they originate. No downstream caller changes or repeated guards are needed. Deleting or inlining the helpers would discard or duplicate useful setup; changing the expect, signatures or fixture machinery would change behavior for no required outcome. The correction costs only a few documentation lines and introduces no new abstraction, dependency, configuration or public contract.

## Constraints and non-goals

Preserve every executable statement, assertion, signature, test name, fixture, queue bound, transaction boundary and error behavior. Do not edit source behavior, other docs, architecture, generated artifacts, spec or original task. Do not add tests, allow/ignore attributes, new helpers or performance work. This review did not authorize implementation refactoring or a broader discovery round.

## Acceptance and focused proof

- AC-R1-1 / FIND-005-1: both HeldBatch fields have accurate rustdoc describing their distinct ownership roles.
- AC-R1-2 / FIND-005-1: source_extracted documents that a failed execution-row read or absent row can panic, under `# Panics`.
- AC-R1-3 / FIND-005-1: the remediation diff changes comments only in those two test files; runtime behavior and existing proof remain identical.

Inspect the actual documentation against the unchanged complete helper bodies and callers, then run `mise run fmt:check` and `git diff --check`. No new runtime proof is needed for comments. Do not require full user journeys, broad aggregates, codegen, benchmarks or full Clippy for this write set. The original reviewed candidate already passed 198 focused behavioral tests, recorded in verification.md; this remediation's proof concerns the missing documentation gate only.

## Obligations

| ID | Promise | Source | Planned proof |
|---|---|---|---|
| R1-1 | `HeldBatch.lock` and `HeldBatch.write` carry rustdoc naming their distinct roles | FIND-005-1, AC-R1-1 | content review against `HeldBatch::start`/`release` |
| R1-2 | `source_extracted` documents its panic under `# Panics` | FIND-005-1, AC-R1-2 | content review against its `expect` |
| R1-3 | Only comments change, in the two named test files | AC-R1-3 | `git diff` |

## Evidence

| Obligation | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R1-1 | field docs: `lock` is the open row-lock transaction whose commit releases the blocked batch; `write` is the second write pending in that batch | `pg_code_review_cache.rs` `HeldBatch` | content review against `start` (returns `HeldBatch { lock, write }` once a batch blocks) and `release` | PASS |
| R1-2 | `# Panics` when the read fails or no row is visible | `pg_verifier_executions.rs` `source_extracted` | content review against `fetch_one(...).expect("execution reads")` | PASS |
| R1-3 | 6 added comment lines, 2 files | `git diff --stat` | `mise run fmt:check` 0; `git diff --check` clean | PASS |

No RED: documentation only. No executable line, test, fixture or other file changed.
