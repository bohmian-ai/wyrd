---
id: TASK-003-R2
kind: remediation
status: proposed
spec: SPEC-positioning
spec_revision: 6
requirements: [REQ-012, REQ-017, INV-001, INV-003, INV-004, INV-005, AC-011, AC-016]
depends_on: [TASK-003-R1]
parent_task: TASK-003
remediates: [FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4]
---

# Complete the Rust tutorial sequence and SDK negative proof

Route directly to `$wyrd-implement`. The Rust agent tutorial demonstrates the
approved explicit setup and flush calls, and TypeScript has the prescribed
unit proof that default configure fails when an optional telemetry package is
missing. Batch two local Rust documentation corrections with that work.

## Approved authority and review context

- Approved specification: `changes/active/positioning/spec.md`, revision 6,
  REQ-017 and AC-016. Review content hash:
  `7f9bdb2a67e6b113b02a58526925b75f91028112`.
- Telemetry authority: `changes/active/service-identity-and-telemetry/spec.md`,
  approved revision 17, hash `6561fb1a747687f56a051edfeff23cc1f0e2375f`;
  current owning sections of `architecture/wyrd-design.md`.
- Original task:
  `changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`,
  hash `6846032793910170e76c0cc0ec9de2f50cb6ad6f`.
- Prior remediation and evidence:
  `changes/active/positioning/review/TASK-003-first-634e19cbc/TASK-003-R1-complete-schedule-reader-setup.md`,
  hash `6f3484e808b5015efa78abdf0c2b52863082fea5`.
- Original base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
  Prior candidate: `634e19cbc517293ed2458f30ce73401714f3447d`.
  Reviewed cumulative candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`.
- Independent diagnosis/ledger: sibling `findings-validation.md` and
  `verdict.md` in this directory; preserve the earlier review records.

These authority/task hashes pin working documents, not necessarily stored Git
objects. The maintainer explicitly approved the new specification revisions;
they govern the SDK additions despite the original task's older metadata.

## Diagnosis and selected correction

### FIND-TASK-003-3: Rust source omits the newly approved public sequence

`examples/refund-agent/refund_agent.rs:105–115` declares its state, starts
telemetry, executes the complete/skipped cases and force-flushes the returned
telemetry handle. Its entire main body contains neither `state.configure`
nor `state.flush`. Accessing the state client implicitly freezes defaults, so
the existing actual-program journey can pass without exercising those calls.

This is an explicit AC-016 tutorial omission, not broken trace completeness.
`sdks/wyrd-sdk-rust/src/otel.rs::Telemetry::shutdown` force-flushes finished
spans correctly. The native state does not own that Rust telemetry handle.
Python and TypeScript already show configure and state flush in their actual
programs. Passing verdict output does not prove the Rust setup that the
approved acceptance criterion specifically requires.

Use the existing `WyrdState::configure(ConfigureOptions::default())` after
declaration and before `otel::start_telemetry`. After the program's finished
spans have been exported, call the existing `state.flush().await` before
verifying the cases. Keep the current telemetry-handle force-flush: replacing
it with native state flush would lose the actual export barrier. Keep final
shutdown and existing error propagation. Reuse the public state/options
exports and update directly affected rustdoc and generated full-file embeds.

Two existing operations and their import close the explicit gap. No provider
ownership redesign, new SDK helper, alternate exporter or source-spelling
assertion is justified. Preserve model values, original spans, all three
verdicts, output captures and retained investigation behavior.

### FIND-TASK-003-4: TypeScript lacks the required missing-package unit case

AC-016 requires unit evidence for missing optional OpenTelemetry packages in
both Python and TypeScript. Python has a committed missing-extra case. The
committed `sdks/wyrd-sdk-ts/wyrd/tests/unit/telemetry.test.ts` eagerly loads
the installed peers and covers success, provider refusal, frozen context,
correlation and managed flushing. None makes an optional telemetry package
unavailable while calling public default configure. The two uncommitted
opt-out tests supplied by the maintainer cover another scenario.

Production `WyrdState.configure` freezes context then awaits its existing
private startup, which dynamically imports `./otel.js` and its optional peers
(`wyrd/src/index.ts:3358,3413–3425`). That source path credibly rejects an
unresolved package; the finding does not assert an incorrect runtime result.
It identifies missing explicitly prescribed negative proof for the new default
application-startup operation. Installed-package unit totals and Python import
tests do not exercise Node's loading boundary.

Add one focused TypeScript unit case using the existing Vitest and module
isolation tools: make one actual optional telemetry dependency unavailable,
call public default configure, and assert the rejected diagnostic identifies
that package. Exercise configure through module loading, rather than mocking
configure's result or calling the private installer. Preserve installed-peer,
existing-provider and frozen-state cases. Failed installation still leaves
configuration frozen and requires correcting code/dependencies and restarting;
do not add production retries, reset globals or unfreeze the state.

The smallest correction is one negative test in the existing unit ownership
area. No server, new framework/dependency, generalized loader abstraction or
production fallback is warranted. If the focused test exposes an actual
behavior defect, diagnose that observed result before changing production.

### FIND-TASK-003-2: two new items reopen the mandatory rustdoc gate

The original five locations are fixed; do not redo them. New public tuple
field `Trace(pub String)` at
`crates/shared/wyrd-client/src/authoring/verifier.rs:112` lacks its own field
rustdoc, and new lifecycle test `flush_drains_only_a_started_writer` at
`crates/shared/wyrd-client/src/observe/tests.rs:1794` lacks `# Panics` for its
expect/assert failures. Root AGENTS makes missing changed-item rustdoc
merge-blocking; the Rust guide includes fields and tests. Green compilation
and runtime tests do not waive it.

Document the existing tuple field as the trace ID value and add a concise
panic description to the existing test. Preserve visibility, signatures,
values, assertions and behavior. Two local comments suffice; hiding the field,
deleting assertions or refactoring lifetime ownership would cost more and
weaken or change unrelated behavior. Batch this gate correction with the two
substantive acceptance corrections above.

## Constraints and non-goals

Preserve default configure, telemetry opt-out, asynchronous TS configure,
frozen configuration after failed installation, existing-provider refusal,
ASGI ownership, writerless success and closed-state refusal. Keep the shared
canonical trace lowering, public marker names, wire/storage contracts, exact
attribution, native insufficiency judgments and original evidence intact.
No server, auth, storage, schema, migration, designated hot-path wait or
dependency change is authorized.

No process-global installer lock or Python Trace declaration relocation is
requested: independent validation rejected those proposals as blockers. Do
not assign them remediation work. Preserve closed FIND-TASK-003-1 and all
unaffected tutorial acceptance evidence.

The user has concurrent uncommitted followups in Python example/docs/rules and
opt-out tests, plus unrelated dirty planning files. They are separate
human-directed changes, not reviewer findings. Do not overwrite, revert or
silently count them as fixes in the reviewed commit. Do not commit unless
separately asked. Review the actual next cumulative candidate with its
applicable rules when those changes are integrated.

## Acceptance and focused proof

| Finding | Closure observation |
|---|---|
| FIND-TASK-003-2 | The two new items have the missing field/panic documentation; the original five fixes remain intact. |
| FIND-TASK-003-3 | Actual Rust tutorial explicitly configures before managed setup and flushes state after exporting finished spans; the existing real-server program journey still reproduces complete/pass, skipped/fail and no-trace/inconclusive plus investigation. |
| FIND-TASK-003-4 | One nonempty TS unit selection calls public default configure with an unavailable optional telemetry package and asserts the missing-package rejection; other setup/provider/freeze cases remain green. |

For the TS negative case, follow RED → GREEN → REFACTOR within the existing
unit runner. An already-correct production rejection may be verification-only;
do not manufacture runtime churn. Record exact selected test name, collection
count, command and observed diagnostic. A planned focused selector is:

```bash
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^configure reports a missing optional telemetry package$'
```

Confirm exactly one test runs; if the existing ownership/tooling requires an
equivalent name or file, record the repaired exact invocation. A zero-selection
exit is not proof. Run the affected telemetry unit file and narrow TS type/lint
checks for the final write set, without a full integration suite.

The existing Rust program test has this exact focused command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test refund_agent -P journey --run-ignored=all -E 'test(=refund_agent_fails_the_skipped_lookup)'"
```

This one affected journey is sufficient; do not rerun Python/TS or scheduled
minute-window journeys merely for the Rust call additions. Rustdoc-only changes
need source inspection and `mise run fmt:check`, not a new runtime test.
Regenerate embedded output through `mise run docs:generate`, then use
`docs:test`, `docs:check:commands` and `docs:linkcheck` for affected publication,
plus `git diff --check`. Avoid broad aggregates and full user-journey suites in
this remediation. Carry forward accepted evidence at the next review, checking
only closure, new diffs and affected paths unless new material evidence warrants
a specific expansion.
