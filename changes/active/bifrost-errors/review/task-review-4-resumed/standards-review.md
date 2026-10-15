# Repository standards review — resumed round 4

## Subject and method

Original task base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; resumed integration
baseline `945a1548c47897dc94583169a6ba04163a73a749`, candidate
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`; DataFusion base
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. Distributed fork remains
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

This reviewer retains the same standards role after the owner explicitly
authorized continuation following the main merge. The original round-4 report
is preserved. This resumed pass inspected the cumulative production, fixture,
language-test, manifest, task-registration and pinned dependency diffs. The
latest remediation delta located the changed row-bound producer and extended
journey, but did not replace cumulative coverage. CodeGraph exploration of
`LiveScribeExec`, `QueryDispatcher`, fragment observations and
`caller_planning_fault` preceded source searches. Actual changed source,
surrounding owner contracts, public-client tests and committed exact-command
evidence were used; the implementation summary does not establish compliance.
No candidate source was edited. Only this report was written.

## Authority coverage

Read `AGENTS.md`, `architecture/agent-rules.md`, the reference router,
spec-driven-development and maintainer-style. Routed references: architecture
constraints and patterns; Rust core, implementation-execution,
testing-workflows, errors, Python API/stubs, TypeScript guide, agent harness;
DataFusion, OLAP serving, Arrow analytical interop and analytical operations
reliability. Governing architecture anchors are Wyrd doctrine; Wyrd design's
doctrine/client model/Bifrost/error catalog; Bifrost's immutable planning,
distributed execution, terminal/public contracts and failure invariants; and
security posture's object authorization, peer-context, tenant and audit rules.

The dependency fork follows native DataFusion conventions. Wyrd item-level
documentation and struct-shape rules are applied to Wyrd source, not used to
create churn in upstream trait implementations or macro-generated fork items.

| Changed surface | Applicable authority | Source assessment |
|---|---|---|
| Oracle logical/physical error conversion (`oracle/mod.rs`) | AGENTS §§3–6, 9–12, 16; errors; security object authorization; Bifrost terminal rules | **PASS.** Pure classification remains synchronous and uses existing derive-backed public errors. SQL caller diagnostics are distinguished from fixed safe physical-planning refusal; resource failures retain their general mapping. No parallel catalog or client error mechanism. |
| Live leaf and fragment reader (`oracle/live.rs`) | AGENTS §§3–6, 10–12, 16; DataFusion, OLAP, Arrow and reliability | **PASS for standards.** Existing `LiveScribeExec` owns route/predicate preparation and `LiveFragmentRead` owns polled IO. Snapshot/pruning remains pure; lazy stream owns admission bindings and cancellation. Method/field/test documentation explains partition ownership, readiness, re-execution, Errors and Panics. Unsupported pushdown preserves correctness owners. Semantic completeness remains the implementation/domain reviewers' question. |
| Session configuration (`resources.rs`) | AGENTS §§3–6, 16; immutable single-plan authority | **PASS.** Existing `OracleSessionShape::apply` configures two passes with rule-order rationale; no new planner, classifier, fallback, runtime or public option. |
| Workspace patches, lock and codec revision | AGENTS §§1–4, 11–12; dependency and peer codec boundaries | **PASS.** All eleven patch revisions, resolved DataFusion sources and `ORACLE_DATAFUSION_REVISION` agree on `b2d37bc182…`. Existing Arrow/distributed sources remain unchanged; no new feature, wildcard, profile or production/client dependency. |
| Dispatcher tally, server peer execution and Scribe observations | AGENTS §§3–6, 9–12, 16; server/test-support boundaries; patterns | **PASS.** Output tally extends its existing owner. Footer rows/latest plan storage remain behind existing `test-support`; production retains ordinary fragment counters. Peer listener/security/context are not replaced. New state/methods document purpose. |
| `query_fixture.rs`, `write.rs`, `peer_cluster.rs` | AGENTS §§3–6, 11–12, 16; observation identity; testing | **PASS.** Existing writer takes optional typed `RunId`, leaving managed stamping server-owned; callers and existing `write` pass the intended correlation. Existing peer cluster gains layout/output observations, with Errors documentation and no separate lifecycle owner. |
| Remote `live_subquery.rs` and module registration | AGENTS §§11–12, 16; real-server external-test justification | **PASS.** Existing Oracle target/managed environment is reused. New fixture owns a real cluster and two tables. Tests/driver/helpers/fields document intent and failure behavior. Latest refusal extension retains one key/client and all original assertions while adding FULL JOIN/grouping-set shapes. |
| Rust SDK query tests, corpus and mise selector | AGENTS §§2–3, 11–12, 16; client/runtime boundaries; testing | **PASS.** Public SDK → real server paths, explicit expected rows and small mixed-state fixtures; no generated workload or second DataFusion oracle. `query_coverage` is registered in owning integration binary and observe journey selector. Helpers and tests have substantive rustdoc and Panics where applicable. |
| Python query tests | AGENTS §§8, 11–12, 16; Python API and testing | **PASS.** Public `wyrd` imports, typed top-level tests, real server fixtures and stable error assertions. No PyO3 implementation, stubs or public export change; Python lifetime remains in Python tests. |
| TypeScript query tests | AGENTS §§2–3, 11–12; TypeScript and testing | **PASS.** Existing `serverTest`, facade, typed Zod/BigInt results and stable errors. No added native transport, server behavior or generated declarations. Node lifetime remains in TypeScript tests. |
| MCP query tests | AGENTS §§9, 11–12, 16; agent-harness/errors | **PASS.** Real mounted tool path and existing problem envelope; journey ignores are environment gates rather than disabled proof. Modified mapper journey and new subquery journey include Errors/Panics documentation. Runtime tool permissions/catalog are unchanged. |
| Fork scalar snapshot | Native `PhysicalExpr::snapshot`, Literal/field metadata contracts | **PASS.** Existing expression hook returns native metadata-preserving literal and native error when pending; tests cover reset, null and metadata. No Wyrd evaluator or codec extension. |
| Fork `single_value`, registration and scalar rewrite | Native AggregateUDF/Accumulator/optimizer/proto conventions | **PASS for standards.** Aggregate owns typed value/count state, normal partial/final merging and execution error. Rewrite uses existing registry/aggregate plan, with no custom node/protocol or Wyrd resource owner. SQL registration and user documentation agree. |
| Latest fork `LogicalPlan::max_rows` | Native logical-plan owner; guaranteed-bound contract | **PASS for standards.** Existing method now documents guarantee/unknown semantics; affected branches use None and checked arithmetic. Direct bound test and SQL cases live in existing expr/sqllogictest homes; no second proof API or Wyrd guard. Correctness of the closed operator match is independently reviewed by domain/invariant roles. |
| Fork proto/config/SQL/user docs | Native roundtrip and sqllogictest conventions | **PASS.** Existing proto helper tests decoded refusal; SQL/config expected output and aggregate docs project native sources. Latest remediation changes no codec/aggregate signature. |
| Active review packet and evidence | AGENTS §14; spec-driven-development | **PASS.** Normally tracked active packet, stable prior review identities, exact command table and explicit limits; no task IDs injected into permanent runtime code. |

## Rule results and prior closure

| Rule | Result and evidence |
|---|---|
| Narrow owner/dependency cost; no SDK durable logic | **PASS:** Vala runtime/server serving/native fork/test-only cluster remain separate; `wyrd-testing` DataFusion remains under `[dev-dependencies]` (manifest lines 133–137), not ordinary dependencies. |
| SQL capability and transaction ownership | **PASS / no new SQL authority:** cumulative changed production signatures/fields introduce no raw PgPool, naked connection/transaction, manual tenant predicate, commit or rollback. Existing authorized execution cut remains required before fragments open. |
| Typed identity and generated contract | **PASS:** optional RunId is typed; existing error catalog projects across surfaces. No Wyrd generated schema, stub or declaration hand-edit. |
| Async/ownership/clone boundaries | **PASS:** pure classification/snapshot/route selection is synchronous; stream/fixture async work awaits IO. Shared Arc state and route snapshots have ownership needs. No ad hoc runtime/new task, hot-path row clone or new resource root. |
| Rustdoc including prior FIND-4 | **PASS:** source contains meaningful Errors/Panics documentation for changed Wyrd operations, fixture helpers and tests; `oracle/mod.rs` mapper test and MCP query-error test retain the prior corrective Panics sections. FIND-4 stays closed. |
| Test integrity and canonical lanes | **PASS:** journey ignores are explicitly run by existing environment-owning lanes/selectors. No assertion weakening, sleep/retry/timeout adjustment, lint suppression or gate deletion. Latest Wyrd refusal journey only adds queries; key-collision correction from preceding round retains a single legitimate client. |
| Latest lint failure recovery | **PASS:** evidence distinguishes earlier fork commit's hidden Clippy failure from final `b2d37bc182…`, records clean changed-crate Clippy, and Wyrd pins the corrected commit; no suppression or policy weakening. |
| Whitespace | **PASS:** independently ran resumed cumulative `git diff --check 945a1548c… 17697dc8c…`, exit 0. |

Non-blocking style deviations remain: function-local imports inside the live
scalar unit fixture, and fully-qualified fixture types in the coverage writer
and peer layout helper. These conflict with agent-rule import spelling/location
preferences but have no demonstrated behavioral, contract or resource
consequence, so cannot alone produce FIX_REQUIRED under this skill. No new
material standards finding is proposed.

## Verification limits and overall result

Committed R3 evidence records exact bound test, fork SQL/expr/optimizer/aggregate/
proto suites, changed-crate Clippy and format; Wyrd exact units/codec/three peer
journeys/corpus and fmt/lints/codegen/dependency/tenant checks. Current source
names, selectors, target wiring and pin identities correspond to the table.
These are recorded runs, not independent reruns in this discovery pass.
No Cargo/environment/codegen lane, benchmark or host load was run here. Earlier
language journeys remain applicable source evidence but final integrated
verification must use the final fork pin; bounded remediation review does not
prematurely require the full capability gate. The prior untraced reservation
observation remains a recorded limit, not a diagnosed new failure.

**PASS.** Material repository-standard ledger is empty. This is the standards
result; task acceptance, SQL correctness and final finding validation remain
the independently assigned reviewers' responsibility.

## Main-merge seam revalidation

The task-attributable baseline-to-current delta retains the same 58 files and
6,385 additions/203 deletions as the original task cumulative range. Direct
pre-merge-to-current diff inspection confirmed no changes to task Vala
Oracle/resources files, peer fragment service, Bifrost query/writer fixtures,
peer-network journeys, Rust query/corpus, Python/TypeScript query tests, MCP
query tests or root DataFusion patches. The fork is unchanged. The complete
coverage above remains current-source evidence, rather than acceptance inferred
from an interrupted PASS.

| Merged-main seam | Integration evidence and rule result |
|---|---|
| Security posture | Authority delta extends existing sealing-keyring protection to gateway managed secrets with tenant/name/provider binding. Bifrost object authorization, peer and terminal authority is unchanged. **PASS.** |
| Shared AppState/Scribe | AppState removes a gateway-specific secret handle while retaining gateway engine and auth sealing owner. Task Scribe footer-row/plan fields, initialization and recording methods survive unchanged behind test-support; peer fragment calls remain intact. **PASS.** |
| Real test-server boot | Gateway resolver now reuses auth sealing key; removed gateway-mounted-key lifetime does not own Bifrost state. Builder still constructs Bifrost runtime and supplies it to AppState/real serving; query fixtures and PeerCluster retain their existing owners. **PASS.** |
| MCP discovery | Bifrost descriptors remain first and dispatch retains authenticated context. Main moves gateway writes into per-caller expansion and adds private/zero-TTL catalog metadata. Discovery/connectivity expected order retains Bifrost tools; task query/error journeys are unchanged. **PASS** for task integration, without independently accepting unrelated gateway features. |
| Rust integration module | Main adds gateway_jev; task query_coverage and query_bifrost registration remains intact. **PASS.** |
| Lock and mise graph | Only extra pre-merge lock dependency is CLI rand; DataFusion lock/patch/codec identities remain b2d37bc182. Main adds framework/gateway lanes, while observe still selects query_coverage and Oracle peer exact target is unchanged. No task proof is suppressed or replaced. **PASS.** |

Independently ran resumed cumulative whitespace check, exit 0, and verified
HEAD `17697dc8c2740213565e5f913cde1edfb0cb8ce5` after inspection. No benchmarks
or Cargo/environment lanes ran. Existing implementation evidence precedes the
merge: its source applicability remains, but this report does not claim a new
execution result for the merged server. Later integrated change review retains
that verification cost.

Resumed overall result: **PASS**, no material repository-standard findings.
