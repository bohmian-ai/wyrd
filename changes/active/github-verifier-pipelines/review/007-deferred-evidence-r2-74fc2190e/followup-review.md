# Focused independent source validation: Artifact refusal assertions

**Result: RESOLVED — CONFIRMED.**

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Prior candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b` (HEAD verified unchanged).
Range inspected: `git diff 4d418df7c 74fc2190e -- crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs`.

This fresh reviewer independently validates only the newly proposed stable-error assertion violation. It does not re-review original FIND-007-1..5 closure, restart discovery, or prescribe new production hooks. The original task, prior remediation, and `scope.md` supply acceptance context; they do not override repository rules.

## Source and authority coverage

CodeGraph was used before source navigation; the second targeted exploration exposed `subscribers` and the full `nonconforming` mapping. The complete new `artifact_evidence_is_admitted_before_it_is_accepted` function was inspected at `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs:777–857`, including declaration setup, all refusal cases, and later valid acceptance. Root AGENTS directs Rust reviews to `agent_docs/rust.md`. There is no nearer server test AGENTS overriding that guide.

`agent_docs/rust.md:257` states: **“Assert Wyrd error codes, not messages”**. The task-review skill preserves explicit mandatory repository rules even when a runtime exploit is not alleged. This is a concrete test-contract requirement, not a preference about assertion spelling.

## FOLLOW-007-R2-1

- **Validation:** CONFIRMED.
- **Classification:** VIOLATION.
- **Changed location:** `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs:813–842`, specifically `:838–841`.
- **Violated obligation:** `agent_docs/rust.md:257`, stable Wyrd error-code assertions.
- **Source evidence:** The new refusal matrix carries `reason` strings `format`, `no such file`, `reports`, and `64-byte`. Each real publication request checks only its HTTP status and `refused.to_string().contains(reason)`. No refusal case asserts `refused["code"]`. The later success assertion does not cover refusal codes.
- **Observable consequence:** The regression proof depends on human wording and would still accept a wrong public error code paired with the same HTTP status and reason. Conversely, changing valid wording can break the test without changing the error contract. This leaves the agent-facing refusal identity unprotected at the newly tested boundary.
- **Materiality:** Retained because the changed test directly violates an explicit mandatory repository rule. No current production misclassification or security exploit is claimed; the finding is the proof's noncompliance and missing stable-contract assertion, not a speculative runtime bug.
- **Existing owner and siblings:** `components/verification/service/evidence.rs::nonconforming` at `:358–375` maps `EngineError::InputTooLarge` to `WyrdError::VerificationInputTooLarge` and ordinary admission invalidity to `WyrdError::VerificationInputInvalid`. The catalog at `crates/wyrd-spec/src/error.rs:2130–2151` supplies `WYRD_VERIFICATION_400_INPUT_INVALID` and `WYRD_VERIFICATION_413_INPUT_TOO_LARGE`. The same test module already asserts `refused["code"]` for existing invalid-input publications at `:297`, as well as unmatched and conflict codes at `:256` and `:362`. The JSON response and existing assertion form already support the correction.
- **Smallest safe correction:** At this existing refusal matrix, assert `WYRD_VERIFICATION_400_INPUT_INVALID` for wrong format, missing path and missing Artifact version, and `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` for the over-limit file. Keep the HTTP status checks and subsequent valid acceptance proving earlier refusals did not retain evidence. Replace wording-based identity checks; no new helper, dependency, fixture, error variant, production hook or public contract is needed. Payloads can remain assertion diagnostics.
- **Correction burden:** A local test-only assertion change. Deleting the refusal cases would weaken required proof; leaving status-only checks would not satisfy the explicit code rule. Reusing the already parsed response and nearby assertion pattern is sufficient.
- **Focused closure proof:** Source inspection establishes exact stable-code assertions in all four cases. Run the one existing `pg_deferred_evidence::artifact_evidence_is_admitted_before_it_is_accepted` test through the repository-managed Postgres lifecycle when Docker is available. No broad suite or new harness is warranted for this correction.

## Verification limits

Source and immutable diff inspected only; no Cargo, Docker, runtime test, source edit, commit, push or Git configuration was performed by this reviewer. Connected execution remains subject to the orchestrator's recorded Docker restriction. The source finding does not depend on interpreting that environment failure as a product failure.
