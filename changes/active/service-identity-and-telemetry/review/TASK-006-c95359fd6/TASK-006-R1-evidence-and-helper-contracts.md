---
id: TASK-006-R1
kind: remediation
status: implemented
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
requirements: [REQ-045, REQ-046]
depends_on: [TASK-006]
parent_task: TASK-006
remediates: [FIND-TASK-006-3]
---

# Document panic invariants on three test helpers

Route directly to `$wyrd-implement`. Approved specification: `changes/active/service-identity-and-telemetry/spec.md` revision 16. Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-006-verification-trace-evidence.md`. Review base: `5fe833547c66916c8c4868afc8e07d985dcba7d7`; reviewed candidate: `c95359fd608fa65d4f0422820751490f61c99202`. Source diagnosis: sibling `findings-validation.md`, FIND-TASK-006-3; current disposition: sibling `verdict.md`, “Maintainer disposition.”

The only remaining remediation is adding required panic documentation to three test helpers. The review found no runtime defect. The maintainer waived FIND-TASK-006-1 and accepted the retrospective independent validation of the four test corrections for FIND-TASK-006-2; neither requires further work. Preserve the original review reports and truthful implementation chronology.

## Diagnosis and correction

Root AGENTS and `agent_docs/rust.md` require `# Panics` documentation on changed Rust items that can panic, including private test helpers. Three new helpers omit the panic invariants inherited from their existing callees:

| Helper at reviewed candidate | Existing panic source | Required documentation |
|---|---|---|
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs::trace_batch` (line 5684) | Calls `canonical_signals::trace_spans`, which delegates to canonical batch assembly | Describe the schema/cell compatibility and batch assembly invariant. |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs::write_spans` (line 2351) | Calls the same canonical span producer; its existing `# Errors` does not cover panics | Describe the inherited schema/batch assembly invariant. |
| `crates/vala/vala-verify/src/blocks/engine/tests.rs::shared_trace_verifier` (line 1763) | Calls existing `verifier`, which expects successful fixture deserialization and validation | Describe the valid Verifier fixture invariant. |

Read each helper and its callee's full panic contract, then add a truthful `# Panics` section to the helper's existing rustdoc. The smallest correction is documentation at these callers; retain their current code and error handling. No new guard, error type, helper, test or harness is needed.

## Constraints and acceptance

Limit the implementation diff to these three rustdoc additions. Preserve test assertions, fixture assembly, production behavior, query authorization, trace sharing and queue semantics. Do not recover historical RED evidence, commission another diagnostician, or change original review reports.

FIND-TASK-006-3 closes when all three helper rustdocs describe their existing panic invariants accurately and the affected Rust formatting and diff checks pass.

## Focused verification

Use static inspection of the three helper bodies and their existing callees as proof. This is a documentation-only correction, so runtime tests and RED/GREEN execution add no relevant evidence. Run:

```bash
mise run fmt
git diff --check
```

Record the three updated locations and check results in the remediation evidence. No server build, database setup, journey suite, typecheck or broad aggregate is required for these rustdoc edits.

## Remediation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-006-3: `trace_batch` documents its inherited panic | `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs::trace_batch` `# Panics` (schema must assemble the rows, as `canonical_signals::trace_spans`) | static: body only calls `canonical_signals::trace_spans`, whose `# Panics` names the same invariant | PASS |
| FIND-TASK-006-3: `write_spans` documents its inherited panic | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs::write_spans` `# Panics` beside the existing `# Errors` | static: every other fallible call returns through `?`; the one panic source is `canonical_signals::trace_spans` | PASS |
| FIND-TASK-006-3: `shared_trace_verifier` documents its inherited panic | `crates/vala/vala-verify/src/blocks/engine/tests.rs::shared_trace_verifier` `# Panics` (fixture must be a valid Verifier, as `verifier`) | static: body only calls `verifier`, which `expect`s deserialization and validation | PASS |
| Diff limited to three rustdoc additions; formatting and whitespace clean | 13 added doc lines across the three files; no code change | `mise run fmt` (exit 0); `git diff --check` (clean) | PASS |

Non-goals stayed excluded: no test, assertion, fixture, production, or
review-report change; no RED recovery or diagnostician run.
