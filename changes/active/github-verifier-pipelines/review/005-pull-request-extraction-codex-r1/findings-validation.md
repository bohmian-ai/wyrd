# Independent findings validation — TASK-005

Recommendation: **FIX_REQUIRED** for one explicit mandatory rustdoc gate violation. No validated behavioral, tenancy, recovery or ownership finding remains.

## Subject and method

First independent review of the complete `git diff b262696af...1f798d132`, base `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`, candidate `1f798d132c03cbc2474e1bad4bc36763e8047848`. HEAD was independently checked and matches. No previous r1/r2 conclusions, acceptance rows or finding IDs were carried forward. Their remediation task and documentation correction are ordinary candidate scope. Stable finding numbering below is local to this fresh independent review.

Read applicability.md, verification.md, all six selected discovery reports, the review skill, original task and amendments, approved spec revision 8, extraction contracts, root AGENTS and Rustdoc authority. Source navigation started with CodeGraph; its incomplete results for the test fixture required a targeted source read. Independently inspected the complete bodies/consumers that the proposed correction would affect: HeldBatch and start/release, queued_write, the two batching tests using HeldBatch, source_extracted and its single/batched parity/replay/stale/rollback/cancellation callers. Compared the actual Git additions with current source. Also sampled the extraction/history coordinator, metadata worker commit loop, baseline query, migration and exact accepted-recovery assertions to validate report coverage rather than accepting unanimous PASS labels.

No runtime commands or source mutation were performed by this validator. The orchestrator's fresh execution record is separate from task Evidence claims. No new blocker was discovered by this validator, so separate validation of a validator-originated finding is unnecessary.

## Applicability and coverage validation

| Contract/invariant | Independent coverage assessment |
|---|---|
| Complete action/declaration/admission-to-output journey | behavior-rev covers X1–X13, negative flows and non-goals with actual assertions. Revision 8's HTTP/MCP-only inspection and no separate repository policy supersede stale task sentences; these are not missing client requirements. |
| Shared accepted state, provenance, completion and future baseline | invariant-rev is required and present. It follows producer -> settlement -> terminal transition -> single/batched SQL -> future history, including recovery of provenance, not merely the new field. Its cited recovery assertions explicitly compare live/recovered/retained values and provider-read counts. |
| Cross-tier repository rules | repo-rev is required and present. standards-review.md covers Rust/spec/server, SQL capabilities/migration, test helpers, TypeScript wire parity, docs/generated text, architecture and packet amendments. It includes the explicit test-helper rustdoc gate missed by compiler visibility rules. |
| Ownership/abstraction and complete existing alternative | maintainer-rev is required and present. It compares whole public ReviewPipeline::pinned acquisition, existing hosted media extraction, existing installation access and verification Completions, and identifies the Wyrd-owned history/projection gap. It does not use leaf reuse to excuse duplicate orchestration. |
| Failure propagation and process recovery | system-rev is required and present. Its provider/history/cache failures, bounded queue admission, commit-before-reply, cancellation, live ACK independence and restart paths cover the new deployed behavior. New waits belong to approved queued extraction/metadata; direct-eligible requests do not enter them. No new full direct/gateway/Bifrost benchmark requirement is established by this diff. |
| Sensitive source/tenant/credential/persistent integrity boundary | domain-review-data-security.md is present and coherent: accepted tenant/source authority -> installation credentials/screened transport -> tenant metadata -> accepted evidence and fenced eligibility/history. Data and security here are one source-custody boundary, not separate omitted protocols. Shared concurrency/fencing is independently covered by invariant-rev and real SQL/cache assertions; no additional unreviewed concurrency or RBAC feature was introduced. |
| Unchanged dependencies and claimed proof | Reports trace existing RunState/restored action, installation access, public pinned pipeline, TenantConn/RLS and SQL pointer/context statements. Fresh focused execution covers changed action/media binding, full extraction flows, metadata protocol and SQL completion. Existing test names alone are not treated as evidence. |

All selected reports are available and sufficiently scoped; no required specialist is missing. No evidence is carried forward from an earlier review. No focused follow-up is required: the sole proposed finding has an independently traceable local cause and no material conflict/unresolved failure path. Uniqueness does not require another discovery reviewer.

## Reconciled obligation coverage

| Obligation | Source/assertion validation and disposition |
|---|---|
| X1 action exclusivity and media preservation | Closed authored extractor plus extract_source validation, declaration HTTP refusal assertions and existing hosted media route; focused spec/vala/server checks pass. PASS. |
| X2 registration-derived queue_only | Registration extends existing derivation; direct eligibility consumes the flag. Declaration/direct refusal assertions prove behavior without adding request scans. PASS. |
| X3 accepted source/tenant authority | Runner/hosted binding provides source; extractor validates pins and uses installation_access. Non-admitted work reads no provider source. PASS. |
| X4 baseline eligibility/fallback/fences | BASELINE_SQL requires completed extracted PR and earlier observation, exact Verifier preferred; authoritative history errors remain retryable. Migration and completion fact are atomic with existing settlement. Single/batched/replay/rollback/stale/cancel assertions exercise the distinction. PASS. |
| X5 public pipeline and one accepted acquisition | Wyrd chooses history then delegates pinned comparisons/context; no clone/index/recovery acquisition owner. Acquisition count assertions distinguish multiple consumers from repeated work. PASS. |
| X6 exact typed output and empty changes | Closed server output and projection, active/full separation, fork/moved-base/rename/delete/source-kind assertions and empty-diff requirement. PASS. |
| X7 ordinary binding and live visibility | Prompt/workspace/bystander and held-persistence assertions prove exact values, selective input and live consumption. Revision 8 excludes base guidance import. PASS. |
| X8 accepted-state recovery | Recovery assertion compares recovered Prompt/workspace with retained extraction after actual context expiry and asserts no new provider reads plus completed eligibility. PASS. |
| X9 honest incomplete/refused outcomes | Missing blob/patch, omitted context and model-size refusal assertions preserve failed/inconclusive results without fabricated source/pass. PASS. |
| X10 tenant group commit | Worker applies SQL-only tenant transaction and sends replies after commit. HeldBatch tests prove shared transaction identity, precommit invisibility, tenant separation and whole-batch rollback; lone-write test proves immediate service. PASS behavior; documentation gate on its test helpers fails separately below. |
| X11 HTTP/MCP inspection and authored field only | Existing details route/tool, actual extraction JSON and final reference/guide correctly expose changes.files/side shapes. No client output schema/helper/CLI is required. PASS with static generated parity and recorded execution limits. |
| X12 owner architecture | Extraction contract and Bifrost source-custody paragraphs agree with ordinary live/accepted step evidence and coordination fact. PASS. |
| X13 owner targets and tenant isolation | Existing integration targets, no new table, TenantConn/RLS baseline, cross-tenant assertion and dependency/tenant checks. PASS. |
| Mandatory changed-item rustdoc | Newly added fields and helper panic condition omit required rustdoc. FAIL: FIND-005-1. |

## Validated final ledger

### FIND-005-1 — Complete mandatory rustdoc on new proof helpers

- Discovery source: **STD-005-1** in standards-review.md.
- Validation: **CONFIRMED**.
- Classification: **VIOLATION** (explicit mandatory repository gate).
- Violated authority: `AGENTS.md:152–154` requires rustdoc for every new/changed Rust item and `# Panics` when it can panic, and explicitly blocks merge on omissions. `agent_docs/rust.md:233–241` expressly includes fields, private helpers, test helpers and tests regardless of visibility.
- Exact changed locations: `crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs:756–757` (`HeldBatch.lock`, `HeldBatch.write`); `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4607–4615` (`source_extracted`). All are additions in the recorded range.
- Source proof: HeldBatch's enclosing rustdoc describes the scenario, but neither field has rustdoc. start owns an open transaction holding the snapshot-record row lock and an in-flight queued metadata write; release commits that lock then awaits the write. The group-commit and failed-batch tests construct/release it, so the ownership distinction is exercised, not dormant. source_extracted reads pr_source_extracted with fetch_one and terminates in `.expect("execution reads")`; a failed query or absent row panics. Its sole existing doc sentence lacks a `# Panics` section. Its callers assert single/batched flags, replay preservation and no stale/rolled-back/cancelled eligibility.
- Observable consequence and materiality: the candidate fails an explicit repository acceptance gate on newly authored Rust proof owners. This finding does not claim a production runtime failure. Green tests or private visibility cannot waive the gate, and the review skill explicitly preserves it.
- Shared cause and correction boundary: missing mandatory documentation in the new test-helper write set. Consolidate the three omissions into one documentation correction; do not create separate findings per field or caller.
- Smallest safe correction: retain the existing helpers, signatures, bodies, assertions and fixture structure. Add field rustdoc stating the retained row-lock transaction and pending metadata write roles; add the concrete read-failure/missing-row panic condition to source_extracted's existing docs. No runtime code, dependency, generator, public shape or behavior changes are needed.
- Correction burden: a few documentation lines in the two test-owner files. Deleting the helpers loses useful proof setup, inlining duplicates it, and replacing the expect changes test behavior without closing the rule more cheaply. Existing helper/owner reuse is already appropriate; a new abstraction or guard would be unnecessary.
- Focused closure proof: inspect those documentation additions against the unchanged bodies and check that only comments changed; run `mise run fmt:check` and `git diff --check`. No new tests, broad journey rerun, aggregate, or benchmark is justified by this documentation-only correction.

Rejected proposals: none. Additional validator-originated findings: none. No optional advice is carried into remediation.

## Verification and recommendation limits

Fresh orchestrator evidence: focused spec 24/24, server unit 2/2, real-Postgres connected server 55/55, SQL execution-owner 40/40 and vala-verify 77/77 tests passed; format, dependency and tenant-isolation checks passed. This is 198 selected behavioral tests plus the dependency lane's inline security test. See verification.md for exact commands and nextest identities. No full Clippy, codegen/docs generator execution, benchmark or production GitHub credential test is claimed. Static source parity and documentation review cover those artifacts without rewriting the immutable subject.

Route the single bounded documentation correction directly to `$wyrd-implement`. The original task/spec behavior is accepted within the recorded coverage and limits; remediation should check this gate's closure and changed comments only unless new evidence establishes another material defect. No specification revision, broad re-review or redesign is required. This is a first review, so remediation convergence assessment does not apply.
