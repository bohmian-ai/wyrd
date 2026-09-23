---
id: TASK-003-R2
kind: remediation
status: approved
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-078, REQ-105, REQ-112, REQ-134, INV-007, AC-019, AC-020, AC-028]
depends_on: [TASK-003-R1]
parent_task: TASK-003
remediates: [FIND-TASK-003-9, FIND-TASK-003-12, FIND-TASK-003-13]
---

# Close TypeScript Status, TenantConn, and Verification-Gate Gaps

## Authority and Immutable Subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- R1 verdict and remediation: `changes/active/verified-change-contract/review/TASK-003-r1/`
- R2 verdict: `changes/active/verified-change-contract/review/TASK-003-r2/verdict.md`
- R2 validated findings: `changes/active/verified-change-contract/review/TASK-003-r2/findings-validation.md`
- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Reviewed cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`

Implement this remediation cumulatively. The next review must reassess the
complete original base through the new candidate and preserve all prior finding
IDs.

## Outcome

TASK-003 is complete when TypeScript exposes the same typed Card verification
status as the other first-class surfaces, the exact-principal lookup relies
only on its `TenantConn` RLS authority, and the cumulative whitespace gate is
reproducibly green.

## Diagnoses and Required Corrections

### `FIND-TASK-003-9` — TypeScript bypasses the public Card status contract

`Cards.get` and `WyrdState.card` return the live Card JSON, including
`status.verification.binding_ids`, but the exported TypeScript `Card` interface
does not declare `status`. The integration journey reaches the real public
path, then casts an unknown property to a test-local shape; this allows the
test and typecheck to pass even when the SDK omits or mis-types the public
contract.

Extend the existing hand-authored TypeScript Card projection with the complete
already-live status contract: `phase`, optional/nullable `message`,
optional/nullable `updated_at`, and optional/nullable `verification`, whose
`binding_ids` is a readonly string list. Make `Card.status` optional/nullable
to match the existing server contract. Delete the local status cast and access
the binding IDs directly through the exported type in the existing journey.
Reuse the current `Card`, `Cards.get`, `WyrdState.card`, and lifecycle JSON
transport; do not add runtime conversion, validation, another client, a new
endpoint, a generator, or a partial verification-only status abstraction.

This closes the gap because TypeScript users and `ts:typecheck` will consume
the same complete Card status shape already served and documented by Rust,
OpenAPI, and JSON Schema.

### `FIND-TASK-003-12` — A `TenantConn` lookup duplicates tenant authority

The remediated `service_account_by_card_ref` correctly selects exactly one
active match, but its SQL still filters `data_tenant_id = $1` and binds the
tenant from `TenantConn`. Repository rules and TASK-003 explicitly prohibit
manual tenant predicates on `TenantConn` paths because forced RLS through
`wyrd.current_tenant()` is the sole tenant authority.

Remove only the manual tenant predicate and its bind from the existing query,
renumbering the principal-kind and CardRef parameters. Preserve active-status
filtering, JSON containment, `LIMIT 2`, exact-one result handling, all callers,
and the caller-owned transaction. Do not add another query API, caller-level
tenant handling, or any replacement tenant predicate.

This closes the gap by reusing the existing forced-RLS `TenantConn` boundary
without changing the fail-closed identity behavior delivered by R1.

### `FIND-TASK-003-13` — The cumulative whitespace gate fails

The candidate records `git diff --check` as passing, but the exact cumulative
command exits nonzero with 28 trailing-whitespace diagnostics in the preserved
R1 standards report. This makes a required completion claim unreproducible.

Remove only the reported trailing spaces from that report without changing any
word, finding, conclusion, or implementation artifact. Reuse the existing Git
gate and rerun it against the original base through the new candidate. No
production or test change is justified for this formatting-only correction.

## Constraints and Preserved Behavior

- Preserve closure of `FIND-TASK-003-1` through `FIND-TASK-003-8`,
  `FIND-TASK-003-10`, and `FIND-TASK-003-11`.
- Preserve exact-one fail-closed CardRef selection for API-key issuance,
  workload JWT, and delegation.
- Preserve forced RLS, caller-owned `TenantConn` transactions, active-principal
  filtering, audit behavior, and tenant isolation.
- Preserve the complete server Card status field names, nullability, native JSON
  transport, stable UUIDv7 values, and generic openness of Card metadata/spec.
- Do not add runtime status conversion, another SDK client, a new endpoint,
  another SQL lookup, a tenant argument, a generator, or a test harness.
- Do not change the content or conclusions of any prior review artifact while
  removing trailing whitespace.

## Acceptance Criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-003-9` | Exported TypeScript Card/status types expose the complete live verification status, and the existing journey accesses stable UUIDv7 binding IDs directly without a cast. |
| `FIND-TASK-003-12` | The shared `TenantConn` CardRef lookup contains no tenant predicate/bind and preserves exact-one fail-closed behavior for every caller. |
| `FIND-TASK-003-13` | The exact original-base-to-new-candidate `git diff --check` command exits zero, with prior review wording unchanged. |

## Focused and Broader Proof

- Update the existing TypeScript Card integration journey; run its exact test
  selector, `mise run ts:typecheck`, and `mise run ts:test:integration`.
- Strengthen the existing SQL-text test for `service_account_by_card_ref` to
  assert principal kind at `$1`, CardRef at `$2`, and absence of
  `data_tenant_id`; run its exact selector.
- Rerun `mise run test:principals:unit`,
  `mise run test:principals:integration`, `mise run test:sql`, and
  `mise run check:tenant-isolation` for the shared lookup boundary.
- Run `mise run codegen:check`, `mise run fmt`, and `mise run lints`.
- Run:

```bash
git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..<new-candidate>
```

Record every newly named test with its exact repository-native selector. Do not
rerun unrelated broad lanes unless a changed shared surface makes them owning
coverage or a required lane fails and repository rules require root-cause
repair.

## Implementation Evidence

Candidate commits: `2f6f76b18` (whitespace), `a11e3d318` (SQL), `fe7e07a9e` (TypeScript), plus this evidence commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-003-9` | `sdks/wyrd-sdk-ts/wyrd/src/index.ts` adds `Card.status?: CardStatus \| null`, exported `CardStatus` (`phase`, nullable `message`/`updated_at`/`verification`) and `VerificationStatus` (`binding_ids?: readonly string[]`, optional because the server omits an empty list); `tests/integration/cards-state.test.ts` deletes the cast helper and reads `(await cards.get(...)).status?.verification?.binding_ids` directly | `mise run ts:typecheck`; exact selector `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/cards-state.test.ts -t "registers, reads, hydrates, and loads offline state"'`; `mise run ts:test:integration` | PASS |
| `FIND-TASK-003-12` | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`: `SERVICE_ACCOUNT_BY_CARD_REF_SQL` drops `data_tenant_id = $1` and its bind; `principal_kind = $1`, `card_ref @> $2`, active filter, `LIMIT 2`, exact-one selection, callers, and caller-owned transaction unchanged; `wyrd.auth_service_accounts` is `FORCE ROW LEVEL SECURITY` (migration `20260601000001_auth.sql:98`) | `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` (asserts `$1`/`$2`, no `data_tenant_id`, no `$3`); `mise run test:principals:unit`; `mise run test:principals:integration`; `mise run test:sql`; `mise run check:tenant-isolation` | PASS |
| `FIND-TASK-003-13` | `review/TASK-003-r1/standards-review.md`: trailing whitespace stripped only (`git diff -w` empty) | `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..<new-candidate>` exits 0 | PASS |

Also ran: `mise run codegen:check`, `mise run fmt`, `mise run lints` — all exit 0.

Non-goals preserved: no runtime status conversion, new client, endpoint, SQL lookup, tenant argument, generator, or harness; no prior review wording changed. No unrelated files changed.
