---
id: TASK-008-R3
title: Close residual declaration import noncompliance
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-014, REQ-015]
acceptance: [AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-4]
---

# TASK-008-R3 — residual import declarations

Route directly to `$wyrd-implement`. This is a bounded repository-rule correction, not a behavioral or architectural change.

## Immutable inputs and authority

Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20.
Original tasks: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md` and `TASK-008-tenant-proven-per-file.md` in that directory.
Candidate: `9c3d7ecb982435919924dfa8e6930352b27a9b7e`.
Parent: `23eafa368bca19208faf8311eb7b5421e3660b38`.
Cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.
TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
Diagnosis and validated recommendation: [findings-validation.md](findings-validation.md); discovery: [standards-review.md](standards-review.md); decision: [verdict.md](verdict.md).
Prior correction: `../task-008-review/TASK-008-R1-close-review-gaps.md`. TASK-008-R2 / FIND-007-7 is closed and requires no further change.

## Diagnosis

`architecture/agent-rules.md` requires module-level imports and bare type names in function signatures. Two helpers newly introduced by cumulative TASK-008 retain qualified argument types:

- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1801`, `provider_error(error: iceberg::Error)`.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4332`, `is_tenant_refusal(error: &datafusion::error::DataFusionError)`.

This is residual FIND-007-4, which the earlier R1 review overbroadly declared closed. Both sites are production-reachable. Catalog `assignment_schema`, `provider` and `pinned_provider` route Iceberg provider errors through the first helper. Oracle refusal audit, pre-stream error mapping and `query_stream::terminal_error_code` share the second helper. Current code preserves their runtime behavior but fails the explicit declaration requirement. Green Clippy does not enforce this prose-only rule; no wrong rows or runtime regression is alleged.

## Correction outcome and selected approach

Reuse the existing module import mechanism at the two owners. Import the existing Iceberg error type in the catalog module's top import block, using an owner-identifying alias if necessary, and use its bare name in `provider_error`. Reuse the already imported `DataFusionError` in Oracle's `is_tenant_refusal` parameter.

Preserve exact concrete types, visibility, bodies and all callers. The invalid shape originates in these declarations; downstream changes cannot correct it. Deleting the helpers, duplicating conversion/classification at callers, introducing wrappers or sweeping unrelated declarations would add scope without closing this finding more safely. No new decision is needed.

## Constraints and preserved behavior

Change only import/declaration spelling in the two owning modules. Preserve provider construction/error wrapping, authenticated file proof, remote/native terminal classification, one leader refusal audit, permissions, grants, staged leases, cancellation and all ACK/WAL/publication behavior. No new helper, type, trait, dependency, feature, checker or test harness. No unrelated TASK-006 changes. FIND-007-3, Postgres tenant columns, accepted error code and known lost-Scribe deadline issue retain the maintainer's disposition.

The current review authorization is static only and forbids cargo/nextest/mise/builds/tests/benchmarks and commits while the capacity benchmark uses this host. This task grants no exception. Do not perform runtime verification until separately permitted; retain honest evidence attribution.

## Acceptance and focused proof

FIND-007-4 closes when both new signatures use top-module imported bare names, and the cumulative diff proves that exact concrete types, bodies, visibility and callers remain unchanged. Inspect both top import blocks and all named consumers. Confirm no unrelated source change.

Static proof:

```sh
git diff --check a7582db587c6170a290760f1741673125612b797
git diff --check 9c3d7ecb982435919924dfa8e6930352b27a9b7e
git diff -- crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs crates/vala/vala-bifrost-redux/src/oracle/mod.rs
```

After committing in a separately authorized workflow, confirmation uses explicit original-base-to-candidate and remediation-base-to-candidate whitespace checks. This review does not authorize a commit.

No runtime RED test is appropriate: this changes only type spelling and module imports, with no new executable behavior. Once execution is permitted, run repository formatting and applicable warnings-denied lints (`mise run fmt`, `mise run lints`). No new runtime assertion, journey rerun or capacity run is needed solely for this correction. Existing behavioral proof is retained, not relabeled as freshly executed proof.
