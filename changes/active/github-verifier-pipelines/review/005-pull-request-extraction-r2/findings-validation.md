# TASK-005 review r2 — findings validation (ponytail-rev)

Recommended verdict: **PASS**.

Subject: prior candidate `79a7f985e` → new candidate `89a6ff99f`. Spec revision 8.
Inputs: r1 `verdict.md`, `findings-validation.md`, `005-R1-reference-output-shape.md`;
r2 `closure-review.md`.

## Scope check

`git diff --stat 79a7f985e 89a6ff99f`: six r1 review artifacts plus one line each in
`docs/src/content/docs/reference/verifier-card.md` and `docs/public/llms-full.txt`
(identical sentence, `/wyrd/` link prefix only). No code, test, schema or SDK change.

## FIND-005-1 — CLOSED (independently confirmed)

| Doc claim | Source | Result |
|---|---|---|
| `changes`, `review.full_pr` are `{base, head, files}` | `output.rs` `ChangeSet { base, head, files }` | true |
| file entry `{path, previous_path, status, before, after, hunks}` | `output.rs` `ChangedFile` (exactly six fields, `deny_unknown_fields`) | true |
| `status` ∈ added/modified/deleted/renamed/type_changed | `ChangeKind`, snake_case | true, complete |
| `before`/`after` nullable | `Option<FileSource>` | true |
| side kinds `text{blob,text}`, `binary{blob}`, `unavailable{blob,limit_bytes}`, `excluded{blob,reason}` | `FileSource` `tag = "kind"`, snake_case | true, complete |
| `changes.files` empty when nothing changed | empty-change journey: `extraction["changes"]["files"] == []` in incremental mode | true |
| example check fails on empty change | identical to `REVIEW` const check; same journey asserts details verdict `failed` | true |
| `GET /v1/verification/executions/{id}/details` | `verification/routes.rs:617`; journey calls `/v1/...details` | true |
| MCP `verification.get_execution_details` | `mcp/verification.rs:55` | true |

## r2 carry-forward

Holds. The diff touches only one docs bullet and its generated mirror, so the r1
evidence for X1–X10, X12, X13 is not invalidated. X11 now PASS.

## Missed material defects

None found. Minor, non-material: the bullet does not spell out `hunks` element
shape or `ExclusionReason` values; the stated failure mode (wrong assertion path
passing on an empty diff) is what FIND-005-1 required, and it is fixed.

## Final ledger

| ID | Status |
|---|---|
| FIND-005-1 | Closed |
| New | None |

**PASS.**
