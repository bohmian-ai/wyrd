# Pod-local Oracle admission

- Change: `oracle-local-admission`
- Specification: `SPEC-oracle-local-admission`, approved revision 2
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Oracle admission is now pod-local. Each process owns one bounded tenant scheduler, concurrency budget, physical-slot partition, scratch boundary, and aggregate DataFusion memory root. Admission no longer requires PostgreSQL coordination, distributed leases, allocation renewals, or cluster-wide capacity agreement.

Interactive and analytical queries share the hard aggregate memory boundary while retaining their class-specific slot guarantees. Tenant fairness remains local and bounded, analytical fan-out cannot reserve every live Oracle by default, and ownership spans queueing through terminal cleanup. Queue deadlines include waiting time and expired grants cannot begin execution.

Operators receive local saturation telemetry that distinguishes queueing, admission refusal, memory, slots, scratch, and query class without high-cardinality labels. Removing cluster admission did not weaken authentication, object authorization, audit, cancellation, or distributed execution authority.

Acceptance closed through PostgreSQL-free admission tests, aggregate memory qualification, multi-tenant fairness and overload cases, bounded analytical participant journeys, deadline tests, telemetry checks, and removal of orphaned OLAP coordination machinery. Revision 2 clarified the one-root memory and local scheduling contract; no material deviation remains.

Current owners and evidence:

- [Bifrost design](../../../architecture/bifrost-design.md).
- [Oracle admission](../../../crates/vala/vala-bifrost-redux/src/oracle/admission.rs), [Oracle resources](../../../crates/vala/vala-bifrost-redux/src/resources.rs), and [Oracle runtime](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs).
- [Capacity journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs).
