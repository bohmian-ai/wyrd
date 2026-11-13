# System-resilience review — TASK-008

Subject: first review of `cd0b8e18b...798b6b0e83595356c0d2328fa541119267e84fb2`, repository and shared map in `scope.md`. HEAD remained the candidate during inspection. No prior acceptance carried forward. This reviewer did not consult other discovery reports or edit production source.

## Authority and deployed-path coverage

Read the task's pending-occurrence, acceptance, continuation and proof requirements; approved spec revision 8 REQ-010/011 and INV-002–006; contracts Human approval; approved-deferred-inputs Approval and deadlines and existing-owner table; approved-nonblocking-execution Persistence and capacity / Lease, cancellation and SQL; architecture Verifier approval and Bifrost ACK/fenced-only acceptance; security Permission model; root AGENTS, relevant SQL rules, testing map and maintainer style. CodeGraph exploration preceded source navigation.

The change runs in the shared server verification runtime and request control surface. The engine can run independent actions while approval dependents block. Its runner owns the queue claim and RunState; Scribe owns accepted state revisions and decision rows; PostgreSQL coordinates queue eligibility, leases and due time. Inspection and decisions recover accepted state through InputResolver. An approval decision is a separate first-wins fenced Scribe row, not a second full-state writer. Existing DeferredEvidence is the closest custody/replay alternative; this implementation reuses its ScribeWrite framing/fence behavior, with a new approval-specific read/row owner. No new deployment service, database payload mirror or process-fatal approval failure was found.

| Path / failure | Source and observable recovery | Assessment |
|---|---|---|
| All work blocked → park | engine.rs:1069–1082 returns Waiting; runner.rs:579–590 mutates sole state and waits for accepted parked revision before Transition::Park; runner.rs:1247–1253 releases queue claim until bounded recheck | Uses existing park handoff, no approval wait service. Block action permits belong to attempts and are gone when engine returns (engine.rs:1733,1812); background claim custody remains bounded. |
| Decision accepted but wake lost / process stops | service/approvals.rs:170–185 logs wake failure; runner.rs:584–586 bounds park by deadline/PARK_RECHECK; every new dispatch reads accepted decisions (runner.rs:930–939,959–1004) | Decision remains canonical. Reclaim reads it without a second decision. No process-local notification is acceptance. |
| Scribe slow/unavailable | verification/approvals.rs:137–165 bounds slot/ACK wait by request half-timeout and returns writer unavailable; durable fence visibility is checked separately at :81–108 | Affected request refuses; it does not kill shared server or silently classify unreadable committed state as absent. |
| Fenced acceptance not query-visible | approvals.rs:89–108 requires a row for each committed fence; runner.rs:970–991 converts read failure to Deferred | Safe unavailable response / retry, without fresh approval identity. |
| Supersession/cancellation while parked | Wake only changes due time for unfinished pending/retrying queue rows; existing claim/renewal/token fences remain; pg_approvals.rs:502–543 approves while runner stopped, supersedes, restarts, asserts cancelled and never passed | Accepted historical receipt remains data and does not authorize replacement head. |
| Approval reached while an independent branch stays active | Engine keeps pending occurrence solely in local awaiting map until all tasks end | Fails SYS-001. |
| Execution's shorter authored deadline elapses while runtime is stopped | Runner respects authored timeout, but new decision-open SQL receives only runtime timeout | Fails SYS-002. |

## Proposed findings

### SYS-001 — INCORRECT: reached approvals remain invisible until all independent work finishes

**Violated obligation:** task Input → pending occurrence says the sole state owner creates the pending identity when the action becomes pending and puts its identity/message into live state, while independent branches continue. Architecture Verifier says every reached occurrence is recorded and inspection lists it. This is the user-facing ability to review a pending approval while independent computation continues, not an arbitrary latency target.

**Location and producer→consumer trace:** `crates/vala/vala-verify/src/blocks/engine.rs:1543–1558` produces an ApprovalOccurrence without recording it; `:1094–1096` places it only in local `awaiting`. `:1069–1082` returns those occurrences only after `tasks.is_empty()`. The sole registration in `crates/wyrd/wyrd-server/src/verification/state.rs:815–842` is `RunState::wait`, called only after compute returns Waiting (`runner.rs:579–590`). Inspection reads only accepted `state.approvals` (`service/inspection.rs:299–304,515–522`), and decide explicitly refuses IDs absent there (`service/approvals.rs:74–87`). Neither invocation/control checkpoints nor another writer records a reached pending occurrence.

**Reachable scenario and consequence:** a Verifier has an immediately reached signoff and an independent long-running model/code branch. While the latter runs, the approver sees no approval occurrence and cannot submit a decision. Human review and independent work are serialized. If that independent branch runs to the execution deadline or fails the execution, a reached approval never appears even as expired. Existing engine proof waits for `run()` to return before examining occurrences (`engine/tests.rs:4405–4424`), so it does not exercise this gap; the HTTP journey's peer work is deferred or already settled, not active throughout approval inspection.

**Materiality:** the explicit task timing/visibility behavior fails on ordinary independent model work; no unusual input or new availability promise is needed.

**Smallest correction outcome:** route occurrence creation through the existing engine→Checkpoints→RunState state mutation boundary when reached, retaining deterministic occurrence identity and coalesced background persistence. Inspection must still expose only accepted state. Preserve independent work and the existing compute-free all-blocked park ACK; do not add per-approval SQL, a polling service, another state writer or durability wait to the engine. The same owner must reconcile speculative disposition, rather than downstream visibility guards. This is a bounded lifecycle extension; simply moving the inspection read cannot reveal a state never produced.

**Focused closure proof:** keep an independent action in flight, observe the reached occurrence through real execution inspection before releasing that action, and accept its decision. Then let independent work settle and verify the same execution uses the decision; verify timeout still leaves the recorded reached occurrence expired. Reuse current engine/mock and server journey owners, no new harness.

### SYS-002 — INCORRECT: decision acceptance ignores the Verifier's shorter execution deadline

**Violated obligation:** REQ-010/011, contracts Human approval, and task Decision acceptance require decision refusal after the execution's one original expiration; the approval has no independent expiration.

**Location and producer→consumer trace:** `crates/wyrd/wyrd-server/src/verification/runner.rs:1448–1460` computes the actual deadline from admission, taking the minimum of queue/runtime remaining time and exact Verifier `limits.timeout_seconds` minus admission age. The new `components/verification/service/approvals.rs:124–130` instead passes only `state.verification.execution_timeout()` to `open_for_decision`. `crates/wyrd/wyrd-sql/src/queries/verifier_executions/approvals.rs:20–25` tests admission plus that supplied runtime duration. The transaction commits before acceptance, and no later path applies the shorter authored ceiling; `ApprovalDecisions::accept` only waits against a request ACK deadline.

**Reachable scenario and consequence:** configure a five-minute runtime ceiling and a Verifier with a five-second `limits.timeout_seconds`. Park its approval, stop the runtime (the ordinary scenario already supported by pg_approvals.rs:458–488), and send a fresh decision after five seconds but before five minutes. Queue status is still pending/retrying and OPEN_SQL returns true. The caller gets a positive approved receipt for an already-expired execution. On runtime restart the engine times out instead of consuming it. This is a full expired interval, independent of the allowed simultaneous-deadline race noted in implementation evidence.

**Materiality:** a normal supported authored execution limit ceases to constrain the approval boundary whenever runtime settlement is offline/delayed. The task explicitly promises that decisions after expiration refuse, even though a delayed receipt cannot make the execution pass.

**Smallest correction outcome:** make decision acceptance use the same effective execution ceiling as the canonical runner, including the exact accepted Verifier's authored limit from admission. Reuse that existing deadline rule and the tenant-scoped PostgreSQL coordination clock; do not invent an approval deadline, reset age, use a newer Verifier version, add approval-specific timers or defer correctness to eventual terminal settlement. Changing generic retry/continuation semantics is unnecessary. The existing runtime-only guard cannot be retained as sufficient merely because the engine later refuses to pass.

**Focused closure proof:** use the existing stopped-runtime approval test setup with a short authored Verifier limit and longer runtime ceiling. Decide after the authored expiration and assert 409/no newly accepted decision; repeat before expiration to show the supported decision still resumes the same execution. Also retain the current runtime-ceiling expiry case.

## New/moved wait inventory and performance

| Wait added/moved by this diff | Callers | Sanctioned boundary / effect |
|---|---|---|
| Approval fence read: tenant connection + `committed_batches` | ApprovalDecisions::decided at approvals.rs:81–92, from retained inspection/decide and resumed queued dispatch | Explicit canonical accepted-state/control-fact read; empty occurrence slice returns without SQL. It is not issued when an approval first becomes pending. |
| Approval rows Bifrost query | approvals.rs:102 → InputResolver::decisions | Only when one or more known decision fences exist; canonical read-back and unavailable-on-invisibility. |
| Queue current-open read + commit | service/approvals.rs:117–131 → open_for_decision | Request acceptance coordination check; must obey actual execution expiration (SYS-002). |
| Outbox reserve plus Scribe ACK | approvals.rs:137–157 | Explicit positive approval receipt boundary, bounded by half request timeout in architecture Bifrost ACK contract. Scribe internally owns WAL/storage/fence writes; those existing waits are not moved onto engine mutations. |
| Queue wake tenant connection/update/commit | service/approvals.rs:173–179, for fresh/replayed decisions | Best-effort continuation coordination; failures logged and park recheck repairs. This remains a request wait even after ACK, no new service loop. |
| Parked checkpoint ACK | runner.rs:587–590 | Existing TASK-007 boundary, extended payload includes approvals; engine/block resources released before ACK. |

No new Vault wait or direct object-storage wait was introduced. Existing DeferredEvidence frame/fence changes preserve its destination, batch-ID composition, attribution and ACK dispatch; no extra wait was found for that sibling. New reader/table permission projection is confined to canonical system reads.

The shared VerificationEngine is also used by direct verification: added decisions Arc/default and waiting bookkeeping alter that hot code even when direct registration refuses approvals. This review established no new PostgreSQL/Scribe wait on direct verify and no SQL at pending action reach, but has no full clean L1 verification benchmark result or comparison against host history. The maintainer memory preference recorded in scope therefore remains an executable verification consideration for the orchestrator, not a fabricated measured PASS.

## Proof limits and overall result

Source inspection verified pg_approvals assertions for authorized/rejected decisions, identical/conflicting retry, runtime-ceiling expiry, stopped-runtime continuation, supersession and held-Scribe refusal/replay. The restart test restarts VerificationRuntime against the same server rather than a complete server process; source traces recover accepted state/decisions and do not rely on a live decision map. No claim of full process-restart execution is made. No Cargo/benchmark/DB commands were run by this reviewer because the orchestrator owns sequential checks.

Overall: **FAIL** — SYS-001 and SYS-002 require independent source validation. No production code, tests, commits or pushes changed.
