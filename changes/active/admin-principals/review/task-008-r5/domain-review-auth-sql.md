# domain-rev-auth-sql — sensitive-domain review, TASK-008 round 5 (`task-008-r5`)

Overall verdict: **FAIL** — one material finding, `DA5-1` (INCORRECT).

## 1. Boundary, authority, source coverage

Boundary: the Card-bound principal lookup on the credential-issuing path and the
accuracy of the documentation that asserts what bounds it.

State checks:

- `git rev-parse HEAD` → `ba223a4db0445d44a6f57eee21115da0b77adb48` (matches subject).
- `git status --porcelain` → clean (the `review/task-008-r5/` dir holds only this report).
- `rtk proxy git diff --name-only c34b9d1e0..ba223a4db` → six `review/task-008-r4/*.md`
  files plus exactly one source file, `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`.
- `rtk proxy git diff c34b9d1e0..ba223a4db -- crates/` → byte-for-byte the hunk in the
  subject file: +3/−4, entirely inside the const's doc comment. No other source change.

Sources read end to end:

- `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (const doc 10–28, const
  29–38, `insert_service_account` 101–149, `service_account_by_card_ref` 174–193, test 432–455)
- `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs` (all 83 lines)
- `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:62–101`,
  `20260601000020_admin_principals.sql:84–150`
- `crates/wyrd-spec/src/reference.rs:17–38`
- Callers: `wyrd-auth/src/issue_api_key.rs:86–98`, `exchange_api_key.rs:584–596`,
  `jwt_bearer.rs:120–160`, and the binding source `wyrd-auth/src/pg_resolvers.rs:195–222`
- Wire shapes: `wyrd-spec/src/auth/issue_key.rs:13–22`, `wyrd-spec/src/auth/admin.rs:127–137`,
  `wyrd-cli/src/auth/issue_key.rs:18–43`
- Non-projection writers: `wyrd-server/src/components/principals/routes.rs:240–248`,
  `wyrd-server/src/components/platform/provisioning.rs:215–228`
- `changes/active/admin-principals/review/task-008-r4/TASK-008-R4-delete-the-unenforced-caller-bound.md`

Authority: `AGENTS.md` §12 (documentation is part of implementation correctness),
§16 (rustdoc must explain intent and the relevant invariants), §9, §15.

### Predicate did not move

```
rtk proxy git show <rev>:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \
  | sed -n '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/p' | md5
```

| rev | md5 |
|---|---|
| `f102e50ee` | `2298c9e6b9cf3e40edb952818226b944` |
| `c34b9d1e0` | `2298c9e6b9cf3e40edb952818226b944` |
| `ba223a4db` | `2298c9e6b9cf3e40edb952818226b944` |

Matches the expected hash from git objects, not the working tree. Criterion 4 holds.

### Pinning test

```
mise exec -- cargo nextest list --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
→ exactly one test listed
mise exec -- cargo nextest run  --locked -p wyrd-sql --lib -E '<same>'
→ Starting 1 test across 1 binary (73 tests skipped); PASS 0.005s; 1 passed, 0 failed
```

Assertions at `service_accounts.rs:450–454` are present and unweakened:
`card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`,
`!...contains("card_ref::text")`. The test body is untouched by this delta.

`mise exec -- cargo fmt -p wyrd-sql -- --check` → clean. Doc lines 19–28 are 69–84
columns, consistent with the file.

### No behavior change

The diff touches only `///` lines. The SQL text, the `Json(card_ref)` bind, the
`conn.data_tenant_id()` bind, and all three callers are unchanged in the range.
RLS posture (`ENABLE`/`FORCE ROW LEVEL SECURITY` + `tenant_isolation` policy,
`20260601000001_auth.sql:97–101`) and the GIN index (`:92–93`) are untouched. The
write side (`auth_projection.rs`) is untouched.

## 2. Truth table for the doc comment as it now stands

| # | Claim (`service_accounts.rs` line) | Confirming / refuting source | Verdict |
|---|---|---|---|
| 1 | Registering a Card-bound principal stores a `uid`-bearing `card_ref` (13–14) | `auth_projection.rs:62` `uid: Some(card_uid.clone())` | TRUE |
| 2 | The projection writes `space: Some(..)` and `uid: Some(..)` (14–15) | `auth_projection.rs:61–62`; `space` unwrapped from required `metadata.space` at `:44–48` | TRUE |
| 3 | A caller can only name `space/Kind/name@version` (15–16) | `wyrd-cli/src/auth/issue_key.rs:18–30` exposes `--kind/--name/--version/--space` and no uid flag; no in-tree caller constructs a uid-bearing request ref | TRUE for every in-tree caller surface — see Limits (1) for the wire caveat |
| 4 | Therefore `card_ref = $3` matched no registered principal at all (16–17) | jsonb `=` against a stored object carrying `uid` cannot equal a uid-less caller ref; `reference.rs:36–37` omits `uid` when `None` | TRUE |
| 5 | Containment relaxes *every* optional `CardRef` field, `space` included (19–20) | `reference.rs:28–29, 36–37` — `space` and `uid` are the only optionals, both `skip_serializing_if = "Option::is_none"`; `@>` ignores absent keys | TRUE |
| 6 | Fact A: the table's `UNIQUE (data_tenant_id, name)` (21–22) | `migrations/20260601000001_auth.sql:85`; not dropped by `20260601000020_admin_principals.sql` (its `DO` loop at `:117–128` drops only `contype = 'c'` checks) | TRUE |
| 7 | Fact B: `auth_projection` keeps the `name` column equal to `card_ref->>'name'` (22–23) | `auth_projection.rs:59` and `:71` bind the same `card.metadata.name`; `auth_projection` is the only production writer of a non-NULL `card_ref` — the other two production `insert_service_account` callers pass `None` (`principals/routes.rs:243`, `provisioning.rs:220`) | TRUE |
| 8 | Those two facts bound the lookup to one intended row (20–23) | `$3` always carries the non-optional `name` (`reference.rs:21`); claim 7 couples it to the `name` column; claim 6 makes it unique per tenant; RLS + `data_tenant_id = $1` scope the tenant | TRUE — and the deleted third fact was not needed for it |
| 9 | `ORDER BY created_at, id LIMIT 1` is in the shipped SQL (23) | `service_accounts.rs:36–37` | TRUE |
| 10 | None of that chain is enforced *here* (24) | the const's `WHERE` names no unique constraint and no `name` column | TRUE |
| 11 | Break-way 1: relax the unique constraint → more rows | drops claim 6, so two Card-bound rows could share a `name` and both satisfy `card_ref @> $3` | TRUE |
| 12 | Break-way 2: decouple the name projection → more rows | drops claim 7, so `UNIQUE (data_tenant_id, name)` no longer constrains `card_ref->>'name'` | TRUE |
| 13 | **Break-way 3: add an optional `CardRef` field → "this predicate starts matching more rows on a credential-issuing path" (25–26)** | With claims 6 and 7 intact, `$3` still carries `name`, so at most one row can match whatever new optional keys exist. A new optional field changes which *keys* are relaxed, never the row count. Refuted by `reference.rs:21` (`name` non-optional) + `auth.sql:85` + `auth_projection.rs:59,71` | **FALSE** → `DA5-1` |
| 14 | The ordered pick is the difference between a bounded and an arbitrary anomaly (26–28) | follows from claim 9 given 11 or 12 | TRUE |
| 15 | "narrow the predicate rather than lean on that fallback" (28) | coherent advice under 11/12 | TRUE |
| 16 | Function doc: the durable key remains `(card_kind, card_uid)` (178) | `auth.sql:66–67` comment and `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` at `:84`; `auth_projection.rs:20` conflicts on exactly that key | TRUE |
| 17 | Function doc: the GIN index on `card_ref` serves this lookup (179) | `auth.sql:92–93` `USING GIN (card_ref)`; default `jsonb_ops` supports `@>` | TRUE |
| 18 | Function doc: binds the caller's ref as JSONB (176) | `service_accounts.rs:191` `.bind(Json(card_ref))` | TRUE |

### Coherence after the deletion (review question 3)

Partially. The *facts* sentence is now tighter and correct: claims 6 + 7 alone are
sufficient to bound the lookup to one row, so deleting the caller clause improved the
argument rather than weakening it, and "`ORDER BY created_at, id LIMIT 1` exists because
none of that chain is enforced here" plus "narrow the predicate rather than lean on that
fallback" both still follow.

What did not survive the edit is the *break-way* list. It still has three entries, and the
third one — "add an optional `CardRef` field" — was the mirror of the deleted third fact.
With the caller bound gone there is no stated fact for it to break, and on its own it does
not produce more rows. The deletion closed the false claim in one sentence and left its
counterpart standing in the next.

## 3. Verification limits

1. **Wire-level caveat on claim 3, disclosed not filed.** `IssueKeyRequest.card_ref`
   (`wyrd-spec/src/auth/issue_key.rs:15`), `RequestedSubject::CardRef`, and
   `CreateWorkloadBindingRequest.card_ref` (`wyrd-spec/src/auth/admin.rs:136`) are plain
   `CardRef`, whose `uid` deserializes from `#[serde(default)]`. An HTTP client that
   already knows a server-resolved uid could therefore name one, making "can only name"
   imprecise as an absolute. No in-tree caller surface does it (the CLI has no uid flag),
   nothing in the bound-to-one-row argument depends on it, and it is the symmetric
   counterpart of the `space` fail-open the round-2 review already ruled
   not production-reachable. Not filed.
2. **No Postgres executed.** Claims 6–8 and 11–13 are argued from the migration text, the
   projection source, and jsonb containment semantics. The remediation task lists a
   Postgres-backed matching test as a non-goal, so no `@>` row-count experiment was run.
3. **No prose test exists.** Nothing in the tree asserts doc-comment content, so every
   truth value above is read-and-trace, not executed.
4. I did not run `mise run lints`, `test:sql`, or any journey lane: the delta is
   doc-comment-only, and the binding rules forbid broad lanes as evidence. `cargo fmt
   --check -p wyrd-sql` and the one focused nextest expression are the whole verification.
5. `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` remains red independently;
   not touched and not attributed here.

## 4. Proposed findings

### `DA5-1` — INCORRECT

**Violated obligation.** AGENTS.md §16: rustdoc must explain intent and the relevant
invariants, and (§12) documentation is part of implementation correctness. Same obligation
`FIND-008-16` cited.

**Location.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:25–26`.

**Current text (lines 24–26).**

```
/// because none of that chain is enforced here: relax the unique constraint,
/// decouple the name projection, or add an optional `CardRef` field, and this
/// predicate starts matching more rows on a credential-issuing path.
```

**Evidence.** The chain is now two facts (line 21). Two of the three break-ways map onto
them; the third does not map onto anything and is false on its own terms:

- `CardRef.name` is non-optional (`crates/wyrd-spec/src/reference.rs:21`), so `$3` always
  carries a `name` key and `card_ref @> $3` always requires `card_ref->>'name'` to match.
- `auth_projection` keeps the `name` column equal to `card_ref->>'name'`
  (`auth_projection.rs:59` and `:71` bind the same `card.metadata.name`).
- `UNIQUE (data_tenant_id, name)` (`migrations/20260601000001_auth.sql:85`) then admits at
  most one such row per tenant, and `data_tenant_id = $1` plus RLS pins the tenant.

Adding an optional `CardRef` field therefore widens the set of *keys* containment relaxes
and cannot widen the matched row count while the two named facts hold. Under the previous
three-fact wording the clause was the counterpart of "every caller passing a fully
qualified ref"; `7009668ca` deleted that fact and kept its break-way.

**Observable consequence.** The comment exists to tell the next maintainer exactly what
holds a credential-issuing lookup to one row and exactly what would break it. It now names
three ways to break a two-link chain. A maintainer reads that a purely additive,
spec-level `CardRef` change endangers credential issuance — and the surviving clause
re-implies that the caller-supplied field set is load-bearing here, which is the precise
misconception `FIND-008-16` was filed to remove. Nothing in the tree catches doc prose.

**Testable correction.** Delete `, or add an optional `CardRef` field` from line 25 and
reflow, leaving "relax the unique constraint or decouple the name projection, and this
predicate starts matching more rows on a credential-issuing path." Acceptance:

```bash
# no break-way without a matching fact (must print nothing)
grep -n 'add an optional `CardRef` field' \
  crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs
# the sentence still names two facts and two break-ways
sed -n '19,28p' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs
# predicate unmoved
sed -n '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/p' \
  crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs | md5
# expect 2298c9e6b9cf3e40edb952818226b944
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
mise exec -- cargo fmt -p wyrd-sql -- --check
```

One doc line. No predicate, test, caller, migration, or fixture change. Do not replace it
with a hedge, do not re-add a caller-side fact, and do not narrow the predicate.

Against the TASK-008-R4 acceptance matrix: criteria 1, 2, 4 and 5 hold. Criterion 3 —
"the `ORDER BY created_at, id LIMIT 1` explanation still follows and still reads as a
fallback for an unenforced chain" — holds for the `ORDER BY` rationale itself but not for
the enumerated chain it rests on, which is why `FIND-008-16` is not fully closed.

## 5. Verdict

**FAIL** — `DA5-1` (INCORRECT) at `service_accounts.rs:25–26`. Everything else in scope
verifies: the predicate is byte-identical (md5 `2298c9e6b9cf3e40edb952818226b944` across
`f102e50ee`, `c34b9d1e0`, `ba223a4db`), no behavior changed, the pinning test selects
exactly one test and passes unweakened, and 17 of the 18 enumerated doc claims are true.

## Commands run

| Command | Result |
|---|---|
| `git rev-parse HEAD` | `ba223a4db0445d44a6f57eee21115da0b77adb48` |
| `git status --porcelain` | clean |
| `rtk proxy git diff c34b9d1e0..ba223a4db -- crates/` | one file, +3/−4, doc comment only |
| `rtk proxy git diff --name-only c34b9d1e0..ba223a4db` | 6 `review/task-008-r4/*.md` + `service_accounts.rs` |
| `rtk proxy git show <rev>:...service_accounts.rs \| sed -n '/^const SERVICE.../,/^        "#;/p' \| md5` (×3) | `2298c9e6b9cf3e40edb952818226b944` for `f102e50ee`, `c34b9d1e0`, `ba223a4db` |
| `grep -rn 'service_account_by_card_ref' crates/ sdks/` | 3 callers + def + test + re-export |
| `grep -rn 'insert_service_account' crates/ sdks/` | 2 production callers (both `None` card_ref), rest tests/fixtures |
| `grep -rn 'UNIQUE' migrations/20260601000001_auth.sql migrations/20260601000020_admin_principals.sql` | `UNIQUE (data_tenant_id, name)` at `20260601000001_auth.sql:85`, not dropped later |
| `mise exec -- cargo nextest list ... -E 'test(=...card_ref_uses_jsonb_card_ref_binding)'` | exactly one test |
| `mise exec -- cargo nextest run ... -E '<same>'` | 1 passed, 73 skipped, 0.005s |
| `mise exec -- cargo fmt -p wyrd-sql -- --check` | clean |
| `awk` line widths 10–28 | 69–84 cols, file-consistent |
