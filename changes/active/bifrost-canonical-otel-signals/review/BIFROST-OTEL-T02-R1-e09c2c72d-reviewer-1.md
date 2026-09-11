# Independent Task Review — BIFROST-OTEL-T02-R1

## Subject

- Base: `f21d0efd436bc3bb41d060c1475a4b999d7a4acb`
- Candidate: `e09c2c72d4bb83064096faa0dbe99ce16ce46ff7`
- Approved authority: `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 9
- Task authority: `02-gate-scribe-convergence.md` plus cumulative remediation `02-R1-convergence-remediation.md`
- Prior findings: FIND-02-1 through FIND-02-4 as reproduced in the remediation packet; no earlier report is present in the candidate review directory or repository history.
- Scope note: the human override excludes non-Bifrost `test:wyrd` and Forge lanes from required verification. Those lanes and unrelated baseline failures do not affect this review.

## Review Findings

### Critical

- None.

### Important

#### FIND-02-R1-1 — MAJOR: one denied OTLP Card assertion rejects valid siblings instead of becoming record-level partial success

- Obligations: REQ-005, REQ-022, AC-006; remediation Scenarios 2 and 3; cumulative closure claim for FIND-02-2.
- Locations: `changes/active/bifrost-canonical-otel-signals/spec.md:207-213`, `changes/active/bifrost-canonical-otel-signals/spec.md:217-225`, `crates/vala/vala-bifrost-redux/src/tables/signal.rs:288-309`, `crates/vala/vala-bifrost-redux/src/gate/mod.rs:474-501`, `crates/vala/vala-bifrost-redux/src/gate/mod.rs:510-541`, `crates/vala/vala-bifrost-redux/src/gate/mod.rs:550-576`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:520-562`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:689-727`, and `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:1049-1115`.
- Current flow: each table projector calls `RecordCorrelation::extract`, which validates only OTLP type and `CardRef` syntax. Gate projects every syntactically valid record in the request into one canonical batch and dispatches it once. Scribe then runs `validate_card_scope` over the complete batch and returns on the first out-of-scope row. A signed member without a UID similarly reaches `resolve_card_uids` and returns for the complete batch. Gate uses `?` on that Scribe result before returning the projector's `IngestOutcome`.
- Reachable scenario: an authenticated OTLP trace, log, or metrics request contains one record with a missing or authorized `wyrd.card_ref` and one record whose well-formed final `wyrd.card_ref` is outside the signed scope (or resolves to a UID-less signed member). Both survive projection. Scribe refuses the shared frame.
- Consequence: the authorized sibling is not durably accepted and Gate cannot return the required signal-specific partial-success count. This changes a record-level OTLP refusal into a whole-request authorization error, contrary to the explicit revised contract.
- Supporting evidence: the spec distinguishes OTLP record-level refusal from canonical Arrow whole-write refusal. The three projector entry points at Gate lines 487-499, 527-539, and 563-574 dispatch one aggregate batch. Scribe's own documentation at lines 689-696 confirms that one denied row refuses the frame. Existing correlation projector tests cover missing, duplicate, wrong-type, and malformed attributes, but do not provide a signed scope or mix in-scope and out-of-scope records. The Scribe scope test covers denial in isolated batches, not one mixed OTLP request.
- Counterevidence considered: wrong-typed and syntactically malformed correlation is correctly rejected before each table accumulator mutates, so those cases preserve sibling atomicity. Missing/null correlation is also accepted as required. Neither fact reaches the signed-scope denial cases because scope is unavailable at projection time.
- Ponytail recommendation: reuse the existing `CardRefScope::authorizes`/`same_identity`, `RecordCorrelation`, and projector rejection accounting. Pass the verified signed scope as the smallest borrowed authorization input from Gate's authenticated context into the three existing table projection entry points. In the shared table-owned correlation check, reject a present out-of-scope or UID-less identity before the record mutates its accumulator; missing/null remains accepted. Gate remains a router, and Scribe retains its current whole-frame scope and UID checks as defense in depth and for canonical Arrow. Do not add a registry lookup, cache, service, trait, or second UID map.
- Required outcome: for traces, logs, and every supported metric point, a mixed request durably stores valid/uncorrelated/in-scope siblings, omits only out-of-scope and UID-less records, preserves accepted order and ordinals, and returns exact partial-success counts and stable reason. Canonical Arrow retains whole-write refusal. No Postgres or cache lookup enters ingest.
- Closure verification: extend the three named `optional_card_correlation_is_atomic_and_lossless` tests with mixed in-scope, out-of-scope, and UID-less signed-scope records; add one focused Gate-to-Scribe composition test proving the aggregate request reaches Scribe with accepted rows only and exact outcome. Run each exact named test through `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=...)'`, then `mise run test:bifrost:integration:redux` and `mise run test:bifrost:journey:scribe`. No new test target or harness is required.

#### FIND-02-R1-2 — MODERATE: duplicate dynamic Arrow correlation columns make scope validation ambiguous and silently discard one assertion

- Obligations: REQ-003, REQ-004, REQ-022; remediation Scenarios 3 and 5; canonical Arrow whole-write refusal.
- Locations: `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:507-510`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:520-562`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:580-617`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:655-668`, `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:704-727`, and `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:1067-1115`.
- Current flow: duplicate-name validation exists only inside `enforce_canonical_source_contract`, which runs when `DecodeContext.definition` is `Some` for a built-in canonical table. Dynamic and pre-declared tables explicitly use `None`. For those tables, `source_schema_fingerprint` removes every field named `card_ref`; `validate_card_scope` uses `column_by_name`, and `resolve_card_uids` uses `index_of`. Arrow 58.3.0 implements both lookups through `Fields::find`, which returns the first matching field. Later user-field projection removes every correlation-named field.
- Reachable scenario: a registered dynamic table receives a valid Arrow IPC batch with two UTF-8 `card_ref` fields. The first is null or in scope and the second is out of scope. Removing both correlation fields leaves the expected registered user fingerprint, scope validation and UID resolution inspect only the first field, and both supplied fields are stripped during stamping.
- Consequence: the write succeeds even though it contains a present out-of-scope Card assertion that REQ-022 requires to fail closed. The assertion is silently discarded. The wrong Card UID is not stamped, which bounds the impact, but canonical Arrow's whole-write validation guarantee is not met.
- Supporting evidence: the generic `decode_rows` reserved-field loop does not reject duplicate names. The one existing `HashSet` duplicate-name check is gated by a built-in definition at lines 557-559 and 584-593. Arrow's `Fields::find` at dependency source `arrow-schema-58.3.0/src/fields.rs:81-84` is first-match, and both `Schema::index_of` and `RecordBatch::column_by_name` delegate to it.
- Counterevidence considered: built-in spans/logs/points are protected by `enforce_canonical_source_contract`, and duplicate managed server-owned fields are rejected by the earlier reserved-field loop. The bypass is limited to the permitted caller correlation names on a dynamic or pre-declared table; that path remains public and is covered by REQ-022's every-ingest-row language.
- Ponytail recommendation: move or reuse the existing one-pass duplicate-name `HashSet` check at the shared `decode_rows` boundary so it applies before fingerprint and scope processing for every table. Keep built-in-only field-shape/value checks where they are. Do not add a new validator abstraction or special-case registry.
- Required outcome: every canonical Arrow batch with duplicate field names, including duplicate `card_ref`, is refused before stamping or WAL preparation regardless of table kind.
- Closure verification: add a focused dynamic-table `decode_rows`/ingress test with first-null and second-out-of-scope duplicate `card_ref` columns and assert `InvalidFrame`, no WAL append, and no fence. Run its exact nextest expression through `mise exec --`, then `mise run test:bifrost:integration:redux` and `mise run test:bifrost:journey:scribe`.

### Suggestions

- None. The dead `live_reservations` test-support vector is real local debt, but it belongs to the separate in-flight Oracle admission change and does not warrant Task 02 churn.

## Open Questions

- None. Both findings have a single contract-preserving correction boundary and require no product or specification decision.

## Coverage Ledger

### Changed production files

- Canonical definitions and catalog identity — reviewed `catalog/bifrost_catalog.rs`, `tables/fields.rs`, `tables/mod.rs`, `tables/{traces,logs,metrics}/mod.rs`, `tables/traces/spans.rs`, `tables/logs/records.rs`, and `tables/metrics/points.rs` for exact field identity, metadata preservation, table registration, dynamic/built-in separation, and public schema compatibility. The built-in registry's existing associated validator is sufficient; FIND-02-R1-2 concerns the unguarded dynamic path.
- OTLP projection and limits — reviewed `otlp_limits.rs`, `tables/signal.rs`, and all three new projection modules for request bounds, lossless attributes, final-key semantics, per-record accumulator mutation, stable outcomes, and Gate ownership. Syntax/type atomicity is correct; scope atomicity is not (FIND-02-R1-1).
- Gate/contracts — reviewed `contracts.rs`, `gate/mod.rs`, and `lib.rs` for authentication, routing, owner transfer, empty accepted subsets, wire-size limits, error propagation, audit context, and removal of the OTLP Scribe payload variant. Gate transfers one canonical batch and does not decode OTLP inside Scribe; the remaining composition defect is FIND-02-R1-1.
- Scribe ingress/admission — reviewed `scribe/admission.rs`, `scribe/ingress.rs`, `scribe/execution_lanes.rs`, `scribe/preprocess.rs`, and `scribe/mod.rs` for schema resolution, scope checks, managed stamping, request-wide ordinal cursors, bounded CPU execution, cancellation ownership, WAL/fence/ACK ordering, and error mapping.
- Persistence/recovery — reviewed `scribe/fixed_ipc.rs`, `scribe/material_plan.rs`, `scribe/wal.rs`, `scribe/replay.rs`, and `scribe/parquet_writer.rs` for recursive nested Arrow validation, structural limits, exact accepted-slice identity, CRC/digest checks, truncation failure, replay idempotence, and physical-schema preservation. Reviewed deletion of `scribe/direct_{traces,logs,metrics}.rs`, `scribe/otlp_managed.rs`, and the test projection oracle as removal of the parallel OTLP persistence path, not loss of a live caller.
- Schema/Parquet/Oracle — reviewed `schema/fingerprint.rs`, `parquet/memory.rs`, `oracle/tail_fence.rs`, and the narrow change in `oracle/admission.rs`. Catalog identity remains Iceberg-normalized while every memory-envelope decode comparison uses `from_arrow_schema_exact`; no remaining normalized caller makes a physical decode decision. Tail fencing now consumes the shared schema identity. Oracle resource probing is test-support and now reads the live query pool.
- Auth/spec — reviewed `wyrd-spec/src/reference.rs`, `wyrd-auth/src/card_scope.rs`, and `wyrd-auth/src/refresh.rs` for exact identity semantics, authoritative registry UID propagation, duplicate-root reconciliation, bounded scope/token size, tenant-bound database use at mint/refresh, and absence of ingest-path Postgres/cache work.
- Server/test-support production owners — reviewed `wyrd-server/src/query/scheduled.rs`, `wyrd-server/src/state.rs`, and `wyrd-testing/src/server.rs`. Biased cancellation correctly gives already-ready cancellation/deadline settlement precedence over buffered frames. State wiring and forced-WAL/query probes remain test-support-only and do not alter production durability.
- Python, PyO3, UI, and TypeScript — no PyO3 wrapper, Python package/stub, UI, TypeScript, N-API, manifest, or public generated contract production file changed. The sole Python change is an integration-test expectation; therefore binding/stub/UI runtime review is not applicable beyond confirming no exposed surface moved.
- Documentation and workflow files — reviewed Bifrost/Wyrd authority changes and the active Task 02/R1 packet for consistency with revision 9. The SQL-foundation restoration, separate Oracle-admission packet, and task-review skill synchronization do not create Task 02 production behavior.

### Obligation and prior-finding coverage

- Original Task 02 S1: accepted-only ordinals and all-invalid no-Scribe behavior are implemented in table projection/Gate tests; no independent defect found.
- Original Task 02 S2: nested `List<Struct<...>>` reaches the common WAL path through recursive fixed IPC/native scan support; no independent defect found.
- Original Task 02 S3: recovery reconstructs the accepted nested slice set, digest, fence, and commit; no independent defect found.
- Original Task 02 S4: `IngressPayload` contains canonical Arrow forms only and deleted direct OTLP Scribe modules have no live callers; no independent defect found.
- FIND-02-1: authoritative registry UIDs are carried for root and secondary scope members and only trusted signed UIDs are stamped. Closed for its original root cause.
- FIND-02-2: the shared final-record attribute extractor exists for all three signals, but revised scope-denial atomicity is not complete across table → Gate → Scribe composition (FIND-02-R1-1). The original absence is fixed; the mapped REQ-022 closure is incomplete.
- FIND-02-3: one native Arrow request advances a checked `i32` ordinal cursor across all record batches. Closed.
- FIND-02-4: built-in table-owned validation and canonical physical identity are enforced before persistence and checked through replay. Closed for built-ins; FIND-02-R1-2 is a separate dynamic correlation ambiguity under REQ-022, not a failure of the built-in physical identity registry.
- Follow-on `b44200d43`: exact-vs-normalized schema fingerprints are correctly separated. `parquet/memory.rs` uses exact spelling for envelope/decode compatibility; catalog consumers retain normalized identity. The existing UTC-spelling regression test is capable of detecting the alias.
- Follow-on `684d6c1e5`: biased `tokio::select!` correctly fixes the already-cancelled/already-buffered success race and preserves validated terminal handling.
- Follow-on `7fe8e7cd9`: `QueryResourceProbe::memory_bytes` observes the live pool instead of an unpopulated vector. Remaining vector cleanup is unrelated Oracle-admission debt.
- Python RBAC expectation: queue terminal denial is emitted once at flush and not retained for `shutdown`; the assertion change follows current production queue semantics and does not weaken authorization.
- WAL-full injection `1bb561a46`: the test-support fault remains asserted until explicit journey release while production retirement still owns latch clearing. No production WAL-full behavior changed.

## Adversarial Clean Evidence

- Correctness outside findings: attempted nested empty lists, nested structs, multi-batch ordinal reset, ordinal overflow, and all-invalid nonempty exports. Recursive IPC/material-plan validation, checked ordinal addition, and the zero-row Gate return resist these cases. Mixed scope denial remains the documented exception.
- Security/auth: attempted missing correlation, client-supplied conflicting `#uid`, secondary Card substitution, absent signed scope, cross-scope identity, and tenant confusion. Missing correlation stamps null; identity comparison ignores the client UID and uses the signed member UID; tenant comes from verified `AuthContext`; no ingest registry IO exists. Mixed OTLP scope and duplicate-column ambiguity are the two exceptions above.
- Architecture/contracts: attempted to find OTLP wire types crossing Scribe, Gate-owned semantic decoding, a second table registry, or Scribe registry authority. OTLP projection is table-owned; Gate authenticates/routes; Scribe receives canonical batches and uses the existing built-in registry. FIND-02-R1-1 requires passing existing verified scope to the table boundary, not moving decoding into Gate.
- Persistence/transactions: attempted cancellation between WAL fsync and ACK, partial frame append, truncated Arrow replay, and replay double-write. The persistence task owns completion after admission; WAL CRC/digest and replay structural decode fail closed; replay dedup/fence identity is asserted. Auth registry IO remains transaction-scoped at token mint/refresh and outside ingest.
- Async/concurrency: attempted ready cancellation racing a ready stream frame, transport cancellation after Scribe admission, forced fault flag leakage, and query-probe release races. Biased selection, owned Scribe task completion, explicit fault release, and watch-backed probe state resist those cases.
- Performance: attempted unbounded OTLP traversal, recursive schema blow-up, per-row database lookup, and oversized signed claims. Request/attribute limits, field/depth limits, bounded Card scope/token size, and linear in-memory lookup resist them; no new hot-path IO is present.
- Code quality/maintainability: attempted to identify a speculative trait, parallel UID map, duplicate OTLP persistence implementation, or table-name match in Scribe. The implementation reuses `CardRefScope`, an associated validator on the existing table definition, and deletes the parallel direct writers. The recommendations above likewise require no new abstraction.
- Tests/DX: exact named remediation tests and Bifrost lanes are recorded at the candidate, and the test names inspect the claimed seams. The missing mixed-scope composition case is material because current tests cannot detect FIND-02-R1-1. Non-Bifrost `test:wyrd`, Forge lifecycle failures, and named baseline failures are excluded by human scope and do not block.
- Bifrost/Vala: attempted normalized `Utf8`/`LargeUtf8` or timezone alias at a memory decode boundary, nested fixed-IPC undercount, and table metadata loss during stamping. Exact envelope fingerprints, recursive walks, and cloning managed/correlation fields from the table definition resist them.
- PyO3/Python: attempted an unsynchronized native/stub/export change and Python interpreter behavior moved into Rust. No Python-visible production surface changed; the integration assertion only aligns terminal-error observation.
- UI/TypeScript: attempted a schema or public response change requiring SDK/UI regeneration. No UI, TypeScript, binding, OpenAPI, or generated artifact changed in this range; not applicable.

## Verification Notes

- Static verification performed against the exact immutable base/candidate range. `git status --short` was clean before writing this assigned report, `HEAD` resolved to the candidate, and `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..e09c2c72d4bb83064096faa0dbe99ce16ce46ff7` passed.
- Reused candidate-bound task evidence: `test:bifrost:integration:redux` 956 passed; Scribe journey 20, server journey 4, MCP journey 7, Python journey 17; format, lints, and diff check clean; all exact six-scenario named tests recorded passing.
- No executable test was rerun: the two findings follow directly from source control flow and concern cases absent from the current assertions. Rerunning the existing tests would not distinguish them.
- Known Forge, `test:wyrd`, external-auth verification, query proptest, and unrelated workload failures are not used as findings. The human explicitly removed non-Bifrost/Forge lanes from required Task 02 verification.

## Independent Review Conclusion

The cumulative candidate needs bounded remediation for FIND-02-R1-1 and FIND-02-R1-2. The other original and remediation obligations, including the five named follow-on corrections, are source-supported. Both remaining defects fit revision 9 and require no specification revision, new persistence path, database/cache hot-path lookup, or new abstraction.
