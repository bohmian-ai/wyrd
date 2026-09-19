# Task implementation review — TASK-008 round 5 (`task-008-r6`)

Reviewer: `task-rev` (fresh, independent). Candidate `b98ac9fa9`, base `ba223a4db`.
Standard: the six acceptance criteria of
`changes/active/admin-principals/review/task-008-r5/TASK-008-R5-delete-the-orphaned-break-way.md`.
Report returned as text by the reviewer; persisted here by the orchestrator.

## Acceptance matrix

| # | Obligation | Implementation evidence | Verification | Result |
|---|---|---|---|---|
| 1 | Break-way list names exactly two ways; no `CardRef`-field break-way remains anywhere | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:24-26` | `grep -n 'add an optional \`CardRef\` field' <file>` → exit 1, no match; `grep -n 'optional' <file>` → only `:19` (the containment-relaxes sentence) and `:248` (`expires_at`), neither a break-way | PASS |
| 2 | Two-fact list unchanged from `ba223a4db`, still naming `UNIQUE (data_tenant_id, name)` and the `auth_projection` name-column coupling | `:20-23` | Raw `rtk proxy git diff ba223a4db..b98ac9fa9 -- crates/` shows `:21-23` only as context | PASS |
| 3 | `ORDER BY created_at, id LIMIT 1` rationale and the closing "narrow the predicate rather than lean on that fallback" survive and follow from a two-link chain | `:23-28` | Verbatim readback below; entailment re-derived independently | PASS |
| 4 | Every other sentence of the const doc and function doc unchanged **and true as written** | const doc `:10-28`, fn doc `:174-182` | Raw diff = 5 `///` lines inside one sentence; claim-by-claim truth table below | PASS |
| 5 | SQL predicate byte-identical, md5 `2298c9e6b9cf3e40edb952818226b944` | `:29-38` | `rtk proxy git show b98ac9fa9:<file> \| awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' \| md5` → match; same at `ba223a4db` | PASS |
| 6 | One source file; no test, caller, migration, or fixture moved | — | `rtk proxy git diff --stat ba223a4db..b98ac9fa9` → 6 markdown under `review/task-008-r5/` + `service_accounts.rs 10 +-`; scoped raw diff over `crates/ sdks/ scripts/ mise.toml` shows only the 5-line `///` hunk | PASS |
| C-a | Predicate byte-identical to `f102e50ee`/`c34b9d1e0`/`ba223a4db` | as above | md5 match | PASS |
| C-b | Do not touch test, caller, `auth_projection`, migration, `wyrd-testing` fixture | — | Raw diff: none present | PASS |
| C-c | The two facts, the `ORDER BY … LIMIT 1` rationale, and the `space` fail-open statement at `:19-20` all stay | `:19-20`, `:20-23`, `:23-28` | Readback; all three unchanged | PASS |
| C-d | §12: no weakened/disabled check, no `#[allow]`, no deleted/`#[ignore]`d test | — | Raw diff contains no attribute or test change; `nextest list` shows the pinning test present and selected | PASS |
| C-e | §13: configured identity, no AI co-author trailer | — | `rtk proxy git log` for `ef635d4ab`/`b98ac9fa9`: author `Thorrester`, no trailer | PASS |
| N-1 | No caller-side guarantee restored to either list | `:20-23`, `:24-26` | Neither list mentions callers; the only surviving caller-side text is `:15-16`, unchanged since `708f01ec9` | PASS |
| N-2 | No `@>`→`=` revert, no `space` clause, no matching-semantics change | `:29-38` | md5 unchanged | PASS |
| N-3 | No `space`/`uid` wire validation added | — | Raw diff touches no Rust code | PASS |
| N-4 | No test, fixture, or assertion added | — | Raw diff touches no `#[cfg(test)]` block | PASS |
| N-5 | Explanation not moved off the const; intra-doc link unchanged; no lint added to a lane | `:176` link intact; `mise.toml` not in diff | Raw diff | PASS |
| N-6 | No history rewrite | — | `rtk proxy git log --oneline -15 -- <file>`: `708f01ec9` → `7009668ca` → `ef635d4ab` linear, prior SHAs intact | PASS |
| N-7 | Neither out-of-scope handoff folded in | — | `wyrd-testing/src/server.rs` and the cross-space `UNIQUE` question absent from the diff | PASS |
| Extra | Hand rewrap stays within the file's existing `///` width | `:24-28` | `awk`: file max `///` = 84 at `:14` (pre-existing, unchanged); the five new lines max 81. `cargo fmt --all -- --check` exit 0 | PASS |
| Extra | Pinning test passes, unweakened, selector selects exactly one | `:433-449` | `nextest list` → exactly 1 test; `nextest run` → `1 passed, 73 skipped`, 0.005s; five assertions present at `:444-448` | PASS |

## Verbatim readback — const doc, `service_accounts.rs:10-28`

```
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

## Criterion 4 — claim-by-claim truth table

| # | Claim | Line | Confirming evidence | Verdict |
|---|---|---|---|---|
| C1 | Resolves an **active**, **Card-bound** principal | `:10-11` | `status = 'active'` (`:35`); `card_ref @> $3` never matches a `card_ref IS NULL` row, and migration 20 forbids `card_ref` on `tenant_admin` (`20260601000020_admin_principals.sql:136-150`) | TRUE |
| C2 | "from the Card identity a client can express" | `:10-11` | All three callers derive `principal_kind` from `card_ref.kind` (`issue_api_key.rs:230-238`) and pass a caller-supplied `CardRef` | TRUE |
| C3 | Registering a Card-bound principal stores a `uid`-bearing `card_ref` | `:13-14` | `auth_projection.rs:58-64` builds `CardRef { …, uid: Some(card_uid) }`; `UPSERT_SQL` writes it | TRUE |
| C4 | The projection lives at `queries::cards::auth_projection` | `:14` | Path exists | TRUE |
| C5 | It writes `space: Some(..)` and `uid: Some(..)` | `:15` | `auth_projection.rs:62-63` | TRUE |
| C6 | "a caller can only name `space/Kind/name@version`" | `:15-16` | The settled wire-level caveat: `space`/`uid` are `#[serde(default, skip_serializing_if)] Option` (`reference.rs:28-37`), so a client holding a uid *could* name one. Corrected by the next clause, outside every write set, not in the two-fact list. Recorded, not filed | TRUE as settled |
| C7 | So `card_ref = $3` matched no registered principal at all | `:16-17` | jsonb `=` is exact; every stored ref carries `uid`, no caller ref does | TRUE |
| C8 | Containment relaxes *every* optional `CardRef` field, `space` included | `:19-20` | `@>` semantics; `space` is `Option` with `skip_serializing_if` (`reference.rs:28-30`) | TRUE |
| C9 | What bounds this to one row is **not the predicate** | `:20-21` | `:32-35` filters only tenant, kind, containment, status | TRUE |
| C10 | Fact A: the table's `UNIQUE (data_tenant_id, name)` | `:21-22` | `migrations/20260601000001_auth.sql:85`; migration 20 drops only `principal_kind` CHECKs and NOT NULLs (`:103-150`); `grep DROP CONSTRAINT` finds only migration 11 and migration 20's `contype='c'` loop | TRUE |
| C11 | Fact B: `auth_projection` keeps the `name` column equal to `card_ref->>'name'` | `:22-23` | `auth_projection.rs:60` binds `card.metadata.name` into the JSONB ref and `:71` the same value into the `name` column; `ON CONFLICT … DO UPDATE` rewrites both together. Both production callers of `insert_service_account` pass `card_ref: None` (`principals/routes.rs:244`, `platform/provisioning.rs:221`); every other caller is a test or fixture | TRUE |
| C12 | The two facts together bound the match to one row | `:20-23` | `CardRef::name` is `pub name: CardName`, no `Option`, no serde attribute (`reference.rs:20-21`) ⇒ every `$3` carries `name` ⇒ `@>` forces `card_ref->>'name'` to match ⇒ C11 forces the `name` column to match ⇒ C10 + `data_tenant_id = $1` admit ≤1 row. **No caller-side premise needed** | TRUE |
| C13 | "none of that chain is enforced here" | `:23-24` | Neither fact appears in `:29-38` | TRUE |
| C14 | Break-way 1: relax the unique constraint ⇒ more rows | `:24` | Without the constraint two active rows may share tenant+name with distinct `card_uid`; both satisfy `@> $3` | TRUE |
| C15 | Break-way 2: decouple the name projection ⇒ more rows | `:25` | If the `name` column no longer mirrors `card_ref->>'name'`, C10 constrains nothing about the JSONB name | TRUE |
| C16 | "on a credential-issuing path" | `:26` | `issue_api_key.rs:94`, `exchange_api_key.rs:591`, `jwt_bearer.rs:155` | TRUE |
| C17 | The stable oldest-first pick is the difference between a bounded and an arbitrary anomaly | `:26-28` | `ORDER BY created_at, id` + `LIMIT 1` (`:36-37`); `id` breaks a tie deterministically | TRUE |
| C18 | "narrow the predicate rather than lean on that fallback" | `:27-28` | Prescriptive, consistent with C9/C13 | TRUE |
| F1 | "Find an active Service/Agent principal by card ref" | `:174` | `status = 'active'`; `principal_kind_for_card` maps only `Service`/`Agent` | TRUE |
| F2 | Binds the caller's ref as JSONB for the const | `:176-177` | `:188` uses the const, `:191` `.bind(Json(card_ref))` | TRUE |
| F3 | Its documentation carries what the predicate does and does not bound | `:177` | The const doc `:19-28` does | TRUE |
| F4 | The durable key remains `(card_kind, card_uid)` | `:177-178` | `auth.sql:64-67` and `:84`; retained by migration 20 (`:96-99`) | TRUE |
| F5 | The GIN index on `card_ref` serves it | `:179` | `auth.sql:92-93`; default `jsonb_ops` supports `@>` | TRUE |
| F6 | `# Errors`: returns the database error when the read fails | `:181-182` | `:193` propagates `sqlx::Error` | TRUE |

### Orphan-dependency check — the question that failed in rounds 3, 4 and 5

The premise deleted this round is "a new optional `CardRef` field can widen the
match"; the premise round 4 deleted is "every caller passing a fully qualified
ref". Nothing remaining depends on either. C12's bound derives purely from the
non-optional `CardRef::name` plus the two in-tree facts, with no caller premise in
the chain. The only surviving caller-side sentence (C6) serves the `=`-vs-`@>`
explanation and is a member of neither list. C8 runs in the opposite direction —
it explains why the predicate does *not* bound — and is strengthened by the
deletion. **No claim is orphaned.**

## Proposed findings

**None.** The finding list is empty and that is the expected, complete outcome.

Every sentence of both doc comments was read against the tree; no false claim.
Two things were considered and are not findings under the round's materiality bar:
C6 (settled, and changes only what a maintainer reads) and the absence of a third
possible break-way (a completeness-of-explanation observation, explicitly
excluded; the task prescribed exactly two).

Ponytail ladder on the delta: rung 1 (delete rather than repair) is the right rung
and is what shipped — five words removed, nothing added, every added line a rewrap
of surviving text. This is the minimum.

## Result

**PASS**

## Verification limits

- Criteria 1-4 are prose; no command proves them. They rest on the verbatim
  readback plus the derivation above.
- `mise run lints`, `mise run test:sql`, and rustdoc lanes were not re-run — the
  subject bars broad lanes as evidence. The delta adds no code, symbol, or
  intra-doc link, so the clippy and rustdoc surface is unchanged by construction;
  `cargo fmt --all --check` passes.
- The predicate was not exercised against Postgres. C12, C14 and C15 are
  derivations from the DDL, `@>` semantics, and the `CardRef` type.
- `wyrd-testing/src/server.rs` fixtures do pass `Some(card_ref)` to
  `insert_service_account`; whether any writes a `name` differing from
  `card_ref->>'name'` was not audited. Fixture-only, cannot affect the production
  bound C11 asserts, and the stale comment there is a declared handoff.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` are not
  reachable by this delta; no coverage claimed against them.
- The `wyrd-diff` MCP server failed to connect; all diffing used `rtk proxy git`.
