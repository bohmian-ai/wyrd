# Task implementation review — TASK-008 round 4 (`task-008-r5`)

Reviewer: `task-rev` (fresh, independent). Candidate `ba223a4db`, base `c34b9d1e0`.
Standard: the five acceptance criteria, constraints, and non-goals of
`changes/active/admin-principals/review/task-008-r4/TASK-008-R4-delete-the-unenforced-caller-bound.md`.
Report returned as text by the reviewer (the Write tool refuses report files);
persisted here by the orchestrator.

## Acceptance matrix

| Requirement / criterion / constraint / non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| **C1** — const rustdoc contains no reference to `IssueKeyArgs` and no claim about what callers pass | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:16-25` (whole file inspected, incl. fn doc `:168-176` and test module `:390-470`) | `grep -n 'IssueKeyArgs\|three facts' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` → no output, exit 1 | PASS |
| **C2** — names two facts, says `two`, both facts are the `UNIQUE (data_tenant_id, name)` constraint and the `auth_projection` name-column coupling | `service_accounts.rs:18-20` | Facts independently re-verified: `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85` carries `UNIQUE (data_tenant_id, name)` on `auth_service_accounts` (table starts `:68`); `grep -rn "DROP CONSTRAINT\|contype = 'u'\|DROP INDEX" crates/wyrd/wyrd-sql/migrations/*.sql` shows no later drop (migration `20260601000020:114-127` drops only `contype='c'`). Coupling: `queries/cards/auth_projection.rs:57-71` binds `card.metadata.name` into both `card_ref.name` (`:58`) and the `name` column (`:70`, `UPSERT_SQL:15-26` `$5`) | PASS |
| **C3** — the `ORDER BY created_at, id LIMIT 1` explanation still follows and still reads as the fallback for an unenforced chain | `service_accounts.rs:20-25` | Prose read in full; every remaining sentence checked against the tree (`:10-14` projection writing `space: Some(..)`/`uid: Some(..)` confirmed at `auth_projection.rs:57-62`) | PASS |
| **C4** — SQL predicate byte-identical, md5 `2298c9e6b9cf3e40edb952818226b944` | `service_accounts.rs:26-35`, untouched by the delta | Recomputed **from git objects**, not the working tree: `rtk proxy git show "${c}:crates/.../service_accounts.rs" \| awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' \| md5` → `2298c9e6…` at `f102e50ee`, `c34b9d1e0`, **and** `ba223a4db`; working tree identical. The implementor's disclosed working-tree-only gap is independently closed | PASS |
| **C5** — no other file changes; no test, caller, migration, fixture moved | `rtk proxy git show --name-only 7009668ca` → one file. `ba223a4db` → six `review/task-008-r4/*.md` only | `rtk proxy git diff c34b9d1e0..ba223a4db -- crates/` → single hunk, +3/−4, all `///` lines | PASS |
| Constraint — reflow to existing file width | `:19` = 81 chars, `:20` = 78 | `awk length` over `:7-25`: pre-existing lines are 84/81/81; edited lines 81/78 | PASS |
| Constraint — AGENTS.md §12: no weakened/disabled check, no `#[allow]`, no deleted/`#[ignore]`d test | No non-doc line changed; test module byte-identical | raw diff; `cargo clippy --locked -p wyrd-sql --lib --all-features` clean; `cargo fmt -p wyrd-sql -- --check` clean | PASS |
| Constraint — AGENTS.md §13 identity, no AI co-author trailer | — | `rtk proxy git log -2 --format='%H%n%an <%ae>%n%B'` → both commits `Thorrester <sjforrester32@gmail.com>`, no trailer | PASS |
| Non-goal — no revert of `@>` to `=`, no `space` clause, no matching-semantics change | `:31` still `AND card_ref @> $3` | md5 identity above | PASS (honored) |
| Non-goal — no `space` validation on `/auth/issue-key`, `RequestedSubject::CardRef`, or the workload-binding write | No file outside `service_accounts.rs` changed | `rtk proxy git diff --stat c34b9d1e0..ba223a4db` | PASS (honored) |
| Non-goal — no new test, fixture, or assertion | test module unchanged | raw diff | PASS (honored) |
| Non-goal — explanation not moved off the const; intra-doc link unchanged; no lane added for `private_intra_doc_links` | explanation still at `:7-25`; link at `:170` unchanged; `mise.toml` not in diff | raw diff | PASS (honored) |
| Non-goal — no history rewrite of `708f01ec9` | still present with its original message | `git log --oneline` | PASS (honored) |
| Non-goal — neither out-of-scope handoff folded in | neither file touched | raw diff | PASS (honored) |
| Regression guard (`FIND-008-15`) — test still asserts `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`, `!card_ref::text` | `service_accounts.rs:444-448`, all five present and unweakened | `cargo nextest list -p wyrd-sql --lib \| grep service_account_by_card_ref` → exactly one test; focused `cargo nextest run` → `Starting 1 test`, `1 passed`, 0.008s | PASS |
| Preflight — HEAD `ba223a4db`, tree clean | — | `git rev-parse HEAD`; `git status --porcelain` | PASS |

## Proposed findings

**None.** The delta is exactly the two prescribed token edits and nothing else
entered the file. Both surviving facts were re-verified against the live tree
rather than taken from the implementor's report.

Ponytail ladder on the delta: a pure deletion plus one numeral. It cannot be
smaller — deleting the clause without fixing the count would leave the sentence
self-contradictory. Nothing speculative, no hedge, no new test, no new
abstraction.

Two observations deliberately **not** filed:

- `:12-13` "a caller can only name `space/Kind/name@version`" is loosely a
  statement about callers, but it is the round-2-prescribed rationale for why `=`
  failed, matches `CardRef::uid`'s documented authoring contract
  (`crates/wyrd-spec/src/reference.rs:31-36`), is outside this delta, and is not
  in the "two facts" bound list, so it cannot reproduce `FIND-008-16`'s failure
  mode.
- The R4 task and ledger cite the `UNIQUE (data_tenant_id, name)` at
  `20260601000001_auth.sql:51`, which is actually `wyrd.auth_roles`; the
  `auth_service_accounts` one is `:85`. A miscitation in a review document, not
  in the candidate — the fact itself is true and live.

## Result

**PASS**

## Verification limits

- No `mise run gate`, `test:rust`, whole-crate/family lane, storage matrix, or
  `--all-features` workspace lane used. `test:sql` was not re-run: the delta
  touches only `///` lines, so `clippy --lib --all-features` on `wyrd-sql` plus
  the focused pinning test is the proportionate proof.
- Rendered rustdoc was not regenerated; the `private_intra_doc_links` warning is
  pre-existing, ungated, and out of scope per round 3's rejection of `RR4-1`.
- The `UNIQUE (data_tenant_id, name)` survival was proven from migration DDL
  statically, not from a live `pg_constraint`; no Postgres was started.
- `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` and
  `mise run test:platform:journey` are independently red; neither was run and
  neither is attributed here.
