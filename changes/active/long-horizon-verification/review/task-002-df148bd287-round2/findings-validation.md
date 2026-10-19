# Structured Ponytail validation — TASK-002, remediation round 2

## Subject and independent method

Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
Candidate: `df148bd287d3e9fcae861470fe9c27bd576f2eeb` on `change/long-horizon-verification-task-002`.
HEAD independently matched the candidate during validation.

Reviewed the complete cumulative executable, declaration, fixture, documentation and test diff, with its task-evidence and historical-review additions; separately reviewed the prior candidate `5c421663b43d7deeca135134d6b07cf49a11b83f` to current delta. Read the original TASK-002, prior verdict and R1 task/evidence, subject, claim comparison, both implementation reports, canonical isolated standards report, supplementary standards report, maintainer, system and both domain reports, and current verification record. No discovery conclusion was treated as source proof. The supplementary standards report is not needed to establish independent coverage; the canonical isolated audit supplies that role after the orchestrator's disclosed independence correction.

Applied AGENTS.md, agent-rules, reference routing, spec-driven development, maintainer style, relevant contract/error/evaluation/drift guidance, and the active Verifier/Trigger design and doctrine. Acceptance is the user-approved revision-13 REQ-012, AC-007 and INV-001–005. Revision-14 draft content does not supply additional authority. CodeGraph preceded code navigation; its results were supplemented by complete changed bodies, callers and SQL/source inspection. This validator ran no runtime commands and changed only this report.

## Independent producer-to-consumer validation

| Boundary | Source evidence and conclusion |
|---|---|
| Authored binding and samples slots | `card/verifier/mod.rs` removes `ScheduledInput`, binding `inputs` and Samples `required`, while retaining `deny_unknown_fields`. `InputSlot::required` returns true for Samples and preserves ordinary JSON/media/trace booleans. Forbidden state is removed at its declaration owner rather than filtered downstream. |
| Shared decode and sibling callers | `envelope.rs::decode` uses the existing serde mechanism through the already-resolved path-diagnostic dependency. `spec_from_kind_value` dispatches every native kind through it; both Card deserialization and `Spec::from_kind_and_value` use the dispatcher. Loader parsing, effective-spec loading and registration therefore share refusal semantics. Existing top-level type preprocessing and External refusal remain unchanged. No second parser or removed-field blacklist is added. |
| Registration and activation | `EffectiveSpecs::{validate_bindings,validate_binding}` decode and validate effective references before projection; `check_activation` requires Samples. Removing `validate_schedule` does not admit mixed required evidence: existing `Validator::validate_evidence_mode` rejects Samples combined with other slot kinds, mixed Run/Window checks and Window preparation steps. Existing Operator/cron/ref checks remain. |
| Durable projection | `BindingProjector::{project,freeze}` retains the caller-owned TenantConn and exact subject, Verifier, Trigger and Operator identities. `project_bindings` removes the obsolete final SQL column/parameter together with its typed field. Migration `20261017000000_drop_verification_binding_inputs.sql` removes only that column; natural-key identity, RLS and transaction ownership remain. No replacement selector state appears. |
| Claim, retry and recovery | `CLAIM_SQL` removes only its selector lookup. `ClaimedRow::into_claimed` constructs Scheduled from persisted start/end and preserves exact Verifier UID/version, subject, binding, authority and fresh lease identity. Submitted inputs/routes retain their separate closed shape. Existing claim/retry/reclaim/settlement writers and lease fences are not replaced. |
| Exact declaration to every requirement | `VerifierRunner::resolve` reads/caches by tenant and immutable Verifier UID. `dispatch` passes that spec and the persisted window to `DriftEngine::checks`, which visits every `spec.drift_checks()` entry. The complete iterator has no binding-map membership filter. `score` retains requirement-keyed baselines and ObservationWindow derived from `run.card_uid` and the frozen window. |
| Missing data and infrastructure failure | Missing scored data becomes `DriftReport::unscored`, rather than absence of a requirement. Existing decision handling gives inconclusive. A scoring/read infrastructure failure propagates EngineOutcome through dispatch/execute to existing retry/terminal settlement, discarding the attempt's incomplete reports; it cannot return a reduced-subset successful judgment. Execution deadlines, SYSTEM read authority, baseline algorithms, outbox and Operator delivery remain existing owners. |
| First-class language consumers and proof | Rust/Python/TS scheduled-drift owners add exact two-requirement maps for 120/0.01 and 80/missing-error-rate cases through sample writes, flush, activation and result reads. Rust also bypasses SDK-local refusal with direct HTTP and asserts the stable registry code, field and subsequent Card absence. Python/TS check local field refusal and absence. Generated TS removes the same fields/types; valid fixtures and public guides use selector-free authoring. |

The existing complete workflow is the appropriate alternative to new machinery: the registration/projector/queue/runner/DriftEngine chain already owns identity, observations, windows, scoring and recovery. Deleting the duplicate selector representation and membership predicate is sufficient. The sole new production helper is stateless synchronous decode beside its existing dispatcher; a new struct, resolver, lifecycle or validation workflow would add unearned machinery. Test setup changes reuse the existing Watch/server owners and skip fitting only for Custom checks without fitted baselines.

## Prior findings: independently closed

| Stable ID and original classification | Producer, consumers and focused closure evidence | Disposition |
|---|---|---|
| FIND-TASK-002-1 — VIOLATION | `envelope.rs:752–781` now documents decode and dispatch intent, callers, path-bearing failures, native kinds, existing preprocessing and External refusal. Both fallible functions have substantive Errors sections. The complete prior-to-current diff changes only their rustdoc; their bodies are unchanged. Existing shared callers need no correction. The current focused decoder test passes in orchestrator evidence; this test supplements, rather than substitutes for, the direct documentation inspection. | CLOSED |
| FIND-TASK-002-2 — REGRESSION | `error.rs:2076` is the existing derive-backed metadata producer. Its remediation now directs authors to Verifier Samples declarations and preserves the existing verify/submit advice. `check_activation` retains the same refusal family/status and field. Derived HTTP/MCP/catalog projections share the corrected metadata; generated `errors.md:351` matches it. The existing actual HTTP branch at `pg_card_registration_route.rs:1079–1095` checks emitted samples guidance and excludes binding-input advice while retaining its stable-code and no-write assertions. Historical R1 RED/GREEN is consistent with the source delta; orchestrator independently reran the exact HTTP test successfully. | CLOSED |

Neither closure requires a new consumer guard. The first changes documentation at the shared decoder; the second changes the authoritative error metadata once. Sibling callers and generated consumers continue using those owners. No prior finding is reopened, renumbered or duplicated by caller.

## Final material finding ledger

**Independently validated empty ledger.** No CONFIRMED or REVISED MISSING, INCORRECT, DRIFT, VIOLATION or REGRESSION finding remains. No new FIND-TASK-002-3 is assigned. No remediation task or specification revision is recommended.

Every canonical discovery report proposed an empty material ledger. Independent source inspection corroborates that result; reviewer agreement alone was not the basis. No conflicting claim, untraced changed failure owner or common repeat-remediation cause requires a focused follow-up.

## Non-blocking and historical observations

- The deleted optional-omission journey asserted the behavior expressly removed by REQ-012. Replacement three-language scenarios require both judgments, including inconclusive missing evidence. This deletion is not a weakened proof of behavior the approved task retains.
- Original implementation evidence explicitly acknowledges no separately executed RED. This is missing historical process evidence, not a reachable present defect or a reason to manufacture history. R1 records the actual stale-guidance RED and corrected GREEN. No timeout increase, suppression or unexplained test weakening was used to conceal a failure.
- Raw HTTP JSON in the Rust SDK negative test and less exact local error-code assertions in Python/TS are test-convention notes. AC-007 expressly requires direct HTTP enforcement proof; the Rust branch checks the stable code and unknown-field location. No missing product behavior follows from those placement/assertion differences.
- The derived drift-monitoring reference retains old selector wording; stronger active design and approved REQ-012 explicitly supersede it. Also, `verifier_executions.rs:217–220` still describes the removed selector return in CLAIM_SQL rustdoc, although the actual query and typed claim no longer return it. These are inaccurate internal wording, without a demonstrated runtime or public-contract defect; they do not justify a style-only round under the skill's finding threshold.
- The cumulative whitespace check finds only an extra blank EOF line in a historical review report. It has no execution or public-contract consequence and is not a new material finding.

## Verification credibility and limits

The original task records focused scheduled SDK story runs (Rust 7, Python 10 and TS 10), SQL/server/runtime/CLI/MCP checks and broad static lanes. R1 records docs generation and a green docs:check after its production/doc commit. These remain attributed historical evidence. They are credible supporting evidence because the inspected tests assert the required observable outcomes and the latest remediation changes no decoder/runtime/SQL/scoring body, rather than because an implementer marked them PASS. They do not independently qualify current crash timing, load, platform behavior or every language runtime.

Current `verification.md` separately records the exact removed-field decoder test passing (1 selected), exact HTTP activation-guidance/no-write test passing (1 selected), Wyrd/Vala migrations/idempotence passing, fmt passing, and two focused served OpenAPI tests passing (2 selected). Postgres was runnable in the sandbox. The OpenAPI tests exercise the served Card/verification contract but do not explicitly assert every removed field; shared schema derives and source inspection supply that contract connection. No full journey suite or broad gate was run.

The first current docs:check invocation stopped during dependency installation with `unable to open database file`. The subsequent sanctioned full lane passed with writable pnpm/npm/uv caches while preserving default tool discovery and repository configuration. Current verification records generation/drift checks, 87 documentation tests, command/link checks, production build/search and a11y for 32 pages passing. Post-run tracked-source diff was empty. Thus this proof limit is resolved by the same gate passing, not waived using historical evidence. Two intervening tool-setup failures remain recorded separately and are not treated as verification passes. Earlier discovery statements that docs proof is unresolved or served OpenAPI proof is absent are superseded by these actual results.

## Assessment

**PASS.** Both prior findings are closed, the independently validated material ledger is empty, the cumulative behavior and exclusions satisfy the approved task, and available verification is credible within the explicitly bounded review scope. No remediation or new product/architecture decision is needed.

Skipped full journeys, broad gate and independent repetition of historical SDK/lint/codegen/recovery runs, as constrained. Remaining qualification risk is limited to those unrerun broader executions; no source-confirmed task defect remains.
