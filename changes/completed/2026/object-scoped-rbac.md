# Object-scoped RBAC

- Change: `object-scoped-rbac`
- Specification: `SPEC-object-scoped-rbac`, approved revision 4
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Wyrd permissions now carry resource, action, and a required typed object scope. `All` covers every object for the same operation, while Bifrost schema scopes cover current and future tables under one catalog/schema and table scopes follow one stable server-managed table UID. Invalid identities, missing scope, and invalid resource or action combinations fail closed through the single persisted permission contract.

Oracle authorizes the complete resolved table set before protected source materialization and denies the entire query when any table is uncovered. HTTP, gRPC, SDK, MCP, interactive, analytical, and distributed paths share that decision. The approved permission and exact authorized table identities enter the signed permission digest so a worker cannot widen coordinator authority.

Authorization denials use the stable `WYRD_VALA_403_QUERY_FORBIDDEN` error, return no rows, and pass through the canonical audit boundary. Sensitive-column metadata remains descriptive; table query permission governs the row. Static grants remain role-derived RBAC with no Policy lookup, grant store, cache, or second checker.

Acceptance closed through serialization and validation tests, three-axis permission coverage, role resolution, pre-source-IO authorization tests, digest tamper checks, and the real-server `analyst`/`data_scientist` matrix covering accepted, rejected, and mixed-table queries. Revision 4 removed unused column-level payload authorization; no material deviation remains.

Current owners and evidence:

- [Security posture](../../../architecture/wyrd-security-posture.md) and [Bifrost design](../../../architecture/bifrost-design.md).
- [Scope contracts](../../../crates/wyrd-spec/src/auth/permission_scope.rs), [runtime permissions](../../../crates/shared/wyrd-runtime/src/permission.rs), and [Oracle enforcement](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs).
- [Scoped-role server journey](../../../crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs).

