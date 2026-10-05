# Forge integration with distributed Oracle

- Change: `bifrost-forge-oracle-integration`
- Specification: `SPEC-bifrost-forge-oracle-integration`, approved revision 5
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: locked Forge candidate `b35ebb5e76b7a63fc7e9549329a5c90101400002`

The completed Forge compaction implementation is integrated with the distributed Oracle destination while preserving the destination Gate, Scribe, Oracle, public-query, server, SDK, harness, and lane semantics. The resulting flow uses one coherent write, maintenance, and read architecture with no duplicate planner, harness, resource owner, lifecycle, or compatibility adapter.

Forge retains promotion and managed rewrite behavior, scheduling, leases and fences, uncertain-operation reconciliation, reader-safe retention and cleanup, durable SQL state, bounded resource ownership, readiness behavior, telemetry, and production journeys. Oracle reader authority protects exact table snapshots during source IO and composes with the destination's single-planner and query-lifecycle rules.

Migration history remained forward-only, generated contracts came from their owners, and the pinned compaction core stays in the single DataFusion/Arrow/Parquet/Iceberg dependency universe. The source-only Oracle first-batch test gate was deliberately dropped because it was not required by Forge and conflicted with destination Oracle ownership.

Acceptance closed with Forge 13/13, Scribe 20/20, Oracle 23/23, Gate unit coverage, generated-contract verification, linting, whitespace checks, and the object-store dependency pin check.

Current owners and evidence:

- [Bifrost design](../../../architecture/bifrost-design.md).
- [Forge implementation](../../../crates/vala/vala-bifrost-redux/src/forge), [Oracle implementation](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs), and [reader authority](../../../crates/vala/vala-sql/src/queries/oracle_reader_authority.rs).
- [Forge journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/forge).

