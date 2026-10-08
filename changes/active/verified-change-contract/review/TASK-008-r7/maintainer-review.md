# Maintainer review

## Immutable subject and scope

- Repository root: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`.
- Base: `345295d8e`.
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`.
- Reviewed range: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`.
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57.
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior findings in scope: `FIND-TASK-008-CLOSEOUT-17` and
  `FIND-TASK-008-CLOSEOUT-18` from `review/TASK-008-r6/`.

This review is intentionally limited to the two prior findings and regressions
introduced by the remediation range. It does not reopen previously accepted
capacity-benchmark code, and it does not assess the deferred full default
`bench:capacity` run (`FIND-TASK-008-CLOSEOUT-13`). The public Oracle read in
the held-commit proof is assessed as the requested producer of a pending audit
decision; the drain cannot distinguish that producer from Drift's Oracle read,
and producer identity is not the interval invariant under review.

The checkout has no `.codegraph/` directory, so navigation used Git, `rg`, and
direct source inspection. The candidate remained at the identity above during
this review.

## Changed-surface coverage

| Changed surface | Owner and caller/consumer trace | Maintainer assessment |
|---|---|---|
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:106-108` (`AUDIT_PENDING`) | Used by `Backlog::with_replicas`, the pure backlog arithmetic proof, and the live pending-gauge helper. | The single named constant makes the production/test metric dependency discoverable and removes the prior repeated literal. Its name, scope, and rustdoc are proportionate. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:163-205` (`Backlog`, `Backlog::with_replicas`) | Durable values originate in `Queue::backlog`; replica-held Scribe and audit values are folded here; `Queue::poll` is the new caller; `Deployment::drain` consumes the result and `Record` carries it to the report. | The existing evidence value remains cohesive. The changed rustdoc accurately distinguishes pre-commit replica ownership from staged durable ownership and points at the named gauge. No duplicate state or new public contract was introduced. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:211-374` (`Queue`, especially `Queue::poll`) | `Deployment::run` calls `Deployment::drain`; `drain` supplies a replica scrape operation to `Queue::poll`; `poll` performs pre-query scrape → durable `Queue::backlog` → conditional post-query scrape; `Drain::judge` accepts or rejects the resulting aggregate. `drain_read` and the held-commit proof exercise the same method. | The main path is explicit and ordered. The conditional second scrape is local to the evidence owner and returns the exact scrape that permitted an empty judgment, which keeps the resulting `Scrapes::after` evidence intelligible. The fallible async method documents errors and cancellation and directly awaits its scrape/query IO. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:303-423` (`Deployment::run`, `Deployment::drain`) | `Benchmark::step` reaches `Deployment::run`; `run` drains before constructing `Scrapes`, querying settled runs, and building `Record`; `report` consumes that record. | The deployment still owns the polling loop, replica collection, 250 ms retry interval, 60-second judgment, and final record assembly. The change is a narrow delegation to the evidence owner rather than a second drain workflow. Deadline and return shapes remain visible at the caller. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:376-456` (`tests`) | Pure `Backlog`, Scribe, histogram, and percentile proofs only. | Credential-free arithmetic remains in the ordinary unit-test module. Test names state outcomes, assertions expose the measured contract, and moved imports do not duplicate setup. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:458-708` (`pg_tests`) | `WyrdTestServer` → public `Bifrost::sql` Oracle read → non-blocking audit owner → held chain-head commit → `Queue::poll` → durable staging → `AuditPublisher`. | The module boundary now truthfully identifies the Postgres/live-server dependency and retains the environment gate. The proof schedules the first Oracle read after the callback captures the first scrape and before `Queue::backlog`; only the conditional second scrape can observe its held decision. Later assertions retain pending-to-staging and publication-to-zero coverage. Helpers are specific, documented, and shared within this one coherent proof. |
| `.config/nextest.toml:62-70` (`release-server-ports`) | Exact default-profile selectors cover `tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline` and `tests::a_slow_replica_stop_leaves_the_runtime_free`; both reach the release-server fixed replica-0 port. A source search found no third capacity test that starts `LocalServer` or the port-8080 stand-in. | The one-thread group is narrowly named, explains the actual shared resource, and follows the existing `embedded-postgres` and `peer-clusters` grouping pattern. Exact selectors avoid serializing unrelated capacity tests. This is a test-harness correction, not a production knob. |
| `changes/active/verified-change-contract/review/TASK-008-r6/TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md` | Records implementation evidence, the integrator-approved Oracle proof vehicle, the fixed-port diagnosis, and focused/broader command results. | The evidence matches the changed symbols and does not claim the deferred default benchmark qualification. It explains why the nextest group exists instead of leaving unexplained harness policy. |

## Prior-finding closure

| Finding | Maintainer result | Evidence |
|---|---|---|
| `FIND-TASK-008-CLOSEOUT-17` | **CLOSED** | `Queue::poll` brackets the durable query with a pre-query scrape and, only for an otherwise-empty aggregate, a post-query scrape. `Deployment::drain` uses the returned later scrape as the final step evidence. The live proof invokes a public Oracle read after the first scrape has been captured, holds its non-blocking audit commit, and observes the decision in the second scrape before continuing through staging and publication. The names, owner path, documentation, and test make this interval findable and safely changeable. |
| `FIND-TASK-008-CLOSEOUT-18` | **CLOSED** | All live-server/Postgres helpers and the held-commit test now live in the in-source `pg_tests` module. Pure evidence arithmetic remains in `tests`, and the live proof retains its explicit ignored/environment gate. |

## Material findings

None.

## Maintainer calibration notes

- `Queue::poll` accepts an async scrape callback even though `Queue` directly
  stores only the owner pool. In isolation that could look like test-driven
  dependency injection. Here it is not a blocking owner-shape issue: `Queue`
  already owns assembly of the benchmark's durable `Backlog`, the method is the
  single evidence-snapshot operation, `Deployment` retains the lifecycle loop
  and real replica dependency, and the callback is the smallest seam that lets
  the live proof place a real audit producer inside the exact scrape/query
  interval. Moving this into another abstraction would add more indirection;
  inlining it back into `Deployment::drain` would make the critical ordering
  harder to prove.
- The held-commit test is necessarily long because it proves several lifecycle
  states against a real server and publisher. Its setup and state transitions
  remain linear, its local helpers isolate repeated environment operations,
  and splitting the scenario would obscure the one decision's continuity.
- Verification commands and results are recorded in the remediation evidence.
  This specialist pass was source-review only; the orchestrator owns sequential
  command execution. No result for the deferred default benchmark is inferred.

## Overall result

**PASS** — coverage of every changed runtime, proof, configuration, and evidence
surface is complete for the user-directed scope. Both prior findings are closed
in a maintainer-readable form, and the range introduces no material maintenance
regression.
