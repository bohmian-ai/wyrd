# Task re-review verdict — TASK-003, TASK-004, TASK-005, TASK-006 (remediation R1)

**Verdict: `FIX_REQUIRED`**

## Immutable subject

| | |
|---|---|
| Repository root | `/home/user/wyrd` |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` — `git merge-base HEAD origin/change/surfaces-oracle-integration` (the branch was rebased onto the updated parent; `origin/main` is not the source of truth here) |
| Candidate | `4225069` — *feat(client): project principal and credential administration to the SDKs* |
| Range | `c5c2075..4225069`, 112 files, +13030 / −311 |
| Approved spec | `changes/active/admin-principals/spec.md`, revision 6 |
| Tasks under review | `TASK-003`, `TASK-004`, `TASK-005`, `TASK-006` |
| Prior verdict | `changes/active/admin-principals/review/task-003-006/verdict.md` (`FIX_REQUIRED`, 19 findings) |
| Remediation task | `changes/active/admin-principals/review/task-003-006/TASK-003-006-R1-lifecycle-audit-and-atomicity.md` (24 acceptance criteria) |
| Remediation commit claimed | `f156e64` — *fix(auth): close the blocking findings from the task-003..006 review* |

Subject stability: the range did not change during review.

## Review topology — deviation, recorded

`$wyrd-task-review` requires a two-wave multi-agent topology (`task-rev`,
`repo-rev`, per-domain `domain-rev`, then `ponytail-rev`). The harness running
this review exposes no agent-delegation tool, so the waves could not be spawned
and this is a single-reviewer audit. Under the skill's own rule that is
`BLOCKED`. It is recorded as a deviation rather than returned as the verdict
because the blocking findings below are mechanical — each is a sequence of
statements read directly from source, independently reproduced against a live
Postgres, and none depends on a judgment a second reviewer could plausibly
overturn. A `PASS` could not have been issued under this deviation; a
`FIX_REQUIRED` on falsified properties can.

## Verification performed and its limits

Scope is `VER-001`…`VER-006`. Broad aggregates were not run; their absence is
not a finding. `mise` is unavailable, so the lanes were reproduced with
`rustup run 1.97.1 cargo` and `scripts/postgres/with-test-postgres.sh`, which is
what the `mise` lanes wrap. No test was weakened, disabled or deleted; no source
file was modified.

| Command | Result |
|---|---|
| `cargo clippy --locked -p wyrd-server -p wyrd-sql -p wyrd-auth --all-targets` | clean |
| `with-test-postgres.sh -- env WYRD_AUTH_E2E=1 cargo test -p wyrd-server --test platform_admin_e2e -- --test-threads=1` | 10 passed, 0 failed |
| `with-test-postgres.sh -- cargo test -p wyrd-sql --test pg_admin_principals --test pg_migration -- --test-threads=1` | 9 + 17 passed, 0 failed |

Limits that bound what green means:

- The journeys return early unless `WYRD_AUTH_E2E=1`; they are real proof only in
  the gated lane.
- The new migration applies cleanly — the in-process server in every journey runs
  it — but no test asserts its effect directly.
- `mise run codegen:check` could not be run. It would pass vacuously: `openapi.yaml`
  still contains zero occurrences of `platform` or `principals`.
- The mutation check that would most directly falsify the FIND-006-1 fix
  (removing the epoch write and observing the journey fail) was not run, because
  the review is forbidden from modifying source. The claim is instead established
  from code: token verification consults only `wyrd.auth_service_accounts.tokens_not_before`,
  never `auth_api_keys.revoked_at`, so nothing but the epoch write can produce the
  refusal the journey observes.

## Remediation closure — the five claims verified independently

### FIND-003-1 — initialization atomicity — **CLOSED (behavior), proof still open**

`crates/wyrd/wyrd-server/src/boot/init.rs:103-140` is genuinely one transaction.
`pool.begin()` at :103; `insert_platform_principal_tx` at :105,
`set_platform_grant_tx` at :128 and `insert_platform_credential_tx` at :129 all
take `&mut tx`; the single `tx.commit()` is at :139. The `?` on :128 and :137
propagates without an explicit rollback, which is correct — dropping a sqlx
`Transaction` rolls it back — so a failure at the grant or the credential leaves
no `platform.principals` row and the next invocation starts clean. The
`AlreadyInitialized` path still works: `SqlError::UniqueViolation` is matched at
:114, the aborted transaction is rolled back at :117, and
`initialization_happens_at_most_once` passes.

What is *not* closed: the correction's stated proof obligation — "inject a failure
at each of the three stages and then show a subsequent invocation succeeding" —
has no test at any tier. Nor does the concurrency case. See FIND-003-2.

### FIND-006-1 — revocation stops already-minted tokens — **CLOSED**

`components/principals/routes.rs:367-373`: `revoke_api_key` and
`revoke_service_account_principal` both execute on `conn` and commit together at
:373 — one transaction, verified.
`wyrd-sql/src/queries/auth/revocation.rs:93-109` confirms the second is a bare
`UPDATE … SET tokens_not_before = now()`; it does **not** touch `status`, so the
principal is not disabled, which was the obvious way this fix could have broken
rotation. It did not.

The journey is not passing on stale cache state. `wyrd-testing/src/server.rs:3736-3739`
selects the epoch-cache TTL from `verify_settings.cache_ttl`; where that is
non-zero the in-process server uses the production 5-second TTL and has no NOTIFY
listener, so the journey's decision at `platform_admin_e2e.rs:533-543` to mint
`live_token` and leave it unused until after the revocation is load-bearing and
correct — its first request is a genuine cache miss. Independently: verification
consults `tokens_not_before` only, never `auth_api_keys.revoked_at`, so the
observed refusal at :564-571 cannot come from anything but the epoch write.

One weakness, recorded not raised: :568 asserts `assert_ne!(status, OK)` rather
than the specific refusal. It still discriminates — the same principal's `reader`
grant makes `200` reachable — so it is proof, just loose.

### FIND-005-1 — audit on the tenant principal surface — **CLOSED (behavior), proof open**

`components/principals/routes.rs:85-137` routes all four decisions through the
repository's existing `audit::append_on` seam on the caller's own `TenantConn`,
so the row and the work it permits commit together (`audit/mod.rs:127-130`).
Allow (:95-102) and deny (:106) are both recorded; the deny path commits its row
at :107 before returning the refusal, which is right — a refused attempt is the
row an operator most needs. Fail-closed holds: `append_on` maps any append
failure to `WyrdError::AuditUnavailable`, and `record_decision` propagates it
with `?`, so an unrecordable decision refuses and commits nothing.

The prior finding's secondary obligation — retire the recorded no-audit stance in
`components/admin/routes.rs` — is satisfied, but by the rebase, not by this
commit: `git log c5c2075..HEAD -- components/admin/routes.rs` is empty and the
file's header now reads "Every handler audits its `service_accounts:write`
verdict exactly once". Recorded so the closure is not credited to the wrong change.

Still open: no test asserts an audit row exists for either outcome, and none
injects an append failure. Both paths *are* exercised end to end (the rotation
journey allows, `automation_cannot_escalate_itself_to_an_administrator` denies),
so this is weaker proof rather than none. FIND-006-4 remains open on its own terms.

### FIND-006-2 — revocation scoped to the named principal — **CLOSED**

`credential_belongs_to` (`wyrd-sql/src/queries/auth/api_keys.rs:93-108`) matches
on `data_tenant_id = wyrd.current_tenant() AND id = $1 AND principal_id = $2`,
and `routes.rs:357-365` refuses with `NotFound` on a mismatch *before* any write,
so the wrong-principal path revokes nothing and advances no epoch.
`a_credential_cannot_be_revoked_through_another_principal` proves both halves.

### Migration `20260910000026_audit_tenant_admin_principal_kind.sql` — **CORRECT**

The original constraint is the inline column `CHECK` at
`20260802000000_vala_audit_staging.sql:54`, which PostgreSQL auto-names
`audit_staging_principal_kind_check` — the name the migration drops. The
replacement is a strict superset (`tenant_admin` added to `user`, `service`,
`agent`), so `ADD CONSTRAINT` revalidates every existing row and none can fail.
The hash chain is unaffected: `principal_kind` is carried as free text through
`vala-bifrost-redux/src/tables/audit/{audit_log,projection}.rs` with no enum
parse and no constraint downstream, so nothing rejects a `tenant_admin` row at
publication. The sequence number is next in the directory. `pg_migration` (17
tests) and every journey apply it cleanly.

One stylistic divergence worth stating and not raising as a finding: sibling
migrations in this change discover the constraint name through
`pg_get_constraintdef` rather than hardcoding it. Here the name is deterministic
from a migration in the same tree, so the hardcoded `DROP CONSTRAINT` without
`IF EXISTS` is safe.

## Findings

### FIND-006-5 — `INCORRECT` (new, introduced by the remediation) — a re-revoke returns 404 after committing a principal-wide token kill

**Violated obligation**: the route's own documented contract
(`routes.rs:337-339`: `NotFound` means "the credential is unknown in this
tenant"); INV-011 fail-closed on ambiguity; REQ-010 read with the remediation
task's constraint that the correction preserve adjacent behavior.

**Location**: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:357-400`.

**Evidence**: the order of operations is unconditional.

```
357  let owned = credential_belongs_to(...)   // TRUE even when already revoked
360  if !owned { return NotFound }            // only the wrong-principal case
367  let revoked = revoke_api_key(...)        // FALSE: SQL filters revoked_at IS NULL
370  revoke_service_account_principal(...)    // epoch bumped UNCONDITIONALLY
373  conn.commit()                            // COMMITTED
379  notify_principal_revoked(...)            // fanned out
393  if revoked { 204 } else { NotFound }     // reports 404
```

`credential_belongs_to` (`api_keys.rs:96-105`) has no `revoked_at` predicate, so
an already-revoked credential passes the ownership gate. `REVOKE_API_KEY_SQL`
(`api_keys.rs:11-15`) carries `AND revoked_at IS NULL`, so `revoke_api_key`
returns `false`. Nothing between :367 and :373 consults that value.

**Falsifying scenario**: automation principal `payments` holds credentials C1 and
C2. An operator revokes C1 (204). The SDK's retry, a second operator, or a
replayed request issues the identical `DELETE
/v1/principals/{payments}/credentials/{C1}`. The response is `404 Not Found` —
telling the caller nothing happened — but `tokens_not_before` has already been
advanced and committed, so every live token C2 minted since the first revocation
stops authorizing. Repeating the 404-returning call is a repeatable,
apparently-inert denial of service against a principal's live tokens, available
to any holder of `service_accounts:write`. It also makes the ordinary
retry-after-timeout on a revoke — the most likely real call — destructive in a
way the 204 path already accounted for and this one does not.

Secondary consequence on the same lines: because the `!owned` return at :360-365
drops `conn` without committing, the `Allowed` audit row recorded at :95-102 is
rolled back. A caller probing `{principal_id, credential_id}` combinations
therefore leaves no audit trail for the probe, which is the opposite of what
FIND-005-1 was closed to achieve.

**Required correction**: the epoch advance must be conditional on the revocation
having actually happened, so a credential that was already revoked is a no-op
that revokes nothing and advances nothing — the same shape `revoke_api_key`
already documents as idempotent (`api_keys.rs:19-20`). Resolve the not-found
case before any write, and let the decision row survive that refusal. Prove it by
revoking a credential twice and asserting that a token minted from a *sibling*
credential after the first revocation still authorizes after the second call.

---

### FIND-006-6 — `INCORRECT` (new, introduced by the remediation) — the revocation NOTIFY names a kind the verifier may never look up

**Violated obligation**: the Ponytail reuse ladder (the repository already owns
this lookup); the commit's own claim that the NOTIFY makes other replicas "drop
their cached epoch instead of serving the revoked token until the TTL lapses".

**Location**: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:379-385`.

**Evidence**: the kind is hardcoded — `PrincipalKindTag::Service`. The epoch
cache key includes the kind (`wyrd-auth/src/revocation_resolver.rs:26-31`,
`invalidate` at :79-82), and `epoch` resolves `TenantAdmin`, `Service` and
`Agent` from the same table but keys them separately (:116-122). The repository's
existing owner does not guess: `revoke_principal_in_conn`
(`wyrd-auth/src/revoke.rs:35-45`) reads `service_account_by_id` and derives the
real kind through `principal_kind_wire`.

**Falsifying scenario**: a tenant administrator revokes a credential belonging to
a principal of kind `tenant_admin` — which provisioning mints one of for every
tenant (`components/platform/provisioning.rs`) — or `agent`. The NOTIFY
invalidates the cache entry for `(tenant, Service, id)`. Every other replica
reads `(tenant, TenantAdmin, id)`, misses the invalidation entirely, and keeps
authorizing the revoked credential's live tokens for the full five-second TTL.
The durable epoch write still enforces it eventually, so this bounds the window
rather than defeating revocation — but it is exactly the window the commit
message claims to have closed, and it closes only for `service` principals.

**Required correction**: reuse the existing kind lookup rather than asserting the
kind, so the NOTIFY names the key the verifier actually reads.

## Prior findings — still open, re-stated

Each was independently re-checked against the current tree; none is addressed.

| ID | State | Evidence at candidate `4225069` |
|---|---|---|
| **FIND-004-1** — tenant lifecycle enforced nowhere on the auth path | **OPEN** | `wyrd-auth/src/exchange_api_key.rs:231` still checks only the *principal's* `status`. `platform.tenants` appears nowhere in `wyrd-auth/src/*.rs` except a comment at `exchange_api_key.rs:1087`. `TenantConn::acquire` performs no directory lookup. A suspended tenant's credentials still exchange and still authorize; a `failed` tenant whose admin rows committed is still fully usable. INV-006 is unenforced. |
| **FIND-004-3** — a failed provisioning burns its slug forever | **OPEN** | `wyrd-sql/src/queries/platform/provisioning.rs:34` is still a bare `INSERT INTO platform.tenants` with no `ON CONFLICT`, and `components/platform/provisioning.rs` has no branch that adopts an existing `provisioning` or `failed` row. A retry for the same slug returns `Conflict` permanently. REQ-027's "resumable to the same outcome" is unmet. |
| **FIND-004-2** — no tenant list / inspect / suspend / resume | **OPEN** | `components/platform/routes.rs:38-45` still exposes exactly two routes: `POST /platform/tenants` and `POST /platform/tenants/admin/credentials`. `set_tenant_suspended` (`queries/platform/provisioning.rs:105`) still has zero callers — grep finds only its definition. `Permission::tenant_read()` / `tenant_suspend()` are granted (`boot/init.rs:63-64`) and required by no route. REQ-028's list/inspect/suspend/resume capability does not exist. |
| **FIND-004-5** — CLI and documentation untouched | **OPEN** | `git diff --name-only c5c2075..HEAD -- docs/ crates/wyrd/wyrd-cli/` returns nothing. The spec's headline three-command journey still has no `wyrd tenant create` and no self-hosting page. |
| **FIND-004-6** — new routes absent from the generated contract | **OPEN** | `grep -rn 'utoipa::path' components/platform/ components/principals/` returns nothing; `openapi.yaml` contains zero occurrences of `platform` or `principals`. `codegen:check` remains vacuous for this surface. |

Also still open, unchanged from the prior verdict and re-confirmed here:

- **FIND-003-2** — no failure-injection, concurrency, or captured-output test for
  initialization; `initialize_platform_root` is still driven as a Rust function,
  never through the `init` subcommand.
- **FIND-003-3** — `InitError::NotConfigured` (`boot/init.rs:38-39`) is still
  unconstructible; grep finds the identifier only at its own declaration.
- **FIND-004-4** — no provisioning failure, retry, or concurrency test.
- **FIND-004-7** — `mark_tenant_failed` is still `let _ = …` and unreachable
  under cancellation.
- **FIND-004-8** — `bootstrap-key` still named at `boot/init.rs:4`.
- **FIND-005-2** — no cross-tenant negative. The new
  `automation_cannot_escalate_itself_to_an_administrator` covers the escalation
  half; tenant A against tenant B's principals and credentials is still absent.
- **FIND-005-4** — a duplicate principal name still maps through `internal()`
  (`routes.rs:237, 416-421`) to a 500 carrying the raw database string; no
  `UniqueViolation` discrimination exists on this surface.
- **FIND-006-3** — `components/platform/recovery.rs` still reads no
  `platform.tenants.status`; recovery against a suspended or failed tenant still
  mints a working credential.
- **FIND-006-4** — no injected-audit-failure proof on the credential surface.

## Acceptance matrix against the remediation task's 24 criteria

| # | Criterion (abbreviated) | Result |
|---|---|---|
| 1 | Injected failure at each init stage leaves it uninitialized; retry succeeds | **FAIL** — behavior correct, no proof (FIND-003-2) |
| 2 | Concurrent init → one principal, one credential, loser refuses | **FAIL** (FIND-003-2) |
| 3 | No credential material in captured server output | **FAIL** (FIND-003-2) |
| 4 | Uninitialized server refuses platform routes with a stable error | **FAIL** (FIND-003-2) |
| 5 | `InitError::NotConfigured` constructed or deleted | **FAIL** (FIND-003-3) |
| 6 | Non-`active` tenant refuses exchange and yields no context | **FAIL** (FIND-004-1) |
| 7 | Suspend stops access; resume restores it | **FAIL** (FIND-004-1, FIND-004-2) |
| 8 | Recovery refuses an unusable tenant | **FAIL** (FIND-006-3) |
| 9 | Retry after failure resumes to one tenant and one credential | **FAIL** (FIND-004-3) |
| 10 | Concurrent creation → one tenant, one admin principal | **FAIL** (FIND-004-4) |
| 11 | A cancelled attempt is observable as failed | **FAIL** (FIND-004-7) |
| 12 | Every decision on the surface audits principal, credential, permission, resource, tenant, outcome | **PASS (behavior)** — `routes.rs:85-137`; no row-level assertion |
| 13 | Unrecordable audit refuses and commits nothing | **FAIL** — code correct, no proof (FIND-006-4) |
| 14 | Denials recorded | **PASS (behavior)** — `routes.rs:106`; exercised, not asserted |
| 15 | Tenant A refused against tenant B, revealing nothing | **FAIL** (FIND-005-2) |
| 16 | No tenant-plane path creates a platform principal | **PASS** — structural + `a_tenant_administrator_cannot_configure_platform_sign_in` |
| 17 | Duplicate principal name → conflict, not a 500 | **FAIL** (FIND-005-4) |
| 18 | A token minted before revocation stops authorizing | **PASS** — `routes.rs:367-373`; `platform_admin_e2e.rs:533-571` |
| 19 | Naming the wrong principal is a not-found and revokes nothing | **PASS** — `api_keys.rs:93-108`; `a_credential_cannot_be_revoked_through_another_principal` |
| 20 | Root can list, inspect, suspend and resume, each audited | **FAIL** (FIND-004-2) |
| 21 | Handlers annotated; `codegen:check` meaningful | **FAIL** (FIND-004-6) |
| 22 | Three-command journey with no database access | **FAIL** (FIND-004-5) |
| 23 | Documentation of install → initialize → create → configure, rotation, recovery | **FAIL** (FIND-004-5) |
| 24 | No `bootstrap-key` outside `changes/` | **FAIL** (FIND-004-8) |

Additional criterion implied by the remediation constraint "preserve adjacent
behavior": **FAIL** — FIND-006-5, FIND-006-6.

## Verdict

`FIX_REQUIRED`.

Four of the five claims the remediation commit makes are true and were verified
independently rather than accepted: initialization is genuinely one transaction
with a working `AlreadyInitialized` path; revocation genuinely advances the
principal epoch in the deciding transaction and the journey proving it is not
reading stale cache; the tenant surface genuinely audits allow and deny through
the repository's existing same-transaction seam with fail-closed behavior; and
credential revocation is genuinely scoped to the named principal. The audit
CHECK migration is correct, widens a strict superset, and cannot break the hash
chain or any existing row.

It nonetheless does not pass. The remediation closed 5 of 24 acceptance criteria.
Two of the still-open five the caller named are severe on their own — FIND-004-1
leaves every tenant lifecycle state advisory, so a suspended tenant's credentials
still authenticate, and FIND-004-3 makes a failed provisioning burn its slug with
no application-level remedy.

And the remediation introduced two defects of its own, neither acknowledged by
the commit message. The message reasons carefully about the *intended*
consequence of coupling revocation to the per-principal epoch — that a sibling
credential's live tokens die too — and concludes, correctly, that this is the
granularity the schema offers. It does not reason about the *unintended* one:
that the epoch advance is unconditional, so the already-revoked path commits a
principal-wide token kill and then reports `404 Not Found` (FIND-006-5). Nor
about the cache key: the NOTIFY hardcodes `PrincipalKindTag::Service` while the
verifier keys the epoch cache by the principal's real kind, so for a
`tenant_admin` or `agent` principal the fanout invalidates nothing and the window
the commit claims to have closed stays open for the full TTL (FIND-006-6).

None of this requires changing approved behavior or an expensive-to-reverse
decision. Every correction lies inside spec revision 6 and reuses an owner the
repository already has — `revoke_api_key`'s existing idempotence for FIND-006-5,
`revoke_principal_in_conn`'s existing kind lookup for FIND-006-6.

The prior remediation task
`changes/active/admin-principals/review/task-003-006/TASK-003-006-R1-lifecycle-audit-and-atomicity.md`
remains the governing remediation for the nineteen prior findings and is not
superseded; `TASK-003-006-R2-revocation-side-effects.md` in this directory covers
only the two defects introduced by R1.

Findings carried forward: FIND-003-2, FIND-003-3, FIND-004-1, FIND-004-2,
FIND-004-3, FIND-004-4, FIND-004-5, FIND-004-6, FIND-004-7, FIND-004-8,
FIND-005-2, FIND-005-4, FIND-006-3, FIND-006-4.
Findings closed: FIND-003-1 (behavior), FIND-005-1 (behavior), FIND-006-1,
FIND-006-2.
New findings: FIND-006-5, FIND-006-6.
