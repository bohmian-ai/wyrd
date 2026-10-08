# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review material consulted: `review/TASK-008-r1/` and
  `review/TASK-008-r2/`
- Governing maintainer authority: `AGENTS.md` §§5, 6, 11, and 16;
  `architecture/agent-rules.md`;
  `architecture/references/languages/{maintainer-style,rust-core,testing-workflows,spec-driven-development}.md`;
  the revision-57 verification-capacity contract; and the applicable Wyrd and
  Bifrost lifecycle authority.

The candidate commit matched the requested identity before and after source
inspection. CodeGraph is absent, so symbol and caller tracing used Git and
repository source directly. This report changes no reviewed production or test
source.

## Changed-surface coverage

| Surface | Material symbols, owners, callers, and tests followed | Maintainer assessment |
|---|---|---|
| Command and target ownership | `mise.toml`'s `bench:capacity`; the `capacity` Cargo target; removal of `verification_capacity`, `bifrost_ingest_capacity`, and `bifrost_query_capacity`; `QueueConfig::default`'s renamed evidence reference | Operators now have one discoverable server-capacity entry point and one binary. The manifest and task names agree. The command-level lifetime claim is not owned at the command boundary (MNT-R3-001). |
| Benchmark lifecycle | `main.rs::{Cli,Lifetime,Benchmark,main}` through `Benchmark::{prepare,run,measure,provision,scale_out,reconnect,step,clean_up}`, `LocalServer::{start,start_replica,stop,Drop}`, and `Report::write_to` | `Benchmark` is a meaningful concrete owner for setup-to-report state, and `main` is thin. Prior r1 lifecycle-ownership drift is closed. The deadline abstraction cannot enforce the lifecycle its docs and command promise (MNT-R3-001), and its shared cancellation operation remains undocumented (MNT-R3-002). |
| Step and deployment model | `step.rs::{Deployment,StepKind,Plan,ResourceWindow,Drain,Record}` through the sequence producer in `Benchmark::measure`, the load driver, profile naming, report verdict selection, and the drain/resource tests | `StepKind` removes the prior stringly step identity, `Deployment` owns measured execution, and resource/drain rules sit beside their consumers. The independently writable `Plan` booleans are not a finding: the only production producer constructs consistent values, matching r2 validation. |
| Load and client lifecycle | `load.rs::{Op,Work,Lane,Tally,TenantClients,Request,Outcome,mix,query}` through each `Deployment::run` lane, public SDK handles, queue flush/shutdown, and mix/query tests | Closed behavioral sets are enums, stateful client behavior is on `TenantClients`, and request/lane cancellation effects are documented. The parallel private client vectors are populated and indexed together by the sole owner; replacing them would be preference without a reachable maintenance failure. |
| Workload fixtures | `fixture.rs::{Kind,Tenant,context,value,await_fit,write_baseline,write_graph,judge_prompt}` through registration, hydration, baseline fitting, seeded evidence, `TenantClients::connect`, and direct/queued request construction | Workload identity and construction are local to the fixture owner. Public-client setup, durable partial progress, and retry meaning are documented at the async boundaries. No generated declaration applies. |
| Judge and profiling fixtures | `judge.rs::{Judge,TlsListener}` and `profile.rs::{Profile,Capture}` through server environment construction, per-step wait collection, capture start/finish, and `Drop` cleanup | Both modules have cohesive owners and keep provider wait distinct from engine overhead. Their names and stored fields expose the evidence they own; no second telemetry or process framework was introduced. |
| Evidence and report | `evidence.rs::{Percentiles,Overhead,Scrapes,scribe_backlog,RunTally,Backlog,Queue}` and `report.rs::{Cell,Row,step_row,op_row,Report,table_row,result}` through Prometheus scrapes, cross-tenant benchmark SQL, Markdown/JSON output, verdict selection, and all focused unit tests | Measurement transformations are small and local; one table uses stable cell meanings; missing/non-applicable evidence is explicit. The Scribe backlog includes staged live members and the judge wait is rendered separately. `Metrics::parse` is public only to the owning harness's focused consumer test; that visibility is not material drift. |
| Release-server harness | `release_server.rs::{LocalServer,STOP_GRACE,MemoryPeak,Metrics,run}` through first-replica provisioning, joined replicas, cgroup evidence, scrape parsing, cleanup, and harness tests | Process ownership remains on `LocalServer`, and exported `STOP_GRACE` is the bound used by `stop`. The synchronous `run` helper's unbounded `Command::output` is reachable from the new benchmark deadline path and is part of MNT-R3-001. |
| Correctness tests moved out of capacity judgment | `pg_verification_runtime.rs::{two_replicas_claim_each_queued_run_exactly_once,a_flooding_tenant_does_not_delay_another_tenants_run}` and their `Harness`, scripted engine, lease, and durable-row helpers | These are appropriately external Postgres integration tests: they compose multiple runtime owners and inspect state not observable through a public process journey. Names, setup, assertions, and panic contracts describe the outcomes. |
| Rust SDK journey proof | `drift_verification.rs::{REJECTED_ANSWER,start_direct_judge,DirectJourney::assert_judgments}` and `observe_run.rs::{emit_with_resubmit,sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes}` plus the queue-size constant conversions | The tests drive real public SDK/server paths and assert failed LLM judgment, exactly-once 100-feature grouping, and bounded client-owned bytes. The changed helpers and tests have substantive rustdoc and precise outcome names. |
| Change packet and evidence | revision-57 `spec.md`, the current task record, r1 verdict/remediation, and the r2 reports whose final verdict was blocked only by its erroneous revision-58 subject | Current task metadata and candidate authority agree on revision 57. Prior r1 findings 1, 3–5, and 7–12 remain visibly closed in this source. Prior findings 2 and 6 remain source-valid as described below. Caller-directed FIND-13 is deferred to post-integration qualification and is not used as a blocker here. |

## Material findings

### MNT-R3-001 — the documented complete-command lifetime has no owner that can enforce it

- **Changed location:** `mise.toml:507-527` and
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:25-27,79-88,124-188,227-245,311-318,359-418,621-635`.
  The reachable process wait is
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,618-635`.
- **Governing principle:** `maintainer-style.md` requires the workflow's owner,
  names, documentation, and behavior to describe the same operation;
  `AGENTS.md` §5 requires a stateful multi-step lifecycle to have one concrete
  owner; `AGENTS.md` §16 requires documentation to state the real invariants and
  side effects a maintainer must preserve. Revision-57 REQ-171 defines one
  default command bounded to 30 minutes including setup.
- **Evidence:** the mise entry point starts RustFS and builds release binaries
  before `capacity::main` exists. Inside the binary, `Lifetime` starts in
  `Benchmark::prepare`, but `prepare` itself is awaited outside
  `Lifetime::measure`. Provisioning then calls `LocalServer::start`, whose
  mandatory `migrate` and per-tenant `setup` paths call synchronous
  `Command::output()`. A Tokio `timeout_at` cannot cancel a future while that
  poll is blocked in `output()`. Nevertheless, the module, constant, lifetime
  type, and task description all tell a maintainer that the whole command from
  setup through cleanup is bounded.
- **Concrete maintenance cost:** there are three apparent lifecycle boundaries
  (mise wrapper, `Benchmark::prepare`, and `Lifetime::measure`) but none owns
  the promised bound end to end. A maintainer can add setup work or rely on the
  documented guarantee while a wedged migration/setup child prevents timeout,
  report generation, and normal cleanup indefinitely. The existing paused-time
  test reinforces the wrong model because it proves only a cooperatively
  yielding future.
- **Smallest testable correction:** keep the existing benchmark lifecycle and
  process owners, but place the one absolute deadline at the outermost owner of
  all work counted by REQ-171 and make migration/setup child waits participate
  in that owned deadline, including termination and reaping on expiry. Preserve
  the current report and `LocalServer` cleanup behavior. Add a process-level
  test with a deliberately stalled setup child that proves bounded exit,
  retained diagnostics, failure reporting, and cleanup. Do not add another
  timeout knob or a second process abstraction.
- **Prior-review continuity:** this is the maintainer consequence of prior
  `FIND-TASK-008-CLOSEOUT-2`, which r2 independently found still open against
  revision 57.

### MNT-R3-002 — the shared cancellation boundary omits its cancellation contract

- **Changed location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`
  (`Lifetime::bounded`), called by `Lifetime::{measure,shut_down}`.
- **Governing principle:** `AGENTS.md` §16 and `architecture/agent-rules.md`
  make cancellation and partial-progress rustdoc mandatory for materially
  changed async Rust boundaries; `maintainer-style.md` requires the owner of an
  operation to expose its failure and partial-progress behavior.
- **Evidence:** `bounded` owns the `timeout_at` call and therefore drops any
  supplied workflow at its next cooperative yield. Its rustdoc says only that
  it runs work until a deadline and names the phase in a failure. The two
  current callers document selected consequences, but the reusable operation
  that performs cancellation does not say that already-produced effects remain
  under the supplied future's owner.
- **Concrete maintenance cost:** a new caller must reverse-engineer
  `timeout_at` and sibling caller comments to learn whether expiry rolls back,
  drops, or preserves partial work. That ambiguity sits at the exact shared
  boundary introduced to make setup and cleanup safe.
- **Smallest testable correction:** document on `Lifetime::bounded` that
  deadline expiry drops the supplied future at its next cooperative yield and
  that effects already produced remain according to that future's owner. Keep
  the more specific measurement and client-shutdown contracts on their current
  methods. Add no wrapper, trait, or duplicate cancellation mechanism. A
  complete async-item rustdoc audit plus the existing focused lifetime test is
  sufficient proof.
- **Prior-review continuity:** this is the documentation portion of prior
  `FIND-TASK-008-CLOSEOUT-6`; r2 validation rejected the unrelated configurable
  diagnostic preference but confirmed this missing contract.

## Uncertain preferences for calibration

- `Plan::{judged,sample_floor}` duplicate facts currently derivable from the
  fixed sequence, `StepKind`, and replica count. The only production producer
  constructs valid combinations, so deleting the booleans would be a possible
  simplification, not an acceptance finding. This preserves r2 validation's
  rejection of that proposal.
- `Benchmark` and `Report::render` are large, but their methods share one
  cohesive lifecycle or one ordered artifact. Splitting either solely for line
  count would make the workflow harder to follow.
- `TenantClients` stores three parallel private vectors. Its sole constructor
  fills them together for the same URLs and all consumers use the same replica
  index; a per-replica wrapper could be equally clear, but no current reachable
  mismatch justifies churn.
- `Metrics::parse` is public for a focused scrape-consumer test. In this
  test-harness crate, narrowing it would not materially improve safety or
  discoverability.

## Verification reviewed and limits

- The task/remediation evidence records 14 passing `capacity` binary tests, 58
  passing `wyrd-testing` library tests under the repository Postgres wrapper,
  passing focused two-replica claim/fairness tests, passing Rust SDK judgment
  and sustained-ingest journeys, clean formatting, workspace lints, focused
  Clippy, and a reduced two-replica smoke. This review inspected the cited test
  bodies and their production/harness callers rather than rerunning Cargo in
  the shared checkout.
- `git diff --check` is clean for the immutable base-to-candidate range.
- The reduced smoke is useful lifecycle/report evidence but is intentionally
  not the default capacity qualification.
- Per caller instruction, `FIND-TASK-008-CLOSEOUT-13` (one unmodified default
  `mise run bench:capacity`) is sequenced after other workstreams integrate. It
  is recorded as deferred and does not block this candidate review.
- No Python, TypeScript, OpenAPI, schema, stub, or other generated declaration
  changed in this task range.

## Overall result

**FAIL**

Coverage is complete, authority revision 57 is present, and the candidate
remained immutable. The code is substantially easier to navigate than the r1
subject, but MNT-R3-001 leaves the documented complete-command lifecycle
without an enforceable owner, and MNT-R3-002 violates the repository's hard
async-cancellation documentation rule. Both corrections are bounded within the
approved task and existing owners.
