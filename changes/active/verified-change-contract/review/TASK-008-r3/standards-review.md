# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Complete range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review context inspected: `review/TASK-008-r1/` and `review/TASK-008-r2/`; their conclusions were treated only as hypotheses and evidence pointers.
- CodeGraph: not applicable; the repository has no `.codegraph/` directory.

The candidate commit matched the requested identity before and after this
review. This report audits repository standards only. It does not decide task
acceptance or perform the Ponytail validation.

## Applicable authority

The complete applicable authority selected through
`architecture/references/README.md` was:

- `AGENTS.md`, especially ownership, Rust, async, testing, completion,
  spec-driven workflow, and documentation rules;
- `architecture/agent-rules.md`;
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx` for the
  server/client and public-surface boundaries;
- `architecture/bifrost-design.md` for Scribe, Oracle, Forge, resource, and
  measurement meanings;
- `architecture/operations/deployment-and-release.md` for the measured replica
  topology, process lifecycle, and release evidence;
- `architecture/references/architecture/patterns.md`;
- `architecture/references/languages/spec-driven-development.md`;
- `architecture/references/languages/implementation-execution.md`;
- `architecture/references/languages/rust-core.md`;
- `architecture/references/languages/testing-workflows.md`;
- `architecture/references/domain/telemetry-observations.md`;
- `architecture/references/domain/olap-serving.md`; and
- `architecture/references/domain/analytical-operations-reliability.md`.

Security, PyO3, Python, TypeScript, generated-schema, migration, and public
error authorities were checked for applicability. The candidate changes no
such implementation surface, so their detailed language-specific rules do not
govern this diff.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Active task metadata, revision-57 evidence, and retained review artifacts | `AGENTS.md` §§11-12, 14-16; `spec-driven-development.md`; `implementation-execution.md` | **PASS** — the task now names approved revision 57, status `review`, and the current mapped obligations. Prior evidence remains historical and the linked remediation records its corrections. |
| Consolidated `capacity` binary: `main`, `load`, `step`, `evidence`, `report`, `fixture`, `judge`, and `profile` | `AGENTS.md` §§4-6, 11-12, 15-16; `agent-rules.md` Rust structure/documentation/import/async rules; `rust-core.md`; `patterns.md`; `testing-workflows.md`; Bifrost, OLAP, telemetry, analytical-reliability, and deployment authorities | **FAIL** — the cohesive `Benchmark`, `Deployment`, and closed `StepKind` shapes conform, as do bounded request permits and typed public-client paths. The claimed absolute lifetime still crosses unbounded blocking child-process waits on a Tokio worker (STD-R3-001), and the helper that actually drops timed-out work lacks the required cancellation/partial-progress contract (STD-R3-002). |
| Release-process harness in `wyrd-testing/src/release_server.rs` | `AGENTS.md` §§6, 11, 16; `agent-rules.md`; `rust-core.md` async/documentation rules; deployment-and-release authority | **FAIL** — replica topology, low joined ports, cgroup envelope, retained abnormal logs, and bounded server termination are explicit. `LocalServer::start`, however, performs synchronous `Command::output()` for migration and tenant setup from an async workflow, so its caller's Tokio deadline cannot preempt a stalled child (STD-R3-001). |
| `wyrd-testing/Cargo.toml` and `mise.toml` benchmark consolidation | `AGENTS.md` §§1, 4, 11-12, 15; `agent-rules.md` feature/task/gate rules; `testing-workflows.md`; `implementation-execution.md` | **PASS** — one opt-in server-capacity task and binary remain, no dependency or Cargo feature was added, the repository-managed Postgres/RustFS environment is preserved, and no fast lane is burdened with the benchmark. |
| Queue default documentation in `wyrd-queue/src/config.rs` | `AGENTS.md` §§4, 16; `agent-rules.md` documentation rule; `rust-core.md` | **PASS** — the edit only points the existing default's evidence to the consolidated benchmark and does not change queue behavior. |
| Postgres-backed runtime integration tests in `pg_verification_runtime.rs` | `AGENTS.md` §§11-12, 16; `agent-rules.md` external-test/Postgres-lane rules; `testing-workflows.md`; Bifrost durability and tenant-fairness authority | **PASS** — the external target is earned by real Postgres and runtime composition, the tests document why this seam cannot be observed through a process journey, and no assertion or gate was weakened. |
| Rust SDK real-server journeys in `drift_verification.rs` and `observe_run.rs` | `AGENTS.md` §§2-4, 9-12, 16; `wyrd-design.md` client model; `wyrd-doctrine.mdx`; `testing-workflows.md`; OLAP and telemetry references | **PASS** — the changed tests drive the public Rust SDK against a real server, stay in the Rust runtime, cover the failed judge verdict and sustained exactly-once/flat-byte behavior, and use the established gated journey convention. No Python or TypeScript surface changed. |
| Deletion of the three superseded benchmark source trees and movement of their required proof | `AGENTS.md` §§11-12, 15; `implementation-execution.md`; `testing-workflows.md` | **PASS** — the obsolete capacity entry points are removed, while their required correctness and durability claims have focused unit, integration, or real-server journey owners. No production contract or generated artifact was deleted. |

## Rule-by-rule evidence

| Applicable rule | Source evidence | Result |
|---|---|---|
| The exact human-approved specification revision governs the task and implementation. | Candidate `spec.md:1-4` is approved revision 57; `task-008-closeout.md:1-18` names revision 57 and status `review`. | **PASS**. The prior r2 authority blocker is resolved. |
| Stateful, dependency-backed, multi-step Rust workflows have a meaningful concrete owner and discoverable inherent methods. | `capacity/main.rs:191-574` places setup-to-report lifecycle state on `Benchmark`; `capacity/step.rs:303-513` places one measured step on `Deployment`; `main.rs:621-637` is a thin process boundary. | **PASS**. Prior STD-002 is closed. |
| Closed behavioral sets use enums when exhaustiveness matters. | `capacity/step.rs:58-93` owns step identity as `StepKind`; report verdict selection consumes that enum. | **PASS**. |
| Async code directly awaits IO/composition, never blocks the Tokio worker, and external calls have enforceable bounds. | `capacity/main.rs:313` wraps `self.measure()` in `timeout_at`, but `Benchmark::provision` calls `LocalServer::start`; `release_server.rs:155-169,624-625` executes migration/setup with blocking `Command::output()`. A stalled child prevents the future from yielding, so the timer cannot fire. Cleanup also invokes synchronous process termination from the async lifecycle at `capacity/main.rs:509-529`. | **FAIL** — STD-R3-001. |
| Every materially changed Rust item has substantive rustdoc; fallible functions document errors and async/durable operations document cancellation, partial progress, retry, and residue where relevant. | Most capacity boundaries have `# Errors` and `# Cancellation`. `Lifetime::bounded` at `capacity/main.rs:174-188`, however, is the common owner of `timeout_at` and drops arbitrary supplied work, yet documents neither which effects may survive nor the retry meaning. | **FAIL** — STD-R3-002. |
| Concurrency and buffering are bounded and owned. | `load.rs` uses the shared semaphore and accounts missed arrivals; client observation traffic uses `WyrdState`'s default bounded queue; `Deployment` owns task and capture lifetime. | **PASS**, apart from the blocking subprocess boundary in STD-R3-001. |
| Test-only evidence does not move durable behavior into a client or alternate engine. | Workload generation uses `Verification`, `WyrdState`, and `Bifrost`; direct SQL in `capacity/evidence.rs` is read-only benchmark observation. Server-owned Gate, Scribe, Oracle, Forge, audit, and verification behavior remain in their owners. | **PASS**. |
| Raw `PgPool` does not enter library contracts. | The owner pool in `capacity/evidence.rs` is private to an opt-in binary; no library signature or exported durable behavior accepts it. | **PASS**. |
| Imports remain top-of-module and signatures use imported bare names. | All changed Rust modules place imports at module scope; no new function-local imports or noisy fully-qualified durable types were introduced in signatures. | **PASS**. |
| Errors, secrets, and diagnostics follow repository rules. | The benchmark binary uses a reporting `Result`; credentials stay in `SecretString`; generated signing and TLS material live under temporary ownership; reports contain no credential values; no new production `unwrap()` or unjustified Clippy allow was added. | **PASS**. |
| External tests earn their targets and service-dependent proof uses repository-managed lanes. | Runtime tests compose Postgres-backed verification owners; SDK tests start `WyrdTestServer`; the implementation/remediation records exact setup-wrapped `nextest` commands. | **PASS**. |
| No gate is weakened with an allow, deleted assertion, or inappropriate ignore. | The new SDK journey uses the established `#[ignore = "requires the repository-managed Postgres journey lifecycle"]` gate and has an explicit `--run-ignored=all` command. Removed benchmark assertions have focused replacement proof. | **PASS**. |
| Build/test configuration avoids unearned dependencies/features and keeps opt-in benchmarks out of normal gates. | Manifest changes replace binary targets only; `bench:capacity` is explicitly opt-in and uses the existing release/cloud build and managed services. | **PASS**. |
| Reported Bifrost evidence preserves the owning metrics' meanings. | Scribe staged members, audit watermark gap, Forge demand, run queue, paired overhead, judge wait, client drain, CPU, and memory have distinct evidence owners and report fields. | **PASS**. |

## Material findings

### STD-R3-001 — The async benchmark lifetime crosses unbounded blocking subprocess waits

- **Violated rule:** `AGENTS.md` §6 requires narrow async boundaries and an
  explicit blocking strategy; `architecture/references/languages/rust-core.md`
  says never to block a Tokio worker and requires bounded external calls;
  `AGENTS.md` §16 requires documentation and behavior to state real lifecycle
  invariants accurately.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:311-314,
  397-429,500-529`; `crates/wyrd/wyrd-testing/src/release_server.rs:147-169,
  355-387,618-635`.
- **Evidence:** `Lifetime::measure` uses `tokio::time::timeout_at`, but the
  measured provisioning future calls `LocalServer::start`. That async function
  synchronously waits for `migrate` and every `setup` through
  `Command::output()`. While a child is stalled, the Tokio worker cannot poll
  the deadline, so the stated setup-through-cleanup lifetime is not enforceable
  on the real path. The focused paused-time test supplies a cooperatively
  pending future and therefore cannot exercise this failure mode. Cleanup also
  runs the synchronous stop loop directly from the async lifecycle.
- **Consequence:** a wedged migration or setup child can keep the operator's
  command alive past its absolute limit without reaching normal cleanup or the
  failure report. It also blocks one async runtime worker rather than making
  blocking process ownership explicit.
- **Testable correction:** make the release-harness subprocess owner
  deadline-aware and cancellable without changing production server behavior:
  spawn children, await/poll them without blocking the Tokio worker, and on the
  existing benchmark deadline terminate and reap the child while retaining its
  diagnostic output. Route synchronous replica stopping through an explicit
  blocking boundary or an equivalently non-blocking process owner. Add a
  process-level test with a deliberately non-terminating setup/migration child
  that proves timeout, termination/reaping, retained diagnostics, cleanup, and
  completion within the one absolute deadline.

### STD-R3-002 — The common timeout owner omits its cancellation and partial-progress contract

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` make
  substantive rustdoc mandatory for every materially changed Rust item;
  `architecture/references/languages/rust-core.md` requires async/durable
  documentation to state cancellation, partial progress, idempotency, and
  retry behavior.
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`.
- **Evidence:** `Lifetime::bounded` is the method that owns `timeout_at` and
  drops the supplied future at the deadline. Its rustdoc only says that it runs
  work until the deadline and names the phase. Unlike its two callers, it does
  not document cancellation, surviving effects, cleanup ownership, or whether
  retrying the supplied operation is meaningful. This is the exact cancellation
  boundary, not a pure delegating helper.
- **Consequence:** a maintainer can change or reuse the common deadline helper
  without seeing that cancellation may retain durable server effects and that
  cleanup depends on the dropped work's external owners. The documentation hard
  gate remains incomplete even though adjacent methods describe selected cases.
- **Testable correction:** document `Lifetime::bounded`'s cancellation and
  partial-progress contract at the owning method: when the future is dropped,
  what effects can survive, which owners perform cleanup, and that retry safety
  is supplied-work-specific. Keep the existing caller-specific details and
  rustdoc/lint/unit checks green.

## Verification reviewed and limits

- Independently run: `mise exec -- cargo nextest run --locked -p wyrd-testing
  --bin capacity` — **14 passed, 0 skipped**.
- Independently run: `git diff --check
  f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
  — clean.
- Recorded evidence reviewed: focused Postgres-backed runtime tests, Rust SDK
  journeys, `mise run fmt`, `mise run lints`, scoped all-target/all-feature
  Clippy, and the reduced full-topology smoke.
- The unit deadline test proves cooperative Tokio cancellation only. There is
  no stalled-child proof for the blocking migration/setup path in
  STD-R3-001.
- Per the caller's sequencing instruction,
  `FIND-TASK-008-CLOSEOUT-13` (the unmodified default benchmark execution) is
  **DEFERRED to integration** and is not a blocker or repository-standard
  finding for this candidate.

## Overall result

**FAIL**

Revision 57 is present and the prior task-authority, lifecycle-owner, report,
and most documentation findings are closed. Two material repository-rule
violations remain: the real subprocess path can block through the claimed
absolute async deadline, and the common timeout owner still lacks its required
cancellation/partial-progress rustdoc.
