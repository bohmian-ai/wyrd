# Repository standards review — R2

Result: **FAIL**, one proposed public-error-boundary regression, **REPO-R2-1**. Prior **FIND-TASK-004-8 is closed**. This is repository-rule coverage, not the task-acceptance verdict.

Immutable subject: `/home/thorrester/Documents/GitHub/wyrd`, incremental `0c2bfe70d5186dff4dc224c7e402b57c24f354d9..4478271030da79e7e2eb09fe108fbf1ed10145d8`; original base `9692bfd06c6bdf2585e88c791884a90669a5dc26` remains cumulative context. HEAD matched the candidate during inspection. Source and prior reports were not edited. The user-authorized working-tree skill update governs this review and is excluded from implementation scope. Only this report was written.

## Authority and scope

Reviewed the complete 22-file incremental diff, scope/contract applicability record, original TASK-004, superseded TASK-022, Bloom task, revision-27 specification and the prior R1 verdict, validated findings, standards and verification records, and R2 remediation/evidence. CodeGraph was consulted before source navigation, including the Scribe lifecycle, principal route and testing-control owners. No narrower AGENTS file governs the changed paths according to the supplied inventory. SQL rules were read to audit actual capabilities in affected dependencies; SDK implementation rules are relevant when tracing binding projections, though no binding source changes here.

Authority keys:

- **R:** root AGENTS; `agent_docs/index.md`, `maintainer-style.md`; `architecture/README.md`; `changes/spec-driven-development.md`. Every row below includes R.
- **RS:** `agent_docs/rust.md`, root testing rules and `TESTING.md`; applies to private and test Rust items as well as public items.
- **ERR:** `agent_docs/errors.md`, `agent_docs/agent-surfaces.md`; stable catalog errors through the existing HTTP mapper, typed public contracts and runtime OpenAPI.
- **SEC:** `architecture/wyrd-security-posture.md`, Principal and credential lifecycle / API keys, Authorization and policy, Postgres tenant binding, Audit integrity and privacy; `crates/wyrd/wyrd-sql/AGENTS.md` capability and transaction rules.
- **B:** `architecture/bifrost-design.md`, Durability and visibility and Scribe Live tail, recovery, and shutdown. The new observation and termination accessors are test-tier evidence, not revised production recovery semantics.
- **UI:** `.agents/skills/wyrd-ui/SKILL.md`, existing Observe/Chart patterns and Evidence Thread `brand/DESIGN.md`/component contracts. No token, CSS, font, geometry or generated theme changes occur.
- **WIRE:** `architecture/wyrd-design.md`, Client model and Runtime identity; Rust owns durable behavior and common typed wire contracts.
- **PKT:** approved spec revision 27, original task transfers, prior ledgers and R2. Historical implementation summaries do not waive repository gates or establish fresh execution.

Prefixes: `redux/` = `crates/vala/vala-bifrost-redux/`; `server/` = `crates/wyrd/wyrd-server/`; `testing/` = `crates/wyrd/wyrd-testing/`; `mcp/` = `crates/wyrd/wyrd-mcp/`; `ui/` = `server/wyrd-ui/`; `r1/` = `changes/active/wyrd-ui-main-integration/review/remediation-0c2bfe70d-r1/`.

## Complete incremental authority coverage

| Changed surface | Applicable authority and inspected standards |
|---|---|
| `r1/TASK-004-R2-recovery-boundary-and-histogram-selection.md` | R, PKT; preserved diagnosis and appended implementation evidence; distinguishes one restart and automated journey from human visual acceptance |
| `r1/claim-comparison.md` | R, PKT; historical review record, no executable or generated contract |
| `r1/domain-review-persistent-data.md` | R, PKT, B, SEC; prior source-evidence context, not current execution |
| `r1/findings-validation.md` | R, PKT; preserved finding IDs, source-backed F1–F6 closure and residual F7/F8/F9 context |
| `r1/followup-review.md` | R, PKT, B; retained readable-COMMIT versus completed-sync diagnosis |
| `r1/scope.md` | R, PKT; immutable prior review subject and bounded carry-forward |
| `r1/standards-review.md` | R, PKT; exact prior accepted-rule map and F8 residual |
| `r1/task-review-behavior.md` | R, PKT; historical acceptance evidence |
| `r1/task-review-invariants.md` | R, PKT; prior numeric/query/index invariant evidence |
| `r1/verdict.md` | R, PKT; prior independent reconciliation, not a new acceptance claim |
| `r1/verification.md` | R, PKT; records actual prior commands and their limits |
| `redux/src/scribe/mod.rs` | R, RS, B; existing lifecycle owner, abort/join/drain access is cfg(test), docs and no new runtime |
| `redux/src/scribe/shards.rs` | R, RS, B; retained join-handle access, existing post-sync fault owner publishes after retained state, test-support cfg boundary, docs |
| `redux/src/scribe/tests/scribe_persistence_path.rs` | R, RS, B; local test helper, no new harness, preserved row/context/retry assertions, CONTEXT rustdoc and helper/test panic docs |
| `redux/src/scribe/wal.rs` | R, RS, B; fault-state field and methods, watch producer/consumer, cfg(test)/test-support boundaries, no WAL format change |
| `crates/wyrd/wyrd-auth/src/passwords.rs` | R, RS, SEC; tenant-scoped fixture role seed, valid local User grant, unchanged limiter assertion and pg_tests placement |
| `mcp/tests/bifrost/mcp/connectivity.rs` | R, RS, ERR, SEC; unconditional read-tool discovery expectations now include actual principals.me registration |
| `mcp/tests/bifrost/mcp/discovery.rs` | R, RS, ERR, SEC; exact expected list/count grows by one actual read tool; no scope or schema assertion weakened |
| `server/src/components/principals/routes.rs` | R, RS, ERR, SEC, WIRE; authorization before delayed typed parse; TenantConn/RLS, staged audit, existing Wyrd mapper and explicit OpenAPI DTO; REPO-R2-1 below |
| `ui/src/lib/ui-pages.integration.test.ts` | R, UI, B, WIRE, SEC; existing real authenticated journey, visibility/group/apart/history assertions, no new skip or timeout increase |
| `ui/src/routes/t/[tenantKey]/observe/+layout.svelte` | R, UI, WIRE; existing host-owned picks/seriesKey and Chart shown/onseries seam, separate latency/rate choices, no new durable UI capability |
| `testing/src/server.rs` | R, RS, SEC, PKT; exact credential-fixture allowlist additions, actual helpers/binding declarations and three sanctioned control set inspected |

This is all 22 files: 11 historical packet files, nine Rust files, one TypeScript journey and one Svelte route. No manifest, production feature, SDK binding, migration, generated wire artifact or docs-site page changes incrementally.

## Rule results and evidence

| Applicable rule | Result | Evidence |
|---|---|---|
| Authority precedence, immutable source and reports-only directory | PASS | HEAD remains candidate; actual Git range inspected; no implementation edits, commits, patches or archives written |
| Wyrd vocabulary, tier imports/dependencies, durable server ownership | PASS | Changes extend existing Scribe, principal route and Observe owners; no new crate/dependency/feature, Python enablement, alternate durable frontend behavior or legacy alias |
| Actual SQL capability signatures/fields | PASS | `principals/routes.rs:113–151` returns `TenantConn` acquired with verified `caller.data_tenant_id`; `require_principal:174` owns that connection. No raw PgPool field/signature added. Existing Scribe retains OperatorPool/ValaPostgres owners; new helpers touch task handles/watch/IO lane only |
| Tenant isolation and transaction ownership | PASS for standards | Principal lookup and User-kind read use the same caller-tenant connection; permission and principal checks precede request deserialization; final commit remains route-owned. Foreign principal cannot change connection tenancy. Fixture seed uses its own TenantConn and commits at caller owner |
| Permission/audit boundary | PASS for standards | Existing `authorize` stages allowed/denied outcome through process Scribe outbox before opening TenantConn; self branch uses `audit::authorize`; admin, own-key and recent-human checks remain in their existing owners. No audit failure is made permission authority |
| Public body/errors/OpenAPI | FAIL, REPO-R2-1 | JSON conversion remains typed and declared DTO is unchanged, but bare Bytes introduces an uncaught extractor-error response under supported larger body limits |
| Typed normal-body validation and generated parity | PASS except extractor rejection | `Json<CreateUserCredentialRequest>::from_bytes` retains serde validation and existing body_rejection mapping; `request_body(Option<CreateUserCredentialRequest>)` and IssuedCredential response remain explicit. No schema/error enum/generated input changes; runtime OpenAPI, rather than codegen, is the applicable proof owner |
| Scrubbed tracing and secrets | PASS | issue_credential instrumentation skips body, state, caller and recent; plaintext response still uses no_store and existing issuance owner. New test observation contains slice identity, not credentials |
| Struct-centered ownership/sync/platform rules | PASS | Stateful test access is inherent on ScribeImpl/ScribeShardRuntime/WalWriter/WalHandle; caller-owned crash helper is free. Async methods await existing task/IO owners. No runtime, forbidden platform mechanism or OS cfg added |
| Feature boundary integrity | PASS | WalFaults and publisher are cfg(any(test, feature="test-support")); post-sync injection block has same cfg. Watch subscriber, handle-taking and abort/settle accessors are cfg(test). Production no-feature code has none of this field/work; no shipped test-support enablement introduced |
| Test validity and no gate weakening | PASS by source | Crash helper positively awaits held slices and keeps existing replay/context/retry assertions. Password fixture seeds a real viewer role instead of empty grants; refusal assertion remains. MCP compares exact real tool set. Setup allowlists add four concrete credential-fixture names, with sanctioned control lists unchanged |
| Mandatory changed-item rustdoc | PASS; F8 CLOSED | Complete item inventory below; CONTEXT now explains representative accepted-context membership. No lint suppression substitutes for docs |
| UI state, accessibility and tokens | PASS by source | Only existing shown/onseries bindings and state documentation change; semantic checkbox/link controls and escaped rendering remain with Chart/host owner. Existing selection map now keys each histogram plot separately. No styling or keyboard behavior replaced |
| Required verification | Supplied evidence retained; fresh runtime execution owned by root | Appended R2 evidence records five focused Rust cases, unit22, filtered Metrics journey, UI check/build, fmt and both redux clippy feature configurations. This reviewer ran only static/read-only commands; these claims are not independent reviewer execution |

### F8 and complete changed Rust-item documentation inventory

- `ScribeImpl::abort_and_settle_for_test` (`mod.rs:1549–1563`) documents crash-style abort, skipped graceful flush, owner termination, IO completion and the safe reopen invariant. It is infallible to its caller and introduces no panic-producing assertion.
- `ScribeShardRuntime::take_owners_for_test` (`shards.rs:992–997`) documents taking retained join handles for termination observation. Existing `process_group` docs cover its workflow/stages and error outcomes; new fault publication stays within its existing test-only retained-state branch.
- `WalFaults::post_sync_held` (`wal.rs:1493–1496`) documents held group slices, completed sync, unanswered admission and initial empty value. Existing WalFaults struct docs describe shared test-only injections.
- `WalHandle::hold_post_sync_for_test` (`wal.rs:2092–2102`) names the retained post-sync producer and links to subscriber. `WalWriter::post_sync_held_for_test` (`:2346–2357`) documents why observing a slice proves sync/retained ownership. Both operations are infallible and have no expected panics.
- `CONTEXT` (`scribe_persistence_path.rs:1301–1303`) now explains that all four sampled accepted-context fields must match; enclosing helper retains its intent and panic docs.
- Renamed/reworked `crash_once_held` (`:1534–1559`) describes target selection, retained completed-sync state, unanswered ingest, abrupt termination and IO barrier; its `# Panics` names early response/watch closure. Both changed interruption tests update their invariant docs and retain existing `# Panics`.
- Changed `issue_credential` (`routes.rs:360–398`) retains meaningful Service/User workflow, permission/recent-login requirements, bounds and `# Errors`; tracing now scrubs the new raw argument. The error-path defect below is not a missing-rustdoc claim.
- Changed queued-login test (`passwords.rs:986–994`) retains fixture/queue-window purpose and accurate fixture/refusal `# Panics`; new expect sites are fixture setup already described there.
- `ADVERTISED_TOOLS` (`discovery.rs:28–32`) documents read/write set and required caller grants; count now matches real catalog. `PYTHON_SETUP` and `TYPESCRIPT_SETUP` (`server.rs:6191–6242`) document credential/server setup membership. No new function/type/variant is introduced by these test list edits. Connectivity's existing test docs remain applicable.

### Credential fixture and MCP gate integrity

The allowlist change is narrow: `bootstrap_user_key` (`testing/src/server.rs:3105`) creates a valid local User, signs in through production password login and issues a ceiling-bound key through the production personal-key route. `seed_user_keys` (`:3168`) inserts nonexchangeable metadata fixture keys at one timestamp for paging ties and returns IDs. Both are credential fixture preparation, consistent with existing bootstrap/scoped-key allowances. They are not an alternate flush, baseline, binding-due or state-inspection control. Python projection (`sdks/wyrd-sdk-python/src/testing.rs:410–437`), TypeScript declaration (`sdks/wyrd-sdk-ts/testing/native.d.cts:147–157`) and actual integration callers name those same operations. The allowlist retains exact explicit names; no wildcard or assertion deletion appears.

`server/src/mcp/principals.rs:44–59` defines ME and includes `me_tool()` in descriptors_unscoped alongside list_credentials. Adding it to connectivity/discovery expected read tools restores actual unconditional read-tool discovery rather than hiding or removing a tool. Existing schema/call authorization tests remain. No tool registration, permission gate, mutable capability or generated catalog changes.

## Proposed finding

### REPO-R2-1 — Raw body extraction can bypass the stable Wyrd error boundary

**Classification: REGRESSION / VIOLATION.** New proposal; independent validator must decide its stable FIND ID.

**Obligation:** root AGENTS requires server handlers to return WyrdError and primary agent surfaces to expose stable errors. `agent_docs/errors.md` requires HTTP public errors through the single Wyrd problem mapper, and `agent_docs/agent-surfaces.md` requires stable codes. This is an explicit public-contract rule, not a preference for extractor syntax.

**Changed location:** `server/src/components/principals/routes.rs:403`, `body: axum::body::Bytes`.

**Source-proven scenario:** configure supported `limits.body_bytes` above 2 MiB, for example 4 MiB, then make an authenticated credential-issuance POST with body length 2 MiB + 1. `config.rs:2364–2366`, `:3188–3194` and `:3466–3474` expose/validate that configuration without a 2 MiB upper bound. Public router `http/router.rs:321–322` applies WyrdBodyLimit with that configured ceiling. `http/middleware/body_limit.rs:188–207` therefore successfully buffers this body, rebuilds Body::from(bytes), and calls the handler service. The global layer does not disable Axum's separate default extraction limit. Bare Bytes then rejects at Axum's 2 MiB default before issue_credential executes, so the handler's WyrdErrorResponse return and delayed body_rejection mapping never run.

Installed exact dependency source verifies this: axum-core 0.5.6 `src/extract/request_parts.rs:100–114` gives Bytes a BytesRejection and calls into_limited_body().collect(); `src/ext_traits/request.rs` supplies the 2 MiB default; `src/extract/rejection.rs:38–45,70–73` turns LengthLimitError into an ordinary 413 rejection. Axum's rejection IntoResponse is text/plain, not Wyrd problem+json. Previously the route captured `Result<Option<Json<CreateUserCredentialRequest>>, JsonRejection>`; with application/json, Json captures the same Bytes failure as JsonRejection and the handler maps it through `http/error.rs:103–108` to canonical WYRD_SPEC_400_VALIDATION.

**Consequence/materiality:** under a supported server configuration, an authenticated client receives an uncatalogued plain-text refusal from this public credential endpoint, losing the stable problem fields used for repair/classification and regressing its existing error contract. Default 1 MiB deployment is protected by WyrdBodyLimit and does not exercise this scenario; ordinary stream IO failures are also handled by that outer layer. The finding is retained for the explicit stable-error requirement, not a claim that large credential payloads are a normal workflow or that the global limiter is absent.

**Smallest correction:** retain bounded buffering, authorization/principal ordering, typed DTO parse and current empty-body Service behavior. Capture BytesRejection and route any extraction failure through the existing Wyrd error boundary after the intended access check: `JsonRejection::from(bytes_rejection)` feeds the existing `http::error::body_rejection` mapper (JsonRejection already contains BytesRejection). A matching capture pattern already exists in `components/gateway/routes.rs:1112` (`Result<Bytes, BytesRejection>`); its gateway-specific error projection must not be copied. No new transport/parser/limit setting, broadened limit or feature is required. This is a local handler error conversion, not a new shared framework.

**Focused proof:** exercise this mounted endpoint with an authenticated principal and configured body limit above 2 MiB, and a body exceeding Axum's 2 MiB extractor limit while below the configured ceiling; assert canonical problem+json/code rather than text/plain. Preserve refused/foreign principal precedence and normal absent Service / typed User issuance cases. Static source trace establishes reachability; no fresh runtime reproduction was run by this reviewer.

## Carry-forward and limits

Carry forward only the exact unchanged standards rows from `r1/standards-review.md` (Authority coverage, Applicable rule results) and its original `range-9692bfd06-ed82f63bf-20261104/standards-review.md`: dev workload, sort/paging, integer transport, fingerprint/tagged WAL, Bloom/fork/dependency, unchanged numeric/context encoder, generic Chart and noncredential permissions. Their owners, generated inputs and dependencies are untouched in this range. Histogram host binding, new fault observation/termination proof and credential ordering were reviewed directly; no blanket prior acceptance was used for them. New credential security coverage is assigned independently to the selected domain reviewer.

`git diff --check 0c2bfe70d 447827103` independently passed. No Cargo command, browser, DB, build, full gate or performance workload was run by this reviewer; root owns costly verification. No generated input changes, so no newly required regeneration claim. Human visual acceptance, prior latency/AC040 uncertainty, and the one-restart retained-WAL fixture limit remain disclosed. No additional gate, second restart or unrelated cleanup is proposed.

At finalization root separately reported fresh selected checks passing: UI 22, recovery/post-sync 5, credential refusal 2, lifecycle 1, queued password 1, sanctioned controls 1, MCP 3, and filtered browser 1 after current native rebuild. Root confirmed source unchanged except the pre-existing skill edit. These strengthen the corresponding scoped evidence; none reproduces or disproves the configured-above-2-MiB extractor rejection identified here.

Skipped fresh runtime/build/visual execution; the conditional extractor-error regression is source-proven and awaits independent validation.
