# TASK-005 review r1: behavior-rev

Subject: base `b262696af`, candidate `79a7f985e` (HEAD verified). Spec revision 8
(approved). This is an independent, read-only review. I treated the task's `## Evidence`
as claims to test. `J` = `crates/wyrd/wyrd-server/tests/integration/pg_github_pull_request_extraction.rs`,
`Q` = `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs`,
`C` = `crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs` (all at the candidate).

Re-run: `pg_github_pull_request_extraction` journeys, 20 of 20 passed.

## Acceptance matrix

| Requirement | Implementation evidence | Verification evidence (assertions) | Result |
|---|---|---|---|
| X1 / REQ-006: `extract` takes exactly one of `from` or `extractor`, and media extract is unchanged | `wyrd-spec/.../verifier/mod.rs` makes `Action::Extract` take `from: Option`, `extractor: Option`. `validate.rs` returns `extract_source` for neither or both, and keeps the media-slot check on `from`. `vala-verify/.../engine.rs:1951` binds `from` only when it is present. `evidence.rs:661` is unchanged. | `validate.rs` cases: `extractor` beside `from`, and `from: null`, both give `extract_source`. The declaration route returns 400 `extract_source`. Media binding goes through the unchanged evidence executor. | PASS |
| X2: registration derives `queue_only`; direct verification refuses before any step | `cards/service.rs:queue_only` returns `true` for any `Extract{extractor: Some}` step before the publish scan. The stored value replaces whatever the author wrote. `direct_eligibility` reads `spec.queue_only` and does no request-time scan. | J:1024 asserts 422 `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`, the exact detail, and `repository_reads` unchanged. | PASS |
| X3 / INV-001 / INV-007: only the queued activation's pins and tenant connection are used; no source means refusal before acquisition; no caller credentials | `runner.rs` builds `PullRequestSource` only for `ExecutionInput::PullRequest`. `blocks.rs::pull_request` is Terminal when `None`. `extract_pull_request` refuses a missing head repository before IO. It uses `caller.data_tenant_id` and `installation_access`. The action has no credential field. | J:1092: enqueue without admission gives step `failed` with the exact reason, `inconclusive`, and no repository read. J:541: token was minted, details hold no `ghs_` prefix. | PASS |
| X4 / REQ-006 baseline: extracted-source eligibility, P/A/B, fallbacks, retryable history, bases kept apart | Migration adds `pr_source_extracted` with a CHECK for completed PR rows only, and replaces the batched function. `COMPLETE_SQL` writes `$5` inside the lease-fenced update. `engines::pr_source_extracted` keys on `CallProvenance::Extract{version:"pull_request/1"}`, which differs from media's `wyrd-extract/1`. `BASELINE_SQL` requires `completed AND pr_source_extracted`, the same PR identity, `pr_observed_at <`, the same Verifier name, and prefers the exact uid. `history()` maps every read error to Retryable and commits before any IO. `review()` maps library `Rewritten`/`Unavailable` to the fallbacks. | J:956: inconclusive `d` is skipped and `b` is incremental from `a`. J:989 and J:2047: failed or not-passed extracted reviews become the baseline. J:1562: A is cancelled, B's previous is P. J:857, 879 and 895 cover the three named fallbacks. J:1169: 2 attempts, `execution history is unavailable now`, no compare read, flag false. J:1750: `pr.base`=d, `merge_base`=`comparison_base`=c. Q tests cover single/batched parity, replay, stale lease, rollback, and completion after cancellation recording nothing. The CHECK refuses the flag on other origins. The only `Completion` producer is `runner.rs:1487`. | PASS |
| X5: one acquisition through task 004's pipeline; no clone, index or discovery; settling reads nothing again | `extraction.rs` makes one `ReviewPipeline::pinned` call per comparison, with no filesystem access. | J:732: one `/compare/` read for both consumers. J:1295: settling makes no further compare read. Fallback reviews call the pipeline twice (see N1). | PASS (see N1) |
| X6: closed typed output; empty active diff is evidence, never a pass | `extraction/output.rs` covers every contracts field; the `FileSource` variants are text, binary, unavailable and excluded. | J:541 and J:1750 cover metadata, fork head, empty body, rename and delete. J:1603 covers binary, unavailable without fetch, excluded vendored and symlink. J:922: empty `changes.files`, verdict `failed`. | PASS |
| X7 / REQ-007: Prompt and workspace binding, bystander, policy, live consumption | Ordinary engine bindings; Wyrd reads no repository guidance (revision 8). | J:732: Prompt values parsed back equal the extraction's `pr`, `changes` and `context`; the review section is not in the Prompt; the mount is the whole extraction; `fs.list` is exactly `/inputs/pr.json`; the bystander sees none of it. J:1603: an edited `AGENTS.md` appears as before/after text and only the Card requirement is judged. J:1295: model call while the Scribe share is held, status `running`. | PASS |
| X8 / INV-002/004/005: recovery reuses accepted extraction after context expiry | Step output is held in accepted revisions; no new mechanism. | J:1392: contexts expire, then restart and lease expiry; no repository read; recovered Prompt values equal the crashed attempt's and the retained output; mount equals retained; flag true. | PASS |
| X9: missing source, truncation, omissions and model limit fail honestly | The library refuses (`PatchUnavailable`, `Truncated`); `pipeline_failure` classifies the error; existing model and requirement limits apply. | J:1873 missing blob gives inconclusive. J:1904 gives the exact `patch unavailable` reason with null output. J:1943 records 5 `FileLimit` omissions and verdict `failed`. J:2047 gives a 400 context-length error and a not-passed review. | PASS |
| X10: metadata group commit | `review_cache.rs`: 4 workers, tenant hashed to a fixed worker, `recv_many` up to 64, `PENDING` 256, one transaction per tenant share, reply after commit, failure fans out to all. Reads stay direct. SQL uses `statement_timestamp()`, so per-statement stamps are kept inside a batch. | C: lone write within 5 s; queued writes share one committer and are invisible before commit; a trigger-refused write fails both writes and applies neither; the pipeline tests run over the batched owner. | PASS |
| X11 / REQ-016 / AC-011 (rev 8): wire field, stable errors, HTTP/MCP inspection, no SDK projection, focused guide | `card-types.ts` regenerated with only `extractor`. `output_schema` returns `None` for the PR extractor. Reference docs added. | J:541: MCP `verification.get_execution_details` output equals the HTTP extraction, including `review.previous`. The docs describe the output shape incorrectly (B1). | FAIL (B1) |
| X12: design authorities | `wyrd-design.md` action table, references and Agents paragraphs; `bifrost-design.md` custody bullet. | Content review. One inaccurate sentence (N1). | PASS (see N1) |
| X13: existing targets, dependency and tenant checks, no new table | Tests in the existing `integration` targets. One column plus a replaced function. `TenantConn` with RLS. | Q tenant isolation reads `FirstReview` for tenant B. Gates claimed green, not re-run here. | PASS |
| AC-004 (cold/warm, full/incremental, unusable baseline, Prompt and workspace, omissions, model limit) | As above. | Cold and warm covered at the cache owner (C pinned-review test). Remaining cases covered by the J rows above. | PASS |
| INV-007 (no secrets in results) | Token stays in `SecretString`; the transport error strips the URL. | J:541 token-prefix assertion. | PASS |

Failure diagnoses: existing tests changed by this task (`pg_github_pull_requests.rs`,
`pg_durable_verifier_work.rs`, `pg_hosted_artifacts.rs`) were only adapted to the new
`Completion`/`Execution` fields. No assertion was weakened.

## Findings

### B1: INCORRECT (blocking). The extraction output shape is misdescribed in the only author-facing contract

- **Violated obligation:** X11 (focused input/workspace guide; REQ-016 "focused user guides", AC-011 "schemas … match served operations"). Revision 8 removes every client-side output schema, so these docs are the only contract an author has.
- **Location:** `docs/src/content/docs/reference/verifier-card.md:78` and its copy in `docs/public/llms-full.txt`.
- **Evidence:** The docs describe `changes` as "(each file's `path`, `status`, `before` and `after` source, and `hunks`)" and `full_pr` as "the whole pull request". The served value is `ChangeSet {base, head, files: [...]}` (`extraction/output.rs`). The journeys bind `steps.pr.output.changes.files`. The words `files`, `previous_path`, `blob` and `text` never appear in the docs.
- **Failing scenario:** References into this output are not type-checked until execution (`validate.rs` `output_schema` returns `None`). A literal reader therefore has two ways to go wrong:
  - Writing `each: steps.pr.output.changes[*].path` fails only at execution.
  - Writing `check: {kind: assert, path: steps.pr.output.changes, operator: is_non_empty}` passes on an empty diff. `container_length` counts the object's 3 keys (`vala-verify/src/operators.rs:723`).
- **Consequence:**
  - The documented shape leads an author to a check that passes an empty or missing change, against X6's "an empty active diff is … never a pass".
  - Per-file bindings cannot be written from the docs alone.
  - The docs end with "Read the output in the execution's step results" without naming the HTTP details route or the MCP tool `verification.get_execution_details`. Those are the only inspection paths revision 8 keeps.
- **Materiality:** The obligation is explicit, the scenario is reachable and proven from source, and the result can be a false pass. This is not a wording preference.
- **Testable correction:** In `reference/verifier-card.md` (and regenerate `llms-full.txt`):
  - Document `changes` and `review.full_pr` as `{base, head, files: [{path, previous_path, status, before, after, hunks}]}`, with each side `{kind: text|binary|unavailable|excluded, blob, text?|limit_bytes?|reason?}`.
  - Show an example binding `steps.<id>.output.changes.files`.
  - State that the baseline requires the same Verifier version.
  - Point to execution details over HTTP and `verification.get_execution_details` over MCP.
  - Then run `mise run docs:check`. A reviewer can check that every documented path resolves against the J:541 output.

### N1: INCORRECT (non-blocking). "Calls the pipeline once" is false for fallback reviews

- **Obligation:** X5 / X12 wording ("calls the code-review pipeline once").
- **Location:** `architecture/wyrd-design.md` (new Agents paragraph); `extraction.rs::review`.
- **Evidence:** On `BaselineUnusable`, `review()` calls `reviews.pinned(..., None)` a second time. The library documents that "an unusable one is reported, never replaced" (`review.rs::pinned`). The first call has already read the PR, the merge-base…head compare and the trees, so a `history_rewritten` or `history_unavailable` review repeats that acquisition.
- **Consequence:** Extra GitHub reads only on fallback reviews. Accepted evidence still comes from one call, so correctness is unaffected.
- **Correction:** Say in the design paragraph that an unusable baseline repeats the call without it, or have the library return the full set alongside the problem. Optionally pin the compare-read count in J:857.

### D1: DRIFT (non-blocking). Output types decode though nothing decodes them

- **Location:** `extraction/output.rs`. Every type derives `Deserialize` and `deny_unknown_fields`.
- **Evidence:** The only decoder is the unit test. The server only serializes `PullRequestExtraction` (`blocks.rs`). "Unknown-field refusal" therefore proves a property no production path uses.
- **Correction:** Remove the `Deserialize` derives and `deny_unknown_fields`, and keep a serialize-to-expected-JSON test. The struct definitions already close the shape.

## Overall

**FAIL**: B1 needs a docs correction. Every behavioral obligation (X1–X10, X12, X13) and
AC-004 held when I tried realistic paths: direct vs queued, baseline selection with P/A/B
and all fallbacks, recovery, Agent consumption, honest failure, and tenancy. N1 and D1 are
optional.
