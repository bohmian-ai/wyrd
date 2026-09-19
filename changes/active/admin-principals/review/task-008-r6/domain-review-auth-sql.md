# Domain review — authentication SQL (`task-008-r6`), `domain-rev-auth-sql`

Verdict: **PASS**. Zero material findings.

## 1. Boundary, authority, source coverage

Reviewed boundary, traced end to end:

- `SERVICE_ACCOUNT_BY_CARD_REF_SQL` and `service_account_by_card_ref`
  (`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:10-38`, `:174-200`)
- three credential-issuing callers: `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:591`,
  `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:155`
- production writer of the Card-bound `card_ref`:
  `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:15-82`
- every other `insert_service_account` caller (23 sites; production set below)
- live table DDL, indexes, RLS: `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:65-101`,
  `20260601000009_auth_principal_revocation.sql`, `20260601000011_auth_relax_created_by_fk.sql`,
  `20260601000014_card_registration_operations.sql:154`,
  `20260601000020_admin_principals.sql:85-150`
- `CardRef`: `crates/wyrd-spec/src/reference.rs:14-38`
- history of the predicate: `f5008e6c1` (pre-fix `card_ref = $3`) → `708f01ec9` (`@>` + ordered `LIMIT 1`)

Authorities applied: `AGENTS.md` §11, §12, §13, §15, §16, and the round-6 materiality
bar in the subject file. The delta touches no contract, kind, header, route, SDK
surface, or Bifrost path, so `architecture/wyrd-design.md` and
`architecture/bifrost-design.md` are not reachable by it — I did not review against
them and claim no coverage there.

## 2. Preconditions

| Check | Result |
|---|---|
| HEAD | `b98ac9fa91670456b87dfbc82059038f47a0627e` ✔ |
| Branch | `claude/admin-principals-spec-qfsmjc` ✔ |
| Tree | clean (`git status --porcelain` empty; report dir created by me) ✔ |
| Delta file set `ba223a4db..b98ac9fa9` | 1 source file `+5/−5`, 6 markdown files under `review/task-008-r5/` ✔ |
| Predicate md5 (const block, git objects at both commits) | `2298c9e6b9cf3e40edb952818226b944` at `ba223a4db` **and** `b98ac9fa9` ✔ |

**Measurement-method warning for the next round.** The hook rewrites more than
`git`. Plain `diff -u prev.rs head.rs` on the two extracted git blobs printed
`✅ Files are identical` — which is **false**. `cmp` reported `differ: char 1208,
line 24`, and `rtk proxy git diff` showed the real ±5. Do not use bare `diff`,
`git diff`, or `git show` as evidence on this file; use `cmp`, `md5`, or
`rtk proxy`.

## 3. Claim-by-claim truth table

Enumeration is **complete**: every sentence of the const doc comment (`:10-28`)
and of the function doc (`:174-182`) is accounted for below — six const sentences,
four function-doc sentences, nothing skipped.

### Const doc comment, `service_accounts.rs:10-28`

| # | Claim | Confirming / refuting site | Verdict |
|---|---|---|---|
| S1 | "Resolve an active Card-bound principal from the Card identity a client can express." | `service_accounts.rs:32-35` — `card_ref @> $3` is NULL-rejecting, so the Card-free rows admitted by `20260601000020_admin_principals.sql:130-142` (`card_ref IS NULL`) never match; `status = 'active'` | TRUE |
| S2a | "registering a Card-bound principal stores a `uid`-bearing `card_ref` — the projection … writes `space: Some(..)` and `uid: Some(..)`" | `auth_projection.rs:58-64` (`space: Some(space.clone())`, `uid: Some(card_uid.clone())`), bound at `:74` | TRUE |
| S2b | implied exclusivity: the projection is the production writer | the only two production `insert_service_account` callers pass `card_ref = None` — `wyrd-server/src/components/platform/provisioning.rs:217-225` (`"tenant_admin"`, `None`) and `wyrd-server/src/components/principals/routes.rs:240-248` (`"service"`, `None`). All other call sites are tests/fixtures. | TRUE |
| S2c | "a caller can only name `space/Kind/name@version`" | `reference.rs:17-37`; the wire-level `uid`/`space` caveat is **settled** per the subject file and is corrected by S3 in the same paragraph. Not filed. | TRUE as scoped |
| S2d | "so `card_ref = $3` matched no registered principal at all" | historically exact: `f5008e6c1` shipped literally `AND card_ref = $3`; jsonb `=` is whole-document equality, and every projected ref carries `uid`, which no caller ref carries | TRUE |
| S3 | "Containment relaxes *every* optional `CardRef` field, `space` included: a ref with no space matches a row in any space." | `reference.rs:28-37` — `space` and `uid` are the only `Option`s, both `#[serde(default, skip_serializing_if)]`; `service_accounts.rs:32-38` has no `space` clause. Scope is one tenant (`data_tenant_id = $1` + RLS at `20260601000001_auth.sql:97-101`), which the same paragraph establishes. | TRUE |
| S4 | "What bounds this to one intended row … two facts outside it — the table's `UNIQUE (data_tenant_id, name)` and `auth_projection` keeping the `name` column equal to `card_ref->>'name'`." | `20260601000001_auth.sql:85` (retained — `20260601000020_admin_principals.sql` drops NOT NULL on the Card columns only, never `name`, and drops only `contype='c'` constraints matching `%principal_kind%` at `:113-128`); `auth_projection.rs:60` and `:72` bind the same `card.metadata.name`. `CardRef.name` is non-`Option` (`reference.rs:21`), so `$3` always carries `name` ⇒ ≤1 row. | TRUE |
| S5 | `ORDER BY created_at, id LIMIT 1` exists because the chain is unenforced here; two named break-ways each make the predicate match more rows on a credential-issuing path | `service_accounts.rs:36-37`; the predicate asserts neither fact. Break-ways analysed in §4 — both TRUE. "Credential-issuing path": all three callers mint credentials (`issue_api_key.rs:86-99` API key; `exchange_api_key.rs:583-595` delegated exchange; `jwt_bearer.rs:150-165` workload token). | TRUE |
| S6 | "The stable oldest-first pick is then the difference between a bounded anomaly and an arbitrary one — narrow the predicate rather than lean on that fallback." | `ORDER BY created_at, id` is total (`id` is the PK tiebreak, `20260601000001_auth.sql:69`), so the pick is deterministic rather than planner-dependent. Closing clause is prescriptive, not a factual claim about the code; the subject file forbids relitigating it. | TRUE |

### Function doc, `service_accounts.rs:174-182`

| # | Claim | Site | Verdict |
|---|---|---|---|
| F1 | "Find an active Service/Agent principal by card ref." | `:32-35`; callers pass `"service"` / `"agent"` via `principal_kind_for_card` | TRUE |
| F2 | "Binds the caller's ref as JSONB for [`SERVICE_ACCOUNT_BY_CARD_REF_SQL`], whose documentation carries what the predicate does and does not bound." | `:188-191` — `.bind(Json(card_ref))`; the const doc is that documentation | TRUE |
| F3a | "The durable key remains `(card_kind, card_uid)`" | `20260601000001_auth.sql:65-67` states it verbatim; enforced by `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` at `:87`, retained per `20260601000020_admin_principals.sql:97-100` | TRUE |
| F3b | "this is the lookup for the identity a client can express" | `reference.rs:17-37` | TRUE |
| F3c | "the GIN index on `card_ref` serves it" | `20260601000001_auth.sql:92-93` — default `jsonb_ops` GIN supports `@>`. (It would *not* have served the old `=`, so the fix and the index agree.) | TRUE |
| F4 | "# Errors / Returns the database error when the read fails." | `:183-192` returns `Result<_, sqlx::Error>`, unmapped | TRUE |

**No standing sentence depends on a premise removed in round 3, 4, or 5.** Checked
specifically:

- Round 4 deleted the caller-side third bounding fact. S4's remaining two facts
  bound the match to ≤1 row **without** any caller guarantee, because
  `CardRef.name` is non-`Option`. S3 asserts the *opposite* of the deleted fact
  (callers may omit `space`), so it is reinforced, not orphaned.
- Round 5 deleted the `CardRef`-field break-way. S6's "bounded anomaly" refers to
  the deterministic pick under a multi-row match, whose sources are now exactly
  S5's two break-ways. "That chain" (S5) = S4's two links. "That fallback" (S6) =
  the ordered `LIMIT 1`. Every referent resolves.
- Round 3 deleted the same-identity-two-Cards claim; no surviving sentence
  references card-identity collision.

## 4. The two surviving break-ways, constructed concretely

Predicate today: `data_tenant_id = $1 AND principal_kind = $2 AND card_ref @> $3
AND status = 'active'`, `$3` = `{kind, name, version}` at minimum.

**Break-way A — relax `UNIQUE (data_tenant_id, name)`.** TRUE, and live rather
than hypothetical. This constraint is unqualified by `space`, so registering
`spaceA/Service/foo@1.0.0` and `spaceB/Service/foo@1.0.0` in one tenant fails
today — which is precisely the out-of-scope cross-space-collision handoff the
verdict hands to the spec owner. The obvious fix is
`UNIQUE (data_tenant_id, space, name)`. Apply it and both rows land: distinct
`card_uid` means `auth_projection.rs:21` (`ON CONFLICT (data_tenant_id,
principal_kind, card_kind, card_uid)`) inserts rather than updates, and both
`card_ref` values contain `{kind: Service, name: foo, version: 1.0.0}`. A caller
that omits `space` — which S3 says it may — then matches **two active rows on the
API-key issuing path**, and the ordered `LIMIT 1` is the only thing making the
credential deterministic. The break-way is correctly stated, and a maintainer
taking that handoff needs exactly this warning.

**Break-way B — decouple the name projection.** TRUE, and independently
sufficient with `UNIQUE (data_tenant_id, name)` left intact. The other plausible
fix for the same collision is to qualify the *column* instead of the constraint:
have `auth_projection` bind `format!("{space}/{name}")` at `:72` while
`card_ref.name` at `:60` stays the bare Card name. Uniqueness on
`(data_tenant_id, name)` is then satisfied by `spaceA/foo` and `spaceB/foo`, yet
both rows still carry `card_ref->>'name' = 'foo'`, so the same space-less caller
ref matches two rows. Row count widens with the constraint untouched.

Both break-ways individually widen the match; the disjunction is correct. Neither
has the defect the deleted third break-way had — that one changed which *keys*
containment relaxed without being able to raise the row *count*, whereas A and B
each directly admit a second row satisfying the unchanged predicate. I attempted
to falsify both and could not.

Incidental, not a finding: `insert_service_account` (`:101-150`) already takes
`name` and `card_ref` as independent parameters, so break-way B is reachable
through that path without editing `auth_projection`. Both of its production
callers pass `card_ref = None`, so no Card-bound row is produced that way. This
strengthens S5's "none of that chain is enforced here" rather than contradicting
anything.

## 5. Behavior change

None. `service_accounts.rs` `+5/−5`, all `///` lines inside one sentence
(`rtk proxy git diff ba223a4db b98ac9fa9`). Const block md5 identical across both
commits. Lookup semantics, `TenantConn` scoping (`:189`
`conn.data_tenant_id().as_uuid()`), the `tenant_isolation` RLS policy with
`FORCE ROW LEVEL SECURITY`, the GIN index, `auth_projection`, every migration and
every fixture are untouched. No `#[allow]`, no `#[ignore]`, no test deleted or
weakened.

## 6. Pinning test

`mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E
'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`
→ `1 test run: 1 passed, 73 skipped`. Exactly one test selected; the expression
cannot pass vacuously. All five predicate assertions present and unweakened
(`:450-454`): `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`,
`LIMIT 1`, `!contains("card_ref::text")`, plus the `Json` round-trip at `:448`.
Byte-identical to `ba223a4db`.

## 7. R5 acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | exactly two break-ways, both real, no `CardRef`-field break-way | PASS — `grep 'add an optional \`CardRef\` field'` → no match (rc=1); `:24-25` names the two |
| 2 | two-fact list unchanged | PASS — `:21-23`, unchanged text |
| 3 | `LIMIT 1` rationale + closing clause survive and follow from a two-link chain | PASS — `:23-28`; §4 confirms both links carry it |
| 4 | every other sentence unchanged and true | PASS — diff confines the change to one sentence; §3 confirms all ten sentences TRUE |
| 5 | predicate md5 `2298c9e6b9cf3e40edb952818226b944` | PASS, from git objects at both commits |
| 6 | no other file changes | PASS — `git diff --name-status` shows one source file plus the r5 markdown packet |

## 8. Verification limits

- No Postgres was started; jsonb `@>`/`=` semantics, GIN `jsonb_ops` operator
  support, and NULL-rejection by `@>` were reasoned from Postgres semantics and
  the DDL, not executed. The subject file prohibits adding a Postgres-backed test
  and the delta is doc-only, so this limit is accepted.
- `mise exec -- cargo fmt -p wyrd-sql -- --check` clean; `awk 'length > 100 &&
  /^\/\/\//'` → no output. I did not run `mise run lints` or `mise run test:sql`
  (the implementor recorded both clean at `ef635d4ab`; a doc-comment rewrap
  cannot reach either, and the subject file bars broad lanes).
- Rustdoc rendering was not built; the `rustdoc::private_intra_doc_links` and
  `[IdError]` items are settled-out-of-scope per the subject file.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` were not
  reviewed — unreachable by this delta, as stated in §1.
- Prose truth is not machine-checkable; §3 rests on my reading of the ten
  sentences against the cited sites.

## 9. Findings

**None.** Zero material findings. This is the expected outcome and I state it
plainly rather than reaching for something to report.

Three observations were considered and **deliberately not filed**, each because it
changes only what a maintainer would *read*, never what they would *do*:

1. The R5 task and remediation evidence cite `auth_projection.rs:59` and `:71`
   for the name coupling; the actual lines are `:60` and `:72` (`:59` is
   `kind:`, `:71` is `.bind(space…)`). Off by one, in the markdown packet only —
   the source doc comment carries no line numbers, and the fact holds. Read-only.
2. S3's "a row in any space" is tenant-scoped rather than global; the same
   paragraph and `data_tenant_id = $1` establish that. Read-only.
3. The settled wire-level `uid`/`space` caveat on S2c — explicitly excluded by
   the subject file and not filed.

`FIND-008-16` is closed by this delta. `FIND-008-17` remains unassigned.

## 10. Commands run and results

```
git rev-parse HEAD                                    → b98ac9fa91670456b87dfbc82059038f47a0627e
git status --porcelain                                → clean
git rev-parse --abbrev-ref HEAD                       → claude/admin-principals-spec-qfsmjc
rtk proxy git show b98ac9fa9:…/service_accounts.rs    → 476 lines → scratchpad/head.rs
rtk proxy git show ba223a4db:…/service_accounts.rs    → 476 lines → scratchpad/prev.rs
diff -u prev.rs head.rs                               → "✅ Files are identical"  ← FALSE, hook-rewritten
cmp prev.rs head.rs                                   → differ: char 1208, line 24 (rc=1)
grep -n 'optional .CardRef. field' prev.rs head.rs    → head: line 19 only; prev: lines 19 and 25
md5 of sed -n '29,38p' on both                        → 2298c9e6b9cf3e40edb952818226b944 (both)
rtk proxy git diff ba223a4db b98ac9fa9 -- …           → +5/−5, one sentence of /// lines
rtk proxy git diff --name-status ba223a4db b98ac9fa9  → 1 M source file, 6 A markdown
rtk proxy git diff --numstat … -- crates/             → 5 5 service_accounts.rs
rtk proxy git log --oneline --follow -- …             → 20 commits; f5008e6c1 introduced the const
rtk proxy git show f5008e6c1:… | grep -A9 'FROM …'    → "AND card_ref = $3", no ORDER BY / LIMIT
rtk proxy git show 708f01ec9:… | grep -A9 'FROM …'    → "AND card_ref @> $3", ORDER BY created_at, id, LIMIT 1
sed -n '1,45p' / '95,200p' / '400,470p' service_accounts.rs   → const doc, fn docs, tests
sed -n '1,100p' cards/auth_projection.rs              → UPSERT_SQL, CardRef construction, binds
sed -n '1,45p' crates/wyrd-spec/src/reference.rs      → CardRef; name non-Option, space/uid Option
grep -rn 'auth_service_accounts' …/migrations/        → 28 hits across 7 migrations
sed -n '65,105p' 20260601000001_auth.sql              → DDL, 3 UNIQUEs, GIN, RLS FORCE + policy
sed -n '85,160p' 20260601000020_admin_principals.sql  → nullability, name-agnostic check drop, new checks
grep -rn 'service_account_by_card_ref|insert_service_account' crates sdks --include='*.rs'  → 3 read callers, 20 insert sites
sed -n '195,250p' platform/provisioning.rs            → insert_service_account(…, "tenant_admin", None, …)
sed -n '205,265p' principals/routes.rs                → insert_service_account(…, "service", None, …)
sed -n '85,100p' issue_api_key.rs / '583,598p' exchange_api_key.rs / '148,165p' jwt_bearer.rs → all credential-issuing
grep -n 'add an optional `CardRef` field' …           → no match (rc=1)
awk 'length > 100 && /^\/\/\//' …                     → no output
mise exec -- cargo fmt -p wyrd-sql -- --check         → clean (rc=0)
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
                                                      → 1 test run: 1 passed, 73 skipped
```

No `git config`, no `GIT_AUTHOR_*` / `GIT_COMMITTER_*`, no gate circumvention, no
source file modified.

## Verdict

**PASS**
