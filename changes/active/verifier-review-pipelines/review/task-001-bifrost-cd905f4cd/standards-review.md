# Repository standards review

Overall: **FAIL**.

Subject: repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`, immutable first-review range `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee`. HEAD was verified as the candidate. All 60 changed paths were included; no earlier acceptance was carried forward. Architecture and change-packet working files are the supplied authority inputs, not candidate source changes. This report is independent of the other discovery reports.

## Authority coverage

Root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md`, `agent_docs/rust.md`, `agent_docs/errors.md`, `agent_docs/agent-surfaces.md`, `agent_docs/maintainer-style.md`, and `TESTING.md` apply across this review. The architecture authority map was read first. CodeGraph exploration preceded source searches. No other nested AGENTS applies to these paths beyond the three named below.

Paths in each row are relative to the named prefix. Each row covers the source changes, relevant surrounding signatures, and changed test/helper documentation, rather than treating a green lint claim as proof.

| Changed surface | Paths | Additional owning authority and coverage |
| --- | --- | --- |
| Bifrost contracts/Gate | `crates/vala/vala-bifrost-redux/src/{contracts.rs,gate/error.rs,gate/mod.rs}` | Bifrost design, Durability and visibility / Ingest; Wyrd security, tenant isolation and internal peers; Rust/error guides. Reviewed state-fence carriage, refusal conversion, and ordinary ingress's explicit absence of a claim. |
| Scribe production | Same prefix, `scribe/{ingress.rs,mod.rs,preprocess.rs,replay.rs,series_index.rs,shards.rs}` | Bifrost design, Durability and visibility / Scribe append, replay and recovery; operations reliability, Scribe failure boundaries. Reviewed new fields, propagation, tenant transaction, refusal removal, replay suppression, and surrounding error handling. |
| Bifrost authored tables | Same prefix, `tables/{mod.rs,verification/execution_events.rs}` | Bifrost design, table/row identity and declared types; Wyrd design, Verifier state. Reviewed authored state-fenced constant, built-in metadata projection, sensitive payload marking, retained column names and revised meanings. |
| Scribe inline tests | Same prefix, `scribe/tests/{scribe_persistence_path.rs,wal_closeout.rs}` | Rust and testing guides. Changes add `state: None` to existing test frames; no new external test binary or weakening of assertions. |
| Vala SQL | `crates/vala/vala-sql/{migrations/20261107000000_scribe_state_heads.sql,src/queries/mod.rs,src/queries/scribe_batch_commits.rs,src/queries/scribe_state_heads.rs,tests/integration/pg_scribe_batch_commits.rs,tests/integration/pg_stream_identity.rs}` | Wyrd security, Postgres tenant binding; `crates/wyrd/wyrd-sql/AGENTS.md` explicitly describes the SQL capability boundary on which vala-sql builds; operations migration contract. Checked actual capabilities, caller-owned commit, forced RLS and policies, timestamps, column mapping, new tests and helpers. |
| Pure state/error contract | `crates/wyrd-spec/src/{vala/error.rs,verification.rs,verification/state.rs}` | Wyrd design, Verifier / error catalog; pure-spec dependency boundary; Rust/error and generated-surface guides. Reviewed version discriminator, serde shape, new fields/variants, decoding test, derived error metadata and consumers. |
| Direct execution and inspection | `crates/wyrd/wyrd-server/src/components/{gateway/capture.rs,verification/service.rs,verification/service/inspection.rs}` | Wyrd design, Run entry points / Verifier dedicated reads; Wyrd security, authorization, tenant binding and audit; Rust/test guides. Checked direct claim call chain, existing authorization/audit owner, internal read table set, cursor projection, typed responses, and moved test Scribe. |
| Peer and shared outbox | `crates/wyrd/wyrd-server/src/{grpc/capture_peer.rs,scribe_outbox.rs}` | Wyrd security, peer identity; Bifrost internal ingress and bounded writer; errors/agent surfaces; operations failure boundaries. Checked protobuf parsing/encoding parity, frame fields, byte accounting, per-frame receipts, lane identity, refusals and test helpers. |
| Run state and runtime | `crates/wyrd/wyrd-server/src/verification/{state.rs,runner.rs,inputs.rs,leases.rs,results.rs,mod.rs,history.rs}` (`history.rs` deleted) | Wyrd design, Verifier state / coordination clock / result assembly; Bifrost accepted state authority; security SQL boundary; operations Verification execution state and recovery. Checked new owning structs, methods and fields, SQL access, exact-row reader, result identity, checkpoint construction, removed projector composition and changed helper documentation. |
| Hosted adapters | Same prefix, `hosted/{blocks.rs,evidence.rs,publications.rs,workspace.rs}` | Wyrd design, Agents/tools/workspaces and File publications; security storage/tenant boundary; Rust ownership/errors. Checked RunState propagation and publication/descriptor type replacement, existing File completion owner and typed workspace consumers. |
| Server integration tests | `crates/wyrd/wyrd-server/tests/integration/{pg_durable_verifier_work.rs,pg_grpc_ingest_smoke.rs,pg_hosted_artifacts.rs,pg_verification_runtime.rs}` | Rust/testing guides and authority of each tested runtime. Reviewed new/changed helpers and test assertions, real server/Postgres placement, runtime outcome-selection assertion changes, and replacement reads for removed SQL owners. |
| Queue SQL and migration | `crates/wyrd/wyrd-sql/{migrations/20261107000000_verifier_execution_state_in_bifrost.sql,src/queries/verifier_executions.rs,src/queries/verifier_executions/events.rs,src/queries/verifier_executions/inspection.rs,src/queries/verifier_executions/publications.rs,tests/integration/pg_verifier_executions.rs}` (three child source files deleted) | Nested SQL AGENTS; security Postgres tenant binding; operations migration and execution-state recovery. Checked remaining production signatures and worker-format/deadline projections, immutable additive migration, old-custody guard, dropped owners and tests, and replacement queue deadline/format proofs. Detailed migration runtime correctness is also assigned to the data specialist, not inferred from this structural review. |
| Shared test owner and Bifrost journeys | `crates/wyrd/wyrd-testing/{Cargo.toml,src/verification.rs,tests/bifrost/oracle/peer_network/listener.rs,tests/bifrost/scribe/lifecycle.rs,tests/bifrost/server/verification_runtime.rs,tests/gateway/peer.rs}` | Rust/testing guides; Bifrost recovery and internal peer authority. Reviewed new dependency, fixture claim/read/lease expiry, retained-volume lifecycle proofs and helpers, ordinary peer requests, and removed projector-only journey. |
| Internal wire | `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` | Wyrd design wire ownership and security internal peers; agent-surfaces generation. Existing reserved tags remain; optional message uses new tag 10, with key/writer/revision/bound mirrored at both Rust transport ends. Prost source is generated by the existing build, not committed by hand. |
| Docs and TS projection | `docs/{public/llms-full.txt,src/content/docs/api/errors.md}`, `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | Nested docs and TS AGENTS; docs CONTRIBUTING/PRODUCT; error/agent-surfaces guides. Checked StateRefused catalog code/title/remediation parity and existing generators; verified llms-full matches a fresh in-memory rendering without writing artifacts. New ExecutionState is internal recovery payload, not an added SDK wire root. |
| Rust SDK journey | `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs` | Root AGENTS and TESTING client-facing journey rules. Reviewed real SDK verify/submit/handle reconstruction assertions and existing lifecycle lane's ignored-by-default convention. |
| Dependency resolution | `Cargo.lock` | Root Cargo/layer restrictions; Rust build/features guide. Only new test-owner vala-verify dependency resolution accompanies its workspace dependency; no new package version, feature, binding enablement or client-to-server dependency. |

## Applicable rule results

| Applicable rule | Source evidence | Result |
| --- | --- | --- |
| Run entry points do not write PostgreSQL on request path (`architecture/wyrd-design.md:1801`) | Direct `service.rs:568` awaits `RunState::fresh`; `state.rs:399` calls claim, which inserts/updates a head and commits at `state.rs:341–353`. | **FAIL**, STD-003 |
| Every new/materially changed Rust item, including tests/helpers, has meaningful rustdoc; fallible items have Errors and panic-capable items have Panics | Most new RunState fields, transitions and tests are documented; named omissions below remain in pure-contract test and SQL/server/lifecycle test helpers. | **FAIL**, STD-001 |
| SQL takes TenantConn for tenant work and OperatorPool for cross-tenant work, with no bare pools/transactions in production signatures | New state-head `claim`, `head`, `advance` take `&mut TenantConn<'_>` at lines 66, 104, 133. Batch `record` retains TenantConn. Remaining queue methods at `verifier_executions.rs:2817–3274` use TenantConn; discovery/telemetry at 3320–3402 use OperatorPool. RunState stores AppState/StateScribe, not a raw SQL capability. | PASS |
| TenantConn callee never commits/rolls back or opens nested transaction | State-head and batch-fence queries execute through supplied transaction. RunState claims commit their locally acquired wrapper; Scribe's fence caller owns the batch transaction (`shards.rs:4163` onward). | PASS |
| No manual tenant predicate on TenantConn paths; map columns by name | New state-head reader/update repeat the RLS predicate and all three new queries decode tuples by ordinal. | **FAIL**, STD-002 |
| Every new tenant table has tenant FK, ENABLE/FORCE RLS, both tenant predicates and migration-login-scoped operator policy | `20261107000000_scribe_state_heads.sql:10–33` supplies all. No new security-definer bridge or grant. | PASS |
| PostgreSQL owns coordination time | State advance compares bounds with `statement_timestamp()` (`scribe_state_heads.rs:148`); queued writer receives SQL-derived original `deadline_at` (`runner.rs:489`). Result start/end and row occurrence times remain domain event facts. | PASS for changed timestamp representation |
| Immutable, ordered migrations; old-format custody not silently reinterpreted | Two new migration files; no old migration edits. SQL payload removal checks old invocation/publication custody before dropping owners and moving format-1 rows to format 2. Structural migration rule met; complete rollout behavior belongs to data/system acceptance review. | PASS for structural standards |
| Durable server behavior remains server-owned and spec remains IO/async/PyO3/heavy-dependency free | `wyrd-spec/verification/state.rs` contains data and serde/version decoding only; new SQL behavior remains in Vala SQL/Scribe, runtime in server; SDK only adds journey and generated catalog. | PASS |
| Across-tier imports use owning tier re-exports | New Vala SQL imports its crate's TenantConn/SqlError; server receives state-head owner through `vala_bifrost_redux::contracts`; hosted adapters use spec declarations. No new direct underlying SQL dependency from client tier. | PASS |
| Struct-centered retained ownership, no added utility trait or runtime | RunState owns live progress and StateScribe its retained writer dependencies. Existing Checkpoints/Scribe traits are implemented; no new ad hoc Tokio runtime, platform gating, or feature introduced. Whole-workflow duplication assessment is assigned to maintainer review. | PASS for structural rules |
| Public errors use derive-backed stable catalog, converted at boundary | `BifrostError::StateRefused` metadata at `wyrd-spec/src/vala/error.rs:515–530`; IngestError maps to it, private StateError maps through WyrdError for public inspection/direct failures. No manual catalog API or TS-only error. | PASS |
| Typed wire and consistent generated projections | Proto optional state at tag 10 is encoded in `scribe_outbox.rs:913` and parsed in `capture_peer.rs:86`; normal constructors explicitly use None. Error string matches generated TS/docs. Existing generator `gen_schemas.rs:110` owns the TS union, docs generator owns catalog/reference. | PASS by source inspection; full codegen not independently run |
| Generated docs and neutral reference page rules | `errors.md` changes one generated row. `llms-full.txt` additionally catches up existing publication-limit reference text; `generate_llms_txt.render_llms_full() == committed text` returned True. | PASS |
| Test tier/placement and real-dependency boundary | New pure state/outbox tests are inline; PostgreSQL tests are pg_*; lifecycle/SDK journeys live in established real-server binaries and use existing explicit journey profile. No new test binary or silent environment skip. | PASS |
| No weakened gate to hide a failure | Deleted projector tests cover deleted payload machinery. Changed runtime assertions express terminal selection preservation and accepted-state requirement; task evidence explicitly describes the semantic replacement. No added allow, ignored test weakening, or narrowed boundary glob found. New journeys use existing lane selection. | PASS for changed-source gate discipline; not an independent rerun |
| Cargo layering/features and platform parity | One workspace vala-verify dependency in wyrd-testing and its lock entry; no production SDK/heavy dependency leakage, wildcard or profile addition, new python feature enablement, or OS-specific path. | PASS |
| New source comments avoid task/review/session attribution | New state, SQL, fixture and test additions describe lasting workflows. Historical references outside materially changed comments were not treated as newly introduced debt. | PASS |

## Proposed findings

### STD-001 — Missing mandatory changed-item rustdoc

Classification: **VIOLATION**. Root AGENTS, Code, and `agent_docs/rust.md`, Rustdoc, explicitly make documentation a merge gate even for private test helpers and tests.

Source-proven omissions in newly added items:

- `crates/wyrd-spec/src/verification/state.rs:139`, `only_the_current_format_decodes`, has no rustdoc at all despite introducing the format compatibility/absence/malformed contract test.
- `crates/vala/vala-sql/tests/integration/pg_scribe_batch_commits.rs:402`, `claim_state`, lacks Panics; it explicitly expects acquisition, query and commit success at lines 411, 414, 415.
- `crates/wyrd/wyrd-server/tests/integration/pg_durable_verifier_work.rs:155`, `Leased::writer`, documents Errors but not Panics; it invokes `Self::query` (which explicitly documents a failing read-authority panic) at line 163 and panic-capable renewal registration at 164.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:619`, `take_over`, returns `Result<RunState, StateError>` but omits Errors; it deliberately returns the inner claim error at line 632, including unavailable/incompatible/superseded state rather than converting all failure to panic.
- `crates/wyrd/wyrd-server/src/verification/state.rs:834`, `held_state`, lacks Panics; it creates the outbox through the existing constructor that spawns immediately and panics without a Tokio runtime (`scribe_outbox.rs:1078–1096`), and calls panic-capable `uid`.

These are actual changed items and reachable in their named tests, not hypothetical production panics. The consequence is violation of the explicit merge gate and incomplete fixture/format failure contracts. A clean Clippy claim cannot waive the documented rule.

Smallest correction: document these items at their existing owners with the behavior and relevant Errors/Panics sections, and include other materially changed helpers in that same bounded documentation pass. No new helper, abstraction, test harness, behavior change or separate style-only remediation round. Closure proof is direct rustdoc/source inspection plus the narrow format/lint lane already needed for the remediation; no new behavioral test is necessary for prose.

### STD-002 — New state-head queries bypass two explicit SQL authoring rules

Classification: **VIOLATION**. The SQL owner requires “NEVER add a manual data_tenant_id predicate on a TenantConn path” and “Map columns by name, never by position”; `agent_docs/rust.md` repeats named mapping when schemas can diverge.

All affected production paths share the new owner `crates/vala/vala-sql/src/queries/scribe_state_heads.rs`:

- `head`, line 111, adds `WHERE data_tenant_id = wyrd.current_tenant()` even though it takes TenantConn and the new table's tenant policy already supplies it.
- `advance`, line 143, duplicates the same predicate for `head.data_tenant_id`.
- `claim` lines 73/89, `head` lines 109/118, and `advance` lines 137/179–185 extract `query_as` tuples by position. The returned logical-table string and batch UUID are the acceptance classification inputs in `advance`, not disposable test values.

Reachable chain: runner/direct claim and inspection invoke claim/head; every fresh state-fenced batch invokes advance from batch `record`. This does **not** establish a tenant leak today: the migration correctly forces RLS. It does establish violation of explicit mandatory authoring rules at a new shared SQL owner; tuple mapping also makes accepted-position and accepted-batch interpretation rely on query column order instead of names.

Smallest correction: leave the existing TenantConn/RLS ownership intact, remove only redundant tenant WHERE filters, retain required insert/conflict key columns, and decode the returned columns by name with the existing SQLx mechanism. The nearby `ExistingScribeBatchCommit` named `FromRow` pattern is already available; no new SQL abstraction, alternative capability or privilege path is warranted. Do not remove logical-table/state-key/writer/revision/deadline guards. Closure proof: inspect the corrected signatures/queries and rerun the existing state-fence integration selector (including cross-tenant refusal), without broadening the test harness.

### STD-003 — Direct verify performs a synchronous PostgreSQL state-head write

Classification: **VIOLATION**. `architecture/wyrd-design.md:1801–1804` says neither Run entry point writes Postgres on the request path and identifies the shared outbox handoff. Root authority precedence puts this owning architecture above private task mechanics.

Reachable production path on every otherwise admissible direct verify:

1. `VerificationControl::run_direct`, `service.rs:568`, awaits `RunState::fresh` before constructing/running the engine.
2. `RunState::fresh`, `state.rs:399`, directly awaits `Self::claim`.
3. `RunState::claim`, `state.rs:335–353`, acquires a tenant connection, calls state-head claim and commits.
4. The SQL owner executes `INSERT INTO vala.scribe_state_heads ... ON CONFLICT ... DO UPDATE` (`scribe_state_heads.rs:74–80`).

This is a write-and-commit request dependency, not the permitted ordinary registry read or a background Scribe batch fence. A fresh per-request UUID does not avoid the write. The direct/queued proof's assertion that direct verify creates no queue row does not test the broader no-Postgres-request-write contract. The meaningful consequence is a required synchronous SQL mutation/commit round trip on every direct request, with request progress and failure coupled to it, contrary to the approved shared-outbox admission boundary.

Smallest correction boundary: preserve RunState, the existing Scribe acceptance owner and current required writer/revision fencing; remove request-owned direct head registration and arrange fresh direct-state registration/acceptance through the existing batched Scribe handoff. Queued takeover/recovery and accepted inspection must retain their existing authority semantics. Merely removing the claim or granting fresh direct writes unfenced acceptance is insufficient. The existing head/fence transaction owner is the reuse target, not a new writer or queue.

If moving initial direct registration to that owner requires a new acceptance/API decision beyond the approved packet, resolve the conflict through spec/authority revision instead of claiming the no-request-write rule means only “no queue row.” This report does not authorize that decision. Focused closure proof must distinguish direct requests from Scribe work and demonstrate direct state initialization performs no request-owned INSERT/UPDATE/commit while its accepted checkpoint still has one writer, ordered revisions and the same outbox acknowledgement boundary.

## Verification and limits

This reviewer ran no Cargo commands and mutated no source. The implementation packet contains historical format/lint/codegen/docs/boundary claims and Revision 1 test claims; these are recorded claims, not independent execution evidence. Source checks included actual SQL signatures, migration policies, call chains, transport/generator parity and changed-item documentation. `git diff --check` on the immutable range passed. The existing llms generator rendered entirely in memory and matched `docs/public/llms-full.txt` exactly. HEAD remained the candidate.

Task acceptance, concurrent fencing behavior, security exploit analysis, migration recovery behavior and whole-workflow complexity have their assigned specialists. The standards coverage above is complete for all touched languages/layers; no Python binding or UI surface changes exist in this range. The static standards failures remain independently actionable even if orchestration proof is green.

Skipped: Cargo/test/codegen reruns, as assigned to the orchestrator; no source edits.
Risk: STD-003 may expose an authority/spec decision rather than a routine local implementation fix; preserve fencing while resolving it.
