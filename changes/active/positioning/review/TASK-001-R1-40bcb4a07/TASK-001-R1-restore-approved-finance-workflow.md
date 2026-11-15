---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6]
---

# Restore the approved finance onboarding workflow

## Outcome, authority and subject

A reader runs the existing Python/Rust/TypeScript finance example, sees why a release is simulated or held, and can inspect the exact policy behind its judgment before and after a policy update. Accomplish this with the approved PDF workflow and existing SDK contracts.

- Approved spec: `changes/active/positioning/spec.md`, SPEC-positioning rev 4 at reviewed candidate.
- Original task: `changes/active/positioning/tasks/TASK-001-positioning-and-finance-onboarding.md` at reviewed candidate.
- Review base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
- Prior candidate: `40bcb4a072dcbb33bee734f9b1c3ff0f63cf0718`.
- Diagnosis authority: sibling `verdict.md` and `findings-validation.md`; preserve their findings/IDs for later closure review.
- Route: `$wyrd-implement`; no implementation is authorized by this review artifact itself beyond the normal caller's implementation instruction.

Uncommitted positioning task/design edits existed during review and are not reviewed implementation; preserve them and reconcile applicable current instructions before working. Do not overwrite user changes or silently amend the specification. Revision 4 does not require a live-provider walkthrough or published-package availability proof.

## Diagnosis and selected corrections

### FIND-TASK-001-1: show the application's decision and reasons

At `finance_release.py:11`, `.rs:25` and `.ts:11`, each actual program prints only aggregate verdict/report text. Requirements/reasons returned by the existing result are discarded, and verification errors propagate without an explicit hold/code demonstration. Quickstart embeds this incomplete flow; current Rust/Python/TS stdout assertions lock it in. The displayed Python API's pass/hold branch does not supply the required actual-program behavior on all three languages. REQ-004/AC-001–002 and Scenarios 1–2 explicitly require this outcome, so safe process exit alone does not close the gap.

At the existing program owners, use authoritative existing verdict fields to simulate release only for passed. Print hold for failed/inconclusive and show available requirement outcomes/reasons; handle verification request/execution errors as hold with native stable error information while preserving error/verdict distinctions. Update source-backed output and parity assertions together. Decisions belong to the application consumer because the shared producer already returns correct judgments/errors. Reuse the existing API decision pattern; no generic error middleware, SDK wrapper, real transmission or new vocabulary is needed. Main reader path remains two releases; other negatives stay in focused proof.

### FIND-TASK-001-2: restore the PDF evidence path

`finance_service.py:40–50,73–76`, `.rs:40–60,70–76`, `.ts:16–27,45–47` create a Markdown file, publish without extraction and mount it raw; the former PDF asset is deleted. FinanceFlow and generated entry text still call it PDF. This misses explicit REQ-002/003/005 and AC-003/004, rather than exposing a missing runtime capability.

Restore the synthetic `company-sharing-policy.pdf` asset. Use the existing Artifact extraction/declaration owner and select its exact stored `extracted/company-sharing-policy.pdf.json` file in the existing workspace. The base finance program and Artifact authoring tests already demonstrate this mechanism. Remove automatic Markdown substitution; align judge prompts, reader asset instructions, fixtures, source embeds and generator-owned output. Preserve gateway/tool grants and registry version selection. Relabeling the visual alone cannot satisfy the approved PDF obligation; new extraction/storage/preparation machinery is unnecessary.

### FIND-TASK-001-3: remove public SDK scope drift

Superseded by spec revision 5 (D2 in `positioning-remediation.md`): the additions are approved and kept; only their rustdoc (R10) remains.

New helpers at `wyrd-client/src/authoring/verifier.rs:123–168`, multi-slot positional behavior at `verify/mod.rs:333–354`, result methods at `wyrd-spec/src/verification/execution.rs:575–595`, Python result wrappers/stubs and TS schema/result additions introduce public behavior forbidden by INV-001 and the spec's no-SDK-contract-change boundary. Existing named inputs, JSON schemas and verdict comparisons support the complete finance story.

Remove these task-added public contracts and adapt their example/API consumers to existing named authoring/invocation and verdict fields. Preserve pre-existing Python model-class authoring and its nested/recursive schema handling, TS TableConfig schema-source support, typed-result decoding, unknown/wrong-kind input validation and native missing named-slot behavior. Restore the prior private owner or retain only private behavior necessary for those base capabilities; do not replace the removed conveniences with another public abstraction. Remove only dedicated assertions/stubs for deleted features, keeping generic regressions. This bounded deletion/restoration avoids a new specification decision and permanent compatibility burden; retaining the new APIs requires separate approval.

### FIND-TASK-001-4: demonstrate policy evolution and execution attribution

Quickstart `:110–132` and `stored-verdicts.sql:2` expose summary IDs/verdict but no direct route from the returned execution to its exact policy/Verifier. Rust's policy test (`examples/tests/finance_release.rs:441–479`) appends inert `Revision 2.`, expects unchanged output and inspects declarations rather than execution details; attachment labels alone do not identify an Artifact. Python tests explicit-version mutation refusal; TS lacks policy-update proof. Server pinning is present, but the required AC-004/005 and Scenario 3 demonstration is absent.

Expose the existing execution ID and directly link the short finance inspection/update steps to existing public `Execution.details` inspection. This supported linked procedure suffices; do not add result fields or dump full provenance into normal output. Use existing details' exact Verifier/subject and evidence Artifact ref/path to connect policy citations to the judged file. Extend existing focused language journeys with a substantive synthetic policy change and controlled expected judgment, proving two original requests reuse one selection, updated declarations select new exact policy/Verifier/Service identities, and old declarations/retained executions retain old selection. Reuse normal version selection and existing details/provider fixtures. Any bounded eventual-publication handling belongs only to explicit inspection/tests, never the verification request. A new history API/store, version algorithm or synchronous durability wait would add unnecessary scope.

### FIND-TASK-001-5: supply release-purpose evidence

All three Release shapes/prompts (`finance_service.py:34–37,61–63,75`, `.rs:34–37,70–72,119–121`, `.ts:9–13,35–37,46`) omit purpose, although REQ-002 explicitly requires it and the policy limits external sharing to external-audit purposes. Fixtures pass based on absence of a bank marker without supplying that fact. This is a source-proven evidence omission, not a claim about live-model behavior.

Add concrete synthetic purpose to the existing release evidence and bind it into the existing policy Agent prompt. Update affected constructors, API input and provider fixtures so the permitted case supplies and observes external-audit purpose together with selected policy. Keep three requirements and existing named-slot/native missing-evidence semantics. One field/binding closes the gap; do not add a policy engine, new requirement or infer an unprovided purpose.

### FIND-TASK-001-6: meet the rustdoc gate on retained code

New fields at `finance_service.rs:15,21–22,28–29,35–37` and nested schema test types/fields at `authoring/tests.rs:1955–1963,1997–2005` lack the mandatory rustdoc required for every changed item. Enclosing-item docs and passing Clippy do not waive that explicit gate.

Document retained new/changed items concisely with their intent and evidence role, including required error/panic documentation where applicable. Items deleted while removing unapproved APIs need no replacement docs. Batch this static correction with the behavior fixes; no runtime test or new documentation checker is required.

## Ownership and preserved boundaries

Primary owners are existing finance declarations/programs/API, their focused Rust/Python/TS test owners, affected SDK authoring/projection/result code being restored, source-backed README/docs/homepage consumers and existing docs generator. Trace sibling callers before deleting contracts and update coupled stubs/types/manifests/fixtures as needed. Shared clients remain the sole transport/validation owners; Artifact publication and Execution.details retain their existing responsibilities.

Preserve accepted opening/CTAs, brand/routes, six sections, motion controls, reduced motion and asynchronous capture story. Preserve typed errors, provider disclosure, supplied recipient/destination facts, exact old policy attribution, generic integrity/refusal coverage and no real report transmission. Do not add dependencies, migrations, runtime behavior, security changes, public APIs, benchmarks exceptions or edits to architecture. Stop with SPEC_REVISION_REQUIRED if satisfying current instructions truly needs a new boundary decision. Retaining Markdown/new SDK APIs is outside this selected correction.

## Acceptance and focused proof

| Findings | Required observation |
|---|---|
| 1 | Each actual program explicitly simulates release only on passed; all non-pass and native verification-error cases hold with outcomes/reasons or stable code, and independent recipient/policy/destination negative results remain distinguishable |
| 2 | Each taught language publishes the PDF plus stored extraction and the gateway policy judge actually reads/cites that selected extraction; rendered/source/generated claims agree |
| 3 | Added public contracts and expanded positional semantics are removed; pre-existing named inputs, Python model authoring, TS TableConfig schemas and typed results remain correct |
| 4 | Existing linked public inspection identifies exact policy file/ref, Verifier and subject for old/new results; reuse, substantive update result and old attribution are proved on the presented language paths |
| 5 | Actual policy judgment receives the concrete synthetic purpose from all three programs/API evidence |
| 6 | All retained changed Rust items satisfy the explicit rustdoc rule |

For executable corrections, follow ordered RED/GREEN/REFACTOR: first decision/error and defining negative branches; then actual PDF read plus purpose; then policy update/inspection, retaining previous proof. Contract deletion/restoration uses focused regression proof; prose/rustdoc/generated text uses static/source/regeneration proof rather than manufactured RED. Confirm any newly added selectors are collected and fail for the diagnosed gap.

Existing focused commands (selectors confirmed in reviewed source; revalidate if changed):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_passes_and_fails) | test(=finance_release_holds_uncertain_releases) | test(=finance_release_policy_versions_remain_attributable)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_insufficient_authority tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_changed_content_and_broken_references tests/integration/test_docsite_quickstart.py::test_finance_api_releases_only_passed_reports"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/finance-release.test.ts -t '^(runs the finance release Quickstart from a reader.s directory|judges each finance requirement on its own)$'"
mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts
```

Also select only existing authoring/projection/typed-result regressions affected by SDK restoration; record exact selectors from source without requiring tests for deleted conveniences. Find owning mise tasks before commands. Run narrow retained-write-set formatting/lint/type checks; regenerate coupled agent text with `mise run docs:generate` and run `mise run docs:check:commands` plus `mise run docs:svelte:check` for affected rendering. Source-inspect rustdoc and compare restored public behavior against base. Avoid concurrent Cargo operations.

Do not run full journey suites, complete docs/SDK aggregate gates or unrelated tests for this remediation; integrated change review owns those. Record actual commands, collection/results, source-static proof and limits against the new immutable candidate. Later task review checks these six IDs, the remediation diff and affected consumers while carrying forward accepted unchanged behavior.
