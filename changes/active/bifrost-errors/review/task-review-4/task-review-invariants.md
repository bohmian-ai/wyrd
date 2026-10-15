# Independent invariant review — round 4

## Subject, authority and method

Wyrd base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`, candidate
`7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`; DataFusion base
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`; distributed fork unchanged
at `8fac2a978bdcf8e06457238ebfe9c7f35023c123`. No original task exists;
spec revision 1, owner steering, and R1–R3 remediation are authority.
Source stayed immutable; only this report was written.

Read AGENTS/agent-rules, specification workflow and maintainer guidance,
reference router and applicable DataFusion/OLAP/error/testing authority, approved
spec, R3 task/evidence and prior validation. CodeGraph preceded Wyrd navigation.
Inspected full cumulative runtime and fork changes, latest bound delta,
closed max_rows match, Filter singleton/FD producer, Aggregate FD producer,
scalar rewrite/invariant consumers, native single_value state/merge and codec,
first-poll snapshot and planned-route ownership, hash build/probe readiness,
peer tests and compact corpus, error mapper and public client paths.
No heavy verification or benchmarks ran; committed commands are evidence claims
reconciled against their actual assertions, not independent execution here.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller logical planning errors expose safe planner detail | oracle/mod.rs:3735–3800 narrow wrapper classifier and separated logical/physical mapping | Mapper exact test; Rust/Python/TS/MCP error journeys retain expected public codes/details | PASS |
| INV-001 tenant binding and no unauthorized schema disclosure | Existing pre-session table authorization; runtime leaf retains binding, grant and signed fragment ownership | Retained unauthorized-table journeys; no authority expansion in runtime delta | PASS |
| REQ-006 valid scalar/IN/EXISTS and run_id correlation | Existing native rewrites; scalar snapshot plus normal hash dynamic filter | Public journeys and compact mixed-state corpus; explicit expected answers | PASS for exercised valid shapes |
| FIND-1 remote selective scalar is static before dispatch; route ownership stable | live.rs:298–317 deals planned route share before snapshot pruning; :441–476 Pre/Post propagation; first-poll LiveFragmentRead resolution; hash_join/stream.rs:689–773 build/report before probe polling | Remote literal journey requires exactly two fragments/two footer rows and both bounds; reset unit exercises fresh snapshots | PASS |
| FIND-2 compact SQL coverage without generated framework/public DataFusion oracle | Fixed mixed-state query_coverage.rs; no promoted testing DataFusion owner/framework | Corpus covers joins, CTE joined to second table, membership, EXISTS, aggregates, windows and scalar controls | PASS |
| FIND-3 scalar cardinality never bypassed through unsafe singleton bound | single_value correctly guards unknown/multi-row plans, but Filter::is_scalar still manufactures Some(1) for key equality to a row-dependent expression | New FULL/grouping SQL, bound and peer tests do not cover this sibling false proof | FAIL |
| R3 corrected FULL/grouping expansion and checked arithmetic | plan.rs:1467–1474, :1493–1516 return unknown for row expansion/overflow | max_rows_is_a_sound_upper_bound plus SQL/peer refusal cases | PASS for diagnosed shapes |
| R3 complete singleton-proof owner audit | max_rows Filter branch trusts Filter::is_scalar; that method accepts arbitrary non-Column RHS without input-dependence checks | Existing bound test has no unique-key row-dependent filter case | FAIL |
| Existing aggregate/global count/codec semantics | Native accumulator counts every row including NULL/equal values; normal Partial/Final and native AggregateUDF encoding | Accumulator cross-batch/merge tests, proto roundtrip and remote published+live duplicate refusal | PASS |
| FIND-4 missing panic docs closed | Mapper test and MCP journey now document panic assertions | Source inspection | PASS |
| AC-003 focused verification | Pinned fork revision/lock/codec fence advance consistently; committed exact commands include executed test counts | R3 evidence reports focused Rust/fork, fmt/lints/codegen/deps/tenancy checks green; no independent rerun here | PASS for recorded paths; does not cover finding below |

## Material proposed finding

### INV-R4-1 — same FIND-bifrost-errors-3 remains open: row-dependent key equality is not a singleton proof

Classification: INCORRECT, incomplete closure. Violated obligation: REQ-006
scalar semantics, R3 criteria 1–2 and its explicit sound-proof invariant.

Invalid-state producer: fork `datafusion/expr/src/logical_plan/plan.rs:2909–2972`
`Filter::is_scalar`, consumed by `LogicalPlan::max_rows` at :1453–1457.
Changed consumer: `datafusion/optimizer/src/scalar_subquery_to_join.rs:383–405`.
Sibling consumer: `datafusion/expr/src/logical_plan/invariants.rs:188`.

Producer-to-consumer trace:

1. Ordinary GROUP BY creates a nonnullable `Dependency::Single` covering the
   output (`common/src/functional_dependencies.rs:487–508`), and Aggregate
   installs those dependencies (`plan.rs:4185–4192`). These are legitimate
   unique-key facts, not erroneous cardinality estimates.
2. `Filter::is_scalar` finds covered unique keys, then counts an equality as
   fixing a key whenever one side is a Column and the other is not a *bare*
   Column (:2936–2958). It does not check whether the other expression depends
   on this input. `x = abs(x)` therefore qualifies even though x changes for
   each row. Both x=1 and x=2 pass and remain unique.
3. `max_rows` returns Some(1) irrespective of the input bound. Projection and
   alias preserve the false proof. The shared Filter/Projection scalar rewrite
   consequently omits single_value, treating the scalar input as a relation.
4. With physical uncorrelated scalars disabled in analytical planning_session,
   the native physical scalar checker cannot repair the omitted check. Two
   surviving input rows can match/multiply outer rows instead of refusal.

Reachable candidate query (substitute an existing authorized nonempty events
table):

```sql
SELECT id,
       (SELECT x
        FROM (SELECT x FROM (VALUES (1), (2)) t(x) GROUP BY x) g
        WHERE x = abs(x)) AS picked
FROM vala.bifrost.{events};
```

A Filter variant compares id against that scalar. VALUES/GROUP BY need no
additional catalog privilege; the real outer table goes through existing
Wyrd discovery/authorization. This is the same producer invariant the R3 task
explicitly required auditing, not unrelated pre-existing debt. The expression
is immutable but input-dependent, so checking volatility alone is insufficient.
No new plan node, Wyrd guard or query-shape exception is warranted.

Observable consequence: analytical scalar queries can still succeed with
wrong results despite the aggregate itself correctly refusing two rows when
present. The old FULL/grouping mistakes are corrected; this sibling path
still produces the same invalid proof. Source establishes the logical error;
focused optimizer/execution reproduction should corroborate survival of this
predicate before final validation. No runtime reproduction was run by this
reviewer; the orchestrator has been notified of the exact query/source path.

Smallest safe correction: repair the existing Filter::is_scalar proof producer.
A unique-key equality contributes a fixed key only if the opposite value is
proven constant for the relevant input invocation; otherwise retain the input's
bound through max_rows and let existing single_value execute. Reuse existing
expression input-reference/volatility analysis (`Expr::any_column_refs`,
`is_volatile`, plus existing subquery/outer-reference semantics); do not
introduce a second singleton classifier downstream. Preserve literal/parameter
and genuinely fixed-expression equalities, composite-key completeness,
nullable-key handling and direct Column=Column rejection. Do not mistake a
nested expression containing a Column for a constant. For values whose
constancy cannot be proved, conservative unknown/input bound is sufficient.

Focused closure proof: a direct grouped-unique-input filter has at least two
rows and cannot report Some(1) for x=abs(x); include a genuinely fixed-key
control. Flag-off scalar Filter and Projection both refuse this two-row
input, while native physical scalar behavior remains the reference. Add the
shape to the existing authorized analytical negative journey, keeping one
client/fixture, and retain existing FULL/grouping/checked-arithmetic and remote
literal/footer proofs. No benchmark or broad harness is needed.

## Other invariants and limits

All ordinary upper-bound operator arms were inspected: window/projection/
repartition/distinct/alias do not expand rows; sort/fetch and limit are caps;
values/empty are exact; global/literal aggregates bound at one; ordinary
nonempty-key aggregate groups cannot outnumber input; FULL/grouping now unknown;
left/right joins with empty-side handling and checked positive products are
sound; semi/anti/mark/asof do not outnumber their retained side; union checked
sum is conservative. Unknown operator cases remain unknown. The remaining
source-backed failure is the Filter proof above, not the corrected arithmetic.

SingleValueAccumulator state has only native producer rows 0 or 1; any second
row fails before publishing a partial. merge counts partials globally and
retains NULL rows. No alternate result store, worker protocol or resource owner
was added. Hash build/report readiness precedes probe polling, so lazy live
snapshot is aligned with the established dynamic-filter producer. Planned
route dealing before per-partition narrowing preserves disjoint ownership.
The join/leader filter remains authoritative when snapshots are conservative.

No changes weaken timeouts, assertions, retries or cancellation to conceal the
recorded API-key collision; the negative journey now creates one key/client.
Earlier untraced reservation observation remains undiagnosed evidence context,
not a newly established leak. Runtime cardinality refusal remains opaque 500
as explicitly preserved by R2/R3; this is not a new error-mapping finding.

## Result

**BLOCKED**: the orchestrator reports that Wyrd HEAD changed from the immutable
candidate `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e` to
`17697dc8c2740213565e5f913cde1edfb0cb8ce5` (merge main) during discovery.
Per the skill, discovery stopped and no new subject is reviewed. Above source
notes preserve the already-inspected candidate proposal; they are not a final
validated finding or acceptance result for the moved HEAD.

Proposed FIND-bifrost-errors-3 Filter proof bypass remains unvalidated; no
focused runtime reproduction was run by this reviewer. FIND-1, FIND-2 and
FIND-4 closure notes describe inspected source only. No candidate edits,
benchmarks or heavy verification. A new immutable review is required.
