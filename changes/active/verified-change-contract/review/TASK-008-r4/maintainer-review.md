# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and
  `review/TASK-008-r3/`
- Remediation reviewed: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`
- Governing maintainer authority: `AGENTS.md` §§5, 6, 11, and 16;
  `architecture/agent-rules.md`;
  `architecture/references/languages/{maintainer-style,rust-core,testing-workflows,spec-driven-development}.md`;
  and the revision-57 verification-capacity contract.

The candidate commit matched the requested identity before and after source
inspection. CodeGraph is absent, so symbol, caller, and consumer tracing used
Git and repository source directly. This report changes no reviewed production
or test source.

## Changed-surface coverage

| Surface | Material symbols, owners, callers, and tests followed | Maintainer assessment |
|---|---|---|
| Command and target ownership | `mise.toml`'s `bench:capacity`; the single `capacity` Cargo target; removal of the three superseded capacity binaries and tasks; `QueueConfig::default`'s evidence reference | The operator has one discoverable entry point. The shell establishes `WYRD_CAPACITY_STARTED` and `WYRD_CAPACITY_DEADLINE` before its first setup action and passes only the remaining budget to RustFS setup, the Postgres wrapper, the release build, and the capacity binary. The command, target, and documentation use the same name and boundary. |
| Benchmark lifetime owner | `capacity/main.rs::{Lifetime,Benchmark,main}` through `Lifetime::{from_command,new,measure,shut_down,bounded}`, `Benchmark::{prepare,run,measure,provision,scale_out,reconnect,step,clean_up}`, and `Report::write_to` | `Benchmark` remains the cohesive setup-to-report owner, while `Lifetime` owns the one absolute deadline and explicit cleanup reserves. `main` is a thin boundary. Names and return shapes make preparation, measurement, cleanup, report, and exit behavior followable without introducing another timeout abstraction. |
| Process ownership and cancellation | `release_server.rs::{LocalServer,OperatorRun,STOP_GRACE}` through migration, serving, tenant setup, joined replicas, graceful stop, and both `Drop` paths | `LocalServer` still owns serving replicas; the focused `OperatorRun` now owns each migration/setup child. `OperatorRun::finish` yields while polling, and `Drop` kills and reaps an unfinished child while retaining stderr diagnostics. `LocalServer::start` documents the aggregate partial-progress behavior. This closes the unowned blocking-child path from prior FIND-2. |
| Cancellation and error documentation | `Lifetime::{measure,shut_down,bounded}`, `Benchmark::{prepare,run,measure,provision,scale_out,reconnect,step,clean_up}`, `LocalServer::start`, and `OperatorRun::{spawn,finish,Drop}` | The shared cancellation owner now states cooperative future drop, the non-interruptibility of non-yielding work, surviving effects, owner cleanup, and workflow-specific retry safety. Caller docs narrow that contract for their own durable effects. Fallible boundaries name their errors. Prior FIND-6 is closed. |
| Step, deployment, load, and evidence model | `step.rs::{Deployment,StepKind,Plan,ResourceWindow,Drain,Record}`, `load.rs::{Op,Work,Lane,TenantClients,Request}`, `fixture.rs::{Kind,Tenant}`, `evidence.rs::{Scrapes,Queue,Backlog}`, `judge.rs::{Judge,TlsListener}`, and `profile.rs::{Profile,Capture}` through the benchmark sequence and report | Stateful workflows remain on meaningful owners, closed behavioral sets use enums, and small free functions are deterministic transformations. The remediation did not disturb the earlier owner/method, naming, type, or evidence-shape corrections. |
| Report contract | `report.rs::{Cell,Row,Report,step_row,op_row}` through Markdown/JSON output, failure rendering, shutdown evidence, and verdict tests | The shared-column report remains locally followable. `setup_seconds` and `total_seconds` now explicitly start at the command boundary, matching their producer in `Benchmark`; no report column or verdict semantic changed. |
| Focused lifecycle proof | `capacity/main.rs::tests::{an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline,a_stalled_tenant_setup_stops_the_run_by_its_deadline}`, the stand-in server, PID reaping helper, and `release_server` tests | Test names and assertions state caller-relevant outcomes. The stalled-setup proof checks bounded failure, child and replica reaping, retained diagnostics, suppression of later setup/measurement, and the failed report. Its environment needs are explicit in the gated annotation and recorded exact command. |
| Cumulative correctness proof | `pg_verification_runtime.rs` two-replica claim and tenant-fairness tests; `drift_verification.rs` failed LLM-judge case; `observe_run.rs` sustained 100-feature ingest journey; their owning helpers and public SDK/server consumers | These remain appropriately placed integration/journey proofs with precise names and substantive rustdoc. The latest remediation changes no production behavior, workload, SLO, public API, or generated contract. |
| Generated declaration parity | Cargo/mise wiring, internal benchmark modules, harness code, and Rust-only tests | No Python, TypeScript, OpenAPI, schema, stub, or generated declaration is changed or consumed by this cumulative task. No generated parity action applies. |

## Prior maintainer-finding closure

| Prior finding | Current evidence | Result |
|---|---|---|
| `MNT-R3-001` / `FIND-TASK-008-CLOSEOUT-2`: the documented complete-command lifetime had no enforceable owner | `mise.toml:507-549` establishes the absolute outer deadline; `capacity/main.rs:81-265,313-425,581-613,702-729` carries it through preparation, measurement, cleanup, reporting, and exit reserves; `release_server.rs:130-208,636-732` makes migration/setup waits cancellable, owned, and reapable | **CLOSED** |
| `MNT-R3-002` / `FIND-TASK-008-CLOSEOUT-6`: the shared cancellation boundary omitted partial-progress semantics | `capacity/main.rs:232-265` documents the exact cooperative cancellation contract, surviving effects, cleanup ownership, and retry delegation; `release_server.rs:148-156,681-713` documents the process-specific consequences | **CLOSED** |

## Material findings

None. Every materially changed symbol has a discoverable owner, descriptive
name and typed shape, substantive documentation, and relevant focused proof.
No personal-preference concern rises to a concrete cost of understanding or
safely changing the approved behavior.

## Calibration notes

- The deadline necessarily crosses the mise wrapper and Rust binary because
  command-owned setup begins before the binary exists. One exported absolute
  deadline joins those boundaries; introducing a public option, supervisor, or
  generalized process framework would add machinery without improving this
  task's maintainer contract.
- `Benchmark` and `Report::render` are large, but each keeps one ordered,
  cohesive workflow. Splitting them solely for line count would not make the
  lifecycle or artifact easier to follow.
- `OperatorRun` is not a speculative abstraction: it owns the two real
  operator-child variants (`migrate` and `setup`) that require identical
  cancellation, diagnostic, and reaping behavior.

## Verification reviewed and limits

- Remediation evidence records the exact stalled-child process proof passing,
  the cooperative lifetime proof passing, 14 passing default `capacity`
  target tests with the process proof gated, two passing `release_server`
  library tests, clean formatting, clean workspace lints, and a clean diff.
  This review inspected the cited test bodies and the paths they exercise.
- `git diff --check` is clean for the immutable cumulative range.
- Cargo and mise verification were not rerun in this shared checkout, to avoid
  overlapping another reviewer's build/test process. The recorded commands and
  static source evidence are the available verification basis for this
  specialist report.
- Per caller instruction, `FIND-TASK-008-CLOSEOUT-13` (one unmodified default
  `mise run bench:capacity`) is deferred until the other workstreams integrate.
  It is a qualification limit, not a blocker for this candidate, and this
  review makes no empirical AC-040/AC-041 capacity claim.

## Overall result

**PASS**

Maintainer coverage is complete. The current candidate closes both prior
maintainer findings within the existing `Benchmark`, `Lifetime`,
`LocalServer`, and report owners, preserves the cumulative task's readable
owner/type/test structure, and introduces no material maintainer-style defect.
