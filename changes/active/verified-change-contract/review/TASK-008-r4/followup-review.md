# Focused follow-up review

## Immutable subject and inspected paths

- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Remediation authority: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`
- Discovery reports: every Markdown report present under `review/TASK-008-r4/`
- Runtime source: `mise.toml:507-549`,
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:379-425,581-613`,
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:371-383`,
  `crates/wyrd/wyrd-testing/src/release_server.rs:365-405,487-501,681-730`, and
  `scripts/postgres/with-test-postgres.sh:11-40,104-123`
- Standards authority: `AGENTS.md` sections 6 and 11,
  `architecture/agent-rules.md:17`, and
  `architecture/references/languages/rust-core.md:473-477`
- Task provenance: the cumulative diff and blame for
  `task-008-closeout.md:716-731,1490-1504`

The candidate still resolved to the named commit after inspection. This pass
ran no Cargo or mise lane. `FIND-TASK-008-CLOSEOUT-13` remains deferred to the
post-merge integration sequence and is not a finding or blocker here.

## Uncertainty 1: report duration versus the setup-to-exit interval

### Evidence

The approved specification says the default command, including setup, must
complete within 30 minutes (`REQ-171`). The later remediation is more precise:

- its Outcome includes diagnostic/report finalization and process exit in the
  setup-to-exit boundary;
- its recommendation says the reported elapsed value must describe the same
  command-wide interval claimed by REQ-171; and
- AC-R2-1 says command exit, including cleanup and report finalization, occurs
  within that boundary and that the report's total duration covers the same
  interval.

Current source samples `total_seconds` at `capacity/main.rs:402`. The sample is
then followed by binary identity collection, report serialization and writes,
console rendering, return from the capacity process and Cargo, and Postgres
Compose teardown by the outer wrapper. No already-serialized value can include
those later durations. This remains true even if the outer process-tree defect
is corrected perfectly; process ownership determines whether the tail is
bounded, not whether an earlier timestamp records it.

The invariant review's result that the start point is correct "subject to the
unenforced outer tree" therefore does not resolve the end-point requirement.
The start producer is correct, but the sink closes the interval too early.

### Resolution

Retain a **separate proposed finding** corresponding to `SYS-R4-002`. It is a
reachable, bounded violation of AC-R2-1, not merely a consequence of the outer
process-tree finding. Validation should keep it distinct from the continuing
`FIND-TASK-008-CLOSEOUT-2` process-ownership defect while requiring one
coherent correction at the externally owned command/report boundary. The
closure proof must delay report finalization or wrapper teardown and show that
durable completion evidence and the enforced setup-to-exit interval agree.
Changing only the field documentation or adding another pre-exit sample does
not close the approved requirement.

## Uncertainty 2: stale and abbreviated commands in the active task

### Evidence

The deleted-target commands at `task-008-closeout.md:725-726` were not added or
modified in the cumulative candidate. They belong to the older planning body,
and the task's current-contract notice explicitly classifies the revision-49
planning notes as history. Their adjacent prose also says that the anchors must
be updated after the rename and that the obsolete binary must not be retained.
Those two lines are therefore historical input, not current executable proof;
they should not independently support a material finding.

The current revision-57 implementation record at
`task-008-closeout.md:1490-1504` is different. It was added by this candidate,
is expressly named as the current contract by the task's opening notice, marks
the affected rows `PASS`, and is the candidate's verification evidence. At
line 1501 it records raw `cargo nextest` rather than the required
repository-pinned `mise exec -- cargo nextest run` commands. At lines
1501, 1503, and 1504 it also uses subsequent detached `-E` fragments rather
than complete commands. `AGENTS.md` section 11 and
`architecture/agent-rules.md:17` require every specifically named Rust test in
a task artifact or implementation report to carry and run an exact command
with package, target, applicable features, and exact expression.

### Resolution

Retain `STD-R4-002` only in **revised, narrowed form**. Its material current
location is `task-008-closeout.md:1498-1504`; do not cite the explicitly
historical deleted-target anchors at lines 725-726 as a present executable
obligation. The current PASS evidence is non-reproducible under the repository's
mandatory command form and was introduced in the reviewed range, so a separate
task-evidence correction is warranted. The smallest correction is documentary:
replace each current raw or detached named-test entry with its complete exact
`mise exec -- cargo nextest run --locked ... -E 'test(=<exact-name>)'` command
and retain the recorded result. No production or test behavior belongs in this
finding.

## Uncertainty 3: synchronous replica stop in async cleanup

### Evidence

`Benchmark::clean_up` is async because it first awaits client shutdown. It then
calls `LocalServer::stop` directly on the Tokio worker for every replica.
`stop` calls `terminate`, which can poll with `std::thread::sleep` for the full
45-second `STOP_GRACE`, then performs blocking child kill/wait and filesystem
copy. The path is reached during every normal benchmark cleanup, not only as a
drop fallback. The candidate materially introduced the current
`Benchmark::clean_up` owner and its direct call, even though the synchronous
`LocalServer` primitive existed at the base.

`architecture/references/languages/rust-core.md:476-477` states that blocking
work must not block an async worker. The current rustdoc accurately calls the
stop synchronous and bounded, but documentation does not supply the required
blocking boundary. The outer `timeout --foreground` defect neither causes nor
repairs this path: a corrected shell process group would still leave normal
in-binary cleanup sleeping and waiting on a Tokio worker.

The blocking `Drop` waits in `LocalServer` and `OperatorRun` are adjacent but
should not broaden this finding without validation of their exact unavoidable
drop/cancellation semantics. The directly awaited normal cleanup path is
sufficient and independently reachable.

### Resolution

Retain a **distinct, narrowed repository-standards proposal** for the direct
`Benchmark::clean_up` -> `LocalServer::stop` -> `terminate` path. Do not merge
its correction into the outer process-tree finding merely because discovery
reported both as `STD-R4-001`. Validation should split the concerns: the prior
`FIND-TASK-008-CLOSEOUT-2` continuation owns shell descendant lifetime, while
this standards proposal owns the explicit async/blocking boundary for normal
replica shutdown. The smallest correction keeps `Benchmark` and `LocalServer`
as owners and routes the existing synchronous stop/reap/copy operation through
an explicit bounded blocking strategy (or an equivalent non-blocking process
owner), preserving stop order, `STOP_GRACE`, logs, and shutdown evidence.

## New-proposal summary

| Discovery claim | Follow-up disposition | Relationship |
|---|---|---|
| `SYS-R4-002` | RETAIN | Separate AC-R2-1 end-of-interval defect; independent of outer process-tree ownership. |
| `STD-R4-002` | REVISE AND RETAIN | Historical lines 725-726 are excluded; current candidate-added revision-57 evidence at lines 1498-1504 remains a material exact-command violation. |
| Blocking portion of `STD-R4-001` | SPLIT AND RETAIN | Normal async cleanup blocks a Tokio worker and survives correction of the outer process-tree defect; validate as its own standards finding. |

## Result

**RESOLVED**

Approved authority and current source resolve all three uncertainties without
a specification revision. Each retained or revised proposal remains subject to
the required independent Ponytail validation; this follow-up does not assign
stable finding IDs or decide the task verdict.
