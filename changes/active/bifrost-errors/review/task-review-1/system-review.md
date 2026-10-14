# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Authority: `changes/active/bifrost-errors/spec.md`, the human requirement that live-subquery support be performant and principled, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, and `architecture/bifrost-design.md`
- Production changes: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs` and `crates/vala/vala-bifrost-redux/src/oracle/live.rs`

The candidate remained at the stated commit while this review inspected it.

## Deployed-path evidence

The public query enters Oracle, resolves and authorizes the complete table set, constructs the optimized logical plan through `Oracle::plan_logical`, freezes the query attempt, and builds one physical root through `Oracle::build_physical_root` (`oracle/mod.rs:1948-1975`, `2697-2814`, `3000-3035`). The error change is confined to this pre-admission logical-planning boundary: typed SQL/planning/schema/unsupported failures are now unwrapped and returned with the planner detail, while resource and unclassified failures continue through the stable general mapper (`oracle/mod.rs:3740-3805`). It adds no process, network, retry, or retained state.

For live data, `OracleTableProvider::scan` composes published, hot, and live sources; each live source is a `LiveScribeExec` (`oracle/exec.rs:1665-1720`). The leader's physical filter-pushdown pass offers predicates to that leaf. The candidate detects `ScalarSubqueryExpr` and omits any containing predicate from the fragment predicate (`oracle/live.rs:382-422`, `509-523`). The parent `FilterExec` remains on the leader because the leaf reports every offered filter unsupported. During execution, `ScalarSubqueryExec` first populates its shared result container and only then executes the outer input (pinned DataFusion source `datafusion/physical-plan/src/scalar_subquery.rs:214-237`); the leader therefore has the scalar value before `LiveScribeExec::execute` opens Scribe fragments. The candidate nevertheless opens those fragments without that resolved predicate.

`LiveScribeExec::execute` partitions the selected routes and opens one signed fragment per Scribe (`oracle/live.rs:462-505`, `615-704`). The Scribe follower decodes the placeholder and pushed filter, substitutes its staged scan, runs DataFusion filter pushdown, and streams framed batches and terminal evidence back (`oracle/follower.rs:819-856`). The leader validates frames and applies the retained filter. Rust, Python, TypeScript, HTTP, and MCP all converge on this same server path; the changed SDK/MCP files are journey consumers rather than alternate implementations.

Analytical roots retain the existing distribution topology. `Analytical::planning_session` installs `LiveUnionBoundary` and the pinned distributed planner (`oracle/analytical.rs:6645-6678`); the candidate does not alter stage placement, remote serialization, participant cuts, or reservation ownership.

## Failure and recovery assessment

- Planning refusal: it happens before admission, peer dispatch, or retained graph ownership. Returning the new mapped error cannot orphan a slot, stream, or graph. Internal/resource errors retain their existing closed mapping.
- Cancellation and deadline: live reads still select on the attempt cancellation token and absolute deadline; drop still closes the fragment stream and releases the Scribe-side snapshot/lease (`oracle/live.rs:462-468`, `647-704`). No changed state survives restart.
- Scribe loss: open-time availability loss before any row remains degradable; loss after a row remains terminal to avoid partial, silently inconsistent results (`oracle/live.rs:615-704`). The candidate does not weaken that integrity boundary.
- Rolling replacement/fence change: route and fragment authority remain bound to the frozen Scribe node and writer epoch. The changed predicate decision does not widen the cut or bypass peer authorization.
- Process restart: all changed runtime state is query-local physical-plan state. Existing cancellation/drop and lease expiry remain the recovery mechanisms; no durable state or replay contract changed.

## Affected capabilities

- Bifrost SQL over live rows through HTTP, Rust/Python/TypeScript SDKs, and MCP.
- Oracle-to-Scribe private fragment execution for scalar-subquery outer scans.
- Planner error status/detail projection on every public query surface.
- Query scan volume, live-fragment duration, and therefore exposure to a terminal mid-stream Scribe loss for scalar-subquery predicates.

## Material finding

### SYS-001 — Scalar-subquery support disables remote filtering after the scalar value is available

- **Violated obligation:** the human-approved review requirement that live subquery support be performant and principled, without a query-path performance regression.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/live.rs:401-422`, especially the new exclusion at line 406 and `reads_scalar_subquery` at lines 509-523.
- **Evidence:** the candidate removes the complete scalar-dependent predicate from the predicate stored on `LiveScribeExec`. The leaf later serializes only `self.predicate` into the Scribe fragment (`oracle/live.rs:286-345`), so the remote staged scan cannot filter on the scalar result. DataFusion's surrounding `ScalarSubqueryExec` completes the scalar subquery before invoking the outer input, which means the scalar value is already available when `LiveScribeExec::execute` and fragment creation run. The candidate's journeys use three rows or correctness-oriented support fixtures and assert returned rows/fragment presence, but none asserts Scribe rows/bytes, route pruning, or an equivalent-join scan bound (`sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:246-317`; `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:69-104`; `sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs:108-132`).
- **Observable system consequence:** `WHERE id = (SELECT max(id) ...)` scans and transfers every outer live row from Scribe to Oracle and filters only on the leader. Cost grows with the complete live tail rather than the qualifying rows. Longer, larger fragment streams also enlarge the interval in which a Scribe loss occurs after output begins and therefore must fail the entire query under the existing integrity rule.
- **Required testable correction:** keep the scalar-dependent predicate beside its `ScalarSubqueryExec` during planning, then, at the live remote-execution boundary after the scalar result has completed, bind the `ScalarSubqueryExpr` to its resolved literal and use that static predicate for route selection and the serialized Scribe fragment. The decision about what a Scribe fragment may carry remains owned by `LiveScribeExec`; if resolving a populated physical scalar-subquery expression is generally useful, put that reusable expression operation in the owned DataFusion fork rather than adding a second SQL/subquery planner in Wyrd. Prove closure with a remote-Scribe scalar-subquery test whose large outer live set has few matches and which asserts bounded remote rows or bytes (and, for a time-axis scalar, route pruning), alongside the existing row-equivalence assertion.

## Verification and proof limits

No command results were supplied with the immutable subject, and this review did not execute the ignored Postgres/peer journey lanes. Source inspection found broad happy-path correctness coverage for published, live, mixed, and remote-live data and all public client surfaces. It found no candidate proof of the performance boundary in SYS-001 and no new fault-injection coverage; existing cancellation, deadline, fencing, frame-validation, and source-loss machinery is unchanged.

## Overall result

**FAIL** — the error-mapping change has no adverse deployment or recovery effect found in this pass, and the live-subquery change preserves failure integrity, but SYS-001 leaves scalar subqueries with unbounded avoidable live-tail transfer and no performance proof.
