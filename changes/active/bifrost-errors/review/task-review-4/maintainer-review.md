# Independent maintainer review — round 4

## Subject and result

**PASS.** Wyrd `526355fb8c58dae6d67d516f98dddcd5c02b0e79` → `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`; pinned DataFusion `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` → `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. Distributed fork remains unchanged. No material maintainer finding.

This is a fresh review of the cumulative source, with the last remediation used to locate changed owners. No implementation summary or other discovery report establishes this conclusion. The approved spec, owner directions, prior remediation tasks, AGENTS, agent-rules, spec-driven-development, maintainer-style and applicable Rust/error/DataFusion/OLAP references govern the review. CodeGraph was used before local symbol navigation.

## Changed-surface coverage

| Surface | Source and caller inspection | Maintainer assessment |
|---|---|---|
| Native row-bound owner | Complete `LogicalPlan::max_rows` match in fork `datafusion/expr/src/logical_plan/plan.rs:1441–1539`; direct bound test at 6250 onward; shared `ScalarSubqueryToJoin::build_join` and scalar invariant consumer | Guarantee is explicit in rustdoc. `None` is the existing native unknown state. Grouping-set expansion and nonempty FULL JOIN lose proof conservatively; multiplication and union accumulation use checked arithmetic. No second proof classifier or query-shape exception is introduced. |
| Native scalar cardinality | `functions-aggregate/src/single_value.rs` complete accumulator/UDAF/test bodies; lib registry; Filter and Projection rewrite call sites and `build_join` wrapping branch | One cohesive accumulator owns value and row count. It uses ordinary aggregate state/merge and default registry, not a new relational node or remote protocol. Existing `max_rows` chooses when validation is necessary. Both rewrite consumers share that source. |
| Native scalar snapshots and codec | `physical-expr/src/scalar_subquery.rs::snapshot` and existing evaluate/proto hooks; snapshot regressions; `proto/tests/cases/plans/scalar_subquery.rs` round-trip refusal; SQL/config/aggregate docs | Snapshot extends the installed expression mechanism and preserves field metadata. Native codec test proves registered aggregate identity/state survives encoding without Wyrd evaluation machinery. Documentation distinguishes supported flag-off positions and runtime refusal. |
| Oracle live boundary | `oracle/live.rs` retained predicate, `resolved`, Pre/Post pushdown, execute, `LiveFragmentRead::into_stream`, fragment construction and unit regression; physical-rule/caller navigation through CodeGraph | Runtime state remains on the original leaf; per-execution static snapshot belongs to the remote-fragment owner. The lazy stream reads it once per executed partition. Route assignment precedes runtime narrowing and the retained authoritative filter is not falsely declared exact. Comments explain the ordering reason. |
| Session and public error mapping | `resources.rs::OracleSessionShape` configuration; Oracle logical-planning callers, physical mapper, shared typed wrapper traversal and mapper unit body | Two passes remain DataFusion configuration rather than a parallel planner. Rule-order rationale explains the change. Separate logical and physical diagnostic exposure is intentional; the narrowly owned helper avoids general cause unwrapping. |
| Scribe proof observation | `NativeOutputTally::rows`; peer executor/footer callers; `Scribe` fields, record/read methods; `PeerCluster` output/layout helpers | Test observations extend existing production-shaped counters and fragment owner. Plan retention is feature-gated. Existing footer output supplies exact rows; no synthetic timing harness or production dependency is added. |
| Shared small fixture and writer | `query_fixture.rs` seed flow, `BifrostWriter::enqueue/write/flush`, complete new peer `LiveTables` fixture and public-client flows | Managed `RunId` correlation is passed through the real write owner. Helpers remain near their callers; retained cluster/tables/instants have a concrete owner. New negative journey reuses one provisioned key/client across four queries, avoiding duplicate credential setup. |
| User-surface evidence | Rust `query_bifrost` and `query_coverage`, peer `live_subquery`; Python fixture/parametrized queries; TypeScript fixture/public client tests; MCP query tests; Rust integration module and mise registrations | Handwritten expected rows cover common SQL with small fixed mixed-state fixtures. Language tests drive public clients; no interpreter lifetime is emulated in Rust. Common corpus is not repeated in each SDK. Documentation/panic sections describe actual assertions, including the two previously missing sections. |
| Dependency and delivery artifacts | Workspace fork pins, Cargo.lock, codec revision; spec and remediation evidence | One native universe is advanced together. No new DataFusion-backed public oracle/framework remains in `wyrd-testing`. Recorded review packets are task evidence, not permanent implementation comments. |

## Existing mechanisms and minimum sufficient extension

The candidate reuses `LogicalPlan::max_rows` rather than creating a scalar-specific row classifier. Its two consumers already treat an unknown bound conservatively. Consequently the latest fix requires no `single_value`, rewrite, codec or Wyrd runtime change. This is the appropriate maintenance boundary: future operators have one exhaustive owner to update, and `Some(n)` has an explicit proof contract.

The cumulative aggregate addition fills a real gap: the flag-off rewrite formerly joined a multi-row scalar as a set. An ordinary native accumulator gives partial/final execution the row-count semantics; retaining the default aggregate registry and codec is smaller and easier to qualify than a new assertion node/protocol. It does not duplicate Wyrd admission, tenancy, stage scheduling, transport or resource lifecycle.

The former general support workload is absent. The current `Case` record, fixed fixtures and explicit expected JSON in `query_coverage` are an ordinary parameterized journey, not a reusable SQL-oracle abstraction. Remote output evidence uses the existing footer producer and test-support observation surface.

## Prior closure from source

- FIND-1: `LiveScribeExec` accepts eligible late dynamic filters, retains the expression, snapshots on first poll, and encodes the resolved predicate; remote literal/footer proof inspects actual Scribe output rather than timing.
- FIND-2: the large generator/DataFusion oracle/public workload surface is removed; common query types retain direct, fixed-answer coverage, including a CTE joined from an outer table.
- FIND-3: full joins and grouping sets no longer provide the false singleton bound, so the shared rewrite automatically retains existing `single_value` validation; SQL cases cover Filter and Projection and peer journey covers reachable registered outer-table use.
- FIND-4: missing `# Panics` sections in changed Wyrd tests are present.

These are maintainer observations, not a substitute for the independent behavior/domain acceptance reviews.

## Non-blocking calibration note

The comment above the FULL JOIN assertions in `max_rows_is_a_sound_upper_bound` says “Two unmatched singleton inputs,” but both helpers use value 1 with equality, so that specific fixture matches. The assertion still correctly pins conservative `None` for nonempty FULL JOIN. Actual unmatched 1/2 inputs are separately present in `subquery.slt` and the public peer journey. This is a local comment precision issue, with no lost required behavior or material maintenance cost; it does not warrant another remediation round.

## Verification limits

Static source/caller/test review only; no benchmarks, synthetic load, heavy lane reruns or candidate mutations. Committed focused-test evidence was checked against actual names, assertions and ownership. The later integrated change review owns full capability verification. Conservative bounds intentionally incur runtime validation on uncertain FULL JOIN/grouping-set scalar inputs; that is documented and preserves correctness.

## Material findings

None.
