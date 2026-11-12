---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-sdk-authoring-ergonomics
spec_revision: 7
requirements: [REQ-013, REQ-014, REQ-015, INV-001, INV-002, INV-003, AC-009, AC-010, AC-011]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1]
---

# Document scoped write cancellation and retry

## Outcome and authority

Make the new async write API's existing cancellation, admission and retry
contract explicit. This is documentation remediation; the write implementation
and its runtime behavior remain unchanged.

- Approved authority: [spec revision 7](../../spec.md).
- Original task: [TASK-003](../../tasks/TASK-003-run-bifrost-write.md).
- Original review base: `eabe91a5c778dc6e991ad594e8f20f59fca6ed6b`.
- Reviewed candidate / remediation starting point:
  `445c4ea04e6980c69c531fdd75be8d3aeefb287a`.
- Evidence: [verdict](verdict.md),
  [independent validation](findings-validation.md#find-task-003-1).

The review verdict remains historical against revision 6. The maintainer has
since approved revision 7's telemetry removal and requested this remediation.
The spec and original task are reconciled in the same handoff. Do not restore
telemetry, add aliases, or re-review unrelated accepted behavior.

## Diagnosis — FIND-TASK-003-1

Root AGENTS.md and `agent_docs/rust.md` require changed async Rust items to
document cancellation, partial progress, idempotency and retry. The newly
exposed `RunBifrost::write`, JSON adapter and common private stage explain
buffering and errors but omit this required contract. The renamed native
TypeScript method also omits its spawned operation's ownership semantics.
This is an explicit mandatory documentation gate, not an inferred runtime bug.

At the reviewed candidate, `crates/shared/wyrd-client/src/observe/mod.rs:483`
(`write`), `:507` (`write_json`) and `:522` (`write_value`) lead to one awaited
table resolution, followed by synchronous row projection and admission.
Cancelling the Rust future while suspended in resolution admits no row.
Successful admission transfers the row to the existing queue; cancellation
does not retract an already admitted row. A repeated caller write is a new
admission, not an idempotent replay of an earlier successful call. Queue-owned
retries retain their existing batch identity and are a different operation.

`sdks/wyrd-sdk-ts/native/src/cards.rs:1065` (`NativeRun::write`) clones the Run
and delegates through `trace::spawn` and `Env::spawn_future`; discarding a
JavaScript promise does not cancel that spawned write. The shared Rust future
and native promise therefore need accurately distinguished contracts. Python
blocks on the shared JSON method and needs no new lifetime or runtime API.

The missing prose leaves callers without the required guidance for abandonment
and retries. Existing green runtime tests do not close a documentation gate.

## Recommended correction and reuse boundary

Document the existing contract at `RunBifrost::write`, and link to it from the
JSON adapter and private common stage rather than duplicating it. Explain the
resolve-before-admission cancellation boundary, all-or-none row admission,
non-idempotence of repeating a successful caller write, and distinction from
queue-owned retained-batch retries. Preserve the existing explanation that
whole-row schema validation can be deferred and success means buffering;
successful state shutdown remains the drain barrier.

Document the existing spawned-promise ownership at `NativeRun::write` without
promising JavaScript abandonment cancels the operation. Reuse the current
`trace::spawn` owner; change no runtime code. Regenerate affected TypeScript
native declaration comments through `mise run ts:build`; never edit generated
declarations by hand.

The correction is a few rustdoc paragraphs/links and generated comment parity.
Deletion would remove the required API, an unlinked resolver contract does not
cover write retries or native ownership, and new cancellation/retry machinery
would exceed both the finding and approved scope. No downstream guard, helper,
configuration, dependency or new test harness is justified.

## Preserved behavior and non-goals

Keep Card/Run attribution, validation/error stages, table resolution/cache,
queue capacity and errors, deferred schema checking, authorization, tenancy,
batching, direct insertion/write_batch, retry and state lifecycle unchanged.
Preserve revision 7's removal of telemetry record/dropped and exclusive
plumbing, including remaining writers' loss accounting. Do not add cancellation
APIs, deduplication keys, automatic retries, durability promises, state-scoped
writers, compatibility aliases or unrelated documentation cleanup.

## Acceptance and focused proof

| Finding / obligation | Required observation |
|---|---|
| FIND-TASK-003-1; changed-item rustdoc gate; REQ-014 | Shared write documentation explicitly covers cancellation, partial progress, idempotency and retry; JSON/private stages link to the common contract. No false promise of full validation or durability on return. |
| FIND-TASK-003-1; REQ-013 | Native write documentation distinguishes spawned-promise abandonment from dropping a caller-owned Rust future. Any changed declaration comments match generated native source. |
| INV-001–003; REQ-015 | Diff contains only the bounded documentation/generated-comment correction and task evidence. No runtime semantics or removed telemetry surfaces change. |

TDD is not applicable: this corrects prose about already implemented behavior.
Do not manufacture a RED or add runtime tests. Read the complete current shared
write/JSON/private bodies, resolver, insertion owner and native spawn/write
bodies to confirm every statement. Existing task-review acceptance evidence
carries forward for unchanged runtime behavior.

Run from the repository root:

```bash
mise run fmt:check
mise run ts:build
mise run ts:format:check
mise exec -- git diff --check
```

Inspect generated `index.d.ts` and `index.d.cts` against the native method;
expected changes are documentation only. Record actual command results and
inspect the cumulative diff for accidental runtime or unrelated generated
changes. The build supplies native generation/parity evidence; no full SDK
journeys, Postgres setup, package smoke suites or broad aggregate is required
for this documentation correction. Do not stage or commit without instruction.

## Stop conditions and handoff

If source no longer supports the documented contract, stop and report the
specific source mismatch rather than changing runtime behavior to fit prose.
A new public cancellation/retry/concurrency or durability decision returns to
specification authority. Reversible wording and accurate links remain owned by
the implementer.

Route directly to `$wyrd-implement`. After implementation, review finding
closure, the prior-candidate-to-new-candidate diff and affected documentation /
generated consumers, retaining the original task, prior review and revision 7
as acceptance context. Do not restart full discovery or change the old verdict.

## Implementation evidence

Source confirmed against unchanged bodies: `write_value`'s only await is
`Bifrost::resolve_writer_table` (abandonment caches nothing); admission is the
synchronous all-or-none `insert_observation`; `NativeRun::write` runs through
`trace::spawn` → `Env::spawn_future` on a cloned Run.

| Finding / obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-1; rustdoc gate; REQ-014 | `crates/shared/wyrd-client/src/observe/mod.rs` `RunBifrost::write` `# Cancellation` / `# Retry`; `write_json` and `write_value` link to it; buffering, deferred validation and shutdown drain prose unchanged | `mise run fmt:check` exit 0 | PASS |
| FIND-TASK-003-1; REQ-013 | `sdks/wyrd-sdk-ts/native/src/cards.rs` `NativeRun::write`: discarding the promise does not cancel the spawned write; success/refusal retry semantics | `mise run ts:build` exit 0; `index.d.ts` / `index.d.cts` gain the same 6 comment lines only; `mise run ts:format:check` exit 0 | PASS |
| INV-001–003; REQ-015 | Diff is rustdoc, generated comments and this evidence; no runtime, telemetry or alias change | `git diff --check` exit 0; generated diff contains only `*` comment lines | PASS |

Non-goals stayed out: no cancellation API, dedup key, automatic retry,
durability promise, state writer or alias. Unrelated working-tree changes
under `changes/` predate this remediation and are untouched.
