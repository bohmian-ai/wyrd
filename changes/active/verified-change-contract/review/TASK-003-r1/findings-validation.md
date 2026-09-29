# TASK-003 Wave-2 Findings Validation

## Immutable Subject

- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Candidate `HEAD` before validation: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- CodeGraph: unavailable because `.codegraph/` is absent

The complete base-to-candidate diff, all five Wave-1 reports, governing repository authorities, and every production caller of a function implicated by a proposed correction were inspected. Corrections below stop at the first existing owner or mechanism that satisfies the approved behavior. No new product, public API, security model, concurrency model, or persistent-data decision is required.

## Wave-1 Finding Dispositions

| Wave-1 source ID | Disposition | Validated result |
|---|---|---|
| `TASKREV-001` | CONFIRMED | Retained as `FIND-TASK-003-1` with `SEC-AUTHZ-001`. |
| `TASKREV-002` | REVISED | Retained as `FIND-TASK-003-2` with `STD-003-001`, `SEC-AUTH-001`, and `DATA-001`. The correction is fail-closed ambiguity handling; no authority defines a credential-boundary default space. |
| `TASKREV-003` | CONFIRMED | Retained as `FIND-TASK-003-5`. |
| `TASKREV-004` | REVISED | Retained as `FIND-TASK-003-8` with `SEC-VER-001`. The nonexistent SYSTEM mint path is excluded; all existing qualifying and excluded paths required by AC-019 remain in scope. |
| `STD-003-001` | REVISED | Deduplicated into `FIND-TASK-003-2`; delegation is also a real caller of the same shared lookup and must remain fail-closed. |
| `STD-003-002` | REVISED | Retained as `FIND-TASK-003-7` only for the new dependency-backed binding-freeze/projection workflow. One-step SQL query primitives are not independently wrapped merely for uniformity. |
| `STD-003-003` | CONFIRMED | Retained as `FIND-TASK-003-6`. |
| `STD-003-004` | REVISED | Retained as `FIND-TASK-003-11`; the correction covers every added or materially modified item in the candidate, not only the examples enumerated in Wave 1. |
| `STD-003-005` | REVISED | Deduplicated with `CONTRACT-002` into `FIND-TASK-003-9`; extend existing SDK journey owners rather than add another harness. |
| `CONTRACT-001` | CONFIRMED | Retained as `FIND-TASK-003-4`. |
| `CONTRACT-002` | REVISED | Deduplicated with `STD-003-005` into `FIND-TASK-003-9`. |
| `CONTRACT-003` | CONFIRMED | Retained as `FIND-TASK-003-10`. |
| `SEC-AUTH-001` | REVISED | Deduplicated into `FIND-TASK-003-2`; the shared root cause covers API-key issuance, workload `jwt-bearer`, and CardRef-targeted delegation. |
| `SEC-AUTHZ-001` | CONFIRMED | Deduplicated into `FIND-TASK-003-1`. |
| `SEC-VER-001` | REVISED | Deduplicated into `FIND-TASK-003-8`; direct issuer tests remain supporting coverage, not journey substitutes. |
| `DATA-001` | REVISED | Deduplicated into `FIND-TASK-003-2`; the proposed “canonical space rule” is rejected because none exists at these credential boundaries. |
| `DATA-002` | CONFIRMED | Retained as `FIND-TASK-003-3`. |

## Validated Finding Ledger

### FIND-TASK-003-1

- **Wave-1 IDs:** `TASKREV-001`, `SEC-AUTHZ-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT / AUTHORIZATION
- **Obligation:** REQ-145, INV-007, AC-030, and the repository audit rules require `cards:write` for registration plus `operators:invoke` whenever any effective binding has a non-empty `on_failure`; every evaluated allow or deny must use the canonical transactional audit path.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:385-405,727-741`; `crates/wyrd/wyrd-server/src/components/cards/service.rs:545-575,999-1027,1120-1125`.
- **Evidence:** `register_card_http` has one caller through the Cards router and evaluates only `Permission::card_write()`. `register_card` carries only that one allowed event, while `persist_node` freezes and persists inline and referenced Operators. Repository-wide search finds no `Permission::operator_invoke()` evaluation in the registration path. The existing success fixture uses builtin `writer`, which already has both permissions, and the denial fixture has neither, so neither distinguishes the required split.
- **Consequence:** a custom role with `cards:write` but without `operators:invoke` can persist an Operator dispatch template that a later SYSTEM worker executes without another end-user authorization decision; the required allow/deny audit evidence is absent.
- **Decision-complete minimal correction:** reuse the existing `audit::authorize_recording_denial`, `AuditEvent`, and `audit::append_on` mechanisms. The Cards route must detect whether the validated registration contains any `on_failure`, evaluate `operators:invoke` exactly once when it does, and carry both allowed events into the existing registration transaction. A refusal must durably record every decision already evaluated and persist no registration rows. Operator-free registration continues to evaluate only `cards:write`; do not add a new permission, endpoint, or audit writer.
- **Adjacent behavior preserved:** existing idempotency replay/lost-race audit cardinality, reference resolution, registration transaction ownership, frozen Operator identity, and SYSTEM execution without reauthorization remain unchanged.
- **Focused closure proof:** a real registration-route test with a custom `cards:write`-only role proves an Operator-free binding succeeds, inline and referenced `on_failure` bindings return the stable 403 and write no Card/principal/binding/operation rows, and a role with both permissions commits exactly one allowed row per evaluated permission with the registration. Assert the corresponding denied rows as well.

### FIND-TASK-003-2

- **Wave-1 IDs:** `TASKREV-002`, `STD-003-001`, `SEC-AUTH-001`, `DATA-001`
- **Status:** REVISED
- **Classification:** REGRESSION / SECURITY
- **Obligation:** REQ-105, REQ-106, REQ-108, REQ-112, and the security posture require a credential or workload assertion to resolve one exact Card-bound principal and require ambiguity to fail closed.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:19-34`; `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:11-39,175-200`; callers at `crates/wyrd/wyrd-auth/src/issue_api_key.rs:86-137`, `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:141-159`, and `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:350-366`.
- **Evidence:** the migration drops the tenant-wide name uniqueness that the lookup's own invariant comment says bounds `card_ref @> $3` to one row. A CardRef omitting `space` can now match identical kind/name/version principals in several spaces, and `ORDER BY created_at, id LIMIT 1` selects the oldest. All three production callers use the same function: API-key issuance inserts a credential for that row, workload `jwt-bearer` authenticates as it, and CardRef-targeted delegation selects it.
- **Consequence:** credential issuance, workload authentication, or delegation can mint authority for and record activity on a sibling-space Card rather than the requested identity, with behavior determined by creation order.
- **Decision-complete minimal correction:** keep the A/B-version uniqueness relaxation, but change the existing shared `service_account_by_card_ref` query so it returns a row only when the supplied predicate has exactly one active match. An explicit-space CardRef remains exact; an omitted-space CardRef matching more than one row returns no principal through the callers' existing not-found/refusal paths. Remove the oldest-row fallback as an identity selector. Do not invent a default space, require a new public field, restore tenant-wide name uniqueness, or add a second lookup API.
- **Adjacent behavior preserved:** UID-bearing and explicit-space lookups, active-status filtering, tenant RLS, API-key hashing/audit, workload assertion verification, delegation permission narrowing, and distinct A/B versions remain unchanged.
- **Focused closure proof:** one Postgres/auth regression creates same-kind/name/version Card-bound principals in two spaces. Prove each explicit-space API-key, workload-`jwt-bearer`, and delegation lookup selects only its own principal; prove the omitted-space form issues no credential/token/delegation and updates neither principal's activity.

### FIND-TASK-003-3

- **Wave-1 IDs:** `DATA-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT / CONCURRENCY
- **Obligation:** REQ-105-108 require `last_authenticated_at` to represent the latest successful qualifying exchange and replicas sharing one principal to share correct activity.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/verification.rs:49-57,317-350`; caller `crates/wyrd/wyrd-auth/src/issuance.rs:247-377`.
- **Evidence:** `TenantTokenIssuer::issue` captures `issued_at = Utc::now()` before calling the only production caller, `record_machine_authentication`. The SQL unconditionally assigns `$2`. Row locking serializes writes but cannot order caller-supplied timestamps, so an older timestamp from a transaction that completes later overwrites a newer committed timestamp. Existing tests call the function sequentially in timestamp order.
- **Consequence:** a successful exchange can move activity backward, shorten the eligibility window, and make all replicas and component bindings expire early.
- **Decision-complete minimal correction:** make the existing `RECORD_AUTHENTICATION_SQL` update monotonic in Postgres by retaining the later of stored `last_authenticated_at` and the supplied server time. Keep the current returned owner UID and null-cursor-only arming logic; no lock, activity table, or application-side read/compare/write is needed.
- **Adjacent behavior preserved:** server-clock capture, transaction coupling with token issuance, inactive/Card-free no-op behavior, and the rule that renewal never moves an armed cursor remain unchanged.
- **Focused closure proof:** a focused Postgres test applies a newer and then older qualifying timestamp in reverse completion order, preferably from two tenant transactions, and proves the newer value remains, the full inactivity window is retained, and `next_run_at` is unchanged.

### FIND-TASK-003-4

- **Wave-1 IDs:** `CONTRACT-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Obligation:** TASK-003 scenarios 1-2, REQ-112, and AC-018 require registration to refuse a schedule that cannot produce the future cursor the first qualifying exchange must initialize.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:169-241`; `crates/wyrd/wyrd-sql/src/queries/verification.rs:136-182,317-350`; test claim at `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2840-2857`.
- **Evidence:** the only registration caller invokes `BindingSchedule::parse` but never `BindingSchedule::next_after`. `parse("0 0 30 2 *", None)` succeeds, while the complete `next_after` body and its unit test prove `ScheduleError::NoOccurrence`. The only later caller catches that error, logs it, and leaves `next_run_at` null. The public route test covers malformed syntax only.
- **Consequence:** registration succeeds and exposes a stable binding ID for a subscription that can never arm or create scheduled work.
- **Decision-complete minimal correction:** in the existing `EffectiveSpecs::validate_binding` pre-write validation, reuse the same parsed `BindingSchedule` and call `next_after` with the server clock to prove at least one future occurrence. Map failure through the existing invalid-card-spec refusal. Do not add another parser or validation layer.
- **Adjacent behavior preserved:** activation/Verifier compatibility, timezone semantics, stored schedule freezing, first-exchange cursor calculation, and malformed-cron refusal remain unchanged.
- **Focused closure proof:** extend the existing registration-route case with `0 0 30 2 *`; assert the stable 400 code and no Card, principal, binding, or operation write, while retaining the malformed-cron case and the schedule arithmetic unit test.

### FIND-TASK-003-5

- **Wave-1 IDs:** `TASKREV-003`
- **Status:** CONFIRMED
- **Classification:** INCORRECT / CONTRACT
- **Obligation:** REQ-104 fixes the natural key as `(tenant, owner_card_uid, subject_occurrence_key, verifier_uid)`, with a reserved owner value for Service-level and standalone-Agent occurrences and the component alias for component occurrences.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:39-43,51-83`; `crates/wyrd/wyrd-sql/src/queries/verification.rs:212-280`; `crates/wyrd/wyrd-server/src/components/cards/service.rs:1147-1203`.
- **Evidence:** the column is nullable, the constraint uses `UNIQUE NULLS NOT DISTINCT`, `NewBinding` represents the key as `Option<String>`, and `owner_bindings` supplies `None` for owner occurrences. Tests intentionally construct owner bindings with `None`. These are the only projection callers and representations.
- **Consequence:** durable rows do not implement the approved total occurrence-key domain, downstream consumers must special-case SQL null, and the reserved-owner/component-alias boundary is not enforced.
- **Decision-complete minimal correction:** define one internal non-null owner-occurrence constant in the binding owner, make `subject_occurrence_key` non-null, project that value for Service-level and standalone-Agent sites, and keep component aliases unchanged. Reuse the existing composition validation to reject the reserved value as a component alias before writes, so the two domains cannot collide. Update the natural-key constraint and existing projection fixtures; do not add a second identity column or public binding name.
- **Adjacent behavior preserved:** stable UUIDv7 reuse, component aliases, exact owner/Verifier UID identity, ordering independence, and no authored binding identifier remain unchanged.
- **Focused closure proof:** the existing projection identity test must assert the persisted non-null owner value for Service and Agent owners, unchanged aliases for components, stable IDs on reapply/reorder, and refusal of a component alias equal to the reserved value before any write.

### FIND-TASK-003-6

- **Wave-1 IDs:** `STD-003-003`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / TYPED IDENTITY
- **Obligation:** `AGENTS.md` section 4 and `rust-core.md` require domain newtypes at durable identity API boundaries.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/verification.rs:317-321,381-423`.
- **Evidence:** the production activity writer accepts raw `Uuid` although its sole production caller has a principal identity domain, and public `BindingActivity` returns raw UUIDs for binding, Card, and principal identities despite existing `BindingId`, `CardUid`, and `PrincipalId` types. `binding_activity` is explicitly required by TASK-003 even though its runtime consumer lands later.
- **Consequence:** callers can interchange identity domains, and `BindingActivity.binding_id` bypasses the UUIDv7 validation that `owner_binding_ids` already enforces.
- **Decision-complete minimal correction:** use the existing `PrincipalId`, `BindingId`, and `CardUid` types in the public signatures and result. Decode SQL into a private raw row only where SQLx requires it, then convert through the existing constructors before returning. Do not add wrapper types or a generic ID abstraction.
- **Adjacent behavior preserved:** SQL column types, RLS, query predicates, caller-owned transactions, and the public wire representation of UUID-backed IDs remain unchanged.
- **Focused closure proof:** focused SQL tests compile against the typed signatures, assert normal rows decode to the correct domains, and prove a non-v7 stored binding UUID is refused rather than returned as a valid `BindingActivity`.

### FIND-TASK-003-7

- **Wave-1 IDs:** `STD-003-002`
- **Status:** REVISED
- **Classification:** VIOLATION / STRUCTURAL OWNERSHIP
- **Obligation:** `AGENTS.md` section 5, `architecture/agent-rules.md`, and `rust-core.md` make dependency-backed multi-step Rust workflows methods on one cohesive concrete owner.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/service.rs:1073-1131,1133-1203`.
- **Evidence:** the newly added `owner_bindings` is called only from `persist_node`, accepts the transaction dependency, walks multiple binding sites, performs registry IO, canonicalizes inline targets, freezes referenced identities, and assembles projection inputs. Its full body is a dependency-backed workflow, not a deterministic helper. The Wave-1 proposal's broader objection to every free function in `queries::verification` is rejected: narrow SQL query operations may remain direct functions, and wrapping them solely for uniformity would add indirection.
- **Consequence:** the new registration capability has no discoverable concrete owner and continues the dependency-threaded workflow shape the repository explicitly makes incomplete for materially modified Rust.
- **Decision-complete minimal correction:** replace the one free binding-freeze/projection path with one private, transaction-scoped concrete owner in the existing Cards service module that owns `&mut TenantConn` and exposes one method to freeze and project an owner Card. Keep `pinned_uid` and `inline_digest` as pure helpers and keep the existing `wyrd-sql` query operations as the persistence mechanism. Do not introduce a trait, repository layer, factory, or public type.
- **Adjacent behavior preserved:** the caller-owned registration transaction, topo order, sibling Trigger visibility, registry error mapping, exact frozen targets, and SQL RLS boundary remain unchanged.
- **Focused closure proof:** existing registration projection, rollback, reapply/reorder, referenced Trigger, inline Operator, and tenant-isolation tests pass through the concrete owner; no new test harness is needed.

### FIND-TASK-003-8

- **Wave-1 IDs:** `TASKREV-004`, `SEC-VER-001`
- **Status:** REVISED
- **Classification:** MISSING VERIFICATION
- **Obligation:** TASK-003 scenarios 2-3, AC-019, AC-020, and the repository journey rules require a real client-to-server journey for the existing qualifying and excluded runtime-activity paths.
- **Exact location:** current lower-tier proof at `crates/wyrd/wyrd-auth/src/issuance.rs:933-1091` and `crates/wyrd/wyrd-sql/tests/pg_verification_bindings.rs:275-480`; partial raw-route proof at `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2749-3025`.
- **Evidence:** the assembled-server test drives one API-key exchange. Workload `jwt-bearer` is invoked only by calling `TenantTokenIssuer::issue` directly, bypassing assertion verification and its route transaction. Delegation, human refresh, Card-free automation, cached bearer traffic, idle expiry/no-refresh, ordinary observations, suspension/deletion, A/B versions, and shared replicas are either lower-tier only or absent from a client/server journey. The SYSTEM mint path does not yet exist and is therefore not a correction target for TASK-003.
- **Consequence:** routing, grant classification, transaction ownership, cached-token, and lifecycle regressions can leave the direct issuer and SQL tests green while activating or renewing the wrong owner at the real boundary.
- **Decision-complete minimal correction:** extend the existing repository-managed auth/Card journey rather than create a new harness. Drive both real API-key and configured workload-`jwt-bearer` exchange, request-driven stale-token re-exchange, idle expiry, and each existing excluded client/server path named by AC-019; assert exact-owner `last_authenticated_at`, cursor stability/no-backfill, lifecycle gating, A/B independence, shared-principal replica behavior, and no writes for exclusions. Keep the direct issuer and SQL tests as exhaustive supporting seams. Do not implement or fake the later SYSTEM mint path.
- **Adjacent behavior preserved:** five-minute permission snapshots, request-driven refresh only, no heartbeat/per-request touch/observation write, no scheduler implementation, and no cancellation of admitted work remain unchanged.
- **Focused closure proof:** run exact selectors for the extended real-server journey plus the existing issuer and SQL seam tests, followed by the owning journey and SQL/auth integration lanes recorded through current `mise` tasks.

### FIND-TASK-003-9

- **Wave-1 IDs:** `STD-003-005`, `CONTRACT-002`
- **Status:** REVISED
- **Classification:** MISSING VERIFICATION
- **Obligation:** TASK-003 scenario 4, AGENTS.md section 11, design doctrine 20, and the testing workflow require the new public `card.status.verification.binding_ids` behavior to be proven through the real SDK surfaces that expose Card envelopes.
- **Exact location:** only current proof at `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2695-3025`; public Rust owner `crates/shared/wyrd-client/src/cards/handle.rs:274-293`; existing journey owners `sdks/wyrd-sdk-rust/tests/cards_state.rs`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts`, and the Python state/Card integration journeys under `sdks/wyrd-sdk-python/tests/integration/`.
- **Evidence:** the candidate uses `WyrdTestServer::start_in_process`, raw Axum requests, and untyped JSON indexing. It never exercises shared-client deserialization or the Rust, TypeScript, and Python envelope/status projections. Existing SDK Card journeys already provide the required real bound server and clients but do not register a bound owner or assert the status field.
- **Consequence:** the HTTP handler can remain green while an SDK drops, mis-serializes, or fails to expose the nested UUIDv7 list.
- **Decision-complete minimal correction:** extend the existing Card journeys for the public surfaces that expose a complete Card envelope; register the smallest Service/Agent binding through each surface's current file/bundle path, read it through that same SDK, and assert typed/JSON-projected UUIDv7 IDs remain stable after reapply. Reuse existing under-privileged and tenant-isolation journey mechanics where that surface exposes them. Do not add another client, status engine, listing endpoint, or test harness.
- **Adjacent behavior preserved:** generic Card reads, offline hydration, authored spec immutability, current authorization errors, and SQL freeze-detail assertions remain in their existing owners.
- **Focused closure proof:** the exact existing Rust, Python, and TypeScript Card journey selectors assert `status.verification.binding_ids`; run their owning gated integration tasks.

### FIND-TASK-003-10

- **Wave-1 IDs:** `CONTRACT-003`
- **Status:** CONFIRMED
- **Classification:** MISSING VERIFICATION
- **Obligation:** AGENTS.md's OpenAPI rule, `testing-workflows.md`, and REQ-134 require the assembled `/openapi.json` document to prove the new public status shape.
- **Exact location:** `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:457-521`; contract owners `crates/wyrd-spec/src/envelope.rs:459-478` and `crates/wyrd-spec/src/card/verifier.rs:177-200`.
- **Evidence:** the only candidate OpenAPI assertion changes the number of `Spec` alternatives. It never traverses `Card -> Status -> verification -> VerificationStatus -> binding_ids`. JSON Schema snapshots are a separate generated surface and cannot prove assembled utoipa registration.
- **Consequence:** runtime machine-readable HTTP documentation can omit or mis-shape the only public binding locator while codegen and current OpenAPI tests stay green.
- **Decision-complete minimal correction:** extend the existing `card_contract_publishes_typed_lifecycle_and_problem_shapes` test to assert the assembled references and the UUID-array shape. Reuse `served_document`; add no schema copy or new harness.
- **Adjacent behavior preserved:** the 15-kind assertion, problem shapes, generated JSON Schema ownership, and runtime-only OpenAPI model remain unchanged.
- **Focused closure proof:** run the exact `card_contract_publishes_typed_lifecycle_and_problem_shapes` selector under the repository-managed Postgres setup, then `mise run test:principals:integration`.

### FIND-TASK-003-11

- **Wave-1 IDs:** `STD-003-004`
- **Status:** REVISED
- **Classification:** VIOLATION / DOCUMENTATION
- **Obligation:** AGENTS.md section 16, `architecture/agent-rules.md`, and `rust-core.md` make substantive rustdoc mandatory for every new or materially modified Rust item, including private constants, associated types, helpers, and tests, with `# Panics` where a panic remains.
- **Exact location:** at minimum `crates/wyrd/wyrd-sql/src/queries/verification.rs:30-90`; `crates/wyrd/wyrd-sql/tests/pg_verification_bindings.rs:30-481`; `crates/wyrd/wyrd-auth/src/issuance.rs:933-1091`; `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2632-2693`; and `crates/wyrd-spec/src/ids.rs:269-276`.
- **Evidence:** six new SQL constants have no item rustdoc; the new `FromStr::Err` associated type has none; new Postgres test helpers/tests and auth test helpers/tests contain `expect`, `unwrap`, assertions, or explicit panic paths without `# Panics`; `bound_service_request` reaches the existing panic-capable request builder without documenting it. The rule is an explicit hard blocker even though these items compile.
- **Consequence:** the candidate fails a repository completion criterion and leaves the new durable SQL and panic contracts undocumented for maintainers.
- **Decision-complete minimal correction:** document every added or materially modified item identified from the candidate diff, explaining purpose/invariant and adding accurate `# Errors`, `# Panics`, cancellation, or partial-progress sections where applicable. Documentation only; do not add wrappers, suppress lints, or alter behavior.
- **Adjacent behavior preserved:** all production logic, tests, visibility, and module boundaries remain unchanged.
- **Focused closure proof:** inspect the complete added/modified-item diff for documentation coverage, then run `mise run fmt` and `mise run lints`; no new runtime test is warranted.

## Specification Revision Decision

`SPEC_REVISION_REQUIRED` is not indicated. Every retained correction is already fixed by revision 33 or repository authority. The rejected correction fragments—choosing an implicit credential space, wrapping one-step SQL functions for symmetry, and fabricating a SYSTEM path before its owning task—would add unauthorized behavior or complexity and are not required to close TASK-003.
