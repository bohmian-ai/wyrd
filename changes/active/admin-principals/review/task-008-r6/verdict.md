# TASK-008 task review — verdict (round 5, `task-008-r6`)

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` (worktree) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `b98ac9fa9` (unchanged throughout the review) |
| Prior candidate (base of this delta) | `ba223a4db` |
| Remediation commits | `ef635d4ab` (source), `b98ac9fa9` (evidence) |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Remediation task being closed | `changes/active/admin-principals/review/task-008-r5/TASK-008-R5-delete-the-orphaned-break-way.md` |
| Prior rounds | `review/task-008-r2/` .. `review/task-008-r5/` |
| Write set | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (+5/−5, all `///` lines) plus the `review/task-008-r5/` markdown packet |

## Verdict

**`PASS`**

All four reviewers pass. The validated ledger is empty. Every obligation of the
original task and of the four remediation tasks is satisfied, non-goals remain
excluded, verification is credible and narrow, and no unrelated change entered the
diff.

`FIND-008-16` is closed. `FIND-008-1` .. `FIND-008-16` are all closed.
`FIND-008-17` was never assigned.

## Acceptance matrix

Standard: the six acceptance criteria, constraints, and non-goals of
`TASK-008-R5-delete-the-orphaned-break-way.md`.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R5 criterion 1 — the break-way list names exactly two ways (relax the unique constraint, decouple the name projection); no `CardRef`-field break-way remains | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:24-26` | `grep -n 'add an optional \`CardRef\` field' <file>` → rc=1. `grep -n 'CardRef\` field'` → only `:19`, which is the legitimate "relaxes *every* optional `CardRef` field" sentence | PASS |
| R5 criterion 2 — the two-fact list is unchanged and still names the `UNIQUE (data_tenant_id, name)` constraint and the `auth_projection` name-column coupling | `:20-23` | Byte-identical to `7009668ca` through `:23`; first divergence at `:24`, inside the licensed sentence | PASS |
| R5 criterion 3 — the `ORDER BY created_at, id LIMIT 1` rationale and the closing "narrow the predicate rather than lean on that fallback" survive and follow from a two-link chain | `:23-28` | Both present verbatim; the entailment was independently re-derived by three reviewers, and both surviving break-ways were verified live (below) | PASS |
| R5 criterion 4 — every other sentence of the const doc and the function doc is unchanged, and each remains true as written | const doc `:10-28`, function doc `:174-182` | Two independent complete enumerations — 24 claims by `task-rev`, 16 by `ponytail-rev`, 10 sentences by `domain-rev-auth-sql` — every claim TRUE against migrations, `auth_projection.rs`, `reference.rs`, both writers, and all three callers | PASS |
| R5 criterion 5 — the SQL predicate is byte-identical, md5 `2298c9e6b9cf3e40edb952818226b944` | Const untouched | Recomputed from **git objects** at `f102e50ee`, `c34b9d1e0`, `ba223a4db`, and `b98ac9fa9` — all four match | PASS |
| R5 criterion 6 — one source file; no test, caller, migration, or fixture moved | `rtk proxy git diff --name-status ba223a4db..b98ac9fa9` → one `.rs` at `5/5`, six `.md` under `review/task-008-r5/` | No `mise.toml`, `scripts/`, `.github`, or `Cargo.toml` in range | PASS |
| R5 constraint — reflow within the file's existing `///` width, hand-wrapped because `cargo fmt` does not reflow doc comments | `:24-28` | Longest `///` line in the file is pre-existing and untouched; the five rewrapped lines are all at or under it. Backtick parity even on every line, so no inline-code span is split — and the five rewrapped lines contain no backticks at all. Adjacent-duplicate scan clean. `cargo fmt -p wyrd-sql -- --check` clean | PASS |
| R5 non-goal — no caller-side guarantee restored to either list | Neither list mentions callers | Raw diff; the only surviving caller-side sentence is `:15-16`, unchanged since `708f01ec9` | PASS (honored) |
| R5 non-goal — no revert of `@>` to `=`, no `space` clause, no matching-semantics change | Predicate byte-identical | Criterion 5 evidence | PASS (honored) |
| R5 non-goal — no `space`/`uid` wire validation | No file outside `service_accounts.rs` changed | Raw diff | PASS (honored) |
| R5 non-goal — no test, fixture, or assertion added | Test module byte-identical | Raw diff | PASS (honored) |
| R5 non-goal — explanation not moved off the const; intra-doc link unchanged; no lane added for `private_intra_doc_links` | Explanation still above the const; link intact; `mise.toml` not in diff | Raw diff | PASS (honored) |
| R5 non-goal — no history rewrite | `708f01ec9` → `7009668ca` → `ef635d4ab` linear, prior SHAs intact | `rtk proxy git log --oneline -15 -- <file>` | PASS (honored) |
| R5 non-goal — neither out-of-scope handoff folded in | Neither file touched | Raw diff | PASS (honored) |
| Regression guard (`FIND-008-15`) — the pinning test still asserts `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`, `!card_ref::text` | `:444-448`, all five present and unweakened, plus the `Json` round-trip at `:443` | `cargo nextest list -p wyrd-sql --lib` → the selector selects exactly one test, so it cannot pass vacuously; focused run → `1 passed, 73 skipped` | PASS |
| AGENTS.md §12 — do not circumvent a gate | No `#[allow]`, no `#[ignore]`, no deleted or weakened test, no boundary glob broadened, no check added or retired | Range grepped for `allow(`, `#[ignore`, `deny(`, `expect(` on added and removed lines → none; `mise run lints` zero diagnostics; `cargo clippy --locked -p wyrd-sql --lib --tests` clean | PASS |
| AGENTS.md §13 — git identity, no AI co-author trailer | — | `ef635d4ab` and `b98ac9fa9`: author **and** committer `Thorrester <sjforrester32@gmail.com>`; both message bodies grepped for co-author and AI trailers → rc=1 | PASS |
| AGENTS.md §16 — rustdoc explains intent, workflow role, and relevant invariants, accurately; `# Errors` on every fallible function | const doc `:10-28`; function doc `:174-182` with `# Errors` at `:181-182` | Criterion 4 evidence | PASS |
| AGENTS.md §11 — the narrowest verification that proves the change | — | Focused `wyrd-sql` selector, `test:sql`, `fmt`, `lints`. No broad aggregate used as evidence by any reviewer | PASS |
| TASK-008 Material Stop Condition — no revocation epoch on the platform plane | Untouched | Raw diff | PASS |

## Wave 1 and Wave 2 results

| Reviewer | Report | Result | Proposed |
|---|---|---|---|
| `task-rev` | `task-review.md` | `PASS` | none |
| `repo-rev` | `standards-review.md` | `PASS` | none |
| `domain-rev-auth-sql` | `domain-review-auth-sql.md` | `PASS` | none |
| `ponytail-rev` (Wave 2) | `findings-validation.md` | validated ledger **EMPTY** | none |

`ponytail-rev` validated the empty union rather than deferring to it: it
enumerated both doc comments independently, re-derived every load-bearing fact
from source, traced both writers and all three readers, and constructed both
surviving break-ways against the schema before opening any Wave 1 report.

## Why this round is structurally different from the last two

Rounds 3, 4 and 5 each closed exactly one false clause in this one doc comment, and
rounds 4 and 5 each failed the same way: a deletion removed a premise and left a
sentence that depended on it. The orchestrator therefore made the orphan hypothesis
the primary question of this round rather than a checklist item, and all three
reviewers who could test it did so independently against all three removed premises.

The reason the sequence terminates here is not that the reviewers looked harder. It
is that the premise class which produced both prior residues is now absent from the
paragraph rather than trimmed from it. The bound to one row derives from
`CardRef.name` being non-`Option` (`crates/wyrd-spec/src/reference.rs:20-21`) plus
the two in-tree schema facts, with no premise about what callers supply anywhere in
the chain. Both remaining break-ways are schema- and projection-side, and both were
independently shown live rather than hypothetical: each is one of the two natural
fixes for the pending cross-space collision handoff — `UNIQUE (data_tenant_id,
space, name)`, or qualifying the `name` column while `card_ref.name` stays bare —
and under either, a space-less caller ref matches two active rows on the API-key
issuing path. There is no third premise left to orphan.

## Validated finding ledger

**Empty.** No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
finding. `FIND-008-17` was never assigned.

Observations considered and deliberately not filed, each because it changes only
what a maintainer would *read* rather than what they would *do*:

- The settled wire-level caveat on "a caller can only name
  `space/Kind/name@version`" — `uid` and `space` are `#[serde(default)] Option`, so
  a client that already knows a server-resolved uid could name one. Corrected by
  the next sentence in the same paragraph, outside every write set, and not a member
  of the two-fact bound list. Disclosed in rounds 4 and 5, filed by nobody.
- "relax the unique constraint" is singular where the table carries three `UNIQUE`
  constraints; the referent is fixed by the preceding sentence.
- The `insert_service_account` second-writer path is unnamed in the doc.
  Completeness of explanation, covered in substance by the second break-way.
- Pre-existing `# Errors` gaps on untouched siblings in the same file
  (`service_account_by_id`, `api_key_by_prefix`, `touch_api_key_last_used`,
  `insert_refresh_token`, `service_account_roles`) — outside the write set,
  unrelated pre-existing debt.
- `rustdoc::private_intra_doc_links` on the public function (rejected as `RR4-1` in
  round 3) and the pre-existing `broken_intra_doc_links` for `[IdError]` at
  `crates/wyrd/wyrd-sql/src/error.rs:204`. Neither is a gate failure: `check:docs`
  (`mise.toml:543-551`) denies only `missing_docs` and `broken_intra_doc_links` and
  covers only `wyrd-spec`, `wyrd-auth-issue`, `wyrd-auth-verify`.

## Prior-finding closure

- `FIND-008-1` .. `FIND-008-7` (the earlier `task-007-008` review): closed by spec
  revision 7 or by landed work.
- `FIND-008-8` .. `FIND-008-14` (round 1): closed, confirmed in round 2 and
  unchanged since.
- `FIND-008-15` (round 2): closed in round 3; the pinning test's five assertions
  remain present and unweakened.
- `FIND-008-16` (round 2): **closed at `b98ac9fa9`.** It survived three
  remediations, each time as one false clause in the same doc comment — the
  same-identity claim (round 3), the caller-qualification fact (round 4), and that
  fact's break-way counterpart (round 5). All three are gone, and the bound the
  finding concerned now derives with no caller premise at all.

## Verification limits

- No broad aggregate was used as evidence by any reviewer. Verification was the
  focused `wyrd-sql` selector, `cargo fmt -p wyrd-sql -- --check`,
  `cargo clippy --locked -p wyrd-sql --lib --tests`, and `mise run lints`.
- No Postgres was started in this round. JSONB `@>` object containment, `@>`
  yielding NULL against a NULL left operand, whole-document `=`, and `jsonb_ops`
  GIN support for `@>` were reasoned from documented Postgres semantics plus the
  DDL. The R5 non-goals forbid adding a Postgres-backed test for the matching
  semantics, so this limit is accepted rather than closed.
- **No test asserts doc prose, and the non-goals forbid adding one.** Every truth
  value across rounds 3, 4 and 5 is read-and-trace. That is the structural reason
  this defect class cost four rounds, and it remains true of the accepted text: a
  future edit to this comment has no gate behind it.
- **Tooling hazard found this round, worth carrying forward.** The `rtk` hook
  rewrites more than `git`. Bare `diff` returns **exit 0 on files that differ** —
  independently reproduced by the orchestrator — so any byte-identity assertion
  written as `diff a b && echo same`, or as a `set -e` step, silently passes. `cat
  -n` is also rewritten and drops this file's first three lines, shifting every
  line citation by −3. Safe: `cmp`, `md5`, `wc -c`, `grep -n ''`, `awk`, `sed -n`,
  and anything under `rtk proxy`.
- `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` remains red
  independently of this work, reproduced by the implementor against a change-free
  commit. Not exercised and not attributed here.
- `Co-Authored-By: Claude` trailers on pre-`9fe02aa2e` branch commits contravene
  AGENTS.md §13, which takes precedence over the harness instruction that produced
  them. Outside every write set reviewed across all five rounds; the only remedy is
  rewriting history, which is the change owner's call. Disclosed in every round,
  filed in none. The ten commits from `9fe02aa2e` onward carry none.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` were not
  re-read for this delta, which touches no contract, kind, header, route, SDK
  surface, or Bifrost path. Stated as a limit rather than claimed as coverage.

## Out-of-scope handoffs for the spec owner

Neither is a finding against TASK-008, and the change owner has confirmed they stay
with the spec owner rather than being folded into this task. Both are now more
relevant than when first raised, because this round established that the accepted
rustdoc's two break-ways are exactly the two natural fixes for the first one:

1. `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts`
   (`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85`) is unqualified by
   space, so two same-named Service or Agent Cards in different spaces within one
   tenant fail the second projection. Either obvious fix — adding `space` to the
   constraint, or qualifying the `name` column while `card_ref.name` stays bare —
   makes a space-less caller ref match two active rows on the API-key issuing path,
   which is what the accepted doc comment now warns about and what `ORDER BY
   created_at, id LIMIT 1` currently absorbs.
2. `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` still comments about a
   uid-less `card_ref` for "the exact JSONB lookup" that the containment predicate
   made obsolete.

## Remediation task

None. `PASS` requires no remediation.

`PASS` is TASK-008's completion gate. This review does not implement, merge, push,
or deploy.
