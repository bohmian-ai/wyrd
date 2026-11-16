# Repository standards review — TASK-004

Result: **FAIL**.

Subject: `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9...487ccb0b60a2a3b40b54ee9e12099880d042ba99`, first review, complete 138-file range. No prior coverage carried forward. This report audits repository rules, not task acceptance or alternative implementation preferences. No other reviewer reports were read.

## Authority coverage

The root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md`, `agent_docs/maintainer-style.md`, and `TESTING.md` apply throughout. `rg --files -g AGENTS.md` found exactly the root and four nested owners below; all were read. CodeGraph was used before source discovery, with repository/GitHub and SQL capability queries; subsequent Git/source inspection expanded that map.

| Changed surface | Applicable additional authorities | Inspected evidence |
|---|---|---|
| Workspace dependencies, manifests and lock | Root dependency/platform rules; `agent_docs/rust.md` build/features | `Cargo.toml`, lock, server/testing/Rust SDK manifest diffs; immutable bohmian revision `9fdf6dc77a329f34d96b7ad4f397331d2d18216b`; protocol dependency is server-owned, fixture dependencies testing-owned |
| `wyrd-spec` repository identities, request/result/retained subject, permission scopes, IDs and error catalog | `agent_docs/rust.md`, `errors.md`, `agent-surfaces.md`; `architecture/wyrd-design.md` Client model, Verifier/execution APIs, Error catalog; `public-contracts.md` Repository requests/results and administration API closure | `repository.rs`, `verification/{execution,state}.rs`, auth scope, IDs, error variants, schema roots; Card subject remains an untagged CardRef arm |
| Shared client, loader, saga and configuration | Rust/errors guides; design Composite declaration, Spec-file authoring/path resolution, Workspace config; public-contracts repository arm | `wyrd-client/github.rs`, client/reexports, verify request construction; loader lib/path/validate; declaration preparation/phases; strict config selectors and conversions |
| Runtime permission matcher, roles, issuer and server authorization | Rust/errors/agent-surface guides; `architecture/wyrd-security-posture.md` Access tokens/internal verification, Authorization and policy, Postgres tenant binding, Audit integrity, Outbound credentials | Runtime permission validation/matching and new resources, built-in roles, issuer unattended path, background authority, GitHub service/credentials/provider/webhook, verification admission/inspection; no SDK-owned permission authority |
| HTTP, MCP and CLI | `agent-surfaces.md`, `errors.md`, Rust guide; design Client model and execution APIs; security permission/audit rules | GitHub mounted routes, typed OpenAPI bodies, scrubbed handler instrumentation; webhook raw-body ingress; MCP descriptors/catalog dispatch and write visibility; CLI file/stdin credential bodies; protocol/source assertions in OpenAPI and surface journeys |
| Verification worker and existing state consumers | Rust guide; design Verifier engine/Coordination clock; Bifrost design Durability/public verification and canonical Scribe outbox/audit paragraphs; operations README/readiness, deployment migration contract, reliability Verification execution state/recovery | `repositories.rs`, boot/runtime/health, runner/cache/authority, scheduler/groups/drift, state/results/submissions, scribe outbox adjustments; existing ClaimLoop, verification control and retained reader composition |
| SQL queries, migration and Postgres owner | `crates/wyrd/wyrd-sql/AGENTS.md`; security Postgres tenant binding; operations Deployment and Release / Migration contract | Entire new migration and query signatures/fields; github/repository_reviews/verifier_executions/verification_target/human_connections/postgres changes, RLS and tenant-composite references, fenced claim SQL and SQL pg proof |
| Storage upload extension | Rust/errors guides; design Composite declaration/Storage lifecycle; security tenant/path/credential rules | `wyrd-storage/service.rs` pending-body validation, local/server upload and unchanged completion ownership; server declaration caller |
| Rust tests, fixture service and helper changes | Rust rustdoc/testing rules; `TESTING.md` tiers, fixture and client-facing journey rules | New repository SDK journeys, fixture GitHub service/keys/archive/check controls, testing server and verification helpers, changed CLI/MCP/server/SQL and existing Card journeys; assertions and setup inspected by concern |
| Python/PyO3 and authored declarations | `sdks/wyrd-sdk-python/AGENTS.md`, Rust/errors guides; design Client model | Native GitHub wrapper/registration, shared trace/runtime calls, public package export, GitHub/testing/verify stubs, public export and integration tests; production/testing feature placement |
| TypeScript/N-API and declarations | `sdks/wyrd-sdk-ts/AGENTS.md`, Rust/errors/agent-surface guides; design Client model | Native GitHub/shared-client bridge, testing-only fixture bridge, public GitHub facade/types, generated `.d.ts`/`.d.cts`, testing declarations/loaders, public journeys and support |
| Generator, docs, README/example and fixtures | `agent-surfaces.md`; `docs/AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; root generated-artifact and fixture rules | Rust schema roots, Python TS generator roots/request sets, generated wire/error types, generated error reference and llms text, MCP reference, matching README/quickstart narrowing and checked-in repository Verifier fixtures |

These authorities were available; no missing-authority blocker applies. Sensitive-domain behavioral acceptance is outside this specialist report; the schema rule below is independently source-proven.

## Applicable rule results

| Rule | Result and source evidence |
|---|---|
| Pure contracts in wyrd-spec; durable behavior server-owned; SDKs consume shared client | PASS for placement. `wyrd-spec/repository.rs` is serialization/domain data; `wyrd-client/github.rs` calls Wyrd routes, and native SDK wrappers call that handle. Provider protocol is server-only. |
| Client/Python dependency boundaries and immutable upstream dependency | PASS by manifest/source inspection and recorded `check:deps`. No new owner-crate Python feature is enabled by Rust/TS; new PyO3 code is in the Python SDK. |
| Stateful operations owned by concrete structs; no ad hoc Tokio runtime or production unwrap | PASS in reviewed changes: GitHub client, provider, administration, ingress and worker owners; bindings use shared runtime. No new target_os gate, raw credential Debug, or unreasoned allow was found. Ownership redesign is not proposed here. |
| Typed HTTP/OpenAPI/MCP contract, stable errors and scrubbed write instrumentation | PASS by route/descriptor inspection: request bodies derive from wyrd-spec, boundary decode errors name position rather than credential values, handlers return the common Wyrd response mapper, webhook explicitly declares its independent security. Catalog-backed GitHub errors are projected into TS/docs. |
| Read MCP tools always discoverable; write visibility supplements operation authorization | PASS: `mcp/mod.rs:183`, `:266`, and `mcp/github.rs` descriptors, `may_manage` and service dispatch. Reads enter the unscoped catalog and writes require integration-write visibility with service checks retained. |
| Secrets wrapped and envelope/key rotation reused | PASS for representation/composition: `github/credentials.rs:18-112` uses SecretString, redacted Debug, Zeroizing plaintext and tenant/integration binding; `human_connections.rs` adds GitHub to existing rewrap enumeration. No live rotation claim is made. |
| Actual SQL function/field capabilities, not only scanning-gate coverage | PASS for capabilities: new tenant queries use `&mut TenantConn`; both cross-tenant due queries and worker fields use OperatorPool. Postgres directory lookup stays inside WyrdPostgres and a narrowly scoped pinned-search-path definer. Existing VerificationTargets remains the documented statement-owner exception. No new PgPool/PgConnection/Transaction parameter was introduced in domain production code. |
| Caller-owned tenant transaction; no manual tenant predicate on TenantConn; PostgreSQL coordination clock | PASS in new query owners. Inserts bind tenant from conn, reads/updates rely on RLS, claim/association/delivery deadlines derive from statement_timestamp and bound durations. Query helpers do not commit their passed TenantConn; service callers commit. |
| Every tenant table forces RLS and has tenant/operator policies | PASS in new migration; all eight tables include both policies with USING/WITH CHECK. |
| Tenant-bearing parent/child foreign keys | FAIL: REPO-004-SQL-FK. RLS does not repair the review-only FK. |
| Every new/changed Rust item has useful rustdoc, fallible items Errors and panicking items Panics | FAIL: REPO-004-RUSTDOC. Green compiler/lint evidence does not waive this explicit merge gate. |
| Python six-layer export/runtime/stub parity; precise types for closed shapes | FAIL for result subjects: REPO-004-PY-SUBJECT. GitHub handle registration/export and testing-extra placement otherwise match. |
| TS native boundary/facade/generator/declaration parity | PASS by static source mapping and recorded checks. Facade uses shared client's github handle; generated subject union is CardRef or wrapped RepositorySubject. `.d.ts` and `.d.cts` are byte-identical (`cmp` independently rerun). NAPI regeneration was not rerun in this immutable checkout. |
| SDK journeys are public examples, checked-in Card YAML, no raw HTTP/result JSON/polling in their bodies/support | FAIL: REPO-004-JOURNEY-OWNERS. Real SDK/server/Postgres journey presence and exact-code negative assertions do pass; that does not waive structure rules. |
| Generated outputs derive from the owning source, docs reference actual catalog | PASS on source-to-output mapping plus recorded codegen/docs checks: schema/TS generator roots include new contracts, errors are derive-backed and generated, new MCP reference names correspond to descriptor constants. README and TypeScript example make the same subject-union adjustment. |
| Source immutable and no source archive/patch/report cross-reading | PASS. HEAD remains candidate; only pre-existing two skill edits/untracked change directories were present before this report. No reviewed source was edited. |

## Proposed findings

### REPO-004-SQL-FK — VIOLATION — Check/review FK omits tenant identity

**Authority:** `architecture/wyrd-security-posture.md:759-764`, Postgres tenant binding: tenant-scoped parents expose composite foreign-key targets and children carry the tenant key in their foreign keys. This is a required tenant-integrity boundary, not an optional indexing preference.

**Location:** `crates/wyrd/wyrd-sql/migrations/20261109000000_github_repository_verification.sql:249-252`; parent at `:205-223`.

`repository_checks.review_id` references only `repository_reviews(review_id)`, while the check independently supplies data_tenant_id. The parent has no unique `(data_tenant_id, review_id)` target. A check for tenant B can therefore satisfy both its own RLS predicate and this FK while referencing a review belonging to tenant A. PostgreSQL referential-integrity checking is not restricted by the caller's row visibility. Normal `admit_repository_review` currently writes matching tenants, but correctness of that writer does not provide the database invariant required for every login; an operator/import or future writer can persist the mismatched relationship. Joins/read claims then have a tenant-owned check with no same-tenant review, breaking delivery integrity. This is a schema-level guarantee failure, not a claim of a demonstrated public HTTP exploit.

**Smallest correction:** add the tenant-bearing unique parent target and use `(data_tenant_id, review_id)` for this FK, following this same migration's association-state/delivery composite references (`:92-93`, `:182-183`). Preserve review/check identities, historical rows and existing delivery lifetimes. Apply migration immutability rules: use a new migration if this one has been applied outside disposable fixtures. Do not add application-side tenant predicates or change RLS to compensate.

**Closure proof:** real repository-managed Postgres, two tenants: same-tenant Check/review succeeds, B Check -> A review is rejected as foreign-key violation; existing admission/delivery SQL proof still passes. No new harness is necessary.

### REPO-004-RUSTDOC — VIOLATION — Mandatory changed-item rustdoc gate remains unmet

**Authority:** root AGENTS Code: every new/changed Rust item requires rustdoc, Errors on fallible items, Panics where it can panic, and missing rustdoc blocks merge; `agent_docs/rust.md` Rustdoc explicitly includes private helpers and tests.

**Locations and directly inspected examples:**

- No rustdoc: `wyrd-config/src/config.rs:96` VerifierSelection::try_from, `:113` conversion back to String, `:194` parse_text; `wyrd-spec/src/repository.rs:155` CardRef conversion.
- No rustdoc or Panics: `wyrd-loader/src/path.rs:244`, `wyrd-runtime/src/permission.rs:1558`, and `wyrd-spec/src/repository.rs:546,558,576,590` newly added tests. These bodies assert and/or unwrap/expect.
- Fallible helpers lack Errors: `wyrd-loader/src/lib.rs:190` load_in, `wyrd-spec/src/repository.rs:248` present, `wyrd-sql/src/queries/repository_reviews.rs:117` due_tenants, `wyrd-sdk-rust/tests/integration/repository_verification.rs:111` associate. The new Debug formatters at `wyrd-client/src/github.rs:49` and server `github/credentials.rs:37` likewise return FmtResult without Errors documentation.
- Additional new tests/helpers document intent but omit Panics despite explicit assertions/expect: loader lib `:404`; server github credentials `:130`, provider `:256,271`, service `:1293,1311`; server verification repositories `:1371,1395`; SQL repository_reviews `:1073,1101`; Rust SDK repository_verification `:111,583,952,1236,1273`.

All shortened crate names above refer to their changed paths in the scope table. This inventory comes from changed-item inspection, not from treating compiler missing_docs coverage as sufficient. For example, `associate` returns a Result but also expects its begin-association operation, so both Errors and Panics have concrete content to describe.

**Consequence/materiality:** the repository explicitly blocks merge on this gate. Configuration decoding and SQL tenant lookup/refusal semantics and journey helper panic assumptions are absent at their owning items; neighboring well-documented functions do not document them.

**Smallest correction and proof:** add concise intent/invariant and applicable Errors/Panics sections to the affected changed items, covering the complete touched helper/test set in the same remediation. Preserve behavior, tests and lint policy; no suppression or separate style-only review round. Source audit of that bounded list plus normal narrow Rust format/lint checks proves closure.

### REPO-004-PY-SUBJECT — MISSING — Python result wire declaration remains Card-only

**Authority:** Python AGENTS Public API six layers and Stubs: author the declaration with the native change, use precise types, declare shared native/wire shapes once. Design Client model requires first-class languages to project the same durable fields and result semantics.

**Location:** `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:349` (`VerificationResultWire.subject`) and `:576` (`VerificationResult.subject`); producer `sdks/wyrd-sdk-python/src/verify.rs:221-224,498-507`; existing typed shape `python/wyrd/github/__init__.pyi:134`.

The shared native VerificationResult now serializes VerificationSubject; `GitHub.submit` returns the existing Execution, whose status serializes the result unchanged. The wire stub still promises `subject: CardRefWire` under status.result, so typechecking accepts Card fields such as subject['kind'] on a repository execution even though runtime sends only {'repository': ...}; the valid repository branch is absent from that type. The object-result getter has been widened to `CardRef | dict[str, Any]` instead of declaring the known closed repository arm, losing all field checking there. The existing RepositorySubject TypedDict already contains its exact six fields. Passing py:typecheck on current code does not exercise this new caller shape.

**Consequence/materiality:** a first-class typed SDK declares a supported result shape incorrectly; a caller following the stub can raise KeyError on a successful repository execution, and valid repository access is rejected or becomes Any. This is public declaration parity, not a preference about annotations.

**Smallest correction:** represent the wrapped repository arm once using the existing precise RepositorySubject shape, and use the proper Card/wrapped-repository unions in both the object getter and VerificationResultWire. Reuse/re-export the shared authored type without duplicating its fields or introducing runtime validation/behavior.

**Closure proof:** a small type-only fixture outside pytest collection exercises repository subjects from both `GitHub.verify` and `GitHub.submit(...).status()['result']`, including safe Card/repository narrowing; py:lints/stub checks and py:typecheck pass. Preserve runtime CardRef behavior and current Python journey assertions.

### REPO-004-JOURNEY-OWNERS — VIOLATION — Client journeys contain protocol and fixture machinery forbidden by their owner

**Authority:** `TESTING.md` Client-facing tests: checked-in YAML only; no raw HTTP, result JSON parsing, sleep or polling loop; fixture setup returns domain objects and the body acts on SDK/typed outcomes. Root AGENTS makes user journeys the contract. These restrictions apply to client-facing helper code too.

**Locations:** Rust `sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs:1349-1363`, Python `tests/integration/test_repository_verification.py:98-106`, TS `wyrd/tests/integration/repository-verification.test.ts:125-137` define three generic poll/sleep owners; their Check-read callers are Python `:382` and TS `:359` and multiple Rust stories. Rust also embeds full Card YAML in LOCAL_TREE (`:2327` onward) and constructs it in `asking` (`:2425`); raw Wyrd HTTP helpers/assertions occur at Rust `:190-211,2452-2474`; TS `:336-356` computes HMAC and posts raw HTTP directly in the SDK story; Python's signed_delivery helper (`:144`) does the same. Rust `:765-767,2640-2647` round-trips typed outcomes through JSON for assertions.

**Consequence/materiality:** these are new independently maintained asynchronous wait/protocol/fixture paths embedded in the files that are required to demonstrate the supported SDK workflow. They bypass the specified ownership boundary and duplicate the same polling machinery in all three clients. Full inline Card envelopes also escape the single checked-in fixture source, so these examples no longer meet the mandated public-journey contract even when their assertions pass. The protocol negative assertions are useful evidence and must be preserved.

**Smallest correction:** consolidate provider event setup/signing and bounded asynchronous observation in the existing `wyrd-testing` GitHub fixture/server owners and project only the narrow test controls the journeys actually need. Keep SDK actions and typed assertions visible in client journeys; use checked-in Card-tree fixtures for full local envelopes and existing typed fields for assertions. Put raw Wyrd HTTP status/invalid-wire assertions in the existing server HTTP/OpenAPI integration owner. Do not create a product API, generic wait framework, new fixture service, weakened tests or extra client transport. Preserve every existing negative assertion at its appropriate tier.

**Closure proof:** inspect all three journey files and their helpers for the listed forbidden machinery, then run their focused existing selectors and the focused HTTP/SQL proof affected by moving assertions. This is one common test-ownership correction, not separate stylistic findings per language.

## Verification and limits

Independently rerun: CodeGraph discovery twice; immutable diff/name/stat/source inspection; changed-item rustdoc inventory against candidate Git source; `mise exec -- cmp sdks/wyrd-sdk-ts/wyrd/index.d.ts sdks/wyrd-sdk-ts/wyrd/index.d.cts`; `mise exec -- git diff --check 8f1bb789e...487ccb0b6` (passed); `git rev-parse HEAD` confirms candidate. No Cargo/build/test/generator ran in this reviewer context.

Recorded task evidence reports fmt/lints, check:deps, check:tenant-isolation, codegen:check, Python lints/typecheck, TS typecheck, docs check and named journeys. The orchestrator separately communicated fresh PASS for nine Rust repository journeys, 24 OpenAPI tests, py:lints/stub checks, py:typecheck, ts:format:check, ts:lints and range diff check. These distinguish actual reruns from historical claims; they do not establish the missing schema or authored-type negative proof above. NAPI/codegen regeneration was deliberately not run against the immutable source; static mapping and recorded evidence were used. No live GHES appliance compatibility was checked or inferred.

No implementation edits, commits, pushes or deployment were performed. Findings are proposals for independent validation; this specialist does not issue task acceptance.
