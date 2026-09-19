# Wave 2 findings validation — TASK-008 review `task-008-r3`

Reviewer: `ponytail-rev` (fresh, independent). Candidate `f102e50ee`, branch
`claude/admin-principals-spec-qfsmjc`, base `968c92641`. Working tree clean; no
source file changed by this review.

> Persisted by the orchestrator from the reviewer's returned report (the
> reviewer's own file write was blocked by the harness). The orchestrator
> independently re-verified the two load-bearing facts in §1.4 and §3 before
> persisting: `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-63`
> writes `space: Some(..)`, `uid: Some(..)`, reached unconditionally from
> `crates/wyrd/wyrd-server/src/components/cards/service.rs:1072-1074`; and
> `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431` asserts
> `card_ref = $3` against the `card_ref @> $3` predicate at `:15`.

Four Wave 1 reports proposed eleven findings (`TR3-1`, `TR3-2`, `RR3-1`, `RR3-2`,
`DS3-1`, `DS3-2`, `DD3-1`..`DD3-5`). After dedupe and independent verification the
final ledger holds **two** findings, both in one file, both closable by one edit.

## 1. The contradiction, resolved empirically

### 1.1 What the reviewers disagreed about

`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15` moved from
`card_ref = $3` to `card_ref @> $3` with `ORDER BY created_at, id LIMIT 1`
(`:17-18`). `domain-rev-security` ruled it safe because
`UNIQUE (data_tenant_id, name)` bounds the match to one row; `domain-rev-data`
reported it had *measured* two matching rows across spaces `alpha`/`beta`,
silently reduced by `LIMIT 1`; `task-rev` and `repo-rev` agreed with the data
reviewer that containment is fail-open on `CardRef.space`.

### 1.2 Live schema — the actual constraints

Migrations applied through the repository-managed environment
(`scripts/postgres/with-test-postgres.sh` + `mise run db:migrate:all:inner`, both
migration lanes green), then `pg_constraint`/`pg_indexes` enumerated on
`wyrd.auth_service_accounts`:

| name | type | definition |
|---|---|---|
| `auth_service_accounts_pkey` | p | `PRIMARY KEY (id)` |
| `auth_service_accounts_data_tenant_id_id_key` | u | `UNIQUE (data_tenant_id, id)` |
| `auth_service_accounts_data_tenant_id_name_key` | u | `UNIQUE (data_tenant_id, name)` |
| `auth_service_accounts_data_tenant_id_principal_kind_card_ki_key` | u | `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` |
| `auth_service_accounts_card_binding_check` | c | Card columns all-NULL or all-NOT-NULL |
| `auth_service_accounts_kind_card_check` | c | `agent`→`Agent`, `service`→`Service`/NULL, `tenant_admin`→NULL |

No unique constraint or index mentions `space`. The only `card_ref` index is
`auth_service_accounts_card_ref_gin` (GIN), which serves `@>` and did **not**
serve `=`.

### 1.3 Containment semantics — measured

```text
stored = {"kind":"Agent","name":"runtime","version":"1.0.0","space":"prod","uid":"1111…"}

stored @> {"kind":"Agent","name":"runtime","version":"1.0.0"}                  -> t
stored @> {"kind":"Agent","name":"runtime","version":"1.0.0","space":"alpha"}  -> f
stored  = {"kind":"Agent","name":"runtime","version":"1.0.0","space":"prod"}   -> f
```

So: omitting `space` matches any space; naming a **wrong** space still refuses;
and whole-document equality could never match a uid-bearing stored row against
the uid-free document every client can express
(`crates/wyrd-spec/src/reference.rs:33-38`, `uid` is
`skip_serializing_if = "Option::is_none"`; `FromStr` at `:503-540` sets
`uid: None` unless the caller writes `#<uid>`).

### 1.4 The fact that decides it — `domain-rev-data`'s load-bearing claim is false

`domain-rev-data` (§2.4, `DD3-3`, `DD3-5`) rests on *"no production writer of a
Card-bound `card_ref` exists"*, traced from the two `insert_service_account` call
sites (`components/principals/routes.rs:240`,
`components/platform/provisioning.rs:217`), which do both pass `None`. That trace
stopped at one function instead of the table.

There is a second, production writer:

- `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:14-25,57-63` —
  `upsert_service_account_from_card` inserts into `wyrd.auth_service_accounts`
  with `card_ref` built as `uid: Some(card_uid)`, `space: Some(space)`,
  `name: card.metadata.name`.
- Reached unconditionally from card registration:
  `crates/wyrd/wyrd-server/src/components/cards/service.rs:1072-1074` —
  `if matches!(card.kind, CardKind::Service | CardKind::Agent) { upsert_service_account_from_card(...) }`.

So registering any Service or Agent Card in production stores a **uid-bearing**
`card_ref`, and under `card_ref = $3` that principal could never be resolved by
any client-expressible ref — on all three callers (`issue_key`,
`exchange_api_key`'s `RequestedSubject::CardRef`, `jwt_bearer`). The defect the
`@>` change fixes is a real production bug, not a fixture artifact.

### 1.5 Ruling

**No reviewer was wholly right; `domain-rev-security` was right on the outcome,
`domain-rev-data` was right on the mechanism and wrong on the fact that
mattered.**

- The `@>` change is **required and correct**. `old_equality_matches = f` above,
  plus the production projection writer, is the proof.
- The match is bounded to **at most one candidate row in production**, but not for
  the reason `domain-rev-security` gave. The operative chain is:
  `auth_projection.rs:57-71` writes `name` column `= card_ref->>'name'`, it is the
  only production writer of a Card-bound `card_ref`, and
  `UNIQUE (data_tenant_id, name)` is unconditional. Every row satisfying
  `card_ref @> {kind,name,version}` therefore carries the same `name`, and at most
  one row per tenant may carry it. `LIMIT 1` is defensive, not a silent reducer.
- `domain-rev-data`'s measured two-row state is real **only** through
  `insert_service_account`, whose `name` and `card_ref` are independent
  parameters. No production caller passes a Card-bound `card_ref` there, and the
  one that writes Card-bound rows keeps the two in lockstep. The measurement does
  not describe a production-reachable state.
- The space fail-open exists at the predicate level (`t` above) but is not
  reachable from the CLI — `IssueKeyArgs.space` is a required `String`
  (`crates/wyrd/wyrd-cli/src/auth/issue_key.rs:28-30`) and `CardRef::FromStr`
  always sets `space: Some(_)`. It is reachable from a raw HTTP body via
  `#[serde(default)]`, and grants nothing: authorization on that route is the
  tenant-wide `Permission::service_accounts_write()` and its resource label is
  already space-insensitive (`components/auth/routes.rs:279-285`,
  `format!("card:{}", request.card_ref.name)`; `src/audit/mod.rs:267-284`). A
  caller omitting `space` reaches the single same-named principal it could reach
  by naming that principal's space correctly.

### 1.6 The two questions that follow

**(1) Is `card_ref @> $3` the minimum correct fix? Yes.**

The ladder runs against the actual owner. The root cause is that the shared read
predicate demanded a field no client can supply, while the production writer
always supplies it — so the shared predicate *is* the correct owner, and one
character-class change there fixes all three callers at once.
`domain-rev-data`'s option 1 (fix `bootstrap_machine_in_tenant`, revert the
predicate) is smaller in lines but lands in the wrong owner: it would make the
fixture agree with a broken predicate and leave every production Card-bound
principal unresolvable. That is the inverse error the ladder warns about — a
production bug papered over to suit a test. Rejected. Its option 2's extra
`card_ref->>'space' = $4` predicate is not required either: naming a wrong space
already refuses, and only one candidate row can exist. More code, no property
gained.

**(2) Does any retained correction need an approved decision? No.**

`SPEC_REVISION_REQUIRED` does **not** apply. `REQ-004` (`spec.md:179-184`)
requires Card-bound principals keep their `card_ref`, `card_ref_scope`,
`wyrd apply` provisioning and emit-scope behavior unchanged; the non-goal
(`spec.md:149-150`) is *changing* `wyrd apply` Card-bound provisioning. Nothing
on the write side moved: the stored shape, the durable
`(tenant_id, principal_kind, card_kind, card_uid)` key named in the baseline
(`spec.md:55-63`), and the projection are all untouched. The change restores the
read path so the behavior `REQ-004` presumes actually works. Repairing a
predicate that could never match a row the spec's own baseline describes as
stored is a bug fix an implementor owns, not a persistent-data decision.
`domain-rev-data`'s escalation is rejected because the premise it rests on — "no
production writer, so the stored shape is not yet a production fact" — is false
(§1.4).

## 2. Per-proposed-finding validation

| Wave 1 ID(s) | Claim | My independent evidence | Ruling |
|---|---|---|---|
| `TR3-1`, `RR3-1`, `DS3-1`, `DD3-1` | A stale unit test asserts `card_ref = $3` and is red | `service_accounts.rs:431` asserts `contains("card_ref = $3")`; predicate at `:15` is `@> $3`. Reproduced: `1 test run: 0 passed, 1 failed` — `panicked at …service_accounts.rs:431: assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")` | **CONFIRMED** → one finding, `FIND-008-15` |
| `TR3-2`, `RR3-2`, `DD3-2` | Containment is fail-open on `space`; a caller omitting it resolves a principal in a space it never named, second match silently dropped | Predicate-level widening real (`omit_space_matches = t`). Reported consequence not reachable: only one candidate row can exist in production (§1.5); wrong-space refuses (`f`); CLI cannot omit `space` (`issue_key.rs:28-30`); no space-scoped authorization exists to broaden (`audit/mod.rs:274`, resource `card:{name}`). The measured two-row state needs the test-only decoupled-`name` path | **REJECTED** — speculative / non-production-reachable path; residual risk recorded as `FIND-008-16` instead |
| `DS3-2`, `DD3-3` | The new rustdoc asserts an invariant the tree contradicts | `service_accounts.rs:157-165`. *"Two active principals sharing one Card identity would require two Cards with the same identity"* is not the operative bound — `UNIQUE (data_tenant_id, name)` is (§1.5), and the doc never states it. The doc also calls this *"the lookup for the identity a client can express"* without recording that an omitted `space` widens the match to every space (measured `t`) | **CONFIRMED (merged)** → one finding, `FIND-008-16` |
| `DD3-4` | No test at any tier pins the new matching semantics | False. `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs:41-92` pins it end to end: `bootstrap_service` stores a uid-bearing `card_ref` (`wyrd-testing/src/server.rs:4619-4629` → `:4514`), the CLI names a uid-free one, and `old_equality_matches = f` proves the journey cannot pass under `=`. Ran it: `PASS [3.863s] wyrd-cli::cli auth_issue_key_journey::auth_issue_key_cli_journey`. AGENTS.md §11 ranks that journey **above** a SQL-tier pin | **REJECTED** — coverage exists at the tier the repository ranks highest; a redundant tier-2 test is not required by the task |
| `DD3-5` | The shared predicate was relaxed to work around a fixture divergence, outside approved scope; spec revision required | Same defect as `DD3-2` plus an out-of-scope claim, and its load-bearing fact is false (§1.4). The fixture divergence is real but runs the other way: `server.rs:2670-2672` strips the uid *"for the exact JSONB lookup"*, i.e. the fixture was bent to the broken predicate, while production stores the uid | **REJECTED** — not a separate defect; premise falsified; no spec revision required (§1.6) |

### Dedupe performed

- `TR3-1` + `RR3-1` + `DS3-1` + `DD3-1` → `FIND-008-15` (verified identical: all
  four name `service_accounts.rs:431` and the same assertion).
- `DS3-2` + `DD3-3` → `FIND-008-16` (verified identical target: the rustdoc at
  `:157-165`).
- `TR3-2` + `RR3-2` + `DD3-2` → one finding, then rejected.
- `DD3-5` is the same defect as `DD3-2`, not a separate `DRIFT`. Decided: merged
  and rejected with it.

## 3. Final deduplicated ledger

### `FIND-008-15` — REGRESSION

- **Wave 1 sources:** `TR3-1`, `RR3-1`, `DS3-1`, `DD3-1`. **CONFIRMED.**
- **Violated obligation:** AGENTS.md §12 Completion Standard (*"Format, lints, and
  the targeted tests/checks for the touched surface pass"*); §11 Verification
  Scope (a `wyrd-sql` edit runs the nearest crate lane). `agent-rules.md`
  `BLOCK_BEFORE_MERGE`.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`
  (assertion), against the predicate at `:15`.
- **Evidence:** `assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3"))`
  while the constant now reads `AND card_ref @> $3`. Reproduced in 0.6 s:
  `Summary [0.627s] 1 test run: 0 passed, 1 failed`.
- **Observable consequence:** `mise run test:sql` — and any `-p wyrd-sql --lib`
  lane — is red on the branch. The candidate's own remediation record lists no
  `wyrd-sql` lane among its commands, which is how a hard red shipped.
- **Correction (decision-complete):** in the existing test
  `queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`,
  change the stale assertion to assert the shipped predicate —
  `contains("card_ref @> $3")` — and add the two assertions that make the test pin
  what changed rather than restate it: `contains("ORDER BY created_at, id")` and
  `contains("LIMIT 1")`. Keep the existing `principal_kind = $2` and
  `!contains("card_ref::text")` assertions. Reuse the test that is already there;
  add no file, no fixture, no dependency.
- **Closure proof:**

  ```bash
  mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
    -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
  ```

  (selector confirmed present via `cargo nextest list`).

### `FIND-008-16` — DRIFT

- **Wave 1 sources:** `DS3-2`, `DD3-3`. **REVISED** (narrowed to the two statements
  I can falsify; `DD3-3`'s items 1 and 3 do not survive as stated — item 1's
  *"stored `card_ref` carries the registered Card's `uid`"* is **true** of the
  production writer at `auth_projection.rs:57-63`, and the
  `tenant_admin_principal_id` analogy is defensible once the real bound is
  stated).
- **Violated obligation:** AGENTS.md §16 (*"Rustdoc MUST explain intent … and
  relevant invariants or side effects"*); §12 (*"Documentation is part of
  implementation correctness"*).
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:162-165`
  (within the rustdoc at `:157-165`).
- **Evidence:** the doc justifies single-row resolution with *"Two active
  principals sharing one Card identity would require two Cards with the same
  identity"*. That is not what bounds the query. Two rows with different
  `card_uid` and identical `kind/name/version` do satisfy `@>` for a space-less
  ref (`omit_space_matches = t`, §1.3); what actually holds the match to one row
  is `UNIQUE (data_tenant_id, name)` (§1.2) combined with
  `auth_projection.rs:57-71` writing the `name` column equal to
  `card_ref->>'name'`. Neither is mentioned. The doc also presents the predicate
  as *"the lookup for the identity a client can express"* without recording that
  `space` — and any optional field `CardRef` gains later — is relaxed too, not
  only `uid`.
- **Observable consequence:** the next maintainer reads an exactness guarantee the
  predicate does not provide and a bound that does not exist. Both the incidental
  nature of the real bound and the space relaxation are invisible, so dropping
  `UNIQUE (data_tenant_id, name)`, decoupling the projection's `name` from
  `card_ref->>'name'`, or adding an optional `CardRef` field would silently make
  the rejected `TR3-2`/`RR3-2`/`DD3-2` widening real on a credential-issuing
  path, with nothing in the tree objecting.
- **Correction (decision-complete):** amend that rustdoc only — no behavior
  change, no predicate change, no new test. Replace the "two Cards with the same
  identity" sentence with the operative facts: containment relaxes *every*
  optional `CardRef` field, `uid` and `space` included, so a caller that omits
  `space` matches any space; single-row resolution is held by
  `UNIQUE (data_tenant_id, name)` together with
  `cards::auth_projection::upsert_service_account_from_card` — the only production
  writer of a Card-bound `card_ref` — keeping the `name` column equal to
  `card_ref->>'name'`; `ORDER BY created_at, id LIMIT 1` is the deterministic
  guard for the anomaly that chain would have to break. Reuse the existing doc
  comment; the existing `tenant_admin_principal_id` cross-reference may stay once
  the real bound is named.
- **Closure proof:** the same command as `FIND-008-15` (the file must still
  compile and its tests pass), plus `mise run lints` and `mise run fmt`.

Both findings live in one file. One edit closes both.

## 4. Why each rejected finding is omitted

- **`TR3-2` / `RR3-2` / `DD3-2` (space fail-open).** The widening is real in the
  predicate and unreachable in the product. `IssueKeyArgs.space` is required, so
  no CLI caller can omit it; `CardRef::FromStr` never yields `space: None`. On the
  raw HTTP path, at most one Card-bound row per `name` can exist per tenant
  (§1.5), so nothing is silently discarded and no principal becomes reachable that
  was not already; a wrong space still refuses. Authorization is the tenant-wide
  `service_accounts_write` permission with an already space-insensitive resource
  label, so there is no space scope to broaden. Per the mandate, a path reachable
  only through a test-only writer or in principle is not a finding. The fact that
  the bound is incidental rather than designed is the part worth keeping, and it
  is kept — as `FIND-008-16`.
- **`DD3-4` (no test pins the semantics).** Falsified: the CLI journey pins it end
  to end and passes, and it cannot pass under the old predicate. AGENTS.md §11
  ranks that journey above a tier-2 SQL pin and calls a lower tier no substitute
  for a higher one, not the reverse. Requesting an additional Postgres-backed
  `wyrd-sql` test is coverage the task never asked for. `FIND-008-15` already
  forces the one test that pins the predicate string to be corrected.
- **`DD3-5` (out-of-scope relaxation / spec revision).** Same defect as `DD3-2`,
  so not a separate `DRIFT`, and its premise is false: `auth_projection.rs:57-63`
  reached from `cards/service.rs:1073` is a production writer of a uid-bearing
  Card-bound `card_ref`. Its recommended option 1 would revert a real production
  fix to accommodate a fixture. `REQ-004` and the non-goal both govern the write
  side, which is untouched.
- **`DD3-3` items 1 and 3** (within the merged `FIND-008-16`) are dropped as
  stated: item 1's premise is true of production, and item 3's precedent survives
  once the real bound is documented.

## 5. Prior-finding closure — `FIND-008-8` .. `FIND-008-14`

I **agree** with all four Wave 1 reviewers: every prior finding is closed. Spot
verification:

| ID | Check | Result |
|---|---|---|
| `FIND-008-8` | grep for `AuthFailed`/`RevokeFailed`/`AdminFailed`/`IssueKeyFailed` in `wyrd-cli/src/error.rs` | no matches — variants deleted |
| `FIND-008-9` | case-insensitive `authorization` in `docs/src/content/docs/for-agents/workflow.svx` | no matches |
| `FIND-008-10` | `authorization` prose in `wyrd-server/src/http/error.rs` and `wyrd-auth/src/error.rs` | no matches; only `wyrd-request-id` / axum constants remain. The prescribed selector `test(/http::error::tests::/)` selects **nothing** (`cargo nextest list -p wyrd-server --lib` returns an empty set) — the implementor's deviation claim is accurate and the substituted grep + journeys are the smallest credible proof available. Immaterial deviation, closed |
| `FIND-008-11` | `wyrd-cli/src/auth/login.rs:114-123` | the `InvalidArgument` arm now carries only `<missing code>` / `<missing state>` / `<missing code and state>`; no operator input reaches the error. The weaker `code=super…` assertion is adequate: the variant's own `expected` legitimately contains the literal `code=<>&state=<>`, so asserting absence of `code=` is impossible without degrading the guidance, while asserting absence of the actual code value proves exactly the invariant |
| `FIND-008-12` | `wyrd-cli/src/auth/login.rs:90-93`; `wyrd-server/src/components/eval/routes.rs:248-262` | both carry intent-bearing rustdoc with `# Errors` |
| `FIND-008-13` | `wyrd-cli/tests/auth_issue_key_journey.rs` run against real Postgres | `PASS [3.863s]` — the CLI issues a credential and spends it on a later call |
| `FIND-008-14` | grep `dev bootstrap` in `mise.toml` | no matches |

## 6. Not in dispute — confirmed

- **clap `disable_version_flag` + `Cli::command().debug_assert()`** — confirmed:
  `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:17` carries the attribute with a
  rustdoc naming why, and
  `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=cli::tests::the_shipped_command_tree_is_consistent)'`
  → `PASS [0.007s]`. This is the right check at the right altitude: no
  per-subcommand test can see a propagated flag.
- **`principal_journey` harness generalization** — confirmed:
  `run_cli_with_credential` / `run_cli_async_with_credential` at
  `tests/principal_journey.rs:27,48`, and no dead `run_cli` remains in that file.
- **`--kind` help text** — confirmed: `issue_key.rs:19` now reads *"spelled as the
  contract spells it (e.g. `Service`, `Agent`)"*, matching what `CardRef` parsing
  accepts.
- **Substituted proofs for `FIND-008-10` and `FIND-008-11`** — both adequate; see
  §5.
- **`auth_e2e::cache_ttl_path_also_flips_verdict`** — confirmed pre-existing:
  `crates/wyrd/wyrd-server/tests/auth_e2e.rs` does not appear in
  `git diff --name-only 968c92641..f102e50ee`, and three Wave 1 reviewers
  independently reproduced the identical `WYRD_AUTH_503_VERIFY_UNAVAILABLE` at or
  before the branch base. Not a candidate regression; not a finding.

## 7. Verification limits and out-of-scope observations

**Limits.**

- No broad aggregate was run or relied on: no `mise run gate`, no
  `mise run test:rust`, no whole-family or storage-matrix lane, no
  `--all-features` workspace lane. Every selector used was confirmed with
  `cargo nextest list` first; no positional filter was used.
- Commands actually run: `db:migrate:all:inner` (both migration lanes PASS) plus a
  read-only `psql` probe inside `scripts/postgres/with-test-postgres.sh`;
  `-p wyrd-sql --lib -E 'test(=…card_ref_uses_jsonb_card_ref_binding)'` (FAIL, as
  reported);
  `WYRD_CLI_E2E=1 … -p wyrd-cli --test cli -E 'test(=auth_issue_key_journey::auth_issue_key_cli_journey)'`
  (PASS);
  `-p wyrd-cli --lib -E 'test(=cli::tests::the_shipped_command_tree_is_consistent)'`
  (PASS); `cargo nextest list -p wyrd-server --lib -E 'test(/http::error::tests::/)'`
  (empty).
- I did not re-run `platform_admin_e2e`, the MCP lane, `codegen:check`,
  `py:test:unit`, or the TypeScript lane. Wave 1 covered those and reported no
  finding I am contradicting; my two findings do not touch those surfaces.
- I did not mutate the predicate back to `=` to observe the journey fail, since
  this review changes no source. The equivalent proof is the measured
  `old_equality_matches = f` combined with the fixture's uid-bearing stored ref.

**Nothing blocks this review.** The contradiction resolved cleanly from the live
schema, the migrations, and the production writer trace.

**Out of scope — handoff, not findings.**

- `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` is unconditional
  and `auth_projection.rs:71` binds the `name` column to `card.metadata.name`, so
  registering two same-named Service/Agent Cards in different spaces within one
  tenant makes the second projection fail on that unique. Pre-existing, predates
  this branch, unrelated to TASK-008 — for the spec owner.
- `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` still comments that the
  principal keeps a *"uid-less `card_ref` for the exact JSONB lookup"*, which the
  shipped predicate has made obsolete. Test-only prose on an item this candidate
  did not otherwise change; noted, not reported.

## 8. Result

**Ledger non-empty — 2 findings (`FIND-008-15` REGRESSION, `FIND-008-16` DRIFT),
both in `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, both closed
by one edit. `SPEC_REVISION_REQUIRED` does not apply.**
