# TASK-008-R5 — delete the break-way orphaned by R4's deletion

Route to `$wyrd-implement`. One file, one clause, no behavior change.

## Subject

| Item | Value |
|---|---|
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Prior remediation task | `changes/active/admin-principals/review/task-008-r4/TASK-008-R4-delete-the-unenforced-caller-bound.md` |
| Candidate reviewed | branch `claude/admin-principals-spec-qfsmjc`, HEAD `ba223a4db` |
| Prior candidate | `c34b9d1e0` |
| Review verdict | `changes/active/admin-principals/review/task-008-r5/verdict.md` (`FIX_REQUIRED`) |
| Validated ledger | `changes/active/admin-principals/review/task-008-r5/findings-validation.md` |
| Finding | `FIND-008-16` (reused ID — the finding is still not closed) |

## Issue diagnosis — `FIND-008-16`, INCORRECT

**Violated obligation.** AGENTS.md §16: rustdoc must explain intent and the
relevant invariants, and "documentation is part of implementation correctness".
AGENTS.md §12 completion standard.

**What R4 asked for and what landed.** R4 asked for two token edits to the doc
comment on the private const `SERVICE_ACCOUNT_BY_CARD_REF_SQL` in
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`: delete the false
third bounding fact ("every caller passing a fully qualified ref
(`IssueKeyArgs::space` is required)") and change "three facts" to "two facts".
Both landed exactly, and nothing else entered the file. That part is correct and
is not being reopened.

**Current behavior.** The sentence immediately after the two-fact list,
`service_accounts.rs:23-26`, reads:

```
/// column equal to `card_ref->>'name'`. `ORDER BY created_at, id LIMIT 1` exists
/// because none of that chain is enforced here: relax the unique constraint,
/// decouple the name projection, or add an optional `CardRef` field, and this
/// predicate starts matching more rows on a credential-issuing path.
```

Three break-ways for what is now a two-link chain. The first two map onto the two
surviving facts. The third was the mirror of the fact R4 deleted, and R4 deleted
the fact while leaving its counterpart standing.

**Exact evidence that the third break-way is false.** `CardRef` declares
`pub name: CardName` with no `Option` and no `skip_serializing_if`
(`crates/wyrd-spec/src/reference.rs:20-21`), so the bound `$3` always carries
`name` and `card_ref @> $3` always requires `card_ref->>'name'` to match. Fact B
keeps the `name` column equal to that value
(`crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:59` and `:71` bind
the same `card.metadata.name` to both). Fact A,
`UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts`
(`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85`, retained through
`20260601000020_admin_principals.sql`), plus `data_tenant_id = $1` and the
table's RLS then admit at most one such row.

So adding an optional `CardRef` field changes which *keys* containment relaxes
and cannot change the matched row *count* while those two facts hold: the caller
omits the new field and the count is unchanged; the caller sets it and the
projection writes it and the count can only narrow; the caller sets it and the
projection does not and the count goes to zero. It never widens. The clause was
coherent only while the deleted third fact stood — under that fact the bound came
partly from callers supplying every optional field, so a new optional field
opened a new relaxed dimension.

**Observable consequence.** This sentence is the one that tells the next
maintainer what would break a credential-issuing lookup. It now asserts that a
purely additive, spec-level `CardRef` change endangers credential issuance, which
would deter a benign change — and, more to the point, it re-implies that the
caller-supplied field set is load-bearing here, which is the precise
misconception `FIND-008-16` exists to remove. Nothing in the tree catches doc
prose.

**Why the candidate and its proof fall short.** R4's criteria 1, 2, 4 and 5 were
mechanically checkable and all pass. Criterion 3 asked that the `ORDER BY` /
`LIMIT 1` explanation "still read as the fallback for an unenforced chain"; the
rationale survived but the enumerated chain did not, and no grep or test can
detect a sentence that is individually well-formed and collectively wrong. Two of
three Wave 1 reviewers read this clause and declined to file it — one calling the
mapping exact, the other calling it loose phrasing — which is itself evidence
that the residue is easy to read past rather than evidence that it is harmless.

## Intended correction outcome

The break-way list names only the two links the chain actually has, and every
remaining sentence of the doc comment is true as written.

## Decision-complete recommendation

Edit one clause of the existing doc comment on `SERVICE_ACCOUNT_BY_CARD_REF_SQL`
in `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`:

Delete `, or add an optional \`CardRef\` field` from the break-way list, so the
sentence reads "relax the unique constraint or decouple the name projection, and
this predicate starts matching more rows on a credential-issuing path." Reflow
the doc comment to the file's existing width. Nothing else changes.

**Why this and not an alternative.** Deleting the orphaned clause is smaller than
repairing it, and the remaining sentence is then accurate and complete — two
facts, two ways to break them, one fallback. Repairing it would mean writing a
true statement about what adding an optional `CardRef` field does (it widens the
relaxed key set without widening the row count), which is longer, and which the
paragraph's opening sentence already says: "Containment relaxes *every* optional
`CardRef` field, `space` included". Do not restore the deleted caller-side fact
to make the break-way true again — that fact is false, and removing it was R4.
Do not add validation, a predicate clause, or a test.

Before editing, read the whole doc comment (`service_accounts.rs:10-28`) and the
function doc (`:174-193`) once more and satisfy yourself that no other claim is
orphaned. This is the third consecutive round in which one false sentence
survived in this one comment; the reviewers enumerated eighteen claims and found
this the only remaining false one, but the cheap check is to read it whole rather
than to edit only the line this task names.

## Constraints and preserved behavior

- The SQL predicate is byte-identical to `f102e50ee` / `c34b9d1e0` / `ba223a4db`
  (md5 `2298c9e6b9cf3e40edb952818226b944`) and must remain so.
- Do not touch the test, any caller, `auth_projection`, any migration, or any
  `wyrd-testing` fixture.
- The two surviving facts and the `ORDER BY created_at, id LIMIT 1` rationale
  stay; so does the `space` fail-open statement at `:19-20`.
- AGENTS.md §12: do not weaken or disable a check, add `#[allow]`, or delete or
  `#[ignore]` a test. AGENTS.md §13: use your configured identity, add no AI
  co-author trailer, never run `git config`, never set `GIT_AUTHOR_*` or
  `GIT_COMMITTER_*`.

## Non-goals

- Restoring any caller-side guarantee to the fact list or the break-way list.
- Reverting `@>` to `=`, adding a `space` clause, or otherwise changing matching
  semantics.
- Validating `space` or `uid` on `/auth/issue-key`, on `RequestedSubject::CardRef`,
  or on the workload-binding write.
- Adding any test, fixture, or assertion, including a Postgres-backed `wyrd-sql`
  test for the matching semantics.
- Moving the explanation off the const, changing the intra-doc link, or adding
  `rustdoc::private_intra_doc_links` to a lane — round 3 rejected `RR4-1` and it
  stays rejected. The pre-existing `broken_intra_doc_links` warning for
  `[IdError]` at `crates/wyrd/wyrd-sql/src/error.rs:204` is also out of scope.
- Rewriting history to correct any earlier commit message.
- The two out-of-scope handoffs in `verdict.md` — the cross-space
  `UNIQUE (data_tenant_id, name)` collision, and the stale comment at
  `crates/wyrd/wyrd-testing/src/server.rs:2670-2672`.

## Acceptance criteria

| # | Criterion | Maps to |
|---|---|---|
| 1 | The break-way list names exactly two ways to break the chain, and they are relaxing the unique constraint and decoupling the name projection. No `CardRef`-field break-way remains. | `FIND-008-16` |
| 2 | The two-fact list is unchanged and still names the `UNIQUE (data_tenant_id, name)` constraint and the `auth_projection` name-column coupling. | `FIND-008-16` |
| 3 | The `ORDER BY created_at, id LIMIT 1` rationale and the closing "narrow the predicate rather than lean on that fallback" both survive and still follow from the two-link chain. | `FIND-008-16` |
| 4 | Every other sentence of the const doc comment and the function doc is unchanged, and each remains true as written. | AGENTS.md §16 |
| 5 | The SQL predicate is byte-identical — md5 `2298c9e6b9cf3e40edb952818226b944`. | constraint |
| 6 | No other file changes; no test, caller, migration, or fixture moves. | constraint |

## Focused proof

```bash
# 1 — the orphaned break-way is gone (must print nothing)
grep -n 'add an optional `CardRef` field' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs

# 5 — the predicate did not move; read it from the git object, not the working tree
rtk proxy git show HEAD:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \
  | awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' | md5
# expect 2298c9e6b9cf3e40edb952818226b944

# the existing pinning test still passes
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
```

Criteria 1 through 4 are prose and no command proves them. Read the doc comment
back in full and record the two-fact list, the two-way break list, and the
fallback sentence verbatim in your evidence, rather than asserting they are
correct.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:sql
```

Do not use `mise run gate`, `mise run test:rust`, a whole-family lane, the
storage matrix, or any `--all-features` workspace lane as evidence.

## Remediation evidence

`status: implemented` at `ef635d4ab`. One file, five `///` lines, no behavior change.

### Verbatim readback of the const doc comment

Criteria 1 through 4 are prose, so the whole comment is recorded as shipped rather than asserted correct:

```rust
/// Resolve an active Card-bound principal from the Card identity a client can
/// express.
///
/// Containment, not equality: registering a Card-bound principal stores a
/// `uid`-bearing `card_ref` — the projection at `queries::cards::auth_projection`
/// writes `space: Some(..)` and `uid: Some(..)` — while a caller can only name
/// `space/Kind/name@version`, so `card_ref = $3` matched no registered principal
/// at all.
///
/// Containment relaxes *every* optional `CardRef` field, `space` included: a ref
/// with no space matches a row in any space. What bounds this to one intended
/// row is not the predicate but two facts outside it — the table's
/// `UNIQUE (data_tenant_id, name)` and `auth_projection` keeping the `name`
/// column equal to `card_ref->>'name'`. `ORDER BY created_at, id LIMIT 1` exists
/// because none of that chain is enforced here: relax the unique constraint or
/// decouple the name projection, and this predicate starts matching more rows on
/// a credential-issuing path. The stable oldest-first pick is then the
/// difference between a bounded anomaly and an arbitrary one — narrow the
/// predicate rather than lean on that fallback.
```

The three lists, isolated:

- **Two-fact bound** — "the table's `UNIQUE (data_tenant_id, name)` and `auth_projection` keeping the `name` column equal to `card_ref->>'name'`". Unchanged from `7009668ca`.
- **Two-way break list** — "relax the unique constraint or decouple the name projection". The `CardRef`-field break-way is gone.
- **Fallback sentence** — "The stable oldest-first pick is then the difference between a bounded anomaly and an arbitrary one — narrow the predicate rather than lean on that fallback." Wording unchanged; only line wrapping moved.

The function doc is untouched and still reads "Binds the caller's ref as JSONB for [`SERVICE_ACCOUNT_BY_CARD_REF_SQL`], whose documentation carries what the predicate does and does not bound."

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 — exactly two break-ways, both the real ones, no `CardRef`-field break-way | `service_accounts.rs:24-25` | `grep -n 'add an optional \`CardRef\` field' …` → no match (exit 1); readback above | PASS |
| 2 — two-fact list unchanged | `service_accounts.rs:21-23` | readback above, identical to the text committed at `7009668ca` | PASS |
| 3 — `LIMIT 1` rationale and the closing clause both survive and follow from a two-link chain | `service_accounts.rs:23-28` | readback above | PASS |
| 4 — every other sentence of the const doc and the function doc unchanged and true | `service_accounts.rs:10-28`, `:174-182` | `git show HEAD` touches five `///` lines, all inside the one sentence; `+5/-5` | PASS |
| 5 — SQL predicate byte-identical | `service_accounts.rs:30-38`, untouched | `rtk proxy git show HEAD:…` piped through `awk` + `md5` → `2298c9e6b9cf3e40edb952818226b944` | PASS |
| 6 — no other file changes; no test, caller, migration, or fixture moved | commit `ef635d4ab` contains one file | `git show --stat` → 1 file, 10 lines; `git status --porcelain` showed only it plus the untracked `review/task-008-r5/` packet; `git diff --check` clean | PASS |

### Commands

```
grep -n 'add an optional `CardRef` field' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs
rtk proxy git show HEAD:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs | awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' | md5
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
mise run fmt
mise run lints
mise run test:sql
```

Results: focused test PASS in 0.013s; `test:sql` 120 + 4 + 113 + 2 passed, 0 failed; `lints` clean; `git diff --check` clean. `cargo fmt` does not reflow doc comments, so the paragraph was rewrapped by hand and checked for over-length `///` lines (`awk 'length > 100 && /^\/\/\//'` → no output).

Measurement-method correction carried over from R4: `rtk proxy git show` does produce raw output, so the predicate md5 is taken from the git object at `HEAD` rather than the working tree. The R4 evidence's working-tree measurement is superseded by this one.

### Independent verification of the finding

The reviewer overruled two `PASS` reviewers on this clause, so its load-bearing fact was checked directly rather than accepted:

- `CardRef::name` is `pub name: CardName` — no `Option`, no `#[serde(default)]`, no `skip_serializing_if` (`crates/wyrd-spec/src/reference.rs:20-21`). Every `$3` therefore carries `name`, and `card_ref @> $3` always requires `card_ref->>'name'` to match.
- `auth_projection` writes `name: card.metadata.name` into both the JSONB ref and the `name` column (`crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-73`).
- `UNIQUE (data_tenant_id, name)` is at `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85` on `wyrd.auth_service_accounts`. The reviewer's R4 task cited `:51`, which is `wyrd.auth_roles`; the reviewer self-corrected this in the r5 verdict and the fact holds at `:85`.

So a new optional `CardRef` field cannot widen the row count: omitted, containment is unchanged; set and projected, the match can only narrow; set and not projected, the match goes to zero.

### Non-goals confirmed excluded

The clause was deleted, not repaired into a true statement; the caller-side fact was not restored; the explanation was not moved off the const; no test was added, relaxed, or renamed; no caller, migration, or fixture moved; no history was rewritten. `RR4-1` (`rustdoc::private_intra_doc_links`) remains unaddressed per the reviewer's rejection.
