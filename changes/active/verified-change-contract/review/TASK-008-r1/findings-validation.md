# Findings validation: TASK-008 closeout revision 57

## Immutable subject and validation scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Cumulative diff: `ce5c09ef3..852894689`
- Approved authority: `changes/active/verified-change-contract/spec.md` revision 57, especially REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, section `bench:capacity implementation (revision 57)`

I read the complete cumulative diff, every discovery report, the focused
follow-up, the applicable repository and architecture authority, and the full
bodies and callers of the proposed correction sites. The traced runtime path
was `mise run bench:capacity` -> `main::benchmark` -> `Deployment::run` ->
`Lane::drive`/public clients -> `Queue` and metrics evidence -> report cells and
the process exit code. Sibling consumers included the still-runnable Bifrost
query benchmark, Scribe's production backlog metric owners, the moved
correctness/fairness journeys, and the task record consumed by later Wyrd
implementation and review skills.

The candidate remained at the named commit throughout validation. The retained
corrections are private benchmark, test, task-record, or benchmark-command
work. None requires a new product, public API, security, compatibility,
cross-service, concurrency, resource-ownership, or persistent-data decision.

## Proposal dispositions

| Proposal | Disposition | Source validation |
|---|---|---|
| BHV-001 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-1 | `mise.toml` still exposes `bench:bifrost:query-capacity`; its Cargo target and release-server-driving source remain. This directly contradicts REQ-171 and the revision 57 history. |
| BHV-002 | **REVISED** into FIND-TASK-008-CLOSEOUT-2 | The defect is the missing benchmark-lifetime bound, not specifically `Report::passed`: the report verdict is defined by the three verdict steps, while the command itself must complete inside 30 minutes. A report-only duration guard would still allow setup or a request tail to hang. |
| BHV-003 | **REJECTED** | At `K = 50`, the specified mix offers 900 direct samples per kind in 180 seconds. REQ-171 and AC-040 do not promise that every possible knee passes; they require the benchmark to fail when the 1,000-sample evidence floor is absent. Oversampling, extending the step, raising the lowest knee, or lowering the floor would change the approved spec. |
| BHV-004 | **REVISED** into FIND-TASK-008-CLOSEOUT-13 | The reduced, intentionally failing smoke proves plumbing only. It does not prove default-duration, AC-040, or the AC-041 `L = 200` rate. This is one verification-evidence gap, deduplicated with CAP-005. |
| INV-001 | **CONFIRMED**, duplicate of FIND-TASK-008-CLOSEOUT-1 | Same reachable second server-capacity entry point as BHV-001/CAP-003. |
| INV-002 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-3 | The benchmark collects only `wyrd_verification_engine_overhead_seconds` plus a judge call count. No `Record`, JSON field, Markdown cell, or benchmark-owned provider-duration evidence reports the judge provider wait separately as AC-040 requires. |
| INV-003 | **REVISED**, duplicate of FIND-TASK-008-CLOSEOUT-2 | Sequential provisioning can consume multiple five-minute waits and each lane awaits outstanding requests after arrivals stop. The benchmark records elapsed time but owns no absolute lifetime deadline. |
| STD-001 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-4 | The changed task appends a revision 57 implementation and `IMPLEMENTED`, while its operative frontmatter remains `status: proposed`, `spec_revision: 49`, and `planning_result: SPEC_REVISION_REQUIRED`, without REQ-171/AC-040 in its mapping. The repository's task lifecycle makes that current contract authoritative, not harmless historical prose. |
| STD-002 | **REVISED** and consolidated with MR-001 as FIND-TASK-008-CLOSEOUT-5 | `benchmark` is a dependency-backed multi-step lifecycle with no concrete owner, while client reconnection mutates all of `Deployment` through a free function. One owner correction closes both manifestations; two nested wrapper findings would prescribe needless structure. |
| STD-003 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-6 | Material async workflows document errors but omit relevant cancellation and partial-progress behavior despite starting processes, registering durable Cards, emitting observations, spawning requests, flushing queues, and collecting partial evidence. AGENTS.md section 16 and `rust-core.md` make this a hard requirement. |
| MR-001 | **REVISED**, consolidated into FIND-TASK-008-CLOSEOUT-5 | `connect` drains and rebuilds `Deployment::clients` from `Deployment::replicas` and `Deployment::tenants`; it is state-owning orchestration, not a stateless helper. |
| MR-002 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-7 | The four closed step kinds are stored as arbitrary strings and verdict selection relies on exact string equality. This leaves a verdict-critical invalid state representable despite the repository's explicit enum rule. |
| SYS-001 | **REVISED** as FIND-TASK-008-CLOSEOUT-8 | The current Scribe value omits durable staged members, so it can become zero before publication completes. The minimum correction does not need a new ledger or to double-count outstanding claims: production's existing `bifrost_scribe_staging_live_members` already includes ready and claimed unpublished members. |
| SYS-002 | **REVISED**, duplicate of FIND-TASK-008-CLOSEOUT-2 | Same missing command-lifetime boundary as BHV-002/INV-003. Production request deadlines are sibling behavior and must remain unchanged. |
| CAP-001 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-9 | `drain` checks emptiness before checking the elapsed limit, and `backlog` passes every `Some(seconds)`. Because lane completion and flush occur before the first poll while the clock starts at nominal arrival stop, the first empty observation can be later than 60 seconds and still pass. |
| CAP-002 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-10 | The rendered Markdown closes a `Steps` table and opens a differently headed `Per operation` table. REQ-171 expressly requires one table, step rows followed by operation rows with the same columns. |
| CAP-003 | **CONFIRMED**, duplicate of FIND-TASK-008-CLOSEOUT-1 | Same legacy Bifrost server-capacity task, Cargo target, and binary as BHV-001/INV-001. |
| CAP-004 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-11 | CPU is sampled after request/client-drain tails but divided by only planned arrival seconds; memory is read later still, after backlog drain. The reported values therefore cover different intervals and the CPU average has the wrong denominator for its sampled interval. |
| CAP-005 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-13 | No unmodified default execution or passing default report is recorded. Unit tests and the shortened smoke cannot establish release-server capacity, the AC-040 sample/SLO result, AC-041 at `L = 200`, or completion within 30 minutes. |
| DUR-001 | **REVISED** into FIND-TASK-008-CLOSEOUT-12 | REQ-171 explicitly counts wrong judgments and lost requests as errors, so those portions are rejected. `duplicate run` is not in the Errors SLI and is the exactly-once judgment revision 57 moved to tests; only that portion remains. |
| FU-001 | **CONFIRMED** as FIND-TASK-008-CLOSEOUT-12 | `Deployment::ops` creates a benchmark-failing `duplicate run` error and the report advertises it, while the approved cross-replica integration test owns exactly-once proof. |

## Final deduplicated finding ledger

### FIND-TASK-008-CLOSEOUT-1 — CONFIRMED — VIOLATION

- **Source IDs:** BHV-001, INV-001, CAP-003
- **Violated obligation:** REQ-171 makes `bench:capacity` Wyrd's one server
  capacity benchmark, and revision 57 says the former Bifrost query-capacity
  entry point is already deleted.
- **Exact location:** `mise.toml:507-509`;
  `crates/wyrd/wyrd-testing/Cargo.toml:145-151`;
  `crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity/main.rs:1-14`.
- **Source evidence:** the mise task builds release `wyrd-server`, invokes the
  registered `bifrost_query_capacity` binary, applies judged capacity rows, and
  fails on them. It is a real, reachable server-capacity benchmark, not an
  external storage-engine comparison.
- **Observable consequence:** operators retain two authoritative-looking
  server-capacity commands with incompatible workloads and verdicts.
- **Decision-complete minimum correction:** delete the stale mise task, its
  Cargo binary registration, and the owned `bifrost_query_capacity` source.
  Preserve separately named ClickBench and observability engine-comparison
  tooling, which revision 57 expressly excludes from consolidation.
- **Focused closure proof:** repository search and task listing show
  `bench:capacity`/the `capacity` binary as the only release-server capacity
  entry point; manifest parsing and the targeted `wyrd-testing` lane remain
  green.

### FIND-TASK-008-CLOSEOUT-2 — REVISED — MISSING

- **Source IDs:** BHV-002, INV-003, SYS-002
- **Violated obligation:** REQ-171 requires the default command, including
  benchmark setup, to complete within 30 minutes.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:113-279`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/fixture.rs:57-58,209-292`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/load.rs:249-307`.
- **Source evidence:** `started` is reporting-only. Four tenants provision
  sequentially; each can enter multiple five-minute waits, and every lane
  awaits all issued requests after its arrival window. No absolute deadline
  stops setup, later steps, or cleanup at the approved boundary.
- **Observable consequence:** a slow dependency or request can keep the
  operator command alive past 30 minutes instead of returning a bounded failed
  result and cleaning up the release replicas.
- **Decision-complete minimum correction:** at the benchmark lifecycle owner,
  enforce one absolute 30-minute deadline beginning before server and tenant
  setup and covering measured steps plus bounded client/replica cleanup. On
  expiry, stop later work, return failure, retain diagnostics, and rely on the
  existing owned process/client cleanup. Do not alter production request
  deadlines or make elapsed duration an additional step SLI.
- **Focused closure proof:** a controllable shortened lifetime test forces a
  non-completing setup or step and proves bounded failure and cleanup; the
  unmodified default run required by FIND-TASK-008-CLOSEOUT-13 finishes within
  30 minutes.

### FIND-TASK-008-CLOSEOUT-3 — CONFIRMED — MISSING

- **Source IDs:** INV-002
- **Violated obligation:** AC-040 requires judge cases to report engine
  overhead and provider waits separately, with no provider-wait threshold.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:62-86`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:157,220`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:184-210,445-448`.
- **Source evidence:** the evidence model contains only the engine-overhead
  histogram and a completion count. `judge_calls` is not a duration, and no
  Markdown or JSON field carries provider-wait evidence.
- **Observable consequence:** the report cannot separate the fixed-delay
  provider portion from Wyrd's local judge overhead, omitting half of the
  required judge diagnostic.
- **Decision-complete minimum correction:** reuse the benchmark-owned local
  TLS `Judge` as the measurement owner: retain per-step provider response-wait
  evidence and render it beside, but separately from, judge engine overhead as
  reported-only data. Do not add a production metric or threshold.
- **Focused closure proof:** an evidence/report test with known judge delay and
  calls shows distinct provider-wait and engine-overhead fields in JSON and
  Markdown, and missing provider-wait evidence cannot silently render as the
  required diagnostic.

### FIND-TASK-008-CLOSEOUT-4 — CONFIRMED — VIOLATION

- **Source IDs:** STD-001
- **Violated obligation:** the spec-driven task contract must identify the
  approved revision, mapped obligations, and current lifecycle status.
- **Exact location:**
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:1-10`,
  conflicting with the candidate-added revision 57 record at `:1485-1571`.
- **Source evidence:** current frontmatter still says revision 49, `proposed`,
  and `SPEC_REVISION_REQUIRED` while the body records implemented revision 57
  work and omits REQ-171/AC-040 from the operative mapping.
- **Observable consequence:** later implementation and review automation reads
  an obsolete authority/lifecycle contract for the task now under review.
- **Decision-complete minimum correction:** update the task frontmatter to
  `status: review`, revision 57, and an explicit REQ-171/AC-040/AC-041 mapping;
  remove the obsolete planning-result blocker while retaining earlier history
  in the body.
- **Focused closure proof:** static frontmatter inspection shows one
  unambiguous revision 57 review contract; `git diff --check` passes.

### FIND-TASK-008-CLOSEOUT-5 — REVISED — VIOLATION

- **Source IDs:** STD-002, MR-001
- **Violated obligation:** AGENTS.md section 5 and `architecture/agent-rules.md`
  require dependency-backed, multi-step Rust workflows and operations over an
  owner's state to be inherent methods on a meaningful concrete owner.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:113-279,289-300`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:33-50`.
- **Source evidence:** free `benchmark` owns the entire setup-to-report
  lifecycle, and free `connect` drains and rebuilds `Deployment::clients` from
  its replicas and tenants. The adjacent measured-step lifecycle already lives
  on `Deployment`.
- **Observable consequence:** lifecycle and client/replica alignment
  invariants are split across module functions, so maintainers cannot discover
  all valid deployment transitions from their owner.
- **Decision-complete minimum correction:** create one meaningful concrete
  benchmark lifecycle owner for the configuration and setup-to-report state,
  keep measured-step behavior on the existing `Deployment`, and make client
  reconnection an inherent operation on the concrete owner that holds the
  deployment/authentication pacing state. Do not introduce a trait, generic
  layer, or zero-sized utility wrapper.
- **Focused closure proof:** source inspection shows a thin process `main`, one
  discoverable lifecycle owner, and no free helper mutating `Deployment`;
  capacity unit tests and the reduced two-replica smoke path pass.

### FIND-TASK-008-CLOSEOUT-6 — CONFIRMED — VIOLATION

- **Source IDs:** STD-003
- **Violated obligation:** AGENTS.md section 16 and
  `architecture/references/languages/rust-core.md` require relevant
  cancellation and partial-progress contracts on new or materially modified
  async/durable Rust operations.
- **Exact location:** representative material boundaries are
  `capacity/main.rs:106-113`, `capacity/fixture.rs:203-209`,
  `capacity/load.rs:240-249`, and `capacity/step.rs:138-142`; their owned
  shutdown/reconnection boundaries are part of the same audit.
- **Source evidence:** the docs describe returned errors but not what happens
  if cancellation occurs after processes start, Cards register, observations
  emit, request tasks reach the server, queues admit data, or partial evidence
  is collected.
- **Observable consequence:** a maintainer cannot determine surviving durable
  effects, cleanup ownership, retry safety, or whether partial step evidence is
  usable.
- **Decision-complete minimum correction:** add substantive cancellation and
  partial-progress rustdoc to every changed async workflow where effects can
  precede cancellation, including the cited lifecycle, provisioning, driver,
  deployment, reconnection, flush, and shutdown boundaries. State owned cleanup,
  possible durable residue, retry meaning, and whether partial evidence is
  discarded; do not change runtime behavior solely to satisfy documentation.
- **Focused closure proof:** a source audit finds the required sections on all
  applicable changed async boundaries; rustdoc/Clippy and capacity tests pass.

### FIND-TASK-008-CLOSEOUT-7 — CONFIRMED — VIOLATION

- **Source IDs:** MR-002
- **Violated obligation:** AGENTS.md requires enums for closed sets where
  exhaustiveness matters.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:55-66`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:372-385`;
  constructors at `capacity/main.rs:188-245`.
- **Source evidence:** `Plan::name` accepts arbitrary strings while report
  verdict selection requires exact `"sustained"` and `"scale-out"` literals.
- **Observable consequence:** a typo or rename compiles, runs load, and then
  appears as missing verdict evidence.
- **Decision-complete minimum correction:** replace the string with one private
  exhaustive four-variant step-kind enum, derive its serialized/rendered label
  in one place, and match variants for verdict selection and profile naming.
- **Focused closure proof:** the verdict test constructs only valid variants,
  proves all three verdict identities are found, and report/profile labels stay
  unchanged.

### FIND-TASK-008-CLOSEOUT-8 — REVISED — INCORRECT

- **Source IDs:** SYS-001
- **Violated obligation:** REQ-171 requires the Scribe backlog to drain within
  60 seconds before the saturation cell passes.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:89-99`, consumed by
  `capacity/step.rs:244-258`; production owner
  `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:710-741`.
- **Source evidence:** the benchmark sums only persistence queue depth and
  immutable generation count. After persistence transfers work into durable
  staging, both can be zero while
  `bifrost_scribe_staging_live_members` remains nonzero. The production
  `StagingBacklog` defines live members as all ready or claimed unpublished
  members, so a separate claims addition is unnecessary for emptiness.
- **Observable consequence:** a step can pass and the benchmark can stop while
  Scribe still owns unpublished durable work.
- **Decision-complete minimum correction:** include the existing staging live-
  member gauge in the benchmark's Scribe zero-backlog decision, alongside the
  existing persistence/immutable signals. Reuse production telemetry; do not
  add a second ledger or sum the subset claim gauge as a separate obligation.
- **Focused closure proof:** an evidence test keeps the Scribe cell failing when
  persistence and immutable values are zero but one staged live member remains,
  then passes when it clears.

### FIND-TASK-008-CLOSEOUT-9 — CONFIRMED — INCORRECT

- **Source IDs:** CAP-001
- **Violated obligation:** REQ-171 requires every server backlog to drain
  within 60 seconds after load stops.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:173-195,225-262`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:221-237`.
- **Source evidence:** `drain` returns `Some(elapsed)` for an empty first poll
  before comparing elapsed with `DRAIN_LIMIT`; the report treats every `Some`
  as PASS. The first poll occurs only after lane request tails and client
  flushes, although elapsed starts at nominal arrival stop.
- **Observable consequence:** a backlog first observed empty after 60 seconds
  can select a false knee or permit a false final PASS.
- **Decision-complete minimum correction:** make the drain result pass only
  when emptiness is observed at or before the existing deadline, and ensure
  report judgment independently preserves that boundary. Reuse
  `DRAIN_LIMIT`; add no retry or grace knob.
- **Focused closure proof:** focused tests cover empty observation below, at,
  and above 60 seconds; only the above-limit case fails.

### FIND-TASK-008-CLOSEOUT-10 — CONFIRMED — INCORRECT

- **Source IDs:** CAP-002
- **Violated obligation:** REQ-171 requires one table whose step rows are
  followed by operation rows with the same SLI columns.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:417-477`.
- **Source evidence:** `Report::render` emits separate `Steps` and
  `Per operation` tables with different headers and meanings (`backlogs
  drained` versus `backlog left`, `CPU / memory` versus `driver`).
- **Observable consequence:** the human artifact lacks the approved single,
  directly comparable diagnostic surface.
- **Decision-complete minimum correction:** render aggregate and operation rows
  consecutively beneath one shared SLI schema, using an explicit non-applicable
  representation only where an operation has no value. Preserve the existing
  step verdict and JSON evidence.
- **Focused closure proof:** a render test proves exactly one table header,
  equal column counts and meanings, and each step immediately followed by its
  operation rows.

### FIND-TASK-008-CLOSEOUT-11 — CONFIRMED — INCORRECT

- **Source IDs:** CAP-004
- **Violated obligation:** REQ-171 requires per-step CPU cores and peak memory
  as meaningful, comparable saturation evidence.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:89-98,143-205`.
- **Source evidence:** CPU accumulation continues until all lane futures and
  client flushes finish but is divided by only planned arrival-window seconds;
  memory continues until after the server backlog drain. The two signals
  therefore cover different windows, and the CPU denominator excludes part of
  its numerator interval.
- **Observable consequence:** reported cores can be overstated and cannot be
  compared consistently with peak memory or between steps.
- **Decision-complete minimum correction:** define one explicit resource
  measurement interval for the step, sample both signals over it, and divide
  CPU delta by that interval's measured duration. Preserve the existing
  cgroup envelope and reported-only status.
- **Focused closure proof:** a pure resource-window test proves the CPU
  denominator and both sampling boundaries use the same interval; the report
  keeps per-replica cores and memory.

### FIND-TASK-008-CLOSEOUT-12 — CONFIRMED — DRIFT

- **Source IDs:** DUR-001, FU-001
- **Violated obligation:** REQ-171 limits benchmark judgment to its stated
  SLOs and assigns exactly-once queued claims across replicas to tests.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:310-312`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:4-8,447`.
- **Source evidence:** `runs.created > tally.accepted` becomes a benchmark-
  failing `duplicate run` error and is advertised in the rendered SLO contract.
  Duplication is not named by the REQ-171 Errors SLI; the dedicated
  `two_replicas_claim_each_queued_run_exactly_once` test owns that proof.
- **Observable consequence:** capacity can fail on an extra exactly-once
  assertion, duplicating test ownership and obscuring the golden-signal result.
- **Decision-complete minimum correction:** delete duplicate-run production and
  advertising from the benchmark. Preserve durable-run reads used for queued
  traffic/backlog and preserve refusals, failures, lost requests, wrong direct
  judgments, and terminal failed-run statuses, which REQ-171 explicitly treats
  as errors.
- **Focused closure proof:** a step/report test shows no duplicate category in
  benchmark judgment while every approved error category still fails; the
  existing cross-replica exactly-once integration test remains green.

### FIND-TASK-008-CLOSEOUT-13 — CONFIRMED — MISSING

- **Source IDs:** BHV-004, CAP-005
- **Violated obligation:** acceptance needs credible measured evidence for the
  default REQ-171 command, AC-040, AC-041 at `L = 200`, and the 30-minute
  completion claim.
- **Exact location:**
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:1493-1501,1535-1563`.
- **Source evidence:** the only recorded run uses `--levels 20` and 5/10/15-
  second windows, exits 1, yields 32 samples per kind, and never offers 500
  observations/s. Unit tests validate construction and arithmetic, not actual
  release-server capacity.
- **Observable consequence:** the task claims PASS for empirical acceptance
  criteria without evidence that the prescribed deployment reaches the rate,
  sample floor, SLOs, final verdict, or time budget.
- **Decision-complete minimum correction:** after the bounded source findings
  above are corrected, run the unmodified `mise run bench:capacity` in the
  prescribed release/Postgres/RustFS/8-CPU/16-GiB envelope and append its full
  report and wall time to the task evidence. It must reach the `L = 200`
  workload, show the AC-040 sample and latency evidence, show AC-041 ingest
  refusal/drain evidence, pass all three verdict steps, and finish within 30
  minutes. A failing run requires root-cause correction or an approved spec
  revision; another shortened substitute does not close this finding.
- **Focused closure proof:** the recorded exact command, complete Markdown/JSON
  report, and duration directly establish every item above; focused tests and
  repository lanes remain supporting evidence only.

## Validation result

The final ledger contains thirteen bounded findings. No retained correction
requires specification revision. The task is not acceptable until all thirteen
are closed and the cumulative candidate is reviewed again.
