# Independent repository standards review

Result: **FAIL**.

Subject: base `f4e79ea00523af5cd9212fdacde64f16c97ad554` to candidate `365bfc94bfef1a153f213462f7ef104afa823741`; first review, full cumulative range. Candidate HEAD remained unchanged. Scope is repository standards across all 103 changed files, including architecture, manifests, Rust contracts/runtime/server/clients/tests, SQL, Python binding/stubs/tests, TypeScript binding/declarations/wrapper/tests, generators, fixtures and documentation. No other reviewer report was read. This is not a separate task-acceptance or abstraction audit.

I read the root and four applicable nested AGENTS files, the task-review skill, navigation, specification revision 16 and publication/public-contract supplements, task evidence, spec-driven-development, maintainer style, Rust/errors/agent-surfaces guides, architecture authority map and owning Verifier/workspace, Bifrost durability/table/public-surface, permission/audit/Postgres, and operations readiness sections. CodeGraph was consulted before source discovery. All changed paths were inventoried from the immutable Git range; source and diffs were inspected by owner and cross-tier consumers.

## Authority coverage

All rows inherit root `AGENTS.md`, `changes/spec-driven-development.md`, and `agent_docs/maintainer-style.md`. Rust rows additionally inherit `agent_docs/rust.md`; exposed errors inherit `agent_docs/errors.md`; public contracts, CLI, MCP and generated projections inherit `agent_docs/agent-surfaces.md`.

| Changed surface | Additional governing authorities | Source coverage |
|---|---|---|
| Architecture amendments | `architecture/README.md` conflict/normative rules; approved revision 16 and supplements | All three amended architecture documents; durable acknowledgement, publications, exact-Verifier inspection, revocation and history clauses compared with their existing owning sections |
| Cargo workspace and four changed crate manifests/lock | Root dependency/feature/tier/platform rules | New `unicase`, reused Unicode normalization, test-only base64; no new feature union, platform gate, runtime or cross-tier SQL dependency |
| `wyrd-spec` execution/ids/error/reference/validation, schema example | `wyrd-design.md` Verifier, client model, errors; pure-contract boundary | New publication/producer/page/cancellation contracts, reference parsing/placement validation, stable error metadata and schema roots |
| `wyrd-runtime` permission and builtin roles | `wyrd-security-posture.md` Permission model/check and execution-read amendment | Exact UID read permission, action separation, builtin role additions and tests |
| Bifrost Gate and table/staged changes | `bifrost-design.md` Table identity, Variant, durability, Public surface | New canonical events table and registration, sensitive JSON column, public write reservation, nullable integer staging and tests |
| `vala-verify` blocks/engine/scope/agent and tests | `wyrd-design.md` Verifier DAG, attempts, requirements, workspace sources | Checkpoint interface, engine restore/write integration, publication mounts/source attribution and engine unit tests |
| SQL queries/events/inspection and five migrations/tests | `crates/wyrd/wyrd-sql/AGENTS.md`; security Postgres tenant binding; coordination clock; Bifrost committed handoff | Actual `TenantConn`/`OperatorPool` signatures, lease predicates, transaction ownership, named row decoding, RLS/composite keys, trigger functions, role migration and pg tests |
| Server admission/runner/scheduler/results/history/outbox | Verifier durable queue and result ownership; Bifrost durability/outbox; operations readiness/recovery | Admission reply after committed submission, checkpoint bridge, deadline duration, existing process outbox, projector and canonical result staging |
| Server hosted workspace/publications/evidence | Verifier workspace; approved publication supplement; security tenant isolation; shared storage lifecycle | Freeze/tree checks, File completion/inventory, fenced manifest acceptance, replay identity, retained-evidence reader and mount materialization |
| HTTP verification service/routes/inspection/error mapping and integration tests | Verifier public-surface and security exact-Verifier/SYSTEM read clauses | Typed paths/queries/responses, audit authorization, pagination limits, exact producer selection, download/content/cancel, served OpenAPI and route tests |
| Auth revoke in owner and server | Security principal lifecycle, accepted authority and audit; SQL transaction ownership | Principal cancellation on revocation's same caller-owned connection; server await path |
| Storage routes/inventory/service and shared-client download dispatch | `wyrd-design.md` File lifecycle; security tenant binding and publication-content clause | Existing File completion reused, common download-plan/local-byte owners, authenticated relative local dispatch |
| Shared Rust client observe/verify/export changes | Client/server locked boundary; agent surfaces | Public execution handle methods, canonical wire exports and common HTTP/transport reuse |
| CLI command tree/lib/new execution module | Agent surfaces; Verifier permissions/contracts | Typed args, one shared execution handle, JSON output, typed stable boundary errors, no credential CLI argument |
| MCP descriptors/dispatcher and MCP tests | Agent surfaces; read tools always available, explicit write scopes; domain authorization | Read/write descriptor split, shared `VerificationControl`, schema types, stable result/error projection and real-server MCP journey |
| Rust SDK manifest/integration journeys and `wyrd-testing` helpers/server journeys | Root testing plus `TESTING.md` SDK/client-facing conventions | Real server/Postgres journeys, crash/restart, publication seeding seam, inspection and CLI assertions; test helper rustdoc included |
| Python PyO3 verify, observe/verify stubs and two integration tests | `sdks/wyrd-sdk-python/AGENTS.md` placement, runtime bridge, errors, six layers, authored stubs, tests | Existing registered `Execution` wrapper extended; shared Rust calls, JSON projection at boundary, public package export tree, TypedDicts and public-import journeys |
| TypeScript native verify/declarations/wire types/error union/wrapper and two journeys | `sdks/wyrd-sdk-ts/AGENTS.md` N-API bridge/types/generated declarations/errors/tests | Native Promise API, thin JSON boundary, matching native declarations, generated wire roots and public typed execution methods |
| `scripts/gen_ts_types.py` and schema-root generator | Agent surfaces generated-artifact ownership | New wire/request roots, runtime-derived schemas, generated TS/error artifacts compared with source contracts |
| Docs source/error reference/generated llms/diagram | `docs/AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; Verifier public surface | Draft guide remains draft; acknowledgement and permission statements, tool reference, generated error row/llms and diagram wording |
| Fixtures and fixtures README | Root vocabulary/Card contract and journey rules | Ordinary Agent/Prompt/Verifier/Service fixtures for existing live verification story; no new Card kind or alternate execution discriminator |

## Rule results

These results describe static compliance of the changed code. Recorded checks are not independent reruns.

| Applicable rule | Evidence and result |
|---|---|
| Narrowest architecture owner; amendments follow approved outcomes | **PASS.** Architecture diffs put publication/workspace and inspection contracts in `wyrd-design.md`, table/outbox history in `bifrost-design.md`, and SYSTEM/revocation controls in security posture. No new noun, compatibility alias, private enterprise path or unverified capacity claim. |
| Durable contracts in pure `wyrd-spec`, server behavior, thin SDK projections | **PASS.** `verification/execution.rs`, `verification/ids.rs`, server owners and shared-client methods preserve this split; no IO/async/PyO3 dependency added to spec. |
| Dependency and platform constraints | **PASS.** Changed manifests add Unicode handling to server and test digest encoding only; no raw SQL dependency in client tier, new feature flag, per-platform behavior or ad hoc runtime. |
| Stateful capability owner, synchronous pure work, typed ids and stable errors | **PASS for material standards.** `Publications`, `EventProjector`, `VerificationControl`, `ExecutionCheckpoints`, `TenantEvidence` and existing client `Execution` retain explicit dependencies. Public publication names validate in spec; producer positions use the existing typed task/execution ids. New public error derives/catalog projection remain canonical. |
| SQL connection capability types | **PASS.** New execution inspection/checkpoint/event methods take `&mut TenantConn<'_>`; the cross-tenant pending-event scan takes `&OperatorPool` (`events.rs:379`). Production fields use `WyrdPostgres`/`OperatorPool`; no new raw-pool function argument or field. Existing `cache.rs` raw connection is unchanged and belongs to the documented statement-bound `VerificationTargets` exception; it was not mistaken for new capability drift. |
| Caller owns transaction and committed cross-owner handoff | **PASS.** SQL methods do not commit/rollback a borrowed `TenantConn`. Server checkpoint/history/publication owners acquire and commit their own connections; revocation composes cancellation on its existing connection. File completion/inventory precede publication-manifest commit, and history batch identity commits before Scribe handoff. |
| Forced RLS, composite tenant keys, operator confinement and no tenant-query filter substitute | **PASS.** New invocation/publication/event tables force RLS with both predicates and migration-login operator policy; execution foreign keys carry tenant. Tenant methods filter execution/producer identity, not manual tenant predicates. New trigger functions are ordinary invoker functions with pinned empty search path, not general SECURITY DEFINER bypasses. |
| PostgreSQL coordination clock; immutable migration additions | **PASS.** Claim binds durations and SQL derives first deadline/lease using `statement_timestamp()`; five new ordered migrations add the state. Cancellation and checkpoints use database timestamps and fences. No old migration is edited. |
| Column-name mapping | **FAIL, non-blocking under review materiality.** New `events.rs:406–420` decodes four aggregates using a tuple, despite SQL AGENTS “Map columns by name, never by position.” The present SELECT and tuple agree; no current incorrect count or supported-use failure is established, so this is not elevated to a material finding or independent remediation requirement. Other new publication/event/page rows use named `FromRow` structs. |
| Tenant/principal derive from server; exact domain permission and non-blocking audit | **PASS.** `inspection.rs::authorized/require_verifier` uses the verified caller tenant and exact queued Verifier; no new caller-supplied tenant authority. `Permission::verifier_read` and `verifier_run` remain distinct, with no `cards:read` fallback. Local content resolves the same publication File and permission as its plan. |
| Public handlers typed, instrumented and stable errors | **PASS.** New verification handlers use typed `ExecutionId`, page/selector and response contracts, scrub `caller/state/query`, and return `WyrdErrorResponse`; no new problem-json mapper. MCP delegates to the same service and catalog conversion. |
| Agent surfaces bounded and discoverable | **PASS.** Page limit defaults to 50/max 200, cursor bounded to 2048 bytes and bound to listing/execution/tenant; exact producer selectors; read tools in unscoped descriptors and cancellation in write descriptors; CLI prints typed JSON. |
| Canonical Bifrost table/data projection | **PASS.** Events joins `BUILTIN_TABLES`, fixed Gate write reservations, sensitive Variant metadata and daily partition layout; live inspection stays on execution records. Existing `StagedRow` handles nullable integers rather than a second Arrow writer. |
| Rustdoc for every new/materially changed item, fields, tests/helpers; Errors/Panics | **FAIL — REPO-001.** New SQL row fields, trait bridge methods, fallible revoke helper, and test/helper panic documentation remain incomplete. Details below batch all known gaps. |
| Python placement, GIL/runtime and canonical exception | **PASS.** New methods remain under SDK `src/verify.rs`; `Bound` input is converted before shared runtime work; existing `trace::block_on` and WyrdPy conversion are reused. No native class registration/export changes are needed for methods on the existing `Execution`. |
| Python authored stubs and runtime public-contract parity | **FAIL — REPO-002.** Native download serializes the full shared response, but the new authored DownloadPlan omits required `ttl_secs`. Other added method signatures match their wrappers and the package `__all__` remains consistent; wire TypedDicts follow the package's existing stub-only declaration convention. |
| Python tests use public imports/top-level functions and real-server journey | **PASS.** New integration test imports `wyrd.verify`, `wyrd.testing` and public modules, is integration marked and uses top-level `test_*`. It exercises actual bindings, not a Rust emulation of Python lifetime. |
| TypeScript thin native binding, public types, generated native parity | **PASS statically.** Five new `NativeExecution` methods match both `index.d.ts` and `index.d.cts`; wrapper calls preserve wire names and typed return values. Generated card-types/error union have corresponding Rust/generator source. No TS durable logic or second transport introduced. Native parity gate not independently rerun. |
| Generated artifacts changed through source/generator; no committed schemas | **PASS on available evidence.** Rust schema roots and Python generator roots account for new TS shapes and catalog error. Task records `codegen:check` and `docs:generate` with no drift. No committed JSON Schema added. Generation itself was not rerun because it mutates the reviewed tree. |
| Tests live at appropriate seam/tier; assertions keep negative/security paths | **PASS for standards, with proof limits.** New backend/SQL tests use pg files, SDK/MCP journeys drive real server and repository Postgres, engine pure logic uses in-module unit tests. Integration journey ignores are the repository's explicit Postgres-lane selection convention, not removed/weakened proof. Task evidence records exact journeys and the corrected nonzero Proof 2 selector. Python/TS publication-content coverage remains the explicitly disclosed limitation; acceptance completeness belongs to behavior review. |
| Docs match owner vocabulary/current behavior; generated refs stay projected | **PASS statically.** Changed guide remains `draft: true`; source and MCP reference correctly distinguish durable admission, read/run permission and shared download plan. Error table derives the new error. No new unproved public guide or copied runnable example was introduced. `docs:check` not independently rerun. |
| No suppression or erased test used to clear failures | **PASS on inspected range.** No new `#[allow]`, deleted negative proof or weakened assertion found. Postgres journey selection annotations remain intentional lane conventions. |

## Material findings

### REPO-001 — VIOLATION: mandatory changed-item rustdoc gate remains open

**Rule.** Root AGENTS Code: “Every new or changed Rust item has rustdoc … `# Errors` on fallible items, `# Panics` when it can panic. Missing rustdoc blocks merge.” `agent_docs/rust.md` explicitly includes fields, private helpers, test helpers and tests, regardless of visibility.

**Locations and source evidence.**

- `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1708`: new `PublicationRow` has eight undocumented fields (`task_id` through `manifest`).
- `crates/wyrd/wyrd-sql/src/queries/verifier_executions/inspection.rs:91–110`: new `InvocationRow` and `PublicationEntryRow` have thirteen undocumented fields in total. Their type comments do not document the fields.
- `crates/wyrd/wyrd-auth/src/revoke.rs:99`: new fallible `cancel_executions` has a summary but no `# Errors`; it propagates the query error through `internal_error`.
- `crates/wyrd/wyrd-server/src/verification/runner.rs:1164,1173,1182`: new `Checkpoints for ExecutionCheckpoints::{begin,forget,settle}` methods have no item rustdoc/error contract. These invoke `fenced`, which can fail on transaction, query, commit or stale lease.
- `crates/vala/vala-verify/src/blocks/engine/tests.rs:2339,2351,2363`: new fallible `Ledger` checkpoint methods have panic sections but no `# Errors` (they currently return only `Ok(())`; that fact can be documented without inventing an error).
- `crates/wyrd-spec/src/verification/ids.rs:105–124`: new `PublicationName` FromStr associated error/method, Deserialize method and JsonSchema methods lack item rustdoc; fallible parsing/deserialization also lack `# Errors`.
- `crates/shared/wyrd-runtime/src/permission.rs:1410`: materially expanded `verifier_run_is_uid_scoped` uses `expect` and assertions without `# Panics`.
- `crates/wyrd-spec/src/verification/ids.rs:270`: new publication-name test uses `expect`/assertions without `# Panics`.
- `crates/wyrd/wyrd-server/tests/integration/pg_durable_verifier_work.rs:162,171`: new `call` uses `PublicationName::new(...).expect`; `rule` explicitly panics on a nonstructured error. Neither helper documents `# Panics`.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:2955–2959`: five newly introduced local tool-name constants have no rustdoc; `sdks/wyrd-sdk-rust/tests/integration/durable_verifier_work.rs:133–134` likewise adds undocumented local REVIEW/JUDGE constants.

**Consequence/materiality.** This is the explicit mandatory merge gate, not a request to improve prose or add speculative documentation. It covers production persistence decoding/transaction bridges and materially changed proof helpers together; green clippy/codegen does not prove these private/test docs exist.

**Smallest correction.** Document the listed newly introduced/materially changed items in place, including field meaning and fallible/panic behavior. Reuse a precise link to the owning trait/helper where it conveys the contract, with required Errors/Panics sections. Do not alter runtime behavior, add a doc checker, expand unrelated docs or create a standalone style round. Batch this bounded set into the same remediation as confirmed implementation work.

**Focused closure proof.** Reinspect the listed item bodies and attached rustdoc, including test/helper code, against the mandatory gate; existing formatting/lint lane for edited Rust only. No new behavioral test is needed for documentation.

### REPO-002 — INCORRECT: authored Python download plan drops the shared expiry field

**Rule.** SDK Python AGENTS requires authored stubs to change with the native API and precisely project durable field names/types; root client/server contract and maintainer-style require types to carry the public contract.

**Location.** `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:463–466`, `DownloadPlan` declares only `get_url`.

**Producer-to-consumer evidence.** `crates/wyrd-spec/src/storage/download.rs:40–48` defines required `get_url: String` and `ttl_secs: u32`, documenting cloud presigned expiry and local zero. `wyrd-storage/src/service.rs::download_plan` populates both. Server publication `download` returns the shared `DownloadInitResponse`; client `Execution::download` preserves it; Python `src/verify.rs::PyExecution::download` serializes the whole plan into the returned mapping. Its declared `DownloadInit.plan` points at the incomplete new TypedDict. The TS generated DownloadPlan correctly includes required `ttl_secs` (`card-types.ts:1819`).

**Observable consequence/materiality.** A supported typed Python caller consuming a presigned publication plan cannot inspect `result['plan']['ttl_secs']` without a type error, although that required field exists at runtime and determines how long its URL may be used. This is a source-proven first-class client public-contract omission, not hypothetical cloud hardening.

**Smallest correction.** Add required `ttl_secs: int` to the authored Python DownloadPlan, with the shared contract's cloud expiry/local-zero meaning. Keep the existing shared server/client/native behavior; no new type adapter, transport, runtime class or generated stub path is needed.

**Focused closure proof.** Statically compare the two required shared fields to the stub and run the existing Python stub/typecheck lane. A tiny typed consumer reading both `get_url` and `ttl_secs` is sufficient if a runtime assertion is added once publication-content seeding is available; do not create a new fixture/harness just for this field.

## Verification and limits

No Cargo build/test job, formatter, generator, install, mutation of source, commit or push was run. I inspected the actual source and recorded task verification independently of the implementation completion summary. Task evidence records fmt/lints/codegen/tenant/dependency/Python/TS checks and focused real-Postgres journeys as passing; this review does not claim to have rerun them.

Orchestrator confirmed `ts:napi:check` depends on a build that rewrites tracked declarations, and `docs:check` runs generation/install/build, so neither was run against the immutable subject. Static declaration/doc-source parity was checked instead; omission of those independently rerun lanes is a verification limit, not itself a code defect. Architecture/failure behavior beyond the standards obligations remains with the selected behavior/domain/system reviewers.

CLI naming concern: `wyrd execution` is broad but the new command's documentation and arguments explicitly identify Verifier executions. No governing authority requires a rename. A future naming choice is non-blocking preference and is not a finding.

Skipped: mutating gates and independent runtime reruns. Remaining review risk: recorded green results do not close the explicit rustdoc and Python declaration defects above.
