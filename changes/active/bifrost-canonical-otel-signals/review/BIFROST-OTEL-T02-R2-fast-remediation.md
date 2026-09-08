# BIFROST-OTEL-T02-R2 — Fast convergence closeout

- Status: READY
- Scope: one remediation task for `FIND-02-R1-1`, `FIND-02-R1-2`, and `FIND-02-R1-3`
- Candidate reviewed: `e09c2c72d4bb83064096faa0dbe99ce16ce46ff7`
- Authority: approved `spec.md` revision 9 and `BIFROST-OTEL-T02-R1-e09c2c72d-verdict.md`

## Goal

Close the three remaining correctness holes without changing the approved architecture:

- Gate authenticates and routes; it does not own OTLP decoding or table semantics.
- Signal tables own OTLP projection and per-record rejection.
- Scribe resolves the registered table schema, validates canonical Arrow, stamps managed columns, and owns WAL/fence/ACK.
- Missing `card_ref` remains valid. A present `card_ref` must be within the principal's verified signed scope, and `card_uid` comes only from the matching signed scope member.
- Ingest performs no PostgreSQL or cache lookup.
- Dynamic-table registration/version behavior and public contracts do not change.

This is one task. Do not split it.

## 1. Preserve valid siblings in mixed OTLP requests

### Issue

The trace, log, and metric projectors validate `wyrd.card_ref` syntax but do not receive the verified signed scope. Gate therefore sends every syntactically valid record to Scribe in one batch. Scribe correctly fails a whole canonical batch when any present reference is out of scope or lacks a trusted signed UID, but that is too late for OTLP: revision 9 requires only the affected OTLP record to be rejected and its valid siblings to be returned as partial success.

Example: 99 valid or uncorrelated spans plus one out-of-scope span currently stores zero instead of 99.

### Affected code

- `crates/vala/vala-bifrost-redux/src/gate/mod.rs`
- `crates/vala/vala-bifrost-redux/src/tables/signal.rs`
- `crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs`
- `crates/vala/vala-bifrost-redux/src/tables/logs/records.rs`
- `crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs`
- Existing correlation tests in the three signal-table test modules
- Existing Gate test `mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals`

### Ponytail decision

Reuse `RecordCorrelation`, `CardRefScope::authorizes`, `CardRef::same_identity`, and the projectors' existing rejection accounting. Pass the verified signed `CardRefScope` as a borrowed optional input from Gate to each table-owned projector. In the shared correlation extraction/check:

1. Missing/null `card_ref` is accepted and produces null `card_uid` later.
2. A present reference must parse, match a signed scope member by exact identity, and that member must carry a trusted UID.
3. Failure rejects that record before its accumulator changes.
4. Accepted order is unchanged; Gate sends only the accepted batch to Scribe and returns the exact projector outcome.
5. Scribe keeps its existing whole-frame scope/UID validation as defense in depth and for canonical Arrow.

Do not add a registry lookup, cache, service, trait, second UID map, or Gate-owned decoder.

### Minimal proof

- Extend each existing `optional_card_correlation_is_atomic_and_lossless` test with one mixed request containing missing, in-scope, out-of-scope, and UID-less references. Assert accepted rows/order and rejected count/reason.
- Extend `gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals` to prove only accepted rows reach Scribe, ordinals are contiguous over accepted rows, trusted signed UIDs are stamped, and the returned partial-success count is exact.

## 2. Reject duplicate Arrow column names for every table

### Issue

The Arrow IPC payload contains its schema. Gate authenticates and routes the raw frame. Scribe structurally decodes it, resolves the table's registered schema from the authenticated tenant and table FQN, removes permitted correlation fields from the user-schema fingerprint, compares the remaining fields with the registered schema, validates correlation, stamps managed fields, and then writes WAL.

Built-in signal tables also run a duplicate-name check. Dynamic and pre-declared tables do not. Because `card_ref` is an allowed client correlation field rather than a server-owned field, two `card_ref` columns are removed from the user-schema comparison. Current name lookup checks only the first; later projection removes both. A first authorized value can therefore hide a second out-of-scope assertion.

### Affected code

- `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs`
- Existing dynamic/pre-declared Arrow ingress tests

### Ponytail decision

Move/reuse the existing one-pass `HashSet` duplicate-name check from `enforce_canonical_source_contract` at the shared `decode_rows` boundary, before fingerprint and correlation processing. It must apply to built-in, dynamic, and pre-declared tables.

Keep built-in-only exact field/value validation where it is. Do not change the permitted correlation columns, server-owned column list, catalog resolution, table registration/version semantics, or Scribe ownership. Add no validator abstraction.

### Minimal proof

- Add one focused dynamic-table test with duplicate `card_ref` columns where the first is null or authorized and the second is out of scope.
- Assert `InvalidFrame` before WAL/fence/ACK. The same shared check covers every duplicate field name; do not add one test per column or table kind.

## 3. Repair the existing Parquet memory-envelope checksum

### Issue

This is an Oracle query-admission safeguard, not a new ingest schema system. The pre-existing Parquet memory envelope binds stored memory estimates to the Arrow layout used to calculate them. Task 02 normalized the shared catalog fingerprint for valid Iceberg round trips; that normalized identity cannot distinguish layouts such as `Utf8` and `LargeUtf8`. Commit `b44200d43` therefore restored a private exact checksum for the Parquet envelope.

The split is necessary, but the restored formula hashes top-level field name plus `Debug` output of the data type. It omits top-level nullability and can include nested metadata `HashMap` iteration order. Equivalent reconstructed schemas can therefore disagree, while a real nullability change can collide.

### Affected code

- `crates/vala/vala-bifrost-redux/src/schema/fingerprint.rs`
- Existing exact-fingerprint and Parquet memory-envelope tests in `crates/vala/vala-bifrost-redux/src/parquet/memory.rs`

No other `from_arrow_schema` caller should change.

### Ponytail decision

Keep the existing normalized catalog fingerprint and the existing private `from_arrow_schema_exact` entry point. Fix that helper in place with one deterministic recursive encoding over the layouts already admitted by the Parquet memory envelope:

- include field and child names, order, nullability, and exact Arrow type variants and parameters;
- preserve distinctions such as `Utf8`/`LargeUtf8` and `Binary`/`LargeBinary`;
- do not use `Debug` output;
- ignore metadata that does not affect Arrow memory layout, avoiding metadata-order instability;
- keep SHA-256 and existing callers.

Do not add another fingerprint type, dependency, registry, persisted field, public contract, or generalized schema framework. Unsupported layouts continue to be refused by the existing Parquet logical sizer.

### Minimal proof

- Add one exact-fingerprint test containing all necessary assertions: equivalent nested schemas with different metadata insertion order match; top-level and nested nullability changes differ; `Utf8`/`LargeUtf8` and `Binary`/`LargeBinary` differ.
- Extend one existing Parquet memory-envelope round-trip test to reconstruct an equivalent schema independently, accept it, and reject one layout change.

## Implementation order

1. Move the shared duplicate-name guard.
2. Pass signed scope into the existing OTLP projector path and filter rejected records before accumulation.
3. Replace exact fingerprint debug hashing with deterministic layout hashing.
4. Run the focused tests after each edit, then the two relevant Bifrost lanes once.

## Completion

The task is complete when:

- mixed OTLP requests store valid siblings and report exact partial success for all three signals;
- missing `card_ref` remains valid, present references use only signed scope/UID data, and ingest performs no database/cache IO;
- every Arrow table rejects duplicate field names before persistence;
- the Parquet exact checksum is deterministic and detects exact layout/nullability differences while catalog/Iceberg normalization remains unchanged;
- no new architecture, dependency, public contract, table-version behavior, or persistence path was introduced.

## Verification

Run only the smallest existing lanes that cover this write set:

```bash
# Every added or modified named Rust test: use its exact fully qualified name.
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support -E 'test(=<exact_test_name>)'

mise run test:bifrost:integration:redux
mise run test:bifrost:journey:scribe
mise run fmt
mise run lints
git diff --check
```

Do not run or require `mise run gate`, `test:wyrd`, Forge tests, unrelated Oracle suites, Python/TypeScript lanes, or new test harnesses for this remediation.

## Evidence to record

- Commit SHA for this single remediation task.
- Exact focused test names and results.
- Relevant Bifrost lane totals.
- Confirmation that `from_arrow_schema_exact` callers remain limited to the Parquet memory-envelope/layout path.
- Confirmation that no PostgreSQL/cache call was added to Gate, table projection, or Scribe ingest.

---

## Execution evidence

Status: COMPLETE. One task, three commits (kept separate per finding for
review; they form one cumulative candidate).

| Finding | Commit | Change |
| --- | --- | --- |
| FIND-02-R1-2 | `19e71d59e` | Duplicate-name guard moved from `enforce_canonical_source_contract` to the shared `decode_rows` boundary; it now covers built-in, dynamic, and pre-declared tables before fingerprint, correlation, and stamping. |
| FIND-02-R1-1 | `2b41ed20f` | `RecordCorrelation::extract` takes the verified signed `CardRefScope`; Gate passes `auth.principal.card_ref_scope()` into all three table projectors. A present `card_ref` must parse, match a signed member by `same_identity`, and that member must carry a UID. Rejection is per record, before its accumulator changes. |
| FIND-02-R1-3 | `48e4553b1` | `from_arrow_schema_exact` replaced with a deterministic recursive encoding (names, order, nullability, exact variants and parameters; metadata excluded). |

### Scenario cycles

- RED `scribe::execution_lanes::tests::duplicate_arrow_column_names_fail_closed_for_dynamic_tables`
  — a dynamic batch with two `card_ref` columns (first null, second out of
  scope) was accepted. GREEN after moving the guard.
- RED the three `optional_card_correlation_is_atomic_and_lossless` tests —
  extended with out-of-scope and UID-less siblings, they failed on the
  unthreaded scope. GREEN with accepted 3 / rejected 4 and the first
  traversal-order reason unchanged.
- RED `gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals`
  — extended to six resources over a scoped service principal; asserts
  accepted 3 / rejected 3, one Scribe call, `rows == [3]`, and the exact
  `card_ref` values handed over. The Scribe double additionally asserts every
  non-null reference it receives is authorized by the frame principal's scope.
- RED `schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only`
  — failed on the metadata-insertion-order assertion (the reported defect).
  GREEN with the recursive encoder; also pins top-level and nested nullability
  and `Utf8`/`LargeUtf8`, `Binary`/`LargeBinary`.

### Focused commands (all passing)

```
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features test-support,bench-support -E 'test(=<name>)'
```

for `schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only`,
`parquet::memory::tests::bifrost_footer_fingerprint_does_not_alias_utc_spellings`,
`scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence`,
`scribe::execution_lanes::tests::duplicate_arrow_column_names_fail_closed_for_dynamic_tables`,
`gate::tests::mixed_otlp_projection_assigns_only_accepted_contiguous_ordinals`,
and the three `optional_card_correlation_is_atomic_and_lossless` tests — 8 run,
8 passed.

### Lane totals

- `mise run test:bifrost:integration:redux` — 958 run, 958 passed.
- `mise run test:bifrost:journey:scribe` — 20 run, 20 passed.
- `mise run fmt`, `mise run lints` clean; `git diff --check` clean.

### Confirmations

- `from_arrow_schema_exact` callers are unchanged and remain confined to
  `parquet/memory.rs` (envelope construction, footer verification, leaf-width
  profile, and its `schema_fingerprint` entry point).
- No PostgreSQL, registry, or cache call was added to Gate, table projection,
  or Scribe ingest. The per-record authorization reads signed claims only.
- No new public contract, dependency, table-version behavior, or persistence
  path.

### Bounded corrections

- The remediation named `tables/{traces/spans,logs/records,metrics/points}.rs`
  for the projector edits; the `RecordCorrelation::extract` call sites actually
  live in the sibling `projection.rs` of each signal. Edited there.
- `scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence`
  asserted the footer's `wyrd.bifrost.schema_fingerprint` equalled
  `SealedArtifactEvidence::schema_fingerprint`. The footer carries the Parquet
  envelope's *exact* identity while the evidence field reports the *normalized*
  catalog identity; the two only coincided while both reduced to the same
  `Debug` string. The assertion now compares the exact identity the footer
  actually carries, computed through `parquet::memory::schema_fingerprint`.

  Observation for review, deliberately not changed here: that evidence field's
  own rustdoc says "the fingerprint the footer was sealed with", and its value
  flows into the persisted `ScribePublishedHotFileV1.promotion_record`. It is
  computed with `from_arrow_schema` (normalized) while every other Scribe
  staging and publication path uses `parquet::memory::schema_fingerprint`
  (exact). Nothing currently compares the persisted value, so this is a
  documentation/consistency gap rather than a live defect — and changing a
  persisted value is outside this remediation's scope.
- Four `vala-bifrost-redux` lib tests (two `catalog::bifrost_catalog::
  production_pin_tests`, two `scribe::persistence::tests`) fail under a bare
  `cargo nextest` invocation both before and after this change; they pass in
  the `test:bifrost:integration:redux` lane, which provides their environment.

## R2 verdict closeout (`BIFROST-OTEL-T02-R2-6f46cd5dd-verdict.md`)

### FIND-02-R2-1 — unused normalized fingerprint deleted end to end

The observation recorded above is now closed by deletion rather than by
correcting the value. `schema_fingerprint` is gone from
`SealedArtifactEvidence`, `BoundedParquetArtifact`, `PublishedHotFileIdentity`,
and `ScribePublishedHotFileV1`, along with its single producer
(`inspect_sealed_artifact`), its single consumer (`file_list_writer`), and the
three fixtures that supplied it (`promotion.rs`, `staging.rs`,
`scribe_workload.rs`). No replacement field, compatibility shim, version bump,
or migration was added: the record is unshipped and nothing read the value.

`from_arrow_schema_exact` keeps exactly one caller,
`crate::parquet::memory::schema_fingerprint`, which remains the identity the
Parquet footer is sealed with and the identity staging, replay, and Oracle
admission compare. Two rustdoc claims that named the removed field were
narrowed to what the code still proves.

### FIND-02-R2-2 — candidate-bound whitespace

The three metadata lines carried trailing double-space Markdown hard breaks.
They are now list items, which keeps them on separate rendered lines without
trailing whitespace. `git diff --check` exits 0.

### Verification

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib
  --features test-support,bench-support -E
  'test(=scribe::promotion::tests::promotion_record_round_trips_and_encodes_deterministically)
  | test(=scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence)
  | test(=scribe::parquet_writer::tests::sealed_artifacts_carry_footer_agreeing_iceberg_metrics)
  | test(=schema::fingerprint::tests::exact_fingerprint_commits_layout_and_nullability_only)
  | test(=parquet::memory::tests::bifrost_footer_fingerprint_does_not_alias_utc_spellings)'`
  — 5/5 passed.
- `mise run test:bifrost:integration:redux` — 958/958 passed.
- `mise run test:bifrost:journey:scribe` — 20/20 passed.
- `mise run fmt`, `mise run lints` — clean.
- `git diff --check` — exit 0.
