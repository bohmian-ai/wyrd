---
id: TASK-003-R1
kind: remediation
status: approved
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-078, REQ-095, REQ-102, REQ-104, REQ-105, REQ-106, REQ-107, REQ-108, REQ-112, REQ-134, REQ-145, INV-001, INV-007, INV-010, AC-018, AC-019, AC-020, AC-028, AC-030]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6, FIND-TASK-003-7, FIND-TASK-003-8, FIND-TASK-003-9, FIND-TASK-003-10, FIND-TASK-003-11]
---

# Close TASK-003 Binding and Activity Contract Gaps

## Authority and Immutable Subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Review verdict: `changes/active/verified-change-contract/review/TASK-003-r1/verdict.md`
- Validated findings: `changes/active/verified-change-contract/review/TASK-003-r1/findings-validation.md`
- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Reviewed candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`

Implement this remediation cumulatively on the reviewed candidate. A later
`$wyrd-task-review` reassesses the complete original base-to-new-candidate
range, not only the remediation diff.

## Outcome

TASK-003 is complete when registration enforces the approved authorization and
binding contracts, authentication always targets one exact owner and records
monotonic activity, the changed Rust follows repository structure and identity
rules, and the public/runtime behavior has the required real-boundary proof.

## Diagnoses and Required Corrections

### `FIND-TASK-003-1` — Operator-bearing registration is under-authorized

Registration currently evaluates only `cards:write`, then freezes inline or
referenced `on_failure` Operators for later SYSTEM execution. A caller with
`cards:write` but no `operators:invoke` can therefore create future outbound
actions, and the required authorization decision has no audit row.

Reuse the existing Cards-route authorization and canonical transactional audit
mechanisms. After the registration request has been validated enough to know
whether any effective binding has a non-empty `on_failure`, evaluate
`operators:invoke` exactly once when needed and carry both allowed decisions
into the existing registration transaction. A denial must durably record the
decisions already evaluated while leaving no Card, principal, binding, or
operation row. Operator-free registration continues to spend only
`cards:write`; do not introduce another permission or audit path.

### `FIND-TASK-003-2` — Partial CardRef lookup can select the wrong principal

Migration 27 removes tenant-wide principal-name uniqueness to allow Card-bound
A/B versions, but the shared `service_account_by_card_ref` query still relies
on that uniqueness. An omitted `space` may match same-kind/name/version Cards
in several spaces, and the query silently selects the oldest row. API-key
issuance, workload `jwt-bearer`, and CardRef-targeted delegation all consume
this lookup, so they can mint authority and activity for the wrong owner.

Keep the A/B relaxation and repair the shared lookup once: return a principal
only when the supplied CardRef predicate has exactly one active match. An
explicit-space or UID-bearing reference remains exact; an ambiguous partial
reference follows the callers' existing not-found/refusal behavior. Do not
invent a default space, restore tenant-wide name uniqueness, require a new
public field, or add sibling lookup APIs.

### `FIND-TASK-003-3` — Concurrent exchanges can move activity backward

The issuance path captures server time before the SQL update. Concurrent
transactions can therefore commit out of timestamp order, allowing an older
timestamp to overwrite a newer `last_authenticated_at` and shorten the shared
eligibility window.

Make the existing Postgres update retain the later of the stored and supplied
timestamps. Preserve its returned owner UID, inactive/Card-free no-op behavior,
transaction coupling, and null-cursor-only arming. Do not add locks, an
activity table, or an application-side read/compare/write cycle.

### `FIND-TASK-003-4` — A registered schedule may have no future occurrence

Registration checks cron syntax but not whether the schedule can ever produce
a future cursor. For example, `0 0 30 2 *` parses but later arming logs
`NoOccurrence` and leaves the binding permanently inert.

In the existing pre-write effective-binding validation, reuse the parsed
schedule and its `next_after` operation with the server clock. Map absence of a
future occurrence through the existing invalid-card-spec refusal. Preserve
timezone semantics and the first-exchange cursor calculation; do not add a
second parser or validation layer.

### `FIND-TASK-003-5` — Owner occurrence identity is nullable

REQ-104 requires one total `subject_occurrence_key` domain: a reserved owner
value for Service-level and standalone-Agent bindings, and component aliases
for component bindings. The candidate instead persists SQL `NULL` and exposes
`Option<String>`, forcing downstream null special cases and leaving the
reserved-value boundary unenforced.

Define one internal non-null owner-occurrence constant in the binding owner,
make the persisted key non-null, and project it for Service and Agent owner
bindings. Keep component aliases unchanged and extend existing composition
validation to reject the reserved value as an alias before writes. Do not add
a public binding name or second identity column.

### `FIND-TASK-003-6` — Verification SQL APIs expose raw identity UUIDs

The activity writer accepts a raw `Uuid`, and `BindingActivity` returns raw
UUIDs for binding, Card, and principal identities even though the repository
already owns `BindingId`, `CardUid`, and `PrincipalId`. This permits domain
mix-ups and bypasses `BindingId` UUIDv7 validation.

Use those existing domain types in public query signatures and results. Where
SQLx needs raw decoding, keep it private and convert through existing
constructors before returning. Do not add new wrappers or a generic ID layer.

### `FIND-TASK-003-7` — Binding freeze/projection lacks a concrete owner

The new `owner_bindings` path is a dependency-backed, multi-step workflow: it
owns a tenant transaction dependency, walks binding sites, performs registry
IO, freezes identities, and assembles projection inputs. Leaving it as a free
function violates the repository's struct-centered Rust rule.

Give only this workflow one private transaction-scoped concrete owner in the
existing Cards service module, owning `&mut TenantConn` and exposing the
freeze-and-project operation. Keep pure digest/UID helpers and narrow
`wyrd-sql` query functions as they are. Do not introduce a trait, repository
layer, factory, or public abstraction.

### `FIND-TASK-003-8` — Runtime-activity journey evidence is incomplete

Direct issuer and SQL tests cover several branches, but the assembled-server
test drives only API-key exchange. It does not prove the real workload
`jwt-bearer` route or the existing excluded, expiry, lifecycle, A/B, and
shared-replica paths required by AC-019.

Extend the existing repository-managed auth/Card journey. Drive both API-key
and configured workload `jwt-bearer` exchange, request-driven stale-token
re-exchange, idle expiry, and each existing excluded client/server path named
by AC-019. Assert exact-owner timestamps, cursor stability/no-backfill,
lifecycle gating, A/B independence, shared-principal replica behavior, and no
activity writes for exclusions. Keep direct issuer/SQL tests as supporting
coverage. The nonexistent TASK-004 SYSTEM mint path is not part of this task.

### `FIND-TASK-003-9` — SDK journeys do not prove binding status

The raw server route test indexes JSON directly and cannot prove shared-client
deserialization or Rust, Python, and TypeScript exposure of
`card.status.verification.binding_ids`.

Extend the existing Card journeys for all three first-class SDKs. Through each
surface's current file/bundle registration path, register the smallest bound
Service or Agent, read it through that SDK, and assert UUIDv7 binding IDs remain
stable after reapply. Reuse existing authorization and tenant-isolation journey
mechanics where exposed. Do not create a new harness, client, endpoint, or
status engine.

### `FIND-TASK-003-10` — Served OpenAPI proof omits the status field

The generated JSON Schemas contain the field, but the assembled OpenAPI test
only changes the number of Card-spec alternatives. It does not prove the
runtime document's `Card -> Status -> verification -> binding_ids` chain.

Extend the existing served-document Card contract test to assert the reference
chain and UUID-array shape. Keep runtime OpenAPI and generated schema ownership
separate; do not add a schema copy or another harness.

### `FIND-TASK-003-11` — Mandatory Rust documentation is incomplete

The candidate adds or materially changes Rust SQL constants, associated items,
helpers, tests, and panic-capable test paths without the substantive rustdoc
required by repository authority.

Audit the complete candidate diff and document every added or materially
modified Rust item with its purpose and invariants, plus accurate `# Errors`,
`# Panics`, cancellation, or partial-progress sections where applicable.
Documentation must not alter behavior or add wrappers or lint suppressions.

## Constraints and Preserved Behavior

- Preserve the caller-owned registration and issuance transactions and forced
  RLS through `TenantConn`; no callee commits.
- Preserve stable UUIDv7 binding IDs under exact owner, occurrence, and
  Verifier identity; reapply and reorder keep IDs.
- Preserve exact Trigger/Operator UID freezing and inline canonical digests.
- Preserve five-minute permission snapshots, request-driven token re-exchange,
  current lifecycle checks at binding admission, and the default 86,400-second
  inactivity window.
- Preserve armed schedule cursors on renewal and do not backfill missed work.
- Preserve existing Card read authorization, tenant isolation, authored-spec
  immutability, and server-derived status.
- Do not add a heartbeat, activation endpoint, background refresh, per-request
  touch, per-observation write, activity table, Vala control store, scheduler,
  SYSTEM mint path, second binding identity, compatibility alias, or new test
  harness.

## Acceptance Criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-003-1` | Operator-free registration needs only `cards:write`; inline or referenced `on_failure` additionally requires and transactionally audits exactly one `operators:invoke` decision, and denial leaves no registration writes. |
| `FIND-TASK-003-2` | Explicit exact CardRefs resolve their own principal; an ambiguous partial CardRef fails closed for API-key issuance, workload `jwt-bearer`, and delegation without activity changes. |
| `FIND-TASK-003-3` | Out-of-order successful exchanges cannot reduce `last_authenticated_at` or move an armed cursor. |
| `FIND-TASK-003-4` | A parseable schedule with no future occurrence is refused before any registration write. |
| `FIND-TASK-003-5` | Every persisted binding has a non-null occurrence key; owner and component domains cannot collide; stable identity behavior is unchanged. |
| `FIND-TASK-003-6` | Verification query boundaries accept and return existing identity newtypes and reject an invalid persisted binding UUID version. |
| `FIND-TASK-003-7` | Binding freeze/projection is discoverable through one cohesive transaction-scoped concrete owner without new public abstraction. |
| `FIND-TASK-003-8` | The real-client/server activity journey proves both qualifying grants and every existing exclusion/lifecycle condition required by AC-019. |
| `FIND-TASK-003-9` | Existing Rust, Python, and TypeScript Card journeys observe stable UUIDv7 binding IDs through their public Card envelopes. |
| `FIND-TASK-003-10` | Served `/openapi.json` proves the nested verification status and UUID-array schema. |
| `FIND-TASK-003-11` | Every added or materially modified Rust item satisfies the repository rustdoc contract, and format/lints pass without suppression. |

## Focused and Broader Proof

Add or extend the smallest existing tests that directly exercise each gap:

- registration authorization/audit cases for no Operator, inline Operator,
  referenced Operator, denial rollback, and exact audit cardinality;
- same-kind/name/version principals in two spaces across API-key, workload JWT,
  and delegation lookup;
- reverse-ordered activity timestamps with cursor stability;
- impossible future cron refusal and zero writes;
- non-null owner occurrence, reserved-alias refusal, and stable projection;
- typed activity decoding including invalid UUIDv7 rejection;
- the existing projection/rollback/freeze tests through the concrete owner;
- the extended assembled auth/Card activity journey;
- existing Rust, Python, and TypeScript Card journeys asserting binding status;
- the exact served OpenAPI Card-contract test.

Record and run every newly named test with its exact repository-native selector.
Then run the narrowest owning lanes from the original task, including:

```bash
mise run test:cards:integration
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:wyrd
mise run test:cli:journey
mise run test:wyrdstate:journey
mise run test:platform:journey
mise run check:tenant-isolation
mise run check:registry-tx-coupling
mise run check:from-pools-allowlist
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

Also run the existing Rust, Python, and TypeScript SDK Card integration tasks
that own the extended journeys. Do not invoke the nonexistent `test:e2e` task;
record the actual repository-managed journey tasks used.

## Implementation Evidence

Candidate range: `467ea07d9..HEAD` on `verified-change-contract` (remediation
commits `209532bd4` through `5d8fe5f5a`). Named Postgres tests ran through
`WYRD_REG_E2E=1 mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p <crate> --test <target> -E 'test(=<name>)'`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-003-1` Operator-bearing registration needs and audits `operators:invoke`; denial leaves no writes | `cards/routes.rs::register_card_http` (`dispatches_operators`, `authorize_recording_denial`, `record_allowed`); `cards/service.rs::{dispatches_operators, record_allowed, register_card, write_registration}` | `wyrd-server --test pg_card_registration_route test(=operator_bearing_registration_requires_operator_invoke)`; `mise run test:cards:integration` | PASS |
| `FIND-TASK-003-2` exact CardRefs resolve their own principal; ambiguous partial refs fail closed for API-key issuance, workload `jwt-bearer`, delegation | `wyrd-sql/src/queries/auth/service_accounts.rs::service_account_by_card_ref` (`LIMIT 2`, single-match only); shared by `issue_api_key.rs`, `jwt_bearer.rs`, `exchange_api_key.rs::resolve_requested_subject` | `pg_card_registration_route test(=same_card_in_two_spaces_resolves_only_exact_refs)` (issue-key + delegation, omitted space → 404, no activity); `identity_e2e test(=workload_jwt_bearer_activates_only_its_exact_owner_keycloak)` via `mise run test:identity:journey` (config always pins space, so only the exact path is reachable); `wyrd-sql --lib test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)` | PASS |
| `FIND-TASK-003-3` out-of-order exchanges cannot reduce activity or move an armed cursor | `verification.rs::RECORD_AUTHENTICATION_SQL` (`GREATEST`) | `wyrd-sql --test pg_verification_bindings test(=out_of_order_exchanges_never_move_activity_backward)` | PASS |
| `FIND-TASK-003-4` parseable schedule with no future occurrence refused before writes | `cards/resolve.rs::validate_binding` (`BindingSchedule::parse(..).and_then(next_after(now))`) | `pg_card_registration_route test(=owner_status_serves_stable_binding_ids_and_exchange_activates)` (`0 0 30 2 *` → 400, `assert_no_registration_writes`) | PASS |
| `FIND-TASK-003-5` non-null occurrence key; owner/component domains cannot collide; identity stable | migration `20260601000027` (`subject_occurrence_key NOT NULL`, owner-kind CHECK); `wyrd-spec card/verifier.rs::OWNER_OCCURRENCE_KEY`; `graph/composition.rs::validate_composition` | `wyrd-spec --lib test(=graph::composition::tests::rejects_reserved_owner_occurrence_alias)`; `pg_verification_bindings test(=agent_owner_occurrence_is_the_reserved_key)`; reserved-alias refusal in `owner_status_serves_stable_binding_ids_and_exchange_activates` | PASS |
| `FIND-TASK-003-6` query boundaries use identity newtypes; invalid stored UUID version rejected | `verification.rs::{record_machine_authentication(PrincipalId), BindingActivity, BindingActivityRow::into_activity, stored_binding_id}` | `pg_verification_bindings test(=non_v7_stored_identities_are_refused)`; `mise run test:sql` | PASS |
| `FIND-TASK-003-7` freeze/projection owned by one transaction-scoped concrete struct | `cards/service.rs::BindingProjector { conn }` (`new`, `project`, `freeze`), called from `persist_node` | existing projection/freeze/rollback route tests via `mise run test:cards:integration`; `mise run lints` | PASS |
| `FIND-TASK-003-8` real client/server activity journey for both grants and every AC-019 exclusion/lifecycle condition | `pg_card_registration_route.rs::runtime_activity_follows_only_qualifying_exchanges` (real `AuthMiddleware` over a bound server, 33s TTL); `identity_e2e.rs::{workload_jwt_bearer_activates_only_its_exact_owner_keycloak, human_oidc_login_journey}`; `wyrd-testing server.rs::last_authenticated_at`; `sdks/wyrd-sdk-rust/tests/observe_run.rs` | `pg_card_registration_route test(=runtime_activity_follows_only_qualifying_exchanges)`: exact owner + component inheritance, A/B independence, shared replica, cached/ordinary request, idle stale client, stale re-exchange renews without cursor move, delegation, Card-free automation, idle expiry at +86,401s, suspension, deletion. `mise run test:identity:journey` (21/21): workload activation + renewal, human login/refresh/delegation record none. `mise run test:bifrost:journey:observe`: observations write no activity | PASS |
| `FIND-TASK-003-9` Rust, Python, TypeScript Card journeys observe stable UUIDv7 binding IDs | `sdks/wyrd-sdk-rust/tests/cards_state.rs::assert_stable_binding_ids`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts` (bound Agent); `sdks/wyrd-sdk-python/tests/integration/state/test_state_journey.py::test_bound_service_serves_stable_uuid7_binding_ids` | `mise run test:wyrdstate:journey` (Rust `cards_state`); `mise run ts:test:integration` (18/18); `mise run py:test:wyrdstate:integration` (8/8) | PASS |
| `FIND-TASK-003-10` served `/openapi.json` proves nested verification status and UUID array | `pg_openapi_contract.rs::{resolve_schema, card_contract_publishes_typed_lifecycle_and_problem_shapes}` | `wyrd-server --test pg_openapi_contract test(=card_contract_publishes_typed_lifecycle_and_problem_shapes)`; `mise run test:principals:integration` | PASS |
| `FIND-TASK-003-11` every added/modified Rust item has rustdoc incl. `# Errors`/`# Panics`; fmt/lints pass without suppression | base-to-HEAD audit of items, fields, `# Errors`, `# Panics` (`ids.rs` `BindingId::Err`/`from_str`/`deserialize`, `persist_node`, issuance pg tests, route/identity/SDK helpers) | `mise run fmt`; `mise run lints`; no `#[allow]` added | PASS |

Lanes run on the final candidate: `mise run fmt`, `lints`, `codegen:check`,
`check:tenant-isolation`, `check:registry-tx-coupling`,
`check:from-pools-allowlist`, `test:cards:integration`, `test:principals:unit`,
`test:principals:integration`, `test:sql`, `test:wyrd` (2053 passed),
`test:cli:journey`, `test:wyrdstate:journey`, `test:platform:journey`,
`test:identity:journey`, `test:bifrost:journey:observe`, `ts:test:integration`,
`py:test:wyrdstate:integration`, `py:test:cards:integration`, `py:lints`, and
`git diff --check` against the original base — all exit 0. The first
`test:sql`/`test:wyrd` run failed on a unit test that pinned the old
`ORDER BY … LIMIT 1` lookup text; that test now pins the fail-closed contract
(`5d8fe5f5a`), and both lanes were re-run green.

Non-goals confirmed excluded: no heartbeat, activation endpoint, background
refresh, per-request touch, per-observation write, activity table, Vala
control store, scheduler, SYSTEM mint path, second binding identity,
compatibility alias, or new test harness. The only harness addition is one
read helper on the existing `WyrdTestServer`, next to `table_describe_count`.
The 4s sleep in the activity journey lets a real token go stale (with the same
precedent as `ttl_expiry_journey`); it is not used for synchronization.

Limits: an ambiguous partial ref cannot reach workload `jwt-bearer`, because
`[[workload_bindings]]` requires `space`. That path is covered by the shared
lookup fix plus the exact-selection Keycloak journey. Scheduler admission of
closed gates is proven through `binding_activity`, the admission read TASK-004
consumes.
