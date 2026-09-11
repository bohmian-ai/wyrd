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
- `card_ref` is supplied per record by the client. It is never derived from the
  authenticated principal or replaced with the principal's root Card. OTLP
  correlation uses the exact record attributes `wyrd.card_ref` and
  `wyrd.run_id` under approved spec revision 9.
- Every present client-supplied `card_ref` must match an exact
  `(kind, space, name, version)` identity in the authenticated principal's
  verified signed `CardRefScope`. Scribe stamps only the UID paired with that
  exact scope member. Missing or null `card_ref` is valid and stamps null
  `card_uid`; malformed, UID-less, or out-of-scope values fail closed.
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

**Behavior.** The client supplies optional correlation independently on each
record. Each table reads only the final record-level `wyrd.card_ref` and
`wyrd.run_id`; neither the table nor any downstream owner derives `card_ref`
from the authenticated principal. Missing values project null. Valid strings
use existing `CardRef` and `RunId` grammars. Wrong-typed or malformed final
values reject only that record. All original attributes remain losslessly
stored. Maps REQ-001, REQ-005, REQ-022, INV-010, and AC-011/AC-012.

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

**Behavior.** Every accepted row receives non-null `principal_id`. A missing or
null client-supplied `card_ref` yields null `card_uid`; Scribe never substitutes
the principal's root Card. Every present client-supplied `card_ref` must match
an exact identity in the authenticated principal's verified signed
`CardRefScope` before the corresponding trusted UID is stamped. Root and
secondary identities get their own signed UIDs. Malformed, UID-less, or
out-of-scope assertions fail closed without registry IO. Maps
REQ-002/REQ-003/REQ-022, INV-007/INV-011, and AC-006/AC-012.

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

**GREEN.** Keep missing and null client correlation valid. For every present
client-supplied `card_ref`, parse its exact `(kind, space, name, version)`, call
`CardRefScope::authorizes` against the authenticated principal's verified signed
scope, find that same member by `same_identity`, require its trusted UID, and
stamp only that UID. Reject the row when any step fails. Never derive or
substitute the principal's root Card, and never trust a client-supplied UID.

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

---

## Execution evidence

Six commits, one per scenario, on `oracle-distributed`:

| Scenario | Commit |
| --- | --- |
| 1 — signed scope carries every authoritative UID | `2fc97abbd` |
| 2 — every signal table applies the OTLP contract | `4ebec40fc` |
| 3 — Scribe stamps optional and scoped Card correlation | `90c217c4c` |
| 4 — native Arrow ordinals span the complete request | `0605df84c` |
| 5 — registry validation preserves canonical physical identity | `2a72148ca` |
| 6 — replay proves identity, digest, and physical schema | `25607e294` |

### Scenario 1 — signed scope preserves every authoritative Card UID

RED: `wyrd-auth` scope-walk tests failed because `resolve_card_ref_scope` dropped
the registry UID off every non-root member. GREEN: the walk now carries the
resolved UID onto each expanded `Spec::Service.components[].ref` member.
REFACTOR: none beyond documenting `# Errors`/`# Panics` on the walk.

### Scenario 2 — every signal table applies the approved OTLP contract

RED: the three named projector tests
(`traces|logs|metrics::tests::optional_card_correlation_is_atomic_and_lossless`)
failed — no projector read `wyrd.card_ref` / `wyrd.run_id`.

GREEN: one shared tri-state extractor in `tables/signal.rs`
(`RecordCorrelation::extract`) reuses the existing final-entry attribute lookup
and identifier parsers, and is called by all three table-owned projectors before
they mutate their accumulators. Missing, valid, duplicate (final wins),
wrong-typed, and malformed values are proven for traces, logs, and metrics; a
wrong-typed or malformed value rejects only its own record (REQ-005 atomicity),
and every original OTLP attribute is preserved losslessly. `card_ref` is read
per record from the client payload and is never derived from the authenticated
principal. Two nullable correlation columns are appended for Scribe; Gate stays
authentication/routing only.

REFACTOR: `signal::without_correlation_columns` splits the two appended columns
back off, so every authority that compares a batch against its declared ledger —
schema identity, IPC/Parquet round trips — reads the ledger projection rather
than a special-cased column count.

### Scenario 3 — Scribe stamps optional and scoped Card correlation

RED: `optional_and_scoped_card_correlations_stamp_trusted_uids` failed — the
decoder resolved UIDs from the principal's own root Card.

GREEN: `resolve_card_uids` now decides entirely from signed claims. A null row
resolves to a null `card_uid` and consults no scope at all, so an uncorrelated
batch never needs a scope. A present reference must parse, must be authorized by
the principal's verified `CardRefScope`, and must match a signed member by exact
identity; only that member's signed UID is stamped. A client-supplied UID is
never trusted and the principal root is never substituted. No registry, Postgres,
or cache lookup is performed on the ingest path — resolution reads
`Principal::card_ref_scope()` only.

### Scenario 4 — native Arrow ordinals span the complete request

RED: `canonical_nested_batches_share_one_managed_wal_path` failed — each record
batch of one Arrow IPC stream restarted its ordinals at zero.

GREEN: `NativeSliceProducer` owns a request-wide `next_row_ordinal` cursor,
passes it into `DecodeContext::start_row_ordinal`, and advances it with checked
arithmetic (`TooManyRows` on overflow). The fixture drives the real metrics
projection through both payload modes, so the canonical batch and the split
Arrow stream must read back identical user columns and the same contiguous
`0..total_rows`.

REFACTOR: the six-plus stamping parameters became `DecodeContext`, which also
removed two `too_many_arguments` violations.

### Scenario 5 — registry validation preserves canonical physical identity

RED: `tables::tests::builtin_registry_dispatches_canonical_value_validation` and
`tables::tests::resolved_identity_rejects_normalized_physical_drift` failed —
`BuiltinTableDefinition` had no validator member.

GREEN: one function-pointer alias `CanonicalBatchValidator`, a
`canonical_validator: Option<CanonicalBatchValidator>` field on
`BuiltinTableDefinition`, a default-`None` `DomainTable::CANONICAL_VALIDATOR`
associated const (the existing constructor is a `const fn`, so this had to be an
associated const rather than a method), and one line copying it in `definition`.
Traces and logs supply thin wrappers around `validate_canonical_user_batch`;
metrics reuses the existing `validate_metric_points`; the other eleven built-ins
stay `None`.

Native and canonical planning resolve that definition once from the frame's
`TableRef` (`ScribeIngress::canonical_definition`) and carry it to the decode.
`enforce_canonical_source_contract` rejects a duplicate column name and any
unknown `wyrd_*` field, lifts the permitted correlations (`card_ref`, `run_id`,
`wyrd_event_time`), invokes the table's validator, and requires the remaining
user fields to equal `(definition.arrow_fields)()` exactly before stamping.
Stamping then constructs only the arrays: every correlation and managed `Field`
is cloned from `(definition.schema)()`, the locally assembled field list is
compared against it first, and `enforce_canonical_physical_identity` re-derives
the `CanonicalPhysicalFingerprint` and compares it with
`ResolvedSchemaIdentity::for_builtin`. Dynamic and pre-declared tables resolve to
`None` and keep the existing `SchemaFingerprint` and managed-field policy.

The Scenario 4 persistence test now writes to the real `vala.metrics.points`
built-in and asserts every stored managed `Field` equals the table-owned physical
field, plus the tenant isolation value on every row.

REFACTOR: no validator trait, table enum, Scribe table-name match, second
registry, or change to the Iceberg-facing normalized fingerprint. The drift test
was split into one named helper purely to satisfy `clippy::too_many_lines`.

### Scenario 6 — replay proves identity, digest, and physical schema

RED: the strengthened
`scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest`
asserted properties the previous test never checked.

GREEN: no production digest change was required. The fence digest is recomputed
independently from the recovered slices in ordinal order as
`slice_index || schema_fingerprint || logical_data_digest || logical_data_len`
(reusing `LogicalBatchDigest` and `logical_data_identity`) and compared with
`ReplayedCommitIdentity::slice_set_digest`; every recovered slice is decoded and
compared field-for-field with `(definition.schema)().fields()` and by
`CanonicalPhysicalFingerprint` against `ResolvedSchemaIdentity::for_builtin`; a
second `replay_wal_directory` must restore one commit with the same fence
identity, the same slice identities, and the same bytes; and a valid, CRC-clean
WAL frame carrying an Arrow stream truncated inside its first `RecordBatch` body
must make `Memtable::decode_replayed` refuse with the existing
`replayed Arrow IPC decode failed` and return no `FrozenMemtable`.

REFACTOR (recorded here, not in the immutable parent task): **T02 conflated the
WAL payload digest with the distinct logical SQL-fence digest.** They answer
different questions and the v6 COMMIT persists both. `validate_pending_batch`
recomputes `slice_set_digest(...)` over frame payloads and compares it with
`identity.wal_digest`; `replayed_commit_identity` is separately handed
`identity.logical_digest` for the fence. The production wiring is already
correct; the parent task's prose treated them as one value, which is why the
original test could pass while proving neither.

### Follow-on defects found and fixed during verification

Five more commits on the same branch, each a defect the scenario work exposed:

| Commit | Defect |
| --- | --- |
| `21a5a4cf6` | `wyrd-sql tests::transaction_discipline_is_documented` read two sentences that had drifted out of `architecture/v1/00-foundations/sql-foundation.md`; restored verbatim, doctrine unchanged. |
| `684d6c1e5` | `ScheduledQueryCaller::consume_to_terminal` used an unbiased `tokio::select!`, so an already-cancelled token could still consume a buffered stream to a successful outcome. Branch order is now `biased`, cancellation and the pinned deadline first. The same journey counted `vala.audit_outbox` the instant a query settled, before the production relay; it now waits on the Oracle residual counter the way `load::matrix` already does. |
| `1bb561a46` | The WAL disk-full injection tripped the same latch a real ENOSPC does, and retirement re-evaluates that latch against a host filesystem that is never full — an owed retirement cleared the refusal between the trip and the append under test. The injection now holds the condition until the journey releases it; retirement still owns clearing the latch. The sustained journey also claimed four concurrent tenants on a current-thread runtime; it is now `multi_thread`. |
| `7fe8e7cd9` | `QueryResourceProbe::memory_bytes` summed a `live_reservations` vector nothing pushes to since the live tail began draining into the query's own pool, so it reported zero for a query demonstrably holding bytes. It now reads the pool the drain fills. The Python RBAC journey also expected `shutdown()` to repeat a denial; a denial is terminal, nothing is retained for retry, so the assertion now matches the contract `wyrd-queue` implements. |
| `b44200d43` | **Regression introduced by this task.** `678cbba18` gave `SchemaFingerprint` the normalizations an Iceberg round trip needs — list-element naming, `Utf8`/`LargeUtf8` collapse, `UTC`/`+00:00`. `BifrostParquetMemoryEnvelope` reads the same fingerprint to answer a decode question, and `Utf8`/`LargeUtf8` are one Iceberg type but two offset widths, so a footer could validate against a schema whose layout does not match the file. The envelope now commits the raw Arrow spelling through `SchemaFingerprint::from_arrow_schema_exact`. |

### Broader verification

Lanes selected by write set. The write set is `vala-bifrost-redux`
(scribe, tables, schema, parquet, oracle), `wyrd-server/src/query/scheduled.rs`,
`wyrd-testing/src` and its scribe/server journey targets, `wyrd-mcp`'s Bifrost
query journey, and one Python integration test. Nothing touches `vala-sql`,
TypeScript, Forge, or the Oracle journey binary, so those lanes are not run.

| Command | Outcome |
| --- | --- |
| every scenario-named focused command | pass |
| `mise run test:bifrost:integration:redux` | 956 passed, 0 failed |
| `mise run test:bifrost:journey:scribe` | 20 passed |
| `mise run test:bifrost:journey:server` | 4 passed |
| `mise run test:bifrost:journey:mcp` | 7 passed |
| `mise run test:bifrost:journey:python` | 17 passed |
| `mise run fmt` | clean |
| `mise run lints` | clean |
| `git diff --check` | clean |

### Known failures outside this write set

- `wyrd-testing bifrost::forge_harness::worker_lifecycle_tests::*` (5). Their
  shared `lifecycle_fixture` calls `append_forge_file`, which drives a real
  Scribe append, on a `StandaloneForgeFixture` that `seed_synthetic_forge_group_for_test`
  builds with `scribe: None` by design. Left as is by owner decision: the
  `forge-compaction-refactor` branch replaces this harness.
- `wyrd-testing bifrost::scribe_workload::tests::scribe_workload_read_boundaries_may_not_reuse_an_earlier_read`.
  Pure in-memory validation of a `ScribeProductionWorkloadV1` record; touches no
  decode, stamping, fingerprint, or query path in this write set.
- `crates/wyrd/wyrd-testing/tests/bifrost/otlp/*.rs` are eight zero-byte
  placeholders, so `test:bifrost:journey:otlp` errors with "no tests to run".
  Authoring those journeys is its own change.
- Named out of scope by the task instruction and reproduced unchanged:
  `wyrd-spec query::tests::display_parse_roundtrip_for_small_queries` and the
  seven `wyrd-auth-verify verify_external_*` reqwest panics.

No test, gate, or check was weakened, ignored, deleted, or allow-listed.

### Cumulative closure of FIND-02-1 through FIND-02-4

- **FIND-02-1** (scope dropped authoritative UIDs) — Scenario 1: the scope walk
  carries every resolved member UID, and Scenario 3 stamps only those.
- **FIND-02-2** (per-record OTLP correlation unimplemented) — Scenario 2: one
  shared extractor, all three tables, atomic and lossless.
- **FIND-02-3** (native Arrow ordinals restarted per record batch) — Scenario 4:
  one request, one contiguous checked ordinal range, proven in both payload
  modes.
- **FIND-02-4** (canonical physical identity unenforced at ingest and unproven at
  replay) — Scenario 5 enforces it on the write path from the table's own
  registry definition; Scenario 6 proves it survives recovery and that a
  structurally invalid Arrow body fails closed.
