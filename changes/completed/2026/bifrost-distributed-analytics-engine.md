# Bifrost distributed analytical queries v1

- Change: `bifrost-distributed-analytics-engine`
- Specification: `SPEC-bifrost-distributed-analytics-engine`, approved revision 6
- Completed: 2026-09-05
- Reviewed base: `f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df`
- Reviewed target: `99c90b13be173809e95e11b16bbba52fb9f69a32`
- Delivery reference: not supplied

Wyrd now serves operational reads, exploratory analytics, and internal scheduled
queries through one authenticated raw-SQL operation. Oracle builds one physical
plan: an ordinary root selects Interactive execution and a `DistributedExec`
root selects Analytical execution. Planning and execution failures are terminal;
there is no second build, caller-selected path, or automatic fallback.

## Delivered behavior

Distributed execution supports representative remote scans, filtering,
projection, grouped aggregation, joins, ordering, exchange, and spill. The same
Oracle process can coordinate or follow work. Private peer services use the
existing authenticated TLS plane and signed, fenced authority.

Rust clients consume incremental Arrow results and validate the terminal and
clean stream end. The server exposes `/mcp` on its existing public listener,
using the official Rust MCP implementation and the supported `2026-07-28`
discovery lifecycle. Its three tools are `bifrost.list_tables`,
`bifrost.describe_table`, and `bifrost.query`. Query results contain ordered
column descriptors, positional rows, and a validated terminal; exceeding a
result ceiling returns an error without successful truncation.

Verified delegation accompanies the effective principal through HTTP, MCP,
gRPC, and signed Oracle forwarding. Existing audit details retain the ordered
delegator identities and Card authority. Authorization continues to evaluate
the effective principal. Delegation survives WAL acceptance and relay; empty
chains remain omitted from historical audit encodings.

## Lasting constraints and decisions

One graph lease owns each exact participant reservation and its descendants.
Activation validates authority before consuming work, reuses valid duplicates,
and rolls back failed activation. Query memory, scratch, slots, fan-out, and
Wyrd-owned queues remain finite. Interactive capacity retains its protected
floor. One original deadline governs preparation and execution; short ticket
acceptance lifetimes do not replace the query deadline.

Cancellation and failure join cleanup before releasing ownership. Cleanup
failure retains observable debt rather than reporting successful release.
Successful terminals require settled ownership. Read acceptance is fsynced in
the existing local WAL before rows and relayed into the canonical tenant audit
outbox, without adding per-stage read events.

The dependency boundary remains one pinned DataFusion 55 / Arrow and Parquet
59.2 universe and the existing pinned distributed executor. No dependency fork,
second scheduler, shuffle service, or separate analytical process role was
introduced. Rust and MCP are this change's shipping projections; Python and
TypeScript projections were explicitly deferred.

Approved revisions 1–2 chose practical aggregate containment and Wyrd-owned
queue guarantees; revision 3 consolidated production closeout; revision 4
narrowed shipping clients to Rust and MCP; revision 5 selected server-hosted
MCP; revision 6 selected the single physical-planning pipeline. These decisions
supersede the earlier heuristic, allowlist, second-build, and fallback designs.

## Requirement and evidence closure

| Obligations | Closure evidence |
| --- | --- |
| REQ-001–003; INV-001/007/008; AC-002/003 | Retained physical-root routing, real process scan/join/aggregation/exchange/spill evidence, and public and scheduled query journeys. |
| REQ-004–007; INV-002/004–006; AC-001/004/005 | Exact graph activation and rollback, finite resource ownership, Interactive contention, original-deadline tests, private topology, cancellation, peer-loss, and cleanup evidence. |
| REQ-008; INV-003; AC-006 | Rust and server terminal/EOF checks, joined settlement, read-audit acceptance, replay, and canonical hash coverage. |
| REQ-009–010/012; INV-009; AC-009 | Production authenticated MCP discovery and invocation, closed inputs, bounded positional results, real delegated allow/deny audits, and signed forwarding attribution. |
| REQ-011; AC-007 | Production path, admission, follower, exchange, spill, cancellation, and cleanup telemetry with resource baselines in process journeys. |
| AC-008 | Reused Scribe 19/19 and Rust SDK 9/9 journeys, plus final MCP 7/7, Oracle 23/23, and server 4/4 journeys; integrated source review found no remaining blocking regression. |

Final remediation evidence includes mutation-sensitive delegation journeys,
the isolated Oracle audit module passing 7/7, formatting, workspace lint, and
generated-contract checks. Final review independently reran the canonical
delegation encoding test successfully and checked the complete range for
whitespace errors. Earlier physical, resource, and Scribe evidence was reused;
a full repository aggregate was not rerun for final review.

Broader checks are not represented as green: the SQL documentation test failed
identically without the remediation, and the broader Wyrd family showed shared
Postgres failures both with and without it. Previously reported tenant-isolation
check findings affect SQL sources unchanged by the reviewed change. These
baseline limitations do not establish a new regression in this implementation.

## Current authorities and implementation

- [Wyrd design](../../../architecture/wyrd-design.md),
  [Bifrost design](../../../architecture/bifrost-design.md), and
  [security posture](../../../architecture/wyrd-security-posture.md).
- [Query and MCP usage](../../../docs/src/content/docs/bifrost/reading-data.svx).
- [Oracle execution](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs)
  and [graph ownership](../../../crates/vala/vala-bifrost-redux/src/oracle/analytical.rs).
- [Rust query client](../../../crates/vala/vala-sdk/src/query.rs) and
  [MCP server](../../../crates/wyrd/wyrd-server/src/mcp/mod.rs).
- [Audit contracts](../../../crates/wyrd-spec/src/vala/audit_detail.rs) and
  [WAL audit publisher](../../../crates/wyrd/wyrd-server/src/oracle/query_audit.rs).
- [Oracle journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/oracle),
  [Scribe journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/scribe),
  [server journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs),
  and [MCP query journeys](../../../crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs).
