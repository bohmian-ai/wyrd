---
id: TASK-012
status: proposed
approved_spec_revision: 13
requirements: [REQ-015, REQ-016, REQ-017, REQ-018]
acceptance: [AC-010, AC-011, AC-012, AC-013, AC-014]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: []
---

# Integrate code mode without losing dynamic pipelines

A developer can use generated Python to gather evidence through declared tools
inside B's existing dynamic Verifier pipelines. Steps, fanout, repeats,
speculation, approvals, deferred inputs, queued recovery and GitHub delivery
keep their behavior. The port also supplies the shared resource ledger, reusable
workers and workspaces, streamed evidence, throughput fixes, instrumentation,
three-language benchmark consumed by TASK-013 and TASK-014.

## Authority, inputs and integration choice

This task derives from approved GitHub pipeline spec revision 13 and contracts,
code-mode spec revision 9 as amended by pipeline revisions 12 and 13, and the settled
maintainer directions recorded in TASK-013 and TASK-014. Revision 12 decides
nested exclusions, crates.io Monty, capacity isolation, recovery spend and the
contributor-contract reconciliation. The current directions also replace A's
200-tenant requirement with 100 tenants and assign two-replica proof to
TASK-014. Revision 13 replaces revision 12's capacity isolation with deployment
targets ([decision](../design/approved-deployment-targets.md)). Reconcile those contracts at their owners before affected
implementation. The present planning handoff changes only tasks and their index.

Inputs are B at `fa7c86dba8c3148a9621248dac94a7353ead24c8`, A at
`0eb5ccdd9ac0b7b35fa4d4f3c6afd8e091a176f0` (`change/verifier-code-mode`), and
merge base `11474b4a9aa95499c3fbd3a69393ff7590a5b245`. Revalidate identities
before execution; record any maintainer-selected successor, never silently
change the imported range. Read A's tracked packet through the pinned tree, at
`changes/active/verifier-code-mode/`: `spec.md`, `design/notes.md`,
`design/capacity-diagnosis.md`, TASK-001/002 and TASK-003/R1/R3 task files,
their cited review evidence and `spec-revisions/`. Read with `git show
change/verifier-code-mode:<path>` after confirming that ref still matches the
pinned A commit; otherwise read the pinned commit directly. B's applicable
supplements are `design/approved-nonblocking-execution.md`,
`design/task-010-benchmark-remediation.md`, and its runtime, extraction,
deferred-input and report-delivery agreements.

Use a **staged port of A's committed delta from the merge base into B's
owners**. B contains the larger orchestration workflow; reviewable,
owner-by-owner composition makes preservation explicit and avoids installing A's
incompatible capacity policy as an intermediate accepted implementation. This
costs a source-to-port coverage record instead of recording A as a merge parent.
Retain source commit identities and account for every A-only change as carried,
adapted or excluded with an authoritative reason. Do not cherry-pick shared
resource-scaling ancestry again or port only a chosen subset of A's runtime. Git
scheduling and commits remain maintainer-owned; this task authorizes no staging,
commits, checkout, reset, stash, merge or push.

The integration inventory and verification recipe are in
`/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode/target/codex-merge-plan.md`,
sections 2–7. Resolve all 37 listed textual paths, inspect the 38 shared
textually clean files and all 17 semantic conflicts; textual cleanliness is not
acceptance. Its reserved-decisions section is superseded by the decisions above.
Use its file table as a coverage inventory, not an instruction to replace whole
files. Preserve the local modified task index and untracked TASK-011 exactly
apart from separately authorized index additions. Exclude A's untracked
payload-sizing packet and unrelated software-factory work.

## Existing owners and required composition

Extend the complete existing workflows, rather than introduce another engine,
dispatcher, persistence service or resource authority:

- `vala-verify/src/blocks/{engine,agent,tools}.rs` and `blocks.rs` retain B's
  logical step/item/iteration/attempt identities, producer restrictions and
  checkpoint flow. Add A's tool ownership and settlement around B's borrowed,
  unspawned block futures. Keep B's `walk.rs` scheduling during this port;
  TASK-014 owns any later execution redesign, without a preselected scheduler. Cancellation dropping a block does
  not prove already-issued host work has settled. Settle it before retry,
  result selection, checkpoint or release of its resource ownership.
- One direct/nested tool dispatcher preserves declared-name and typed-argument
  validation, per-call tool/data permissions and nonblocking audits, bounded
  arguments/results, shared request/concurrency budgets, start-order records,
  recipient tags, original evidence/citations and check-Agent records. Forward
  B's provider `call_id` and producer through the recorder: ordinary model
  publication remains replay-safe and step-only. The nested bridge excludes
  `code.run`, `fs.write` and `publish`; Python already writes private `/work`,
  and nested publication has no provider call identity. **Decided in revision 12: nested `fs.write` and `publish` are excluded.**
  Ordinary declared model calls retain both tools; check Agents still cannot
  publish. Nested publication identities are outside tasks 012–014.
- Restore tool-call spend through the same attempt records B uses for
  `max_agent_invocations` after `db5520e38`. Each attempt records its
  invocation's spent count as each request spends, persisted in background
  with the execution state, and a later attempt, after a refusal or an
  interruption, resumes from the latest persisted count (revision 15). Share
  allowance across authored retries and preserve the original deadline
  across park/takeover. No direct resumable state is introduced.
- `wyrd-server/src/verification/hosted/` composes A's reusable fresh-session
  sandbox, runnable-turn bound and charged streamed workspace with B's mounted
  publications/collections, extraction, writes and immutable snapshots. Charge
  growth before mutation, preserve replacement/no-overwrite distinctions and
  unchanged content on refusal, and release scratch only after deletion or
  ownership transfer. Keep suspended session envelopes charged through reset or
  reap; retain charges for surviving references after cache eviction.
- The existing `wyrd-runtime` memory/scratch owner remains the single process
  ledger, including Bifrost, edge bodies, live queued state, frozen copies,
  encoding and terminal/outbox custody. Reserve before copying, transfer charges
  without gaps or double promises, and retain B's memory-first progress,
  bounded background group commit, ACK-before-retirement and claim backpressure.
  Do not add persistence waits to engine mutations or direct verification.
- Keep arrival-based direct deadlines through target lookup/admission/model
  work, lowered by the authored timeout. Retain B's verification-only gateway
  capture bypass, destination-batched direct evidence, individually fenced
  queued ACKs, mode-aware target cache and invalidation, queue-only refusal
  before steps, compact/details distinction and HTTP/MCP-only inspection.
  Preserve RBAC-only verification authorization and `verifier:approve`.
- Monty stays on its crates.io release; exclude A's Monty `[patch.crates-io]`
  entries, their git-source type-checking patch, and the fork-only sandbox code
  that depends on them (the `service_mounts`/`resume_serviced` split), recording
  each as excluded by maintainer decision in the coverage record. Keep the
  sandbox's runnable-turn bound working against the crates.io API. Keep B's
  pinned code-review library; resolve lockfile, workspace-hack
  and feature graph together. Benchmark dependencies and instrumentation stay
  behind diagnostic builds and out of production SDK packages. Compose authored
  Python stubs/exports and native sources; regenerate wire types, errors,
  loaders/declarations, testing declarations and docs from final sources.

## Capacity boundary and architecture handoff

**Revision 13 sets the isolation boundary through deployment targets, and it
lands here.** Delete the rejected queued reservation (`QueuedShare`,
`HostedLimits.queued_share`, the queued CPU/memory/scratch percent settings,
reservation-only tests and `ProcessResources::reserve` unless another
production owner uses it). In `combined`, direct and queued work share one
hosted capacity owner (one sandbox pool, runnable-turn bound and model-call
limit) charged to the one process ledger only while in use;
`WYRD_VERIFICATION_QUEUED_EXECUTIONS` caps queued concurrency and holds nothing
idle. Admission stays tenant-fair, replica budgets stay local, and the paths
stay portable with no OS isolation controls.

Replace the closed `WYRD_TARGET` set with `combined` (default), `direct`,
`worker`, `bifrost-ingest`, `bifrost-query` and `bifrost-maintenance`, with no
aliases. `direct` and `worker` run Scribe and Oracle without Forge;
`bifrost-maintenance` runs both Forge roles. Route, readiness, hosted blocks, gateway, queued runtime and GitHub ingress
follow the target's capabilities. A target refuses work it does not accept
before admission or any side effect, identically over HTTP and MCP, and never
forwards it. Update test-server target derivation and the Kubernetes
deployment manifests. Original deadlines, Busy deferral, fenced takeover and
ordered drain are unchanged. TASK-014 owns throughput optimization and final
saturation qualification on `direct` pods.

Before affected changes, reconcile only the shipped contract in
`architecture/wyrd-design.md` (Verifier / Agents, tools, and workspaces),
`architecture/bifrost-design.md` (Admission and memory / Resource and failure
invariants) and the accepted-authority security section. B's orchestration,
recovery, GitHub and Repository contracts stay normative; A's `call_tool` and
ledger mechanics stay normative. Remove forbidden OS-identity/network-isolation
wording and every queued-reservation, reserved-direct and borrowing claim; document
the target set in `architecture/operations/`; describe portable
interpreter/allocator limits and parent-mediated access honestly. TASK-014
rewrites the in-revision direct/capacity design; do not claim HA qualification
from functional integration. Keep Repository as kind 13, including the stale
architecture count. Reconcile the exact six-control union (`flush_bifrost`,
`wait_for_baseline`, `make_binding_due`, `hold_publications`,
`occupy_hosted_memory`, `wait_for_hosted_waiters`) across REQ-195, contributor
guidance, Rust guard, Python and TypeScript. Preserve strict detection of every
unsanctioned control, blocking wait semantics and production exclusion.

## Outputs, obligation coverage and observable proof

| Obligation source | Trigger and required observation |
|---|---|
| Code-mode revision 9 functional requirements/invariants and AC-001–018; diagnosis conflicts 1–3,7–8,11,13 | Real declared Agent step and check call nested tools inside fanout/repeat; dependent calls complete at concurrency one, record recipients and retain citations. Preserve the existing reduced-evidence efficiency journey and its byte bound, least-privilege recovery and authored-code isolation. Model-originated publication replays its original receipt; nested write/publication and check publication refuse. Cancel/timeout during host work, retry, park and take over: settled work and receipts persist, unsettled attempts consume their allowance, authority/deadline never replenish, and ownership releases only after settlement. |
| Pipeline REQ-001–014/016, AC-001–009/011 and INV-001–007; conflicts 9–13,16 | Retain connect→Repository→PR admission→pinned extraction→review→approval/deferred evidence→same execution→report/Check/comment journeys and negative flows across shipped surfaces. Source supersession/access loss fences continuation and delivery; provider retry never recomputes verification. No newly added SDK/CLI inspection where HTTP/MCP alone is sanctioned. |
| REQ-015/017–018, AC-010/012–014; code-mode resource requirements and AC-028–034; conflicts 1,4–6,10,12,14 | Pressure on publication mounts, scratch replacement, queued snapshots and held persistence remains fully charged and bounded; compute frees before queued ACK without losing custody. Each target accepts exactly its work and refuses the rest before any side effect over HTTP and MCP; `combined` holds no idle queued reservation; a `worker` completes webhook, deferred-input and approval journeys and its crash allows takeover without resetting deadlines; a `direct` pod never claims queued work. Compare direct c64 latency and throughput on a `direct` pod with and without saturated `worker` pods: no degradation beyond measured run variability. No warm-cache direct SQL/state, queued-only cache bypass, lost write, starvation or premature release. |
| REQ-195 and both packets' projection/isolation obligations; conflicts 14–17 | Exact six-control guard rejects any additional hook in all projections. Dependencies resolve against crates.io Monty with no fork patch; regenerated consumers compile, historical records decode, production packages exclude testing/benchmark machinery. |
| Diagnosis sections 5/7 and inherited TASK-003/R1/R3 obligations | Retain A's tracked packet and referenced evidence in B's change tracking with source identities and an explicit carried/changed/pending disposition for every inherited row. Keep TASK-003-R2 superseded and TASK-003-R3 open. TASK-013 owns the Rust-only 100-tenant mixed comparison; TASK-014 owns sustained 100-tenant saturation/recovery, remaining qualification, artifact analysis and two-real-replica evidence. Record the 100-tenant amendment against inherited REQ-033/AC-034 and R3 V6 rather than retaining their older threshold; no historical FAIL/UNVERIFIED becomes PASS by importing it. Preserve the shared resource-scaling packet, historical deprecated verifier-review packet and B's intentional execution/inspection distinctions. Reconcile packet coverage/status only against actual evidence, never invent review verdicts. |

Use diagnosis section 6's functional coverage on the composed tree, subject to
this task's superseding decisions and scoped verification below: real SDK →
WyrdTestServer + repository Postgres → SDK in Rust/Python/TypeScript; real
HTTP/MCP/CLI at their existing owners; engine unit/integration and server/SQL
seam tests. Add combined-path tests described above at those owners, not new
test binaries. Record each new test's actual focused command before RED.

Existing exact selectors (the code-mode selector arrives from A; build the
worker executable first):

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_durable_verifier_work::only_queued_verification_keeps_run_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)'
```

Combined server seam coverage, retaining the existing target and module names:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(/^(pg_durable_verifier_work|pg_hosted_artifacts|pg_hosted_files|sandbox_worker|pg_verification_routes|pg_verification_runtime|pg_github_pull_requests|pg_github_pull_request_extraction|pg_deferred_evidence|pg_approvals|pg_github_delivery)::/)'
```

Regenerate with `mise run codegen:regen`, `mise run ts:build`, `mise run
ts:build:testing`, `mise run docs:generate`. Use the combined-runtime selector
above and affected owner lanes: `test:verification:sandbox`,
`test:principals:integration`, `test:rust-sdk:journey`, `py:test:integration`,
`ts:test:integration`, `test:bifrost`, `test:sql`, plus affected MCP/CLI
journeys. Run `mise run check`, `check:deps`, `check:tenant-isolation`,
`codegen:check`, `py:lints`, `py:typecheck`, `ts:lints`, `ts:typecheck`,
`ts:napi:check` and `docs:check` through their existing mise tasks; format
changed languages. Explicitly inspect generated testing declarations: public
`ts:napi:check` does not cover them. Full shipped-surface suites and any
required broad aggregate belong to integrated change review. These are future
implementation commands, not planning-run actions.

After functional verification, the maintainer supplies an immutable composed
candidate. Capture full clean, quiet-host, unprofiled `mise run
bench:l1:verification`, `mise run bench:l1:gateway` and `mise run
bench:l1:bifrost`. Record exact tree/dependency/build identities, configuration,
resource assignments, fixture/method and every repetition; preserve
Rust/Python/TypeScript c1/c64 and matched-rate diagnostics, achieved server
concurrency, arrival lateness/GC, latency/overhead phases, parent plus
live/reaped worker CPU/memory/churn, offered/accepted/completed/refused counts,
evidence reconciliation, backlog/drain, memory/scratch and queued custody. Use
existing functional pressure journeys for the port's isolation boundary. The new
standard mixed benchmark and its baseline belong to TASK-013; sustained
saturation and two-replica qualification belong to TASK-014. Baseline validity
and target attainment are separate: record misses honestly, without declaring
either packet complete. Historical parent-only CPU stays parent-only. Dirty,
smoke, profiled or selected-step runs cannot qualify.

Deliver the composed source/contracts/projections, conflict coverage and
inherited-obligation dispositions, functional evidence, and immutable benchmark
records with exact commands to TASK-013 and TASK-014. Local diagnostic artifacts
referenced by the diagnosis must have retained, readable evidence or be marked
unavailable and rerun; a vanished target path is not closure. No source
modification of external reviewed libraries, payload-sizing redesign or nested
publication identity extension belongs here.

## Obligations

| ID | Promise (contract source) | Planned proof |
|---|---|---|
| OBL-1 | Port integrity: A's delta `11474b4a9..0eb5ccdd9` is ported into B owners; every A-only path is carried, adapted or excluded with a reason; source identities retained; A's payload-sizing and software-factory work excluded; local task index and TASK-011 untouched (task: integration choice) | Coverage record below; `git diff` audit |
| OBL-2 | Contract reconciliation before affected code: wyrd-design Verifier/Agents/tools/workspaces, bifrost-design Admission/memory, security accepted-authority; no OS-identity/network-isolation wording; no queued-reservation/reserved-direct/borrowing claims; operations target table and readiness mapping; Repository kind 13 incl. stale count; six-control union in REQ-195 and contributor guidance (REQ-015, rev 12–13) | Content review of owner sections |
| OBL-3 | Engine: B's step/item/iteration/attempt identities, producer restriction, checkpoints and `walk.rs` scheduling kept; A's tool ownership settles host work before retry, result selection, checkpoint or resource release, including when a block future is dropped (AC-010, INV-001..003; code-mode AC) | vala-verify unit/integration tests incl. cancel/timeout during host work |
| OBL-4 | One direct/nested dispatcher: name/typed-arg validation, per-call tool/data permission + nonblocking audit, bounded args/results, shared request/concurrency budget, start order, recipient tags, citations, check-Agent records; provider `call_id` and producer forwarded; nested bridge excludes `code.run`, `fs.write`, `publish`; check Agents cannot publish (rev 12) | blocks_agent/tools tests; hosted tools pg_tests; code-mode journey |
| OBL-5 | Tool-call spend restored from attempt records like `max_agent_invocations`; each attempt records its spent count as requests spend and a later attempt, after a refusal or interruption, resumes from the latest persisted count; allowance shared across retries; original deadline kept across park/takeover (rev 12, rev 15) | engine recovery tests; pg_durable_verifier_work takeover |
| OBL-6 | Hosted workspace: A's reusable fresh-session sandbox, runnable-turn bound, charged streamed workspace composed with B's publication/collection mounts, extraction, writes, immutable snapshots; charge before mutation, replacement vs no-overwrite, unchanged on refusal, release after delete/transfer; suspended envelopes charged until reset/reap (REQ-015, code-mode resource reqs) | workspace tests/pg_tests, sandbox_worker, pg_hosted_* |
| OBL-7 | One `wyrd-runtime` memory/scratch ledger covers Bifrost, edge bodies, live queued state, frozen copies, encoding, terminal/outbox custody; reserve before copy; transfers without gap/double count; B's memory-first progress, group commit, ACK-before-retirement, claim backpressure kept; no persistence waits on engine mutation/direct path (REQ-017/018, AC-012/013) | runtime/state unit tests; pg_durable_verifier_work |
| OBL-8 | Direct path: arrival-based deadline lowered by authored timeout through lookup/admission/model; verification-only capture bypass; destination-batched direct evidence; individually fenced queued ACKs; mode-aware target cache + invalidation; queue-only refusal before steps; compact/details; HTTP/MCP-only inspection; RBAC-only authz and `verifier:approve` (REQ-016, AC-011) | pg_verification_routes/runtime, existing selectors |
| OBL-9 | Isolation by deployment target: closed `WYRD_TARGET` set `combined` (default)/`direct`/`worker`/`bifrost-ingest`/`bifrost-query`/`bifrost-maintenance`, old values removed; each target accepts exactly its work and refuses the rest before admission or side effect over HTTP and MCP, never forwarding; readiness follows owners and capabilities; `direct`/`worker` run Scribe+Oracle without Forge, `bifrost-maintenance` both Forge roles; `combined` has one hosted capacity owner charged per use, no reservation, queued concurrency cap; reservation code and percent settings deleted; deadlines, Busy deferral, fenced takeover and drain unchanged; portable only (REQ-015, AC-010, rev 13) | target journeys over HTTP/MCP; pg seam tests; direct c64 on `direct` pod with and without saturated `worker` pods |
| OBL-10 | Dependencies: Monty on crates.io, no `[patch.crates-io]`/git type-checking patch/fork-only `service_mounts`/`resume_serviced`; runnable-turn bound works on crates.io API; B's code-review pin kept; lockfile/workspace-hack/features coherent; benchmark deps diagnostic-only and out of SDK packages (rev 12) | `check:deps`, Cargo.toml inspection, package contents |
| OBL-11 | Projections: authored Python stubs/exports and native sources composed; wire types, errors, loaders/declarations, testing declarations, docs regenerated; historical records decode; six-control guard rejects any extra hook in Rust/Python/TS with blocking waits and production exclusion (REQ-195) | codegen:check, ts:napi:check, testing d.ts inspection, guard tests |
| OBL-12 | Pipeline journeys preserved: connect→Repository→PR admission→pinned extraction→review→approval/deferred→same execution→report/Check/comment and negatives; supersession/access loss fences; provider retry never recomputes (REQ-001–014/016, AC-001–009/011, INV-001–007) | combined server seam selector; SDK journey lanes |
| OBL-13 | Packet carry: A's tracked packet + evidence retained with source identities and carried/changed/pending disposition per inherited row; TASK-003-R2 superseded, R3 open; 100-tenant amendment recorded against REQ-033/AC-034 and R3 V6; no historical FAIL becomes PASS; diagnosis artifacts retained or marked unavailable | content review |
| OBL-14 | Benchmark handoff: immutable candidate full `bench:l1:{verification,gateway,bifrost}` records with identities and every listed diagnostic; misses recorded honestly (task outputs) | benchmark records (maintainer supplies candidate) |

## Evidence

Candidate: branch `task/github-verifier-pipelines/TASK-012-integrate-code-mode`,
commits `ee068f5a2..HEAD` (port, contract reconciliation, revisions 13–15,
and the journey repairs below). Selector prefixes: **E**
`-p vala-verify --lib` `blocks::engine::tests::`; **VI** `-p vala-verify
--test integration` `blocks_agent::`; **SL** `-p wyrd-server --lib`; **SI**
`-p wyrd-server --test integration --run-ignored=all`, run under
`scripts/postgres/with-test-postgres.sh`; **J** `-p wyrd-ai --test integration
-P journey --run-ignored=all`; **T** `-p wyrd-testing --test server -P journey
--run-ignored=all`; **M** `-p wyrd-mcp` verification journeys.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| OBL-1 port integrity, A-only paths carried/adapted/excluded, identities, exclusions, index and TASK-011 untouched | ee068f5a2; `design/code-mode-port.md` | code-mode-port.md §Source identities, §Path coverage, §Excluded | content review; `git status` shows `tasks/README.md` modified and TASK-011 untracked, neither staged in any task commit | PASS |
| OBL-2 owner reconciliation, no OS-isolation/reservation/borrowing claims, target table, kind 13, six-control union | bb78916eb, 5b358f452, 456546fbb, b2afb89a8 | `architecture/wyrd-design.md` Verifier/Agents and capacity paragraphs; `architecture/operations/README.md:52-69`; REQ-195 | content review; `docs:check` | PASS |
| OBL-3 identities, producer restriction, checkpoints, walk scheduling; settlement before retry/result/checkpoint/release, cancel/timeout, dropped future | ee068f5a2 engine/agent/tools | E `array_fanout_freezes_and_orders_items` (tests.rs:3112), `repeat_refines_one_iteration_at_a_time` (:3356), `checkpoints_restore_settled_blocks_and_count_interrupted_attempts` (:2859), `publication_references_resolve_to_attempts_and_statuses` (:2516), `ready_successors_do_not_wait_for_unrelated_steps` (:3026), `capacity_refusal_lets_dispatched_work_settle` (:1994); VI `timed_out_attempt_settles_before_retry_or_result` (blocks_agent.rs:2911), `agent_check_settles_before_result` (:3050); SI `pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation` (:2797), `code_shutdown_settles_pending_host_work` (:2954), `pg_durable_verifier_work::dropped_agent_work_makes_no_later_call` (:2253) | E 82/82, VI 23/23, SI combined seam selector | PASS |
| OBL-4 one dispatcher: validation, tool+data permission, nonblocking audit, bounds, shared budget, start order/recipients, citations, check records, `call_id`/producer, nested exclusions, check cannot publish | ee068f5a2 `hosted/tools.rs`, `blocks/tools.rs` | VI `step_agent_refuses_unknown_tool`, `agent_tool_budget_spans_attempts_without_refund`, `code_bridge_shares_the_recorded_path`, `check_agent_keeps_only_valid_citations`, `agent_records_survive_failed_steps_and_inconclusive_checks`, `nested_calls_refuse_writable_tools_and_model_calls_keep_their_call_id` (:3255); SL `verification::hosted::tools::pg_tests::{tools_require_their_exact_invoke_grant,tool_and_domain_grants_are_both_required,tools_refuse_forged_arguments_and_spent_budgets}`, `verification::hosted::tools::tests::reprs_stop_at_the_limit`; `wyrd-runtime` `outbox::tests::bounded_audit_staging_remains_nonblocking`; SI `pg_hosted_artifacts::{code_bridge_preserves_declared_tool_boundaries,code_calls_share_non_code_concurrency}`; `wyrd-spec` `card::verifier::validate::tests::verifier_spec_admits_every_builtin_step_tool` (check publish refused, validate.rs:2304); J `verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations`; J `hosted_agent_tools::agent_publish_retries_use_host_identity` (producer) | VI, SL, SI, J 7/7 hosted_agent_tools, code-mode selector | PASS |
| OBL-5 spend from attempt records, recorded as spent, resumed after refusal or interruption, shared across retries, deadline kept | 91c48a972, b78edf183 | E `refused_attempts_keep_their_tool_spend` (:1907); `vala-verify` `blocks::tools::tests::resumed_invocation_continues_from_its_recorded_spend`; SI `pg_durable_verifier_work::invocations_resume_from_bifrost` (:2353); SI `pg_deferred_evidence::a_resumed_execution_keeps_its_admission_deadline`; `wyrd-sql` `pg_verifier_executions::reclaims_keep_the_admission_deadline` | E, SI, `test:sql` | PASS |
| OBL-6 sandbox reuse, runnable-turn bound, streamed charged workspace, mounts, extraction, writes charged before mutation, snapshots, replacement/no-overwrite/unchanged on refusal, release after delete/transfer, suspended envelopes | ee068f5a2; 68b4fb9b3 (`fs.write` admits through the scratch ledger) | SI `sandbox_worker::{sandbox_reuses_processes_with_fresh_tenant_sessions,suspended_sessions_release_runnable_capacity,prewarmed_workers_are_charged_and_reaped_under_pressure,failed_workers_settle_before_capacity_returns,workspace_writes_share_one_budget}`; SI `pg_hosted_artifacts::{workspaces_stream_evidence_to_disk,evidence_reads_charge_process_memory_before_reading}`; SI `pg_durable_verifier_work::{publication_mounts_preserve_status_and_lineage,file_and_json_publications_are_immutable}` (no-overwrite); SL `verification::hosted::workspace::tests::{extraction_charges_process_memory_and_scratch,admitted_writes_charge_process_scratch,writes_charge_process_scratch,cancelled_snapshot_keeps_writer_exclusion}`; SL `verification::hosted::workspace::pg_tests::{workspace_cleans_up_on_drop,cancelled_open_returns_its_charges}`; J `hosted_agent_tools::tool_limits_release_capacity_and_preserve_provenance` (replacement fits, refused replacement leaves the old file) | SL workspace 13/13 + pg 5/5, SI seam, J 7/7 | PASS |
| OBL-7 one ledger incl. edge bodies, queued state, frozen copies, outbox custody; reserve before copy; no double count; memory-first, group commit, ACK-before-retirement, claim backpressure; no persistence waits | ee068f5a2, 35fd76607 | `wyrd-runtime` `resources::tests::{process_grants_follow_use_without_double_promises,release_wakes_a_registered_waiter}`; SL `http::middleware::body_limit::tests::generic_bodies_charge_process_memory`; SL `verification::state::tests::{transitions_complete_while_persistence_is_held,held_changes_coalesce_into_the_next_revision}`; SI `pg_verification_runtime::{failed_group_commits_retry_as_groups,a_parked_attempt_keeps_its_claim_until_acknowledged,lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates,held_persistence_bounds_queued_custody_by_claim_permits,sustained_scribe_outage_caps_tenant_spans}`; SI `pg_durable_verifier_work::{state_format_changes_preserve_existing_custody,batched_checkpoints_recover_exactly_the_accepted_batch}` | SL, SI seam | PASS |
| OBL-8 arrival deadline, capture bypass, destination batches, fenced queued ACKs, mode-aware cache, queue-only refusal, compact/details, HTTP/MCP-only inspection, RBAC-only authz, `verifier:approve` | ee068f5a2 | SI `pg_verification_routes::{direct_execution_waits_end_at_the_arrival_deadline,details_preserve_retained_execution_evidence,direct_execution_is_authorized_by_rbac_alone}`; SI `pg_verification_runtime::{target_loading_spends_the_claimed_deadline,queued_origins_require_their_own_input_ack}`; SL `components::gateway::pg_invocation_tests::capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline` (bound call never waits); SL `scribe_outbox::tests::direct_results_deliver_as_destination_batches`; SL `verification::cache::tests::*`; SI `pg_github_pull_request_extraction::direct_verification_of_a_pull_request_verifier_refuses_before_any_step`; SI `pg_approvals::a_service_account_decides_on_a_users_behalf`; inspection is served by the HTTP and MCP routes only (no SDK/CLI inspection added: `git diff ee068f5a2^ -- crates/shared/wyrd-client/src crates/wyrd/wyrd-cli/src sdks/*/src` adds no execution-inspection call) | SL, SI seam, J `verification_capacity::*` | PASS |
| OBL-9 six targets, wrong-target refusal before side effects over HTTP and MCP, readiness, Forge placement, one shared owner in `combined`, reservation deleted, worker completes webhook/deferred/approval journeys, worker crash takeover keeps deadline, direct never claims queued work; direct c64 with and without saturated workers | 5b358f452, 35fd76607, 507dbd866, e8d845674, 5083ffc98, b2afb89a8, 712227050 | SI `pg_verification_routes::{direct_target_runs_only_direct_work,worker_target_runs_only_queued_work}`; SI `pg_router_smoke` target table (:938); T `verification_runtime::isolated_capacity::direct_verification_runs_while_queued_capacity_is_held`; SI `pg_deferred_evidence::*` and `pg_approvals::*` on `worker` via `Deployment::start_limited` (pg_github_pull_requests.rs:112); `wyrd-sql` `reclaims_keep_the_admission_deadline` (claim reads the remaining admission deadline on every target) | SI 40/40 on the three GitHub suites; T pass | **FAIL**: the direct c64 comparison with and without saturated `worker` pods has no harness (`bench:l1:verification` runs one combined server) and no record. Takeover is proven at the claim seam, not by a crashed `worker` pod journey. |
| OBL-10 crates.io Monty, A's fork patches excluded, no fork-only sandbox split, bound on crates.io API, pins, features, benchmark deps diagnostic-only | ee068f5a2, dfa919edc | `Cargo.toml:150-152`; the remaining `monty-type-checking` path stand-in predates the merge base (`0e886ceec`) and is not A's patch; no `service_mounts`/`resume_serviced`; `scripts/checks/deps.sh` | `check:deps`, `check`, `lints` | PASS |
| OBL-11 projections regenerated, historical decode, six-control guard in Rust/Python/TS with blocking waits and production exclusion | 2aecc1fee | `wyrd-testing` `server::tests::public_controls_are_the_six_sanctioned_operations` (server.rs:6584); J `verification_capacity::{verify_waits_for_hosted_memory_then_passes,submit_defers_under_hosted_memory_then_passes}`; SI `pg_durable_verifier_work::state_format_changes_preserve_existing_custody`; `sdks/wyrd-sdk-ts/testing/native.d.cts` inspected | `codegen:check`, `ts:napi:check`, `check:deps` | PASS |
| OBL-12 pipeline journeys and negatives, supersession/access fences, provider retry | preserved; 9edfa4fa7 etc. repair stale journey expectations only | J `github_connection::connections_associate_only_after_github_confirms_them`, `repository_declaration::pull_requests_admit_pinned_executions_and_supersede_them`; SI `pg_github_pull_requests::*`, `pg_github_pull_request_extraction::*`, `pg_deferred_evidence::*`, `pg_approvals::*`, `pg_github_delivery::*` | SI combined seam selector, `test:rust-sdk:journey` | PASS |
| OBL-13 inherited packet carried with dispositions, R2 superseded, R3 open, 100-tenant amendment, no FAIL made PASS | c33e9a137 | code-mode-port.md §Inherited packet, §100-tenant amendment, §TASK-003-R3 obligations | content review | PASS |
| OBL-14 immutable candidate `bench:l1:{verification,gateway,bifrost}` records | none | none | not run: needs the maintainer's immutable candidate on a quiet host | **FAIL** |

### Diagnoses

- **Scribe outage journey.** The recovered-span proof needed a Verifier
  execute on a query-only pod. Fix: a traced card read on the query pod
  (e9cd7e78c), approved.
- **Capacity journey submit hang.** Cause: since 35fd76607 Python and Scribe
  share process memory; holding all of it left the queued submission unable to
  stage. Fix: the queued hold leaves 64 MiB (9edfa4fa7). The server already
  ends the wait at the 30 s edge timeout with a stable error.
- **Capacity journey scratch deferral.** Cause: `Workspace::write` checked
  only its own `/work` budget and never the process scratch ledger, so a full
  ledger neither deferred queued work nor refused direct work. Fix: admit each
  write through `admit_write`, a full ledger stopping busy (68b4fb9b3). RED:
  `verification::hosted::workspace::tests::writes_charge_process_scratch`
  failed with "the written bytes are charged" before the fix. The journey then
  counts the deferred attempt's model call (0c4a767e8).
- **Fan-out journey.** Cause: it asserted an item-scoped hold from the removed
  workspace bound; a model-call permit is per call. Fix: assert at most two
  calls in flight and that the limit is reached (ea34e9566).
- **Limits journey.** Cause: the 64 KiB tool-argument cap refused the large
  writes before the publication and `/work` limits. Fix: the journey's
  server allows arguments up to twice its `/work` size (f456470aa).
- A suspected wrong error code (`SKALD_TOOL_422_INPUT` on direct calls) is the
  documented direct-call convention; no change.

Each diagnosis that touched a test had a read-only diagnostician's report
before the fix.

### Process exceptions

Revision 13–15 production changes in 5b358f452, 35fd76607, 91c48a972,
b2afb89a8 and b78edf183 were written before their changed tests failed; their
proof is the changed tests listed above, not RED.

### Status

`BLOCKED` on OBL-9 (direct c64 comparison against saturated `worker` pods,
and a crashed-`worker` takeover journey) and OBL-14 (immutable benchmark
records). Both need maintainer input: a multi-pod benchmark harness or manual
cluster run, and an immutable candidate.
