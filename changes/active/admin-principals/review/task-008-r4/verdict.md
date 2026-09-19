# TASK-008 task review — verdict (round 3, `task-008-r4`)

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` (worktree) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `c34b9d1e0` (unchanged throughout the review) |
| Round-2 candidate (base for this delta) | `f102e50ee` |
| Remediation commits under review | `708f01ec9` (source), `c34b9d1e0` (evidence) |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Prior reviews | `review/task-008-r2/` (round 1), `review/task-008-r3/` (round 2) |
| Remediation task being closed | `review/task-008-r3/TASK-008-R3-pin-the-shipped-card-ref-predicate.md` |
| Write set | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (+42/−10) plus round-2 review evidence markdown |

## Verdict

**`FIX_REQUIRED`**

One finding remains: the const rustdoc introduced by `708f01ec9` publishes a
bounding guarantee that the tree does not enforce, on a credential-issuing
query. It is prose in one doc comment, closable by deleting one clause.

## Acceptance matrix

Round 3 reviews the two open round-2 findings against the five acceptance
criteria of `TASK-008-R3-pin-the-shipped-card-ref-predicate.md`, and re-checks
that the earlier closures held.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R3 criterion 1 — the `wyrd-sql` unit test asserts the predicate the query ships: `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`, and no `card_ref::text` | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:448-454` | `mise exec -- cargo nextest run -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` green; selector confirmed by `cargo nextest list` | PASS |
| R3 criterion 2 — the red was closed by strengthening the assertions, not by deleting, relaxing, or `#[ignore]`-ing anything | Same test: 4 → 6 falsifiable assertions; no `#[allow]`, `#[ignore]`, or deleted test in the diff | `rtk proxy git diff f102e50ee..c34b9d1e0` reviewed line by line by three reviewers | PASS |
| R3 criterion 3 — the rustdoc states what containment does and does not bound: that it relaxes every optional `CardRef` field including `space`, and what actually resolves the intended row | `service_accounts.rs:10-29` | Three reviewers independently verified 11 of 12 factual claims true against migrations, `auth_projection`, and `CardRef` | **FAIL** — `FIND-008-16` |
| R3 criterion 4 — the shipped SQL predicate is byte-identical to `f102e50ee`; no behavior change | Const unchanged; md5 `2298c9e6b9cf3e40edb952818226b944` at both revisions | Independently recomputed by two reviewers from `git show` | PASS |
| R3 criterion 5 — no caller, write-side file, migration, or fixture moved | `rtk proxy git diff --stat f102e50ee..c34b9d1e0`: one source file plus review markdown | Same | PASS |
| R3 non-goal — do not revert `@>` to `=`, narrow it with a `space` clause, or change matching semantics | Predicate byte-identical | Criterion 4 evidence | PASS (honored) |
| R3 non-goal — do not add a Postgres-backed `wyrd-sql` test for the matching semantics | No new test file or `#[sqlx::test]` in the diff | Diff review | PASS (honored) |
| TASK-008 Material Stop Condition — no revocation epoch on the platform plane | Untouched by this delta | Diff review | PASS |
| AGENTS.md §12 — do not circumvent a gate | No weakened check, no `#[allow]`, no `#[ignore]`, no deleted test, no added or retired repository check | Diff review by all three reviewers | PASS |
| AGENTS.md §13 — git identity, no AI co-author trailer | `708f01ec9` and `c34b9d1e0` authored and committed by `Thorrester <sjforrester32@gmail.com>` | `rtk proxy git log --format` | PASS |
| AGENTS.md §16 — rustdoc on every materially modified item explains intent and relevant invariants, accurately | `service_accounts.rs:10-29`, `:175-183` | See `FIND-008-16` | **FAIL** |

## Wave 1 results

| Reviewer | Report | Result | Proposed |
|---|---|---|---|
| `task-rev` | `task-review.md` | `FAIL` | `TR4-1` (DRIFT) |
| `repo-rev` | `standards-review.md` | `FAIL` | `RR4-1` (VIOLATION) |
| `domain-rev-auth-sql` | `domain-review-auth-sql.md` | `FAIL` | `DA4-1` (INCORRECT) |
| `ponytail-rev` (Wave 2) | `findings-validation.md` | ledger non-empty | 1 confirmed, 1 rejected |

## Validated finding ledger

| ID | Sources | Status | Class | Location | Defect |
|---|---|---|---|---|---|
| `FIND-008-16` | `TR4-1`, `DA4-1` | CONFIRMED (merged, reclassified) | INCORRECT | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:22-24` | The const rustdoc names a third fact bounding the lookup to one row — "every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)" — that is enforced at none of the three real callers. |

Rejected and omitted: `RR4-1`. The new `rustdoc::private_intra_doc_links`
warning is real but gated nowhere (`mise.toml:543-551` denies only
`missing_docs` and `broken_intra_doc_links`, and covers only `wyrd-spec`,
`wyrd-auth-issue`, `wyrd-auth-verify`), has in-tree precedent including two
instances in `wyrd-client`, and the placement it objects to is the one round 2
prescribed. Full reasoning in `findings-validation.md` §4.

`FIND-008-16` reuses its round-2 ID rather than minting `FIND-008-17`: the
finding was "the rustdoc justifies single-row resolution with a bound that does
not exist", and the same doc comment still does, with a different nonexistent
bound. `FIND-008-17` remains unassigned.

## Prior-finding closure

- `FIND-008-1` .. `FIND-008-7`: closed by spec revision 7 or landed work.
- `FIND-008-8` .. `FIND-008-14`: closed, confirmed again this round — the write
  set cannot have regressed them.
- `FIND-008-15`: **closed.** The test now asserts the shipped predicate and was
  strengthened, not relaxed.
- `FIND-008-16`: **not closed.**

## Verification limits

- No broad aggregate was used as evidence. Verification was the focused
  `wyrd-sql` selector plus `fmt`, `lints`, and `cargo doc -p wyrd-sql --no-deps`.
- `708f01ec9`'s commit message body repeats a weakened form of the false clause.
  History rewriting is out of scope; the correction removes the claim from the
  rustdoc, which is the durable artifact.
- `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` remains red
  independently of this work (reproduced against a change-free commit) and is
  not attributed here.
- `Co-Authored-By: Claude` trailers on the pre-`9fe02aa2e` branch commits
  contravene AGENTS.md §13. Outside this write set; the only remedy is rewriting
  history, which is the change owner's call. Disclosed, not filed.

## Out-of-scope handoffs for the spec owner (carried forward, unchanged)

1. `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` plus the
   `name`-column projection means two same-named Service/Agent Cards in
   different spaces within one tenant fail the second projection.
2. `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` still comments about a
   uid-less `card_ref` for "the exact JSONB lookup" that the containment
   predicate made obsolete.

## Remediation task

`changes/active/admin-principals/review/task-008-r4/TASK-008-R4-delete-the-unenforced-caller-bound.md`
