# TASK-008 round-four behavior review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and
  `review/TASK-008-r3/`
- Current remediation: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`

The candidate remained at the named commit throughout this review. The checkout
has no `.codegraph/` index, so navigation used Git, repository search, and direct
caller/consumer inspection. I reviewed the complete cumulative diff rather than
the implementation record alone, then traced the remediation delta from the
operator command through `Benchmark`, `Lifetime`, `LocalServer`, `OperatorRun`,
cleanup, report production, and exit.

Per the caller's explicit sequencing instruction,
`FIND-TASK-008-CLOSEOUT-13`—the full unmodified default benchmark—is deferred to
integration after the other workstreams merge. It is not a blocker in this
review and supplies no empirical AC-040/AC-041 qualification.

## Realistic command-to-result trace

`mise run bench:capacity` fixes `WYRD_CAPACITY_STARTED` and
`WYRD_CAPACITY_DEADLINE` before its first setup action. It then runs two RustFS
Docker Compose commands, enters the repository Postgres wrapper, builds the
release server, and starts the `capacity` binary. The binary maps the exported
wall-clock interval onto `Lifetime`, bounds `Benchmark::prepare` and
`Benchmark::measure`, reserves client and replica cleanup time, and writes the
Markdown/JSON report. `LocalServer::start` now owns `migrate` and tenant `setup`
children through `OperatorRun`; polling yields to the Tokio deadline, and drop
kills and reaps a still-running child while preserving stderr. A serving
replica remains owned by `LocalServer` and is killed/reaped with its log kept on
abnormal drop. These paths close the previously demonstrated stalled tenant
setup failure and now document cooperative cancellation and retained effects.

The command boundary is still incomplete before the Postgres wrapper. The two
RustFS steps at `mise.toml:526-529` invoke GNU `timeout --foreground` with the
entire remaining budget but no `--kill-after`. GNU `timeout` sends `TERM` when
the duration expires and only sends `KILL` when `--kill-after` is supplied; its
own help also states that foreground-mode children are not timed out. A Docker
Compose client that does not exit on `TERM` therefore leaves `timeout` waiting
past the absolute deadline. This path is mandatory and occurs before
`Benchmark` or its report owner exists, so the in-binary lifetime,
`OperatorRun`, replica drop, and outer Postgres-wrapper kill reserve cannot
recover it. The recorded shell proof used successful stubs and did not exercise
this path.

Mise's argument handoff was inspected separately rather than inferred from the
literal TOML. A non-executing shell override showed mise rendering the end of
the task as `bash -lc '...' _ --profile --levels 20 ...`; `_` is the inner
shell's `$0`, and the appended words correctly populate its `$@`. The inner
profile branch and final `cargo run ... "$@"` therefore receive task arguments;
there is no argument-forwarding finding.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exact approved revision 57 and current task contract | `spec.md:1-4`; `task-008-closeout.md:1-13` | Immutable blob/frontmatter inspection | PASS |
| CLOSE-01 and inherited TASK-008 proof closure remain current | Cumulative diff preserves the registered correctness, SDK, server, runtime, storage, authorization, tenancy, replay, and generated-contract owners; task evidence maps their strongest-tier commands | Prior recorded focused/gate evidence reviewed; no affected inherited production owner was changed in this range | PASS, subject to the review's no-rerun limit |
| REQ-171: one reachable server capacity benchmark | `mise.toml:507-549`; one `capacity` `[[bin]]` in `wyrd-testing/Cargo.toml`; obsolete verification/ingest/query capacity targets and sources deleted | Changed-path and repository search inspection | PASS |
| REQ-171: exact four-tenant public-client workload and warmup/ramp/knee/sustained/scale-out sequence | `capacity/load.rs::{Lane::drive,Request::send,mix,query}`; `capacity/main.rs:427-475`; `capacity/fixture.rs` | Existing capacity unit evidence and reduced smoke recorded in the task | PASS structurally |
| REQ-171: traffic, errors, client percentiles, overhead, ingest drain, all four server backlogs, CPU, and peak-memory evidence | `capacity/{evidence,step,report}.rs`; staged Scribe members participate; resource samples share one measured interval; one report table renders step then operation rows | Focused report/evidence tests recorded as passing | PASS structurally |
| AC-040: the five reference workloads, four non-judge p95/sample-floor judgments, and separate judge wait diagnostics | `fixture.rs::Kind`; `evidence.rs::Scrapes::overhead`; `judge.rs`; `report.rs:192-234` | Focused workload/report tests recorded as passing; default empirical qualification deferred | PASS structurally; empirical result deferred |
| AC-041: default-queue 500 observations/s × 100 features at `L=200`, zero queue refusals, drain threshold, exactly-once rows, and flat bytes | `load.rs` rate/feature construction; report error/drain cells; `observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` | Focused real-server journey recorded as passing; default empirical qualification deferred | PASS structurally; empirical result deferred |
| Revision-57 test-owned correctness: cross-replica exclusive claims, tenant fairness, direct failed LLM/SPC judgments, queued Custom failure | `pg_verification_runtime.rs` focused tests; `drift_verification.rs` judgment cases; duplicate-claim judgment absent from capacity report | Recorded focused commands in task evidence | PASS |
| Prior findings 1, 3-5, and 7-12 remain closed | One entry point, separate provider wait, current task metadata, `Benchmark` owner, `StepKind`, complete Scribe backlog, exact drain edge, one table, common resource interval, and test-owned duplication all remain in current source | Prior review evidence plus cumulative source inspection | PASS |
| Prior FIND-6 / R2 AC-R2-3: cancellation and partial progress are explicit | `Lifetime::bounded` at `capacity/main.rs:232-265`; `LocalServer::start` at `release_server.rs:130-157`; `OperatorRun::finish` at `:681-713` | Static async/process documentation audit; recorded lint result | PASS |
| Prior FIND-2 / R2 AC-R2-2: a stalled in-binary migration/setup child is owned, terminable, reapable, and diagnostic | `release_server.rs:157-207,636-731`; `capacity/main.rs:778-898` | Recorded ignored process proof and ordinary capacity/lib tests | PASS for the demonstrated in-binary operator-child path |
| REQ-171 and R2 AC-R2-1: the complete mandatory command exits inside one enforceable absolute 30-minute boundary | The Postgres wrapper, build, binary, `Lifetime`, and operator children have remaining-budget/kill or owned-drop behavior, but mandatory RustFS commands at `mise.toml:526-529` have TERM-only timeouts with no forced-kill reserve | Successful-stub dry run only; no stalled pre-Postgres child proof | **FAIL — `BHV-R4-001`** |
| REQ-171 `--profile` and task smoke arguments reach the capacity binary unchanged | Mise appends task arguments after the nested shell's `_` `$0`; the inner script branches on `$*` and forwards `"$@"` | Non-executing `mise run --shell /usr/bin/printf bench:capacity -- --profile --levels 20 ...` rendering | PASS |
| R2 AC-R2-4 and task non-goals: no workload/SLO/public/production timeout redesign or speculative limiter | Remediation delta is confined to review evidence, benchmark/test harness, report field docs, and `mise.toml`; cumulative behavioral test changes remain within approved revision-57 proof | Cumulative diff inspection | PASS |
| FIND-TASK-008-CLOSEOUT-13 default execution | Caller explicitly sequences it after integration | Not run by design | DEFERRED, non-blocking |

## Proposed findings

### `BHV-R4-001` — INCORRECT — the pre-Postgres RustFS setup can outlive the absolute command deadline

- **Violated obligation:** Revision-57 REQ-171 and remediation AC-R2-1 require
  the complete default `mise run bench:capacity` command, from its first setup
  action through exit, to finish inside one enforceable absolute 30-minute
  boundary. Expiry must stop later work and leave no owned command child.
- **Exact location:** `mise.toml:526-529`.
- **Evidence:** Both mandatory Docker Compose setup commands use
  `timeout --foreground "$seconds"` with the complete remaining budget and no
  `--kill-after`. GNU `timeout` only sends `TERM` at expiry unless a kill-after
  duration is configured. If either client does not terminate on `TERM`, the
  wrapper continues waiting beyond `WYRD_CAPACITY_DEADLINE`. The later outer
  Postgres-wrapper timeout, in-binary `Lifetime`, `OperatorRun`, cleanup, and
  report path have not started and cannot intervene. The remediation's shim
  dry run covered argument/deadline propagation with successful stubs, not a
  TERM-resistant first or second setup command.
- **Observable consequence:** A stalled RustFS startup/setup client can keep
  the advertised default command alive past 30 minutes and prevent all later
  setup, diagnostics, benchmark reporting, and normal exit. The corrected
  migration/setup behavior does not cover this earlier reachable path.
- **Required testable correction:** Continue to reuse the one exported absolute
  deadline and GNU `timeout`, but reserve a bounded forced-kill interval before
  that deadline and apply it to each pre-Postgres RustFS command so a command
  that ignores `TERM` is killed and reaped before the absolute boundary. A
  focused shell/process proof must replace one mandatory pre-Postgres command
  with a controlled child that traps or ignores `TERM`, then prove bounded
  nonzero exit, child reaping, and that the next setup phase never starts.
  Preserve the existing workload, SLOs, Docker topology, Postgres wrapper,
  binary lifetime, public surfaces, and deferred empirical run. No new
  supervisor, dependency, public timeout knob, retry, or grace configuration is
  needed.
- **Prior-finding relationship:** This is a proposed reachable continuation of
  stable `FIND-TASK-008-CLOSEOUT-2`, not a separate product behavior or a
  reopening of `FIND-TASK-008-CLOSEOUT-6`.

## Verification limits

- Per orchestrator direction, I did not run Cargo, nextest, mise verification,
  or the long benchmark concurrently with other reviewers.
- I inspected the recorded remediation evidence: capacity target 14 passed / 1
  skipped, release-server tests 2 passed, the explicit stalled-setup process
  proof passed, and formatting/lints/diff-check were recorded clean.
- `git diff --check` for the immutable cumulative range was inspected as clean
  during this review.
- The full default benchmark is expressly deferred and is neither a candidate
  blocker nor empirical acceptance evidence.
- There is no recorded TERM-resistant pre-Postgres setup proof; the existing
  shell shim exercised successful commands only.
- A non-executing mise shell override confirmed that task arguments are appended
  after the nested shell's `_` `$0` and therefore reach its `$@`; it did not run
  Docker, Cargo, Postgres, or the benchmark.

## Overall result

**FAIL**

The cumulative candidate preserves the revision-57 workload, report,
test-owned correctness, and non-goals, and it closes the in-binary command
lifetime and cancellation-documentation gaps. It does not yet enforce the same
absolute deadline over the two mandatory RustFS setup commands, so the complete
command can still outlive the approved boundary.
