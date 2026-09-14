---
id: TASK-002-R3
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-019, REQ-056, REQ-060, AC-004, AC-021]
depends_on: [TASK-002-R2]
parent_task: TASK-002
remediates: [FIND-TASK-002-7, FIND-TASK-002-8, FIND-TASK-002-15, FIND-TASK-002-16, FIND-TASK-002-17, FIND-TASK-002-18]
---

# Close TASK-002-R2 review findings

## Authority and candidate

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior reviews: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/` and `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/`
- Review: `changes/active/surfaces-oracle-integration/review/task-002-r2-f345cd8a4-review-01/`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Implementation route: `$wyrd-implement`

All committed branch content is authorized TASK-002 candidate content and is
fair game for correctness review. The package-local Python SDK release profile
is expressly allowed. Do not remove either merely for scope or default-policy
reasons.

## Outcome

Finish the remaining repository-rule and lifecycle gaps without changing the
approved public API or architecture: fully document relocated workflows, use
module-top type imports, make cumulative diff evidence truthful, coordinate
direct writes with shutdown, make completed settlement non-reentrant, and
produce an equivalent candidate history without prohibited AI co-author
trailers.

## Issue diagnoses and required corrections

### `FIND-TASK-002-7` — incomplete fallibility and cancellation rustdoc

The R2 documentation pass added descriptive prose but did not satisfy the
mandatory `# Errors` and applicable cancellation/partial-progress rules on live
fallible workflows. Validated sites include Cards configuration and registration
saga functions under `crates/shared/wyrd-client/src/cards`, plus
`PgIssuerResolver::trusted_issuer` in
`crates/wyrd/wyrd-auth/src/pg_resolvers.rs`. These functions are called by
public Cards registration and authentication flows and can fail or be cancelled
after filesystem, network, upload, Card-completion, or database progress.

Complete documentation on the existing items only. Add accurate `# Errors`
sections and cancellation/partial-progress text where the full body can perform
IO or durable work before cancellation. Do not move code, extract helpers, add
a lint suppression, or create another permanent check.

### `FIND-TASK-002-8` — qualified signatures and function-scoped import remain

Materially relocated and changed code still uses governed qualified types in
`wyrd-client` Bifrost, the TypeScript native boundary, and real-server test
signatures. A Scribe WAL test also declares `BifrostNamespace` inside a test
function. This violates the mandatory module-top dependency-manifest rule and
shows R2 fixed only the previously enumerated examples.

Use the existing module-top import blocks, adding non-conflicting aliases only
for real name collisions, and replace every governed qualified field,
parameter, return, bound, and `where` type in the cumulative changed set. Move
the WAL import to its test module's import block. Add no wrapper, helper, or
module.

### `FIND-TASK-002-15` — cumulative diff check fails

The recorded unqualified `git diff --check` inspected only the clean working
tree. The required immutable command reports an extra blank line at EOF in the
first review's `findings-validation.md` and the R1 review's `verdict.md`.

Delete only those two terminal blank lines. Do not add a formatter or check.
Prove the new immutable cumulative range directly from the original base.

### `FIND-TASK-002-16` — admitted direct Arrow writes can outlive shutdown

`WriterPool::write_batch` reads the atomic closed flag and then independently
encodes, reserves shared capacity, and awaits its sink. `WriterPool::shutdown`
sets the flag while holding the producer-map lock and drains only buffered
producers. A direct write can observe open, pause before admission, then reserve
and send after an empty-pool shutdown has returned success. Rust, Python, and
TypeScript public `write_batch` and shutdown paths all reach this owner.

Keep `WriterPool` as the single lifecycle owner. Coordinate direct-send
admission and in-flight settlement so shutdown first refuses later direct sends
and then waits for every already admitted direct send before returning success.
Preserve the current single-batch sender, batch identity, retry semantics,
shared live-batch/byte budget, and buffered-producer drain. Add no parallel
lifecycle service or second registry.

### `FIND-TASK-002-17` — failed healthy-drain settlement can repeat

`QueryResultStream::settle` installs `Settled`, then drains a previously healthy
stream. A malformed or trailing frame calls `mark_broken`, overwriting that
terminal state with `Broken`. After status polling completes, a second public
`settle()` sends cancellation and polls again, contradicting its at-most-once
contract.

Preserve resumability when the settlement future itself is cancelled, but keep
the existing `QueryResultStream` terminally settled after a completed
drain-failure/status cycle. Reuse the current state and settlement owner; add no
second state machine or cancellation owner.

### `FIND-TASK-002-18` — prohibited AI co-author trailers

Twenty-two commits in the reviewed base-to-candidate range contain AI
`Co-Authored-By` trailers, while the configured author identity is otherwise
correct. This violates `AGENTS.md` section 13 even though tree content is
unchanged.

After obtaining explicit caller authorization for the history-changing
operation, establish a new immutable candidate with equivalent reviewed tree
content and those trailers absent. Preserve the configured contributor identity
and do not add replacement attribution. The review and implementation agent
must not rewrite history without that authorization.

## Constraints and preserved behavior

- Keep `wyrd-client` and `Bifrost` as the existing shared/public owners.
- Preserve the now-closed client error metadata, problem+json OpenAPI, current
  SDK docs, tenant/auth/audit behavior, greenfield migration baseline, stream
  framing, terminal validation, backpressure, retry, and batch identity.
- Preserve every user-authorized branch commit's tree content except the five
  bounded source/document corrections above.
- Preserve the expressly allowed Python SDK package profile.
- Do not weaken, ignore, allowlist around, or delete a check or test.
- Do not hand-edit generated artifacts.

## Non-goals

- No new public API, lifecycle service, registry, state machine, abstraction,
  dependency, feature, compatibility path, or permanent check.
- No reclassification of empty Bifrost URLs; existing Bifrost projections are
  consistent and preserve structured transport details.
- No migration redesign, OIDC redesign, Forge redesign, or removal of
  authorized cumulative commits.
- No merge, push, release, deployment, or Bifrost aggregate.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| Every validated fallible workflow has accurate `# Errors` and applicable cancellation/partial-progress rustdoc. | `FIND-TASK-002-7` |
| Every cumulative changed governed field/signature/bound uses a module-top import and bare name, and the WAL test import is module-scoped. | `FIND-TASK-002-8` |
| Exact original-base-to-new-candidate `git diff --check` exits zero with no output. | `FIND-TASK-002-15` |
| A direct write is refused before admission or is fully settled before successful shutdown returns, leaving no live batch/byte ownership. | `FIND-TASK-002-16` |
| A completed settlement whose healthy drain fails performs exactly one cancellation and one completed status cycle; a second call is a no-op. | `FIND-TASK-002-17` |
| Every commit in the new cumulative range uses the configured contributor identity and contains no AI co-author trailer. | `FIND-TASK-002-18` |

## Required proof

Add one deterministic stalled-sink test for direct-write admission versus
shutdown and run its exact focused command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=bifrost::handle::tests::shutdown_waits_for_admitted_direct_write)'
```

Add one failed-healthy-drain settlement test and run its exact focused command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=bifrost::query::tests::failed_healthy_drain_settles_once)'
```

Re-run the adjacent existing proofs:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=bifrost::handle::tests::shutdown_and_producer_admission_share_the_pool_lock) | test(=bifrost::query::tests::query_result_stream_settles_every_incomplete_exit_once)'
```

Enumerate every cumulative materially relocated/changed fallible Rust item and
governed signature/import rather than scanning only newly added signature
lines. Verify commit metadata and cumulative diff with the new immutable hash:

```bash
git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<new-candidate>
git log --format='%H%n%B' 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<new-candidate>
```

The second command's output must contain no AI `Co-Authored-By` trailer.

Run the narrow broader lanes covering changed code:

- `mise run fmt`
- `mise run lints`
- the nearest `wyrd-client`, queue, auth, Python-native, and TypeScript-native
  compile/test lanes touched by documentation/import changes
- `mise run py:format`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- the existing Rust, Python, and TypeScript Bifrost write/shutdown and query
  journeys affected by lifecycle changes

Do not run the prohibited Bifrost aggregate, and do not substitute a broad lane
for either named focused proof.

## Implementation evidence

Candidate commits: `ae402d0b7`, `43c9a4382`, `55a41a061`, `e23848c5b`,
`d162e6c61`, `34b58cd0e`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Fallible workflows carry `# Errors`/cancellation rustdoc | `55a41a061`, `e23848c5b` (Cards saga/reads, storage upload, auth issuers, SDK natives) | cumulative missing-`# Errors` scan empty; `mise run lints`; `mise run codegen:check` | PASS |
| Governed signatures use module-top imports; WAL test import module-scoped | `e23848c5b`; napi `Result` kept literal for declaration codegen in `d162e6c61` | cumulative qualified-type scan empty; `mise run ts:build`; `mise run ts:typecheck`; `mise run check:client-tier`; `mise run check:pyo3-scope` | PASS |
| `git diff --check 861f8d86c..<candidate>` exits zero | `43c9a4382` | `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..34b58cd0e` → exit 0, no output | PASS |
| Direct write refused before admission or settled before shutdown returns | `bifrost/handle.rs` `DirectSendPermit` + shutdown wait (`ae402d0b7`) | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::handle::tests::shutdown_waits_for_admitted_direct_write)'` (fails with wait removed) | PASS |
| Failed healthy drain settles exactly once | `bifrost/query.rs` `settle` marks `Settled` (`ae402d0b7`) | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::failed_healthy_drain_settles_once)'` (fails without fix: 2 vs 1) | PASS |
| No AI co-author trailer in the cumulative range | New R3 commits carry none; 22 earlier commits still do | `git log --format='%B' 861f8d86c..HEAD \| grep -c "Co-Authored-By: Claude"` → 22 | BLOCKED: history rewrite needs explicit owner authorization |

Broader lanes: `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run py:typecheck`, `mise run ts:build`,
`mise run ts:typecheck`, `mise run codegen:check`, `mise run test:shared`
(644 passed), `mise run test:bifrost:unit:python:inner`,
`mise run test:bifrost:unit:typescript:inner`,
`mise run test:bifrost:journey:sdk` (16 passed),
`mise run test:bifrost:journey:python` (35 passed),
`mise run test:bifrost:journey:typescript` (16 passed) — all exit 0.

Handled failures surfaced during verification: the SDK lifecycle journey read
removed `vala.audit_staging` columns and a removed `.attempt` operation; it now
asserts the staged `resource`/`outcome` decision rows (`34b58cd0e`). The Python
Bifrost unit task referenced the deleted `tests/test_bifrost.py` (`34b58cd0e`).
Non-goals stayed excluded; unrelated working-tree Forge/SQL edits were not staged.
