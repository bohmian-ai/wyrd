# Domain review — shared principal-resolution query on the authentication path

Reviewer: `domain-rev-auth-sql` (fresh, independent). Candidate
`c34b9d1e01c2c29ec8056e4d5ed220d61f9a6793`, branch
`claude/admin-principals-spec-qfsmjc`, base `968c92641`, previously reviewed
candidate `f102e50ee`. Working tree clean at start and at finish apart from this
review directory. No source file changed by this review.

**Overall result: FAIL** — one `INCORRECT` finding (`DA4-1`). The mechanism did
not move (byte-identity confirmed independently), the corrected guard test does
pin the two properties `FIND-008-15` named, and every other factual claim in the
new rustdoc verifies. One newly invented clause in that rustdoc asserts a
property the tree does not provide and that round 2 had already measured to be
false, which is the exact failure mode `FIND-008-16` was raised to prevent.

## 1. Reviewed boundary and how it was traced

Boundary: `SERVICE_ACCOUNT_BY_CARD_REF_SQL` and
`service_account_by_card_ref` in
`/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces/crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`,
its three production callers, its write side, and the documentation the candidate
now publishes about it.

Traced, in order:

1. The r4 diff via `rtk proxy git diff f102e50ee..c34b9d1e0` (verified to be a
   real unified diff, not an `rtk` stat summary) and
   `rtk proxy git diff --name-status` over all paths, not only `crates/`.
2. The const and function rustdoc as they now stand
   (`service_accounts.rs:7-36`, `:169-189`) and the corrected guard test
   (`:428-450`).
3. The schema: `migrations/20260601000001_auth.sql:68-101` (table, uniques, GIN
   index, RLS `ENABLE` + `FORCE` + `wyrd.current_tenant()` policy) and every
   later migration that touches the table
   (`…000009`, `…000011`, `…000012`, `…000014`, `…000020`), checking whether any
   drops `UNIQUE (data_tenant_id, name)`. None does.
4. The live schema through the repository-managed environment
   (`scripts/postgres/with-test-postgres.sh` + `mise run db:migrate:all:inner`),
   enumerating `pg_constraint` on `wyrd.auth_service_accounts`, and measuring
   `jsonb @>` containment for a space-less, a wrong-space and a right-space right
   operand.
5. The write side: `queries/cards/auth_projection.rs:15-84`
   (`UPSERT_SQL` + `upsert_service_account_from_card`).
6. All three callers, and upward to the surface that supplies their `CardRef`:
   `wyrd-auth/src/issue_api_key.rs:94` ← `wyrd-server/src/components/auth/routes.rs:260-308`
   ← `wyrd-spec/src/auth/issue_key.rs:13-22`;
   `wyrd-auth/src/exchange_api_key.rs:582-596` ← `wyrd-spec/src/auth/token.rs:63-74`;
   `wyrd-auth/src/jwt_bearer.rs:148-167` ← the workload-binding resolver ←
   `wyrd-sql/src/queries/auth/workload_bindings.rs:75-89` (`card_ref: Value`).
   `wyrd-cli/src/auth/issue_key.rs:18-60` was read because the new rustdoc cites
   it.
7. Round 2's `findings-validation.md` §1.2-§1.6 read for context, with §1.2,
   §1.3 and §1.5's load-bearing facts re-measured here rather than inherited.

## 2. Claim-by-claim verification of the new rustdoc

The rustdoc under review is `service_accounts.rs:7-26` (on the const) plus
`:169-177` (the function doc that now points at it).

| # | Factual assertion (location) | Evidence | Verdict |
|---|---|---|---|
| C1 | Registering a Card-bound principal stores a `uid`-bearing `card_ref`; `queries::cards::auth_projection` writes `space: Some(..)` and `uid: Some(..)` (`:10-12`) | `auth_projection.rs:58-64` builds `CardRef { space: Some(space.clone()), uid: Some(card_uid.clone()), .. }`; `space` is `ok_or`-required at `:45-49` | **TRUE** |
| C2 | Therefore `card_ref = $3` matched no registered principal at all (`:13-14`) | Caller refs carry `uid: None` (`wyrd-cli/src/auth/issue_key.rs:56-60` via `CardRef::FromStr`); whole-document equality against a `uid`-bearing stored document fails. Round 2 measured `stored = {…,"space":"prod"} -> f` (§1.3) | **TRUE** |
| C3 | Containment relaxes *every* optional `CardRef` field, `space` included: a ref with no space matches a row in any space (`:16-17`) | Measured on the repository-managed Postgres: space-less right operand → `t`; wrong space → `f`; right space → `t`. `CardRef.space` and `.uid` are `#[serde(default, skip_serializing_if)] Option` (`wyrd-spec/src/reference.rs:24-37`), so an omitted field is absent from `$3` entirely | **TRUE** |
| C4 | A wrong space still refuses | Measured `f` (above). Note: the **const** rustdoc does not actually state this; only the test rustdoc's neighbourhood implies it. Accurate where stated, not a false claim — recorded as an observation, not a finding | **TRUE (but not asserted in the const doc)** |
| C5 | The table's `UNIQUE (data_tenant_id, name)` is one of the facts bounding the match to one row (`:18-19`) | `migrations/20260601000001_auth.sql:85`; live `pg_constraint` returns `auth_service_accounts_data_tenant_id_name_key\|UNIQUE (data_tenant_id, name)`; no later migration drops it (`…000020` drops only `%principal_kind%` check constraints, `…000011` only the `auth_users` FKs) | **TRUE** |
| C6 | `auth_projection` keeps the `name` column equal to `card_ref->>'name'` (`:19-20`) | `auth_projection.rs:58-76`: `card_ref.name` and the `name` bind both come from `card.metadata.name`; `UPSERT_SQL`'s `DO UPDATE SET card_ref = EXCLUDED.card_ref, … name = EXCLUDED.name` (`:21-25`) keeps them in lockstep on re-registration | **TRUE** |
| C7 | **"every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)"** (`:20-21`) | See `DA4-1`. `IssueKeyArgs` is a `wyrd-cli` clap struct and is not a caller of this query. The actual callers receive a `CardRef` whose `space` is `#[serde(default)] Option<SpaceName>`, and no handler requires it: `routes.rs:260-308` performs authorization on `format!("card:{}", request.card_ref.name)` and passes `request` straight through with no space check; `RequestedSubject::CardRef` (`wyrd-spec/src/auth/token.rs:69-73`) likewise; `jwt_bearer.rs:155` passes a ref decoded from the admin-supplied `auth_workload_bindings.card_ref` JSONB (`workload_bindings.rs:75-89`, typed `Value`), with no space requirement on that write either. Round 2 established the same fact: *"It is reachable from a raw HTTP body via `#[serde(default)]`"* (`findings-validation.md:116`) | **FALSE** |
| C8 | `ORDER BY created_at, id LIMIT 1` exists because none of that chain is enforced at the query; relaxing the unique constraint or decoupling the name projection makes the predicate match more rows (`:21-24`) | `SERVICE_ACCOUNT_BY_CARD_REF_SQL` (`:27-36`) contains no uniqueness or space predicate, so the bound is indeed external. Both named relaxations do widen it. (The clause's third item, "add an optional `CardRef` field", cannot by itself produce a second row while `UNIQUE (data_tenant_id, name)` and the name projection hold — but this wording is inherited verbatim from round 2's prescribed correction and is forward-looking, so it is not raised as a finding) | **TRUE** |
| C9 | "narrow the predicate rather than lean on that fallback" (`:25-26`) | Guidance, not a factual assertion | **N/A** |
| C10 | The durable key remains `(card_kind, card_uid)` (`:173`) | `UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid)`, `auth.sql:84`, live-confirmed; `auth_projection.rs`'s `ON CONFLICT` keys on it | **TRUE** |
| C11 | The GIN index on `card_ref` serves this lookup (`:174`) | `auth_service_accounts_card_ref_gin … USING GIN (card_ref)` (`auth.sql:92-93`); `jsonb @>` is the `jsonb_ops` containment operator this index supports. Notably this claim was *not* true of the old `=` predicate, and is now | **TRUE** |
| C12 | The false "two Cards with the same identity" justification is gone | Removed in `708f01ec9` (diff hunk at `:174-183` of the old file); `grep` for "same identity" over the file returns nothing | **TRUE** |

No other new statement in the rustdoc is unverifiable. Nothing in it is
speculative except `C7`.

## 3. Independent result for the byte-identity claim

Confirmed. The const was extracted from both revisions and compared directly,
not by re-running the implementor's command:

```
rtk proxy git show f102e50ee:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs > old_sa.rs
awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;$/' old_sa.rs                      > old_const.txt
awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;$/' <working copy>                 > new_const.txt
```

- `md5(old_const.txt) = 2298c9e6b9cf3e40edb952818226b944`
- `md5(new_const.txt) = 2298c9e6b9cf3e40edb952818226b944`
- `diff old_const.txt new_const.txt` → identical

The claimed hash is the hash these files actually have. The shipped text is:

```
WHERE data_tenant_id = $1 AND principal_kind = $2 AND card_ref @> $3
  AND status = 'active' ORDER BY created_at, id LIMIT 1
```

**The mechanism did not move.** `rtk proxy git diff --name-status
f102e50ee..c34b9d1e0` over *all* paths returns exactly one file under `crates/`:
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`. Everything else
added is under `changes/active/admin-principals/review/task-008-r3/`. Within
that file the only changes are the const rustdoc, the function rustdoc, the test
rustdoc, and three assertion lines. Unchanged: the tenant predicate
(`data_tenant_id = $1`), `status = 'active'`, the `ORDER BY`/`LIMIT`, the
`.bind` order in `service_account_by_card_ref` (`:183-188`), all three callers,
`queries/cards/auth_projection.rs`, the stored `card_ref` shape, the durable key,
and every migration.

## 4. Security properties round 1 and round 2 accepted — spot-verification

All verified against the current tree; each `HOLDS`.

1. **Platform extractor canonical-header binding and indistinguishable
   rejection** — `components/auth/platform_extractor.rs:56` reads only
   `WYRD_ACCESS_TOKEN_HEADER` (`components/auth/token_extract.rs:16-17`,
   `x-wyrd-access-token`); no `Authorization` fallback, pinned at
   `platform_extractor.rs:236-247`. One `unauthenticated()` constructor
   (`:69-74`) serves missing/malformed (`:120`), failed verify (`:135`) and
   `PlatformSessionError::Invalid` (`:139`), and `Invalid` itself collapses
   unknown credential, wrong secret, revoked/expired credential and suspended
   principal (`wyrd-auth/src/platform_sessions.rs:62-68`, produced at `:213`,
   `:218`, `:245`, `:247`, `:271`, `:274`). Store outage remains a distinct 500
   (`platform_extractor.rs:141-145`), which is the intended shape.
2. **`PLATFORM_TOKEN_SCOPE` plane separation, both directions** — tenant token on
   a platform path fails `deny_unknown_fields` decode of
   `PlatformAccessTokenClaims` (`wyrd-auth-verify/src/lib.rs:786-788`) and the
   scope marker is re-checked at `:447-449` and again at
   `platform_sessions.rs:208-210`; platform token on a tenant path fails
   `AccessTokenClaims`' required `principal`/`roles` and `deny_unknown_fields`
   (`lib.rs:757-766`, `:1016-1024`). Both directions are pinned end-to-end at
   `wyrd-server/tests/platform_admin_e2e.rs:186-237` and `:239-255`.
3. **Single `TokenVerifier` signing-key resolver** —
   `TokenVerifier::signing_key` (`lib.rs:413-425`) is the only reader of
   `decoding_keys` (`:320`), called from exactly the platform (`:444`) and tenant
   (`:512`) paths; the production map is built once at
   `wyrd-server/src/boot/auth.rs:69-74`. Other hits are test fixtures.
4. **No revocation epoch, cache or memoization on the platform plane** —
   `verify_platform` (`lib.rs:443-450`) touches neither `self.cache` nor
   `self.revocation`; both appear only on the tenant path (`:466-506`, `:509-`).
   The platform request path re-reads credential, principal and grant per request
   (`platform_sessions.rs:229-256`, `:265-280`; `platform_extractor.rs:78-105`),
   and `platform_admin_e2e.rs:257-300` proves revocation ends a live session.
5. **Credential plaintext crosses a client surface exactly once** — the only
   `expose()` call sites under `wyrd-cli/src` are
   `wyrd-cli/src/auth/issue_key.rs:83` and `wyrd-cli/src/auth/login.rs:71,73`.
   No `tracing::`/`{:?}` of args anywhere under `wyrd-cli/src/auth/`.
   `SecretBearer` redacts on `Debug` (`wyrd-spec/src/auth/secret_bearer.rs:36-40`),
   `WyrdApiKey.secret` is `SecretString` (`issue_api_key.rs:181`), server-side
   issuance is `#[tracing::instrument(skip(self, conn, request), …)]`
   (`issue_api_key.rs:78-83`), and no `IssueKeyError` variant (`:53-71`) carries
   key material. The login refusal never echoes the paste
   (`wyrd-cli/src/auth/login.rs:115-123`, pinned at `:160-176`).
   `wyrd-cli/tests/auth_issue_key_journey.rs:32-116` recovers the key only from
   the single printed line and spends it against a real server.
6. **Tenant isolation on the changed query's callers** — the query binds
   `conn.data_tenant_id()` (`service_accounts.rs:184`) into a `data_tenant_id = $1`
   predicate, on a `TenantConn` transaction that sets `wyrd.current_tenant()` for
   its lifetime (`wyrd-sql/src/tenant_conn.rs:45-54`), under `ENABLE` + `FORCE`
   RLS with a `wyrd.current_tenant()` policy (`auth.sql:97-101`). `issue_api_key`
   takes its tenant from the authenticated caller
   (`routes.rs:292-297`); `exchange_api_key` verifies the presented token against
   `conn.data_tenant_id()` (`exchange_api_key.rs:296-299`, rejected at
   `lib.rs:516-520`); `jwt_bearer` derives the tenant from the request by
   construction (no principal exists yet — `wyrd-server/src/auth/jwt_bearer.rs:59-77`)
   but authenticates *inside* that tenant against that tenant's trusted issuer
   and binding (`wyrd-auth/src/jwt_bearer.rs:103-127`), so no cross-tenant read
   is reachable.

## 5. Does the corrected test pin the security-relevant property?

`service_account_by_card_ref_uses_jsonb_card_ref_binding`
(`service_accounts.rs:433-450`) now asserts, against the **shipped const** (not a
copy):

- `card_ref @> $3` — the containment operator `FIND-008-15`'s predecessor
  mis-asserted;
- `ORDER BY created_at, id` and `LIMIT 1` — the deterministic single-row pick;
- `principal_kind = $2`;
- absence of `card_ref::text`.

**What it protects.** Reverting to `card_ref = $3`, dropping the `ORDER BY`,
dropping the `LIMIT 1`, or reintroducing a text-cast comparison each fail the
test. That is precisely the widening `FIND-008-15` observed nothing objected to,
and it is checked against `SERVICE_ACCOUNT_BY_CARD_REF_SQL` itself — unlike the
sibling `api_key_lookup_filters_all_public_invalid_key_cases` (`:402-426`), which
asserts against an inline *copy* of its SQL and therefore proves nothing about
what ships. The new test rustdoc (`:428-432`) states why the text is pinned. The
`FIND-008-15` gap is closed.

**What it does not protect.** It is a substring test on SQL text and executes
nothing. It cannot observe containment *semantics* (the space relaxation is
measured nowhere in the repository's own tests — only by reviewers, out of band),
and a semantically different query that still contains those substrings would
pass: an added `OR TRUE`, a `LIMIT 1` that moved into a subquery, a removed
`status = 'active'`, or a removed `data_tenant_id = $1` — the tenant predicate,
the single most security-relevant clause in the statement, is not asserted.
Widening the test to cover those is outside the remediation task's scope and is
recorded here as a coverage limit, not as a finding.

## 6. Authority and source coverage; verification limits

Authorities read: `AGENTS.md` (§9 server/contract rules, §11 testing taxonomy and
verification scope, §12 completion standard, §16 rustdoc obligations),
`changes/active/admin-principals/spec.md` rev 7 `REQ-004`,
`changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`,
`changes/active/admin-principals/review/task-008-r3/` (task file, verdict,
`findings-validation.md` §1.2-§1.6).

Verification run (all narrow; no `mise run gate`, no `mise run test:rust`, no
`--all-features` workspace lane; every selector confirmed by
`cargo nextest list` first):

| Lane | Command | Result |
|---|---|---|
| Selector | `mise exec -- cargo nextest list -p wyrd-sql --lib` | prints `queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding` |
| Focused | `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` | 1 run, 1 passed, 73 skipped |
| `wyrd-sql` lib | `mise exec -- cargo nextest run --locked -p wyrd-sql --lib` | 74 passed |
| Owning lane | `mise run test:sql` | exit 0; 120 + 4 + 113 + 2 passed, 0 failed |
| Platform E2E | `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'` | exit 0; 17 passed |
| CLI journey | `WYRD_CLI_E2E=1 mise run test:cli:journey` | 23 passed, 0 failed, 5 ignored (self-declared out-of-lane); `auth_issue_key_journey::auth_issue_key_cli_journey ... ok` |
| Live schema | `scripts/postgres/with-test-postgres.sh` + `mise run db:migrate:all:inner` + `pg_constraint` enumeration | `UNIQUE (data_tenant_id, name)` present |
| Containment | same environment, `jsonb @>` with space-less / wrong-space / right-space right operands | `t` / `f` / `t` |

Limits — stated explicitly:

- The space relaxation and the one-row bound were measured **out of band** by
  this review, not by any test in the repository. The candidate adds no executed
  test for either; the guard test is text-only. Nothing in the tree would fail if
  a future change made the widening real *and* also updated the pinned SQL text.
- `wyrd-server --test auth_e2e` was not run; the known pre-existing
  `cache_ttl_path_also_flips_verdict` failure (`WYRD_AUTH_503_VERIFY_UNAVAILABLE`)
  is out of scope per the subject and is not attributed to this candidate. None
  of the lanes above select it.
- The rustdoc's forward-looking clause "add an optional `CardRef` field, and this
  predicate starts matching more rows" is unfalsifiable against the current tree
  by construction; it is not counted as a verified or a false claim.
- Security items 1-4 in §4 were verified by source reading, and items 2 and 4 in
  addition by the `platform_admin_e2e` assertions cited; they were not
  independently re-derived by new dynamic probes.

## 7. Proposed findings

### `DA4-1` — INCORRECT

The replacement rustdoc asserts a caller invariant the tree does not provide, and
attributes it to a symbol that is not a caller.

- **Violated obligation:** `AGENTS.md` §16 (*"Rustdoc MUST explain intent … and
  relevant invariants"* — an invariant stated must be one the code holds) and
  §12 (*documentation is part of implementation correctness*). It is also the
  non-closure of `FIND-008-16`, whose whole subject was a rustdoc justifying
  single-row resolution with an invariant the schema does not provide; round 2's
  prescribed correction did **not** contain this clause.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:20-21`
  (the clause *"and every caller passing a fully qualified ref
  (`IssueKeyArgs::space` is required)"*, inside the rustdoc at `:7-26`).
- **Evidence:**
  - `IssueKeyArgs` (`crates/wyrd/wyrd-cli/src/auth/issue_key.rs:18-42`) is a
    `wyrd-cli` clap struct. It is not a caller of this query — it is one CLI
    client's argument surface, in a crate `wyrd-sql` does not depend on. The
    three actual callers are `wyrd-auth/src/issue_api_key.rs:94`,
    `wyrd-auth/src/exchange_api_key.rs:591`, and
    `wyrd-auth/src/jwt_bearer.rs:155`.
  - `CardRef.space` is `#[serde(default, skip_serializing_if = "Option::is_none")]
    Option<SpaceName>` (`crates/wyrd-spec/src/reference.rs:24-29`), so a body that
    omits it deserializes to `space: None` and `$3` carries no `space` key at all.
  - **`issue_api_key`:** `IssueKeyRequest.card_ref` (`crates/wyrd-spec/src/auth/issue_key.rs:13-15`)
    reaches `IssueApiKey::execute` unvalidated — `crates/wyrd/wyrd-server/src/components/auth/routes.rs:260-308`
    authorizes on `format!("card:{}", request.card_ref.name)` (`:283`) and passes
    `request` through at `:300`, with no space-presence check anywhere on the
    route. So a raw `POST /auth/issue-key` with
    `card_ref: {kind, name, version}` reaches the query unqualified.
  - **`exchange_api_key`:** `RequestedSubject::CardRef { card_ref }`
    (`crates/wyrd-spec/src/auth/token.rs:69-73`) carries the same optional
    `space` and is destructured straight into the query at
    `exchange_api_key.rs:588-593`; no space check.
  - **`jwt_bearer`:** the ref is decoded from
    `wyrd.auth_workload_bindings.card_ref`, whose write shape is a bare
    `card_ref: Value` (`crates/wyrd/wyrd-sql/src/queries/auth/workload_bindings.rs:75-89`)
    with no space requirement, so a binding registered with a space-less
    `CardRef` yields a space-less ref at `jwt_bearer.rs:155`.
  - Round 2 had already established exactly this: *"It is reachable from a raw
    HTTP body via `#[serde(default)]`"*
    (`changes/active/admin-principals/review/task-008-r3/findings-validation.md:116`).
    The new doc asserts the opposite of a fact the review it is remediating
    measured.
  - Measured containment confirms the consequence is real at the predicate:
    space-less right operand → `t` (matches a row in any space).
- **Observable consequence:** the clause is one of three facts the doc presents
  as jointly holding the match to one row. Only two of the three are real
  (`UNIQUE (data_tenant_id, name)` and the `name` projection — either alone is
  sufficient given `$3` always pins `name`). A maintainer who trusts the doc
  believes an unqualified ref is unreachable from the wire, and may therefore
  relax `UNIQUE (data_tenant_id, name)` — the same-name-across-spaces projection
  limitation round 2 handed off is a live reason someone would want to — on the
  belief that caller qualification still bounds the read. It does not, and the
  ordered `LIMIT 1` then silently picks one of several principals on a
  credential-issuing path. This is the precise harm `FIND-008-16` was raised to
  prevent, restated with a different false invariant.
- **Correction (decision-complete, bounded):** amend the rustdoc only — no
  behavior change, no predicate change, no new test, no caller touched. Delete
  the clause *"and every caller passing a fully qualified ref
  (`IssueKeyArgs::space` is required)"* and the `IssueKeyArgs` citation. State
  instead what the tree provides: single-row resolution is held by
  `UNIQUE (data_tenant_id, name)` together with
  `cards::auth_projection::upsert_service_account_from_card` — the only
  production writer of a Card-bound `card_ref` — keeping the `name` column equal
  to `card_ref->>'name'`, so `$3`'s always-present `name` pins at most one row;
  and record that callers may supply an unqualified ref, because `CardRef.space`
  is `#[serde(default)]` and no route requires it, so a space-less ref resolves a
  same-named principal in a space the caller never named (a wrong space still
  refuses). Then the same closing guidance already present: narrow the predicate
  rather than lean on the ordered `LIMIT 1`.
- **Testable:** `grep -n 'IssueKeyArgs' crates/wyrd/wyrd-sql/` returns nothing,
  and every remaining invariant sentence in the rustdoc names a constraint,
  column projection, or serde attribute that exists at a cited `file:line`.
- **No approved decision required.** This is a documentation-accuracy fix inside
  the file the remediation task already owns. It changes no contract, no
  persistent data shape, and no security control, and does not touch `REQ-004`.

No other material finding. Specifically **not** reported, deliberately: the guard
test's missing `data_tenant_id = $1` assertion and its inability to observe
containment semantics (coverage limits outside the task's scope, §5); the two
out-of-scope handoffs from round 2 (the same-name-across-spaces projection
failure and the stale `wyrd-testing/src/server.rs:2670-2672` comment), which were
confirmed left alone as round 2's non-goals require and are not gaps; and the
pre-existing `auth_e2e::cache_ttl_path_also_flips_verdict` failure.

## 8. Overall result

**FAIL** — `DA4-1` (`INCORRECT`).
