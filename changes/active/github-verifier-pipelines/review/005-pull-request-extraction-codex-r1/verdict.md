# TASK-005 — FIX_REQUIRED

One independently confirmed mandatory rustdoc gate remains. No material behavioral, ownership, tenant-isolation or recovery defect was validated.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`.
- Candidate: `1f798d132c03cbc2474e1bad4bc36763e8047848`.
- Actual reviewed range: `git diff b262696af...1f798d132`, entire implementation, first review.
- Task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.
- Authority: approved revision 8 of `changes/active/github-verifier-pipelines/spec.md`, extraction contracts and applicable repository/architecture rules.

No previous review conclusions, finding IDs, acceptance evidence or closure claims are carried forward. The old r1 remediation task and documentation correction were inspected as ordinary candidate implementation. Revision 8 supersedes stale task wording about repository guidance and SDK/CLI extraction projections. No spec/task/source/test/doc file was edited during review; no commit was created. Only this review directory was added. The recorded HEAD remained unchanged.

## Validated applicability and independent results

The shared starting map and source-backed selection rationale are in [applicability.md](applicability.md). The fresh independent validator confirmed complete selected coverage, including unchanged owners and coherent sensitive-domain boundaries.

| Independent role | Report | Result |
|---|---|---|
| behavior-rev: comprehensive X1–X13 discovery | [task-review-behavior.md](task-review-behavior.md) | PASS |
| invariant-rev: accepted evidence, eligibility, baseline and metadata state | [task-review-invariants.md](task-review-invariants.md) | PASS |
| repo-rev: cross-tier rules, generated/docs parity, SQL and all changed-item rustdoc | [standards-review.md](standards-review.md) | FAIL: STD-005-1 |
| maintainer-rev: extraction/transport/batching ownership and existing alternatives | [maintainer-review.md](maintainer-review.md) | PASS |
| system-rev: deployed failures, waits, bounded queue and recovery | [system-review.md](system-review.md) | PASS |
| domain-rev: tenant-bound source, credentials and persistent integrity | [domain-review-data-security.md](domain-review-data-security.md) | PASS |
| fresh ponytail-rev: source/materiality/correction and coverage validation | [findings-validation.md](findings-validation.md) | FIND-005-1 CONFIRMED; FIX_REQUIRED recommended |

All selected reports are present. No focused follow-up was required: reports present no material conflict or unresolved common cause; the sole unique proposal received independent source validation. No validator-originated blocker required separate validation. No required specialist was omitted. This is a first review, so prior-finding closure and remediation convergence assessment do not apply.

## Reconciled acceptance matrix

`J` = server `tests/integration/pg_github_pull_request_extraction.rs`; `C` = server `tests/integration/pg_code_review_cache.rs`; `Q` = SQL `tests/integration/pg_verifier_executions.rs`. Full source/assertion matrices are retained in discovery and independently reconciled in findings-validation.md.

| Obligation | Implementation and proof | Result |
|---|---|---|
| X1 exclusive authored source; media unchanged | spec extract_source validation, existing media block route; declaration refusals, spec/vala/media tests | PASS |
| X2 server-derived queue_only and cold direct refusal | cards registration owner and existing flag check; declaration classification and J direct 422/no acquisition assertions | PASS |
| X3 admitted tenant/source, no supplied credentials | runner -> hosted binding -> installation_access; J non-admitted refusal/no read and no token in details | PASS |
| X4 completed extracted-source baseline, exact policy and fallbacks | migration, provenance derivation, fenced single/batch completion, BASELINE_SQL/history; Q parity/replay/stale/rollback/cancel/RLS and J P/A/B/fallback/unavailable history | PASS |
| X5 one accepted acquisition through public library | existing pinned ReviewPipeline delegates acquisition; J compare-read counts across consumers/settlement | PASS |
| X6 closed exact output and active/full separation | server extraction/output; J fork/moved-base/rename/delete/file-kind/empty-diff assertions and roundtrip test | PASS |
| X7 normal Prompt/workspace selective live consumption | ordinary ActionOutput/RunState bindings; J exact Prompt/mount/bystander/held-ACK assertions | PASS |
| X8 accepted evidence recovery after library expiry | existing accepted checkpoint restoration; J unchanged provider-read count, identical retained/recovered values and completion fact | PASS |
| X9 honest missing/truncated/omitted/model-limit results | existing source/step/requirement limits; J missing blob/patch, omission accounting and model-refusal assertions | PASS |
| X10 bounded independent tenant group commits | CodeReviewMetadata/Worker over existing SQL; C shared commit identity, precommit invisibility, tenant separation, lone write and failed-batch rollback | PASS behavior |
| X11 authored wire field and HTTP/MCP details/guide | spec has no client output schema; TS parity; J same details JSON over HTTP/MCP; docs correct changes.files and tagged sides | PASS within recorded generator limits |
| X12 owning architecture custody | wyrd-design Verifier/workspaces and bifrost-design accepted source/coordination paragraphs | PASS |
| X13 existing targets, no new table, tenant/dependency gates | TenantConn/RLS and Boolean coordination migration; Q cross-tenant baseline, C tenant transactions; boundary checks | PASS |
| Mandatory rustdoc for new Rust items | HeldBatch fields and source_extracted panic documentation are incomplete | FAIL: FIND-005-1 |

The implementation extends existing hosted/media, installation access, public library pipeline, metadata SQL and accepted-state owners. No independently validated duplicate orchestration or unearned abstraction remains. Report format/provider delivery/source checkout/splitting and extra client surfaces remain excluded. Adjacent status-only task bookkeeping, the approved spec amendment and prior review artifacts are packet context rather than proof; no material unrelated implementation entered acceptance scope.

## Validated finding ledger

**FIND-005-1 — CONFIRMED / VIOLATION / discovery STD-005-1.** `pg_code_review_cache.rs:756–757` adds undocumented HeldBatch.lock/write fields; `pg_verifier_executions.rs:4607–4615` adds source_extracted with an expect panic and no `# Panics`. Root AGENTS and agent_docs/rust.md expressly make these test-helper rustdoc omissions merge-blocking. The minimal correction adds comments only in the two existing test owners; it preserves all runtime bodies, assertions and fixtures. Complete source proof, consumers, materiality and correction-burden analysis are in [findings-validation.md](findings-validation.md).

Remediation: [005-R1-complete-test-helper-rustdoc.md](005-R1-complete-test-helper-rustdoc.md), routed directly to `$wyrd-implement`. Closure needs documentation inspection, format and diff checks only. No behavioral redesign or spec revision is required; a later remediation review carries forward source-backed acceptance and inspects this bounded correction and affected paths.

## Verification and limits

Fresh execution passed 198 selected behavioral tests: spec 24, server unit 2, connected server integration 55, SQL execution integration 40 and vala-verify 77. Real integration lanes used repository-managed isolated Postgres and local provider/model transports. Format, dependency and tenant-isolation checks passed; the dependency gate also passed its inline security test. Exact commands/run identities and the resolved temporary-cache environment issue are in [verification.md](verification.md).

No full Clippy, source-rewriting codegen/docs generators, benchmark or production-credential test was run. Generated parity/docs are source-reviewed, distinct from historical execution claims. Changed waits are explicitly approved queued extraction/history/metadata paths; the system reviewer and validator establish no new direct/gateway/Bifrost hot-path wait needing a full performance campaign. The remaining risk is the mandatory documentation gate, not a discovered runtime failure. The candidate is not accepted until FIND-005-1 is closed.

**Verdict: FIX_REQUIRED.**
