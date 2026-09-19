# Domain review — persistent data / query semantics (`task-008-r3`)

Reviewer: `domain-rev-data` (fresh, independent). Reviewed candidate
`f102e50eea437ff4ba29571412197a1c4923cbbc` on `claude/admin-principals-spec-qfsmjc`.
No source file was changed. Working tree clean at exit.

**Result: FAIL**

## 1. Reviewed boundary and how it was traced

The durable Postgres layer for tenant-scope machine principals and the matching
semantics of the one query the remediation changed:

- `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (the query, its
  rustdoc, and its unit tests).
- Every DDL statement that shapes `wyrd.auth_service_accounts.card_ref`:
  `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:68-101` and
  `crates/wyrd/wyrd-sql/migrations/20260601000020_admin_principals.sql:104-152`.
- Every writer of that column (`insert_service_account` call sites, production
  and fixture).
- All three readers (`issue_api_key`, `exchange_api_key`, `jwt_bearer`) and what
  each binds to `$3`.
- The serde shape of `wyrd_spec::reference::CardRef` and `wyrd_semver::VersionBlock`.
- The `TenantConn` / RLS boundary.
- `mise.toml` `test:sql` / `test:sql:inner` selection.

Tracing was by `rtk proxy git diff 4668d8d33..f102e50ee`, direct reads, focused
`cargo nextest list` / `run`, and one empirical `psql` session against the
repository-managed Postgres (`scripts/postgres/with-test-postgres.sh`).

## 2. Established facts

### 2.1 Stored shape

`card_ref` is `JSONB`, `NOT NULL` at `20260601000001_auth.sql:74`, made nullable
by `20260601000020_admin_principals.sql:104-108`. It is a *denormalized
projection*; the migration comment states the durable key is
`(card_kind, card_uid)`, backed by
`UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)`
(`20260601000001_auth.sql:84`). `auth_service_accounts_card_binding_check`
(`20260601000020:139-146`) forces `card_kind`, `card_uid`, `card_ref`, `space`,
`version` to be all-present or all-absent.

The document is flat. `CardRef` (`crates/wyrd-spec/src/reference.rs:15-38`)
serializes `kind`, `name`, `version` always, and `space` and `uid` under
`#[serde(default, skip_serializing_if = "Option::is_none")]`.
`VersionBlock` is `pub struct VersionBlock(String)`
(`crates/shared/wyrd-semver/src/block.rs:15`), so `version` is a JSON string, not
a nested object. Therefore an absent optional field is **omitted**, never written
as `null` — the `{"a":null}` vs `{}` containment hazard does not arise here
(verified: `'{"a":1}'::jsonb @> '{"a":1,"uid":null}'::jsonb` is `f`).

**`uid` is *not* always populated, and the writers disagree with each other:**

| Writer | `card_ref` stored | `uid` present? |
|---|---|---|
| `wyrd-server/src/components/principals/routes.rs:240` (`POST /v1/principals`, production) | `None` | n/a — Card-free |
| `wyrd-server/src/components/platform/provisioning.rs:217` (tenant admin, production) | `None` | n/a — Card-free |
| `wyrd-testing/src/server.rs:2679-2688` (`seed_card_principal_in_tenant`) | caller's ref, **uid stripped**, with an explicit comment: *"The registry row needs its own uid, while the principal keeps the binding's uid-less `card_ref` for the exact JSONB lookup"* (`server.rs:2670-2671`) | **no** |
| `wyrd-testing/src/server.rs:4356`, `:4514`, `bootstrap_machine_in_tenant:2741` | the same ref given to `seed_machine_card`, built by `card_ref()` at `server.rs:4619-4629`, which sets `uid: Some(now_v7)` | **yes** |

There is **no production writer of a Card-bound `auth_service_accounts` row at
all**. `wyrd apply` Card-bound provisioning is specified
(`spec.md:59-63`) but unimplemented. So the production stored shape for this
lookup is presently *unwritten*, and the only observed shapes are two fixture
conventions that contradict each other.

### 2.2 Queried shape ( `$3` )

All three callers bind `Json(card_ref)` of a `&CardRef`, so optional fields are
omitted rather than nulled:

- `issue_api_key.rs:94` — ref from `IssueKeyRequest.card_ref`
  (`crates/wyrd-spec/src/auth/issue_key.rs:13-22`), straight off the wire. `uid`
  and `space` are both optional on that wire. The CLI
  (`wyrd-cli/src/auth/issue_key.rs:29`) requires `--space` and never sends a
  `uid`, but the HTTP/MCP route (`wyrd-server/src/components/auth/routes.rs:260-268`)
  performs **no** space or uid validation before the lookup.
- `exchange_api_key.rs:591` — `RequestedSubject::CardRef` delegation, wire-supplied,
  same optionality. (The ordinary API-key exchange resolves by `api_key_by_prefix`,
  not by card ref, so it was never affected.)
- `jwt_bearer.rs:155` — ref read out of `wyrd.auth_workload_bindings.card_ref`,
  itself written from a wire payload (`components/admin/routes.rs:402`), so
  typically uid-free.

### 2.3 Index and plan — the implementor's index claim holds

`auth_service_accounts_card_ref_gin` exists (`20260601000001_auth.sql:92-93`) and
uses the **default `jsonb_ops`** operator class. Verified against the
repository-managed Postgres that `jsonb_ops` supports
`@>(jsonb,jsonb)`, `?`, `?|`, `?&`, `@?`, `@@` — and *not* `=`. Measured plans on
a 20 002-row table with `ANALYZE` and `enable_seqscan=off`:

```
-- new predicate
Limit -> Sort (Sort Key: created_at, id)
  -> Bitmap Heap Scan on t  Recheck Cond: (card_ref @> '…')
       -> Bitmap Index Scan on t_gin  Index Cond: (card_ref @> '…')

-- old predicate
Seq Scan on t  Filter: (card_ref = '…')
```

So the change **gains** an index it did not previously have; the added
`ORDER BY created_at, id LIMIT 1` costs a `Sort` above the bitmap scan but does
not lose the index. No finding here — this part of the rationale is correct and
is in fact stronger than claimed.

### 2.4 Was the defect real?

Yes, but narrower than claimed. Verified against real Postgres that a uid-free
spaced ref is not equal to a uid-bearing stored document
(`count(*) = 0` under `=`). Since `bootstrap_machine_in_tenant` stores a
uid-bearing `card_ref` and the CLI can only name `space/Kind/name@version`,
`wyrd auth issue-key` against a `bootstrap_service` principal could resolve no
row under `card_ref = $3`. The command was genuinely unusable on that path.

Blast radius before the fix was **smaller** than the remediation record states:

- `exchange_api_key` is affected only on the `RequestedSubject::CardRef`
  delegation path, not on ordinary key exchange.
- The existing `identity_e2e` tests that drive these paths
  (`identity_e2e.rs:397`, `:958`, `:1208`) all use `seed_card_principal*`, which
  stores a **uid-less** ref — so `=` matched and those paths were not broken.

The reachable defect therefore traces to the divergence between the two fixture
writers (`server.rs:2670` strips uid *deliberately, for this exact reason*;
`server.rs:4619-4629` does not), on a lookup that has no production writer yet.

### 2.5 Determinism and ambiguity

Two rows satisfying `@>` is reachable from ordinary, well-formed input — not only
from an anomaly. Measured:

```
-- two rows: same kind/name/version, different space, different uid
SELECT card_ref->>'space' FROM t
 WHERE card_ref @> '{"kind":"Service","name":"dup","version":"1.0.0"}';
 alpha
 beta        (2 rows)
```

`UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)` does not prevent
this: the two rows differ in `card_uid`. `UNIQUE (data_tenant_id, name)` is on
the *principal* name, which is not required to equal the Card name
(`create_service_principal` takes an arbitrary `request.name`;
`bootstrap_machine` happens to reuse the card name), so it does not prevent it
either. `LIMIT 1` is thus **not** dead code — it is silently masking a live
ambiguous resolution in a credential-issuing path.

The cited precedent does not carry. `tenant_admin_principal_id`
(`service_accounts.rs:211-228`) resolves a row whose multiplicity is
*one-by-construction* — the provisioner reuses the existing row
(`provisioning.rs:210-219`) — so its `LIMIT 1` guards a genuine anomaly. Here the
second row arrives from a valid request that merely omitted an optional field.
Same syntax, different rationale.

### 2.6 Tenant isolation — intact

The predicate keeps `data_tenant_id = $1`; the table has
`ENABLE`/`FORCE ROW LEVEL SECURITY` with `USING (data_tenant_id = wyrd.current_tenant())`
(`20260601000001_auth.sql:96-101`), and `TenantConn` binds
`app.current_tenant` via `set_config(..., true)`
(`crates/wyrd/wyrd-sql/src/tenant_conn.rs:16-23`). Containment cannot reach
across tenants. No finding.

### 2.7 Coverage

- The **only** test that pinned this query's matching semantics is
  `service_accounts.rs:420-434`, and the candidate leaves it **red** (see DD3-1).
- No SQL-tier test exercises `service_account_by_card_ref` against real Postgres
  at all. Confirmed with `mise exec -- cargo nextest list -p wyrd-sql`: the only
  matching selectors are the lib unit test above and unrelated
  `pg_admin_principals` / `pg_migration` cases.
- The change's only end-to-end proof is one new CLI journey,
  `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs`, which happens to
  traverse the single fixture convention that produces a uid-bearing stored ref.
- `mise.toml:1463-1474` (`test:sql:inner`) runs
  `cargo nextest run --locked -p wyrd-sql`, which includes the lib target — so
  `mise run test:sql` would have caught DD3-1. It does not appear in the
  remediation record's "Commands run" list
  (`TASK-008-R2-…md:493-512`): **no wyrd-sql lane was run for a wyrd-sql edit.**

### 2.8 Migration / compatibility

No schema change, no backfill, no data migration required — the edit is a
read-side predicate. It does change what a previously-stored row resolves to in
both directions: a uid-bearing row that previously resolved to nothing now
resolves, and a query ref that omits `space` (previously matching nothing) now
matches every space in the tenant. No stored row becomes unreadable.

## 3. Authority and source coverage; verification limits

Read: `changes/active/admin-principals/spec.md` (revision 7 — Current baseline
`:55-65`, non-goals `:149-152`, REQ-004 `:179-184`, REQ-036/038/047, AC-013/014),
the original `TASK-008` packet, the remediation task
`TASK-008-R2-close-header-prose-and-credential-gaps.md` (constraints `:322-360`,
implementor evidence `:450-512`), `AGENTS.md` §3/§9/§11/§15/§16, both owning
migrations, all writers and readers, `mise.toml`.

Verified by execution:

- `mise exec -- cargo nextest list -p wyrd-sql --lib -E 'test(/service_accounts::tests/)'` — 4 selectors confirmed.
- `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`
  → **FAILED**, 1 run / 1 failed / 73 skipped (a non-empty selection).
- `scripts/postgres/with-test-postgres.sh` + `psql "$WYRD_TEST_DATABASE_ADMIN_URL"`:
  opclass enumeration, both `EXPLAIN`s, the two-row containment case, the
  equality-misses case, and the null-vs-absent case.

**Limits.** I did not run any whole-crate or aggregate lane (barred by the review
rules) and did not run `mise run test:sql` end to end; DD3-1 is proven by the
focused selector instead. I did not independently re-verify the pre-existing
`auth_e2e::cache_ttl_path_also_flips_verdict` red (outside this boundary). I
could not determine the production stored shape of a Card-bound `card_ref`,
because no production writer exists — that is itself a finding input, not a gap I
can close by reading.

## 4. Findings

### DD3-1 — REGRESSION — the candidate ships a red unit test in the crate it edited

- **Obligation:** `AGENTS.md` §12 Completion Standard ("the targeted
  tests/checks for the touched surface pass"); §11 Verification Scope (a Rust
  crate change runs the nearest crate-specific lane).
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`
  against `:15`.
- **Evidence:** the test asserts
  `SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")` while the constant
  now reads `card_ref @> $3`.

  ```
  thread '…::service_account_by_card_ref_uses_jsonb_card_ref_binding' panicked at
    crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431:9:
  assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")
  Summary [0.054s] 1 test run: 0 passed, 1 failed, 73 skipped
  ```

  The remediation record's "Commands run" (`:493-512`) contains no wyrd-sql lane.
  `mise.toml:1470` proves `test:sql` would have caught it.
- **Consequence:** `mise run test:sql` — and any `wyrd-sql` lane in CI — fails on
  this branch. The one test that documented this query's matching semantics is
  now a broken assertion rather than a pin.
- **Correction:** update the assertion to the semantics actually shipped and run
  the focused selector plus `mise run test:sql`. Testable: the command above
  exits 0.

### DD3-2 — INCORRECT — containment also drops `space`, so a space-less ref resolves a principal in any space, silently

- **Obligation:** `AGENTS.md` §9 ("Preserve tenant isolation across every public
  and internal server path"; write handlers own durable/policy decisions) and
  §15 (stop at the *first correct* option; the smallest incomplete solution is
  still wrong). Spec `REQ-004`: existing Card-bound principals keep their
  `card_ref` behavior unchanged.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15`
  and `:17-18`; reachable via
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:260-268` with
  `crates/wyrd-spec/src/auth/issue_key.rs:15` and
  `crates/wyrd-spec/src/reference.rs:28-31`.
- **Evidence:** `space` is `skip_serializing_if = Option::is_none`, so a wire
  caller may omit it, and the route validates neither `space` nor `uid` before
  the lookup. `@>` then matches on whatever subset the caller did name.
  Measured on the repository Postgres: a ref of
  `{"kind":"Service","name":"dup","version":"1.0.0"}` returned rows in **both**
  space `alpha` and space `beta`; the same ref with `space` under `=` returned
  `0`. `ORDER BY created_at, id LIMIT 1` then discards the second match without
  signalling. Containment relaxes *every* field at once, including any optional
  field `CardRef` gains later, not just `uid`.
- **Consequence:** an admin-scoped `POST /auth/issue-key` (or an
  `exchange_api_key` delegation) that omits `space` mints or delegates a
  credential bound to an arbitrary — oldest — same-named principal in a
  *different* space than the caller named, with no error. Under `=` that request
  matched nothing. This is a cross-space authorization broadening on a
  credential-issuing path, within the tenant (tenant isolation itself is intact,
  DD3-2 does not cross tenants).
- **Correction (bounded, no new decision needed):** keep the uid-insensitive
  intent but name the fields explicitly rather than relaxing all of them — e.g.
  retain `card_ref @> $3` for the index and add
  `AND card_ref->>'space' = $4` bound from `card_ref.space`, so an absent space
  refuses rather than widens; or require `space` at the wire boundary. Then drop
  `LIMIT 1`'s silent selection or keep it only once ambiguity is genuinely
  unreachable. Testable: a `wyrd-sql` Postgres-backed test seeding two principals
  with identical `kind/name/version` in spaces `alpha` and `beta` asserts that a
  space-less ref resolves `None` and each spaced ref resolves its own row.

### DD3-3 — INCORRECT — the new rustdoc asserts three things the tree contradicts

- **Obligation:** `AGENTS.md` §16 ("Rustdoc MUST explain intent … and relevant
  invariants"; documentation is part of implementation correctness).
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:157-165`.
- **Evidence:**
  1. *"the stored `card_ref` carries the registered Card's `uid`"* — true only of
     `wyrd-testing/src/server.rs:4619-4629`. `server.rs:2670-2678` deliberately
     stores a **uid-less** ref with the comment *"the principal keeps the
     binding's uid-less `card_ref` for the exact JSONB lookup"*, and no
     production writer stores a Card-bound `card_ref` at all
     (`principals/routes.rs:240` and `provisioning.rs:217` both pass `None`).
  2. *"Two active principals sharing one Card identity would require two Cards
     with the same identity"* — false as a guard on this predicate. Two rows with
     **different** `card_uid` and identical `kind/name/version` satisfy `@>` for
     a space-less ref (measured, §2.5); the table's uniqueness keys do not
     prevent it.
  3. *"the oldest wins for the same reason [`tenant_admin_principal_id`] picks
     the oldest: an anomaly must still resolve to one row"* — the precedent's
     multiplicity is one-by-construction (`provisioning.rs:210-219` reuses the
     existing row). Here the second row arrives from a valid request, so this is
     not an anomaly guard.
- **Consequence:** a maintainer reading this doc concludes the relaxation is
  uid-only and that ambiguity is impossible. Both conclusions are wrong, which is
  exactly how DD3-2 gets re-introduced after a future edit.
- **Correction:** state the actual relaxation (every optional `CardRef` field,
  not only `uid`), the actual reachable ambiguity, and the actual uniqueness
  keys; drop the `tenant_admin_principal_id` analogy or qualify it. Testable by
  review of the corrected doc against §2.1/§2.5 above.

### DD3-4 — MISSING — no test at any tier pins the new matching semantics

- **Obligation:** `AGENTS.md` §11 (a Rust crate change runs the nearest
  crate-specific lane; tier 2 integration tests pin a seam contract where a full
  journey would be noisy) and §12 ("New core behavior has Rust tests when
  practical").
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:169-180`;
  absent coverage in `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs`.
- **Evidence:** `mise exec -- cargo nextest list -p wyrd-sql` yields no
  Postgres-backed selector for this query — before or after the change. The only
  semantic pin was the string assertion now red per DD3-1. The sole proof offered
  is `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs`, which exercises only
  the uid-bearing fixture convention and would pass identically under several
  wrong predicates (including one that ignores `space`).
- **Consequence:** the behavioral property the change exists to establish —
  *uid-insensitive, otherwise-exact* resolution — is unverified, and DD3-2's
  broadening passes every green lane on the branch.
- **Correction:** add one Postgres-backed test in
  `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs` asserting: a uid-less
  spaced ref resolves a uid-bearing stored row; a ref naming a different space
  resolves `None`; and (per DD3-2) a space-less ref does not silently pick one of
  two spaces. Testable via
  `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_admin_principals -E 'test(=pg_tests::<name>)'`.

### DD3-5 — DRIFT — a shared authentication predicate was relaxed to work around a fixture divergence, outside the approved scope

- **Obligation:** spec `REQ-004` (`spec.md:179-184`) — *"Existing Card-bound
  Service and Agent principals keep their `card_ref`, `card_ref_scope`, `wyrd
  apply` provisioning, and emit-scope behavior unchanged"*; non-goals
  (`spec.md:149-150`) — *"Changing `wyrd apply` Card-bound principal
  provisioning"*; the remediation task's constraints and non-goals
  (`TASK-008-R2-…md:322-360`), which prescribe no query change. `AGENTS.md` §15
  ladder: fix the root cause at the shared owner, and stop at the *first correct*
  option.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15,17-18`
  vs root cause at `crates/wyrd/wyrd-testing/src/server.rs:4619-4629`
  (uid stamped onto the principal's stored ref) compared with
  `crates/wyrd/wyrd-testing/src/server.rs:2670-2678` (uid deliberately stripped,
  with a comment naming the exact lookup at stake).
- **Evidence:** the only path that could not resolve was the one fed by the
  `bootstrap_machine` fixture. The repository already contains the correct
  convention one function away, applied for this exact reason, and the two
  fixtures disagree. Meanwhile there is no production writer of a Card-bound
  `card_ref`, so the "stored shape" the predicate was relaxed to accommodate is
  not yet a production fact — it will be fixed by whatever `wyrd apply`
  provisioning writes, which the spec places out of scope. `ORDER BY created_at,
  id LIMIT 1` also changes the observable resolution of Card-bound principals,
  which `REQ-004` says is unchanged.
- **Consequence:** the durable resolution semantics of every Card-bound
  principal — on three authentication and credential-issuance callers — were
  widened to make one test fixture line up, ahead of the provisioning decision
  that will determine the real stored shape. Two of the three callers were not
  broken.
- **Correction, two bounded options:**
  1. *Smaller, and I recommend it for this task:* make
     `bootstrap_machine_in_tenant` split the refs the way
     `seed_card_principal_in_tenant:2670-2678` already does — uid on the
     registry Card row, uid-less ref on the principal — and revert
     `service_accounts.rs:15,17-18` to `card_ref = $3`. The CLI journey then
     passes on the repository's existing convention, the shared predicate is
     untouched, DD3-2 does not arise, and DD3-1/DD3-3 dissolve. Testable: the
     new CLI journey plus `mise run test:sql` both pass with the predicate
     unchanged.
  2. *If uid-insensitive resolution is wanted as product behavior:* land DD3-2's
     narrower predicate plus DD3-4's coverage.

  **Explicitly:** option 1 is a bounded fix. Option 2 is *also* bounded as
  written, **but** the underlying question the implementor raised — whether
  Card-bound principal resolution is keyed on Card *identity*
  (`space/Kind/name@version`) or on the durable `(card_kind, card_uid)` the spec
  names at `spec.md:59-63` — **is an approved persistent-data decision, not an
  implementor call.** It determines what `wyrd apply` must store, and it cannot
  be settled from this tree because no production writer exists. Ruling on the
  implementor's question: *yes, a spec revision is required before the shared
  predicate's uid sensitivity is changed as product behavior.* It is not required
  to unblock TASK-008, because option 1 fixes the observed defect without
  touching the predicate.

## 5. Result

**FAIL** — DD3-1 alone (a red test in the edited crate, provable in 0.05 s) is
disqualifying; DD3-2 is a cross-space authorization broadening on a
credential-issuing path.
