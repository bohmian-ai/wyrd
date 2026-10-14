# Queryable open-shaped Bifrost data

- Change ID: `SPEC-bifrost-variant`; approved main specification revision 18 plus separately approved later amendments through revision 26.
- Completed: 2026-10-14.
- Historical reviewed candidates include `6147cc617d81f2c03464043be698ab2565e9d745` and `861a0342f`; no final integrated PASS is claimed.
- Completion target: `06a89286943a64c83eb588562b308c3d35663d66`.
- Delivery reference: integration merge `46dc7e41b`; release reference not supplied.
- Completion authority: human-approved closeout extending the explicit completion override on 2026-10-14.

## Intent and implemented capability

OpenTelemetry attributes, log bodies and user-defined open values become
queryable data rather than opaque protobuf bytes or JSON strings. Fixed shapes
use typed Structs; open shapes use standard Parquet/Iceberg Variant values.
Rust, Python and TypeScript clients declare and write nested/open table data,
and Oracle exposes typed field access and `->`/`->>` queries.

Iceberg v3 and lineage-aware rewrite retain logical row identity. Per-file
shredding places useful typed leaves beside a residual Variant value, while
logical values, schema fingerprints and query results remain independent of
the physical layout. Recovery-stage runs retain their exact unshredded schema;
final hot objects and Forge outputs select their own layout.

## Lasting constraints and decisions

No silent field loss or inferred JSON-from-text is permitted. Unsupported
values are refused through stable errors. Tenant and sensitive-column
permissions apply to whole values and every extracted leaf. Processing and
inference remain bounded; missing or unsupported pruning evidence is
conservative. Client packages do not acquire server-owned analytical behavior.
The registered Iceberg schema owns physical table identity and supported-type
conformance. This was fresh-only development, not an existing-data migration.

Approved amendments established one per-file read core, stratified sampling,
output leaf projection, nested Struct/List paths and unnest, the revised
benchmark shape, native DataFusion ParquetSource reads, and DataFusion-owned
Variant functionality. Generic engine functionality belongs in the pinned
DataFusion/Arrow/distributed forks; Wyrd-specific admission, tenancy, storage
and lifecycle behavior stays in Wyrd. Opening upstream pull requests was not
part of this delivery. Later [main integration](bifrost-variant-main-integration.md)
decisions govern the combined product; current architecture supersedes older
packet wording and overlapping revision numbers.

## Acceptance evidence and limits

| Obligation family | Retained evidence |
|---|---|
| Typed/open data, SDK projection and refusal semantics | Task records and independent task reviews trace built-in schemas, logical Variant projection, stable errors and first-class SDK tests. Historical reviews also retained defects requiring remediation; closeout does not declare every historical finding independently closed. |
| Durability, v3 lineage and shredding equivalence | Recorded Bifrost/Forge/Scribe tests cover acknowledgement, recovery, publication and rewritten logical values; task-specific review evidence identifies the applicable boundaries. |
| Nested projection, filtering and distributed reads | Recorded TASK-003 evidence reports Bifrost tests, 53 Oracle journeys, fork verification and focused nested-field benchmarks passing; its historical review still requested corrections. |
| Verification and benchmark context | Records include passing format/lint/codegen/docs/boundary results and mixed review-time failures. Merged read-capacity reports were recorded at `5c775110f`; telemetry reports included a local fork fast-path patch, so they are not clean immutable integrated release proof. |

The owner authorizes administrative completion with these evidence limits.
No fresh full integrated verification, capacity threshold, final-review PASS,
live-cloud qualification or release result is asserted. Performance profiling
and remaining optimization goals are not promoted into shipped guarantees.

## Current authority, owners and tests

- [Bifrost design](../../../architecture/bifrost-design.md), [Wyrd design](../../../architecture/wyrd-design.md), and [security posture](../../../architecture/wyrd-security-posture.md).
- [Shared Variant values](../../../crates/shared/wyrd-types/src/variant.rs), [catalog](../../../crates/vala/vala-bifrost-redux/src/catalog/mod.rs), [Oracle](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs), [Scribe](../../../crates/vala/vala-bifrost-redux/src/scribe/mod.rs), and [Forge](../../../crates/vala/vala-bifrost-redux/src/forge/mod.rs).
- [Python Variant journeys](../../../sdks/wyrd-sdk-python/tests/integration/bifrost/test_variant_tables.py), [TypeScript Variant journeys](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/variant-tables.test.ts), and [Bifrost journey tree](../../../crates/wyrd/wyrd-testing/tests/bifrost).
