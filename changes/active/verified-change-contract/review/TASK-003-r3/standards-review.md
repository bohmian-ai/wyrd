# TASK-003 R3 Repository Standards Review

**Immutable base:** `1609e102881dd55b12154248834a8aedbb7a36b2`
**Immutable candidate:** `a5a5b60f446981760ac831f64aba871582ec45e4`
**Scope:** complete cumulative repository-rule audit only; task acceptance and Ponytail validation are excluded.
**Overall result:** **FAIL**

## Material Source-Local Findings

### Critical

None.

### Important

- **STD-003-R3-001 — New Rust items violate the mandatory top-level-import and bare-type signature rules.**
  **Violated authority:** `architecture/agent-rules.md` requires every import to live in the module's top-of-file dependency block and requires imported bare type names in struct fields, function parameters, return types, trait bounds, and type aliases. The only function-local import exception is a narrowly scoped `use TraitName as _;`; test modules are separate scopes but their imports still belong at the top of that module.
  **Exact locations:** `crates/wyrd-spec/src/envelope.rs:476`; `crates/wyrd-spec/src/ids.rs:234,247,257,290-294`; `crates/wyrd/wyrd-testing/src/server.rs:1613-1616`; `crates/wyrd/wyrd-server/tests/identity_e2e.rs:517,555-558`; `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2791-2796,2811-2814,3481,3496-3499,3521-3524,3538-3541,3555-3560`.
  **Validated evidence:** the cumulative candidate adds a fully qualified `crate::card::verifier::VerificationStatus` struct field, defines the new `BindingId` field and API in terms of `uuid::Uuid`, gives new fixture and journey helpers fully qualified `chrono`, `uuid`, `axum`, `wyrd_spec`, and `wyrd_sql` types in signatures or aliases, and places `use wyrd_sql::queries::verification::{InactivityTimeout, binding_activity};` inside `owner_gates`. These are added items, not inherited untouched code. `mise run lints` passes because Clippy does not enforce this repository-specific source-shape rule.
  **Impact:** the cumulative candidate fails an explicit repository implementation boundary and hides dependencies outside the required module dependency manifests. This is source-shape drift rather than a runtime defect, but `architecture/agent-rules.md` is mandatory and leaves no test-only exemption for qualified signatures or function-local ordinary imports.
  **Minimum correction:** add the required types to each module's top-level `use` block and use their bare names in the listed new fields, aliases, parameters, and return types; move `InactivityTimeout` and `binding_activity` to the existing top-level `wyrd_sql::queries::verification` import in the route test. Preserve behavior and rerun `mise run fmt`, `mise run lints`, and the focused journeys whose files change.

### Suggestions

None.

## Authority Coverage

The review read the repository router and every applicable authority completely before judging the cumulative source:

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `architecture/references/README.md`
- `architecture/references/doctrine/positioning-and-vocabulary.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`

`architecture/bifrost-design.md` and the Bifrost/domain analytical reference slices are not applicable: this candidate does not change Bifrost ingest, query, admission, publication, maintenance, or analytical reliability behavior. PyO3/Python-boundary references are likewise not applicable; the Python change is an existing public journey assertion with no wrapper, export, stub, or runtime-boundary change.

| Changed surface | Applicable repository rule | Result | Evidence |
|---|---|---|---|
| Workspace dependencies and lockfile | Dependencies remain workspace-pinned, features must be earned, and specialized cost stays in the narrow owner | PASS | `chrono-tz` and `croner` are workspace dependencies consumed by `wyrd-sql`; no new Cargo feature or profile block was added. |
| `wyrd-spec` contract and identifiers | Pure/IO-free/PyO3-free foundation, domain identifiers, server-managed status, generated-contract alignment | **FAIL source shape only** | `BindingId`, `VerificationStatus`, and schema projection are correctly owned and typed, but new fields/signatures use qualified paths (`STD-003-R3-001`). |
| Card registration and binding projection | Durable orchestration remains server-owned, struct-centered, transactionally composed, and tenant-scoped | PASS | `BindingProjector` owns freeze/project work on the caller's `TenantConn`; `persist_node` composes Card, principal, bindings, and audit writes without callee commit/rollback. |
| Auth issuance and runtime activity | Exact verified principal, fail-closed ambiguity, qualifying-grant-only activity, monotonic timestamps, canonical audit | PASS | Shared CardRef lookup accepts exactly one active match; issuance callers retain it; `GREATEST` prevents backward activity; excluded flows remain no-ops. |
| Tenant SQL and migration | Forced RLS is the tenant authority; no raw pool in production library signatures; no manual tenant predicate on the remediated `TenantConn` lookup | PASS | `SERVICE_ACCOUNT_BY_CARD_REF_SQL` now binds only principal kind and CardRef, retains `LIMIT 2`, and relies on the table's forced-RLS policy. `mise run check:tenant-isolation` passes. |
| Rust structure, imports, and documentation | Concrete workflow owners; top-level imports; bare names in signatures; substantive rustdoc with error/panic sections | **FAIL** | Workflow ownership and rustdoc close prior findings, but the cumulative additions retain the import/signature violations in `STD-003-R3-001`. |
| Public Card/HTTP/OpenAPI contract | Status remains server-derived; wire fields and generated schemas stay aligned; public errors remain catalog-backed | PASS | Rust schemas, runtime OpenAPI assertions, and read-side hydration expose `status.verification.binding_ids`; no new error mapper or parallel catalog exists. |
| Rust, Python, and TypeScript SDK journeys | Every first-class shipped Card surface proves stable binding IDs through a real client/server path | PASS | Rust, Python, and TypeScript journeys exist. The R3 TypeScript source now exposes the complete status shape and accesses it without a cast; the exact TypeScript journey passed independently. |
| Generated artifacts | Generated schemas/goldens come from owners and are drift-free | PASS | `mise run codegen:check` passed independently; no unmatched generated delta remained. |
| Completion evidence | Exact commands are reproducible and cumulative whitespace is clean | PASS | Focused SQL/TypeScript, format, lint, tenant, codegen, and `git diff --check` commands passed. The new standards finding is not covered by those automated gates. |

## Applicable Rule Results

| Rule | Result | Source evidence |
|---|---|---|
| Verification is expressed through typed Card bindings and server-derived Card status without rewriting authored specs | PASS | `VerificationStatus`, `hydrate_card`, projection tests, schemas, served OpenAPI, and SDK journeys agree. |
| Durable identities use domain newtypes and reject invalid stored UUID versions | PASS | Public verification SQL boundaries use `BindingId`, `CardUid`, and `PrincipalId`; private decode rows validate persisted UUIDv7 identities. |
| Dependency-backed multi-step Rust workflows have a cohesive concrete owner | PASS | `BindingProjector<'_, '_>` owns the borrowed transaction and freeze/project workflow; narrow SQL operations and deterministic conversions remain free. |
| Imports form a top-of-module dependency manifest and signatures use imported bare type names | **FAIL** | See `STD-003-R3-001`. |
| Tenant-scoped library SQL accepts `TenantConn`, relies on forced RLS, and never ends the caller transaction | PASS | Verification queries and the CardRef principal lookup use `TenantConn`; the remediated lookup has no tenant predicate; callees do not commit or roll back. |
| Every independently evaluated authorization decision is transactionally audited and failure closes the operation | PASS | Operator-bearing registration evaluates and records `cards:write` plus conditional `operators:invoke`; denial and append failure paths leave no registration writes. |
| New user-facing behavior has real SDK-to-server-to-SDK journeys for Rust, Python, and TypeScript | PASS | All three Card surfaces read stable UUIDv7 binding IDs; the TypeScript journey now receives compile-time contract coverage. |
| TypeScript projects the existing wire contract without owning durable behavior or inventing errors | PASS | `CardStatus` and `VerificationStatus` are readonly projections over the existing native JSON transport; no conversion, endpoint, client, or error code was added. |
| Generated schemas and goldens are regenerated, not independently authored | PASS | Source annotations and four matching schema artifacts are drift-free under `codegen:check`. |
| No legacy vocabulary, compatibility path, raw production pool, new feature, lint suppression, or gate circumvention was added | PASS | Complete cumulative diff and manifests contain none of these prohibited changes. |

## Prior-Finding Closure

| Finding | R3 disposition | Reassessment evidence |
|---|---|---|
| `FIND-TASK-003-1` | VERIFIED CLOSED | Effective Operator-bearing registration requires and audits `operators:invoke`; denial/rollback/cardinality proof remains present. |
| `FIND-TASK-003-2` | VERIFIED CLOSED | The shared CardRef lookup fetches at most two active matches and returns only one exact match; ambiguous partial references fail closed for all retained callers. |
| `FIND-TASK-003-3` | VERIFIED CLOSED | Activity uses `GREATEST(last_authenticated_at, $2)` and cursor arming remains null-only. |
| `FIND-TASK-003-4` | VERIFIED CLOSED | Effective schedules require a future occurrence before registration writes. |
| `FIND-TASK-003-5` | VERIFIED CLOSED | `$owner` is non-null, reserved against component aliases, and enforced for standalone Agent owners. |
| `FIND-TASK-003-6` | VERIFIED CLOSED | Public verification APIs use existing typed identities and refuse invalid stored UUID versions. |
| `FIND-TASK-003-7` | VERIFIED CLOSED | `BindingProjector` is the cohesive transaction-scoped workflow owner. |
| `FIND-TASK-003-8` | VERIFIED CLOSED | Existing API-key, workload JWT, lifecycle, exclusion, A/B-version, replica, and observation journeys remain in the cumulative candidate. |
| `FIND-TASK-003-9` | VERIFIED CLOSED | `Card.status`, `CardStatus`, and `VerificationStatus` are exported in TypeScript; the journey reads `status?.verification?.binding_ids` directly without a local cast. |
| `FIND-TASK-003-10` | VERIFIED CLOSED | Served OpenAPI resolves the complete nested status chain and UUID-array item shape. |
| `FIND-TASK-003-11` | VERIFIED CLOSED | Added/materially modified Rust items carry the required rustdoc and applicable `# Errors`/`# Panics`; R3's import/signature violation is a separate rule. |
| `FIND-TASK-003-12` | VERIFIED CLOSED | `SERVICE_ACCOUNT_BY_CARD_REF_SQL` has no `data_tenant_id` predicate or tenant bind, uses `$1`/`$2`, and preserves forced-RLS exact-one selection. |
| `FIND-TASK-003-13` | VERIFIED CLOSED | `git diff --check base..candidate` exits zero and `git diff -w 449aceb..candidate --` for the R1 standards report is empty. |

## Open Questions

None. The retained correction is fully determined by existing repository rules and does not require a specification or architecture decision.

## Verification Notes

- Confirmed `.codegraph/` is absent before source discovery.
- Confirmed `HEAD` was `a5a5b60f446981760ac831f64aba871582ec45e4` before review and remained pinned after the audit.
- Inspected the complete `base..candidate` diff, all R1/R2 verdicts, validation ledgers, remediation tasks, changed manifests and lockfile, source owners, tests, generated schemas, served-contract proof, and first-class SDK consumers.
- Independently passed:
  - `mise run fmt`
  - `mise run lints`
  - `mise run ts:typecheck`
  - exact TypeScript `cards-state.test.ts` selector under the repository Postgres wrapper
  - exact `wyrd-sql` CardRef SQL-text unit test
  - `mise run check:tenant-isolation`
  - `mise run codegen:check`
  - `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4`
- Broader cumulative SQL, principals, cards, identity, Rust/Python/TypeScript journey, and Wyrd-family results are recorded in the immutable task/remediation evidence and were cross-checked against their exact current tests; they were not all rerun in this standards slice.
- Automated checks do not enforce the retained top-level-import/bare-signature rule; their success does not close `STD-003-R3-001`.

## Overall Verdict

**FAIL** — `FIND-TASK-003-1` through `FIND-TASK-003-13` are verified closed, including every R2 remediation, but the cumulative candidate still violates the mandatory Rust import/signature source-shape rule in newly added production and test items. The correction is mechanical and behavior-preserving, but repository authority makes it required before approval.
