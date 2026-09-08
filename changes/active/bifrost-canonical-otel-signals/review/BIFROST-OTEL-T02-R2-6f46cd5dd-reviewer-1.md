# Independent Task Review — BIFROST-OTEL-T02-R2

## Subject

- Base: `f21d0efd436bc3bb41d060c1475a4b999d7a4acb`
- Candidate: `6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7`
- Approved authority: `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 9
- Task authority: `tasks/02-gate-scribe-convergence.md`, `tasks/02-R1-convergence-remediation.md`, and `review/BIFROST-OTEL-T02-R2-fast-remediation.md`
- Prior review authority: `review/BIFROST-OTEL-T02-R1-e09c2c72d-reviewer-1.md` and `review/BIFROST-OTEL-T02-R1-e09c2c72d-verdict.md`
- Scope override: non-Bifrost `test:wyrd`, Forge, and broad gate lanes are explicitly excluded and do not block this review.

## Review Findings

### Critical

- None.

### Important

- None. The cumulative candidate closes `FIND-02-1` through `FIND-02-4` and `FIND-02-R1-1` through `FIND-02-R1-3` without introducing a source-validated, reachable correctness, security, durability, or architecture failure.

### Suggestions

- `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:267`, `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:909`, `crates/vala/vala-bifrost-redux/src/scribe/promotion.rs:232`, `crates/vala/vala-bifrost-redux/src/scribe/promotion.rs:275`, and `crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs:398`: the artifact/promotion `schema_fingerprint` field is documented as the exact fingerprint sealed into the Parquet footer, but `inspect_sealed_artifact` fills it with the normalized catalog fingerprint. Source search finds no reader or comparison of the persisted value; it is serialized and round-tripped only. The user confirms this record has not shipped, so compatibility does not justify keeping an unused ambiguous field. Ponytail recommendation: delete the field end-to-end from `SealedArtifactEvidence`, `BoundedParquetArtifact`, `PublishedHotFileIdentity`, `ScribePublishedHotFileV1`, construction, and fixtures; retain the exact fingerprint solely in the existing Parquet memory envelope/footer where Oracle consumes it. This is nonblocking because no current consumer reads the value and no reachable operation depends on it.
- [`changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T02-R2-fast-remediation.md:3`](BIFROST-OTEL-T02-R2-fast-remediation.md): the candidate-bound `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7` exits 2 on three trailing-space lines despite the recorded clean result. Remove the three Markdown hard-break spaces when next touching review evidence. This is documentation-only hygiene, not a product or verification-coverage failure; all required executable behavior has direct candidate-bound proof.

## Open Questions

- None affecting approval. If the unconsumed promotion fingerprint is retained rather than deleted, its owner must choose and document one meaning before a reader is added: normalized catalog identity or exact Parquet-envelope layout identity. No current consumer forces that decision.

## Coverage Ledger

### Changed production files

- Catalog and contracts: this reviewer inspected `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs`, `contracts.rs`, and `lib.rs` for tenant-bound resolution, canonical ingress ownership, table identity, and removal of an OTLP Scribe payload. The table registry remains the sole built-in schema/validator authority.
- Gate and OTLP limits: this reviewer inspected `gate/mod.rs` and `otlp_limits.rs` for authentication before projection, bounded traversal, partial-success accounting, all-invalid handling, accepted-order preservation, canonical dispatch, and ownership transfer. Gate passes the verified borrowed scope but does not acquire table semantics or Card-registry IO.
- Signal tables: this reviewer inspected `tables/fields.rs`, `tables/mod.rs`, `tables/signal.rs`, `tables/traces/{mod.rs,projection.rs,spans.rs}`, `tables/logs/{mod.rs,projection.rs,records.rs}`, and `tables/metrics/{mod.rs,projection.rs,points.rs}` for lossless values, final-attribute semantics, per-record accumulator atomicity, signed-scope authorization, canonical physical identity, and stable rejection outcomes.
- Scribe ingress and stamping: this reviewer inspected `scribe/admission.rs`, `scribe/ingress.rs`, `scribe/execution_lanes.rs`, `scribe/preprocess.rs`, `scribe/material_plan.rs`, and `scribe/mod.rs` for schema resolution, shared duplicate-name refusal, reserved-column handling, Card UID stamping, request-wide ordinals, admission, and the unchanged WAL/fence/ACK boundary.
- Persistence and recovery: this reviewer inspected `scribe/fixed_ipc.rs`, `scribe/wal.rs`, `scribe/replay.rs`, `scribe/parquet_writer.rs`, `scribe/tests/scribe_persistence_path.rs`, and `scribe/tests/wal_closeout.rs` for recursive nested validation, CRC/digest identity, truncation refusal, replay idempotence, accepted-slice reconstruction, Parquet sealing, and test effectiveness.
- Deleted parallel persistence owners: this reviewer inspected deletion and caller closure for `scribe/direct_logs.rs`, `scribe/direct_metrics.rs`, `scribe/direct_traces.rs`, `scribe/otlp_managed.rs`, `scribe/test_projection_oracle.rs`, and `scribe/test_projection_oracle/map.rs`. No production OTLP payload or signal-specific persistence path remains behind Scribe.
- Schema, Parquet, and Oracle: this reviewer inspected `schema/fingerprint.rs`, `parquet/memory.rs`, `oracle/tail_fence.rs`, and the narrow `oracle/admission.rs` change. Catalog identity remains Iceberg-normalized; exact recursive layout identity is confined to the Parquet memory-envelope path and distinguishes all Arrow variants, parameters, child ordering, names, and nullability while excluding metadata deterministically.
- Auth and contract identity: this reviewer inspected `crates/wyrd-spec/src/reference.rs`, `crates/wyrd/wyrd-auth/src/card_scope.rs`, and `crates/wyrd/wyrd-auth/src/refresh.rs` for exact Card identity, authoritative registry UID propagation at mint/refresh, trusted signed claims, and no ingest-path Postgres/cache lookup.
- Server and test-support production owners: this reviewer inspected `crates/wyrd/wyrd-server/src/query/scheduled.rs`, `crates/wyrd/wyrd-server/src/state.rs`, and `crates/wyrd/wyrd-testing/src/server.rs` for cancellation precedence and test-only fault/probe wiring. These follow-on fixes preserve production query and Scribe boundaries.
- Tests and journeys: this reviewer inspected `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`, `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs`, `crates/wyrd/wyrd-testing/tests/bifrost/scribe/{backpressure.rs,round_robin.rs,support.rs,sustained.rs,write_read.rs}`, `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs`, and `python/py-wyrd/tests/integration/test_bifrost_e2e.py` for assertion strength and fixture fidelity. The latest `support.rs` edit only supplies `None` to the new optional signed-scope argument for uncorrelated test data.
- PyO3, UI, and TypeScript production boundaries: no PyO3 wrapper, Python package/stub, UI, Svelte, TypeScript, N-API, generated API contract, manifest, or dependency file changed. These runtime/binding lenses are source-backed not applicable; the Python file is an integration-test expectation only.
- Interleaved changes under the separate `oracle-local-admission` packet and workflow-skill/architecture maintenance were classified outside Task 02 except for the narrow follow-ons explicitly documented above.

### Mapped obligations

- REQ-001/REQ-020 and INV-010: table-owned canonical schemas and projectors replace duplicate Scribe mapping authority; inspected in the table registry, projection modules, deleted direct writers, and `IngressPayload`.
- REQ-002/REQ-005, INV-002/INV-008/INV-009, and AC-006/AC-007: Gate emits one accepted canonical subset, preserves order, assigns contiguous request ordinals, performs no empty append, and returns only after Scribe durability; inspected in Gate, decode context, preprocess/WAL/replay, and focused tests.
- REQ-003/REQ-004, INV-003/INV-007/INV-009, and AC-002/AC-008: canonical Arrow and OTLP converge before one managed stamping/WAL path; nested schemas, physical validators, duplicate fields, multi-batch ordinals, and replay identity were inspected in `execution_lanes`, `fixed_ipc`, `material_plan`, the table registry, and replay.
- REQ-022, INV-006/INV-011, and AC-012: missing `card_ref` is accepted, present refs are authorized per record against signed scope, only the scope member UID is stamped, and ingest does no Card-registry IO; inspected from token scope construction through Gate/table projection and Scribe defense-in-depth.
- AC-011: new assertions live in the existing Gate, signal-table, Scribe, fingerprint, Parquet, and Scribe-journey owners; no parallel harness was added.

### Prior finding closure

- `FIND-02-1`: closed. Mint/refresh scope resolution retains authoritative UIDs for root and secondary identities; Scribe stamps only the matching signed UID.
- `FIND-02-2`: closed. Final record-level `wyrd.card_ref` and `wyrd.run_id` are extracted in all three table projectors, with per-record rejection and lossless original attributes.
- `FIND-02-3`: closed. Native Arrow IPC and canonical payload modes share a checked request-wide ordinal cursor.
- `FIND-02-4`: closed. Built-in table registry dispatches canonical value/physical identity validation at ingest and replay.
- `FIND-02-R1-1`: closed by `2b41ed20f`. Gate supplies the verified scope; the table-owned shared extractor rejects out-of-scope and UID-less records before accumulator mutation. Mixed Gate proof shows accepted siblings alone reach one Scribe call.
- `FIND-02-R1-2`: closed by `19e71d59e`. One `HashSet` guard at `decode_rows` rejects duplicate names for built-in, dynamic, and pre-declared tables before fingerprint, scope, stamping, or WAL work.
- `FIND-02-R1-3`: closed by `48e4553b1` plus `6f46cd5dd`. The exact fingerprint no longer uses `Debug`; it recursively commits exact Arrow variants, parameters, names/order, and nullability while ignoring metadata. Footer reconstruction and layout-drift tests pass.

## High-Risk Boundary Review

- Security/authz: attempted missing scope, out-of-scope identity, UID-less signed member, hostile client `#uid`, cross-card substitution, duplicate `card_ref` columns, and mixed authorized/unauthorized OTLP siblings. Missing correlation remains valid; client UID is ignored by `same_identity`; per-record table authorization and whole-frame Scribe validation use signed scope only; duplicate names fail before lookup. No database/cache call was added.
- Persistence/transactions: attempted accepted-sibling loss, empty WAL append, partial row persistence, cancellation after admission, truncated IPC recovery, digest mismatch, and replay double-write. Accepted subsets are assembled before Scribe; Scribe retains owned completion and WAL/fence/ACK order; recursive decode, CRC/digest checks, and replay dedup fail closed.
- Async/concurrency: attempted ready cancellation racing a ready response, transport cancellation after durable ownership transfer, forced WAL latch clearing, and query-probe stale accounting. Biased selection, owned Scribe work, explicit test fault release, and live pool measurement resist these cases.
- Bifrost/Vala and Oracle: attempted nested-list schema drift, normalized `Utf8`/`LargeUtf8` alias at a decode boundary, metadata-order nondeterminism, top-level/nested nullability collision, and independently reconstructed footer schema. Recursive exact hashing and memory-envelope validation resist each case; normalized catalog identity remains separate.
- Performance: attempted per-row Postgres/cache IO, unbounded signed scope, unbounded OTLP traversal, and recursive schema amplification. The candidate performs linear in-memory bounded-scope lookup, retains wire/record/schema limits, and adds no dependency or blocking IO to ingest.
- Architecture/contracts: attempted Gate-owned decoding/table semantics, Scribe Card-registry authority, a second schema registry, a new fingerprint type, and changed dynamic-table version behavior. The candidate reuses existing owners and one private helper; no public or generated contract changes.

## Adversarial Clean Evidence

- Correctness: a six-resource trace request mixes missing, malformed, in-scope, out-of-scope, and UID-less refs. Exactly three accepted rows reach one Scribe double in request order; all-invalid continues to bypass Scribe. Equivalent table tests cover traces, logs, and metrics.
- Security: a dynamic Arrow batch with two same-named `card_ref` fields, first null and second hostile, is refused by the shared decode boundary. The guard covers every duplicate field name and table kind rather than special-casing correlation.
- Schema durability: equivalent nested schemas with reversed metadata insertion order hash equally; top-level/nested nullability and small/large offset layouts hash differently; a separately rebuilt footer schema validates while changed nullability is refused.
- Maintainability/Ponytail: no new trait, service, cache, registry, dependency, public surface, or table-name dispatch was added. Each correction reuses an existing shared owner (`RecordCorrelation`, `decode_rows`, `from_arrow_schema_exact`). The only removable remainder is the unused promotion fingerprint noted above.
- Tests/DX: each remediation behavior has one focused owner test; broader current evidence covers the complete redux integration family and Scribe journey. The excluded non-Bifrost/Forge/broad lanes are unrelated by explicit user authority.

## Verification Notes

- Reused candidate-bound evidence: eight named remediation tests passed; `mise run test:bifrost:integration:redux` reported 958/958; `mise run test:bifrost:journey:scribe` reported 20/20; `mise run fmt` and `mise run lints` were recorded clean.
- Independently rerun against candidate on 2026-09-07: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only) | test(=parquet::memory::tests::bifrost_footer_fingerprint_does_not_alias_utc_spellings) | test(=scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence) | test(=scribe::execution_lanes::tests::duplicate_arrow_column_names_fail_closed_for_dynamic_tables) | test(=gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals) | test(=tables::traces::tests::optional_card_correlation_is_atomic_and_lossless) | test(=tables::logs::tests::optional_card_correlation_is_atomic_and_lossless) | test(=tables::metrics::tests::optional_card_correlation_is_atomic_and_lossless)'` — exit 0, 8/8 passed, nextest run `cfeb344b-01bb-49e1-b643-d89d3a354dc4`.
- Independently rerun: `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7` — exit 2 solely for the three review-markdown trailing spaces noted under Suggestions.
- Not rerun: the 958-test redux lane, 20-test Scribe journey, format, or lints because current candidate-bound evidence is credible and the focused tests resolve the changed remediation behavior. Non-Bifrost `test:wyrd`, Forge, and broad gate lanes are outside the human-approved verification scope.

## Independent Conclusion

No blocking finding. The candidate closes the cumulative Task 02 obligations and all seven prior findings. The unused, unshipped promotion fingerprint should be deleted as ordinary Ponytail cleanup before any consumer gives it semantics; the three trailing spaces should be removed when review evidence is next edited. Neither creates a reachable product failure in this candidate.
