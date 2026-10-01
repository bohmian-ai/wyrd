# Repository standards review — TASK-008-R1

## Subject and limits

Candidate `23eafa368bca19208faf8311eb7b5421e3660b38`; correction parent `6e7add054e33701ca5ecb52a5c859948b15161a3`; TASK-008 immediate base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; original TASK-007 base `a7582db587c6170a290760f1741673125612b797`. Reviewed cumulative TASK-007/008 source and latest correction, excluding unrelated TASK-006 benchmark/lifecycle changes as directed. Static independent review only: no Cargo runs, Postgres wrappers, full lanes, source edits, or commits. HEAD remains candidate.

Current maintainer decisions govern: FIND-007-3 remains unchanged; Postgres tenant columns are excluded; stable code is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`. Older reference statements about row tripwires, audit WAL and Forge estimates do not override current Bifrost architecture and explicit decisions.

## Authority coverage

Read AGENTS.md, agent-rules.md, reference router, spec-driven-development and maintainer-style; applied Wyrd design doctrine/client/Bifrost authority, wyrd-doctrine.mdx, Bifrost design, applicable security posture, TESTING.md and approved revision 20/tasks/remediations. Complete routed focused references: doctrine/architecture-constraints; architecture/patterns; languages/rust-core, errors, testing-workflows, implementation-execution; domain/olap-serving, iceberg, datafusion, arrow-analytical-interop, analytical-operations-reliability.

| Changed surface | Applicable rules and routed authority | Source coverage |
|---|---|---|
| Footer tenant proof, shared Parquet scan and cache | AGENTS ownership/Rust/async/tenant/doc rules; errors; patterns; Arrow/DataFusion/Iceberg/OLAP/reliability | parquet/footer.rs, writer_properties.rs; storage/cache.rs; oracle/exec.rs PublishedFooterLoader, tenant_proven_reader_metadata, HotParquetExec; follower source construction |
| Scribe/Forge producer bindings | Same Rust/ownership/Arrow/Iceberg authorities; Bifrost stage/rewrite boundaries | scribe/parquet_writer.rs, claim_assembly.rs, staging_runtime.rs, member_stager.rs, execution_lanes.rs; forge/managed/policy.rs and executor.rs |
| Envelope/contracts and permanent field IDs | AGENTS pure-spec/server contracts; patterns; Arrow/Iceberg identity | wyrd-spec vala/{managed_columns,mod}.rs; redux schema/managed_columns.rs; tables/{managed_columns,mod}.rs, drift/result_features.rs, eval/result_items.rs, gateway/calls.rs; contracts.rs; catalog/layout.rs |
| Catalog/deleted provider | AGENTS owner/method rules; DataFusion provider and tenant authority | catalog/bifrost_catalog.rs; removed provider/filter/table modules; native IcebergStaticTableProvider composition |
| Native/remote fragment completion | AGENTS Rust/async/runtime/test/doc rules; errors; Arrow/DataFusion/reliability; listener ownership | oracle/{dispatcher,live,follower}.rs; resources.rs; server oracle/peer_service.rs |
| Audit/error routing and collaborator removal | AGENTS canonical audit/error rules; security posture; patterns/errors; Bifrost terminal authority | oracle/mod.rs, query_stream.rs, codec.rs; analytical/analytical_transport constructor consumers; server boot/mod.rs, state.rs; wyrd-spec audit_detail.rs |
| Generated audit JSON | AGENTS generated-output/codegen rules; contracts/testing | both bifrost_audit_event.json copies match TenantFile discriminator |
| Dependency/native version cone | AGENTS narrow dependency/features/version rules; Rust/DataFusion | workspace Cargo.toml/lock and redux manifest; existing Iceberg revision/opendal-fs engine dependency |
| Tests and evidence | AGENTS §§11–12/14/16; spec-driven-development/testing-workflows/implementation-execution; TESTING.md | inline footer/schema/scan/follower/native/producer proofs; server peer-service tests; distributed journey evidence; original/R1 recipes and final-named.log |
| Architecture/docs | Wyrd doctrine/design; Bifrost design; OLAP/DataFusion; AGENTS docs gate | Bifrost envelope/footer paragraphs, routed reference edits and seven docs bifrost pages |

No Python/PyO3, TypeScript/N-API, UI or MCP implementation/public SDK signature changes enter bounded source scope; their binding-specific declaration standards add no source review here. Generated audit JSON is the cross-surface projection.

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Durable engine/server/spec/client ownership | PASS | Vala owns scans/footer/native accounting; server owns wire adapter/listeners; spec edits remain pure; no client analytical dependency. |
| Typed/authenticated tenant and SQL boundary | PASS | DataTenantId is retained; producer tenant comes from seal/table binding; no raw pools/RLS predicates/migration expansion. |
| Module imports and bare changed declaration types | PASS | exec.rs imports DataTenantId/ArrowReaderMetadata, test module imports WriterProperties; writer_properties.rs, policy.rs, claim_assembly.rs import DataTenantId. Corrected declarations preserve identity. |
| Cohesive owning structs/earned async | PASS | ScribeTailResolver owns FileIO/storage; producer owners remain; NativeOutputTally/LiveFrameDecoder own state; pure footer compare is deterministic; live_leaf awaits filesystem IO. |
| Stable errors and canonical audit | PASS | Existing QueryTenantInvariant source survives peer/reader classification; Oracle::audit_tenant_refusal owns one leader TenantFile event; no second writer or error code. |
| No compatibility/row tenant mechanism | PASS | Row column, tripwire/provider/codec removed; no missing-footer fallback; retired field IDs retained. |
| Substantive rewritten Rust docs/panic contract | PASS | Schema test panic docs match canonical field order, tenant absence and principal/request nullability assertions; native/partition/footer proof docs explain their contracts. |
| Native test runtime/tier and exact named recipes | PASS, static | Unit tests inline; real-server refusal in wyrd-testing; exact section gives package/features/names and journey target/environment/profile. Concrete table rows determine substitutions without test discovery. |
| Generated parity/docs checks | PASS, evidence limit | Both JSON projections match source; task claims regenerated/codegen/docs passes, not rerun here. |
| Format/lint and no weakened gates | PASS, evidence limit | R1 only imports/docs/evidence; no suppressions/weakened assertions. Root independently reports 13/13 exact redux tests, scoped fmt and test-support all-targets Clippy passing. |
| Whitespace/final diff audit | FAIL proposal | Working-tree check passes; actual immutable correction/cumulative ranges fail on committed review artifacts. |

## Prior closure

**FIND-007-4 closed.** All validated tenant declaration sites plus reader metadata and fixture WriterProperties return now use owning module/test-module imports and bare names. Qualified constructor/value expressions remain permitted. No runtime ownership, feature gate or value changes.

**FIND-007-5 closed for named proof precision.** Integrated-tree evidence supplies thirteen engine identities, three server identities, and nine journey target/name pairs with package/features/profile/setup recipes. final-named.log gives one selected/one passed for each and migration setup. Current lead proof is distinguished from earlier implementer evidence; full lane/benchmark remain caller-owned. No invented RED run. The old scratchpad log path is superseded by the user-provided durable review log and does not prevent reproducing the recipes.

**FIND-007-6 closed.** schema/managed_columns.rs:48–55 now describes actual field-order/no-tenant/non-null identity assertion panic conditions, retaining assertions unchanged.

## Proposed finding

### RSTD-R1-1 — VIOLATION: committed evidence fails required whitespace check

**Obligation:** AGENTS §12 says a red gate blocks completion regardless of authorship; original TASK-007/008 and TASK-008-R1 require git diff --check; implementation-execution requires whitespace checking and a clean final diff audit.

**Locations:** changes/active/bifrost-scribe-live-reads/review/task-008-review/final-named.log:1–10 (trailing spaces in captured Docker progress); same directory task-review-invariants.md:55 (new blank EOF).

Independently executed static commands on candidate:

| Command | Result |
|---|---|
| git diff --check | exit 0; clean working-tree diff |
| git diff --check HEAD~1 HEAD | exit 2; ten log trailing-space diagnostics and report EOF diagnostic |
| git diff --check a7582db58 HEAD | exit 2; same diagnostics |

**Consequence/materiality:** no runtime or test-result corruption is alleged. Captured artifacts are committed in this correction and the required whitespace check is red when checking the actual immutable change. An empty working-tree check after commit does not certify committed additions. This is a bounded gate-compliance proposal, not a discretionary cosmetic preference. Same-tree evidence may describe a genuinely passing earlier unstaged check; it does not establish this committed range passes. Independent validation should reconcile that distinction under the completion rule.

**Smallest testable correction:** remove the ten trailing spaces and normalize the report EOF without changing commands/results/content; record passing git diff --check over cumulative reviewed range. No runtime tests/full lane/new checker/dependency or production edit required. Prior FIND-007-4/5/6 closure remains valid.

## Overall result

**FAIL**, pending independent validation of RSTD-R1-1. Cumulative implementation standards and prior declaration/panic/exact-proof gaps pass static review. Only proposal is concrete committed-artifact whitespace gate failure. No runtime regression introduced by correction found. No forbidden verification executed.
