# TASK-002 cumulative R4 review verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Original task base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Cumulative candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Prior remediation tasks: `review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md` and `review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md`
- Prior review history: `review/TASK-002-r1`, `review/TASK-002-r2`, and `review/TASK-002-r3`

The complete original-base-to-candidate range was reviewed. Candidate commit
`375d97e67f3affe0d5c59727ef3135b22a459140` is included and was audited even
though the human removed FIND-TASK-002-11 from the R3 remediation task after
that implementation appeared. HEAD remained the requested candidate throughout
discovery, follow-up, validation, and candidate-bound verification. Only this
R4 review directory was written.

## Independent review results

| Required review | Result | Report |
|---|---|---|
| Behavior implementation | FAIL | [task-review-behavior.md](task-review-behavior.md) |
| Invariant implementation | FAIL | [task-review-invariants.md](task-review-invariants.md) |
| Repository standards | FAIL proposal; verification proposal rejected by final validation | [standards-review.md](standards-review.md) |
| Maintainer | PASS | [maintainer-review.md](maintainer-review.md) |
| System resilience | PASS | [system-review.md](system-review.md) |
| Registry durability/concurrency domain | PASS | [domain-review-registry-durability.md](domain-review-registry-durability.md) |
| Tenancy/security domain | PASS | [domain-review-tenancy-security.md](domain-review-tenancy-security.md) |
| Workflow runtime/lifecycle domain | PASS | [domain-review-workflow-runtime.md](domain-review-workflow-runtime.md) |
| Focused follow-up | RESOLVED | [followup-review.md](followup-review.md) |
| Structured Ponytail validation | COMPLETE; one retained finding | [findings-validation.md](findings-validation.md) |

Every required discovery reviewer returned a complete report. The security
reviewer's role prohibited direct filesystem edits, so its complete returned
report was preserved verbatim at the assigned path. The focused follow-up was
required because discovery reports materially disagreed on malformed Workflow
selector closure. A fresh validator independently traced every proposal and
produced the final ledger.

## Reconciled acceptance matrix

| Obligation | Cumulative implementation and proof | Result |
|---|---|---|
| Native Workflow/Agent/Prompt graph and runtime reuse | Existing loader, Cards traversal, Skald body resolver/binder/runtime, and thin SDK projections serve local, mixed, registered, exact, UID, and after-v2 paths. | PASS |
| Pure and resolved validation before dispatch or writes | Loader/Skald validation and server `EffectiveSpecs` reject malformed graphs and bindings without provider dispatch or partial registration. | PASS |
| Sibling/external provenance and exact locked graph | Separate body stores preserve provenance; registered reads require Active exact identities and UID-bearing relationships. | PASS |
| Composite registration, original version intent, audit, transaction, UID/lifecycle fence, replay and recovery | Existing registration owners retain authored intent, tenant transaction, canonical audit, exact UID recheck/locks, idempotency and post-commit recovery. Focused server/SQL tests pass. | PASS |
| Public Rust/Python/TypeScript loading and Native execution journeys | All three real SDK journeys pass and retain credential, authorization, inactive dependency, mixed selector, pinning and Native result assertions. | PASS |
| Exact per-language Workflow→Agent and Agent→Prompt proof | Each journey now compares all three exact Agent/Prompt spec refs and server-derived relationship targets. | PASS — FIND-TASK-002-7 closed |
| Public TypeScript JSON/run contract | Recursive JSON input and complete native run/step/error snapshot remain aligned and type-checked. | PASS — FIND-TASK-002-8 closed |
| Struct-centered ownership, async filesystem boundary, rustdoc and import rules | Hydrator, `EffectiveSpecs`, `RegistrationWriter`, Workflow facade and foreign-runtime wrappers retain cohesive owners; filesystem work uses Tokio's installed blocking pool. | PASS — FIND-TASK-002-5/-6/-9/-13 closed |
| Generic Python registry selector errors | Generic selectors use request `Validation`; Workflow-specific selectors and genuine Data validation keep their owners. | PASS — FIND-TASK-002-12 closed |
| Malformed Workflow selector stable semantics | Python and several Rust/TypeScript cases use `WorkflowInvalidCardRef`, but TypeScript collapses invalid individual fields to `selector`; derived `VersionBlock` deserialization admits ranges that Rust/TypeScript send to registry IO and report as a Registry error. | **FAIL — FIND-TASK-002-11** |
| No Workflow principal, Workflow-root WyrdState, new transport/parser/cache/executor, registration-time execution, secret resolution, or TASK-003/004/005 behavior | Prohibited and later-task behavior remains absent. | PASS |
| Generated contracts, typing, boundary checks, workspace feature union and cumulative patch hygiene | Candidate-bound R4 runs pass codegen, Python/TS typing, N-API, client/PyO3/transaction boundaries, Hakari, formatting and cumulative diff hygiene. | PASS; REPO-R4-01 rejected |
| Human standard-mechanism direction | Candidate and remediation use existing owners and ordinary Serde, Tokio, PostgreSQL, SDK and test mechanisms. No unsupported bespoke check, file, setting, option, framework or dependency is accepted or required. | PASS |

## Follow-up and validated ledger

[claim-comparison.md](claim-comparison.md) records why a follow-up was needed.
The follow-up proved that the behavior and invariant proposals share one source
and preserve the prior stable ID. Independent Ponytail validation retained only:

| Stable finding | Status | Classification | Required outcome |
|---|---|---|---|
| FIND-TASK-002-11 | REVISED | INCORRECT | Restore validated `VersionBlock` deserialization and field-specific TypeScript selector conversion so malformed inputs refuse locally with the Workflow-owned error and exact offending field. |

The standards evidence proposal `REPO-R4-01` was rejected because
[verification.md](verification.md) supplies all cited final candidate-bound
passes. No other discovery proposal survived source validation.

## Prior-finding closure

FIND-TASK-002-1 through -10 and -12 through -15 are closed exactly as recorded
in [findings-validation.md](findings-validation.md). FIND-TASK-002-3 remains
superseded under Revision 12 and its removed keyed-normalization machinery must
not return. FIND-TASK-002-11 is only partially closed by `375d97e67`: Python,
wrong-kind, versionless, and invalid-shape cases use the selected Workflow
error, while TypeScript field attribution and Rust/TypeScript invalid exact
version refusal remain open.

## Verification evidence and limits

[verification.md](verification.md) records independent candidate-bound passes
for the focused loader/client tests, three server registration and UID tests,
the SQL lifecycle race, all three real SDK journeys, Python selector/data
tests, generated contracts, type/N-API checks, client/PyO3/transaction
boundaries, Hakari, Rust formatting, and cumulative patch hygiene.

These runs directly cover the remediated seams but do not constitute an
independent rerun of every broad aggregate claimed in implementation evidence.
That limit does not create another finding. Existing green tests cannot close
FIND-TASK-002-11 because one server assertion currently expects the inconsistent
range-version registry path and the TypeScript journey does not inspect malformed
individual-field attribution.

## Verdict and handoff

**FIX_REQUIRED.** One bounded correction remains within the approved behavior;
it needs no specification, architecture, public API, security, concurrency,
resource-ownership, or persistent-data decision.

Remediation: [TASK-002-R4-close-workflow-selector-validation.md](TASK-002-R4-close-workflow-selector-validation.md), ready for `$wyrd-implement`.
Subsequent review must reassess the complete original-base-to-new-candidate
range against the original task and all remediation history.
