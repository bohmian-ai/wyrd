# TASK-005-R1 — Document the pull_request extraction's real output shape

- Spec: `changes/active/github-verifier-pipelines/spec.md` revision 8 (approved).
- Original task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.
- Candidate reviewed: `79a7f985e` (base `b262696af`). Verdict: `verdict.md` (FIND-005-1).

## Diagnosis — FIND-005-1

- Obligation: X11 / REQ-016 requires a focused input/workspace guide; AC-011
  requires a reader to complete the journey from the docs; X6 says an empty
  active diff is never a pass.
- Current behavior: `docs/src/content/docs/reference/verifier-card.md:78`
  describes `changes` as "each file's `path`, `status`, `before` and `after`
  source, and `hunks`", i.e. a per-file list. The server serializes `changes`
  and `review.full_pr` as `{base, head, files: [...]}`
  (`crates/wyrd/wyrd-server/src/components/github/extraction/output.rs`), with
  each side tagged by `kind`. The doc names neither `files` nor the side kinds,
  and ends with "Read the output in the execution's step results" without
  naming the HTTP or MCP read.
- Consequence: an author following the guide writes
  `check: {kind: assert, path: steps.pr.output.changes, operator: is_non_empty}`.
  `is_non_empty` counts an object's keys (`crates/vala/vala-verify/src/operators.rs:723-726`),
  so the check passes on an empty diff. Revision 8 has no client output schema,
  so registration cannot catch the wrong path. The guide is the only contract.

## Intended outcome

A literal reader of the reference can bind and assert the extraction correctly,
and an empty change fails a requirement written from the guide.

## Correction

Edit only that bullet in `verifier-card.md`, then regenerate:

1. State that `changes` (the reviewed range) and `review.full_pr` (the whole pull
   request) are `{base, head, files}`; each file is `{path, previous_path,
   status, before, after, hunks}` with `status` one of `added`, `modified`,
   `deleted`, `renamed`, `type_changed`; a side is `null` when the file is absent
   on that side, else `{kind: text, blob, text}`, `{kind: binary, blob}`,
   `{kind: unavailable, blob, limit_bytes}` or `{kind: excluded, blob, reason}`.
2. Give the example requirement
   `check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}`
   and say `changes.files` is empty when nothing changed since the previous review.
3. Replace "Read the output in the execution's step results" with the existing
   reads: `GET /v1/verification/executions/{id}/details` over HTTP or the MCP
   tool `verification.get_execution_details`.
4. Run `mise run docs:generate` so `docs/public/llms-full.txt` mirrors the edit.

Constraints and non-goals: no code, schema, SDK or CLI change; no other docs
page; keep the existing fallback-reason and binary/unavailable/excluded wording
accurate; do not restate the library `context` shape.

## Acceptance

- AC-R1-1 (FIND-005-1): every path the bullet names exists in the extraction
  asserted by `pg_github_pull_request_extraction.rs` (`successive_heads…`,
  `a_fork_behind…`, `unreviewable_files…`).
- AC-R1-2 (FIND-005-1): the example check equals the `REVIEW` constant's check
  in that file, which `an_empty_incremental_change_is_evidence_and_never_a_pass`
  shows does not pass on an empty diff.
- AC-R1-3 (FIND-005-1): the bullet names the HTTP details route and the MCP tool.

## Proof

`mise run docs:check` exits 0; `git diff` shows the same change in
`verifier-card.md` and `docs/public/llms-full.txt`; content review against the
cited assertions. No test lane is required.

## Obligations

| ID | Promise | Source | Planned proof |
|---|---|---|---|
| R1-1 | The reference documents `changes`/`review.full_pr` as `{base, head, files}` with every per-file field, `status` value and side `kind` | FIND-005-1, AC-R1-1 | content review against `output.rs` and the journeys |
| R1-2 | The guide's example check fails on an empty change | FIND-005-1, AC-R1-2, X6 | equals `REVIEW` in J:43, exercised by the empty-change journey |
| R1-3 | The guide names the HTTP details read and the MCP tool | FIND-005-1, AC-R1-3, REQ-016 | content review against the route and MCP reference |

## Evidence

| Obligation | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| R1-1 | `verifier-card.md:78`, mirrored in `llms-full.txt` | `output.rs` `ChangeSet`/`ChangedFile`/`ChangeKind`/`FileSource`; J `successive_heads…`, `a_fork_behind…`, `unreviewable_files…` assert `changes.files`, `previous_path`, `status`, `before`/`after` kinds | content review; `docs:check` 0 | PASS |
| R1-2 | example `check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}` | J:43 `REVIEW` uses the same check; `an_empty_incremental_change_is_evidence_and_never_a_pass` shows it not passing on an empty diff | content review | PASS |
| R1-3 | `GET /v1/verification/executions/{id}/details` and `verification.get_execution_details` | `verification/routes.rs:617`, `wyrd-client/src/verify/execution.rs:278`; `reference/mcp-tools.md:43` | content review | PASS |

No code, schema, SDK, CLI or other docs page changed. Commands: `mise run docs:generate`,
`mise run docs:check` (exit 0 with the change staged), `git diff --check`. No RED: documentation only.
