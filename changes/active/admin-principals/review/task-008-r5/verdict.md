# TASK-008 task review — verdict (round 4, `task-008-r5`)

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` (worktree) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `ba223a4db` (unchanged throughout the review) |
| Prior candidate (base of this delta) | `c34b9d1e0` |
| Remediation commits | `7009668ca` (source), `ba223a4db` (evidence) |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Remediation task being closed | `changes/active/admin-principals/review/task-008-r4/TASK-008-R4-delete-the-unenforced-caller-bound.md` |
| Prior verdict | `changes/active/admin-principals/review/task-008-r4/verdict.md` |
| Prior rounds | `review/task-008-r2/`, `review/task-008-r3/`, `review/task-008-r4/` |
| Write set | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (+3/−4, all `///` lines) plus the `review/task-008-r4/` markdown packet |

## Verdict

**`FIX_REQUIRED`**

The prescribed deletion landed exactly as specified. It left the deleted
premise's counterpart standing in the next sentence: the doc comment now names
three ways to break a two-link chain, and the third is false for the same reason
the deleted fact was.

Five words.

## Acceptance matrix

Standard: the five acceptance criteria, constraints, and non-goals of
`TASK-008-R4-delete-the-unenforced-caller-bound.md`.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R4 criterion 1 — no reference to `IssueKeyArgs` and no claim about what callers pass survives in the file | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (whole file inspected, const doc, function doc, test module) | `grep -n 'IssueKeyArgs\|three facts' <file>` → no output, exit 1; `grep -rn 'IssueKeyArgs' crates/` → only `wyrd-cli/src/auth/issue_key.rs` and `auth/mod.rs` | PASS |
| R4 criterion 2 — the sentence names two facts and says `two`; both are the `UNIQUE (data_tenant_id, name)` constraint and the `auth_projection` name-column coupling | `service_accounts.rs:21-23` | Both facts independently re-verified against the tree by three reviewers: `migrations/20260601000001_auth.sql:85` (not dropped by `20260601000020`, whose `DO` loop drops only `contype='c'`), and `queries/cards/auth_projection.rs:59`/`:71` binding the same `card.metadata.name` into `card_ref.name` and the `name` column | PASS |
| R4 criterion 3 — the `ORDER BY created_at, id LIMIT 1` explanation still follows and still reads as the fallback for an unenforced chain | `service_accounts.rs:23-28` | The `ORDER BY` rationale itself survives intact, but the chain it enumerates does not — see `FIND-008-16` | **FAIL** |
| R4 criterion 4 — the SQL predicate is byte-identical, md5 `2298c9e6b9cf3e40edb952818226b944` | Const untouched by the delta | Recomputed from **git objects** at `f102e50ee`, `c34b9d1e0`, and `ba223a4db` by two reviewers independently — all three match. The implementor's disclosed working-tree-only measurement is now independently closed | PASS |
| R4 criterion 5 — one source file; no test, caller, migration, or fixture moved | `rtk proxy git show --name-only 7009668ca` → one file; `ba223a4db` → six `review/task-008-r4/*.md` | `rtk proxy git diff c34b9d1e0..ba223a4db -- crates/` → single hunk, +3/−4, every changed line a `///` line | PASS |
| R4 constraint — reflow to the file's existing width | `:19` 81 chars, `:20` 78 chars | `awk length` over the doc block: pre-existing lines 84/81/81 | PASS |
| R4 non-goal — no revert of `@>` to `=`, no `space` clause, no matching-semantics change | Predicate byte-identical | Criterion 4 evidence | PASS (honored) |
| R4 non-goal — no wire validation of `space` on `/auth/issue-key`, `RequestedSubject::CardRef`, or the workload-binding write | No file outside `service_accounts.rs` changed | Raw diff | PASS (honored) |
| R4 non-goal — no new test, fixture, or assertion | Test module byte-identical | Raw diff | PASS (honored) |
| R4 non-goal — explanation not moved off the const; intra-doc link unchanged; no lane added for `private_intra_doc_links` | Explanation still above the const; link unchanged; `mise.toml` not in the diff | Raw diff | PASS (honored) |
| R4 non-goal — no history rewrite | `708f01ec9` present with its original message | `git log --oneline` | PASS (honored) |
| R4 non-goal — neither out-of-scope handoff folded in | Neither file touched | Raw diff | PASS (honored) |
| Regression guard (`FIND-008-15`) — the pinning test still asserts `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`, `!card_ref::text` | `service_accounts.rs:450-454`, all five present and unweakened | `mise exec -- cargo nextest list -p wyrd-sql --lib` → the selector selects exactly one test; focused `cargo nextest run` → `1 passed`, 73 skipped | PASS |
| AGENTS.md §12 — do not circumvent a gate | No `#[allow]`, no `#[ignore]`, no deleted or weakened test, no boundary glob broadened, no check added or retired | Raw diff; `cargo clippy --locked -p wyrd-sql --lib --all-features` clean; `mise run lints` zero warnings; `cargo fmt -p wyrd-sql -- --check` clean | PASS |
| AGENTS.md §13 — git identity, no AI co-author trailer | — | `rtk proxy git log -2 --format='%H%n%an <%ae>%n%cn <%ce>%n%B'` → both commits authored and committed by `Thorrester <sjforrester32@gmail.com>`, no trailer in either body | PASS |
| AGENTS.md §11 — verification scope is the narrowest set that proves the change | — | Focused `wyrd-sql` selector + `test:sql` + `fmt` + `lints`; no broad aggregate used as evidence | PASS |
| AGENTS.md §16 — rustdoc on every materially modified item explains intent and relevant invariants, accurately | `service_accounts.rs:24-26` | See `FIND-008-16` | **FAIL** |
| TASK-008 Material Stop Condition — no revocation epoch on the platform plane | Untouched | Raw diff | PASS |

## Wave 1 results

| Reviewer | Report | Result | Proposed |
|---|---|---|---|
| `task-rev` | `task-review.md` | `PASS` | none — read the clause and declined to file it |
| `repo-rev` | `standards-review.md` | `PASS` | none — read the clause and declined to file it as "loose phrasing" |
| `domain-rev-auth-sql` | `domain-review-auth-sql.md` | `FAIL` | `DA5-1` (INCORRECT) |
| `ponytail-rev` (Wave 2) | `findings-validation.md` | ledger non-empty | `DA5-1` CONFIRMED; both PASS declinations overruled |

This is the first round with a genuine three-way split, and it is what Wave 2
exists for. `ponytail-rev` sided with the single dissenter. The orchestrator
independently re-verified the load-bearing fact before accepting that: `CardRef`
declares `pub name: CardName` with no `Option` and no `skip_serializing_if`
(`crates/wyrd-spec/src/reference.rs:20-21`), so `$3` always constrains `name`,
and the two surviving facts cap the matched set at one row for **any** set of
optional keys. Adding an optional `CardRef` field therefore cannot widen the row
count.

## Validated finding ledger

| ID | Sources | Status | Class | Location | Defect |
|---|---|---|---|---|---|
| `FIND-008-16` | `DA5-1` | CONFIRMED | INCORRECT | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:25` | The break-way list still names three ways to break a chain the same commit reduced to two links, and the third — "add an optional `CardRef` field, and this predicate starts matching more rows" — is false once the caller-qualification fact it mirrored is gone. |

`FIND-008-16` is reused rather than minting `FIND-008-17`: the finding is "the
rustdoc justifies single-row resolution with a bound that does not exist", and
the surviving clause is the counterpart of the exact premise round 4 deleted.
`FIND-008-17` remains unassigned.

Rejected and omitted:

- `task-rev`'s declination — it held that the three break-ways "map exactly onto
  the two surviving facts plus the containment relaxation". Mapping a clause onto
  a true statement elsewhere in the paragraph is not the same as the clause being
  true. Overruled.
- `repo-rev`'s declination — it held the clause names three ways to *widen*, the
  third widening containment itself, and called it "loose phrasing, not an
  inaccuracy". That defense concedes the literal text says something else, and
  the reading it substitutes duplicates the paragraph's opening sentence
  verbatim. Overruled.
- `RR4-1` (`rustdoc::private_intra_doc_links`) — settled in round 3 and out of
  scope. Not re-filed. `cargo doc -p wyrd-sql --no-deps` still emits it, together
  with a pre-existing `broken_intra_doc_links` for `[IdError]` at
  `crates/wyrd/wyrd-sql/src/error.rs:204`; neither is a gate failure, since
  `check:docs` (`mise.toml:543-551`) covers only `wyrd-spec`, `wyrd-auth-issue`,
  and `wyrd-auth-verify`.
- The wire-level `uid` caveat on "a caller can only name
  `space/Kind/name@version`" (`:15-16`) — disclosed by two reviewers, filed by
  neither. It is corrected by the very next sentence in the same paragraph, is
  outside this delta, is not in the two-fact bound list, and so cannot reproduce
  this failure mode.

## Prior-finding closure

- `FIND-008-1` .. `FIND-008-7`: closed by spec revision 7 or by landed work.
- `FIND-008-8` .. `FIND-008-14`: closed; the write set cannot have regressed them.
- `FIND-008-15`: closed; the pinning test's assertions are present and unweakened.
- `FIND-008-16`: **not closed.** R4 criteria 1, 2, 4, 5 pass; criterion 3 fails.

## Verification limits

- No broad aggregate was used as evidence. Verification was the focused
  `wyrd-sql` selector, `cargo fmt -p wyrd-sql -- --check`,
  `cargo clippy --locked -p wyrd-sql --lib --all-features`, `mise run lints`, and
  `cargo doc -p wyrd-sql --no-deps`.
- No Postgres was started. The truth of both surviving facts was argued from
  migration DDL, projection source, and JSONB containment semantics, not from a
  live `pg_constraint`. A Postgres-backed test for the matching semantics remains
  a stated non-goal.
- No test asserts doc prose. Every truth value in this round is read-and-trace.
  That is the structural reason this defect class has now survived three
  remediations, and it is inherent to the non-goal that forbids adding one.
- `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` remains red
  independently of this work; not exercised and not attributed here.
- `Co-Authored-By: Claude` trailers on pre-`9fe02aa2e` branch commits contravene
  AGENTS.md §13. Outside every write set under review; the only remedy is
  rewriting history, which is the change owner's call. Disclosed, not filed.
- A miscitation in this review's own prior artifacts, corrected here: the R4 task
  and ledger cite `UNIQUE (data_tenant_id, name)` at
  `migrations/20260601000001_auth.sql:51`, which is `wyrd.auth_roles`. The
  `auth_service_accounts` constraint is at `:85`. The fact is true and live;
  only the line reference was wrong.

## Out-of-scope handoffs for the spec owner (carried forward, unchanged)

1. `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` plus the
   `name`-column projection means two same-named Service/Agent Cards in
   different spaces within one tenant fail the second projection.
2. `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` still comments about a
   uid-less `card_ref` for "the exact JSONB lookup" that the containment
   predicate made obsolete.

## Remediation task

`changes/active/admin-principals/review/task-008-r5/TASK-008-R5-delete-the-orphaned-break-way.md`
