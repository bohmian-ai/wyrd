# TASK-002 Bifrost caller errors and live-table subqueries

Spec: `../spec.md` REQ-002, REQ-006, INV-001, AC-001..AC-003.

1. Caller-fault planning failures (unknown column/table/function, type
   mismatch, ambiguous reference) return a 4xx WyrdError carrying the
   planner's message and suggestion, the same for single-table queries,
   joins, and subqueries, over HTTP, SDK, and MCP `bifrost.query`. Today an
   unknown column returns a generic 400 on one table and a generic 500 inside
   a three-way join. Internal failures stay 5xx and opaque. Messages never
   name tables or columns the caller cannot see.
2. Scalar, IN, and EXISTS subqueries over live (uncompacted) tables succeed.
   Today a scalar subquery over a live table fails with
   WYRD_VALA_500_QUERY_EXECUTION_FAILED ("live Scribe fragment open failed:
   peer security or fragment contract rejected"); joins and self-joins work.
   Fix the root cause in the owning path, not per query shape.

Read `architecture/bifrost-design.md` first. Evidence: Bifrost journeys
(Rust, Python, TypeScript, and MCP) reproducing both failures on live data,
negative case for an unauthorized table, and the owning `test:bifrost` lanes.
