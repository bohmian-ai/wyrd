# Wave 2 validation — TASK-008 round 5 (`task-008-r6`)

Reviewer: `ponytail-rev` (fresh, independent). Candidate `b98ac9fa9`, base
`ba223a4db`. Given no intended verdict. The reviewer's report write was refused by
the harness; this file is its report, persisted by the orchestrator.

**Validated ledger: EMPTY.** Wave 1's zero-finding conclusion is CONFIRMED on the
basis of independent source inspection, not their agreement. The reviewer
re-derived the load-bearing facts from `reference.rs`, `auth_projection.rs`, the
live DDL across all seven migrations touching the table, and both production
writers, and constructed both surviving break-ways against the schema **before**
reading any Wave 1 report. Only `domain-review-auth-sql.md` was on disk at that
time.

## 1. Mechanical criteria — independent verification

```
git rev-parse HEAD         → b98ac9fa91670456b87dfbc82059038f47a0627e
git rev-parse --abbrev-ref → claude/admin-principals-spec-qfsmjc
git status --porcelain     → ?? changes/active/.../review/task-008-r6/   (clean beyond permitted untracked)
```

| # | Criterion | Result |
|---|---|---|
| 1 | Two break-ways, no `CardRef`-field break-way | `grep -n 'add an optional \`CardRef\` field'` → rc=1. `grep -n 'CardRef\` field'` → line 19 only, which is A7's legitimate "relaxes *every* optional `CardRef` field", not a break-way. List at `:24-25` names exactly two | PASS |
| 2 | Two-fact list unchanged | vs `7009668ca`: byte-identical through `:23`; first divergence `:24`, inside the licensed sentence | PASS |
| 3 | `LIMIT 1` rationale + closing clause survive and follow from two links | `:23-28` present; §4 confirms both links carry it | PASS |
| 4 | Every other sentence unchanged and true | Diff confines the change to five `///` lines of one sentence; truth in §2 | PASS |
| 5 | Predicate md5 | PASS at four commits (below) |
| 6 | No other file changes | `--name-status` / `--numstat`: 1 source file `5/5` + 6 added markdown under `review/task-008-r5/`. No `mise.toml`, `scripts/`, `.github` in range | PASS |

Criterion 5, recomputed from git objects (not the working tree):

```
rtk proxy git show <rev>:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \
  | awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' | md5
b98ac9fa9 → 2298c9e6b9cf3e40edb952818226b944
ba223a4db → 2298c9e6b9cf3e40edb952818226b944
c34b9d1e0 → 2298c9e6b9cf3e40edb952818226b944
f102e50ee → 2298c9e6b9cf3e40edb952818226b944
```

Gate integrity and identity: `grep -E '^\+.*(#\[allow|#\[ignore|#!\[allow)'` over
the source diff → nothing. No `mise.toml` / `scripts/` / workflow file in range →
no check added or retired. `ef635d4ab` and `b98ac9fa9`: author and committer both
`Thorrester <sjforrester32@gmail.com>`; `grep -i -E 'co-authored|generated
with|claude'` over both messages → rc=1. No `git config`; no `GIT_AUTHOR_*` /
`GIT_COMMITTER_*`.

Narrow code checks (no `gate`, `test:rust`, family lane, or `--all-features`
workspace lane):

```
mise exec -- cargo fmt -p wyrd-sql -- --check                 → clean, rc=0
mise exec -- cargo clippy --locked -p wyrd-sql --lib --tests  → clean
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
                                                              → 1 passed, 73 skipped
```

Exactly one test selected, so the expression cannot pass vacuously. All five
predicate assertions intact (`:444-448`) plus the `Json` round-trip (`:443`).

Rewrap damage — the part `cargo fmt` cannot police:

```
backtick parity over :10-28      → every doc line EVEN (no split inline-code span)
awk 'length > 100 && /^\/\/\//'  → no output
doubled-word scan :10-28         → none
```

Longest `///` line is 84 **bytes** (fewer characters; em dashes are 3 bytes each).
No damage.

## 2. Independent sentence-by-sentence truth assessment

**Enumeration is complete**: const doc `:10-28` and function doc `:174-182`, split
into 10 const claims (A1-A10) and 6 function claims (B1-B6); every clause of both
is accounted for.

| # | Claim | Line | Evidence | Verdict |
|---|---|---|---|---|
| A1 | "Resolve an active Card-bound principal from the Card identity a client can express." | `:10-11` | `:34` `card_ref @> $3` — containment against a NULL left operand yields NULL, so the Card-free rows `20260601000020_admin_principals.sql:138-139` now permits never match; `:35` `status='active'` | TRUE |
| A2 | Registering a Card-bound principal stores a `uid`-bearing `card_ref` | `:13-14` | `auth_projection.rs:63` `uid: Some(card_uid.clone())`, serialized `:66`, bound `:74` | TRUE |
| A3 | "the projection at `queries::cards::auth_projection` writes `space: Some(..)` and `uid: Some(..)`" | `:14-15` | `auth_projection.rs:62-63` verbatim. Path resolves; backticked, not a rustdoc link, so no link lint is reachable | TRUE |
| A4 | The projection is the writer of Card-bound rows (implied exclusivity) | `:13-15` | The second writer, `insert_service_account`, takes `card_ref` and `name` as *independent* parameters. Both production callers pass `card_ref = None`: `provisioning.rs:217-225`, `principals/routes.rs:240-248`. All other sites are test or fixture, and fixtures derive `name` from the ref (`wyrd-testing/src/server.rs:2685`, `:4333`) | TRUE |
| A5 | "a caller can only name `space/Kind/name@version`" | `:15-16` | `reference.rs:17-37`. Wire-level `uid` caveat settled per the subject, corrected by A6 in the same paragraph. Not filed | TRUE as scoped |
| A6 | "so `card_ref = $3` matched no registered principal at all" | `:16-17` | jsonb `=` is whole-document equality; every projected ref carries `uid` (A2), no caller ref does (A5). Past tense, describes the fixed bug | TRUE |
| A7 | "Containment relaxes *every* optional `CardRef` field, `space` included: a ref with no space matches a row in any space." | `:19-20` | `reference.rs:28-29`, `:36-37` — `space`/`uid` are the only `Option`s, both `#[serde(default, skip_serializing_if = "Option::is_none")]`, so a `None` field is absent from `$3` and `@>` leaves it unconstrained. `kind`/`name`/`version` are non-`Option` with no skip attribute → always required. No `space` clause in the predicate. "Any space" is tenant-scoped by `data_tenant_id = $1` + RLS (`20260601000001_auth.sql:97-101`) | TRUE |
| A8 | The two-fact bound | `:20-23` | **Fact A**: `20260601000001_auth.sql:85`, retained — `20260601000020_admin_principals.sql:102-107` drops `NOT NULL` on the five Card columns only, never `name`; its `DO` block `:113-128` drops only `contype='c'` matching `%principal_kind%`, structurally unable to reach a `contype='u'`. **Fact B**: `auth_projection.rs:60` binds `card.metadata.name.clone()` into `card_ref.name`, `:72` binds `card.metadata.name.as_str()` into the `name` column — one source, two destinations. **Closure**: `CardRef.name` is `pub name: CardName`, non-`Option`, no serde attribute (`reference.rs:21`) → `$3` always carries `name` → `@>` always requires `card_ref->>'name'` to match → Fact B equates it to the `name` column → Fact A admits ≤1 such row per tenant. No caller premise anywhere in the derivation | TRUE |
| A9 | `ORDER BY created_at, id LIMIT 1` exists because the chain is unenforced here; two break-ways each widen the match on a credential-issuing path | `:23-26` | The predicate (`:32-35`) asserts neither fact → "not enforced here" is exact. Break-ways in §4, both TRUE and live. All three readers mint credentials: `issue_api_key.rs:94` (key generated `:101-105`), `exchange_api_key.rs:591`, `jwt_bearer.rs:155` | TRUE |
| A10 | "The stable oldest-first pick is then the difference between a bounded anomaly and an arbitrary one — narrow the predicate rather than lean on that fallback." | `:26-28` | `ORDER BY created_at, id` is **total** — `id` is the PK (`20260601000001_auth.sql:69`), so equal `created_at` still resolves deterministically, not by planner choice. The closing clause is prescriptive advice, not a factual assertion | TRUE |
| B1 | "Find an active Service/Agent principal by card ref." | `:174` | `:33`, `:35`; callers pass `"service"`/`"agent"` via `principal_kind_for_card` (`issue_api_key.rs:92`) | TRUE |
| B2 | "Binds the caller's ref as JSONB for [`SERVICE_ACCOUNT_BY_CARD_REF_SQL`], whose documentation carries what the predicate does and does not bound." | `:176-177` | `:191` `.bind(Json(card_ref))` | TRUE |
| B3 | "The durable key remains `(card_kind, card_uid)`" | `:178` | `20260601000001_auth.sql:66-67` states it verbatim; enforced by `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` at `:84`, explicitly retained per `20260601000020_admin_principals.sql:96-99` | TRUE |
| B4 | "this is the lookup for the identity a client can express" | `:178-179` | `reference.rs:17-37` + A5 | TRUE |
| B5 | "the GIN index on `card_ref` serves it" | `:179` | `20260601000001_auth.sql:92-93` `USING GIN (card_ref)`; default `jsonb_ops` supports `@>`. It would *not* have served the old `=`, so the fix and the index agree | TRUE |
| B6 | `# Errors` — "Returns the database error when the read fails." | `:181-182` | `:187` returns `Result<Option<…>, sqlx::Error>`; `:188-193` propagates unmapped | TRUE |

No claim FALSE. No claim UNVERIFIABLE, subject to the Postgres-semantics limit in
§9.

**Citation note affecting every `file:line` above:** line numbers are true file
offsets from `grep -n ''`. `cat -n` under this harness silently drops the file's
first three lines and renumbers, shifting citations by −3.

## 3. Orphan-hypothesis analysis — the failure mode that recurred in rounds 4 and 5

**Round 3's removed premise — "two Cards with the same identity."** No surviving
sentence references card-identity collision, duplicate Cards, or same-identity
registration. A8's bound is stated over the `name` column and `card_ref->>'name'`,
never over Card identity; A9's break-ways are a constraint and a projection.
**No orphan.**

**Round 4's removed premise — "every caller passing a fully qualified ref
(`IssueKeyArgs::space` is required)."** Tested hardest, since round 4 deleted the
fact and left its mirror standing:

- A8 names its bound exhaustively — "not the predicate but **two** facts outside
  it" — and both are in-tree schema/projection facts. The ≤1-row result was
  re-derived independently from `reference.rs:21` + Fact A + Fact B; it closes
  **without any premise about what callers supply**. The caller premise is not
  merely deleted from the list, it is absent from the derivation.
- A7 asserts the *logical negation* of the removed premise (callers may omit
  `space`) — reinforced, not orphaned.
- A9's break-ways are both schema/projection-side, not caller field-set claims.
- A5 concerns what a caller *cannot* name, the opposite direction, and is settled.

**No orphan.**

**Round 5's removed premise — "or add an optional `CardRef` field."** Every
referent bracketing the deletion resolves: "none of **that chain**" (`:24`) → A8's
two facts; "**that fallback**" (`:28`) → the ordered `LIMIT 1` named at `:23`; "a
**bounded** anomaly" (`:26-27`) → bounded by A10's total ordering, whose only
multi-row sources are now exactly A9's two break-ways, both real. Arithmetic: "two
facts" (2) ↔ two break-ways (2) ↔ "that chain". Aligned; the count mismatch round
5 caught is gone and no new one was created. **No orphan.**

The inverse was also checked — whether the deletion orphaned anything by removing a
needed referent. The deleted clause was referenced by nothing, and A7 independently
carries the true statement about optional `CardRef` fields, so the paragraph still
tells a maintainer what an added optional field does. Nothing was lost.

**Conclusion: a third orphan was looked for specifically and does not exist.**

## 4. Falsification attempt on both surviving break-ways

Predicate: `data_tenant_id = $1 AND principal_kind = $2 AND card_ref @> $3 AND
status='active'`, `ORDER BY created_at, id LIMIT 1`. `$3` always carries at least
`{kind, name, version}`.

**Break-way A — "relax the unique constraint."** Falsification attempted and
failed; **true and live**. `UNIQUE (data_tenant_id, name)` is unqualified by
`space`, so registering `prod/Agent/runtime@1.0.0` and
`staging/Agent/runtime@1.0.0` in one tenant fails **today** on that constraint —
precisely the out-of-scope cross-space handoff. The natural fix is
`UNIQUE (data_tenant_id, space, name)`. Apply it and both rows land: distinct
`card_uid` means `auth_projection.rs:21`'s
`ON CONFLICT (data_tenant_id, principal_kind, card_kind, card_uid)` inserts rather
than updates. Both `card_ref` values contain
`{"kind":"Agent","name":"runtime","version":"1.0.0"}`. A caller ref omitting
`space` — licensed by A7, and absent from `$3` via `skip_serializing_if` — then
satisfies `@>` against **both** active rows at `issue_api_key.rs:94`, and the
ordered `LIMIT 1` is the only thing making the issued credential deterministic.

**Break-way B — "decouple the name projection."** Falsification attempted and
failed; **true and independently sufficient with Fact A fully intact**. The other
plausible fix for the same collision qualifies the *column* instead of the
constraint: bind `format!("{space}/{name}")` at `auth_projection.rs:72` while
`card_ref.name` at `:60` stays bare. `UNIQUE (data_tenant_id, name)` is then
satisfied by `prod/runtime` and `staging/runtime` — no migration, constraint
untouched — yet both rows still carry `card_ref->>'name' = "runtime"`, so the same
space-less ref matches two rows.

Neither has the defect the deleted third break-way had: that one changed which
*keys* containment relaxes without being able to raise the row *count* (omitted →
unchanged; set-and-projected → narrows; set-and-unprojected → zero). A and B each
directly admit a second row satisfying the unchanged predicate.

**Incidental, deliberately not filed.** `insert_service_account` already takes
`name` and `card_ref` as independent parameters, so break-way B is reachable
through that path with no edit to `auth_projection`. Both production callers pass
`card_ref = None`, so no Card-bound row is produced that way today (A4). This
*strengthens* A9's "none of that chain is enforced here" — the coupling is one
writer's convention, not an invariant — and contradicts nothing. Naming that second
path would be a completeness-of-explanation improvement, which the materiality bar
excludes, and "decouple the name projection" covers it in substance.

## 5. The bare-`diff` measurement warning

**It holds, and it is worse than reported.** Reproduced on the two extracted git
blobs:

```
wc -c  old.rs → 16402    new.rs → 16368     (34-byte difference)
md5    old.rs → c21ff0bf4566b8b83d13e3c2b3b54d3d
       new.rs → 16b4cd7d13b9e6c3aa0ad1d5714a4ca3
cmp    → "differ: char 1208, line 24"      rc=1
diff -u → "✅ Files are identical"          rc=0   ← FALSE
rtk proxy diff -u → the real ±5 hunk        rc=1
```

The hook rewrites `diff`, not only `git`. The failure mode is a **false negative
with exit status 0**, which would silently satisfy any byte-identity assertion
written as `diff a b && echo same` or as a `set -e` step. The orchestrator
independently reproduced the exit-0-on-differing-files behavior.

**A second instance:** `cat -n` is also rewritten — it omitted this file's first
three lines (`//!` module doc plus the raw-query allowlist comment) and renumbered
from the fourth, shifting every citation by −3.

**Safe measurement set:** `cmp`, `md5`, `wc -c`, `grep -n ''`, `awk`, `sed -n`,
anything under `rtk proxy`. Not bare `diff`, `git diff`, `git show`, or `cat -n`.
Tooling hazard, not a candidate defect — recorded, not filed.

## 6. Final validated ledger

**EMPTY**, and validated as empty rather than rubber-stamped: the doc was
enumerated independently (§2, 16 claims, complete), every load-bearing fact
re-derived from source (`reference.rs:21`, `auth_projection.rs:60`/`:72`,
`20260601000001_auth.sql:85`/`:92-93`,
`20260601000020_admin_principals.sql:102-150`), both writers and all three readers
traced, and both break-ways constructed before any Wave 1 report was opened.

**The failure mode that recurred in rounds 4 and 5 was looked for specifically** —
a standing sentence depending on a premise an earlier round deleted — and all three
removed premises were tested against all sixteen surviving claims (§3). No third
orphan exists. The two-fact bound now closes on non-optional `CardRef.name` plus
two in-tree schema facts with no caller premise in the derivation, which is
structurally why this round differs from the last two: the premise class that
produced both prior residues is eliminated from the paragraph, not merely trimmed.

No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding.
`FIND-008-17` remains unassigned.

Three observations considered and **not filed** — each changes only what a
maintainer would *read*, never what they would *do*:

1. The R5 task and evidence cite `auth_projection.rs:59`/`:71` for the name
   coupling; the true lines are `:60`/`:72`. Markdown packet only — the shipped doc
   carries no line numbers and the fact holds.
2. A9's "relax the unique constraint" is singular where the table has three
   `UNIQUE` constraints; the referent is fixed by A8 one sentence earlier. Wording.
3. The `insert_service_account` second-writer path (§4) is unnamed in the doc.
   Completeness of explanation, excluded, covered in substance by break-way B.

**Ladder applied to the delta itself.** Rung 1 is the right rung: the correction is
a deletion, five words, no replacement prose. Repairing the clause into a true
statement would be longer and would duplicate what A7 already says. Nothing beyond
the deletion and the rewrap entered the file — `+5/−5`, all `///`, one sentence,
one file — and the rewrap is verified damage-free (§1). This is the minimum.

## 7. Prior-finding closure

| Finding | Status | Basis |
|---|---|---|
| `FIND-008-1` .. `FIND-008-15` | **Closed, confirmed still closed** | Closed in rounds 2-4. This delta cannot reopen any: the entire source change is five `///` lines in one sentence, predicate md5 identical at all four commits, no test, caller, migration, fixture, or check moved |
| `FIND-008-16` | **CLOSED** | The orphaned `, or add an optional \`CardRef\` field` is gone (`grep` rc=1); the list names exactly the two real links; all six R5 criteria PASS (§1); and the misconception the finding existed to remove — that the caller-supplied field set is load-bearing here — is absent, with the ≤1-row bound verified to derive with no caller premise (§2 A8, §3) |
| `FIND-008-17` | Unassigned | No finding filed |

## 8. Spec revision

**`SPEC_REVISION_REQUIRED` does not apply.** The delta changes five `///` lines of
a private const's doc comment: no specification obligation, no observable
behavior, no contract, no expensive-to-reverse decision. The two out-of-scope
handoffs already belong to the spec owner, pre-date this delta, and are not created
or changed by it.

## 9. Verification limits

**Nothing blocks this review.**

- **No Postgres started.** jsonb `@>` object containment, `@>` returning NULL
  against a NULL left operand, whole-document `=`, and `jsonb_ops` GIN support for
  `@>` were reasoned from documented Postgres semantics plus the DDL, not executed.
  The R5 non-goals forbid adding a Postgres-backed test and the delta is doc-only,
  so this is accepted rather than closed. It is the only place the truth assessment
  rests on unexecuted semantics.
- **Broad lanes not run, by instruction.** Substituted crate-scoped
  `cargo fmt -p wyrd-sql -- --check` and
  `cargo clippy --locked -p wyrd-sql --lib --tests`, both clean.
- **No journey lane.** Unreachable by a doc change.
  `auth_e2e::cache_ttl_path_also_flips_verdict` was not attributed here.
- **Rustdoc not built.** `RR4-1` and the pre-existing `[IdError]` warning are
  settled out of scope and were not re-examined.
- **Two Wave 1 reports absent at review time.** `task-review.md` and
  `standards-review.md` were not yet on disk. The validation is independent of all
  three, so their absence changes no conclusion.
- **Architecture authorities not reviewed.** Unreachable by this delta; no coverage
  claimed.
- **Prose truth is not machine-checkable.** §2 rests on reading sixteen claims
  against the cited sites. No grep or test can fail on doc prose, which is the
  standing reason this comment has cost four rounds.

## 10. Overall

**Ledger empty.** Wave 1's zero-finding conclusion confirmed.
