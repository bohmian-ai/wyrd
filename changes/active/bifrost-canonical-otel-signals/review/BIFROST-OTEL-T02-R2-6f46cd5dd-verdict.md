# REMEDIATE

## Subject
- Base: `f21d0efd436bc3bb41d060c1475a4b999d7a4acb`
- Candidate: `6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7`
- Planning snapshot: None
- Evidence snapshot: `6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7`

## Verdict Basis
- The cumulative candidate correctly closes `FIND-02-1` through `FIND-02-4` and `FIND-02-R1-1` through `FIND-02-R1-3`. Two bounded closeout defects remain: an unshipped versioned promotion record persists an unused normalized fingerprint while claiming it is the exact footer fingerprint, and the candidate fails its required candidate-bound whitespace check despite recording it clean. Neither requires a specification decision, compatibility path, migration, new abstraction, or another behavioral redesign.

## Verification
- Reused: candidate evidence records eight named tests passing; `mise run test:bifrost:integration:redux` 958/958; `mise run test:bifrost:journey:scribe` 20/20; `mise run fmt`; and `mise run lints`. The human explicitly excluded non-Bifrost `test:wyrd`, Forge, and broad gate lanes.
- Rerun: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only) | test(=parquet::memory::tests::bifrost_footer_fingerprint_does_not_alias_utc_spellings) | test(=scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence) | test(=scribe::execution_lanes::tests::duplicate_arrow_column_names_fail_closed_for_dynamic_tables) | test(=gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals) | test(=tables::traces::tests::optional_card_correlation_is_atomic_and_lossless) | test(=tables::logs::tests::optional_card_correlation_is_atomic_and_lossless) | test(=tables::metrics::tests::optional_card_correlation_is_atomic_and_lossless)'` — exit 0, 8/8 passed, nextest run `11de9319-27aa-4402-9eeb-efb9897976b2`, 2026-09-07; `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7` — exit 2 on three trailing-space lines in the fast-remediation artifact.
- Not run: the 958-test redux lane, 20-test Scribe journey, format, and lints were not repeated because their candidate evidence is current and the exact behavioral tests were rerun. Non-Bifrost `test:wyrd`, Forge, unrelated Oracle suites, and `mise run gate` are outside the human-approved scope.

## Findings

### FIND-02-R2-1 — MODERATE: unshipped promotion evidence persists a false and unused schema fingerprint
- Obligations: fast-remediation Issue 3 and Completion; AC-008 and AC-011; Bifrost authority requiring exact writer-derived promotion evidence.
- Locations: `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:267`, `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:909`, `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:1020`, `crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs:389`, `crates/vala/vala-bifrost-redux/src/scribe/promotion.rs:217`, and `crates/vala/vala-bifrost-redux/src/scribe/promotion.rs:255`.
- Scenario: Scribe seals a schema for which normalized catalog identity differs from exact Parquet layout identity, such as an offset-width, timezone-spelling, or nullability distinction. The footer contains and validates the exact checksum, but `inspect_sealed_artifact` places the normalized catalog fingerprint in `BoundedParquetArtifact`; publication copies it into `ScribePublishedHotFileV1.schema_fingerprint`, whose contract says it is the fingerprint sealed into the footer.
- Consequence: every such publication persists versioned evidence that does not describe the footer it claims to describe. No current reader compares the field, so present publication does not fail, but shipping it creates a misleading durable contract and compatibility cost for data with no consumer.
- Supporting evidence: the value is computed with `SchemaFingerprint::from_arrow_schema` at `parquet_writer.rs:1023`, while the footer uses `parquet::memory::schema_fingerprint`/`from_arrow_schema_exact`. Repository-wide source search finds only construction, serialization, fixtures, and round-trip preservation for the promotion field; `validate_object` and `data_file` do not read it. The user confirms nothing carrying this record has shipped and no compatibility contract requires retention.
- Counterevidence: the exact checksum is live and required in the Parquet memory envelope, staging identity, recovery, and Oracle admission; it must remain there. Object key, size, digest, and `DataFile` promotion evidence are independently validated and remain required.
- Ponytail recommendation: delete the unused field end-to-end from `SealedArtifactEvidence`, `BoundedParquetArtifact`, `PublishedHotFileIdentity`, `ScribePublishedHotFileV1`, their constructors, fixtures, and assertions. Keep the exact checksum solely in the existing Parquet memory-envelope/footer path where it is consumed. Reuse the existing promotion round-trip and footer tests; add no replacement field, compatibility shim, version bump, migration, or new test because the record is unshipped and the remaining tests already cover both live contracts.
- Required outcome: new promotion records contain only evidence consumed to validate or rebuild the published object; the exact Parquet checksum remains written and validated by the memory envelope; no normalized or duplicate schema fingerprint is copied into artifact/promotion evidence.
- Closure verification: update the existing `promotion_record_round_trips_and_encodes_deterministically` and `parquet_footer_preserves_complete_claim_evidence` fixtures/assertions, then run their exact nextest expressions plus `schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only`, `parquet::memory::tests::bifrost_footer_fingerprint_does_not_alias_utc_spellings`, `mise run test:bifrost:integration:redux`, and `mise run test:bifrost:journey:scribe`.

### FIND-02-R2-2 — MODERATE: the candidate does not pass its recorded `git diff --check`
- Obligations: fast-remediation Verification and Evidence; `AGENTS.md` Completion Standard; candidate-bound review evidence.
- Locations: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T02-R2-fast-remediation.md:3`, `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T02-R2-fast-remediation.md:4`, and `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T02-R2-fast-remediation.md:5`.
- Scenario: run the required check against the immutable cumulative range rather than an already-clean working tree: `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..6f46cd5dd9bc1181dc3b34a699f720ecb86b84c7`.
- Consequence: the command exits 2 on three trailing-space lines, so the recorded clean result is not candidate-bound and the repository completion check is unmet.
- Supporting evidence: both the orchestrator and independent reviewer reproduced the same three lines and exit status against the exact candidate.
- Counterevidence: the spaces are Markdown hard-break formatting and do not affect executable behavior; all eight focused tests pass. The repository check nevertheless rejects them and the task explicitly requires a clean result.
- Ponytail recommendation: remove the two trailing spaces from the three metadata lines. Add no check, allowlist, formatting exception, or test.
- Required outcome: the cumulative candidate produces no whitespace errors.
- Closure verification: `git diff --check f21d0efd436bc3bb41d060c1475a4b999d7a4acb..<new-candidate>` exits 0.

## Follow-up
- None.

## Obligation Coverage
- The implementation closes all original Task 02 and R1 behavioral obligations: signed authoritative Card UIDs, per-record OTLP scope rejection with valid-sibling partial success, shared duplicate-name refusal, request-wide Arrow ordinals, built-in canonical physical/value validation, deterministic exact Parquet layout identity, and replay identity. Approval is withheld only for removal of contradictory unused promotion evidence and the failed required whitespace check.

## Code Review Coverage
- Baseline lenses: independent reviewer `t02_r2_independent_review` covered correctness, security, code quality, maintainability, tests/DX, architecture/contracts, persistence/transactions, async/concurrency, Bifrost/Vala, and source-backed PyO3/UI/TypeScript applicability in `BIFROST-OTEL-T02-R2-6f46cd5dd-reviewer-1.md`; the orchestrator independently validated its candidates and clean conclusions.
- Specialist trigger audit: the baseline reviewer covered signed-scope authorization, Arrow schema validation, Scribe WAL/replay, Parquet sealing and Oracle admission, promotion evidence, cancellation, and test-support changes with named source inspection. PyO3, UI, TypeScript, generated contracts, manifests, and dependency changes are not applicable because no production surface in those categories changed.
- Changed production files: the independent reviewer inspected all cumulative owners in catalog/contracts, Gate/limits, every signal table/projector, Scribe ingress/stamping/WAL/replay/Parquet/publication, schema/Parquet/Oracle, auth/spec, server cancellation, and test-support; it also checked deletion of the parallel OTLP Scribe modules. The orchestrator re-inspected the four R2 commits, all exact-fingerprint callers, all promotion-fingerprint consumers, and the candidate-bound evidence artifact.
- Obligations: the independent reviewer mapped REQ-001 through REQ-005, REQ-020, REQ-022, INV-002/003/006/007/008/009/010/011, AC-002/006/007/008/011/012, original Scenarios 1-4, R1 Scenarios 1-6, and all seven prior findings to production and test evidence.
- High-risk boundaries and user-facing behavior: principal signed scope to table-owned OTLP projection; accepted subset to Gate partial success; canonical Arrow to shared schema/duplicate validation; Scribe stamping to WAL/fence/ACK/replay; Arrow layout to Parquet footer, staging, Oracle admission, and promotion record; and unshipped durable JSON evidence were independently covered.
- Adversarial clean evidence: mixed authorized/unauthorized/missing/UID-less OTLP records, hostile client UID, duplicate dynamic `card_ref`, nested metadata insertion order, exact offset widths/timezones/nullability, reconstructed footer schemas, truncated/double replay, cancellation races, empty batches, and ingest database/cache lookup were attempted. The candidate resists them; only the contradictory unused promotion field and candidate-bound whitespace check remain.

## Validation Ledger
- `FIND-02-R1-1` closure: accepted; Gate passes verified signed scope to all table-owned projectors and only accepted siblings reach Scribe.
- `FIND-02-R1-2` closure: accepted; one shared `decode_rows` guard refuses duplicates for every table kind before fingerprint, correlation, or persistence.
- `FIND-02-R1-3` closure: accepted; exact hashing is deterministic, recursive, metadata-independent, nullability-aware, and confined to the Parquet memory-envelope path.
- Independent suggestion on the promotion fingerprint: elevated to blocking `FIND-02-R2-1`; the field is actively persisted under a false exact-footer contract, is unconsumed, and the user confirms it is unshipped, making deletion the smallest safe pre-ship result.
- Independent suggestion on trailing spaces: elevated to blocking `FIND-02-R2-2`; the explicit required candidate-bound command fails and the recorded clean result cannot prove the immutable subject.
- Broad non-Bifrost, Forge, and unrelated baseline failures: rejected under the human verification-scope override.

## Prior Finding Closure
- FIND-02-1: closed.
- FIND-02-2: closed.
- FIND-02-3: closed.
- FIND-02-4: closed.
- FIND-02-R1-1: closed by `2b41ed20f`.
- FIND-02-R1-2: closed by `19e71d59e`.
- FIND-02-R1-3: closed by `48e4553b1` and `6f46cd5dd`.

## Routing
- Next skill: `$wyrd-plan`
- Finding IDs: `FIND-02-R2-1`, `FIND-02-R2-2`
