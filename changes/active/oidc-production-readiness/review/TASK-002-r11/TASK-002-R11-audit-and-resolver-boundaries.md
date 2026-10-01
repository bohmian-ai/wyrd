---
id: TASK-002-R11
kind: remediation
status: implemented
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-005, REQ-017, INV-001, INV-004, AC-007]
depends_on: [TASK-002-R10]
parent_task: TASK-002
remediates: [FIND-TASK-002-25, FIND-TASK-002-26, FIND-TASK-002-27]
---

# Close activation audit and SQL capability gaps

## Authority and review subject

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 5.
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md` (historical revision 4).
- Prior remediation: both packets in
  `changes/active/oidc-production-readiness/review/TASK-002-r10/` and their
  completed evidence.
- Reviewed original base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`.
- Reviewed candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8`.
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-002-r11/findings-validation.md`;
  verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r11/verdict.md`.

Apply `AGENTS.md`, `architecture/agent-rules.md`, and the approved spec.
Implement these three corrections together, then review the original
base-to-new-candidate range. Preserve the R10 human-directed work and closure
of `FIND-TASK-002-1` through `FIND-TASK-002-24`.

## FIND-TASK-002-25 — audit the allowed keyless activation refusal

`crates/wyrd/wyrd-server/src/components/admin/identity.rs:303-314` evaluates
`identity_connections:write` and passes an uncommitted allowed `AuditEvent`
to `HumanConnections::activate`. At
`crates/wyrd/wyrd-auth/src/connections.rs:469-471`, `require_keyring()?`
returns `sealing_key_missing` before `begin_locked` appends that decision.
An authorized administrator's refused activation therefore leaves no
canonical authorization audit row, violating REQ-017 and the repository's
transactional audit rule. The current keyless test checks the error but not
audit persistence.

Keep the missing-key refusal. In `HumanConnections::activate`, enter its
existing `begin_locked` transaction before checking the keyring, then use
the existing `commit_refusal` path for a missing key before reading the
candidate or recovery key. This records the already evaluated decision in
the activation transaction. Reuse that owner and path; do not call the
handler's separate `refuse_after_decision`, create a second audit sink, or
change the denied-caller and recovery-key decision paths. Audit append
failure still refuses and never promotes a connection.

## FIND-TASK-002-26 — correct resolver module rustdoc

`crates/wyrd/wyrd-auth/src/pg_resolvers.rs:3-9` still says the module lives
in `wyrd-server`, holds a `PgPool`, and uses an earlier commit's queries.
The changed production resolvers live in `wyrd-auth`, hold `WyrdPostgres`,
and acquire tenant-scoped reads through it. The stale `PgPool` intra-doc
link also lost its import target. This misstates the SQL and RLS boundary of
a materially changed module, contrary to `AGENTS.md` §16.

Replace only that module header with the actual owner, existing
`WyrdPostgres` handle, optional issuer sealing key, and tenant-scoped RLS
reads. Remove the old commit and `F02` references. Preserve executable code
and the accurate item-level documentation; no new documentation checker or
behavior test is needed.

## FIND-TASK-002-27 — remove raw pools from the live audit boundary

`crates/wyrd/wyrd-server/src/audit/mod.rs:145-186` still accepts `&PgPool`
in `record_audit` and an owned `PgPool` in `record_audit_owned`, then builds
`TenantConn` internally. Identity connection refusals and shared permission
denials call the first form; gateway invocation, cards, Bifrost and query
paths also call these forms. The signatures let callers propagate an
arbitrary raw SQL pool across the audit boundary. The active no-exceptions
instruction and `architecture/agent-rules.md` allow only scoped SQL
capabilities there. The current `check:tenant-isolation` does not scan this
server audit module, so a green check does not establish compliance.

Retain the one canonical `append_audit` and standalone transaction semantics.
Have both audit forms acquire their `TenantConn` through the existing
`ValaPostgres::tenant_conn` owner and pass that owner, rather than a raw
pool, at every production caller. Preserve the owned form's `Send` future,
gateway invocation's tracked non-blocking audit behavior, and query audit
failure semantics. Do not add a pool wrapper, second audit function family,
publisher, or per-caller SQL transaction machinery. Extend the existing
`check:tenant-isolation` to reject raw-pool signatures in this module while
retaining the separate pool-construction check. The unchanged, currently
uncalled eval resolver is outside this task; it is not an approved exception
to the global rule.

## Acceptance and proof

| Finding | Required result | Direct proof |
|---|---|---|
| 25 | A keyless public-client activation returns the same typed refusal, promotes no connection, and commits exactly one redacted allowed activation decision. An audit append failure refuses. | Extend `connections::probe_tests::activation_without_a_sealing_key_is_refused_for_a_secretless_provider` to inspect canonical staging and no promotion. Reuse the existing activation audit-failure case in `tenant_connection_rotation_journey`. |
| 26 | Resolver module header names the real owner and SQL boundary; executable resolver code is unchanged. | Inspect the header against both resolver bodies and verify a documentation-only subdiff. |
| 27 | No production audit entry point or caller passes a raw SQL pool; identity, gateway and query audit behavior is preserved. | Inspect both audit forms and all callers; prove the amended tenant-isolation check rejects an injected raw-pool audit signature; run the affected identity, gateway and query lanes. |

The named keyless Postgres test runs through the repository-managed setup:

```text
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=connections::probe_tests::activation_without_a_sealing_key_is_refused_for_a_secretless_provider)'
```

The named provider-backed journey runs through the identity setup:

```text
mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=tenant_connection_rotation_journey mise run test:identity:journey
```

Confirm both selectors still select exactly one test. Then run `mise run
test:identity:journey`, `mise run test:principals:integration`, `mise run
test:gateway:native`, and `mise run test:bifrost:journey:server` for the
affected audit callers. Run `mise run check:tenant-isolation`, `mise run
check:from-pools-allowlist`, `mise run fmt`, `mise run lints`, and `git diff
--check`. Record the focused results, full caller inventory, static rustdoc
inspection, and any gate limits. Do not count prior green runs as proof of
these corrections.

## Material stop conditions

Stop and report if the existing `ValaPostgres` tenant connection cannot
preserve the audit transaction or owned-future semantics, or if a correction
would require a new audit authority, authorization policy, or public contract.
TASK-003 browser and TASK-004 CLI redemption remain downstream and are not
implemented by this remediation.

## Implementation evidence

Commits: `1e20fb184` (findings 25, 26), `3088a6f4c` (finding 27), on top of
candidate `bae424cc6`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 25: keyless activation commits exactly one redacted allowed decision, promotes nothing, keeps `sealing_key_missing` | `HumanConnections::activate` enters `begin_locked` first, then `commit_refusal` on `require_keyring` failure before any candidate or recovery-key read (`crates/wyrd/wyrd-auth/src/connections.rs`) | `connections::probe_tests::activation_without_a_sealing_key_is_refused_for_a_secretless_provider` now asserts one staged allowed `identity.oidc.candidate.activate` row without the recovery secret and no Active connection (1/1 pass); `tenant_connection_rotation_journey` with injected activation-audit failure (1/1 pass) | PASS |
| 26: resolver module header names the real owner and SQL boundary | `crates/wyrd/wyrd-auth/src/pg_resolvers.rs` module rustdoc only: `wyrd-auth`, `WyrdPostgres`, optional issuer sealing keyring, tenant RLS through `WyrdPostgres`; commit/`F02` markers and stale `PgPool` link removed | Documentation-only subdiff inspected against `PgIssuerResolver`/`PgWorkloadBindingResolver` bodies; `mise run fmt`, `mise run lints` | PASS |
| 27: no production audit entry point or caller passes a raw pool | `record_audit(&ValaPostgres, ..)` acquires via `ValaPostgres::tenant_conn`; `record_audit_owned(ValaPostgres, ..)` spawns `record_audit` (still `Send`); gateway tracked task clones `ValaPostgres` | Caller inventory below; `check:tenant-isolation` now scans `wyrd-server/src/audit/` and rejects `PgPool`, `.app_pool()` and `.vala_pool()` — an injected `&sqlx::PgPool` signature failed the check, restored tree passes | PASS |

Caller inventory (14 sites, all pass `postgres.vala()` / an owned `ValaPostgres`):
`audit/mod.rs` (2 denial paths, owned adapter), `bifrost/service.rs`,
`components/admin/identity.rs` (2), `components/cards/routes.rs`,
`components/cards/service.rs` (3), `components/gateway/service.rs`,
`components/gateway/invocation.rs` (tracked non-blocking), `query/service.rs` (2 owned).

Verification (all exit 0):

- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=connections::probe_tests::activation_without_a_sealing_key_is_refused_for_a_secretless_provider)'` — 1 selected, 1 passed.
- `mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=tenant_connection_rotation_journey mise run test:identity:journey` — 1 selected, 1 passed.
- `mise run test:identity:journey` — 27/27.
- `mise run test:principals:integration` — 16 + 16 + 19 passed.
- `mise run test:gateway:native` — 8 passed (2 skipped by the lane), Python 2 passed.
- `mise run test:bifrost:journey:server` — 16/16.
- `mise run check:tenant-isolation`, `mise run check:from-pools-allowlist`, `mise run fmt`, `mise run lints`, `git diff --check`.

Non-goals stayed out of scope: no new audit sink, wrapper, or publisher; denied-caller and recovery-key paths are unchanged; the dormant eval resolver and TASK-003/004 redemption are untouched.

Remaining risk: `ServerPostgres::vala_pool()` itself still exists for non-audit callers outside this scope.
