# TASK-001 behavior review

## Result

**PASS within the assigned behavior scope. No proposed material findings.**

This is an independent static acceptance review of the complete range
`5234961d51798581a026834cfffd5efaadc226b4...9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
It is not the final task verdict: required specialist coverage and independent
validation remain the orchestrator's responsibility. No tests/builds were run
and no production, task, or specification files were changed.

## Authority and scope

Read the root instructions, task-review skill, approved revision-2 specification,
original TASK-001 (including its available implementation evidence),
`changes/spec-driven-development.md`, `agent_docs/index.md`,
`agent_docs/rust.md`, `agent_docs/maintainer-style.md`, and the source-authority,
Variant SQL, nested-field-read, and resource/failure sections of
`architecture/bifrost-design.md`. No `.codegraph/` exists. The changed source
paths have no applicable nested AGENTS.md. All 11 changed files were inspected;
source-only changes are concentrated in Oracle's live/codec/follower owners and
Scribe's captured-memory reader. The other source changes close request/read
callers; test support moves preserve the existing row renderer.

No prior acceptance evidence was carried forward. The scope was the caller-to-result
memory workflow, exact filtering, ordered projection, Variant adaptation and
fallback, negative paths, and credible regressions, with the full range available.
Persistence, lifecycle/accounting, security, and repository rules were traced at
the relevant seams; selected specialists own their deeper independent audits.
No further reviewer was spawned.

## Acceptance matrix

Paths below are relative to `crates/vala/vala-bifrost-redux/src/`, except where
otherwise stated. Verification names identify source inspected and available
execution claims, not tests rerun by this reviewer.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / scenario 1: requirements survive local and remote source resolution | `oracle/live.rs:481-506` puts predicates and outputs into the fingerprinted placeholder; `oracle/codec.rs:429-456` serializes both; `oracle/follower.rs:730-780` parses and forwards them | `learned_variant_requirements_reach_memory` inspects received expressions, pruning and fields; real journey selects both `one_mixed()` and `role_separated()` | PASS |
| Ordered and repeated projections retain fixed names/types and exclude filter-only inputs | `LiveScribeExec::try_swapping_with_projection` merges outputs onto the closure; `signed_requirements` derives output schema from that closure; `AdaptedRequirements::evaluate` emits outputs in order | V1 checks `r,v,r2`; V2 checks repeated `v,r,v`, type and values; journey checks repeated Variant output | PASS |
| Exact-versus-pruning meaning established before IO | `oracle/live.rs:631-675` reports eligible static filters exact; `oracle/follower.rs:730-776` parses requirements before opening tail and separately constructs optional `BatchPruning`; `scribe/memtable.rs:2559-2578` applies the exact Boolean mask regardless of bounds | Exact-filter live unit test; V1 checks both exact predicate and separate pruning; V2 checks COUNT and null/IN results | PASS |
| REQ-002 / scenario 2: covered scalar/filter-only memory paths avoid unrelated root reconstruction | `ReadRequirements::adapt` uses the installed DataFusion adapter once per generation; `AdaptedRequirements::evaluate` evaluates adapted expressions before fallback; `ReadableSource::project` no longer eagerly reconstructs requirements' roots | V2 scalar/count/null/IN/nested/list matrix observes zero whole-column reconstructions; V3 repeats covered queries through real SDK/server topologies | PASS |
| Missing/null/mixed types retain exact SQL semantics | Adapter evaluates the complete expression on each source layout; masks reject null rows through Arrow's filtering; unsuccessful adaptation/evaluation falls back to original logical expression | V2 includes missing region, null root, numeric region mixed with text, `IS NULL`, `IN`; V3 has typed/mistyped/null/missing documents | PASS |
| Whole-root/residual and unsupported/unshredded uses retain complete fallback | `scribe/memtable.rs:2359-2413` retains the original reconstruction/cast owner; `:2481-2503` retains original expressions when adaptation fails; `:2560-2569` and `:2604-2623` reconstruct logical rows for evaluation failures/type mismatch | V2 compares complete root JSON, including `extra`; canonical/learned batches share fixture and expected answers; V3 compares complete `to_json` documents and bare-root cardinality/reconstruction. Forced adapter-failure branch is static-only evidence | PASS |
| Empty output projection retains surviving row cardinality | `AdaptedRequirements::evaluate` takes rows from the filtered physical batch and supplies `RecordBatchOptions::with_row_count`; `exec.rs::project_batch` also preserves row count | V1 empty output schema; V2 zero columns with two surviving rows per generation; V3 filter-only COUNT returns one | PASS |
| Canonical and learned captured generations union without duplication | `LiveCapture::read` enumerates retained sources once; requirements adapt independently in each `ReadableSource`; follower emits memory and staged leaves through existing union owner | V2 captures canonical and learned generations together, expects two batches with identical per-generation answers; journey publishes first cohort before querying learned cohort | PASS |
| Trustworthy statistics never replace exact filtering | Required-field statistics remain unknown after cast/null-fill; changed row masks make bounds inexact before output statistics projection; exact mask is unconditional when requirements include filter | V2 filters null/mixed predicates and compares exact survivors; unchanged MemtableSource consumes resulting bounds for sizing/pruning | PASS |
| REQ-004 / scenario 3: permissions and assignment/schema refusal precede source reads | `PhysicalPlanFollower::preflight` checks plan fingerprint, binding, role/fences and exact scan set; `signed_closure_schema` plus `signed_requirements` run before `tail.read_live`; resolver rechecks full schema | V1 off-closure/non-Boolean/misaligned outputs never open tail; existing mismatch tests remain; V3 unprivileged caller checks no fragment dispatch | PASS |
| Captured-generation ownership, deadline and cancellation behavior remain connected | `FetchLiveTailService::open_capture` retains existing stream/query/binding/deadline checks; `LiveCapture::read` still clones captured charges and staged lease; follower supplies charges to existing `MemtableSource` and staged objects | V3 cancellation uses a second cohort and `await_live_released` checks open producers, follower query bytes and all Oracle admission/query memory; existing V5 lifetime journey retained | PASS |
| REQ-005 memory measurement absence stays distinct from file bytes | No scan-metric producer or aggregation definition changes; follower collects actual scan handles after pushdown; memory source remains a non-file source | Existing `scan_stats_preserve_non_file_absence_and_scan_aggregation` asserts memory `None` and explicit remote byte/file aggregation | PASS |
| Common path preserves staged/published behavior pending TASK-002 | `ScribeTailResolver::live_leaf` still uses `hot_parquet_scan` and existing lease; requirements wrap staged scan in normal filter/projection operators; follower runs existing FilterPushdown; published reader policy unchanged | Existing staged row-group pruning/lifetime tests preserved; oracle journey family claimed green; full file-specific consolidation/proof belongs to TASK-002 | PASS |
| No new public query mode, durable representation, learning policy, planner, ingestion change or dependency family | Complete range has no manifests, contracts, WAL/persistence writer, learning implementation, SDK or public query payload changes; new requirements belong to existing private scan/capture path; existing adapter and optimizer owners are reused | Static diff; generated-contract check is inapplicable to this range; speedup/L1 measurement explicitly belongs to TASK-003 | PASS |
| Scalar-subquery test correction preserves the original regression property | `peer_network/live_subquery.rs:177-231` still checks picked id, exactly two fragments, exactly two returned rows, and both static/resolved timestamp literals; its changed observation decodes `RemoteScanPayload.filter` | Independently traced encoding/snapshot path below; existing journey pass is reported in task evidence | PASS |

## Caller-to-result and adversarial tracing

### Requirements and source authority

`OracleTableProvider::scan` derives one ordered required-column closure and
builds persisted/live siblings beneath the existing union. Its unchanged
`required_columns_closure` deliberately keeps a time field for a no-column
scan; an empty SQL projection therefore does not create an invalid signed
assignment. `LiveScribeExec` now retains this closure separately from advertised
outputs, so projection absorption does not change the field indices used by
filters or route pruning. Physical projection merging uses the installed
`ProjectionExprs::try_merge`, not a second expression-rewrite implementation.
The union rule extends the existing boundary and calls the existing projection
pushdown rule. Non-live children keep normal projection operators.

The live fragment fingerprints encoded bytes containing the expressions. The
follower's scan collection records the entire placeholder payload and still
requires exact assignment/schema-fingerprint membership. Before reading,
`ScribeTailResolver` obtains its schema from the existing authoritative owner,
validates the signed closure, parses requirements against it, and checks the
local scan identity. Schema validation after resolution uses the same
`signed_requirements` derivation, preventing a changed output shape from being
silently treated as the original closure. Repeated output names work because
`project_batch` checks matching positions before looking up by name; the new
requirements output is already ordered.

I inspected the negative V1 cases against the actual producer/consumer code,
not only their names. Misaligned expressions/names are rejected explicitly;
non-Boolean filter is refused; the tested outside-closure column cannot plan
against the closure; no request is appended to the recording tail in those
cases. This supports those tested boundaries without claiming exhaustive
malformed-physical-expression validation, which the security specialist audits.

### Memory evaluation and fallback

Every captured source obtains the required-field physical view and existing
conservative projected bounds. Bounds may skip impossible batches, but cannot
stand in for exact filtering. Requirements adapt once against the generation's
physical schema, keeping original expressions available. Exact masks are
computed before output expressions and filter both physical and already-restored
logical rows together. Outputs then evaluate against the surviving batch. Bare
columns reconstruct individually; computed outputs use adapted results only
when evaluation succeeds and type matches. Otherwise one logical batch is
constructed and the original expression executes there. Errors in that logical
execution remain query errors rather than silently retained/dropped rows.
Zero matched rows are skipped; zero output columns retain the surviving row
count explicitly. Changed row sets make statistics inexact, so original row
counts are not falsely advertised as exact after filtering.

The existing `schema::logical_column` remains the reconstruction owner. The
new `logical_column`/`logical_batch` methods extract that existing operation for
lazy use; they do not implement a second Variant representation or decode
semantics. Adapter failure restores the full required logical closure, which
can be more work than a covered path but satisfies the approved fallback.
No new fallback product decision is necessary.

V2 is stronger than checking a shredded layout: it reads both canonical and
learned sources, compares expected scalar/null/mixed/IN/nested/list values and
records a reconstruction at the actual logical-column boundary. It explicitly
requires zero for covered reads and one for the learned whole root. V3 proves
that query planning actually reaches that path after a claim teaches the next
generation; its result assertions would still detect wrong SQL even if the
counter stayed zero. The bare-root journey checks cardinality rather than each
root value, but V2 and the journey's complete `to_json` comparison supply the
complete residual-value assertions at their respective owners.

### Preserved sibling and failure paths

Absent requirements preserve the original memory projection path. Callers in
shards/persistence tests explicitly supply `None`; requirements do not alter
capture ownership or ingestion. Staged inputs consume the same filter/outputs
using ordinary DataFusion FilterExec/ProjectionExec over the existing leased
hot-Parquet owner. No leader-side remote footer read or new file lifecycle is
introduced. A construction/evaluation/decoding error propagates through the
existing query/follower result; the changed path contains no process exit or
panic on an ordinary expression error. Codec version 2 deliberately rejects
an incompatible extension rather than execute incomplete requirements.

The new cancellation journey waits until the first live batch has actually
reached Oracle, then cancels while production is paused before its second batch.
The extra learned cohort makes the pause reachable; its release assertion checks
all Scribe follower bytes and Oracle admission/query-memory snapshots. This is
credible lifecycle regression proof after source-local expression execution,
while the invariant/system reviewers must independently assess temporary
allocation accounting and interruption during synchronous capture evaluation.

### Independent scalar-subquery diagnosis

The original assertion printed the protobuf `PhysicalPlanNode` and searched for
the H1 static timestamp and H2 resolved scalar timestamp. Before the change,
`LiveScribeExec::fragment` emitted a native `FilterExec` whose protobuf expression
was visible in that Debug representation. The complete diff proves that the
new fragment is a single `RemoteSourcePlaceholderExec::with_requirements`.
`OraclePhysicalExtensionCodec::try_encode` serializes its filter into
`RemoteScanPayload`, then serializes that message into the envelope's byte
payload; the outer PhysicalPlanNode therefore exposes bytes rather than the
filter's literal values.

`LiveScribeExec::resolved_partition` still invokes `snapshot_physical_expr`
before fragment creation, then narrows routes using the resolved predicate.
The changed test decodes precisely that envelope through the real codec and
searches its decoded `.filter`, retaining both timestamp assertions. It does
not delete or weaken the picked-event result, two-dispatch assertion, or two-row
footer assertion. Omitting the resolved scalar would fail the decoded predicate
check and/or dispatch/returned-row checks. The new observation also rejects an
unexpected non-extension root. Thus the stated failure cause and the correction
are supported independently by producer and consumer source. I subsequently read
the original independent diagnostician report and disposition supplied under the
task's Diagnoses section during review. Its encoding explanation and minimal test
correction agree with this independent source trace; my acceptance conclusion
does not rely on its verdict. Scope.md records the supplemental-evidence hash
correspondence without changing the source candidate or approved obligations.

## Proposed findings

None. No source-proven MISSING, INCORRECT, DRIFT, VIOLATION, or REGRESSION
meeting the task-review materiality/correction-burden threshold was established
within this assigned scope. No optional improvement or speculative hardening
is proposed. No remediation machinery is recommended.

## Verification notes and limits

The task records V1/V2/V4/V6 passes, V3/V5 Postgres-backed passes, 99 owner tests,
63 Oracle journeys, format/lints/dependency/tenant checks and diff validation.
I inspected the actual assertions and relevant execution source; those command
results were available claims and were not independently rerun. The new tests
do not deliberately force the adapter-error/type-mismatch fallback branch;
acceptance there rests on traced original-expression fallback plus canonical,
whole-root and residual-value tests. I did not run performance benchmarks;
TASK-003 owns measurable CPU/latency acceptance. TASK-002 owns full file-specific
lifecycle and executed-file accounting closure.

At report close, HEAD remained
`9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`. Authority SHA-256 values matched scope:
spec `6709bdcd455c6b669ebd335af83412d5065622825795ac229dd2e2b6a01dfa9f`;
original task `e49724f93a33de8ff6b0f808ba4125ad4742f579188085230898ff79557e55d8`.
The subsequently supplied diagnosis evidence changes only the task evidence hash
to `dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`,
as documented by the exact correspondence in scope.md.
Existing unrelated benchmark-scaling deletions were not reviewed or modified.

Skipped: test/build reruns and benchmark execution; no source edits.
Risk: this static PASS depends on independent specialist/validator coverage and the recorded execution evidence.
