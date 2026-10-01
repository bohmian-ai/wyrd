# TASK-009 Review Verdict — Round 1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Reviewed range: complete cumulative `base..candidate` diff

The candidate remained `HEAD` throughout discovery, follow-up, validation, and
verdict preparation.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Initial Card selection is shared, local, immutable, and projected through Rust, Python, and TypeScript | Shared `WyrdState::run_for_card` resolves before `Run::new`; language bindings delegate; focused tests cover root, selected, sibling, identity, and unknown alias paths | PASS |
| Python scopes enrich active and child spans, nest, survive `await`, and isolate ordinary concurrent asyncio tasks | PyO3 delegates to `wyrd.otel`; sequential/nested/async tests and the persisted journey exercise the healthy paths | PASS |
| Every directly entered concurrent scope uses the same private OTel context key | `_scope_key` has an unlocked lazy check/create; two concurrent first entries can attach under distinct UUID-backed keys | FAIL — `FIND-TASK-009-4` |
| Scope exit restores prior correlation and detach failure degrades to no enrichment | Normal detach restores, but a failed or internally swallowed reset leaves stale correlation; the current test quarantines that leak | FAIL — `FIND-TASK-009-3` |
| Required optional-telemetry failures are proven not to block explicit observations | Missing-package proof reaches Drift; registration, attach, processor enrichment, and detach cases do not all reach an explicit observation | FAIL — `FIND-TASK-009-1` |
| Real authenticated OTLP export persists trace/custom/Eval joins with managed identity | The extended Python journey uses stock OTLP/HTTP, explicit lifecycle barriers, and persisted Bifrost queries | PASS |
| Context exit is not a flush, shutdown, span lifecycle operation, or durability acknowledgement | Exit only delegates correlation cleanup; the journey separately flushes the provider, shuts down Bifrost, and waits for publication | PASS |
| Optional OTel remains optional; no second pipeline, wrapper span, server Run resource, client-authored managed identity, or log/metric promise appears | Production dependencies and server contracts are unchanged; Python uses the caller's provider and existing observation paths | PASS |
| Materially changed Rust documentation states the actual Run subject invariant | `Run::subject` still claims the root Service remains the subject until `for_card`, contradicting initial non-root construction | FAIL — `FIND-TASK-009-2` |

## Independent review results

| Review | Result | Material output |
|---|---|---|
| Behavior | PASS | No proposed findings |
| Invariants | FAIL | `INV-REV-001` |
| Repository standards | PASS | No material repository-rule findings |
| Maintainer | FAIL | `MR-001` |
| System resilience | FAIL | `SYS-001` |
| Telemetry domain | FAIL | `D-TEL-001` |
| Focused follow-up | RESOLVED | Confirmed the proof gap with narrowed authority wording |
| Structured Ponytail validation | Four retained findings | `FIND-TASK-009-1` through `FIND-TASK-009-4` |

The follow-up was required because the behavior and invariant reviews disagreed
on whether the fail-open evidence satisfied AC-032. It established that source
inspection supports independence today, but the approved criterion expressly
requires focused failure-to-observation proof. The Ponytail validator then
revised that proposal to remove an unnecessary per-call-site enrichment test.

## Validated finding ledger

| ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-009-1` | REVISED | MISSING | Pair registration/API-only, attach, processor-enrichment, and detach failures with one representative explicit observation in the existing focused test home. |
| `FIND-TASK-009-2` | CONFIRMED | VIOLATION | Correct the `Run::subject` field rustdoc to describe root, initially selected, and sibling-selected views accurately. |
| `FIND-TASK-009-3` | CONFIRMED | INCORRECT | Restore or neutralize the prior Wyrd correlation after detach/reset failure, including nested outer-scope restoration, without affecting application exceptions or lifecycle. |
| `FIND-TASK-009-4` | CONFIRMED | INCORRECT | Serialize lazy `_scope_key` creation with the existing module lock so concurrent first entries share one key. |

Full validation evidence and decision-complete correction boundaries are in
`findings-validation.md`.

## Verification limits

- The orchestrator independently ran `git diff --check` and the focused Python
  unit file; the latter passed all 30 tests.
- The telemetry reviewer independently reproduced `FIND-TASK-009-4` under the
  pinned OpenTelemetry 1.42.1 runtime.
- Discovery did not rerun Cargo-backed or Postgres-backed lanes concurrently.
  The task's recorded broader Rust, Python, TypeScript, codegen, boundary,
  lint, and persisted-journey results were inspected as supplied evidence, not
  treated as substitutes for source review.
- No prior remediation findings exist for this first review round.

## Verdict

**FIX_REQUIRED**

The four retained findings are bounded corrections within approved behavior;
none requires a new product, public API, architecture, security, compatibility,
concurrency-semantics, resource-ownership, or persistent-data decision.

Remediation task:
`changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`
