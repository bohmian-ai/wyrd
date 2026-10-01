---
id: TASK-007-R5
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-142, REQ-146, REQ-147, REQ-152, INV-015, AC-030, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-18, FIND-TASK-007-19]
---

# TASK-007-R5 — Enforce the Operator Key Source and Bound Retry Settlement

## Authority and immutable subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger:
  `changes/active/verified-change-contract/review/TASK-007-r5/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`

## Outcome

Make the existing Operator key-source validator exactly enforce revision 36
and make provider-directed retries settle durably for every decimal
`Retry-After` the current parser accepts. Preserve the working encryption,
key rotation, provider delivery, PostgreSQL clock ownership, retry budget,
lease fencing, and all public contracts.

## Issue diagnosis and required correction

### `FIND-TASK-007-18` — production accepts a development-only environment KEK

Revision-36 `REQ-147` makes environment-sourced Operator KEKs development-only,
allows the restrictive mounted-file source for explicitly single-tenant
production, and requires Vault over HTTPS for multi-tenant production.

`OperatorKeysConfig::validate` currently rejects non-Vault sources only when
both `production` and `multi_tenant` are true. When production config includes
`auth.tenant_slug`, the caller passes `multi_tenant = false`, so
`OperatorKeySource::Env` returns success. The single-tenant readiness path then
correctly defers provider access, leaving the invalid source live until normal
connection sealing or delivery reads the environment key.

The observable consequence is that a production server can create and open
Operator credentials under a process-environment KEK, outside the approved
file/Vault production boundary.

Correct this in the existing `OperatorKeysConfig::validate` owner by rejecting
the environment source whenever `production` is true. Preserve:

- environment keys in development;
- owner-only file or Vault for explicitly single-tenant production;
- Vault-only, HTTPS, active-tenant readiness for multi-tenant production;
- exact active-version validation; and
- deferred provider availability for single-tenant production, so a missing
  file/Vault key still refuses only sealing while delivery uses the existing
  credential-store retry behavior.

This correction needs no new source, resolver, flag, trait, or readiness
mechanism.

### `FIND-TASK-007-19` — oversized `Retry-After` prevents durable settlement

`status_failure` accepts decimal `Retry-After` values through `u64::MAX` and
passes the resulting duration to `OperatorDispatchQueue::retry`. The queue
binds a saturating millisecond value, but `RETRY_SQL` first evaluates the
requested interval and only then applies the outer absolute-deadline `LEAST`.
PostgreSQL rejects the maximum value with `interval out of range` before the
clip can operate.

The observable consequence is a settlement error that leaves the dispatch
`running` until lease expiry. Reclaim can consume another attempt and repeat an
externally ambiguous send instead of recording a bounded retry or terminal
transition.

Keep PostgreSQL as the coordination owner. In the existing retry statement,
bound the requested delay operand by the already supplied configured deadline
operand before multiplying it by the millisecond interval. Retain the outer
absolute-deadline clip, attempt and deadline predicates, lease-token fence,
error payload, database timestamps, and current fixed queue ceilings. Do not
add Rust-side wall-clock/deadline calculation, a parser branch, retry type,
clock abstraction, or dependency.

## Constraints and preserved behavior

- Do not change the approved env/file/HashiCorp Vault contract or add a key
  source.
- Do not eagerly read a single-tenant production file/Vault key at readiness;
  preserve the approved failure isolation for unavailable keys.
- Do not change encryption, AAD, versioning, rewrap, RLS, audit, redaction, or
  credential lifetime.
- Do not change provider protocols, accepted `Retry-After` syntax, attempt
  count, 30-second attempt timeout, 30-second/two-minute normal backoff,
  five-minute deadline, status model, lease fencing, or at-least-once
  ambiguity.
- Do not change public HTTP, MCP, CLI, Rust, Python, TypeScript, schema, or
  OpenAPI contracts.

## Non-goals

- No new key resolver, cloud provider, retry parser, duration wrapper,
  scheduler, clock, SQL schema, migration, cache, trait, dependency, or public
  error.
- No broader key-readiness redesign or delivery-worker refactor.
- No exactly-once delivery claim and no change to HTTP-date `Retry-After`
  support.

## Acceptance criteria

| Finding | Required closure |
|---|---|
| `FIND-TASK-007-18` | Development env remains accepted; production single-tenant env is rejected during configuration validation before key access; production single-tenant owner-only file and Vault remain accepted; production multi-tenant remains Vault-only over HTTPS with active-key readiness. |
| `FIND-TASK-007-19` | A retryable provider response with the maximum accepted decimal `Retry-After` settles without a SQL interval error; the dispatch becomes retrying or terminal under its existing budget/deadline, never schedules after the database-owned deadline, and does not rely on lease expiry for progress. |

## Focused and broader proof

Add one focused configuration test covering the four key-source cases in
`FIND-TASK-007-18`, while retaining the existing multi-tenant readiness and
single-tenant deferred-availability tests.

Add one focused Postgres-backed queue or real-server provider regression for
`FIND-TASK-007-19` using the maximum accepted decimal `Retry-After`. Assert the
durable state, database-owned deadline bound, and absence of lease-expiry
recovery. Retain the ordinary 90-second provider-delay and existing
fence/deadline coverage.

Run each newly named Rust test through its exact `mise exec -- cargo nextest
run --locked` command with explicit package, target, features, and exact test
expression, using the repository Postgres wrapper where required. Then run the
narrow owning lanes and gates:

```bash
mise run test:sql
mise run test:wyrd
mise run fmt
mise run lints
mise run check:tenant-isolation
mise run check:unwrap-audit
git diff --check
```

Run `mise run codegen:check` only if implementation unexpectedly touches a
generated/public contract; no such change is required.

Route this task directly to `$wyrd-implement`.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-007-18` | `OperatorKeysConfig::validate` in `crates/wyrd/wyrd-server/src/config.rs` refuses `source = "env"` whenever `production`; file/Vault and the multi-tenant Vault/HTTPS rule are unchanged; readiness untouched | `config::tests::operator_key_source_follows_deployment` (dev env accepted; single-tenant prod env refused; single-tenant prod file and Vault accepted; multi-tenant prod Vault accepted, file refused); retained `production_vault_requires_https`, `pg_operator_connection_routes::production_boot_requires_every_active_tenant_key` | PASS |
| `FIND-TASK-007-19` | `RETRY_SQL` in `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs` bounds `$5` by `$6` (`LEAST($5, $6)`) before building the interval; outer deadline clip, predicates, fence, and payload unchanged | `pg_verifier_runs::maximum_retry_after_settles_at_the_deadline` (`u64::MAX` seconds → `retrying`, `next_attempt_at = created_at + 5 min`, lease cleared); fails with `interval out of range` without the bound; retained `dispatch_delivery_obeys_budget_deadline_and_fencing`, `expired_dispatch_deadline_fails_without_a_claim`, and the `pg_operator_delivery` 90-second provider-delay coverage | PASS |

Commands (all exit 0 in this session):

- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::operator_key_source_follows_deployment) | test(=config::tests::production_vault_requires_https) | test(=config::tests::operator_key_version_must_fit_i32)'`
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=maximum_retry_after_settles_at_the_deadline) | test(=dispatch_delivery_obeys_budget_deadline_and_fencing) | test(=expired_dispatch_deadline_fails_without_a_claim)'`
- `mise run test:sql`, `mise run test:wyrd` (2160 passed), `mise run fmt`, `mise run lints`,
  `mise run check:tenant-isolation`, `mise run check:unwrap-audit`, `git diff --check`

Non-goals held: no new source, resolver, parser branch, retry type, clock,
migration, dependency, public error, or public contract change; `codegen:check`
not required. Also imported the `wyrd_server::config` types at module scope in
`crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs` (test-only
consistency cleanup).
