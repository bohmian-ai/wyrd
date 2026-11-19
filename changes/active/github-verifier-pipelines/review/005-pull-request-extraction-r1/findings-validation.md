# TASK-005 review r1 — findings validation (ponytail-rev)

Subject: base `b262696af`, candidate `79a7f985e` (HEAD confirmed). Recorded by
the orchestrator from the validator's returned report, because the harness
refused the subagent's own file write.

Recommended verdict: **FIX_REQUIRED** — one validated docs finding.

## Applicability validation

All omissions in `applicability.md` hold against source:

- invariant-rev: `pr_source_extracted` has one writer (`engines::pr_source_extracted`
  → `Transition::Complete` → `COMPLETE_SQL` / `complete_verifier_executions`) and
  one reader (`BASELINE_SQL`), bounded by a CHECK; `queue_only` is derived once
  (`cards/service.rs::queue_only`) and read once (`direct_eligibility`).
- repo-rev: `wyrd-spec` gains only the optional serde `extractor` field with an
  unchanged `Cargo.toml`; `card-types.ts` is regenerated; the Rust/Python SDKs and
  `wyrd-client` have no extract projection; the existing TS `Action.extract(from)`
  media helper is unchanged; the migration edits no existing migration.
- maintainer-rev: the extractor is an inherent method on `GithubConnections`;
  private free functions are pure transforms; batching extends
  `CodeReviewMetadata` with a private worker; no new trait, tier or owner.
- system-rev: recovery is ordinary step output (`J:1392`); unreadable history is
  retryable before IO; failures use existing `BlockError` classes.

## Per-finding validation

- **B1 → CONFIRMED as FIND-005-1.** `extraction/output.rs` serializes `changes`
  and `review.full_pr` as `{base, head, files: [...]}`; `verifier-card.md:78`
  presents `changes` as a per-file list and never names `files`, `previous_path`,
  `blob`, `text`, `limit_bytes` or `reason`, nor the HTTP/MCP details reads.
  `operators.rs:393` `IsNonEmpty` → `container_length` counts an object's keys
  (`:726`), so `steps.pr.output.changes is_non_empty` passes on an empty diff,
  and revision 8 removed any client output schema that could catch it.
- **N1 → REJECTED (not material).** `extraction.rs:194-213` does call the library
  again on an unusable baseline, and its reads repeat; but the obligation is one
  call for accepted evidence and no reads at settle, both of which hold. The
  structural fix belongs to task 004's library.
- **D1 → REJECTED (not material).** `Deserialize`/`deny_unknown_fields` serve only
  the unit test; X6's closed shape holds through the struct definitions; no
  runtime consequence.
- **domain-review-data.md non-findings → none material.** In-place function drop
  fails closed with precedent and no published image; positional decode follows
  the existing pattern and fails on reorder; shared metadata workers are the
  approved design; the CHECK scan is trivially valid; the RLS planner question
  applies equally to existing queries.

## Acceptance coverage (X1–X13, revision 8)

| Obligation | Covered by |
|---|---|
| X1–X3, X5–X9, X12 | behavior-rev (journeys re-run, 20/20) |
| X4, X10 | behavior-rev and domain-rev (SQL 40, code-review cache 8) |
| X11 | behavior-rev; fails on docs only (FIND-005-1) |
| X13 | behavior-rev and domain-rev |

## Ledger

**FIND-005-1** — sources: behavior-rev B1; status CONFIRMED; INCORRECT, blocking.

- Violated obligation: X11 / REQ-016 focused input/workspace guide; AC-011;
  X6 "an empty active diff is never a pass" as experienced through the guide.
- Location: `docs/src/content/docs/reference/verifier-card.md:78` and its
  generated copy in `docs/public/llms-full.txt`.
- Consequence: an author following the only contract writes
  `steps.pr.output.changes is_non_empty`, which passes on an empty diff; per-file
  paths such as `changes[*].path` fail only at execution.
- Materiality: any author following the guide can hit it; the result is a false pass.
- Correction burden: one docs paragraph and regeneration; no code.
- Correction: document `changes` and `review.full_pr` as `{base, head, files}`,
  each file `{path, previous_path, status (added|modified|deleted|renamed|type_changed),
  before, after, hunks}`, a side `null` when absent or
  `{kind: text, blob, text}` / `{kind: binary, blob}` /
  `{kind: unavailable, blob, limit_bytes}` / `{kind: excluded, blob, reason}`;
  give the example `check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}`
  and say `changes.files` is empty when nothing changed since the previous review;
  name `GET /v1/verification/executions/{id}/details` and the MCP tool
  `verification.get_execution_details`; regenerate and check docs.
- Closure proof: every documented path resolves in the `J:541`/`J:1750`
  extraction JSON; the example check is the one `J:43` uses and `J:922` shows it
  fails on an empty diff; `mise run docs:check` exits 0 and `llms-full.txt`
  mirrors the edit.
