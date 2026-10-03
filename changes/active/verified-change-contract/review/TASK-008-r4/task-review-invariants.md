# TASK-008 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews used as hypotheses: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and `review/TASK-008-r3/`
- Current remediation: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`

The candidate remained at `8022436387f3a9a9499527ebf8b8b8140c6559cb`
through source inspection. `.codegraph/` is absent, so navigation used repository
source and `rg`.

The caller explicitly sequenced `FIND-TASK-008-CLOSEOUT-13`, the unmodified
default benchmark execution, after the other workstreams merge. This report
records that proof as `DEFERRED`; it is neither a candidate finding nor a
blocker.

## Navigation and invariant trace

`mise.toml:507-549` is the command-lifetime producer. It records one Unix-second
start and deadline before RustFS startup, exports them through the Postgres
wrapper, budgets each setup/build/run command from the same deadline, and passes
the values to `capacity`. Mise appends task arguments to the final command, so
the placeholder `_` becomes inner `bash`'s `$0`, its `$@` receives `--profile`
or smoke flags, the profile branch sees those arguments, and the final
`cargo run ... -- "${binary[@]}" "$@"` forwards them to `Cli`.

`capacity/main.rs::Lifetime::from_command` converts both wall-clock values to
monotonic `Instant`s. `Lifetime::new` derives measurement and client-shutdown
boundaries while reserving two replica stops and final exit time.
`main` bounds `Benchmark::prepare`; `Benchmark::run` bounds provisioning and
measurement, shuts clients down under the next boundary, stops replicas, and
feeds command-start-based setup/total values and the failure into `Report`.
`Report::{write_to,passed}` is the elapsed/failure sink.

Within the binary, migration and tenant setup now flow through
`LocalServer::start` to `OperatorRun`. `OperatorRun::finish` polls and yields;
dropping the future kills and reaps the direct child and retains stderr. A
partially started `LocalServer` similarly kills and reaps the serving replica
and preserves its log. The ignored process test reaches a stalled tenant setup
after the first replica is serving and proves that cooperative in-binary path.
`Lifetime::bounded` now documents cooperative cancellation, surviving effects,
owner cleanup, and workflow-specific retry safety.

The remaining break is one owner upstream. Every shell layer uses GNU
`timeout --foreground`. The installed native tool contract explicitly states
that in foreground mode “children of COMMAND will not be timed out.” Therefore
the RustFS/setup command descendants, `cargo build`'s compiler children, the
Postgres wrapper's child shell, and especially `cargo run`'s `capacity` child
are not members of the timeout's enforced boundary. The RustFS and build
timeouts also have no `--kill-after`; the two timeouts that do have one can
hard-kill only their immediate command. `OperatorRun` begins too far downstream
to repair this outer ownership gap. A TERM-resistant or stuck descendant can
survive the advertised deadline, and a timed-out cargo/wrapper parent can exit
while benchmark-owned work remains alive.

Sibling consumers remain coherent. `QueueConfig::default` still supplies the
benchmark and AC-041 journey, direct and queued correctness remain test-owned,
the workload/report value flows audited in r3 are unchanged, and the lifetime
delta adds no production timeout, public contract, SLO, workload, or capacity
claim.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Revision-57 authority, task, and remediation identities agree | `spec.md:1-5`; task frontmatter; R2 frontmatter | Immutable candidate inspection | PASS |
| REQ-171 uses one command-start/deadline value before the first setup action and carries remaining budget into the binary | `mise.toml:511-548`; `main.rs:81-203,702-720` | Static producer-to-consumer trace; recorded shell dry run | PASS |
| Smoke and profile arguments survive the nested task/wrapper boundary | Mise appends task args after the final `_`; inner `$@` is inspected at `mise.toml:540` and forwarded at `mise.toml:548` | Same-shape temporary mise task showed inner `$@ = --profile --levels 20`; no repository file changed | PASS |
| AC-R2-1 / prior FIND-2: one enforceable absolute boundary owns the complete command process tree through exit | All stages use `timeout --foreground` at `mise.toml:527-548` | GNU `timeout --help` says foreground mode does not time out children; no process-tree proof exercises the task entry point | **FAIL (INV-R4-001)** |
| AC-R2-1 elapsed values use the command start rather than binary start | `Lifetime::from_command`; `setup_seconds` at `main.rs:499`; `total_seconds` at `main.rs:402`; report field meanings at `report.rs:375-383` | Lifetime unit test and source trace | PASS, subject to the unenforced outer tree above |
| AC-R2-2 migration/setup children yield to cancellation and are killed/reaped with diagnostics retained | `release_server.rs:130-207,636-732`; `LocalServer`/`OperatorRun` drop ownership | Recorded `a_stalled_tenant_setup_stops_the_run_by_its_deadline` result | PASS for direct in-binary children |
| AC-R2-2 no command-owned descendant survives deadline expiry | `--foreground` excludes descendants; `cargo run` supervises the actual capacity binary at `mise.toml:548` | The stalled-setup test invokes `Benchmark` directly and does not cross mise, the wrapper, Cargo, or outer timeout | **FAIL (INV-R4-001)** |
| AC-R2-3 shared cancellation/partial-progress behavior is explicit | `main.rs:232-265`; `release_server.rs:148-156,681-713` | Static rustdoc audit; recorded clean lints | PASS |
| AC-R2-4 preserves workload, SLOs, report schema, production behavior, and public surfaces | R2 delta is confined to benchmark task/lifetime/process harness, report field clarification, and review artifacts | Cumulative and remediation diff inspection | PASS |
| REQ-171 workload, step topology, evidence accounting, verdict, and report meanings remain coherent | `capacity/{load,fixture,evidence,step,report}.rs`; unchanged from the r3-audited owners except elapsed-field docs | Recorded 14-test capacity target plus r1/r3 focused evidence | PASS with recorded-evidence limit |
| AC-040 and AC-041 reference work, SLOs, and real-server correctness/durability proof remain intact | Benchmark workload/report owners; `observe_run.rs`, `drift_verification.rs`, and `pg_verification_runtime.rs` tests retained | Task and remediation record the focused results | PASS, with empirical default run deferred |
| Prior FIND-6 cancellation documentation closes without another abstraction | `Lifetime::bounded` is still the sole cooperative async cancellation owner and now states its contract | Static audit | PASS |
| Prior FIND-13 default unmodified qualification run | No new performance claim; remediation explicitly leaves it to integration | Caller-directed sequencing | DEFERRED — non-blocking |
| Non-goals: no new timeout knob, production limiter, supervisor framework, public API, workload, SLO, storage format, or capacity claim | Complete cumulative changed-path inspection | Static diff inspection | PASS |

## Proposed findings

### INV-R4-001 — INCORRECT — the outer command deadline does not own descendant processes

- **Prior finding:** `FIND-TASK-008-CLOSEOUT-2`, still open with a narrowed
  upstream cause.
- **Violated obligation:** revision-57 REQ-171 and remediation AC-R2-1/AC-R2-2
  require one enforceable 30-minute setup-to-exit boundary over the complete
  command process tree, with expiry leaving no command-owned child behind.
- **Exact location:** `mise.toml:511-548`, especially each
  `timeout --foreground` at lines 527, 529, 531, 542, 545, and 548. The
  downstream boundary that cannot cover those descendants is
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:232-265,590-612`.
- **Evidence:** GNU `timeout`'s installed contract states that in
  `--foreground` mode children of the command are not timed out. Thus the
  outer hard-stop applies to an immediate Docker/Cargo/wrapper process, not
  the child processes doing the work. The RustFS and build stages omit
  `--kill-after` as well. In particular, `cargo run` spawns `capacity`; expiry
  can terminate Cargo without terminating the benchmark binary. The new
  stalled-setup test starts `Benchmark` directly, so it proves
  `OperatorRun`/`LocalServer` cancellation but never exercises this reachable
  command boundary.
- **Observable consequence:** on deadline or interruption, a compiler,
  capacity binary, replica, or setup/storage descendant may keep running after
  its supervised parent or the mise task exits. If the descendant is stuck in
  synchronous cleanup or another non-yielding operation, neither the
  in-binary cooperative timer nor the outer foreground timeout forces it to
  exit by 30 minutes. This can retain ports, scopes, containers, and database
  activity beyond the supposedly finished run.
- **Required testable correction:** keep the single fixed timestamp/deadline
  and existing `Benchmark`, `LocalServer`, and Postgres-wrapper cleanup
  owners, but make the command boundary supervise and, after its grace,
  terminate the entire descendant tree for every pre-binary and binary stage.
  Preserve signal forwarding, diagnostic/report behavior, argument forwarding,
  and wrapper teardown; add no public knob or general supervisor. Extend the
  shortened process proof through the actual task/wrapper boundary with a
  controlled descendant that does not exit on the initial signal, and prove
  the descendant is gone, the command returns nonzero by the absolute bound,
  diagnostics survive, and no later stage starts. Retain the current
  in-binary stalled-setup proof for its narrower ownership contract.

## Prior-finding closure

| Prior finding | Invariant-review result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: only `bench:capacity` remains as a server-capacity entry point. |
| `FIND-TASK-008-CLOSEOUT-2` | OPEN as `INV-R4-001`: command timestamps and direct operator children are corrected, but outer foreground timeouts exclude descendants. |
| `FIND-TASK-008-CLOSEOUT-3` | CLOSED: judge wait remains separate from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | CLOSED: revision-57 authority and task metadata agree. |
| `FIND-TASK-008-CLOSEOUT-5` | CLOSED: `Benchmark` remains the cohesive lifecycle owner. |
| `FIND-TASK-008-CLOSEOUT-6` | CLOSED: `Lifetime::bounded` documents cooperative cancellation and partial progress. |
| `FIND-TASK-008-CLOSEOUT-7` through `FIND-TASK-008-CLOSEOUT-12` | CLOSED: r3's typed step, backlog, drain, report, resource-window, and test-owned correctness closures remain unchanged. |
| `FIND-TASK-008-CLOSEOUT-13` | DEFERRED by explicit caller sequencing to post-merge integration; not a candidate finding or blocker. |

## Verification notes and limits

- No Cargo or repository mise verification was started in this review, per
  the orchestrator's shared-checkout instruction. Recorded R2 evidence was
  inspected: 14 capacity tests passed with the process test skipped; the
  ignored stalled-setup proof passed when run explicitly; release-server tests,
  formatting, lints, and diff check were recorded clean.
- `git diff --check
  f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
  was clean in this review.
- Argument forwarding was checked with a temporary `/tmp` mise task using the
  same nested `bash -lc ... _ "$@"` shape; it did not modify the repository.
- The process proof does not invoke the actual `mise.toml` command, Postgres
  wrapper, Cargo parent/child path, or outer foreground timeouts. That missing
  reach is material because it is the exact boundary that fails.
- The real `--profile` path remains recorded-only evidence; no host profile was
  run here.
- The unmodified default benchmark remains intentionally deferred under
  `FIND-TASK-008-CLOSEOUT-13` and contributes no failure to this result.

## Overall result

**FAIL**

The candidate correctly shares one absolute timestamp/deadline, bounds the
cooperative in-binary workflow, owns migration/setup children, documents
cancellation, and preserves adjacent benchmark behavior. It does not yet own
the complete command process tree: its chosen foreground timeout mode
explicitly excludes child processes. Prior `FIND-TASK-008-CLOSEOUT-2` therefore
remains open at the outer command boundary; the separately deferred default
qualification run is not part of this failure.
