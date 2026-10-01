# Repository standards review — TASK-008 and cumulative TASK-007/R1

## Subject and limits

Candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`; immediate base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; original TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.

Reviewed the complete immediate diff and cumulative TASK-007 live-read changes, with TASK-006 benchmark/harness changes excluded as directed. No implementation edits, commits, Postgres commands, or aggregate verification lanes were made. `git diff --check` passed and HEAD remained the stated candidate. The task evidence's broad test results are recorded claims, not independently rerun proof. The orchestrator's `verification.md` independently records 5/5 focused tests passing on this candidate; that proof covers footer refusal, partition counts, and native terminal reconciliation, not broad gates. FIND-007-3, SQL `data_tenant_id`, and the accepted `WYRD_VALA_500_QUERY_TENANT_INVARIANT` code are not findings.

## Authority coverage

The reference router is `architecture/references/README.md`. Applicable authority is AGENTS.md, architecture/agent-rules.md, Wyrd design/doctrine, Bifrost design, security posture, TESTING.md, and approved spec revision 20. Where older general prose still describes a row tripwire, the current user direction and approved per-file tenant decision govern this task; this audit does not demand restoration of the removed column.

| Changed surface | Routed focused authority | Source coverage |
|---|---|---|
| Parquet tenant identity, cache key, published/hot/staged reads | rust-core; errors; architecture/patterns; doctrine/architecture-constraints; domain/arrow-analytical-interop, datafusion, iceberg, olap-serving, analytical-operations-reliability | parquet/footer.rs; parquet/writer_properties.rs; storage/cache.rs; oracle/exec.rs; oracle/follower.rs; catalog/bifrost_catalog.rs |
| Staged and hot producers, rewrite tenant propagation | Same Rust/ownership/Arrow/Iceberg/reliability authorities | scribe/parquet_writer.rs, claim_assembly.rs, staging_runtime.rs, member_stager.rs, execution_lanes.rs, persistence.rs, tail_rpc.rs; forge/managed/policy.rs and executor.rs |
| Envelope/schema deletion and permanent field identities | rust-core; architecture/patterns; Arrow/Iceberg references; Bifrost table identity | wyrd-spec/src/vala/{managed_columns,mod}.rs; redux schema/managed_columns.rs, tables/managed_columns.rs, tables/mod.rs, drift/result_features.rs, eval/result_items.rs, gateway/calls.rs; contracts.rs; catalog/layout.rs |
| Native completion and remote execution contracts | Rust/errors/testing references; DataFusion and analytical reliability; serving ownership | oracle/{dispatcher,live,codec,follower,analytical,analytical_supervisor,analytical_transport,pruning,query_stream}.rs; resources.rs; wyrd-server/src/oracle/peer_service.rs |
| Leader security event and removal of redundant follower audit collaborators | architecture/patterns; doctrine/architecture-constraints; errors; security posture; Bifrost audit/terminal contract | oracle/mod.rs; wyrd-server/src/{boot/mod,state}.rs; wyrd-spec/src/vala/audit_detail.rs; both bifrost_audit_event.json schemas |
| Runtime and dependencies introduced by TASK-007 | Rust async/ownership rules; DataFusion and analytical reliability | workspace Cargo.toml, Cargo.lock, redux Cargo.toml; staged_run_io and ScribeTailResolver; live scan and staged lease callers |
| Tests and evidence | languages/spec-driven-development, maintainer-style, testing-workflows; TESTING.md; AGENTS §§11/16 | changed inline tests; scribe/tests/scribe_persistence_path.rs; wyrd-testing Oracle distributed/support/peer-network analytical tests; task evidence and exact recipes |
| Architecture/reference and product documentation | Wyrd doctrine/design; Bifrost design; focused DataFusion/OLAP authority; docs verification rule | architecture/bifrost-design.md; references/domain/{datafusion,olap-serving}.md; docs bifrost architecture, data-plane-internals, forge, index, quickstart, schema, writing-data |

Python, TypeScript, PyO3, N-API, UI, CLI, and MCP implementations/declarations are not changed within the bounded subject. Their binding-specific standards therefore introduce no extra source work here. The generated audit schema is the changed cross-surface contract projection.

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Durable analytical behavior remains in Vala; listeners remain in wyrd-server; wyrd-spec stays pure | PASS | Native completion belongs to the engine; peer adapter stays in server; spec edits only remove a name and change a typed audit discriminator. |
| Use domain tenant types; do not invent SQL tenant predicates/raw pool interfaces | PASS | Footer identity uses DataTenantId; producer tenants come from existing bindings; no SQL schema/filter changes enter this task. |
| Import types at module scope and use bare names in fields/signatures | FAIL | RSTD-008-1 below: newly added tenant parameters/field and the new reader-metadata return type are fully qualified. |
| Concrete owners retain workflows; pure comparisons/conversions may remain free; async earns IO | PASS | BifrostFooterIdentity, ClaimAssembler, RollingArtifactWriter, ForgeTablePolicy, ScribeTailResolver, NativeOutputTally, LiveFrameDecoder, and Oracle retain their responsibilities. Footer compare is deterministic; live_leaf awaits async filesystem metadata. |
| Native analytical version cone and narrow dependency ownership | PASS | iceberg-storage-opendal reuses the exact existing Iceberg revision and opendal-fs feature within redux; lock change adds its already-resolved dependency edge. No foundational/client dependency expansion. |
| Errors use existing stable public catalog and preserve source chains | PASS | Footer refusal maps to existing BifrostError::QueryTenantInvariant; published loader retains it as an Iceberg source; peer/private completion errors remain local typed errors. |
| No compatibility layer, duplicate schemas, or parallel audit write owner | PASS | Row filter/provider, row tripwire/codec and redundant Scribe query-audit ownership are deleted; leader uses the existing audit collaborator. Both generated audit schema copies change tenant_row to tenant_file consistently with source. |
| New/materially changed Rust items document intent, errors, and remaining panics | FAIL | Core added tenant-proof/completion helpers have substantive docs, but the materially rewritten schema assertion test lacks the required panic contract: RSTD-008-2. |
| Tests reside in owning runtime/tier; fast tests avoid Postgres; exact recipes identify named tests | PASS, static | New proof tests are inline Rust tests; real-server refusal stays in wyrd-testing; TASK-007 R1 records exact unit and journey expressions and distinguishes historical/deferred execution. No foreign runtime is initialized by Rust tests. |
| Gates/test scope respected without changing checks to hide failures | PASS, bounded evidence | No added suppression, ignored failure, or weakened gate observed in this bounded change; removed tripwire assertions exercise a deliberately removed mechanism and are replaced by footer refusal checks. Aggregate and Postgres verification is intentionally not rerun under current user constraints. |
| Generated artifact and docs verification | Evidence limit | Source/schema parity is visible, and task evidence reports codegen:check/docs:check. Those lanes were not independently executed here. |
| Permanent implementation avoids task/agent history | PASS | Added runtime symbols describe actual tenant, output, scan, and ownership responsibilities rather than task IDs. |

## Proposed findings

### RSTD-008-1 — VIOLATION: new typed interfaces omit module imports

**Rule:** architecture/agent-rules.md: “Bring types in with `use` and use bare names in signatures”; it expressly applies to struct fields, arguments, and return types. Rust-core's module rules also make the top `use` block the dependency manifest.

**Changed locations:** oracle/exec.rs:1111 and 1137–1138; forge/managed/policy.rs:260; parquet/writer_properties.rs:121; scribe/claim_assembly.rs:87. The new `tenant_proven_reader_metadata` return type also uses `parquet::arrow::arrow_reader::ArrowReaderMetadata` directly. The new tenant parameter on hot_metadata_key at exec.rs:1593 has the same problem.

**Evidence and consequence:** These are newly added parameters/fields or newly introduced functions in the immediate diff, not a request to tidy old signatures. The production consumers are PublishedFooterLoader::load/hot_stream, ForgeManagedRewrite's two configuration calls, and ClaimAssembler's real encoding handoff. The dependency-bearing interfaces bypass the repository's mandatory import convention and leave their new dependencies absent from the module's dependency inventory. This is a repository compliance defect; no runtime tenant bypass is claimed.

**Smallest correction:** Add module imports for DataTenantId and ArrowReaderMetadata and use those names in these newly added/modified interfaces. Preserve all binding values, footers, error mapping and lifetime behavior. No new helper, abstraction, dependency or behavioral test is needed; format/scoped lint or compile proof suffices.

### RSTD-008-2 — VIOLATION: materially changed schema proof has incomplete rustdoc

**Rule:** AGENTS.md §16 requires substantive rustdoc on materially modified test functions and “Add `# Panics` whenever a panic remains possible”; missing/incomplete documentation is a hard blocker. architecture/agent-rules.md repeats that private tests are included.

**Changed location:** schema/managed_columns.rs:48–49, `with_managed_columns_appends_the_envelope_without_a_tenant_column`.

**Evidence and consequence:** The test was renamed and its tenant-column checks were removed/replaced to prove the new envelope. Its one-line doc states the outcome but omits the panic contract despite assert_eq!/assert! at lines 52–78. A maintainer receives no documented account of which envelope/nullability changes this proof intentionally refuses. This is narrowly a documentation compliance defect, not a failed behavioral test.

**Smallest correction:** Document the exact schema ordering and non-null principal/request identity assertions and their panic conditions on this changed test. Keep the assertions intact. Do not widen this into an unrelated test-documentation cleanup. Nearby new footer proof tests already use substantive `# Panics` sections. The existing exact schema test plus formatting is sufficient closure proof.

## Overall result

**FAIL** — two bounded repository-rule proposals require independent validation. No task acceptance verdict is inferred here; unexecuted broad gates are explicit verification limits, not requests to violate the review's command restrictions.
