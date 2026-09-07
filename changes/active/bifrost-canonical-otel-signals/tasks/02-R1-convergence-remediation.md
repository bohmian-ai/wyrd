---
id: BIFROST-OTEL-T02-R1
title: Repair Gate/Scribe convergence
kind: remediation
mode: REMEDIATE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 9
parent_task: BIFROST-OTEL-T02
remediates: [FIND-02-1, FIND-02-2, FIND-02-3, FIND-02-4]
depends_on: [BIFROST-OTEL-T02]
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-022]
invariants: [INV-003, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-002, AC-006, AC-007, AC-008, AC-010, AC-011, AC-012]
---

# Task 02 R1 — Gate/Scribe convergence remediation

## Outcome and value

Generic telemetry remains attributable to its authenticated publisher without
requiring a Card. Optional Card correlation resolves from trusted signed
claims without Postgres on ingest. Canonical Arrow streams retain one
request-wide row identity, and canonical table validation and physical field
identity survive stamping, WAL, replay, and readback.

Required execution skill: `$wyrd-implement`.

## Fixed ownership and non-goals

- Server transport decodes bounded OTLP. Canonical tables project OTLP and
  validate canonical Arrow. Gate authenticates and routes. Scribe owns managed
  stamping, request-wide ordinals, WAL, fence, replay, and ACK.
- `wyrd-auth` enriches the existing bounded `CardRefScope` during token mint and
  refresh. The existing compact claim remains the sole Card-scope contract.
- OTLP correlation uses the exact record attributes `wyrd.card_ref` and
  `wyrd.run_id` under approved spec revision 9.
- No Gate Arrow decoder, table projector, schema validator, ordinal owner, new
  claim object, ingest Postgres/cache lookup, resolver trait, validator trait,
  second table registry, WAL version, digest change, dependency, migration,
  harness, test target, or compatibility path.

## Scenario 1 — Signed scope preserves every authoritative Card UID

**Behavior.** Token mint and refresh resolve every bounded tenant-local scope
member once. The signed and verified claim retains each canonical Card identity
and registry UID. Maps REQ-022, INV-006, and AC-012.

**RED.** Add:

- `reference::tests::card_ref_scope_serde_preserves_resolved_uids`
- `card_scope::pg_tests::resolve_scope_populates_every_member_uid`
- `refresh::pg_tests::refresh_signs_resolved_scope_uids`

Prove the compact claim round-trips `#uid`; the tenant-scoped mint walk replaces
root and secondary authored references with exact registry identities and UIDs;
and real refresh rotation, access-token signing, verification, and runtime
`Principal` projection preserve both UIDs.

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=reference::tests::card_ref_scope_serde_preserves_resolved_uids)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=card_scope::pg_tests::resolve_scope_populates_every_member_uid)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=refresh::pg_tests::refresh_signs_resolved_scope_uids)'"
```

**GREEN.** In `wyrd_auth::card_scope::resolve_card_ref_scope`, construct each
scope member from the existing tenant-local `get_card_by_ref` result, including
`ParsedCardRow.card_uid`. Keep the bounded walk, `TenantConn`, mint/refresh
callers, token ceiling, and string serialization.

**REFACTOR.** Reuse `CardRefScope`; do not add a parallel UID map.

## Scenario 2 — Every signal table applies the approved OTLP contract

**Behavior.** Each table reads only the final record-level `wyrd.card_ref` and
`wyrd.run_id`. Missing values project null. Valid strings use existing
`CardRef` and `RunId` grammars. Wrong-typed or malformed final values reject
only that record. All original attributes remain losslessly stored. Maps
REQ-001, REQ-005, REQ-022, INV-010, and AC-011/AC-012.

**RED.** Add:

- `tables::traces::tests::optional_card_correlation_is_atomic_and_lossless`
- `tables::logs::tests::optional_card_correlation_is_atomic_and_lossless`
- `tables::metrics::tests::optional_card_correlation_is_atomic_and_lossless`

Each test covers missing, valid, duplicate, wrong-typed, and malformed final
values. Assert missing to null; final duplicate wins; valid text reaches the
writable correlation columns; every ordered source attribute remains
byte-for-byte lossless; only the affected record is rejected; and accepted
count, rejected count, and first reason are exact.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=tables::traces::tests::optional_card_correlation_is_atomic_and_lossless)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=tables::logs::tests::optional_card_correlation_is_atomic_and_lossless)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=tables::metrics::tests::optional_card_correlation_is_atomic_and_lossless)'
```

**GREEN.** Reuse one small table-layer helper for the shared tri-state
record-attribute rule. Call it from `project_resource_spans`,
`project_resource_logs`, and each metric point projection before mutating its
column accumulator. Append nullable correlation fields without rewriting the
lossless source attributes.

**REFACTOR.** Reuse the existing final-entry lookup and identifier parsers. No
transport-specific mapping belongs in Gate or Scribe.

## Scenario 3 — Scribe stamps optional and scoped Card correlation

**Behavior.** Every accepted row receives non-null `principal_id`. Missing or
null `card_ref` yields null `card_uid`. Root and secondary identities get their
own signed UIDs. Malformed, UID-less, or out-of-scope assertions fail closed
without registry IO. Maps REQ-002/REQ-003/REQ-022, INV-007/INV-011, and
AC-006/AC-012.

**RED.** Add
`scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_trusted_uids`.
Cover absent and explicit-null correlation, root and secondary members, a
client UID conflicting with the signed UID, UID-less signed membership,
malformed text, and out-of-scope identity. Assert exact `principal_id` and null
or trusted `card_uid` values.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_trusted_uids)'
```

Extend the existing Scribe journey with
`write_read::scribe_optional_and_scoped_card_correlation_journey`. Through the
real SDK/server, write uncorrelated, root-correlated, and secondary-correlated
rows; read exact publisher/Card values; refuse an out-of-scope Card; and
construct the server without an ingest Card resolver.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=write_read::scribe_optional_and_scoped_card_correlation_journey)'"
```

**GREEN.** Keep nulls valid in Scribe's scope validator. For non-null parsed
identity, use `CardRefScope::authorizes`, find the same verified member by
`same_identity`, require its UID, and stamp that UID. Never substitute the
principal root or client UID.

**REFACTOR.** Reuse `CardRef`, `CardRefScope`, and the existing mint-time
registry walk. Remove root-only UID substitution reached by the test.

## Scenario 4 — Native Arrow ordinals span the complete request

**Behavior.** All RecordBatches in one Arrow IPC write share one batch ID and
receive disjoint contiguous ordinal ranges in stream order. Maps REQ-002–REQ-005,
INV-007–INV-009, and AC-002/AC-007.

**RED.** Extend
`scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path`
as a Scribe seam test. Project one accepted nested dataset through its owning
OTLP table projector. Submit it under one batch ID as `CanonicalIngress`; split
the same rows into two non-empty RecordBatches, encode one Arrow IPC stream,
and submit under a different batch ID as `ArrowIpc`. Read both and compare user
columns exactly. Assert input order, one Arrow batch ID, unique contiguous
ordinals `0..total_rows`, one fence, and identical replayed row identities.
This calls `Scribe::ingest_frame` directly and is not public-route proof.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path)'
```

**GREEN.** Add one `next_row_ordinal` to request-owned `NativeSliceProducer`.
Pass its current value through `decode_native_batch`, `DecodeContext`,
`stamp_correlation_columns`, and `append_managed_columns`. Stamp
`start..start + row_count`, then advance only after successful stamping with
checked arithmetic. Use existing `TooManyRows` before WAL mutation on overflow.

**REFACTOR.** Do not concatenate the streaming path or move ordinal state to
Gate, global state, WAL, or table projection.

## Scenario 5 — Registry validation preserves canonical physical identity

**Behavior.** Canonical built-ins reject exact physical/value drift and retain
the table-owned managed fields through stamping and storage. Dynamic tables
retain their current catalog-fingerprint policy. Maps REQ-001/REQ-003/REQ-004,
INV-003/INV-009/INV-010, and AC-002/AC-008.

**RED.** Add:

- `tables::tests::builtin_registry_dispatches_canonical_value_validation`
- `tables::tests::resolved_identity_rejects_normalized_physical_drift`

The first supplies schema-valid but value-invalid trace, log, and metric
batches, including a metric-kind column violation, and proves a noncanonical
built-in has no canonical validator. The second covers `Utf8`/`LargeUtf8`,
`Binary`/`LargeBinary`, timezone, nested metadata, field IDs, unknown `wyrd_*`,
and duplicate reserved fields. Extend the Scenario 4 persistence test to assert
every stored managed `Field` equals the table-owned physical field and value.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=tables::tests::builtin_registry_dispatches_canonical_value_validation)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=tables::tests::resolved_identity_rejects_normalized_physical_drift)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path)'
```

**GREEN.** Define one function-pointer alias matching the existing validator
shape. Add `canonical_validator: Option<CanonicalBatchValidator>` to
`BuiltinTableDefinition`, a default-`None` associated member on `DomainTable`,
and copy it in the existing definition constructor. Trace/log tables provide
thin wrappers around `validate_canonical_user_batch`; metrics uses existing
`validate_metric_points`; other built-ins remain `None`.

In native planning, resolve that existing definition; reject unknown/duplicate
reserved fields; extract permitted correlations; invoke the validator; and
compare remaining user fields exactly with `(definition.arrow_fields)()` before
stamping. Clone correlation and managed `Field`s from `(definition.schema)()`
while constructing only their arrays. After stamping, compare complete fields
and `CanonicalPhysicalFingerprint` with the same definition. Dynamic tables
keep the existing `SchemaFingerprint` and managed-field policy.

**REFACTOR.** No validator trait, table enum, Scribe table-name match, second
registry, or global redesign of the Iceberg-facing normalized fingerprint.

## Scenario 6 — Replay proves identity, digest, and physical schema

**Behavior.** Replay restores accepted nested slices once, verifies the
authenticated logical commit identity and physical schema, and fails closed on
structurally invalid Arrow inside a valid WAL frame. Maps REQ-003–REQ-005,
INV-008–INV-010, and AC-002/AC-006–AC-008/AC-011.

**RED.** Strengthen
`scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest`.
Independently recompute the logical fence digest from recovered slices in
ordinal order as
`slice_index || schema_fingerprint || logical_data_digest || logical_data_len`
and compare it with `ReplayedCommitIdentity::slice_set_digest`. Replay twice;
assert identical rows, row identities, and one commit.

Decode every valid replayed IPC slice. Compare complete `Schema::fields()` with
`(definition.schema)().fields()` from `builtin_table(namespace, name)` and its
`CanonicalPhysicalFingerprint` with
`ResolvedSchemaIdentity::for_builtin(definition).canonical_physical_fingerprint`.

Then truncate a valid nested Arrow IPC after its schema message but inside the
first RecordBatch body and wrap it with existing
`WalWriter::append_and_commit_for_replay_test`. Call `replay_wal_directory`,
select the reconstructed state, and pass it to `Memtable::decode_replayed`.
Assert exact existing `replayed Arrow IPC decode failed` and that no
`FrozenMemtable` is returned. Do not call `restore_replayed` or
`insert_replayed_frozen`; do not assert that the valid WAL COMMIT or a prior SQL
fence is absent.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest)'
```

**GREEN.** No production digest change is expected. Reuse recursive IPC
validation, WAL payload validation, logical fence digest, registry definition,
and duplicate suppression. Fix only a separate source-proven defect exposed by
the RED assertions.

**REFACTOR.** Record here—not in the immutable parent task—that T02 conflated
the WAL payload digest with the distinct logical SQL-fence digest.

## Expected write set and consumer closure

- `crates/wyrd-spec/src/reference.rs`
- `crates/wyrd/wyrd-auth/src/{card_scope,refresh}.rs`
- `crates/vala/vala-bifrost-redux/src/tables/{mod,signal,traces,logs,metrics}`
- `crates/vala/vala-bifrost-redux/src/scribe/{execution_lanes,preprocess}.rs`
- existing Scribe persistence and replay test modules
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs`

Consumer closure includes token mint, refresh, signing, verification, canonical
Arrow SDK writes, OTLP HTTP/gRPC projection, Scribe live/replay, Forge/Iceberg,
and Oracle readback. Authority is already updated in revision 9 of the active
spec and the owning Wyrd/Bifrost/telemetry architecture documents.

## Broader verification

Run every named command, then:

```bash
mise run test:wyrd
mise run test:bifrost:journey:scribe
mise run codegen:check
mise run fmt
mise run lints
mise run verify:bifrost
mise run gate
git diff --check
```

Record scenario RED/GREEN/REFACTOR evidence; claim and row outcomes; absence of
an ingest registry/cache dependency; multi-batch ordinals; physical fields;
logical digest; duplicate replay; corrupt nested refusal; and cumulative
closure of FIND-02-1 through FIND-02-4.

## Material stop conditions

- The bounded signed scope cannot carry registry UIDs within the token ceiling.
- Correctness requires Postgres/cache after token verification, a persisted
  schema or WAL format change, or another schema/validator authority.
- Approved OTLP keys, identifier grammar, authz/error result, tenant boundary,
  durability boundary, or public query behavior must change.

Return those to `$wyrd-spec`.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md` revision 9
- `changes/active/bifrost-canonical-otel-signals/tasks/02-gate-scribe-convergence.md`

## Completion evidence

Append scenario-by-scenario results, exact commands/outcomes, final changed-file
audit, consumer closure, and independently proven baseline failures. Do not edit
the immutable parent task.
