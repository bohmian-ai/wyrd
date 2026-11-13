---
id: TASK-007-R2
status: proposed
approved_spec_revision: 7
findings: [FIND-007-1, FIND-007-2, FIND-007-3, FIND-007-6]
---

# Finish publication regression proof

Route directly to `$wyrd-implement`. The production corrections are source-correct; this bounded task completes their required regression proof and fixes the new test's stable-error assertions. Do not redesign publication or restart the original task.

Approved specification: `changes/active/github-verifier-pipelines/spec.md`, revision 7.
Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Original base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Prior candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Reviewed remediation candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.
Acceptance context: prior sibling `007-deferred-evidence-r1-4d418df7c/` verdict, validation and R1 remediation; this directory's `findings-validation.md`, `followup-review.md` and `verdict.md`.

## Diagnosis and selected correction

### FIND-007-1 — Artifact proof stops at acceptance

`components/verification/service/evidence.rs::subscribers` now uses the existing `TenantEvidence::admit` before custody. The new `pg_deferred_evidence::artifact_evidence_is_admitted_before_it_is_accepted` rejects wrong format, missing path/version and oversized files and then accepts the valid Artifact for the same source/input. The prior immutable-invalid-acceptance defect is corrected in source.

The test ends at that receipt. It does not demonstrate the R1-required publisher Card-read denial on the publication path or a matching consumer successfully using the accepted Artifact. `pg_hosted_artifacts::artifact_reads_refuse_before_use` proves the shared admission owner, but cannot demonstrate that this producer's refusal leaves custody untouched or that deferred resumption delivers the accepted Artifact. These are explicit acceptance-boundary and producer-to-consumer proof gaps, not claims of a remaining production defect.

Complete the focused proof at the existing deferred-publication journey owner using its real registered Artifact and existing authorization fixtures. Publish with a caller allowed to publish for the Repository/subscriber but denied read of the Artifact; assert refusal before retention. Then use an authorized publisher for the same source/input and demonstrate valid acceptance and successful consumption by the matching execution, preserving its identity. Reuse the existing hosted Artifact authorization/declaration and deferred execution facilities; retain the current negative format/path/version/size cases. A direct admission-only test or a receipt-only success cannot substitute for the missing flow. No new storage, permission category or harness is needed.

### FIND-007-2 — delegated canonical-row proof is absent

`publish_evidence` correctly passes `caller.attribution()` to `DeferredEvidence::accept`; the unchanged Scribe path carries it to the retained row. The R1 Evidence explicitly records no runtime test for this boundary after unsuccessful credential-fixture attempts. Correct owner reuse establishes source closure, but does not supply the required regression assertion that a normal delegated publication retains the effective subject and verified outer actor.

Use the existing verified delegation/authentication facilities and registry relationships to credential a permitted publisher for the Repository/subscriber. Publish through the normal publication service and read the canonical accepted evidence row through the existing Bifrost facilities. Assert distinct effective subject and verified outer actor; keep ordinary undelegated principal/principal behavior. Correct fixture authorization at its existing owner rather than synthesizing unverified Caller fields or bypassing publication authorization. Do not modify the production attribution or delegation protocol. The correction is focused auth-fixture setup and retained-row assertions; unrelated delegated workflow coverage is unnecessary.

### FIND-007-3 — losing-offer receipt reconciliation has no regression assertion

The repaired error reconciliation in `publish_evidence` adds canonical raced acceptances to `held` and clears `fresh`, so receipt status/time come from the winner. The R1 Evidence explicitly records source-only proof. Existing sequential HTTP replay checks never exercise a locally offered timestamp that lost to another canonical acceptance.

Retain the source correction. Add deterministic owner-level proof of the actual production receipt reconciliation with an earlier canonical accepted timestamp and a distinct losing offered timestamp. Assert replayed status, canonical evidence identity and first-accepted time, with the existing real custody/fence and sequential HTTP replay tests establishing persistence and public receipt behavior. Exercise production-used receipt logic rather than a test implementation of that logic; if a local production boundary is needed for direct testing, keep it at the current publication owner and limited to the existing reconciliation/receipt responsibility.

This deliberately narrows the former interleaving mechanics while preserving the required canonical-receipt outcome. Holding Scribe stalls other progress, and ordinary concurrent requests may execute sequentially without exercising reconciliation. Do not add test-only production hooks, timing sleeps, probabilistic retry-until-race checks, a new concurrency harness, or a new locking/idempotency protocol. A deterministic local regression check plus the existing connected custody/replay proof is the smallest credible correction; source inspection alone does not protect the changed receipt branch.

### FIND-007-6 — refusal checks assert wording instead of stable codes

The new Artifact test's refusal matrix (`pg_deferred_evidence.rs:813–842`) checks HTTP status and `refused.to_string().contains(reason)`, violating `agent_docs/rust.md`'s explicit “Assert Wyrd error codes, not messages” rule. A wrong public code with the same status/wording could pass. The separate focused review confirmed this test-contract violation; no current production error misclassification is alleged.

Use the already parsed response and same-module code assertion pattern. Assert `WYRD_VERIFICATION_400_INPUT_INVALID` for format/path/version refusal and `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` for size refusal. Preserve status assertions and later valid acceptance. Response text can remain failure diagnostics; it must not identify the expected error. This is a local test correction with no new helper or catalog change.

## Preserved behavior and non-goals

FIND-007-4 and FIND-007-5 are closed: preserve the four unignored lifecycle tests and SQL name-based decoding. Preserve publisher and consumer authority checks, exact source/policy identity, durable acceptance before success, conflicts, unmatched behavior, wake-on-replay, no terminal reopening, original deadline and compute/claim ownership. Keep the approved attempt refund, single CLI `--file` behavior and ordinary restart AC-007.

No new test-only production hooks, dependencies, public APIs, PostgreSQL tables/payload mirrors, subscription or concurrency semantics, attribution parsing, persistence owner or general test harness. Do not refactor source-correct implementation beyond the minimum production-used receipt boundary needed to test FIND-007-3. No whole-task or SDK journey rerun is prescribed here.

## Focused acceptance and verification

| Finding | Required observable proof |
|---|---|
| FIND-007-1 | Publisher denied Artifact read is refused before custody; permitted publication at the same source/input succeeds and the matching consumer uses it successfully. Existing Artifact negative cases remain. |
| FIND-007-2 | Normal delegated publication's canonical accepted row records effective subject and verified outer actor; undelegated attribution remains principal/principal. |
| FIND-007-3 | Actual production receipt reconciliation rejects the losing offer's time in favor of canonical first-acceptance identity/time and replay status. Existing real custody/fence and sequential HTTP replay assertions remain passing. No schedule-dependent proof. |
| FIND-007-6 | Every Artifact refusal case asserts the exact stable error code and expected status; valid acceptance still proves refusals did not retain evidence. |

Use `mise`, the repository-managed `scripts/postgres/with-test-postgres.sh` lifecycle and `db:migrate:all:inner`. Run exact selectors for the changed Artifact publication case, the existing sequential publication/replay case, the new delegated-row case and the focused deterministic receipt test. Confirm actual test names/counts and record exact commands; a zero-test invocation is not proof. Run only the narrow affected regression cases and required formatting/lints for the write set; broad aggregates/full user-journey suites belong to change review. No need to repeat all four unchanged lifecycle cases solely for test-assertion edits.

Docker was inaccessible in this review, so connected tests were not freshly executed. This environment limitation is not the finding: the missing assertions are absent from candidate source. Record implementation/source/fresh runtime evidence separately and execute the focused connected cases in an environment with the existing Postgres lifecycle. No commit, push or Git configuration is authorized by this task.
