# TASK-008 round-seven independent behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Branch: `worktree-agent-a82d72f51901e1dc5`
- Base: `345295d8e`
- Candidate: `54373c36856d41ffe3554c977c35e9d033918cdd`
- Reviewed range: `345295d8e..54373c36856d41ffe3554c977c35e9d033918cdd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior verdict and validation: `changes/active/verified-change-contract/review/TASK-008-r6/`
- Remediation task: `changes/active/verified-change-contract/review/TASK-008-r6/TASK-008-CLOSEOUT-R5-bracket-audit-drain-and-place-pg-proof.md`

The candidate remained at the named commit during this review. The repository
has no `.codegraph/` directory, so navigation used Git, `rg`, and direct source
inspection.

This review applies the user-directed closure boundary exactly: determine only
whether `FIND-TASK-008-CLOSEOUT-17` and
`FIND-TASK-008-CLOSEOUT-18` are closed by this remediation range and whether
the range introduces a regression. Earlier accepted implementation was used
only to trace the changed behavior and to determine whether a benchmark result
could become false. `FIND-TASK-008-CLOSEOUT-13` remains deferred to integration
and supplies no capacity qualification here.

## Navigation map and caller expansion

| Changed surface | Owner and role | Reachable callers or consumers | Relevant proof |
|---|---|---|---|
| `capacity/evidence.rs:105-107` | `AUDIT_PENDING` names the existing replica-owned pending-decision gauge | `Backlog::with_replicas` and the held-commit proof | Pure pending-plus-staged arithmetic and live Oracle audit proof |
| `capacity/evidence.rs:180-204` | `Backlog` composes durable and replica observations | `Queue::poll`; then `Deployment::drain` and the step verdict/report | `pending_decisions_add_to_staged_audit_rows` |
| `capacity/evidence.rs:307-341` | `Queue::poll` owns one bracketed drain observation | Sole production caller `Deployment::drain`; test callers `drain_read` and the held-commit proof | Live proof creates a pending decision between the first sample and durable query |
| `capacity/step.rs:383-423` | `Deployment::drain` owns the poll loop, deadline judgment, and final returned scrape | `Deployment::run`, then report backlog and pass/fail calculation | Existing exact drain-edge proof plus complete capacity target evidence recorded by the remediation |
| `capacity/evidence.rs:378-456,458-708` | Pure tests remain in `mod tests`; Postgres/live-server helpers and proof now live in `mod pg_tests` | Default capacity selection skips the ignored environment proof; repository Postgres command selects it explicitly | Exact `evidence::pg_tests::...` registration |
| `.config/nextest.toml:62-70` | Default-profile serialization for the two capacity tests that bind release-server port 8080 | Full capacity target with ignored tests enabled | The two exact test names are selected into a one-thread group |

`Queue::poll` has no additional production consumer: the only production path
is `Deployment::drain` at `capacity/step.rs:407-416`. The generic scrape
callback therefore does not create a second behavior owner or an unused
extension point; it keeps the replica observation at the existing deployment
owner while putting the required ordering beside the durable queue read.

## Caller-to-result behavior trace

### The false-empty interval is closed

`Deployment::drain` delegates each loop iteration to `Queue::poll` and passes a
callback that scrapes every serving replica (`capacity/step.rs:406-416`). The
poll performs these operations in order:

1. it captures every replica's pending audit and Scribe state;
2. it reads the durable run, staged-audit, and Forge backlogs;
3. it combines the first scrape with the durable reading;
4. only if that combined result is empty, it captures every replica again and
   adds the second scrape before returning
   (`capacity/evidence.rs:324-340`).

This covers the three relevant producer states without changing their owners:

- a decision pending before the durable query is retained by the first scrape,
  even if its commit lands during the query;
- a committed unpublished decision is retained by the existing
  `audit_staging.seq > audit_chain_head.published_seq` query
  (`capacity/evidence.rs:284-303`); and
- a decision first staged after the first metrics sample is retained by the
  second scrape before an otherwise-empty result can reach `Drain::judge`.

If any run or other backlog remains nonzero, the unchanged loop cannot report
drained and a later poll observes the next transition. The second scrape is
therefore needed only at the zero decision boundary. When the second scrape is
used, its `Metrics` values are returned to `Deployment::drain`, so the report's
final replica evidence is the observation that permitted the zero decision.
The unchanged `Drain::judge(stopped_at.elapsed(), backlog.is_empty())` call
still applies the exact deadline after the complete poll
(`capacity/step.rs:417-420`).

`Backlog::with_replicas` overwrites Scribe from the supplied scrape and adds
pending audit to the durable audit count (`capacity/evidence.rs:186-203`). The
second call cannot accidentally double-count the first scrape because that
branch is entered only when the first combined `Backlog` is entirely empty.

### The focused proof exercises the required ordering

The live proof is not a parser-only or fabricated metric test. It uses a public
`wyrd_client::Bifrost` Oracle query, the server's actual audit outbox gauge, a
real `WyrdTestServer` and repository Postgres, the real tenant audit-chain
commit, and the real `AuditPublisher`
(`capacity/evidence.rs:571-616`). It locks the tenant's chain head, then invokes
the public Oracle read inside `Queue::poll` after capturing the first metrics
sample but before allowing the callback to return to the durable query. It
waits until Postgres proves the actual audit writer is blocked by that lock
(`capacity/evidence.rs:637-652`). Consequently:

- the first scrape necessarily contains zero for this new decision;
- the held transaction prevents the decision from appearing in durable
  staging;
- `late.audit == 1` can be supplied only by `Queue::poll`'s post-query scrape
  (`capacity/evidence.rs:653-656`).

That is the false-empty interval diagnosed by FIND-17. The drain is agnostic to
which authorized surface produced the pending decision. Per the integrator's
direction, using the public Oracle read proves the changed observation
protocol directly without constructing a second queued-Drift harness. The
rest of the same proof retains the lifecycle assertions: two held decisions
are pending, pending-to-staging never reads below two, at least one row is
stamped after the captured stop, unpublished rows remain counted, and the cell
reaches zero only after publication advances the watermark
(`capacity/evidence.rs:657-703`).

### The environment boundary is corrected

The range separates the pure metrics/backlog/percentile proofs under ordinary
`#[cfg(test)] mod tests` (`capacity/evidence.rs:378-456`) from the
Postgres/live-server proof and all its environment-specific helpers under
`#[cfg(test)] mod pg_tests` (`capacity/evidence.rs:458-708`). The live proof
retains its `#[ignore]` execution gate at lines 569-570. This exactly satisfies
the repository rule for Postgres and live-server tests without adding an
external target, duplicating a fixture, or broadening the credential-free
default selection.

### The fixed-port serialization is a bounded regression correction

Both changed test selections start the same release-server stand-in on the
fixed replica-zero public port 8080 (`capacity/main.rs:790-792,829-831,919-921`;
`release_server.rs:29-34`). They are separate nextest processes, so process
isolation does not prevent a simultaneous bind. The new
`release-server-ports` test group has `max-threads = 1` and its override names
exactly those two tests under the `wyrd-testing` capacity binary
(`.config/nextest.toml:62-70`). No other capacity test calls
`LocalServer::start`; the search results are the benchmark's production setup
and these two tests. The group therefore serializes only the conflicting
resources, follows existing repository test-group practice, and does not hide
or weaken an assertion, add a retry, extend a timeout, or change benchmark
runtime behavior.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-17 / AC-R5-1: an otherwise-empty durable snapshot is bracketed by pre- and post-query pending observations from every serving replica | `Queue::poll` at `capacity/evidence.rs:324-340`; sole production caller supplies all replicas at `capacity/step.rs:407-416` | Source ordering; remediation record reports the focused live proof and full capacity target passing | **PASS — CLOSED** |
| FIND-17: a decision pending before SQL, handed off during SQL, or first created after the pre-query observation cannot permit a false zero | First scrape plus existing durable watermark query plus conditional second scrape; zero branch cannot carry prior replica state because `Backlog::is_empty` gates it | Held-chain lifecycle assertions at `capacity/evidence.rs:637-703`; pending arithmetic test | **PASS — CLOSED** |
| Integrator-narrowed AC-R5-2: prove the false-empty interval using the existing held-commit harness and a real public Oracle producer | Public Oracle read is issued after metrics sampling inside the first scrape callback; chain-head lock and blocked-writer check keep durable staging empty; returned audit count must come from the second scrape | `the_audit_backlog_holds_from_a_pending_decision_until_its_publication`; remediation record reports the exact Postgres-wrapped selector passing | **PASS** |
| The final returned scrape is the scrape that permitted the zero decision | The nonempty fast path returns the first scrape; the otherwise-empty path replaces it with and returns the second scrape at `capacity/evidence.rs:336-340` | Direct source trace through `Deployment::drain`'s returned `after` scrapes | **PASS** |
| FIND-18 / AC-R5-3: environment-owned proof lives at the required source boundary, retains its gate, and does not enter the default fast selection | Pure tests at `capacity/evidence.rs:378-456`; environment helpers/proof at `capacity/evidence.rs:458-708`; live proof remains ignored | Exact test name is `evidence::pg_tests::the_audit_backlog_holds_from_a_pending_decision_until_its_publication`; remediation record reports 15 default tests passed with 5 skipped | **PASS — CLOSED** |
| AC-R5-4: exact 60-second judgment, report fields, workload, audit publication semantics, and adjacent capacity behavior remain unchanged | Range changes only polling composition, test placement/proof, one gauge-name constant, and test-run serialization; `Drain::judge` and caller/report flow are unchanged | Remediation record reports focused drain edge, capacity target, release-server tests, server journey, formatting, lints, and diff check passing | **PASS within recorded verification** |
| The nextest change addresses a real resource conflict without suppressing behavior | Exact two fixed-port tests share one `max-threads = 1` group; no ignore/allow/retry/assertion/timeout change | Source trace of port 8080 ownership and test names; nextest registration discovery found both exact selectors | **PASS** |
| Preserve non-blocking audit commits, canonical staging/publication, and existing lifecycle owners | No server, audit-outbox, publisher, schema, API, workload, SLO, report, or timeout implementation changed | Diff and caller trace | **PASS** |
| FIND-13 full default `bench:capacity` qualification remains deferred | No benchmark result or AC-040/AC-041 qualification is inferred from this review | Explicit user direction and remediation evidence note | **PASS — DEFERRED, NON-BLOCKING** |

## Proposed findings

No findings.

I found no `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
within the directed review boundary. In particular, using a public Oracle read
instead of queued Drift does not weaken the FIND-17 proof: the behavior being
proved is the drain's inability to identify which producer owns a pending
decision, and the proof places a real decision in the exact previously
unobserved interval while the durable row is provably absent.

## Verification limits

- This reviewer performed source inspection of the full remediation range and
  surrounding callers, tests, fixed-port ownership, and applicable authority.
- Before the orchestrator's source-only coordination note arrived, test
  registration was inspected with `cargo nextest list` through `mise`; with
  ignored tests included it listed the exact live proof, both fixed-port tests,
  the pending arithmetic proof, and the exact drain-edge proof. This was test
  discovery, not behavioral execution.
- No test, benchmark, database-backed command, codegen command, formatter, or
  lint lane was executed by this reviewer. Behavioral results cited above are
  the immutable remediation evidence and still require the orchestrator's
  independent verification reconciliation.
- The full unmodified default `mise run bench:capacity` remains deferred as
  FIND-13 and is not represented as passing here.

## Overall result

**PASS**

Within the caller-directed closure scope, the range closes
`FIND-TASK-008-CLOSEOUT-17` and `FIND-TASK-008-CLOSEOUT-18`. The conditional
post-query scrape prevents the diagnosed false-empty decision, the real held-
commit Oracle proof exercises that interval, the environment proof now resides
under `pg_tests`, and the exact fixed-port test group introduces no behavioral
regression.
