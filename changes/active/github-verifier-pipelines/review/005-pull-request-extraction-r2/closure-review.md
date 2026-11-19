# TASK-005 review r2 — remediation closure (ponytail-rev)

Recommended verdict: **PASS**.

## Subject

- Task base `b262696af`; prior candidate `79a7f985e`; new candidate `89a6ff99f`.
- Spec revision 8 (approved); task `tasks/005-pull-request-extraction.md`;
  remediation `review/005-pull-request-extraction-r1/005-R1-reference-output-shape.md`.

## Scope

`git diff --name-only 79a7f985e 89a6ff99f`: the six r1 review reports
(`fbc4e6fe1`) plus `docs/src/content/docs/reference/verifier-card.md` and
`docs/public/llms-full.txt`, one line each (the `pull_request` extract bullet).
No code, schema, SDK, CLI, test or other docs page changed. `git diff --check`
is clean. Matches the remediation's constraints.

## FIND-005-1 closure — CLOSED

Each statement in the edited bullet (`verifier-card.md:78`) checked against source:

| Statement | Source | Result |
|---|---|---|
| `changes` and `review.full_pr` are `{base, head, files}` | `output.rs` `ChangeSet { base, head, files }`; `PullRequestReview.full_pr: ChangeSet`; `PullRequestExtraction.changes: ChangeSet`; J:598, J:622, J:870 | true |
| each file is `{path, previous_path, status, before, after, hunks}` | `output.rs` `ChangedFile` (exactly these six fields, `deny_unknown_fields`); J:587, J:1853 | true |
| `status` one of `added`, `modified`, `deleted`, `renamed`, `type_changed` | `ChangeKind`, `rename_all = "snake_case"` | true, complete |
| `before` null for added, `after` null for deleted | `ChangedFile.before`/`after` rustdoc; `Option<FileSource>` | true |
| side is `{kind: text, blob, text}` / `{kind: binary, blob}` / `{kind: unavailable, blob, limit_bytes}` / `{kind: excluded, blob, reason}` | `FileSource` (`tag = "kind"`, snake_case); J:615-624 (`text`), J:1688 (`unavailable`, `limit_bytes`), J:1692/1696 (`excluded`, `reason`) | true, complete |
| `changes.files` empty when nothing changed since the previous review | `ChangeSet.files` rustdoc; J:937 (`an_empty_incremental_change_is_evidence_and_never_a_pass`) | true |
| example `check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}` fails on an empty change | identical to `REVIEW` check at J:43; J:946 asserts verdict `failed` on the empty incremental change | true |
| read with `GET /v1/verification/executions/{id}/details` | `verification/routes.rs:617` `get_execution_details` (`/verification/executions/{execution_id}/details`, served under `/v1` as J:445, J:563 call it) | true |
| or MCP tool `verification.get_execution_details` | `wyrd-server/src/mcp/verification.rs:55`; `reference/mcp-tools.md:43`; J:468 | true |

Unchanged wording (fallback reasons, binary/unavailable/excluded carriage) remains
consistent with `FallbackReason` and `FileSource`.

Mirror: the `llms-full.txt` hunk is the same sentence with the generator's
`/wyrd/` link prefix. `mise run docs:check` exits 0 (rerun with `pipefail`),
working tree clean afterward, so the generated copy is current.

AC-R1-1, AC-R1-2, AC-R1-3 hold.

## Acceptance matrix

| Obligation | Result | Basis |
|---|---|---|
| X1–X10, X12, X13 | PASS (carried forward) | r1 `verdict.md`/`findings-validation.md`; the diff touches only one docs bullet and its generated copy, so no behavioral evidence is invalidated |
| X11 | PASS | FIND-005-1 closed above; the reference now gives the real output shape, a correct example requirement, and the HTTP and MCP reads |

## New material findings

None. The edit introduces no incorrect statement, touches nothing outside its
scope, and adds no new surface.

## Recommendation

**PASS.**
