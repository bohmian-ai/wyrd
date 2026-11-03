---
id: TASK-001-R1
kind: remediation
status: implemented
spec: SPEC-local-init-cleanup
spec_revision: 1
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2]
depends_on: []
---

# Reuse the existing test log capture and complete changed-item rustdoc

## Authority and subject

- [Approved spec revision 1](../../spec.md).
- [Original TASK-001](../../tasks/TASK-001-local-setup-cleanup.md).
- [Verdict](verdict.md) and [independently validated findings](findings-validation.md).
- Original base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
- Reviewed candidate: `862a69b187c81a5395bc73b872695a7bb0150330`.
- This remediation is bounded by the root `AGENTS.md` reuse and rustdoc rules
  and `agent_docs/rust.md`; no spec revision is needed.

## Diagnosis

### FIND-TASK-001-1 — a complete test fixture was duplicated

The candidate adds `CapturedLogs` at
`crates/wyrd/wyrd-server/src/test_support.rs:288–332`. The new HTTP logging
test and health transition test install it on their current thread and inspect
captured text. Neither needs its cloneable sink or a guard separate from the
capture's lifetime.

`crates/wyrd/wyrd-testing/src/logs.rs` already exposes
`wyrd_testing::logs::LogCapture::install()` and `text()` with the required
thread-default DEBUG-and-above, no-ANSI capture workflow. The server already
has the dev dependency; existing server integration tests use this owner.
The test-support module's AppState/harness cycle concern does not apply to
this independent fixture. The pre-existing private capture in
`operators/keys.rs` is not a reason to add another implementation.

The new sink/subscriber/writer is therefore redundant machinery under the
explicit reuse rule. Its two consumers create an unnecessary second owner to
maintain. Documenting or shortening it leaves that cause intact.

**Correction:** delete the newly added CapturedLogs implementation and use
the existing public LogCapture directly in both new logging tests. Keep the
test scenarios, assertions, subscriber lifetime and current-thread execution.
This is two consumer substitutions and deletion, without a new dependency,
feature, fixture, wrapper or test harness. Do not migrate unrelated private
captures.

### FIND-TASK-001-2 — mandatory documentation is incomplete

The new `GatewaySummary::serialize` at
`crates/wyrd/wyrd-cli/src/init.rs:386` propagates the generic serializer's
error but has no `# Errors`. The new summary regression at `init.rs:1913` and
the transition regression at
`crates/wyrd/wyrd-server/src/components/health/mod.rs:677` assert failure
conditions but have no `# Panics`. Added capture writer methods have further
error/panic omissions; removing that duplicate under the first finding closes
those locations.

These are explicit merge-blocking changed-item rustdoc obligations. Passing
tests do not establish compliance. The source-proven correction is bounded
documentation, not another runtime feature or a general documentation audit.

**Correction:** document serializer-error propagation and the two surviving
tests' panic conditions with the required headings. Keep implementation and
assertions unchanged. Bundle this work with the fixture reuse; no new test or
style-only follow-up round is needed.

## Preserved behavior and non-goals

Carry forward the accepted current URL, quiet readiness 503s, readiness
transition logs, ordinary-request error logs, tenant gateway reads,
configured/empty/unknown distinctions, JSON shape, checklist and persisted
configuration behavior. Do not weaken assertions, add skips/exemptions, change
timeouts, broaden production edits or execute TASK-002. No dependency, grant,
public interface, provider call or durable state change is authorized.

## Ordered closure scenarios

### Scenario 1 — required logging regressions reuse the shared owner

**Behavior.** Both logging tests use the existing public capture with their
original assertions, readiness status checks, ordinary-error control and
transition diagnostics intact.

**RED.** Existing runtime tests already pass; do not fabricate a failing
runtime assertion. The source acceptance is currently red because the added
CapturedLogs owner and both callers duplicate the available workflow.

**GREEN.** Delete the duplicate and reuse LogCapture at those callers. Run the
exact focused command below and confirm both original tests remain green.

**REFACTOR.** Stop at the deletion/reuse change; no wrapper, dependency or
unrelated capture migration is justified.

### Scenario 2 — surviving new items satisfy the mandatory rustdoc rule

**Proof strategy.** Static review is sufficient. Confirm serializer Errors
and test Panics sections accurately describe their complete bodies and that
deleting the duplicate closes its writer omissions. Do not add prose-pinning
tests or change behavior to make documentation easier.

## Acceptance criteria

| Finding | Required closure observation |
| --- | --- |
| FIND-TASK-001-1 | Added CapturedLogs is gone; both new logging regressions use the existing LogCapture owner and pass with their assertions intact. No dependency/feature or alternate fixture is introduced. |
| FIND-TASK-001-2 | The surviving serializer and two new tests have accurate required Errors/Panics sections; duplicate writer omissions disappear with deletion. No unrelated documentation rewrite or exemption is added. |

## Focused verification

For both changed capture consumers:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::router::tests::readiness_503_is_not_logged_as_a_failure) | test(=components::health::tests::readiness_transitions_are_logged)'
```

Inspect the three surviving rustdoc blocks against their bodies. No new
runtime proof is required solely for those prose edits; the previously passed
gateway-summary behavior proof carries forward when its body is unchanged.
Run `mise run fmt`, `mise run lints` and `mise exec -- git diff --check` for
the candidate. Do not require full SDK/journey suites or the broad gate for
this bounded remediation. If the correction expands into production behavior,
stop and establish the affected scope rather than broadening this task.

Record the new immutable candidate and closure evidence, then return to
`$wyrd-task-review` with this original review retained. Implementation,
commits and pushes remain separate caller-directed actions.

## Implementation evidence

Candidate: `050057ff4a0556db2669385e95bec289828df49a` (on reviewed `862a69b18`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `CapturedLogs` deleted from `wyrd-server/src/test_support.rs`; `http/router.rs` and `components/health/mod.rs` tests call `wyrd_testing::logs::LogCapture::install()` with assertions unchanged | focused nextest command below: 2 passed | PASS |
| FIND-TASK-001-2 | `# Errors` on `GatewaySummary::serialize` (`wyrd-cli/src/init.rs`); `# Panics` on `gateway_summary_distinguishes_empty_and_failed_reads` and `readiness_transitions_are_logged`; duplicate writer methods removed | static review against bodies; `init::tests::gateway_summary_distinguishes_empty_and_failed_reads` passed (body unchanged) | PASS |

Commands:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::router::tests::readiness_503_is_not_logged_as_a_failure) | test(=components::health::tests::readiness_transitions_are_logged)'
mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=init::tests::gateway_summary_distinguishes_empty_and_failed_reads)'
mise run fmt && mise run lints && mise exec -- git diff --check
```

No dependency, feature, wrapper, production behavior or unrelated capture
migration was added; `operators/keys.rs` capture untouched; TASK-002 not started.
