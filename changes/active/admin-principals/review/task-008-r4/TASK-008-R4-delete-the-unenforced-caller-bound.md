# TASK-008-R4 — delete the unenforced caller bound from the card-ref predicate rustdoc

Route to `$wyrd-implement`. One file, one sentence, no behavior change.

## Subject

| Item | Value |
|---|---|
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Prior remediation task | `changes/active/admin-principals/review/task-008-r3/TASK-008-R3-pin-the-shipped-card-ref-predicate.md` |
| Candidate reviewed | branch `claude/admin-principals-spec-qfsmjc`, HEAD `c34b9d1e0` |
| Prior candidate | `f102e50ee` |
| Review verdict | `changes/active/admin-principals/review/task-008-r4/verdict.md` (`FIX_REQUIRED`) |
| Validated ledger | `changes/active/admin-principals/review/task-008-r4/findings-validation.md` |
| Finding | `FIND-008-16` (reused ID — the round-2 finding is not closed) |

## Issue diagnosis — `FIND-008-16`, INCORRECT

**Violated obligation.** AGENTS.md §16: rustdoc must explain intent and the
relevant invariants, and "documentation is part of implementation correctness".
AGENTS.md §12 completion standard.

**Current behavior.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:20-24`
reads:

```
/// row is not the predicate but three facts outside it — the table's
/// `UNIQUE (data_tenant_id, name)`, `auth_projection` keeping the `name` column
/// equal to `card_ref->>'name'`, and every caller passing a fully qualified ref
/// (`IssueKeyArgs::space` is required).
```

The next sentence makes those three facts load-bearing: "`ORDER BY created_at,
id LIMIT 1` exists because none of that chain is enforced here".

**Exact evidence.** The third fact is false, and `IssueKeyArgs` is not a caller
of this query at all.

- `IssueKeyArgs` exists only at `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:18`
  and is a `clap::Args` struct. A repo-wide `grep -rn IssueKeyArgs
  --include='*.rs'` returns that definition and this doc line, nothing else.
- The three real callers of `service_account_by_card_ref` are
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:591`, and
  `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:155`.
- None of them requires a space. `IssueKeyRequest.card_ref` is a plain `CardRef`
  with no space requirement (`crates/wyrd-spec/src/auth/issue_key.rs:13-22`);
  the `/auth/issue-key` handler validates RBAC only and passes the body through
  (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:260-300`);
  `principal_kind_for_card` inspects `kind` only (`issue_api_key.rs:230-238`);
  `RequestedSubject::CardRef { card_ref: CardRef }` deserializes with no space
  check (`crates/wyrd-spec/src/auth/token.rs:69-73`); and `jwt_bearer` passes a
  `card_ref` read back out of `auth_workload_bindings`.
- `CardRef.space` is `Option<SpaceName>` with `#[serde(default,
  skip_serializing_if = ...)]` (`crates/wyrd-spec/src/reference.rs:24-29`), so a
  wire caller simply omits it.

The other two named facts are true and were independently verified:
`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:51` carries the live
`UNIQUE (data_tenant_id, name)`, and
`crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-75` binds the same
`card.metadata.name` into both `card_ref.name` and the `name` column.

**Observable consequence.** This doc comment exists to tell the next maintainer
exactly what holds a credential-issuing lookup to one row. Two of its three
named guards exist; the third exists nowhere. A maintainer counting three guards
may relax one of the real two — the unique constraint or the name projection —
believing a caller-side space guarantee still covers it. Nothing in the tree
would catch that.

**Why the candidate falls short.** Round 2's criterion 3 asked for what
containment does and does not bound, and prescribed exactly three facts: the
relaxation of every optional field, the `UNIQUE` + name-projection pairing, and
the ordered `LIMIT 1`. The implementation met those three literally and then
added a fourth claim of its own, which is the false one. The prescription did
not forbid accurate additional context; it is the inaccuracy that is the defect.
No test asserts doc prose, so the existing proof could not catch it.

## Intended correction outcome

The const rustdoc names only bounds the tree actually enforces. The predicate,
the test, and every caller are unchanged.

## Decision-complete recommendation

Edit one sentence of the existing doc comment on
`SERVICE_ACCOUNT_BY_CARD_REF_SQL` in
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`:

1. Delete the clause
   ``, and every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)``.
2. Change `three facts outside it` to `two facts outside it`.

Reflow the doc comment to the file's existing width. Nothing else changes.

**Why this and not an alternative.** Deleting a false clause is smaller than
writing a true one, and the remaining text is still correct and complete — the
two surviving facts are what actually bounds the query, which is precisely what
round 2 prescribed. Do not substitute a hedge such as "callers usually pass a
space" or "the CLI requires `--space`": the CLI's required flag is a property of
one optional client of one of three routes and enforces nothing at this query,
and stating it here is what produced the error. Do not add a validation layer to
make the clause true — that would change wire behavior on an approved contract
and is outside this task. Do not restate the invariant on the public function.

## Constraints and preserved behavior

- The SQL predicate is byte-identical to `f102e50ee`
  (md5 `2298c9e6b9cf3e40edb952818226b944`) and must remain so.
- Do not touch the test, any caller, `auth_projection`, any migration, or any
  `wyrd-testing` fixture.
- AGENTS.md §12: do not weaken or disable a check, add `#[allow]`, or delete or
  `#[ignore]` a test. AGENTS.md §13: use your configured identity, add no AI
  co-author trailer, never run `git config` and never set `GIT_AUTHOR_*` or
  `GIT_COMMITTER_*`.

## Non-goals

- Reverting `@>` to `=`, adding a `space` clause, or otherwise changing matching
  semantics.
- Validating `space` on `/auth/issue-key`, on `RequestedSubject::CardRef`, or on
  the workload-binding write.
- Adding a Postgres-backed `wyrd-sql` test for the matching semantics, or any
  new test, fixture, or assertion.
- Moving the explanation off the const, changing the intra-doc link, or adding
  `rustdoc::private_intra_doc_links` to a lane. The review rejected `RR4-1`; the
  placement round 2 prescribed stands.
- Rewriting history to correct `708f01ec9`'s commit message.
- The two out-of-scope handoffs in `verdict.md` — the cross-space `UNIQUE (data_tenant_id, name)`
  collision, and the stale comment at `crates/wyrd/wyrd-testing/src/server.rs:2670-2672`.

## Acceptance criteria

| # | Criterion | Maps to |
|---|---|---|
| 1 | The const rustdoc contains no reference to `IssueKeyArgs` and no claim about what callers pass. | `FIND-008-16` |
| 2 | The sentence names two facts and says `two`, and both named facts remain the `UNIQUE (data_tenant_id, name)` constraint and the `auth_projection` name-column coupling. | `FIND-008-16` |
| 3 | The `ORDER BY created_at, id LIMIT 1` explanation still follows and still reads as a fallback for an unenforced chain. | `FIND-008-16` |
| 4 | The SQL predicate is byte-identical to `c34b9d1e0` — md5 `2298c9e6b9cf3e40edb952818226b944`. | constraint |
| 5 | No other file changes; no test, caller, migration, or fixture moves. | constraint |

## Focused proof

```bash
# 1 — the false claim is gone (must print nothing)
grep -n 'IssueKeyArgs\|three facts' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs

# 4 — the predicate did not move
rtk proxy git show HEAD:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \
  | awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' | md5
# expect 2298c9e6b9cf3e40edb952818226b944

# the existing pinning test still passes
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
```

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:sql
```

Do not use `mise run gate`, `mise run test:rust`, a whole-family lane, the
storage matrix, or any `--all-features` workspace lane as evidence.

## Remediation evidence

`status: implemented` at `7009668ca`. One file, one sentence, no behavior change.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 — no `IssueKeyArgs` reference and no claim about what callers pass | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:20-23` | `grep -n 'IssueKeyArgs\|three facts' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` → no match (exit 1) | PASS |
| 2 — the sentence names two facts, says `two`, and keeps both real ones | `service_accounts.rs:21-23`: "two facts outside it — the table's `UNIQUE (data_tenant_id, name)` and `auth_projection` keeping the `name` column equal to `card_ref->>'name'`" | same grep; rendered doc read back in full | PASS |
| 3 — the `ORDER BY created_at, id LIMIT 1` explanation still follows and still reads as a fallback for an unenforced chain | `service_accounts.rs:23-28`, sentence unmodified: "because none of that chain is enforced here … narrow the predicate rather than lean on that fallback" | doc read back in full | PASS |
| 4 — SQL predicate byte-identical to `c34b9d1e0` | `service_accounts.rs:30-38`, untouched | `awk` extraction of the const piped to `md5` → `2298c9e6b9cf3e40edb952818226b944` | PASS |
| 5 — no other file changes; no test, caller, migration, or fixture moved | commit `7009668ca` contains one file | `git status --porcelain` showed only this file modified plus the untracked `review/task-008-r4/` packet; `git diff --check` clean | PASS |

### Commands

```
grep -n 'IssueKeyArgs\|three facts' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs
sed -n '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/p' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs | md5
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
mise run fmt
mise run lints
mise run test:sql
```

Results: focused test PASS in 0.009s; `test:sql` 120 + 4 + 113 + 2 passed, 0 failed; `fmt` and `lints` clean; `git diff --check` clean.

The predicate md5 was recomputed from the working tree rather than from `git show` because the rtk proxy suppresses raw `git show`/`git diff` output in this environment; the extracted region is identical either way, and the commit touches only doc-comment lines.

### Independent verification of the finding

The finding was checked rather than taken on report, since it asserts a falsehood about code the correction removes:

- `IssueKeyArgs` appears repo-wide only at `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:18` (definition), `:55`, `:96`, `:101`, `crates/wyrd/wyrd-cli/src/auth/mod.rs:16` — all wyrd-cli — plus the doc line now deleted. It is a clap struct with no path to this query.
- The three real callers are `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`, `exchange_api_key.rs:591`, `jwt_bearer.rs:155`.
- `IssueKeyRequest.card_ref` is a plain `CardRef` (`crates/wyrd-spec/src/auth/issue_key.rs:13-22`) and `CardRef::space` is `#[serde(default)] Option<SpaceName>` (`crates/wyrd-spec/src/reference.rs:24-29`), so no caller is obliged to send a space.

### Non-goals confirmed excluded

No wire validation was added to make the deleted clause true, the clause was not hedged into a weaker claim, the explanation was not moved off the const, no test was added or relaxed, and `708f01ec9`'s commit message was not rewritten. The commit message body of `7009668ca` states the two real facts, so the durable record is consistent even though the earlier message is not.
