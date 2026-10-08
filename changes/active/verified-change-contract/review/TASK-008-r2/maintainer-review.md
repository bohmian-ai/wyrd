# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Range: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`
- Governing maintainer authority: `AGENTS.md` §§5, 6, 11, and 16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,maintainer-style,rust-core,testing-workflows}.md`; and the applicable verification-capacity contract in `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the active change specification.

The candidate remained at the named commit throughout this audit. CodeGraph is
not available in this checkout, so callers and consumers were traced with the
repository source and `rg`.

## Authority blocker

The user named approved specification revision 58. The immutable candidate's
`changes/active/verified-change-contract/spec.md` frontmatter is instead
`revision: 57`, and its latest revision-history entry is **Revision 57 one
capacity benchmark (2026-10-03, approved)**. Repository search finds no
revision-58 specification text in this change packet. The task frontmatter and
the prior remediation likewise bind themselves to revision 57.

`wyrd-task-review` requires the approved specification as part of the immutable
subject. Revision 57 cannot be silently substituted for the explicitly named
revision 58, so this maintainer review cannot return a conclusive PASS or FAIL
against the requested authority. The source audit below is complete against the
committed revision-57 packet so the authority mismatch does not conceal code
maintenance issues.

## Changed-surface coverage

| Surface | Symbols and consumers followed | Maintainer assessment |
|---|---|---|
| Change contract and evidence | `spec.md` frontmatter/history, `task-008-closeout.md` frontmatter and revision-57 implementation record, prior verdict, remediation task, and candidate commit | The task is internally aligned to committed revision 57 and preserves older planning as marked history. The absent requested revision 58 blocks authoritative acceptance. |
| Capacity entry point and lifecycle | `main.rs::{Cli, Lifetime, Benchmark, main}` through `Benchmark::{prepare, run, measure, provision, scale_out, reconnect, step, clean_up}`, `Report::write_to`, `Deployment::run`, and the `bench:capacity` mise task | The former free setup/reconnection workflows now live on the meaningful `Benchmark` owner and `main` is thin. `Lifetime` has one misleading configuration/diagnostic seam (MR-002). |
| Deployment and step model | `step.rs::{Deployment, StepKind, Plan, ResourceWindow, Drain, Record}` through sequence construction in `Benchmark::measure`, profile directory naming, `Report::{verdict_steps, passed}`, and report tests | `StepKind` closes the stringly step identity from r1. `Plan` nevertheless retains two independently writable booleans derived from that identity and deployment shape (MR-001). Resource-window and drain ownership are otherwise local and followable. |
| Workload and client lifecycle | `load.rs::{Op, Work, Lane, TenantClients, Request, Outcome}` through `mix`, every `Deployment::run` lane, cleanup, and load tests | Closed sets are enums; client connection, flush, shutdown, request dispatch, and cancellation effects are documented on their owners. The three parallel client vectors remain private and are populated together, so changing them to another shape would be preference rather than a material correction. |
| Fixture and judge | `fixture.rs::{Kind, Tenant}` provisioning/seeding paths and `judge.rs::{Judge, TlsListener}` through server environment construction and per-step wait collection | Both modules have coherent owners. Provider wait is captured on `Judge` and reaches `Record` and the report without a second telemetry framework. Cancellation and durable partial-progress documentation is substantive on the changed workflow boundaries. |
| Evidence and backlog | `evidence.rs::{Percentiles, Scrapes, scribe_backlog, Backlog, Queue}` through release-server metrics, database queries, `Deployment::drain`, and focused tests | The Scribe calculation now includes the production staged-live-members gauge. The DB reader remains explicit about cross-tenant benchmark ownership. `Metrics::parse` was made public for a focused consumer test; because `wyrd-testing` is the owning test-harness crate and the type was already its public scrape result, this is noted for calibration rather than blocked. |
| Report | `report.rs::{Cell, Row, step_row, op_row, Report, HEADER, table_row}` through Markdown/JSON output and all report tests | One shared table now renders each step immediately before its operation rows, with stable cell meanings and `n/a` where a cell does not apply. Verdict selection uses `StepKind`; the remaining invalid `Plan` flag combinations are covered by MR-001. |
| Release-server harness | `release_server.rs::{LocalServer, STOP_GRACE, MemoryPeak, Metrics}` through capacity startup, resource sampling, cleanup reservation, and existing harness tests | The exported stop bound is the same bound `LocalServer::stop` enforces. Process lifecycle remains on `LocalServer`; no parallel process abstraction was added. |
| Command and target retirement | `mise.toml`, `wyrd-testing/Cargo.toml`, deleted `bifrost_query_capacity/*`, and non-change-packet repository references | The obsolete runnable target, command, and four-file implementation are removed. Remaining names are historical evidence inside `changes/`, not competing executable entry points. |
| Focused proof | Fourteen capacity binary tests; prior two-replica claim/fairness tests; Rust SDK sustained-ingest and direct/queued judgment journeys; reduced two-replica smoke evidence | Tests are named for caller-visible outcomes and exercise the new pure decision boundaries. No generated declarations apply. The unmodified default benchmark was not run in the remediation evidence, and revision-58 proof cannot be assessed because that authority is absent. |

## Prior maintainer-finding closure

| Prior finding | Current evidence | Result |
|---|---|---|
| r1 MR-001: state-owning reconnection workflow outside `Deployment` | `main.rs:191-225,227-574` introduces `Benchmark`, which owns the deployment and authentication pacing; `Benchmark::reconnect` performs the transition and `main` only prepares/runs it | **CLOSED**. The chosen owner holds the complete lifecycle state; moving only reconnection back to `Deployment` would split the pacing dependency again. |
| r1 MR-002: verdict step identity is an unchecked string | `step.rs:58-93` defines `StepKind`; sequence construction, profile paths, serialization, and `Report::verdict_steps` use variants/`label()` | **CLOSED** for the string identity. MR-001 below is a separate remaining invalid-state problem in adjacent `Plan` flags. |

## Material findings

### MR-001 — `Plan` duplicates step semantics in independently writable booleans

- Changed location: `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:95-121` (`Plan::{name, judged, sample_floor}`), constructed at `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:368-393`, and consumed at `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:196-203,554-562`.
- Governing principle: `AGENTS.md` §5 requires enums for closed sets where exhaustiveness matters; `rust-core.md` requires APIs that make invalid states difficult to represent; `maintainer-style.md` requires argument and return shapes to expose the real contract.
- Evidence: `Plan::new` derives `judged` from `StepKind`, but all three fields are public within the binary crate and the one-replica sustained step overwrites `sample_floor` with struct-update syntax. Code can therefore compile a judged warmup, an unjudged ramp, or a one-replica sustained record with no sample floor. `Report::result` and `overhead` trust those booleans instead of the closed step identity and replica count that already determine them.
- Concrete maintenance cost: adding or rearranging a step requires a maintainer to keep three representations of the same step semantics synchronized. A missed boolean still produces a plausible report while silently changing which SLOs decide its verdict, defeating the compile-time safety the new `StepKind` was introduced to provide.
- Smallest testable correction: delete `Plan::judged` and `Plan::sample_floor`. Derive warmup judgment from `StepKind` and derive the AC-040 floor from `Record`'s `StepKind::Sustained` plus `replicas == 1` at the report owner. Keep the external labels and JSON shape only if they are required evidence; otherwise do not serialize redundant derived fields. Update the existing SLO, one-table, and verdict-step tests to prove the same outputs using only valid variants.
- Nearby pattern: `load.rs::{Op, Work}` derives operation behavior from its enum variants rather than carrying parallel behavior booleans.

### MR-002 — `Lifetime` accepts one limit but diagnoses a different global limit

- Changed location: `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:124-188`, with the shortened-lifetime proof at `main.rs:647-679`.
- Governing principle: `maintainer-style.md` requires names, argument types, documentation, and behavior to describe the same operation; `AGENTS.md` §16 requires substantive documentation of async cancellation and partial progress.
- Evidence: `Lifetime::new(limit, ...)` computes deadlines from its `limit` argument but does not retain that value. Static `Lifetime::bounded` always formats `LIMIT.as_secs() / 60`, so the 60-second unit fixture intentionally asserts that its timeout says "30 minute limit" (`main.rs:657-667`). `bounded` is also the new async cancellation boundary that drops the supplied workflow, but its own rustdoc does not state that propagation; only its two callers describe selected cases.
- Concrete maintenance cost: the controllable proof and the reusable type disagree about which lifetime expired. A future maintainer changing a smoke/test bound, cleanup reservation, or production limit can receive a confident but false diagnostic, and must infer the nested future's cancellation behavior from callers rather than the boundary that performs it.
- Smallest testable correction: retain the configured total limit (or its truthful display value) on `Lifetime`, make the bound operation use that owner rather than the global constant, and document that cancellation of `bounded` drops the supplied future with whatever partial effects its owner describes. Change the paused-clock test to assert the configured 60-second lifetime while preserving the 45-second measurement and 50-second client-cleanup deadlines.

## Calibration notes

- `Metrics::parse` has one non-owner caller, the focused staged-Scribe test. A narrower public surface would be preferable in an ordinary runtime crate, but `wyrd-testing` owns this scrape test harness and `Metrics` is already the public result of `LocalServer::metrics`; this review does not make the visibility change a material finding.
- `Benchmark` is large, but its methods share one setup-to-report lifecycle, deadline, fixtures, deployment, and evidence. Splitting it again would recreate the ownership problem remediated from r1.
- `Report::render` is long because it emits one ordered artifact and delegates cell judgment and row construction. Line count alone does not justify another renderer abstraction.

## Verification reviewed

The remediation records 14 passing `capacity` binary tests, 58 passing
`wyrd-testing` library tests under the repository Postgres wrapper, passing
focused two-replica claim/fairness tests, passing Rust SDK sustained-ingest and
judgment journeys, clean formatting/lints/Clippy, and a reduced two-replica
smoke. `git diff --check` is clean for the immutable range. This maintainer
review inspected the cited test bodies and traced their production/test-harness
callers; it did not rerun Cargo work in the shared checkout. The required
unmodified default benchmark remains explicitly not run in the remediation
record.

## Overall result

**BLOCKED**

The explicitly requested approved revision 58 is not present in the immutable
candidate, while every committed task and remediation authority names revision
57. That authority mismatch prevents a conclusive maintainer acceptance result.
Against the committed revision-57 packet, the original r1 maintainer findings
are closed, but MR-001 and MR-002 remain concrete, bounded maintenance defects
for independent validation.
