# Repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Remediation reviewed: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

The candidate remained at the named commit throughout this review. CodeGraph is
not indexed in this checkout, so navigation used Git, `rg`, and direct source
inspection. Prior reviews were treated as hypotheses and evidence, not as this
review's conclusions. The caller's two fixed decisions were honored:
`FIND-TASK-008-CLOSEOUT-13` is deferred to integration and non-blocking, and a
report written before teardown is not required to include post-report teardown
in its elapsed total.

## Authority coverage

| Changed surface | Owning authority and applicable references | Coverage result |
|---|---|---|
| Consolidated `capacity` benchmark, load generation, evidence, report, steps, fixtures, local TLS judge, and profiling | `AGENTS.md` §§3-6, 10-12, 15-16; `architecture/agent-rules.md`; `architecture/references/languages/{rust-core,testing-workflows,implementation-execution,maintainer-style}.md`; `architecture/references/domain/{olap-serving,analytical-operations-reliability}.md`; `architecture/bifrost-design.md`; `architecture/operations/reliability-and-recovery.md` | **PASS** — one opt-in owner drives public clients against release servers, keeps demand and task fan-out bounded, reads owner-specific evidence without becoming a durable owner, and documents every new Rust item and lifecycle boundary. |
| Release-server process lifecycle and operator subcommands | `AGENTS.md` §§5-6, 12, 16; `architecture/agent-rules.md`; Rust core; deployment/release and reliability/recovery operations authority | **PASS** — `LocalServer` and `OperatorRun` retain concrete ownership; polling yields in async code, synchronous termination remains bounded, and normal replica stop now runs through `spawn_blocking` at `capacity/main.rs:649-676`. |
| `bench:capacity` task process groups, absolute deadline, RustFS/Postgres/build/run orchestration | `AGENTS.md` §§11-12, 15-16; agent rules; testing workflows; implementation execution; deployment/release operations authority | **PASS** — `mise.toml:507-557` fixes one deadline before setup, gives every phase a separate descendant group, forwards interruption, applies TERM-to-KILL escalation, sweeps survivors, and preserves wrapper teardown opportunity. |
| `wyrd-testing` target/dependency manifest and retired benchmark entry points | `AGENTS.md` §§3-4, 11-12, 15; agent rules; testing workflows | **PASS** — `Cargo.toml:145-151` has one earned opt-in `capacity` binary, adds no feature or dependency, and removes superseded binaries without weakening a required gate. |
| Queue-default documentation | `AGENTS.md` §§3-4, 12, 16; Rust core; analytical reliability | **PASS** — `wyrd-queue/src/config.rs:100-113` changes only the benchmark reference; the bounded default and validation contract are unchanged. |
| Verification-runtime Postgres integration tests | `AGENTS.md` §§11-12, 16; agent external-test/Postgres-lane rules; testing workflows; verification runtime authority in `wyrd-design.md` | **PASS** — `pg_verification_runtime.rs:1267-1401` uses the existing earned external target, composes the actual runtime over repository-managed Postgres, and explains why claim exclusivity and claim order are integration seams rather than client journeys. |
| Rust SDK direct-verification and sustained-observation journeys | `AGENTS.md` §§3, 8-12, 16; architecture patterns; testing workflows; Bifrost and analytical reliability authority | **PASS** — the changed ignored tests use the public Rust SDK against a real server, assert judgments and durable read-back, carry explicit environment reasons, and have exact repository-managed execution commands. |
| Active task and remediation evidence | Spec-driven development; implementation execution; `AGENTS.md` §11 exact-command rule | **PASS** — `task-008-closeout.md:1498-1504` now gives each of the six named capacity tests a complete pinned exact command; remediation evidence at `TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md:274-280` records exact process-tree, cleanup, target, format, and lint proof. |

No Python, TypeScript, PyO3, public schema, OpenAPI, MCP catalog, migration, UI,
or generated artifact changed, so their independent authorities do not apply to
this cumulative task diff.

## Applicable rule results

| Repository rule | Exact source or verification evidence | Result |
|---|---|---|
| The review uses the approved revision and immutable cumulative range. | `spec.md:1-4` is approved revision 57; task frontmatter names revision 57; candidate stayed `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`. | **PASS** |
| Stateful and multi-step Rust workflows have cohesive concrete owners; free functions remain narrow. | `capacity/main.rs:134-266` (`Lifetime`), `:277-647` (`Benchmark`), `capacity/step.rs:39-513` (`Deployment`), `release_server.rs:112-503` (`LocalServer`), and `:636-731` (`OperatorRun`). `stop_replicas` is a narrow blocking-boundary adapter. | **PASS** |
| Async code directly awaits IO/composition and does not block a Tokio worker. | `capacity/main.rs:649-676` moves synchronous stop/reap/log-copy work to `tokio::task::spawn_blocking`; `release_server.rs:681-713` polls child state with an async timer. Recorded `a_slow_replica_stop_leaves_the_runtime_free` proof passed. | **PASS** |
| External work, queues, and fan-out are bounded and cancellation/partial progress are explicit. | `capacity/main.rs:134-265` partitions the fixed deadline; `mise.toml:526-540` bounds whole process groups; `load.rs:273-331` uses the shared finite semaphore and joins request tasks; async workflow rustdoc states cancellation effects. | **PASS** |
| Command shutdown owns descendants and does not leave work after return. | `mise.toml:511-556` uses non-foreground GNU `timeout` process groups, finite escalation, and a final group sweep. Exact ignored proofs for pre-wrapper setup and nested benchmark descendants are recorded at remediation lines 274-280. | **PASS** |
| Rust errors, imports, secret handling, and diagnostics follow repository rules. | Benchmark fallibility stays binary-local; imports are module-scoped; setup stdout containing credentials is unlinked and environment values are excluded from `OperatorRun` diagnostics (`release_server.rs:636-678`). No new production `allow`, unsafe escape, wildcard version, or per-crate profile exists. | **PASS** |
| Every new or materially modified Rust item has substantive rustdoc, with `# Errors`, `# Panics`, and cancellation/partial-progress sections where applicable. | Complete inspection of all current capacity modules, changed release-server items, runtime tests, and SDK journeys found intent and lifecycle documentation. Representative lifecycle docs are `capacity/main.rs:649-662`, `:1018-1028`, and `release_server.rs:636-692`. | **PASS** |
| Raw Postgres pools do not enter library APIs or replace tenant/server owners. | `capacity/evidence.rs:186-205` keeps its owner pool private to the opt-in binary for read-only cross-tenant benchmark evidence. No library signature or server durable path receives a raw pool; runtime tests retain fixture-owned pools. | **PASS** |
| External tests earn their targets and service-dependent tests use repository-managed lanes. | Runtime additions stay in the existing Postgres target; SDK additions stay in existing real-server journey targets; process tests are environment-gated and have explicit `--run-ignored` commands. | **PASS** |
| No gate is weakened to obtain a pass. | Deleted benchmark entry points are replaced by one approved benchmark plus focused runtime/SDK coverage. New `#[ignore]` annotations name required systemd/Postgres environments and the evidence explicitly runs them; no assertion, test, lint, or boundary check is disabled. | **PASS** |
| Every named capacity test in current evidence has a complete exact pinned command. | Six independent commands appear at `task-008-closeout.md:1501-1504`; three remediation tests and broader target/library commands appear at remediation lines 276-280. A fresh combined exact selector run executed the six capacity tests: 6 passed. | **PASS** |
| Performance claims remain tied to fixed workload/environment; configuration alone is not represented as qualification. | The report envelope records binary, resources, tenants, levels, windows, replicas, permits, and limit; task evidence labels the short run a smoke run. The unmodified default run is explicitly deferred and no empirical final capacity claim is made. | **PASS** |
| Generated artifacts, public contracts, migrations, dependency versions, and features change only when required and verified. | Cumulative manifest and path inspection found no such change. `git diff --check` is clean and the worktree has no formatter drift. | **PASS** |

## Review findings

No material repository-rule violations were found.

The prior process-boundary standards defect is closed by the descendant-group
supervision in `mise.toml:526-556`; the prior Tokio-worker defect is closed by
`capacity/main.rs:649-676`; and the six current capacity commands are exact and
reproducible. The integrator-rejected post-report teardown requirement is not a
repository-rule finding.

## Verification notes

- Fresh in this review:
  - `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
    'test(=load::tests::mix_offers_the_required_rates) |
    test(=load::tests::queries_read_the_last_five_minutes) |
    test(=report::tests::every_slo_failure_fails_the_step) |
    test(=report::tests::verdict_needs_every_verdict_step) |
    test(=evidence::tests::quantile_reads_bucket_deltas) |
    test(=evidence::tests::raw_percentiles_use_nearest_rank)'`: 6 passed.
  - `mise run fmt`: passed and left the worktree clean.
  - `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E
    'test(/release_server::/)'`: 2 passed.
  - `git diff --check
    f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`:
    passed.
- Recorded candidate evidence reviewed: complete capacity target 14 passed / 4
  skipped; three exact ignored lifecycle/process tests passed; `mise run
  lints`, format, release-server tests, Postgres runtime tests, and Rust SDK
  journeys passed.
- `FIND-TASK-008-CLOSEOUT-13`, the full unmodified default benchmark, is
  **DEFERRED** to integration after the other workstreams merge by caller
  authority. It is a residual qualification gap, not a blocker or standards
  failure for this candidate.

## Overall result

**PASS**

The cumulative candidate covers every changed language and layer with the
applicable repository authority, closes the prior standards findings, and has
no material repository-rule violation. Only the explicitly deferred full
default capacity qualification remains outside this candidate review.
