# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and `review/TASK-008-r3/`
- Remediation reviewed: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`

The candidate remained at the named commit during this review. The checkout
has no `.codegraph/` directory, so process and caller tracing used the
repository source and Git diff directly. The full cumulative range was
inspected, with the round-three remediation treated as a hypothesis to verify
against the current source rather than as proof.

## Deployed path and affected capabilities

The candidate does not change production server, queue, Scribe, Oracle,
Forge, verification-engine, SDK, database, or deployment semantics. Its
runtime effect is confined to the opt-in capacity command and the release
server/process harness used by that command. The only non-test crate change is
a documentation reference in `wyrd-queue`.

The command and process topology is:

1. `mise.toml:507-548` fixes wall-clock start/deadline variables, starts the
   RustFS Compose service and setup container, enters the repository-managed
   Postgres wrapper, builds release `wyrd-server`, then invokes `cargo run` for
   the `capacity` binary. It wraps each phase in GNU `timeout --foreground`.
2. `capacity/main.rs:150-275,702-728` maps the shell timestamps to a monotonic
   `Lifetime`, reserves measurement, client-cleanup, replica-stop, and exit
   intervals, and bounds preparation plus measured work cooperatively.
3. `capacity/main.rs:392-510` prepares fixtures, starts replica 0, migrates,
   provisions four tenants, connects clients, and runs the one-replica steps.
   It later adds replica 1 against the same Postgres/RustFS deployment and
   runs the two-replica steps.
4. `release_server.rs:157-207,224-258,636-731` owns migration and tenant-setup
   children through `OperatorRun`, polling them asynchronously so cancellation
   drops, kills, and reaps the current direct child while retaining stderr.
   `LocalServer` owns each serving process.
5. `capacity/main.rs:590-613` shuts clients down, stops replicas newest first,
   copies their logs, and then `main.rs:399-424` renders the failed or passing
   report.

Affected capabilities are benchmark evidence for direct and queued
verification, client ingest, Oracle queries, Scribe/audit/Forge drain, and
one-to-two-replica scale-out. Production availability is measured but not
modified.

## Failure and recovery paths

| Failure or interruption | What stops or remains available | Recovery and proof assessment |
|---|---|---|
| A public request is refused, times out, or produces a wrong judgment | The affected benchmark operation is counted as an error; other operations and replicas continue. | The load generator does not retry ambiguous work, so it does not amplify non-idempotent direct execution. The affected step fails from public-client evidence. |
| Postgres, RustFS, metrics, or evidence SQL becomes unavailable after the binary starts | The local request, scrape, or query fails; the production server is not deliberately crashed. | `Benchmark::run` reaches owned client/replica cleanup and writes a failed report when the error propagates through the cooperative in-binary path. |
| Migration or tenant setup stalls after the binary starts | `Lifetime::bounded` drops `LocalServer::start`; `OperatorRun::Drop` kills and reaps the active direct child, and an already-started `LocalServer` is killed and reaped with its log retained. | The ignored process test at `main.rs:806-898` exercises this path with an executable that `exec`s the stalled child and proves failed-report generation. This closes the former blocking `Command::output()` path inside the binary. |
| A measured future reaches its share of the deadline | Its future is dropped at a cooperative yield; completed durable effects remain and no partial step record is promoted. | `Lifetime::bounded` now documents cooperative cancellation, surviving effects, owner cleanup, and workflow-specific retry safety (`main.rs:232-265`). The paused-time test covers this local boundary. |
| A release replica ignores `SIGTERM` during ordinary binary-owned cleanup | The affected server remains up through the 45-second grace; other replicas are stopped sequentially. | `LocalServer::terminate` kills and reaps it after the grace, reports failure, and preserves/copies logs (`release_server.rs:365-405,487-502`). |
| The outer command reaches its deadline while the Postgres wrapper, build, `cargo run`, or a descendant is still active | GNU `timeout --foreground` signals only its direct command, not that command's children. The wrapper or Cargo may exit/be killed while the nested shell, compiler, capacity binary, replicas, or other descendants remain alive. | The claimed command-wide owner does not own this process tree; see SYS-R4-001. The recorded shell shim proved deadline arithmetic and argument propagation, not descendant termination. |
| Report writing or Postgres teardown is slow | The report's recorded total has already been sampled, although command-owned work continues. | The wrapper is intended to reserve time for these phases, but the artifact cannot report their duration; see SYS-R4-002. |

## Material proposed findings

### SYS-R4-001 — INCORRECT — foreground timeouts do not bound the command's descendant process tree

- **Violated obligation:** Revision-57 REQ-171 and remediation AC-R2-1/2
  require one enforceable setup-to-exit 30-minute boundary over the complete
  default command, with expiry returning nonzero and leaving no owned child
  behind. This is the still-reachable outer-command part of prior
  `FIND-TASK-008-CLOSEOUT-2`.
- **Exact location:** `mise.toml:511-548`; nested process owner
  `scripts/postgres/with-test-postgres.sh:12-37,104-108`; binary entry
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:702-728`.
- **Evidence:** Every shell deadline uses GNU `timeout --foreground`. GNU
  `timeout --help` and the installed Coreutils manual explicitly state that
  in foreground mode children of the managed command are not timed out. The
  outer instance therefore signals `with-test-postgres.sh`, not its nested
  `bash`/Cargo/capacity descendants, and the inner instance signals `cargo`,
  not the capacity binary Cargo spawned. The wrapper's TERM handler performs
  synchronous Compose teardown and signals only itself; the outer
  `--kill-after=5` can kill that wrapper without terminating its descendants.
  The RustFS and release-build timeouts have no `--kill-after` at all. The
  in-binary stalled-setup test begins at `Benchmark::prepare` and the recorded
  shell shim stubs Docker/Cargo; neither traverses or asserts termination of
  the actual nested shell/Cargo process tree.
- **Observable system consequence:** A stalled or TERM-ignoring nested build
  or benchmark process can remain alive after the timeout reports failure or
  after the Postgres wrapper is killed. It can retain a capacity binary,
  systemd-scoped replicas, compiler work, and file handles after the advertised
  command deadline, while teardown removes its database and no final report
  or logs are guaranteed. The operator sees a bounded wrapper exit without a
  bounded deployment lifecycle.
- **Testable correction:** At the existing `bench:capacity` command boundary,
  use one process-group/session ownership arrangement that delivers TERM and
  the bounded KILL escalation to every command-owned descendant while
  preserving the existing interactive-signal intent. Preserve the in-binary
  `Lifetime`, `Benchmark`, `OperatorRun`, and `LocalServer` owners for orderly
  report and diagnostic cleanup. Add a shortened command-level proof whose
  nested wrapper/Cargo stand-in leaves a child running; prove deadline-bounded
  nonzero exit, descendant termination/reaping, wrapper teardown, and no later
  phase. Reuse native shell/Coreutils process-group behavior; do not add a
  supervisor framework or public timeout option.

### SYS-R4-002 — INCORRECT — the report duration ends before the command interval it claims to describe

- **Violated obligation:** Remediation AC-R2-1 requires the report's total
  duration to cover the same setup-to-exit interval as the command-wide
  deadline, including diagnostic/report finalization and process exit.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:392-424`, especially
  `main.rs:402`; remaining command work at `mise.toml:531-548` and
  `scripts/postgres/with-test-postgres.sh:17-27`.
- **Evidence:** `Report::total_seconds` is sampled while constructing the
  report, before `identity`, JSON/Markdown serialization and writes,
  `println!`, capacity-process and Cargo exit, and the Postgres wrapper's
  Compose teardown. Those phases are explicitly part of the remediated
  setup-to-exit boundary, but their time cannot appear in the already-written
  value. Reserving 60 seconds for them bounds neither the recorded interval
  nor its fidelity.
- **Observable system consequence:** A slow report filesystem or slow
  Postgres teardown can consume the remaining command budget—or cross it via
  SYS-R4-001—while the durable report understates elapsed time and appears to
  demonstrate compliance. Post-run diagnosis cannot distinguish a bounded
  complete command from one whose unreported finalization dominated the tail.
- **Testable correction:** Make the durable command evidence record the end
  of the same externally owned interval, or stop claiming that an in-binary
  pre-exit sample is the setup-to-exit duration. Keep the existing report
  schema and command boundary; the focused proof must introduce delayed final
  report/wrapper teardown and show that the recorded completion evidence and
  enforced deadline agree. Do not add a second benchmark or production
  telemetry surface.

## Verification and recovery assessment

Recorded remediation evidence reports the focused capacity target (14 passed,
one ignored), `release_server` unit tests, the ignored stalled-setup process
proof, formatting, lints, and a shell shim dry run. Static inspection confirms
that `OperatorRun` removes the prior uninterruptible in-binary migration/setup
wait and that the cancellation documentation now states the real cooperative
contract.

No Cargo or mise lane was rerun during this parallel review. No available
proof exercises the actual `timeout --foreground` -> Postgres wrapper -> nested
shell -> Cargo -> capacity process topology at expiry, nor a delayed report and
wrapper teardown. The full unmodified default benchmark identified by
`FIND-TASK-008-CLOSEOUT-13` is explicitly deferred by the caller until other
workstreams integrate; it is recorded as a verification limit and is not a
blocker for this candidate. No empirical AC-040/AC-041 capacity claim is made.

## Overall result

**FAIL**

The in-binary cancellation, operator-child ownership, cleanup documentation,
and failed-report path are materially improved. The default shell command still
does not bound the descendants it creates, and its report does not measure the
complete interval it claims. Both are bounded corrections within the existing
private benchmark/process-harness boundary and require no specification,
public API, production-runtime, persistence, security, or deployment decision.
