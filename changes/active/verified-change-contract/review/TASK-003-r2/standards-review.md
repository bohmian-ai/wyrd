# TASK-003-R2 Repository Standards Review

## Review Findings

### Critical

None.

### Important

- **STD-003-R2-001 — `TenantConn` credential lookup retains a prohibited manual tenant predicate.**
  **Rule:** `architecture/agent-rules.md` (“TenantConn (Postgres RLS) is the load-bearing tenant boundary. Do not add manual per-query tenant filters”), `architecture/references/languages/rust-core.md` (“Do not add manual tenant predicates to a `TenantConn` query”), `architecture/references/languages/implementation-execution.md` (“rely on Postgres RLS rather than duplicating tenant predicates”), and `architecture/v1/00-foundations/{postgres-layout,sql-foundation}.md` require tenant query modules to rely on the transaction-bound `wyrd.current_tenant()` RLS policy rather than repeat tenancy in SQL.
  **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:26-34,184-195`.
  **Evidence:** the remediation materially changes `SERVICE_ACCOUNT_BY_CARD_REF_SQL` and `service_account_by_card_ref`, but the query still includes `WHERE data_tenant_id = $1` and the `TenantConn` callee still binds `conn.data_tenant_id()` before the actual lookup inputs. The table is already tenant-scoped through `TenantConn` and forced RLS. The original task independently names manual tenant predicates as prohibited and asks the refactor stage to remove redundant ones.
  **Consequence:** this credential-issuing lookup now has two tenant authorities in one tenant-scoped path, preserving exactly the drift-prone query shape the repository rule forbids. Future RLS or lookup changes can diverge between the duplicated predicate and the authoritative connection binding; passing `check:tenant-isolation` does not waive the source rule because that check currently verifies connection/RLS structure rather than banning redundant predicates.
  **Testable correction:** remove the `data_tenant_id` predicate and tenant bind from this materially modified lookup, renumber the remaining placeholders, and keep the existing `LIMIT 2`/single-match ambiguity refusal. Extend the existing SQL-text unit assertion to reject a manual tenant predicate, then rerun its exact test plus `mise run test:principals:unit`, `mise run test:principals:integration`, `mise run test:sql`, and `mise run check:tenant-isolation`.

- **STD-003-R2-002 — the cumulative candidate fails a required whitespace gate that its remediation record reports as passing.**
  **Rule:** `AGENTS.md` §§11–12, `architecture/references/languages/spec-driven-development.md` (TDD `VERIFY` and credible completion evidence), and `architecture/references/languages/implementation-execution.md` (“Focused verification” and “Completion standard”) require `git diff --check`, accurate verification evidence, and a clean final diff audit.
  **Location:** trailing whitespace in `changes/active/verified-change-contract/review/TASK-003-r1/standards-review.md:3-5,43-47,50-54,57-61,64-68,71-75`; contradictory claim in `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md:276-286` (specifically line 283).
  **Evidence:** independent execution of `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..449aceb346f5f5fbc27958d260bd9c0c50225466` exits `2` and reports 28 trailing-whitespace violations (56 diagnostic lines), all in the prior standards artifact. The remediation record says the same original-base diff command “all exit 0.”
  **Consequence:** the cumulative candidate does not meet its explicitly required completion gate, and the durable remediation evidence is not reproducible from the immutable subject. Review artifacts are part of this candidate and cannot be excluded from the complete diff audit.
  **Testable correction:** remove only the reported trailing spaces without changing the prior report’s conclusions, rerun the exact base-to-new-candidate `git diff --check`, and make the remediation evidence reflect the observed result.

### Suggestions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Prior implementation candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md`
- CodeGraph: unavailable because `.codegraph/` is absent

The candidate was at the pinned commit before inspection and before this report was written. This review covers repository-rule conformance only; it does not perform task acceptance or Ponytail validation.

## Authority Coverage

| Changed surface | Complete applicable authority read | Evidence inspected | Result |
|---|---|---|---|
| Wyrd contract additions (`BindingId`, owner occurrence key, verification status, composition validation) | `AGENTS.md` §§2–9, 15–16; `architecture/wyrd-design.md` doctrines 3, 8–10, 18, 20–21 and registry lifecycle; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/{positioning-and-vocabulary,architecture-constraints}.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors}.md`; `architecture/v1/06-crates/wyrd-spec.md`; approved spec REQ-090–108/112 | `crates/wyrd-spec/src/{ids.rs,envelope.rs,lib.rs,card/verifier.rs,graph/composition.rs}` and contract tests | PASS — pure, IO/async/PyO3-free contracts; UUIDv7 newtype; non-null collision-safe owner key; server-derived status; substantive rustdoc. |
| Generated JSON schema and served OpenAPI | `AGENTS.md` §§8, 9, 11–12; `architecture/agent-rules.md` generated-artifact rule; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{testing-workflows,errors}.md` | four JSON schema/golden files; `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:457-566`; recorded `codegen:check` | PASS — source and generated mirrors agree, and served OpenAPI now proves the nested UUID array. |
| Card registration authorization, audit, validation, projection, and status hydration | `AGENTS.md` §§5–6, 9, 16; `architecture/agent-rules.md` audit/transaction/structure rules; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/{permission-check,permission-model,security,service-identity}.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors}.md` | `crates/wyrd/wyrd-server/src/components/cards/{routes.rs,resolve.rs,service.rs}` and route/OpenAPI tests | PASS — Operator-bearing registration evaluates and records `operators:invoke`; impossible schedules fail pre-write; `BindingProjector` owns the dependency-backed freeze/project workflow; allowed decisions compose with registration. |
| Auth issuance and exact principal activity | `AGENTS.md` §§4–6, 9, 16; security posture principal/credential/authorization/audit sections; `architecture/v1/00-foundations/{service-identity,security,principal-extraction}.md`; `architecture/references/languages/rust-core.md` | `crates/wyrd/wyrd-auth/src/issuance.rs`; production API-key, workload, refresh, delegation callers; auth/identity tests | PASS — qualifying grants alone record typed-principal activity in the issuing transaction; exclusions remain gated by the grant enum. |
| Tenant SQL migration, principal lookup, binding projection, activity and admission reads | `AGENTS.md` §§4–6, 9, 15–16; `architecture/agent-rules.md` TenantConn/RLS/transaction/type/import rules; `architecture/wyrd-security-posture.md` tenant/data isolation; `architecture/v1/00-foundations/{postgres-layout,sql-foundation,tenancy,rbac-crud}.md`; `architecture/references/languages/rust-core.md` Postgres boundary | migration 27; `crates/wyrd/wyrd-sql/src/queries/{verification.rs,auth/service_accounts.rs,mod.rs}`; SQL tests; manifests/lock | **FAIL** — migration and verification queries use forced RLS, caller-owned `TenantConn`, typed identities, and no callee commit; the materially modified CardRef principal lookup still duplicates the tenant predicate (`STD-003-R2-001`). |
| Rust/Postgres/server test placement and documentation | `AGENTS.md` §§11, 16; `architecture/agent-rules.md` test placement/Postgres/rustdoc rules; `architecture/references/languages/{rust-core,testing-workflows}.md`; `TESTING.md` | `wyrd-auth` pg tests; `wyrd-sql/tests/pg_verification_bindings.rs`; `wyrd-server/tests/{identity_e2e,pg_card_registration_route,pg_openapi_contract}.rs`; `wyrd-testing/src/server.rs` | PASS — external targets earn placement through Postgres/server boundaries; added/materially modified Rust items inspected have workflow rustdoc and applicable `# Errors`/`# Panics`; no suppression was added. |
| First-class Rust, Python, and TypeScript SDK journeys | `AGENTS.md` §§2, 8, 11–12; `architecture/wyrd-design.md` doctrine 20/client model; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{testing-workflows,typescript-guide}.md`; `TESTING.md` | `sdks/wyrd-sdk-rust/tests/{cards_state,observe_run}.rs`; Python `test_state_journey.py`; TypeScript `cards-state.test.ts`; public SDK consumers | PASS — each language reads stable UUIDv7 binding IDs through its existing public Card journey; the Rust observation journey checks that ordinary observation writes do not renew activity. |
| Workspace dependencies and crate ownership | `AGENTS.md` §§3–4; `architecture/agent-rules.md` feature/dependency rules; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | workspace/crate `Cargo.toml`, `Cargo.lock`, dependency call sites | PASS — `croner` and `chrono-tz` are placed only in the SQL owner that parses/fixes persisted schedules; no feature, wildcard version, client-tier analytical dependency, or duplicate scheduler abstraction was introduced. |
| Task, prior review, remediation, and verification artifacts | `AGENTS.md` §§11–14, 16; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md`; `.agents/skills/wyrd-task-review/SKILL.md` | original task evidence; every `TASK-003-r1` report; remediation task and evidence; cumulative diff audit | **FAIL** — artifacts preserve the prior findings and mapped closure, but the cumulative required whitespace check is red while the remediation evidence records it green (`STD-003-R2-002`). The working `architecture/verifier-contract.md` is explicitly non-authoritative for this continuous Drift/Eval delivery; approved spec revision 33 and `wyrd-design.md` govern. |

## Applicable Rule Results

| Applicable repository rule | Exact candidate evidence | Result |
|---|---|---|
| `wyrd-spec` remains pure, typed, IO/async/PyO3-free | Contract-only changes in `wyrd-spec`; `BindingId` enforces UUIDv7; no runtime dependency added | PASS |
| Status is server-derived and authored specs remain declarative | `service.rs:93-108` loads binding IDs; `hydrate_card` overlays status; schema/OpenAPI project the same contract | PASS |
| Stateful dependency-backed workflow has a concrete owner | `service.rs:1179-1297` gives freeze/project to transaction-scoped `BindingProjector` | PASS |
| Async exists only around actual IO/composition | async changes await SQL, registry, server, or client IO; schedule parsing and value operations remain synchronous | PASS |
| Durable identities use domain newtypes | `verification.rs:332-465` accepts `PrincipalId` and returns `BindingId`, `CardUid`, `PrincipalId`; raw SQL rows convert before leaving the module | PASS |
| Tenant tables use composite tenant keys, forced RLS, and caller-owned `TenantConn` | migration lines 53–101; verification query functions accept `TenantConn` and do not commit | PASS |
| `TenantConn` queries do not duplicate tenant predicates | changed CardRef principal lookup uses both RLS and `data_tenant_id = $1` | **FAIL (`STD-003-R2-001`)** |
| Every authorization verdict uses the canonical audit path and fails closed | `routes.rs:404-430`; `service.rs:533-610,1019-1107` records denials immediately and allowed events either with registration or standalone when no write commits | PASS |
| Credential uncertainty and ambiguous identity fail closed | `service_accounts.rs:26-34,184-200` fetches at most two and accepts exactly one; explicit exact journeys cover spaces | PASS |
| Machine activity is monotonic and schedule arming never moves an armed cursor | `verification.rs:56-66,332-374` uses Postgres `GREATEST` and `next_run_at IS NULL`; reverse-order SQL proof exists | PASS |
| Public user-facing contract has real first-class client journeys | Rust, Python, and TypeScript existing Card journeys assert stable UUIDv7 binding IDs; activity/exclusion journeys exercise bound server/client seams | PASS |
| Generated artifacts are owner-derived and drift-checked | source contract matches all four JSON artifacts; remediation records `codegen:check`; served OpenAPI has independent runtime proof | PASS (recorded lane not rerun here) |
| New/material Rust items carry substantive rustdoc and error/panic contracts | complete added-item inspection across contract, auth, SQL, server, harness, and Rust journey files | PASS |
| Verification evidence is accurate and `git diff --check` passes | remediation line 283 claims success; independent immutable-range command exits 2 on 28 violations | **FAIL (`STD-003-R2-002`)** |
| No gate circumvention, legacy vocabulary, secret material, raw production pool, callee transaction close, or parallel public error path enters the diff | complete cumulative diff and surrounding-source inspection | PASS |

## Prior Finding Closure

| Prior finding | Closure evidence | Standards result |
|---|---|---|
| `FIND-TASK-003-1` Operator authorization/audit | route evaluates `Permission::operator_invoke()` for effective `on_failure`; allowed events enter the registration transaction; focused route proof covers denial/no writes/cardinality | CLOSED |
| `FIND-TASK-003-2` ambiguous CardRef principal resolution | shared lookup accepts exactly one of at most two matches; exact-space API-key/delegation and workload journeys cover consumers | CLOSED; `STD-003-R2-001` concerns duplicate tenancy enforcement, not ambiguity behavior |
| `FIND-TASK-003-3` monotonic activity | `GREATEST(last_authenticated_at, $2)` plus reverse-order Postgres proof | CLOSED |
| `FIND-TASK-003-4` never-firing schedules | registration validation calls `next_after(Utc::now())`; route proof asserts refusal/no writes | CLOSED |
| `FIND-TASK-003-5` total owner occurrence key | `$owner` constant, `NOT NULL`, collision-refusing composition validation, and Agent owner-row proof | CLOSED |
| `FIND-TASK-003-6` typed durable identities | verification API uses `BindingId`, `CardUid`, and `PrincipalId`; invalid stored UUID version is refused | CLOSED |
| `FIND-TASK-003-7` concrete workflow owner | transaction-scoped `BindingProjector` owns binding freeze/projection | CLOSED |
| `FIND-TASK-003-8` runtime-activity journeys | bound-client API-key lifecycle, configured workload JWT journey, identity/exclusion/lifecycle/A/B/replica/observation proofs are present and recorded green | CLOSED (recorded evidence inspected, not rerun in this slice) |
| `FIND-TASK-003-9` SDK Card status journeys | public Rust, Python, and TypeScript Card journeys observe stable UUIDv7 IDs | CLOSED (recorded lanes inspected, not rerun) |
| `FIND-TASK-003-10` served OpenAPI status chain | served-document test resolves `Card -> Status -> VerificationStatus -> binding_ids` and asserts UUID items | CLOSED |
| `FIND-TASK-003-11` mandatory rustdoc | SQL constants, identity conversions, projector, helpers, and added tests now carry substantive documentation and applicable sections | CLOSED |

## Open Questions

None. Both findings have bounded, repository-authorized corrections and require no product, public API, architecture, security, compatibility, concurrency, or persistent-data decision.

## Verification Notes

- Independently inspected the complete 38-file base-to-candidate diff, the remediation-only diff, surrounding production callers, migrations, tests, manifests, lockfile, generated schemas, public SDK consumers, task evidence, and all prior TASK-003 review artifacts.
- Read `AGENTS.md`, `architecture/agent-rules.md`, the reference router, and the complete applicable design, doctrine, security, patterns, Rust, testing, error, TypeScript, SQL/RLS, permission, service-identity, crate-boundary, workflow, task, and test authorities. `architecture/bifrost-design.md` and analytical domain references are not applicable because TASK-003 does not change Bifrost ingest/query/storage behavior; the changed observation journey only asserts absence of an activity side effect through an existing surface.
- Independently ran `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..449aceb346f5f5fbc27958d260bd9c0c50225466`: **FAIL**, exit 2, 28 trailing-whitespace violations in the committed prior standards report.
- The remediation artifact records the relevant format, lint, codegen, SQL, server, boundary, identity, Bifrost-observe, and first-class SDK lanes as passing. This review did not rerun those long Cargo/Postgres/language lanes; their source assertions and command mapping were inspected. The independently reproduced diff-check failure invalidates only that recorded lane claim.
- No source file was modified. HEAD remained `449aceb346f5f5fbc27958d260bd9c0c50225466` through source inspection; only this requested review report was added afterward.

## Overall Result

**FAIL** — all eleven prior implementation/standards findings are closed in the candidate code and coverage, but the materially modified CardRef lookup still violates the repository's explicit no-manual-tenant-predicate rule, and the cumulative candidate fails a required `git diff --check` gate that its remediation artifact reports as passing.
