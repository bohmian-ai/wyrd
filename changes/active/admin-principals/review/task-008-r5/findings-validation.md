# ponytail-rev — Wave 2 validation, TASK-008 round 5 (`task-008-r5`)

Ledger: **NON-EMPTY** — one finding, `FIND-008-16` (reused, `INCORRECT`).

State: `git rev-parse HEAD` → `ba223a4db0445d44a6f57eee21115da0b77adb48`;
`git status --porcelain` → only untracked `changes/active/admin-principals/review/task-008-r5/`.
Not `BLOCKED`.

## 1. Resolution of the three-way disagreement

The disputed clause, read from the git object `ba223a4db:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`,
lines 23-26:

```
/// column equal to `card_ref->>'name'`. `ORDER BY created_at, id LIMIT 1` exists
/// because none of that chain is enforced here: relax the unique constraint,
/// decouple the name projection, or add an optional `CardRef` field, and this
/// predicate starts matching more rows on a credential-issuing path.
```

**The literal claim is FALSE.** `domain-rev-auth-sql` is right; `task-rev` and
`repo-rev` are wrong.

Constructed case, as instructed. Add `foo: Option<Foo>` to `CardRef` with
`#[serde(default, skip_serializing_if = "Option::is_none")]`, and let
`auth_projection` write it. `$3` is `Json(card_ref)` of the caller's ref
(`service_accounts.rs:191`); the predicate is
`data_tenant_id = $1 AND principal_kind = $2 AND card_ref @> $3 AND status = 'active'`
(`:32-35`). Three exhaustive sub-cases:

| caller sends | `$3` key set | effect on `card_ref @> $3` | matched rows |
|---|---|---|---|
| `foo` omitted | unchanged | unchanged | identical set |
| `foo` set, projection writes it | one extra required key/value | strictly stricter | same or fewer |
| `foo` set, projection does not write it | one extra required key absent from every stored row | never satisfiable | zero |

Adding keys to the *stored* side likewise cannot create a match for a fixed
`$3`: `@>` ignores keys of the left object that `$3` does not name. So an
additional optional `CardRef` field can only hold the matched set constant or
shrink it. It can never grow it.

And the row count is independently capped at one while the two surviving facts
hold, for any key set whatsoever:

- `CardRef.name` is **non-optional** (`crates/wyrd-spec/src/reference.rs:21`,
  `pub name: CardName`, no `skip_serializing_if`), so `$3` always carries `name`
  and `card_ref @> $3` always requires `card_ref->>'name'` to equal it.
- `auth_projection` binds the same `card.metadata.name` into both the `card_ref`
  JSON (`crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:60`) and the
  `name` column (`:71`, `$5` of `UPSERT_SQL` at `:15-26`), and is the only
  production writer of a non-NULL `card_ref` — the two other production
  `insert_service_account` callers pass `None`
  (`wyrd-server/src/components/principals/routes.rs:243`,
  `.../platform/provisioning.rs:220`).
- `UNIQUE (data_tenant_id, name)` is live at
  `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85` and is not
  dropped by `20260601000020_admin_principals.sql` (its `DO` loop filters
  `contype = 'c'`, `:121`; the migration only relaxes `NOT NULL` on
  `card_kind/card_uid/card_ref/space/version`, `:103-107`).
- `data_tenant_id = $1` plus `FORCE ROW LEVEL SECURITY` + `tenant_isolation`
  (`20260601000001_auth.sql:97-101`) pins the tenant.

At most one active row per tenant carries a given `card_ref->>'name'`. Row count
≤ 1 regardless of how many optional fields `CardRef` grows.

No reading rescues the clause:

- *A field added without the projection writing it* → zero matches (fail closed),
  not more.
- *A field added without `skip_serializing_if`* → `"foo": null` always in `$3`,
  absent from rows written before the field existed → zero matches. A regression,
  still not "more rows".
- *Non-tenant-scoped* → not reachable; `$1` is bound from
  `conn.data_tenant_id()` (`:189`) and RLS is `FORCE`.
- *A future where fact A or B is independently relaxed* → already covered by
  break-ways 1 and 2. The clause adds nothing true there, and is offered as an
  independent third way.

**Why I side against the two PASS reviewers.** Both defenses concede the point
in substance. `task-rev` maps the third break-way onto "the containment
relaxation stated at `:16-18`" — i.e. onto the paragraph's own opening sentence
at `:19-20` ("Containment relaxes *every* optional `CardRef` field"), which is a
statement about relaxed *keys*, not about row count. `repo-rev` says the same
outright and calls it "loose phrasing, not an inaccuracy". But the sentence does
not say the new field is relaxed; it says the predicate "starts matching more
rows on a credential-issuing path", coordinated with two clauses that genuinely
do produce more rows. Defending it requires reading "matching more rows" as
"relaxing more keys" — which is both not what it says and, if it were, a verbatim
duplicate of the sentence two lines above. Either way the clause should go.

**Material, not polish.** I weighed the case for rejecting it honestly: a
subordinate clause, one private const, doc-only, round four, and a maintainer who
obeys it merely over-narrows a predicate. Three things outweigh that:

1. It is inside the R4 task's own acceptance boundary, not new scope. Criterion 3
   requires the `ORDER BY ... LIMIT 1` explanation to "still read as a fallback
   for an unenforced chain". The chain is now two links (`:21`) and the same
   sentence enumerates three ways to break it. The delta corrected the premise
   and left its consequent standing in the next clause.
2. The intended correction outcome of TASK-008-R4 is "the const rustdoc names
   only bounds the tree actually enforces". The surviving clause re-implies that
   the caller-supplied field set is load-bearing at this query — the exact
   misconception `FIND-008-16` exists to remove.
3. `AGENTS.md` §16 makes rustdoc that misstates the relevant invariants an
   implementation defect, and §12 makes it part of completion. Two prior rounds
   each found exactly one false claim in this same doc comment; the base rate
   here is not "polish".

## 2. Per-proposed-finding validation

| Wave 1 ID | Claim | My independent evidence | Result |
|---|---|---|---|
| `DA5-1` (domain-rev) | Third break-way at `:25-26` is false after the fact deletion | `reference.rs:21` (`name` non-optional) + `auth_projection.rs:60,71` + `auth.sql:85` + `service_accounts.rs:32-35`; three-case containment table in §1 | **CONFIRMED**, re-filed as `FIND-008-16` (see §3 for the ID choice) |
| `task-rev` — declined to file `:25-26` | The three break-ways map onto two facts plus the `:19-20` relaxation | The `:19-20` relaxation is a key-set claim; the disputed clause asserts a row-count consequence. Mapping is not truth. | **REJECTED as a non-finding position** — overruled |
| `repo-rev` — declined to file `:25-26` | "Loose phrasing, not an inaccuracy" | Same; the defense re-reads "matching more rows" as something other than row count | **REJECTED as a non-finding position** — overruled |
| `RR4-1` (round 3) | `rustdoc::private_intra_doc_links` | Settled in round 3; subject file forbids re-filing | Not considered |

Spot-checks of the agreed items, all independently re-run, all holding:

| Check | Result |
|---|---|
| predicate md5 from git objects, `rtk proxy git show <rev>:...` \| `sed -n '/^const SERVICE.../,/^        "#;/p'` \| `md5` | `2298c9e6b9cf3e40edb952818226b944` at `f102e50ee`, `c34b9d1e0`, `ba223a4db` |
| `rtk proxy git diff --stat c34b9d1e0..ba223a4db -- crates/` | one file, +3/−4 |
| `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` | 1 passed, 73 skipped, 0.006s |
| five pinned assertions at `service_accounts.rs:448-453` | present, unweakened |
| `git log -2 --format='%an <%ae> / %cn <%ce>'` | both commits authored **and** committed by `Thorrester <sjforrester32@gmail.com>`; no AI co-author trailer |
| `grep -n 'IssueKeyArgs\|three facts' service_accounts.rs` | no match — criterion 1 and the word count in criterion 2 hold |

### Independent adversarial sweep of the rest of the doc comment (review question 5)

`domain-rev-auth-sql` enumerated 18 claims. Its enumeration is *almost*
complete: it omits the summary line `:10-11` ("Resolve an active Card-bound
principal from the Card identity a client can express" — TRUE: `status =
'active'` at `:35`, a non-NULL `card_ref` is required for `@>` to match, and the
bound ref is the client-expressible identity) and the function's `# Errors`
clause at `:181-182` (TRUE). Both omissions are true claims, so the count 17/18
understates the enumeration but not the verdict.

Re-examined the two ranges named in my brief:

- **`:12-17`.** Claims 1-4 hold (`auth_projection.rs:59-63` writes
  `space: Some(..)`, `uid: Some(..)`; `reference.rs:32-37` skips `uid` when
  `None`, so jsonb `=` against a uid-bearing stored row could not match). One
  imprecision, **disclosed not filed**: "a caller can only name
  `space/Kind/name@version`" is (a) not absolute — `IssueKeyRequest.card_ref`
  and `RequestedSubject::CardRef` are plain `CardRef` whose `uid` is
  `#[serde(default)]`, so a wire client that already holds a resolved uid could
  name one, though no in-tree caller surface does — and (b) reads as if a caller
  always supplies a `space`. It is not filed because it is the *vocabulary* of
  the ref, its purpose is the uid asymmetry that explains the `=` failure, and
  the very next sentence at `:19-20` explicitly states the no-space case ("a ref
  with no space matches a row in any space"). That is the line between this and
  `FIND-008-16`: here the paragraph supplies its own correction two lines later;
  the disputed clause is contradicted by nothing in the paragraph and asserts a
  causal row-count consequence.
- **`:26-28`.** "The stable oldest-first pick is then the difference between a
  bounded anomaly and an arbitrary one — narrow the predicate rather than lean on
  that fallback." TRUE under break-ways 1 and 2, and it survives the proposed
  five-word deletion unchanged. No orphan here.

No other claim was orphaned by `7009668ca`.

## 3. Final deduplicated ledger

### `FIND-008-16` (reused) — `INCORRECT`

**ID choice.** Reused rather than minting `FIND-008-17`. The defect is the
surviving counterpart of the premise `7009668ca` deleted: same doc comment, same
sentence, same obligation (`AGENTS.md` §16 / §12), same misconception (that the
caller-supplied field set bounds this lookup), and it is TASK-008-R4's own
criterion 3 that fails. `FIND-008-16` is therefore **not closed**. Round 4 set
the precedent by reusing the round-2 ID for exactly this reason.

| Field | Value |
|---|---|
| Wave 1 source | `DA5-1` (`domain-rev-auth-sql`) |
| Status | CONFIRMED |
| Classification | `INCORRECT` |
| Violated obligation | `AGENTS.md` §16 — rustdoc MUST explain intent and the relevant invariants; §12 — documentation is part of implementation correctness. TASK-008-R4 acceptance criterion 3 and its intended correction outcome. |
| Location | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:25` (clause), sentence spanning `:23-26` |

**Evidence.** The bound is two facts (`:21-23`). The break-way list is three
(`:24-26`). The third, "add an optional `CardRef` field, and this predicate
starts matching more rows on a credential-issuing path", is false: `CardRef.name`
is non-optional (`crates/wyrd-spec/src/reference.rs:21`) so `$3` always requires
a `name` match; `auth_projection` couples the `name` column to
`card_ref->>'name'` (`auth_projection.rs:60`, `:71`); `UNIQUE (data_tenant_id,
name)` (`20260601000001_auth.sql:85`) then admits at most one active row per
tenant. An additional optional field widens the set of relaxed *keys* and can
only hold the matched row count constant or reduce it — see the three-case table
in §1.

**Observable consequence.** The comment's job is to tell the next maintainer what
holds a credential-issuing lookup to one row and what would break it. A
maintainer reads that a purely additive, spec-level `CardRef` field endangers
credential issuance (it does not), and reads back in that the caller-supplied
field set is load-bearing at this query (it is not) — which is the misconception
`FIND-008-16` was filed to remove. Nothing in the tree asserts doc prose, so no
gate catches it.

**Decision-complete correction.** Reuse the existing owner — the same doc comment
on the same private const, edited the same way round 4 was. Delete the five words
`, or add an optional \`CardRef\` field` from line 25 and reflow to the file's
existing width, leaving:

```
/// because none of that chain is enforced here: relax the unique constraint or
/// decouple the name projection, and this predicate starts matching more rows on
/// a credential-issuing path.
```

Two facts, two break-ways, one-to-one. Nothing else changes: no predicate, test,
caller, migration, or fixture edit; no hedge; no re-added caller-side fact; no
new validation.

**Ladder applied to the correction.** (1) Delete — yes, and deletion *is* the
whole correction; there is nothing to add. (2) Existing repository behavior —
the true content of the clause is already stated at `:19-20` ("Containment
relaxes *every* optional `CardRef` field"), which is precisely why the clause is
redundant as well as wrong; no new prose is needed. (3)-(4) N/A, no code or
dependency. (5) Minimum: five words. The repair alternative — rewording it into a
true statement — would have to say either something `:19-20` already says, or
something conditioned on facts A/B having already given way, which break-ways 1
and 2 already cover. Repair is strictly larger and yields duplication. Delete.

**Smallest focused closure proof.**

```bash
# the false break-way is gone (must print nothing)
grep -n 'add an optional `CardRef` field' \
  crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs

# two facts, two break-ways, read back in full
sed -n '19,28p' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs

# predicate unmoved — expect 2298c9e6b9cf3e40edb952818226b944
sed -n '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/p' \
  crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs | md5

mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
mise exec -- cargo fmt -p wyrd-sql -- --check
```

## 4. Rejected / omitted, with reasons

- **`task-rev`'s and `repo-rev`'s declinations on `:25-26`** — omitted as
  positions, overruled in §1. Both rest on reading "matching more rows" as
  "relaxing more keys"; the text says the former, and if it meant the latter it
  duplicates `:19-20`.
- **`RR4-1` (private intra-doc link)** — not re-filed; settled in round 3 and
  prohibited by the subject file.
- **The `space` fail-open at the predicate level** — rejected in round 2 as not
  production-reachable; unchanged by this delta.
- **Wire-level `uid` caveat on `:15-16`** — disclosed in §2, not filed: true for
  every in-tree caller surface, nothing in the bound-to-one-row argument depends
  on it, and it is the symmetric counterpart of the already-rejected `space`
  fail-open.
- **The `space/` reading of `:15-16`** — disclosed, not filed: corrected by
  `:19-20` inside the same paragraph. Filing it would be reviewer polish; rung 1
  of the ladder (delete the finding) holds.
- **Cross-space `UNIQUE (data_tenant_id, name)` collision; stale comment at
  `crates/wyrd/wyrd-testing/src/server.rs:2670-2672`** — spec-owner handoffs, not
  this task.
- **`Co-Authored-By: Claude` trailers on pre-`9fe02aa2e` commits** — outside every
  write set under review; `7009668ca` and `ba223a4db` carry none.
- **`wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict`** — red
  independently of this work; not attributed.

## 5. Prior-finding closure

- `FIND-008-1` .. `FIND-008-15`: **closed**, confirmed. Nothing in this delta or
  in my sweep reopens any of them; the delta is +3/−4 doc-comment lines in one
  file and the predicate is byte-identical to `f102e50ee`.
- `FIND-008-16`: **NOT closed.** Criteria 1, 2, 4 and 5 of TASK-008-R4 pass.
  Criterion 3 fails on the enumerated chain inside the same sentence: the false
  premise was deleted and its consequent left standing.
- `FIND-008-17`: still unassigned.

## 6. Spec-revision assessment

The single retained correction deletes five words from one `///` line on a
private const. It requires no product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, or persistent-data decision.
**`SPEC_REVISION_REQUIRED` does not apply.**

## 7. Verification limits

1. **No Postgres executed.** The containment/row-count argument is derived from
   the migration text, the projection source, `CardRef`'s serde attributes, and
   jsonb `@>` semantics. A Postgres-backed matching test is an explicit non-goal
   of TASK-008-R4, so no row-count experiment was run. This is the one limit that
   bears on `FIND-008-16`; the argument is nonetheless closed under the three
   exhaustive serialization cases in §1 and does not depend on runtime behavior.
2. **No prose test exists.** Nothing in the tree asserts doc-comment content, so
   every truth value here is read-and-trace.
3. **Broad lanes not used as evidence**, per the subject file: no `gate`, no
   `test:rust`, no family lane, no `--all-features` workspace lane. Verification
   is the one focused nextest expression, `cargo fmt -p wyrd-sql --check` (run by
   `domain-rev-auth-sql`), and git-object reads.
4. **Two Wave 1 reports were not on disk** when I reviewed
   (`task-review.md`, `standards-review.md`); I worked from their summarized
   positions as supplied and re-derived the disputed clause from source rather
   than from any reviewer's report.
5. Nothing blocks this review.

## 8. Overall

**Ledger NON-EMPTY** — one finding, `FIND-008-16` (reused, `INCORRECT`), one
five-word doc deletion at `service_accounts.rs:25`.
