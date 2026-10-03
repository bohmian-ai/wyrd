# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Range: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd..852894689388124960993014a46934e73c0ed2a8`
- Approved authority reviewed: `changes/active/verified-change-contract/spec.md` revision 57 (`REQ-171`, `AC-040`, `AC-041`, revision 57 history) and `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Governing maintainer authority: `AGENTS.md` required struct-centered Rust style, `architecture/agent-rules.md`, and `architecture/references/languages/maintainer-style.md`

The candidate remained at the named commit while this report was prepared.

## Changed-surface coverage

| Surface | Symbols and consumers followed | Maintainer assessment |
|---|---|---|
| Capacity entry point and sequence | `main.rs::{Cli, benchmark, connect, identity, release_binary}` through `Deployment::run`, `Report::write_to`, `LocalServer`, and the `bench:capacity` mise task | The top-level sequence is readable and keeps setup, ramp, scale-out, shutdown, and reporting in execution order. One state-owning workflow remains a free function (MR-001). |
| Workload fixture | `fixture.rs::{Kind, Tenant, Tenant::provision, Tenant::seed, write_graph, write_baseline, judge_prompt}` through Cards, `WyrdState`, Bifrost, and load-request construction | The module has one coherent responsibility, domain names are specific, fallible operations describe their errors, and generated fixture content stays local to the fixture owner. |
| Open-loop driver | `load.rs::{Op, Work, Lane, Tally, TenantClients, Request, Outcome, Lane::drive, Request::send, mix, query}` through every `Deployment::run` lane and the mix/query tests | Request preparation, sending, and tallying are discoverable from their owners. `Op` and `Work` correctly use enums for closed sets. The private parallel client vectors are built in one loop and indexed only inside this module; a per-replica wrapper would be an equally valid alternative, not a material defect. |
| Step orchestration and evidence | `step.rs::{Deployment, Plan, Record, Deployment::run, drain, ops, start_captures}`, `evidence.rs::{Scrapes, Queue, Backlog, RunTally}`, and their server/database/metrics consumers | Runtime state and multi-stage work are centered on `Deployment`, `Queue`, `Scrapes`, and `Record`. The closed step identity is stringly and is later matched by literal text (MR-002). |
| Report | `report.rs::{Cell, Row, step_row, op_row, Report, verdict_steps, passed, write_to, render}` through Markdown/JSON output and both report tests | Pure formatting and judgment helpers are appropriately free functions. Output rows, verdict selection, and serialization are locally followable; the verdict's dependency on `Plan.name` literals is covered by MR-002. |
| Judge and profiling fixtures | `judge.rs::{Judge, TlsListener}` and `profile.rs::{Profile, Capture}` through fixture provisioning and per-step capture | Both modules have narrow owners, substantive lifecycle documentation, and explicit drop cleanup. No unnecessary trait or configuration layer was introduced. |
| Release-server harness | `release_server.rs::{LocalServer::start_replica, await_ready, Drop, operator}` through capacity startup and existing benchmark callers | The asymmetric joined-replica ports and preserved failure logs are documented where the behavior lives. The reduced smoke benchmark exercises joined startup; the harness keeps lifecycle work on `LocalServer`. |
| Manifest and command wiring | `wyrd-testing/Cargo.toml`, `mise.toml`, deleted benchmark targets, and repository references to old binary/task names | The `capacity` binary and `bench:capacity` command agree. Old names remain only in historical spec/task text. No generated declaration applies to this internal binary. |
| Queue documentation | `wyrd-queue/src/config.rs` comment-only clarification and its configuration consumers | The edit clarifies the existing invariant without adding a second configuration path. |
| Regression proof | `pg_verification_runtime.rs` two-replica/fairness tests, `drift_verification.rs` rejected judge case, and `observe_run.rs` sustained ingestion test | The external tests earn their location by composing Postgres/real-server behavior. Names and rustdoc state the caller-visible outcome and explain why the runtime tests are integration-level. |

## Material findings

### MR-001 — State-owning client reconnection workflow is outside `Deployment`

- Changed location: `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:289` (`connect`), with the owned state declared at `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:34` (`Deployment`).
- Governing principle: `AGENTS.md` required struct-centered Rust style and `maintainer-style.md` “put a workflow with its owner” / “own shared state once.” Internal orchestration that reads and mutates an owner's dependencies must be an inherent method on that owner; passing `&mut Deployment` does not make the workflow stateless.
- Evidence: `connect` drains `deployment.clients`, derives URLs from `deployment.replicas`, iterates `deployment.tenants`, and repopulates `deployment.clients`. It is called twice as a deployment lifecycle transition, while the adjacent step lifecycle already lives on `Deployment::{run, drain, ops, start_captures}`.
- Concrete maintenance cost: a maintainer looking at `Deployment` cannot discover or preserve its client/replica alignment invariant from its methods; changing replica membership requires knowing about a free workflow in another module. This is the exact functional drift the repository's struct-centered rule makes merge-blocking.
- Smallest testable correction: move this workflow to a descriptively named inherent method on `Deployment` (accepting the authentication pacing dependency), keep `TenantClients::connect` as the per-tenant constructor, and update the two call sites. The existing capacity binary tests plus the reduced two-replica smoke command directly cover the preserved lifecycle.
- Nearby pattern: `Deployment::run`, `Deployment::drain`, and `Deployment::start_captures` already keep the other deployment transitions on the owner.

### MR-002 — `Plan` encodes its closed step kind as an unchecked string

- Changed location: `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:55-58` (`Plan::name`), consumed by `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:188-245` and `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:373-385` (`Report::verdict_steps`).
- Governing principle: `AGENTS.md` abstraction rules require enums for closed sets where exhaustiveness matters; `maintainer-style.md` requires argument and return shapes to make behavior safely followable.
- Evidence: the field documentation enumerates exactly `warmup`, `ramp`, `sustained`, and `scale-out`, but construction and verdict selection repeat those values as string literals. `Report::passed` depends on finding exact `"sustained"` and `"scale-out"` text, so a typo or rename compiles and presents as a mysteriously missing verdict step.
- Concrete maintenance cost: the compiler cannot keep sequence construction, profile-directory naming, rendered output, and verdict selection in sync. The report tests also construct arbitrary `&'static str` values, preserving the invalid state instead of preventing it.
- Smallest testable correction: replace `Plan::name` with one private closed enum for the four step kinds, derive/implement the existing serialized and display text once, and match enum variants in `verdict_steps`. Reuse the existing `verdict_needs_every_verdict_step` and rendered report coverage; no public or wire contract changes.
- Nearby pattern: `load.rs::{Op, Work}` already model the benchmark's other closed sets as exhaustive enums and derive their report labels in one place.

## Calibration notes

- `TenantClients` stores three private vectors aligned by replica. Because its constructor pushes all three in one loop and no external caller can mutate them, converting it to a nested per-replica struct is optional and is not a finding.
- `Deployment::run` and `Tenant::provision` are long, but each presents one ordered workflow on its natural owner and delegates coherent stages. Line count alone does not justify splitting them.
- Requirement identifiers in benchmark rustdoc point to the lasting workload contract rather than an implementation session or agent note. They do not obscure the behavior described alongside them, so this review does not treat them as a maintainer defect.

## Verification reviewed

The task records clean formatting, workspace lints, focused all-target/all-feature Clippy for `wyrd-testing` and `wyrd-sdk-rust`, six passing `capacity` binary tests, passing focused Postgres/runtime and Rust SDK journeys, `git diff --check`, and a reduced two-replica smoke run. This review independently checked the complete diff, manifest/task references, changed symbols, callers, and relevant test bodies; it did not rerun the expensive benchmark. The full default 30-minute benchmark was not available, but that is a task-acceptance evidence limit rather than a maintainer-style gap.

## Overall result

**FAIL**

The implementation is otherwise unusually well documented and capability-shaped, but MR-001 violates the repository's hard struct-centered owner rule and MR-002 leaves a verdict-critical closed state stringly typed. Both corrections are bounded, private, and directly covered by existing proof.
