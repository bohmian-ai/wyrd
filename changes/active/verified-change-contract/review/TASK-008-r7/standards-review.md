# TASK-008 round-seven repository standards review

## Immutable subject and scope

- Repository:
  `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `345295d8e`
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`
- Reviewed range:
  `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
- Approved specification:
  `changes/active/verified-change-contract/spec.md`, revision 57
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation:
  `changes/active/verified-change-contract/review/TASK-008-r6/`

This is the user-directed closure review. I reviewed only whether the range
closes `FIND-TASK-008-CLOSEOUT-17` and
`FIND-TASK-008-CLOSEOUT-18`, and whether the range introduces a repository-rule
regression. Earlier accepted code was not reopened except where it could make
the benchmark report a false PASS or FAIL. `FIND-TASK-008-CLOSEOUT-13` remains
deferred to integration and supplies no empirical capacity qualification.

The checkout has no `.codegraph/` directory, so navigation used Git, `rg`, and
direct source inspection. Review was static only; the review orchestrator owns
all Cargo, `mise`, codegen, and database-backed verification for this shared
checkout.

## Navigation map

| Changed surface | Owner and role | Callers or consumers inspected | Proof inspected |
|---|---|---|---|
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs` | `Queue` owns durable benchmark evidence reads; new `Queue::poll` composes the replica/durable/replica observation interval; `Backlog` combines durable and process-local evidence | `Deployment::drain`; the in-source pure tests and `pg_tests`; report construction through `Scrapes` and `Record` | The held-chain-head public Oracle proof, pure pending-plus-staged arithmetic, and the prior R6 false-empty diagnosis |
| `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs` | `Deployment` owns serving replicas, their metrics scrapes, and the step drain lifecycle | `Deployment::run` is the sole production caller of `drain`; `drain` is the sole production caller of `Queue::poll` | Exact drain-decision source and existing drain-edge proof location |
| `.config/nextest.toml` | Repository-wide nextest execution policy | Default-profile `wyrd-testing` capacity target and the two named ignored systemd/stand-in proofs | Both named tests' `LocalServer`/stand-in source and the fixed replica-0 HTTP port in `release_server.rs` |
| R6 remediation record | Active task execution evidence for the bounded correction | Round-seven review and eventual completion review | Acceptance mapping, fixed-port failure diagnosis, recorded commands, and explicit FIND-13 deferral |
| Unchanged verification/configuration context | `wyrd-testing` manifest and `mise` own the capacity binary, family lanes, Bifrost server journey, lint policy, and opt-in benchmark | `Cargo.toml`, `mise.toml`, `TESTING.md`, release-server owner, capacity module root | Dependency/feature shape, target registration, environment wrappers, and canonical verification lanes |

## Authority coverage

| Changed surface | Applicable repository authority |
|---|---|
| All changed Rust and in-source tests | `AGENTS.md` sections 4-6, 11, 12, 15, and 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md` |
| Test tier placement, exact selectors, environment ownership, and nextest concurrency | `AGENTS.md` section 11; `architecture/agent-rules.md` test-boundary and gate-integrity rules; `TESTING.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` |
| Audit backlog observation and capacity drain reliability | Revision-57 `REQ-171`; original TASK-008; R6 verdict, validation, and R5 remediation; `AGENTS.md` audit rules; `architecture/references/architecture/patterns.md`; `architecture/references/domain/analytical-operations-reliability.md` |
| Spec/remediation evidence update | `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; approved revision-57 spec and ready R5 remediation task |
| Unchanged manifests and consumers used to establish scope | `crates/wyrd/wyrd-testing/Cargo.toml`; `mise.toml`; capacity `main.rs`, `step.rs`, `evidence.rs`, and `release_server.rs`; `.config/nextest.toml` |

No Python, TypeScript, PyO3, schema, OpenAPI, MCP catalog, migration, public API,
or generated-artifact surface changed, so their independent language and
codegen authorities are not applicable to this range.

## Applicable rule results

| Applicable rule | Exact source evidence | Result |
|---|---|---|
| A stateful, multi-step evidence workflow belongs on its concrete owner | `evidence.rs:211-215,307-341` places the replica/durable/replica poll on the existing dependency-owning `Queue`; `step.rs:406-416` supplies only `Deployment`'s replica scrape operation | PASS |
| Async is limited to real IO or intentional composition of IO | `Queue::poll` awaits the replica metrics callback and `Queue::backlog`; `Deployment::drain` awaits `Queue::poll`; the evidence arithmetic remains synchronous | PASS |
| Every new or materially modified Rust item has substantive rustdoc, including errors and cancellation where applicable | `evidence.rs:106-108` documents `AUDIT_PENDING`; `evidence.rs:307-323` documents `Queue::poll`, its ordering, errors, and cancellation; `evidence.rs:458-459,478-568` documents the `pg_tests` module and its helpers/test; `step.rs:383-394` remains accurate for the modified drain | PASS |
| Imports stay at module tops and signatures use repository-native types | Both `tests` and `pg_tests` have top-of-module import blocks (`evidence.rs:376-380,461-476`); the production change introduces no function-scoped import or new cross-tier type path | PASS |
| An otherwise-empty audit drain must bracket the durable snapshot and must not accept a false zero | `Queue::poll` takes the first scrape, queries durable backlog, and only for an otherwise-empty result takes a second scrape before returning (`evidence.rs:324-340`); `Deployment::drain` judges only that result (`step.rs:406-420`) | PASS; closes the repository-reliability part of FIND-17 |
| The returned final replica evidence must match the observation that permits a zero decision | The nonempty first result returns the first scrapes (`evidence.rs:336-337`); the otherwise-empty path returns the second scrapes with the second combined backlog (`evidence.rs:339-340`); `Deployment::run` stores that result as `Scrapes.after` (`step.rs:359-373`) | PASS |
| The focused proof must exercise the real false-empty interval without inventing a parallel audit mechanism | The test captures the first scrape before issuing a public Oracle read, waits until the canonical writer is blocked, then allows `Queue::poll` to take its durable read and second scrape (`evidence.rs:637-656`). It reuses `WyrdTestServer`, the public `Bifrost` query client, canonical staging, and `AuditPublisher`; no production audit owner changes | PASS under the integrator-approved Oracle proof narrowing |
| Tests requiring Postgres or a live server live under `mod pg_tests`/`pg_*`; ordinary tests stay environment-free | The WyrdTestServer/Postgres helpers and handoff proof now live under `#[cfg(test)] mod pg_tests` (`evidence.rs:458-708`); pure metrics/backlog/percentile tests remain in `mod tests` (`evidence.rs:376-456`) | PASS; closes FIND-18 |
| The environment-backed proof retains an explicit gate and exact environment-owned selector | The live proof remains `#[ignore]` with an environment reason (`evidence.rs:569-571`); the R5 execution record names the exact `evidence::pg_tests::...` selector under `scripts/postgres/with-test-postgres.sh` and migrations | PASS structurally; execution belongs to the orchestrator |
| Gate behavior must not be weakened to clear a failure | No assertion, timeout, ignore state, boundary check, or production behavior was weakened. The module move changes source classification only, and the new proof adds the second-scrape assertion (`evidence.rs:641-656`) | PASS |
| Shared ports/global resources should be serialized at the narrowest test boundary | Replica 0 has fixed HTTP port 8080 (`release_server.rs:29-34`); exactly the two capacity tests named in the override start the stand-in through `LocalServer` (`main.rs:829-905,919-980`). The new `release-server-ports` group has `max-threads = 1` and its filter selects only those exact tests (`.config/nextest.toml:62-70`) | PASS |
| Test-runner serialization must be justified by a real collision, not used to hide a product race | The configuration comment names the fixed-port collision; the R5 evidence records the observed `Address already in use` failure and paired reproduction. The tests use distinct nextest processes but the same host port, so process isolation cannot prevent the collision | PASS |
| Test/build changes must use the existing repository mechanism rather than add a new harness or dependency | The change reuses nextest's existing test-group pattern beside `embedded-postgres` and `peer-clusters`; no dependency, Cargo feature, test target, helper binary, or `mise` task was added | PASS |
| REQ-171's workload, SLO, report, deadline, and public/durable contracts must remain unchanged | The production diff is limited to evidence polling in `Queue`/`Deployment`; the only other executable change is nextest scheduling. `mise.toml`, manifests, workload, report, CLI, audit writer, publication, and deadline code are unchanged | PASS within the closure scope |
| Performance claims require the complete fixed workload/environment; configuration and focused checks are not qualification | The evidence record explicitly leaves the default `bench:capacity` run and AC-040/AC-041 qualification deferred under FIND-13 | PASS |
| Security, tenancy, secrets, public errors, SSRF, and generated contracts must not regress | The range changes no production authorization, audit write path, tenant SQL, credential handling, URL fetch, public error, wire/schema, SDK, or generated surface | NOT APPLICABLE / no regression |

## Material findings

None.

## Prior-finding closure and regression assessment

- `FIND-TASK-008-CLOSEOUT-17`: **CLOSED for repository standards.** The
  existing pre-query scrape and durable watermark query remain intact, and
  `Queue::poll` adds the required post-query observation only before accepting
  an otherwise-empty snapshot. The integrator-approved public Oracle test
  deterministically creates the decision after the first snapshot and before
  the durable query, so its assertion exercises the false-empty interval
  rather than a parser-only approximation.
- `FIND-TASK-008-CLOSEOUT-18`: **CLOSED.** Every environment-owned helper and
  the live Postgres/server proof now live in `pg_tests`; pure tests remain in
  the ordinary module, and the environment gate is retained.
- The narrow nextest group is a compliant test-infrastructure correction for
  two tests that bind the same fixed host port. It does not serialize product
  work, reduce assertions, or broaden unrelated test scheduling.
- No repository-rule regression was found in
  `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.

## Verification limits

- Static source, diff, manifest, `mise`, nextest configuration, caller, and
  test review completed.
- `git diff --check 345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
  passed during this review.
- Per orchestrator coordination, this reviewer ran no Cargo, `mise`, codegen,
  live-server, or database command. The implementation record reports the
  focused Postgres proof, pure focused tests, complete capacity target,
  release-server tests, Bifrost server journey, formatting, and lints passing;
  those results remain evidence for orchestrator reconciliation rather than
  independently rerun evidence from this standards pass.
- The full default `mise run bench:capacity` and empirical AC-040/AC-041
  qualification remain deferred as FIND-13 by caller direction.

## Overall result

**PASS**

Both scoped prior findings are closed under the applicable repository rules,
and the remediation range introduces no material standards regression.
