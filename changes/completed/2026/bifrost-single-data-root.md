# Automatic Bifrost local storage from one filesystem root

- Change: `bifrost-single-data-root`
- Specification: `SPEC-bifrost-single-data-root`, approved revision 3
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Every Bifrost server target now derives its owned local paths from one durable filesystem root. `WYRD_BIFROST_DATA_DIR` is the sole environment override and `.wyrd/bifrost` is the local default. Server boot creates and validates the Scribe WAL, stable node identity, staging, output scratch, Oracle spill, and Forge spill directories beneath that root.

The independent Scribe WAL configuration surface was removed. Local development, tests, and deployment use the same path model, allowing one mounted volume to preserve all Bifrost local state needed across restart. Managed subpaths remain server-owned and callers do not select subsystem internals.

The lasting constraints are one root per server target, automatic and fail-fast directory preparation, preserved local durability, and no Kubernetes-specific storage contract in core code. Acceptance closed through root-resolution tests, automatic-path boot checks, restart durability journeys, deployment configuration checks, and focused Bifrost verification. No material deviation remains.

Current owners and evidence:

- [Bifrost design](../../../architecture/bifrost-design.md).
- [Data-root owner](../../../crates/wyrd/wyrd-server/src/boot/data_root.rs), [server boot](../../../crates/wyrd/wyrd-server/src/boot/mod.rs), and [server configuration](../../../crates/wyrd/wyrd-server/src/config.rs).
- [Bifrost server and Scribe journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost).
