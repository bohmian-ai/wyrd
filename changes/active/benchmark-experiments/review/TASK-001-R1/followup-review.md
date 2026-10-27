# Focused documentation follow-up

Result: **RESOLVED** — the proposed Verification sibling expansion of REP-001 is source-supported.

Immutable subject: base `0739bbf38af7cc05bbfefb09850490c55432ec2d`, candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`. Scope is only the new `impl Workload for Bench` methods in `crates/wyrd/wyrd-testing/src/bin/capacity/verification/run.rs`. CodeGraph was consulted first; candidate source and the base-to-candidate diff were then inspected through Git. All shell commands ran through mise.

## Authority and source evidence

Root `AGENTS.md` Code mandates rustdoc and `# Errors` on fallible changed items and states missing rustdoc blocks merge. `agent_docs/rust.md` Rustdoc applies regardless of visibility and requires async/durable operations to document cancellation, partial progress, idempotency and retry. These are explicit mandatory gates. Scope authority records no nested AGENTS for these paths.

The candidate diff adds all three methods. Their complete bodies and existing documentation were inspected:

| Candidate location | Source evidence | Documentation gap |
|---|---|---|
| `verification/run.rs:532–568`, `Bench::prepare` at 535 | Creates the log directory, resolves the release binary, starts Collector and LocalServer, stores the collector, validates the provisioned tenant, converts application count and awaits Tenant::provision, then publishes states/server. Directory/binary/collector/start/provision errors propagate; tenant-count validation explicitly errors. Several await points follow partial setup. | Purpose summary only; no Errors section, cancellation/partial-progress or retry explanation. |
| `verification/run.rs:579–591`, `Bench::measure` at 581 | Dispatches to and awaits closed or competing. closed at 241–319 propagates missing server/collector, invalid client count and window scrape errors. competing at 329 onward uses the server, schedule parsing and scrapes, produces direct/background work and drains queued work. Requests can have executed before later observation/collection. | Workload summary only; no Errors section or cancellation/partial-progress/retry explanation. Trait text does not supply these implementation-specific details. |
| `verification/run.rs:593–603`, `Bench::finish` at 594 | Clears states, takes the server (success if absent), then calls LocalServer::stop and maps stop/log preservation failure into ExperimentError. No await occurs in this body despite the async trait signature. | Summary only; no Errors section or explanation of state removal before possible stop/log failure and repeated-call behavior. Document the synchronous body accurately, without inventing a mid-await cancellation state. |

## Resolution

Add these three sibling items to REP-001's existing bounded documentation correction. This is the same explicit rustdoc gate and Workload lifecycle owner, not a new finding. Supply accurate Errors sections and actual cancellation/partial-effect/retry behavior where applicable. Do not change lifecycle behavior, introduce helpers or a checker, or promise cleanup/idempotency guarantees absent from implementation. The consequence is failure of the mandatory changed-item documentation gate and an omitted operational contract for maintainers of this lifecycle.

Closure proof: inspect these three rustdoc blocks against their full bodies and the owning workflow; run the existing write-set format/lint lanes. No benchmark execution is necessary solely for documentation.

No broader discovery, source edits, Cargo or runtime checks were performed. This follow-up does not certify sample recording, runtime budgets or complete task acceptance, and establishes no separate runtime defect.

## Additional bounded Verification W/C/T follow-up

Result: **RESOLVED** — adding the Verification L2 path to INV-05 is source-supported. This is the same measured-work versus frozen resource-window cause, not a new finding.

Authority: approved specification REQ-006 requires repetition CPU and work accounting over the same window; verification/mod.rs:255–301 declares the verifications rate, per-vCPU, per-CPU and CPU-per-work derivatives using `verifications`. The shared `Repetition::derive` at experiment.rs:1258–1304 consumes precisely that work reading and fixed repetition seconds/CPU.

Producer-to-consumer evidence in candidate verification/run.rs:

1. `Bench::competing` at 329–363 opens ResourceWindow and concurrently runs both arrival senders and observe via tokio::join. Observe at 665–710 reads phase/recovery metrics and calls resources.freeze at 697 after the window's last scrape. ResourceWindow::finish (capacity.rs:416–443) reuses frozen counters when present, so later finish does not move the CPU endpoint.
2. `arrivals` at 731–778 assigns each request a scheduled stage index before spawning it. After awaiting call at 768 it records the answer into that stage tally at 769, without any completion-time cutoff. After scheduling, 774–775 awaits every remaining task. Thus a successful request sent just before the window end and completed after the freeze is still a successful latency entry. This is ordinary supported in-flight tail work during the required L2 drain, without special invalid input.
3. The outer join waits for those completions. Only afterward competing clones all tallies (365–367), builds full expected direct timing count (368–375), and records `seconds: window.as_secs_f64()` and frozen server CPU (380–384). Its `verifications` work numerator at 503–513 sums all successful latency entries across every stage/operation, including after-end completions. No subsequent completion-time filter removes these from work.
4. Shared derive consequently counts such late work in W/T and W/C while its post-freeze CPU is absent from C and elapsed drain is absent from T. A successful direct request begun immediately before the deadline, then completing a second after the final metrics scrape/freeze, demonstrates the mismatch. L2 latency-specific Ended::between already filters server timing samples by end timestamp (630–642), but that filtering is not used to construct the shared verifications work reading. Correct end-window latency selection therefore does not close the work-accounting defect.

Smallest correction boundary: preserve the fixed measurement interval and its existing frozen CPU owner; retain completion timestamps/cutoffs in the existing arrival/tally owner so measured work counts only completions within that interval. Preserve all tail completions, errors and collection-count reconciliation as drain/integrity evidence; do not drop late requests from total expected timing counts or silently forgive failures. Reuse the existing phase/end boundaries and timing-window mechanism where applicable rather than adding another workload/collector. This is a local correction to INV-05, not an instruction to redesign stage metrics or adjacent submission semantics.

Focused closure proof: deterministically arrange one successful direct completion before the endpoint and one after it, and verify the measured work/derived rate and efficiency credit only the first while full completion/timing reconciliation still includes both. Confirm the fixed C/T window remains unchanged. No Gateway expansion was inspected or proposed. No executable verification was performed; this is independent static confirmation only.
