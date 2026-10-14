# Independent structured Ponytail validation

## Immutable subject and inputs

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`7d6a1ff95b3dc09315e3af236ab9906b305e11d0`. HEAD still equals the candidate.
Pinned DataFusion is `5697cfa35192f175bbcd54f59a5fd88527c2bbf5` and the
distributed fork is `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Reviewed the subject/navigation map, approved spec, original absence of a task,
prior verdict and validated ledger, R1 task and evidence, cumulative diff,
both implementation reports, standards, maintainer, system, DataFusion and
tenancy reports, claim comparison and focused follow-up. Applicable authority
is AGENTS, agent-rules, spec-driven development, maintainer style and the
governing Bifrost/DataFusion/Rust boundaries. CodeGraph preceded Wyrd source
navigation. Reviewed actual producers and consumers rather than accepting
reviewer agreement. No candidate source, dependencies or previous reports were
edited; no benchmarks or executable test lanes were run.

## Dispositions

| Proposal | Disposition | Source-backed reason |
|---|---|---|
| DF-R2-1: analytical scalar inequalities lack runtime range pushdown | REJECTED as a blocking finding | The execution limitation is real, but native NLJ does not produce the missing runtime predicate and the bounded task does not require creating every join optimization. Existing analytic range behavior is not shown to regress. R1's concrete dropped-predicate correction and selective equality proof are closed. |
| DF-R2-2: multiple-row scalar input silently uses set-join semantics | CONFIRMED, recommendation REVISED; FIND-bifrost-errors-3 | REQ-006 accepts scalar behavior without restricting it to provably singleton aggregates. The actual scalar owner rejects a second row globally; the analytical rewrite bypasses that owner. A successful join is not equivalent SQL scalar behavior. Correct the relational scalar producer, not Wyrd's leaf or the join output. |
| STD-R2-1: missing Panics sections in two changed tests | CONFIRMED; FIND-bifrost-errors-4 | Both items have substantive behavior docs but omit a required panic condition. AGENTS §16 explicitly requires Panics documentation whenever panics remain possible; Rust-core says incomplete rustdoc is a hard blocker. This is batched with the substantive correction, not a separate style round. |

The behavior/invariant reports' classification of cardinality as out-of-task
debt is rejected. The flag predates this candidate, but the defect is in the
very scalar capability being accepted and newly qualified. This is not a
claim that remediation introduced the error. By contrast, requiring a new
range-filter optimization would enlarge the bounded execution correction.

## Prior finding closure

### FIND-bifrost-errors-1 — CLOSED for its diagnosed defect

The concrete scalar exclusion is absent. DataFusion's scalar expression now
implements its established recursive snapshot contract, using built-in
Literal and FieldMetadata, with pending-state refusal. Wyrd retains the
runtime expression and snapshots an execution-local leaf at first poll
(`live.rs:298-317,441-476,533-557,668-680`). Static conjuncts still prune
planning routes; assigning stable planned indices before runtime narrowing
preserves disjoint partition ownership, even when separate streams snapshot
at different times. R1's private opposite ordering is not stronger authority
than that invariant.

On analytical equality, `resources.rs:4122` supplies the second logical pass;
native equijoin extraction can consume the equality created by the earlier
scalar rewrite. DataFusion `hash_join/exec.rs:1954-2017` creates a Post filter
and registers its producer only when the probe subtree retains a consumer.
`hash_join/stream.rs:670-779` waits for build collection and, when present,
partition bounds before polling the probe. Wyrd's deferred snapshot therefore
observes the build result on this path, while the join stays authoritative.
The two-fragment/two-footer-row journey at
`peer_network/live_subquery.rs:110-154` checks remote output and both time
literals; the physical-scalar unit checks pending/reset/partition behavior.
The codec revision and Cargo pins agree. These prove the diagnosed selective
transfer property, not universal scalar-range pruning or one-row physical IO.

### FIND-bifrost-errors-2 — CLOSED

The generated support-workload/oracle/public framework is absent, and
`wyrd-testing` has no promoted ordinary DataFusion dependency. The compact
`query_coverage.rs` corpus uses fixed small mixed published/live fixtures and
explicit answers, including the owner-requested CTE joined to another table.
The separate remote join journey remains narrow. Those human-directed
coverage additions are authority, not retroactive reviewer FIND IDs.

## Validated ledger

### FIND-bifrost-errors-3

- Discovery IDs: DF-R2-2; focused follow-up confirms the same cause.
- Status: CONFIRMED; correction REVISED to one native relational producer
  boundary with explicit global distribution and serialization requirements.
- Classification: INCORRECT.
- Obligation: REQ-006 scalar-subquery semantics; owner expectation that common
  query support works correctly. A scalar that is executed must return zero
  or one row, and refuse multiple rows rather than produce successful set
  results.
- Locations: Wyrd `oracle/analytical.rs:6645-6678` selects distributed session
  configuration; distributed `session_state_builder_ext.rs:25-29` disables
  physical scalars; pinned DataFusion
  `optimizer/src/scalar_subquery_to_join.rs:108-238,339-430`, particularly the
  unrestricted join at 389-391. These are producer/selection locations, not
  separate findings against each consumer.
- Evidence: Read the full optimizer `rewrite`, extraction rewriter and
  `build_join`, and the complete native scalar execution/extraction bodies.
  Filter and Projection both call `build_join`; uncorrelated inputs become
  aliased relations with an ON-true left join and no runtime cardinality
  validation. Correlated cases retain their existing analyzer aggregation
  checks (`expr/src/logical_plan/invariants.rs:156-207`). The ordinary
  `ScalarSubqueryExec::execute` waits for extraction before opening the main
  input; `execute_scalar_subquery` (`physical-plan/src/scalar_subquery.rs:397-436`)
  checks every nonempty batch, refuses a second row (including duplicate
  values), and emits typed NULL on empty input. `execute_stream`
  (`execution_plan.rs:1799-1814`) coalesces multiple partitions. The analytical
  rewrite never invokes this owner. Config docs at `common/src/config.rs:1626-1640`
  explicitly describe the resulting successful-wrong-result limitation.
- Reachability: the new fixture has amount 50 at order 10 and amount 30 at
  order 11. `WHERE amount = (SELECT amount FROM orders WHERE order_id IN
  (10,11))` is accepted in the analytical configuration and interpreted as
  a relation join, permitting orders matching either value. Correct scalar
  execution refuses two inner rows. Projection scalars share this producer
  and can multiply outer rows. The current corpus's singleton/empty/NULL
  cases at `query_coverage.rs:188-212` do not test the violated invariant.
- Observable consequence: analytical-configured callers receive successful
  wrong answers for valid scalar syntax whose input returns multiple rows;
  the retained join applies those wrong set semantics and cannot repair them.

#### Decision-complete smallest correction

Keep the distributed physical-scalar flag disabled and retain the equality
hash-join/filter improvement. In the owned DataFusion fork, make the
uncorrelated scalar-to-join input a **native relational scalar dependency**
which validates the complete scalar input before emitting its scalar row.
Reuse the existing `execute_scalar_subquery` extraction semantics; do not
implement a Wyrd evaluator or add a post-join assertion. The dependency emits
one typed row for zero/singleton input (typed NULL for zero), or a typed
execution error on a second row. It must retain row multiplicity: DISTINCT,
first-value aggregation and LIMIT 1 are not scalar validation.

An existing suitable relational cardinality assertion is not present in the
pinned expr/optimizer/physical-plan implementation, and the installed
aggregate package has no single-value scalar validator to substitute. The
existing ScalarSubqueryExec cannot simply be re-enabled: its expression
results scope can be cut apart by remote stage boundaries, as documented in
distributed `task_metrics_collector.rs:221-235`. Therefore extend DataFusion's
native relational planning/execution machinery only enough to preserve that
scalar invariant at the rewrite producer, for both Filter and Projection.
The existing correlated analyzer rules are adjacent behavior, not a new
general decorrelation project.

The validation consumes a global single-partition input through DataFusion's
existing distribution/coalescing contract. Place global validation above that
coalescing boundary, rather than independently asserting each worker's rows.
The distributed planner already recognizes a CoalescePartitionsExec parent,
injects NetworkCoalesceExec and requires Maximum(1)
(`inject_network_boundaries.rs:364-400`). Use that existing collection contract.
Serialize any new native relational node using DataFusion's ordinary physical
plan proto encode/decode owner; the existing per-node native proto pattern
and ExecutionPlanEncodeCtx/DecodeCtx carry relational children, while the
problematic scalar expression scope is unnecessary. Keep Wyrd's revision pins
and codec fence coherent when advancing the fork. This adds no new stage
protocol, shared scalar-result registry, Wyrd public API or resource owner.

This is a bounded engine-semantic correction under the approved scalar
capability. Native single-partition collection and native relational codecs
already supply the needed distributed boundary; no new material scheduling
decision is established. If implementation discovers they cannot preserve
this invariant without a new stage/resource protocol, stop for
SPEC_REVISION_REQUIRED instead of weakening global validation.

Validation belongs before the scalar relation is exposed to either hash or
no-key join consumers. A check on emitted join rows can miss invalid scalar
input when the outer relation has no matches and can miscount outer
multiplicity. Do not turn this observation into a universal promise that an
optimizer-eliminated expression executes: require global refusal whenever
the scalar dependency is actually evaluated. The native existing scalar
owner provides the reference behavior for empty-outer execution tests.

#### Focused closure proof

One analytical-configured real server query rejects multiple scalar rows,
with another case splitting those rows across published/live data. Native
fork tests exercise batches/partitions, two identical values, no matching
outer rows with an evaluated scalar, and Filter/Projection consumers. Confirm
that complete cardinality validation precedes scalar output; retain singleton,
empty and typed-NULL results. Check empty-outer behavior against the native
scalar reference without requiring evaluation of deliberately eliminated
work. Retain the equality literal/footer proof. Exercise native codec
roundtrip and existing network-coalesce execution for the new dependency.
Use the narrow affected tests and checks, not benchmarks or whole journey
aggregates in the remediation task.

### FIND-bifrost-errors-4

- Discovery ID: STD-R2-1.
- Status: CONFIRMED.
- Classification: VIOLATION; bounded documentation correction.
- Obligation: AGENTS §16 requires Panics docs when panics remain possible;
  Rust-core Documentation states missing or incomplete rustdoc is a hard
  blocker. The task-review exception preserves this explicit rustdoc gate.
- Locations: `oracle/mod.rs:4473-4477`,
  `wyrd-mcp/tests/bifrost/mcp/query.rs:202-214`.
- Evidence: cumulative diff materially rewrites the mapper test and expands
  the MCP error journey. Both bodies contain assertions that intentionally
  panic on mismapped/misdisclosed errors. Their docs explain the workflow;
  MCP also has Errors. Neither documents the panic condition. The maintainer
  report's blanket statement that all such docs are complete is unsupported
  for these two symbols.
- Consequence: explicit repository documentation completion gate fails; no
  runtime defect is alleged.
- Smallest correction: retain existing intent docs and add concise Panics
  sections identifying the failed mapping/detail/redaction assertions on the
  two items. Batch this with FIND-3; no new harness, lint, runtime behavior or
  style-only review round.
- Focused proof: source inspection and the relevant formatting check.

## Rejected proposal and non-blocking limits

DF-R2-1's native NLJ trace is correct: range comparison is not an equijoin key,
and the NLJ Post hook forwards parent dynamic filters rather than synthesizing
a scalar-range producer. The current analytic equality proof must not claim
to close arbitrary range optimization. Nevertheless, the task's original
defect was discarding available predicates, and its bounded remote proof is
one selective scenario. Native pre-existing inequalities do not establish a
new regression. A new NLJ producer is omitted from required remediation.

No other discovery proposal remains. Source confirms narrow error wrapper
classification with resource precedence, authorized provider registration,
authenticated tenant-bound fragment identities, and test-support-only encoded
plan/row observations. None introduces a second durable lifecycle owner.

Verification is the committed evidence table, assessed against registered
test names and assertions, not fresh execution. Oracle 57/57 and the named
focused tests provide useful proof for their asserted paths, but not for
multiple-row scalar refusal. Earlier Python/TS proofs remain relevant to
unchanged client behavior; integrated fork/configuration verification belongs
at change review. Historical comment estimates do not measure optimizer cost.

The earlier untraced 604-byte reservation failure remains unexplained. A later
green run supplies no diagnosis; no source-backed new leak or weakened
assertion was found. Dynamic-filter generic reset is not supported by the
dependency and no changed Wyrd caller was established that uses it; the direct
scalar reset test is not general hash-join reuse proof. The prior verdict's
trailing blank line is a non-blocking cumulative diff-check note.

## Validation result

Complete validated retained ledger: **FIND-bifrost-errors-3 and
FIND-bifrost-errors-4**. Prior FIND-1 and FIND-2 are closed for their diagnosed
causes. Result: **FIX_REQUIRED**, with one bounded semantic remediation and
the two rustdoc corrections batched into it.

Skipped: no benchmarks or executable test reruns; only this report was written.
Risk: analytical multiple-row scalar inputs remain successful wrong results
until the native relational producer enforces their cardinality.
