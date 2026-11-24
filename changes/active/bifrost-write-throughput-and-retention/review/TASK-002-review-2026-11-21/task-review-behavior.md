# TASK-002 independent behavior review

Subject: base `5a057524f07d23a515d505caa4955539926d7fee`, candidate
`c40e7245c3ada4a35177e3a8c2d37c42d058c619`; approved
`SPEC-bifrost-write-throughput-and-retention` revision 3 and original
`tasks/TASK-002-cpu-throughput.md`. Source was not modified.

The current maintainer instruction approves the Arrow fork and expressly excludes
deferred benchmarks as an acceptance blocker for this review. This report makes no
claim that the 80 input MiB/server CPU-second target, workload matrix, or read tails
have been demonstrated.

## Scope and authority

Primary scope is requirements, scenario coverage, preparation/projector semantics,
shard publication boundaries, memtable pruning, Oracle planning/authorization,
dependency coherence, and applicable repository rules. Separate specialists own
encoder retained-memory/lifecycle and WAL persistence/recovery. Their reports are
needed before final acceptance of those boundaries.

Read root `AGENTS.md`, `changes/spec-driven-development.md`,
`agent_docs/index.md`, `agent_docs/rust.md`,
`agent_docs/maintainer-style.md`, relevant `TESTING.md` sections, the assigned spec,
original task and execution README, CPU design, and owning Bifrost architecture
sections on durability/visibility, Scribe ownership, Oracle planning, and resource
invariants. No nested AGENTS files were found in the affected Vala/shared trees.
Used CodeGraph before source navigation and inspected the immutable diff directly.

Navigation: server OTLP preflight/decode → flattened `LogInput` →
`LogColumns::with_capacity`/`RowEnvelopes::with_capacity`; mailbox →
`ShardOwner::apply_command`/`drain_mailbox` → existing group completion;
`LiveCapture::read` → `ReadableSource::prepare` → `RecordedBounds` → identity
second pass → lazy batch evaluation; `Oracle::plan_logical` → payload gate →
registered query planner in `Oracle::plan_physical`. All Arrow workspace patches,
lock entries, and workspace-hack pins advance together to `f349772c38fe34d874f0050708da8386f3d1b849`.

## Coverage assessment

| Requirement / task scenario | Source and assertions inspected | Assessment |
|---|---|---|
| REQ-001 / scenarios 1–2: existing enqueue, durable completion, and pre-ACK visibility | `scribe/shards.rs:1507–1531` leaves append commands queued without inspection publication; existing group publication before ACK remains on the unmodified completion path. New `mailbox_drain_publishes_inspection_only_for_control_commands` checks two queued appends leave a sentinel unchanged and `FlushAll` publishes actual memtable state. | PASS in primary scope; WAL/recovery specialist checks downstream boundaries. |
| REQ-010 / scenario 1: checked counts and unchanged accepted input | `tables/logs/projection.rs:141–203,268–301` uses existing flattened counts; validation/push/rejection ordering is unchanged. Existing server `preflight_logs_protobuf` enforces the expanded ceiling and decoded reservation before projection; borrowed/fallback decoding stays unchanged. `log_projection_reserves_columns_from_the_record_count` asserts exact fixed-width retained capacities and accepted/rejected counts. | PASS. |
| REQ-004 and REQ-011 / scenario 2: compatible ordered chunks and accepted replay | The diff changes prepared payload ownership and bounded writer windows, not WAL framing or fence/checkpoint authority. Task evidence reports exact reference bytes, 200-chunk replay, short/error writes, retained leases, redux integration, and final Scribe recovery journeys. | Independent WAL specialist owns final source-backed conclusion. |
| REQ-012 / scenario 3: safe bounded context reuse | Existing ingress pool receives an encoder owner; no additional pool/worker count. Production preprocess passes that owner. Shutdown clears retained slots. Revised exact memory assertions retain zero Prepared-after-ACK and add the measured worker Metadata owner. | Independent lifecycle specialist owns final source-backed conclusion. |
| REQ-013 / read-side scenario: prune before projected bounds | `scribe/memtable.rs:2221–2305,2383–2505` maps projected predicate columns back only to unchanged source columns. Missing/cast columns answer unknown. Missing batch statistics use unknown source statistics. Initial false entries remain false when combined with the identity-bound second pass; identity bounds are computed only for initially kept batches. `capture_read_projects_bounds_only_for_batches_recorded_bounds_keep` asserts exact kept LSN and one projected-statistics construction; existing `identity_time_and_child_predicates_prune_only_by_known_bounds` covers old/new schemas, missing fields, nulls, casts and identity/time conjunctions. Whole-schema sharing and existing pruning tests cover unchanged cases. | PASS for inspected algorithm and recorded owner-test evidence. |
| REQ-013 / read-side scenario: optimized authorized plan executes once | `oracle/mod.rs:3225–3268` binds parameters, optimizes, authorizes payload columns, then invokes the same session's registered query planner directly. No authorization bypass, alternate root selection, new cache, or routing heuristic. `physical_planning_reuses_the_optimized_logical_plan` asserts no extra logical rule invocation and a physical table source; existing bind-value and gateway payload tests exercise unchanged producer semantics. | PASS for inspected algorithm and recorded owner-test evidence; focused journey execution pending central runner. |
| REQ-013 / CPU and read/mixed-load qualification | Task explicitly records unmeasured results. | UNRESOLVED measurements; excluded as a blocker by current maintainer instruction. |

The primary diff adds no PostgreSQL, object storage, Vault, or Scribe wait. Existing
WAL append/fsync, acceptance fence, query cut/source IO, and admission waits remain
with their previous owners. The physical planning call changes only which
DataFusion entry point invokes the registered planner, removing redundant logical
optimization; it does not add another physical build or storage operation.

No functional implementation defect was found in the primary scope. The new
adapter and owner methods fit existing responsibilities; no alternate orchestration,
new public contract, SDK projection, generated schema, or migration is introduced.

## Proposed finding B1 — mandatory item rustdoc was displaced

- Classification: `VIOLATION`.
- Authority: root `AGENTS.md`, Code: “Every new or changed Rust item has rustdoc:
  intent, workflow role, invariants” and “Missing rustdoc blocks merge.”
- Location: `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:2221–2228`
  and `:2313–2314`.
- Starting condition/trigger: the new `RecordedBounds` declaration was inserted
  between the existing `ReadableSource` documentation and its declaration.
- Expected: each owner retains its own accurate item documentation.
- Actual/evidence: the six-line description beginning “One bucket's or generation's
  batches” now attaches to `RecordedBounds`, immediately before the adapter's own
  description. `ReadableSource` now begins with `#[derive(Debug, Clone)]` and no
  item rustdoc. The base-to-candidate diff proves this displacement.
- Consequence: the ownership/lifetime description is attached to the wrong type and
  the changed source violates an explicit merge-blocking documentation rule.
- Smallest correction: move the existing six-line capture description back above
  `ReadableSource`, leaving the adapter's own description above `RecordedBounds`.
  No production or test changes are needed; source inspection and formatting prove
  closure.

This is a bounded mandatory documentation correction, not a request to rewrite
surrounding documentation or enforce stylistic preferences in unrelated code.
Independent validation must decide whether to retain B1.

## Execution evidence and limits

Inspected the actual assertions named above and the recorded task evidence:
993 redux library tests, earlier 1067-test redux integration result, candidate
clippy, fork tests, and final 75 Scribe/OTLP journey tests. Earlier scenario prose
that says Scribe journeys were not yet run is superseded by scenario 3's final
75-test evidence. The earlier integration run predates final context retention;
final context-specific library tests and final Scribe/OTLP journeys supply later
boundary evidence.

The orchestrator is running mandatory formatting, workspace lints and dependency
checks centrally, and has been given two exact focused Oracle journey selectors:
`published::source_transitions_preserve_exact_rows` and
`published::variant_sql_registry_covers_every_session`. The former drives live and
published capture consumers and exact rows; the latter drives Interactive and
Analytical registered planners and exact payload-column refusal before remote
work. These results must be recorded in the verdict; they were not rerun by this
reviewer to avoid overlapping Cargo operations. `git diff --check
5a057524f...c40e7245c` passed during this review.

TDD: scenario 1, chunk ownership/windows, read-side reductions, and the fork record
specific missing-behavior RED failures. Scenario 3 Wyrd reuse records “worker kept
no context” with `restore` stubbed and an unrelated-pool retention failure. A stub
can be valid test-first scaffolding, but can also be a later mutation; the bundled
commit and task record do not independently reconstruct that order. The record
claims TDD PASS; this reviewer found no evidence proving a violation and does not
infer one from bundled commits, but cannot independently certify the full historical
ordering. Behavioral proof and performance qualification remain separate.

Skipped: benchmarks as instructed, broad journey suites, duplicate Cargo reruns,
and implementation changes. Performance gains and tails remain unmeasured.
