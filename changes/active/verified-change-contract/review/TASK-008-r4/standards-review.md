# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Complete range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review/remediation evidence inspected as hypotheses and evidence pointers:
  `review/TASK-008-r1/`, `review/TASK-008-r2/`, `review/TASK-008-r3/`, and
  `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`
- CodeGraph: not applicable; this checkout has no `.codegraph/` directory.

The candidate commit matched the requested identity before and after this
review. This report audits repository standards only; it does not decide task
acceptance or validate another reviewer's findings.

## Applicable authority

The complete applicable slices selected through
`architecture/references/README.md` were:

- `AGENTS.md`, especially §§1-6, 11-12, and 15-16;
- `architecture/agent-rules.md`;
- `architecture/bifrost-design.md`;
- `architecture/operations/README.md`;
- `architecture/operations/deployment-and-release.md`;
- `architecture/operations/reliability-and-recovery.md`;
- `architecture/operations/runbooks.md`;
- `architecture/references/architecture/patterns.md`;
- `architecture/references/languages/spec-driven-development.md`;
- `architecture/references/languages/implementation-execution.md`;
- `architecture/references/languages/maintainer-style.md`;
- `architecture/references/languages/rust-core.md`;
- `architecture/references/languages/testing-workflows.md`;
- `architecture/references/domain/telemetry-observations.md`;
- `architecture/references/domain/olap-serving.md`; and
- `architecture/references/domain/analytical-operations-reliability.md`.

Python, TypeScript, PyO3, schema-generation, migration, public-error, and
security/RBAC implementation authorities were checked for applicability. This
range changes no implementation in those surfaces. Existing language journeys
are evidence consumers, not changed language-runtime behavior.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Active task metadata and implementation evidence | `AGENTS.md` §§11-12, 14-16; spec-driven development; implementation execution; testing workflows | **FAIL** — the task names approved revision 57 and the current candidate work, but its current focused-command registry still names a deleted binary and records several named tests without full `mise exec --` commands (STD-R4-002). |
| Unified `capacity` benchmark (`main`, `load`, `step`, `evidence`, `report`, `fixture`, `judge`, `profile`) | `AGENTS.md` §§4-6, 11, 15-16; agent rules; Rust core; maintainer style; Bifrost and analytical reliability | **FAIL** — cohesive owners, typed step identity, bounded request permits, public-client traffic, and report semantics conform. The async cleanup path still performs synchronous process termination on a Tokio worker and the outer deadline does not own command descendants (STD-R4-001). |
| Release-process harness in `wyrd-testing/src/release_server.rs` | `AGENTS.md` §§6, 11, 16; agent rules; Rust core; deployment/release and reliability authorities | **FAIL** — `OperatorRun` improves migration/setup polling, credential output handling, cancellation, reaping, and diagnostics. `LocalServer::stop`, `LocalServer::terminate`, and both process-owner `Drop` paths still synchronously wait from the async benchmark lifecycle, so the claimed bounded boundary is not an explicit non-blocking process owner (STD-R4-001). |
| `mise.toml` benchmark command | `AGENTS.md` §§11-12, 15; testing workflows; implementation execution; deployment/release | **FAIL** — it fixes one wall-clock value before setup and gives phases remaining budgets, but `timeout --foreground` explicitly does not time out command children; several calls also have no kill-after. This cannot be the process-tree bound the comments and evidence claim (STD-R4-001). |
| `wyrd-testing/Cargo.toml` benchmark consolidation | `AGENTS.md` §§3-4, 11-12, 15; agent feature/dependency rules; testing workflows | **PASS** — one capacity binary remains, no new dependency or Cargo feature was added, and the opt-in benchmark is not added to a fast or aggregate gate. |
| Queue default documentation | `AGENTS.md` §§4, 16; Rust core documentation rules | **PASS** — the edit only points existing default rationale to the unified benchmark and changes no queue behavior. |
| Postgres-backed verification runtime tests | `AGENTS.md` §11; agent external-test/Postgres-lane rules; testing workflows | **PASS** — the existing external target is warranted by Postgres and runtime composition; the new tests document why claim exclusivity and claim order require this supporting seam rather than a process journey. |
| Rust SDK real-server journeys | `AGENTS.md` §§8-12, 16; architecture patterns; testing workflows; Bifrost/OLAP authority | **PASS** — changed tests exercise public Rust SDK paths against a real server, stay in the Rust runtime, assert durable read-back, and use the repository's established ignored journey convention with explicit run-ignored commands. |
| Removed benchmark source trees | `AGENTS.md` §§11-12, 15; testing workflows; implementation execution | **PASS** — superseded entry points are deleted and required correctness assertions have focused test owners. No production contract, generated artifact, or current gate was removed. |

## Applicable rule results

| Repository rule | Source evidence | Result |
|---|---|---|
| Review and implementation use the exact approved spec revision and immutable candidate. | `spec.md:1-4` is approved revision 57; task frontmatter names revision 57; Git remained at `8022436387f3a9a9499527ebf8b8b8140c6559cb`. | **PASS** |
| Stateful and multi-step Rust workflows have cohesive concrete owners. | `capacity/main.rs:134-266` (`Lifetime`), `277-655` (`Benchmark`); `capacity/step.rs` (`Deployment`); `release_server.rs:112-503` (`LocalServer`); `636-732` (`OperatorRun`). | **PASS** |
| Closed behavior uses typed exhaustive shapes. | `capacity/step.rs` owns step identity with `StepKind`; workload operation/kind sets are enums and fixed arrays. | **PASS** |
| Async workflows await IO and do not block Tokio workers; external work has enforceable bounds. | `capacity/main.rs:590-610` synchronously calls `LocalServer::stop`; `release_server.rs:373-405` loops with `std::thread::sleep` and blocking process waits. `mise.toml:527-548` uses `timeout --foreground`; the installed GNU tool documents that children of the command are not timed out in this mode. | **FAIL — STD-R4-001** |
| Async/process documentation describes true cancellation and partial progress. | `Lifetime::bounded`, `Benchmark` stages, `LocalServer::start`, and `OperatorRun::finish` now have substantive cancellation sections. Their claim that the whole command is bounded, however, is contradicted by the blocking/unowned descendant paths above. | **FAIL — STD-R4-001** |
| Every new/materially modified Rust item has substantive rustdoc and fallible operations have `# Errors`. | Static inspection of all added benchmark modules, test items, `OperatorRun`, and changed public fields found intent and error/panic documentation. | **PASS**, except for the behavioral mismatch captured by STD-R4-001. |
| Secrets and diagnostics do not disclose credentials. | `release_server.rs:639-678` sends setup stdout to an unlinked file, records only program/arguments in diagnostics, and keeps credentials in `SecretString`; environment values are excluded from the formatted command. | **PASS** |
| Concurrency, queues, fan-out, and benchmark demand are bounded. | `load.rs` uses a shared finite semaphore; WyrdState uses the default bounded client queue; per-step capture/task ownership is joined; Bifrost evidence reads concrete owner metrics. | **PASS** |
| Client/test code does not become a second durable owner. | The benchmark drives `Verification`, `WyrdState`, and `Bifrost`; its SQL is read-only evidence collection. Gate, Scribe, Oracle, Forge, audit, and verification durable behavior remain server/Vala owned. | **PASS** |
| Raw Postgres pools do not enter library contracts. | Benchmark SQL pool usage is private to the opt-in binary; no exported library field or signature adds `PgPool`. Runtime tests use their established fixture boundary. | **PASS** |
| Imports are module-scoped and signatures use imported bare names. | Changed Rust modules retain top-of-module imports and bare type names. | **PASS** |
| External tests earn separate targets and service-dependent proof uses repository-managed lanes. | Runtime tests use the existing Postgres target; SDK tests use real `WyrdTestServer`; the stalled-process test is isolated in the capacity binary and documents its systemd/python requirements. | **PASS** |
| A gate is not weakened by an allow, deleted assertion, or hidden skip. | The new ignored tests have explicit environment reasons and recorded `--run-ignored` commands; removed benchmark assertions reappear in focused runtime/journey tests. No new `#[allow]` was added. | **PASS** |
| Every named test in an active task/evidence record carries an exact `mise exec --` command with package, target, features, and exact selector. | `task-008-closeout.md:725-731` still names deleted target `verification_capacity`; `:1501-1504` uses raw `cargo nextest` or selector fragments for named tests. | **FAIL — STD-R4-002** |
| Bifrost metrics preserve their concrete owner and documented meaning. | `capacity/evidence.rs` reads Scribe staging, Oracle/verification histograms, audit watermark gap, Forge demand, and cgroup resources separately; report code does not turn metrics into durable authority. | **PASS** |
| Performance evidence is tied to fixed workload/environment and configuration alone is not evidence. | Report envelope records binary/resource/workload identity; full default qualification remains explicitly deferred. Reduced smoke and focused correctness tests are not represented as final capacity qualification. | **PASS** |
| Generated artifacts, public contracts, migrations, dependencies, and features are unchanged unless approved. | Cumulative path and manifest inspection shows no generated/public schema, migration, dependency-version, or feature addition. | **PASS** |

## Material findings

### STD-R4-001 — The command lifetime still crosses blocking and unowned descendant processes

- **Violated rules:** `AGENTS.md` §6 requires narrow async boundaries and an
  explicit blocking strategy; `architecture/references/languages/rust-core.md`
  says not to block a Tokio worker and to bound external calls;
  `architecture/agent-rules.md` requires documentation to describe the real
  lifecycle; operations authority requires shutdown to drain/stop owned work
  before process exit.
- **Locations:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:581-613`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:365-405,487-501,693-730`;
  `mise.toml:511-548`.
- **Evidence:** `Benchmark::clean_up` is async but directly invokes
  `replica.stop`. That function calls `terminate`, which polls with
  `std::thread::sleep` for as long as 45 seconds and then calls blocking
  `Child::kill`/`wait`; the process-owner `Drop` implementations also call
  blocking `wait` when a deadline drops their future. At the command boundary,
  every phase uses `timeout --foreground`. GNU `timeout --help` for the
  installed tool states that in foreground mode children of the command are
  not timed out. The RustFS setup and release-build calls also omit
  `--kill-after`; killing `cargo run` or a wrapper is therefore not proof that
  its benchmark/server descendants exited. The focused stalled-setup test
  drives `Benchmark` directly and proves a cooperative direct child; the
  recorded shell proof stubs commands and does not exercise descendant
  ownership.
- **Consequence:** a wedged server stop, Cargo child, wrapper descendant, or
  foreground setup child can block a Tokio worker or survive the process that
  the timeout signalled. The advertised complete setup-to-exit limit and clean
  process ownership are therefore not repository-standard guarantees, and a
  timed-out run can leave benchmark/server work alive or omit final cleanup and
  report evidence.
- **Testable correction:** keep `Benchmark`/`LocalServer` as owners, but move
  synchronous server termination/reaping behind an explicit blocking boundary
  (or use a non-blocking process owner) and make the outer command supervise the
  complete descendant process group with a finite TERM-to-KILL path. Preserve
  interactive signal handling without selecting a timeout mode that excludes
  children. Add a shortened command-level process-tree test whose descendant
  ignores TERM and prove the task kills/reaps the descendant, exits nonzero,
  and retains diagnostics within the one absolute deadline; retain the existing
  in-binary stalled-setup proof.

### STD-R4-002 — The active task's focused verification commands are stale and noncanonical

- **Violated rules:** `AGENTS.md` §11, `architecture/agent-rules.md`,
  `spec-driven-development.md`, and `testing-workflows.md` require every named
  Rust test in a task or implementation record to have an exact repository-
  pinned `mise exec -- cargo nextest run` command with explicit package, target,
  feature set, and exact expression. A positional or partial selector must not
  be capable of selecting no test.
- **Location:**
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:716-731,
  1498-1504`.
- **Evidence:** the section labelled "confirmed in the current
  source/manifest" still uses `--bin verification_capacity`, although this
  candidate deletes that target, and the next paragraph itself instructs the
  implementer to update it after the rename. The current implementation matrix
  then records raw `cargo nextest` for `mix_offers_the_required_rates` and
  `queries_read_the_last_five_minutes`, and only detached `-E` fragments for
  the report/evidence tests. No complete current exact command accompanies
  those named tests in the active task.
- **Consequence:** the tracked task packet cannot reproduce its own claimed
  proof: two prescribed commands address a nonexistent target, while the
  abbreviated entries can be copied without the repository toolchain or even
  without selecting the intended test. This defeats the required durable
  verification record even if a broader capacity target run was green.
- **Testable correction:** replace the deleted-target anchors and every raw or
  detached named-test entry with the actual `capacity` target and a complete
  `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
  'test(=<exact-name>)'` command. Confirm names through the repository-pinned
  nextest listing and record the focused results without weakening them to a
  whole-target-only claim.

## Verification reviewed and limits

- `git diff --check
  f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
  was clean.
- Static inspection covered the complete cumulative path set, all current
  capacity modules, the release-process harness, manifest/task wiring, changed
  server integration tests, changed SDK journeys, their surrounding owners,
  and prior remediation evidence.
- Recorded evidence reviewed includes the capacity target (14 passed, one
  environment-gated test skipped), the explicitly run stalled-setup test,
  release-server tests, format, workspace lints, focused Postgres/runtime
  tests, SDK journeys, and the shell shim dry run.
- Per orchestrator direction, no Cargo or `mise` verification was started
  concurrently from this reviewer. The installed `timeout --help` was inspected
  to validate the exact semantics of `--foreground` used by the task.
- `FIND-TASK-008-CLOSEOUT-13`, the unmodified full default benchmark, is
  **DEFERRED to integration** after the other workstreams merge and is not a
  blocker or repository-standard finding for this candidate.

## Overall result

**FAIL**

The candidate closes the prior migration/setup polling and cancellation-
documentation defects, and the cumulative Rust/test/measurement structure is
otherwise repository-conformant. Two material standards failures remain: the
complete command still lacks a non-blocking, descendant-owning deadline
boundary, and the active task packet does not provide valid exact commands for
several named tests it claims as proof.
