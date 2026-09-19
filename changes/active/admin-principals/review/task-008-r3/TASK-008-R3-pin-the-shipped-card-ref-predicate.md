---
task: TASK-008-R3
title: Pin the shipped card_ref predicate and document what actually bounds it
spec: SPEC-admin-principals
spec_revision: 7
reviews: changes/active/admin-principals/review/task-008-r3/
obligations: [REQ-047, AC-014]
findings: [FIND-008-15, FIND-008-16]
status: implemented
---

## Subject

| Item | Value |
|---|---|
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Prior remediation task (implemented) | `changes/active/admin-principals/review/task-008-r2/TASK-008-R2-close-header-prose-and-credential-gaps.md` |
| Reviewed candidate | branch `claude/admin-principals-spec-qfsmjc`, HEAD `f102e50ee` |
| Review verdict | `changes/active/admin-principals/review/task-008-r3/verdict.md` — `FIX_REQUIRED` |
| Validated ledger | `changes/active/admin-principals/review/task-008-r3/findings-validation.md` |

Every finding from the previous round (`FIND-008-8`..`FIND-008-14`) is closed and
stays closed. Two findings remain, both in
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, and **one edit to
that file closes both**. No behavior changes. No new file, test file, dependency,
fixture, or check.

Read the ruling in §1.4-§1.6 of `findings-validation.md` before editing. It
establishes the two facts these corrections depend on, and you should not
re-derive or contradict them:

1. `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-63` writes a
   `card_ref` carrying `space: Some(..)` and `uid: Some(..)`, reached
   unconditionally from card registration at
   `crates/wyrd/wyrd-server/src/components/cards/service.rs:1072-1074`. It is the
   only production writer of a Card-bound `card_ref`, and it keeps the `name`
   column equal to `card_ref->>'name'`.
2. `wyrd.auth_service_accounts` carries an unconditional
   `UNIQUE (data_tenant_id, name)`. No unique constraint or index mentions
   `space`.

Together those make `card_ref @> $3` the correct minimal repair — whole-document
equality could never match a uid-bearing stored row against the uid-free document
a client can express — and they are what actually bounds the query to one row.
**The predicate itself is accepted and must not change.** Do not revert it to
`=`, do not add a `space` equality clause, and do not change
`ORDER BY created_at, id LIMIT 1`. Three Wave 1 reviewers proposed variations on
those; all were rejected on evidence.

---

## FIND-008-15 — REGRESSION — the guard test still asserts the predicate the remediation replaced

**Violated obligation.** AGENTS.md §12 Completion Standard — "Format, lints, and
the targeted tests/checks for the touched surface pass"; §11 Verification Scope —
a `wyrd-sql` edit runs the nearest crate lane. `architecture/agent-rules.md`
classes this `BLOCK_BEFORE_MERGE`.

**Current behavior.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`
asserts `SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")`. The constant
at `:15` now reads `AND card_ref @> $3`. The test fails:

```text
Summary [0.627s] 1 test run: 0 passed, 1 failed
panicked at .../service_accounts.rs:431:
assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")
```

**Why the candidate's proof falls short.** The remediation's recorded command list
contains no `wyrd-sql` lane at all. `mise run test:sql` and any
`-p wyrd-sql --lib` invocation are red on the branch, and the test whose entire
job is to pin the resolution operator now pins the wrong one — so after this fix
is applied *nothing* in the tree objects if the predicate is changed back.

**Observable consequence.** A hard red in the fast lane, on the crate the
remediation edited, plus the loss of the only assertion that guards the predicate
string.

**Correction.** Edit the existing test
`queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`
in place. Assert the shipped predicate — `contains("card_ref @> $3")` — and add
the two assertions that make the test pin what the remediation changed rather
than merely restate the old line: `contains("ORDER BY created_at, id")` and
`contains("LIMIT 1")`. Keep the existing `principal_kind = $2` assertion and the
existing `!contains("card_ref::text")` negative assertion; both still hold and
both still matter. Reuse the test that is already there — add no file, no
fixture, no dependency, and no second test.

**Proof.**

```bash
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
```

Confirm the selector with `mise exec -- cargo nextest list -p wyrd-sql --lib`
first. Then run `mise run test:sql` to show the lane is green.

---

## FIND-008-16 — DRIFT — the rustdoc justifies single-row resolution with a bound that does not exist

**Violated obligation.** AGENTS.md §16 — "Rustdoc MUST explain intent, how the
item participates in the surrounding workflow, and relevant invariants or side
effects"; §12 — "Documentation is part of implementation correctness".

**Current behavior.** The rustdoc at
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:157-165` justifies
`LIMIT 1` with "Two active principals sharing one Card identity would require two
Cards with the same identity" (`:162-165`), and presents the predicate as "the
lookup for the identity a client can express".

Both statements mislead:

- That is not what bounds the query. Two rows with different `card_uid` and
  identical `kind`/`name`/`version` *do* satisfy `@>` for a space-less ref —
  measured `t` against the repository Postgres. What actually holds the match to
  one row is `UNIQUE (data_tenant_id, name)` combined with
  `cards::auth_projection::upsert_service_account_from_card` keeping the `name`
  column equal to `card_ref->>'name'`. Neither is mentioned anywhere in the file.
- Containment relaxes **every** optional `CardRef` field, not only `uid`. A caller
  that omits `space` matches a principal in any space; a caller that names a
  *wrong* space still refuses. The doc records neither.

**Why the candidate's proof falls short.** Nothing executable can catch a doc
comment that is wrong about the schema. This is also precisely the gap that made
three of four Wave 1 reviewers reach conflicting conclusions about whether the
shipped predicate is safe, and it cost a full Postgres investigation to settle:
the safety argument exists, but it is nowhere in the code.

**Observable consequence.** The next maintainer reads an exactness guarantee the
predicate does not provide and a bound that does not exist. Because the real bound
is incidental rather than designed, dropping `UNIQUE (data_tenant_id, name)`,
decoupling the projection's `name` from `card_ref->>'name'`, or adding an optional
field to `CardRef` would each silently turn the space relaxation into a real
widening on a credential-issuing path, with nothing in the tree objecting.

**Correction.** Amend that rustdoc only. No predicate change, no behavior change,
no new test. Replace the "two Cards with the same identity" sentence with the
operative facts:

- containment relaxes every optional `CardRef` field, `uid` and `space` included,
  so a caller that omits `space` matches any space while a wrong space refuses;
- single-row resolution is held by `UNIQUE (data_tenant_id, name)` together with
  `cards::auth_projection::upsert_service_account_from_card` — the only production
  writer of a Card-bound `card_ref` — keeping the `name` column equal to
  `card_ref->>'name'`;
- `ORDER BY created_at, id LIMIT 1` is the deterministic guard for the anomaly
  that chain would have to break.

Reuse the existing doc comment; the existing `tenant_admin_principal_id`
cross-reference may stay once the real bound is named. State the facts plainly —
do not argue the safety case at length, and do not restate the review.

**Proof.** The same command as `FIND-008-15` (the file must still compile and its
tests pass), plus `mise run fmt` and `mise run lints`.

---

## Constraints, preserved behavior, non-goals

**Preserve unchanged.** The `card_ref @> $3` predicate, its
`ORDER BY created_at, id LIMIT 1`, the tenant predicate, the
`TenantConn`/`OperatorPool` boundary, every caller of
`service_account_by_card_ref` (`issue_key`, `exchange_api_key`, `jwt_bearer`), the
write side (`auth_projection.rs`, the stored `card_ref` shape, the durable
`(tenant_id, principal_kind, card_kind, card_uid)` key), and everything the
previous round closed: the four deleted CLI error variants, `workflow.svx`, both
`BadTokenFormat` mappers, `login.rs`'s scrubbed refusal and its test,
`parse_callback_input` and `check_lease` rustdoc, `auth_issue_key_journey.rs`, the
`cli:dev-bootstrap` deletion, the clap `disable_version_flag` fix and its
`debug_assert` test, and the `principal_journey` harness generalization.

**Constraints.**

- The original task's **Prohibited changes** and **Material Stop Conditions**
  remain in force verbatim.
- Add **no** new file, test file, dependency, abstraction, helper, fixture, or
  repository check. Both corrections are edits to lines that already exist.
- Do not circumvent a gate: no `#[allow]`, no `#[ignore]`, no deleted or weakened
  test. In particular, do not close `FIND-008-15` by deleting the assertion or the
  test — the point is that the predicate stays pinned.
- Do not rewrite git history.

**Non-goals — all rejected by validation; do not do these.**

- Do not revert `card_ref @> $3` to `=`, and do not "fix the fixture instead"
  (`DD3-5`). That would restore a production bug in which no Card-bound principal
  can be resolved.
- Do not add a `card_ref->>'space' = $4` clause or any other narrowing predicate
  (`TR3-2`, `RR3-2`, `DD3-2`). A wrong space already refuses; only one candidate
  row can exist; there is no space-scoped authorization to protect.
- Do not add a Postgres-backed `wyrd-sql` test for the matching semantics
  (`DD3-4`). `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs` already pins
  them end to end at the tier AGENTS.md §11 ranks highest, and cannot pass under
  the old predicate.
- Do not change `crates/wyrd/wyrd-testing/src/server.rs`, the
  `UNIQUE (data_tenant_id, name)` constraint, or the `name`-column projection.
  Those are recorded as out-of-scope handoffs in the verdict.
- Do not repair `mise run test:platform:journey`'s missing `setup:postgres`
  dependency, and do not attempt to fix
  `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` — confirmed
  pre-existing at the branch base and outside the write set.

## Acceptance criteria

| # | Criterion | Finding |
|---|---|---|
| 1 | `service_account_by_card_ref_uses_jsonb_card_ref_binding` asserts `card_ref @> $3`, `ORDER BY created_at, id`, and `LIMIT 1`, retains its `principal_kind = $2` and `!card_ref::text` assertions, and passes | `FIND-008-15` |
| 2 | `mise run test:sql` is green, and no test was deleted or `#[ignore]`d to achieve it | `FIND-008-15` |
| 3 | The rustdoc on `SERVICE_ACCOUNT_BY_CARD_REF_SQL` names the relaxation of every optional `CardRef` field including `space`, names `UNIQUE (data_tenant_id, name)` plus the `auth_projection` `name`-column coupling as what bounds resolution to one row, and no longer claims the "two Cards with the same identity" bound | `FIND-008-16` |
| 4 | The SQL predicate, its `ORDER BY`/`LIMIT`, every caller, and the write side are byte-identical to `f102e50ee` | both |
| 5 | No preserved behavior above changed, no non-goal was entered, and no gate was weakened | both |

## Verification

Focused proof — fails at `f102e50ee`, passes after the correction:

```bash
mise exec -- cargo nextest list -p wyrd-sql --lib | grep card_ref
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
```

Then, scoped to the touched surface:

```bash
mise run fmt
mise run lints
mise run test:sql
```

Confirm the predicate and its callers did not move:

```bash
rtk proxy git diff f102e50ee..HEAD -- crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs
```

The diff must touch only the rustdoc and the test assertions. Do not offer a broad
aggregate (`mise run gate`, `test:rust`, a whole-crate or family lane, the storage
matrix, any `--all-features` workspace lane) as evidence for any criterion above.

## Authority links

- Approved spec: `changes/active/admin-principals/spec.md`, revision 7
- Original task: `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`
- This round's verdict and ledger: `changes/active/admin-principals/review/task-008-r3/`
- Previous round: `changes/active/admin-principals/review/task-008-r2/`
- `AGENTS.md` §11, §12, §15, §16; `architecture/agent-rules.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`

---

## Implementation Evidence

Commit `708f01ec9`. One file: `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`.

| # | Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|---|
| 1 | The test asserts the shipped predicate | `service_account_by_card_ref_uses_jsonb_card_ref_binding` now asserts `card_ref @> $3`, `ORDER BY created_at, id` and `LIMIT 1`, keeping `principal_kind = $2` and `!card_ref::text`; it gained rustdoc naming why the text is pinned | `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` — PASS; red before the edit | PASS |
| 2 | The SQL lane is green, nothing deleted or ignored | no test removed or `#[ignore]`d; the existing assertion was corrected, two added | `mise run test:sql` — 120 + 4 + 113 + 2 passed, 0 failed; `mise exec -- cargo nextest run --locked -p wyrd-sql --lib` — 74 passed | PASS |
| 3 | The rustdoc names the real bound | doc moved onto `SERVICE_ACCOUNT_BY_CARD_REF_SQL`: containment relaxes every optional `CardRef` field including `space`; one row comes from `UNIQUE (data_tenant_id, name)`, `auth_projection` keeping the `name` column equal to `card_ref->>'name'`, and callers passing a qualified ref (`IssueKeyArgs::space` required); the ordered `LIMIT 1` is there because none of that is enforced here, with the instruction to narrow the predicate if a link is dropped. The "two Cards with the same identity" claim is gone. The function doc now points at the const. | inspection against AGENTS.md §16; `mise run lints` | PASS |
| 4 | SQL, `ORDER BY`/`LIMIT`, callers and write side byte-identical to `f102e50ee` | `sed -n '/SERVICE_ACCOUNT_BY_CARD_REF_SQL: &str/,/"#;/p'` md5 matches `f102e50ee`'s (`2298c9e6b9cf3e40edb952818226b944`); the commit changes only doc comments and three assertion lines | `git diff --stat f102e50ee` — one file, docs plus assertions | PASS |
| 5 | Nothing preserved changed, no non-goal entered, no gate weakened | no other file touched; no `#[allow]`, `#[ignore]`, or deleted test | `mise run lints`; `mise run test:sql` | PASS |

### Process correction carried forward

`mise run test:sql` was missing from the TASK-008 and R2 verification sets even
though the change touched `wyrd-sql`. That omission, not the predicate, is why a
red unit test shipped. It is in the command list below and in the R2 record's.

```bash
mise run fmt
mise run lints
mise run test:sql
mise exec -- cargo nextest run --locked -p wyrd-sql --lib
mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
  -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
git diff --check
```
