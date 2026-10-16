# Focused follow-up: Custom selected-window completeness

**RESOLVED.** DRIFT-D1 describes a reachable TASK-002 acceptance gap. The invariant review's REQ-014 PASS is supported for unchanged numerical algorithms, but does not establish preservation of existing incomplete-window behavior. The two claims concern different layers of the same path.

## Subject, scope, and authority

Base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`; candidate `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`, rechecked unchanged. Read the frozen subject, original task, specification r6, cumulative diff, and the two conflicting discovery reports. This pass investigates only Custom completeness and its ownership, not other findings.

Applicable authority inspected: AGENTS.md, agent rules, reference router, spec-driven-development and maintainer-style references, drift-monitoring reference, and relevant Wyrd design/doctrine sections. The approved revision replaces the old authored implementation-kind shape; that does not replace incomplete-evidence semantics.

- Approved REQ-014 (`spec-snapshot.md:658-662`) retains insufficient-data behavior as well as algorithms, thresholds, sample floors and baseline readiness.
- INV-006 (`spec-snapshot.md:791-794`) forbids insufficient drift data silently becoming success.
- Original TASK-002 scenario 4 explicitly owns readiness and inconclusive insufficient data in the standalone engine. Its drift reuse map includes existing server completeness tests as reference behavior. It assigns IO, source reads, scheduling and hosted composition to TASK-005, not permission to return a false healthy verdict on incomplete selected samples.
- `architecture/references/domain/drift-monitoring.md:38-42` requires partial windows to be indeterminate, not no drift.
- TASK-005 scenario 4 preserves source selection, readiness, baselines, scheduling and reactions. It does not state that TASK-002 may assume a prevalidated null-free batch. No such constructor invariant is enforced or documented by `DriftChecks`.

## Source and caller tracing

CodeGraph preceded source navigation; expanded through direct source and repository call-site searches. All production calls to `score_drift`, `score_custom`, and `score_custom_mean` were accounted for:

| Producer / entry | Consumer | Completeness behavior |
| --- | --- | --- |
| `DirectExecutor::drift`, `verification/direct.rs:173-234` | `score_drift` at 225 | Resolves configured feature names, rejects absent fields, and requires every selected feature value to be present (208-218); incomplete data yields `DriftReport::unscored` (222). Custom uses `profile.metric_name` (184-185). |
| `DriftEngine::try_verify`, `verification/drift.rs:692-771` | `score_custom_mean` at 727 | `fold_custom` (628-644) requires positive selected count, equality to numeric count, valid and finite mean; otherwise no scored report is produced. This is aggregate admission, not a different Custom equation. |
| New `DriftChecks::new/judge`, `vala-eval/src/blocks/drift.rs:38-78` | `score_drift` at 74 | Accepts arbitrary valid RecordBatch by slot; selects Custom fitted marker and directly calls scorer. No completeness precondition is checked. |
| `score_drift`, `vala-drift/src/baseline/mod.rs:129-170` | `score_custom` at 167 | Dispatch only; Custom profile and fitted-marker checks do not establish data completeness. |
| `score_custom`, `vala-drift/src/custom/mod.rs:29-68` | `score_custom_mean` at 67 | Resolves configured column by name, requires numeric, calls `collect_f64_non_null`, refuses empty/non-finite surviving values, computes mean of surviving values. |
| Remaining calls | Drift crate tests and public-surface nameability test | No production boundary or alternate upstream completeness guarantee. |

`feature.rs:28-40::collect_f64_non_null` explicitly discards null rows. `score_custom_mean` (81-127) computes absolute mean deviation against the configured threshold; it cannot recover the selected-row count. PSI/SPC sibling raw scorers already call `target_complete` (`psi/mod.rs:196`, `spc/mod.rs:203`) before scoring selected rows. That helper is crate-private; exporting its internal target representation is unnecessary for a correction.

The new adapter's injected batch is the selected population, unlike a scheduling selector or query awaiting execution. Its constructor owns the evidence maps and permits nullable arrays, and its public `judge` has no null-free caller contract. `VerificationEngine::with_drift` accepts that adapter; `check_bindings` deliberately resolves no ordinary evidence for a drift check (`engine.rs:534-546`), so there is no preceding guard. `judge` delegates to the adapter at 414. `Judged::checked` at 682 projects Passed unchanged; final `decide` at 749 returns Passed for one passing applicable requirement. The raw adapter is explicitly required by TASK-002, so a lack of hosted callers at this intermediate task does not make the path dormant.

## Proposed finding: FOLLOWUP-DRIFT-001 (same cause as DRIFT-D1)

**Classification:** INCORRECT. **Location:** `crates/vala/vala-eval/src/blocks/drift.rs:57-76`.

A valid numeric selected batch `ms = [Some(100.0), None]`, Custom metric `ms`, baseline 100 and threshold 5 produces mean 100 after null filtering, a NoDrift report, a Passed requirement and a Passed all-Verifier result. The second selected observation is missing its required measurement. Existing direct and scheduled owners both refuse to score that population. The unchanged math alone therefore does not preserve required insufficient-data behavior.

**Correction boundary:** the requirement-level adapter's selected raw evidence boundary, before calling the unchanged Custom scorer. Preserve the established configured-metric completeness rule, producing the existing `DriftReport::unscored(Custom)` through `DriftChecks::scored` for a null-containing selected metric column. Resolve by configured column name and leave unrelated columns irrelevant. Preserve current missing-column/type refusals, baseline handling and score errors. Reuse Arrow's existing named-column/null APIs and the existing report projection; do not export private target helpers, change `score_custom`'s public low-level behavior, duplicate statistical math, add sampling/imputation policy, or touch hosted admission/scheduling.

A consumer guard is appropriate here because the adapter owns the supplied selected batch and is the last boundary with the complete raw population before the explicitly null-filtering numerical helper. There is no common production producer that already guarantees completeness for both standalone and hosted callers. Hosted source selection and count construction still belong to TASK-005; the proposed local check cannot establish watermark or source-wide completeness and must not claim to do so.

**Focused closure proof:** reuse the existing IO-free drift engine fixture with one nullable numeric selected metric `[100, null]`; assert requirement and `all` aggregate Inconclusive and retained supplied window provenance/detail. Keep complete matching and complete shifted controls passing/failing respectively. Record and run the resulting exact nextest selector plus the existing drift adapter test and task-local vala-eval library lane. No hosted journey or new harness is needed.

## Limits

Static source tracing, no tests rerun and no implementation changed. The existing engine drift test supplies complete Custom samples only; it cannot falsify this path. This report proposes the same finding as DRIFT-D1, not an additional independent defect or a final verdict. Independent Ponytail validation must deduplicate and assess it.
