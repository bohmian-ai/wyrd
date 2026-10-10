# TASK-003 R1 invariant review

**Result: FAIL**

## Immutable subject and method

- Base `7f79fb3417db651adedac194ada8908f0a0372d7`; candidate `9a8f9f7eef95f70d356c037a192b7d7b90a37f31` (HEAD checked during review).
- Authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, `changes/active/verification-closeout/spec.md`, original `tasks/TASK-003-r4-canonical-support-desk-closeout.md`, prior `review/task-003-r4-canonical-closeout/verdict.md`, and its `TASK-003-R1-canonical-closeout.md`.
- Reviewed the cumulative base-to-candidate change, using `f6c841d57..9a8f9f7ee` to locate remediation. Traced the changed Forge worker through startup recovery, plan and heartbeat spawning, SQL reclaim, and the server restart supervisor. Also checked the Oracle memory predicate, three example deployment identities, delayed-Scribe journey topology, role rerun, and documentation closure. Verification results below are recorded evidence, not rerun by this review.

## Acceptance matrix

| Obligation | Source evidence | Verification evidence | Result |
|---|---|---|---|
| Declared Service tables, optional activation, unbound writer activation, application Run results, and direct task results on the shared outbox | Cumulative registration, verification, and result owners; no R1 edit to these owners | Original task matrix and focused server journeys | PASS |
| Paired Card UID gateway attribution, pre-dispatch authorization, captured UID, and no CardRef alias | Cumulative `gateway/ingress.rs`, `gateway/invocation.rs`, `gateway/capture.rs`, shared client `Run::invoke` | Original gateway integration, OpenAPI, and codegen evidence | PASS |
| Gateway-owned judges, Agent invocation, Run scope, telemetry, and three public SDK journeys | Cumulative gateway/verification owners and Rust, Python, TypeScript projections | Original task matrix and SDK journey evidence | PASS |
| Each example deploy checks the Prompt's exact gateway model identity and explains missing configuration | Prompt YAML names `open_ai_chat_completion`/`gpt-4o`; route maps that to `openai/gpt-4o`; Python, Rust, and TS `deploy` compare both provider and model and name the admin deployment endpoint | Three R1 support-desk journeys with an `anthropic/gpt-4o` collision, plus example/type/lint checks | PASS; closes FIND-1 |
| Oracle shutdown waits for Oracle query bytes, preserving both wakeups and shared capacity | `oracle/admission.rs:739-790` enables both notifications before checking Oracle attribution in the existing governor | New mixed-role admission test and 28 admission tests reported passing | PASS; closes FIND-2 |
| Changed Rust items have required rustdoc | `wyrd-client/src/bifrost/mod.rs:152-157` and `examples/support-desk/rust/main.rs:12-21` | `fmt` reported passing | PASS; closes FIND-3 |
| Active observation design describes Card UID without altering registry CardRef meaning | R1 design/evaluation/telemetry prose diff changes correlation passages only | `docs:check` reported passing | PASS; closes FIND-4 |
| Role SQL rerun fails on SQL error | `scripts/postgres/test-roles.sh:27-29` passes `ON_ERROR_STOP=1` to container `psql` | Roles lane and deliberate SQL error exit 3 reported | PASS; closes FIND-5 |
| An audit staged before Scribe joins is delivered after its registration | `server/query.rs:765-810` delays the last, Scribe-only node, queries before restarting it, then drains Oracle and reads audit; `cluster.rs:253-261,1238-1248` confirms node order and delayed-last behavior | Exact late-Scribe journey and retry trace reported passing | PASS; closes FIND-6 |
| A Forge restart fast-reclaims its own unexpired attempt only when prior work is physically quiesced | `forge/worker.rs:2173-2192` installs the quiescence marker and stop guard only after `start_and_drain`; that startup path can itself spawn a runner and heartbeat | Quiescence unit test covers only a flag set manually inside a panicking task; existing restart journeys do not interrupt startup execution | **FAIL; FIND-7 remains open on startup recovery** |
| Process-owned Scribe route, poller, role heartbeats, Postgres wrapper, absence of replacement machinery and aliases | Cumulative boot/state/cluster/outbox and script diffs; R1 adds no second routing or storage path | Original Scribe 28/28, Forge 22/22, Oracle 50/50, gate, codegen, boundary and journey evidence | PASS within inspected invariant paths |

## Proposed finding

### INV-R1-1 — Startup recovery is outside the Forge quiescence guard

**Classification:** INCORRECT (incomplete closure of prior FIND-TASK-003-7). **Obligation:** a supervised restart may fast-reclaim unexpired same-owner attempts only after the prior incarnation's physical execution and heartbeat work stopped.

**Location and producer-to-consumer path:** `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2173-2187` awaits `start_and_drain` before constructing `_stop_on_unwind` and before `quiescence.enter()`. Startup `drain_recoverable_work` calls `execute_one_recoverable_cleanup` (`:2249-2252,2311-2343`), which calls `execute_claim`. That call admits a pool, can spawn a plan runner (`:3976-4022,6000-6016`), and spawns a claim heartbeat (`:5281-5300,8350-8394`). The heartbeat is a detached Tokio task if the owning future unwinds. The worker's quiescence starts `true` (`:713-735,2052-2056`), and `restarting_worker` in `wyrd-server/src/app/supervise.rs:119-180` builds another worker incarnation after a panic. Its startup passes `Some(owner)` to `reclaim_attempts` while the flag is still true (`worker.rs:2234-2243`), clearing an unexpired attempt even though the prior startup's plan and heartbeat can still run. Because the cancellation guard was also absent, the detached heartbeat can continue to renew until the SQL clear makes its next beat fail.

**Observable consequence:** a crash during a recoverable cleanup execution can overlap old and new work under the same durable owner, recreating the precise self-reclaim race R1 was meant to close. Exact-attempt SQL fences limit some downstream writes but do not establish the required physical quiescence before reclaim.

**Testable correction:** cover the entire worker invocation, including startup recovery, with the same quiescence and unwind cancellation ownership used for the event loop. Mark joined only after any work that invocation spawned has stopped; after a startup panic, use ordinary lease-expiry recovery. Prove the startup recovery interruption with a focused test that observes the successor refusing unexpired self-reclaim while the old startup work is live, and retain the clean restart fast-reclaim proof. Reuse the current worker token, shared flag, supervisor, and SQL reclaim predicate; no persistent fence or new mechanism is needed.

## Notes and verification limits

The R1 deployment check is correctly scoped to the checked-in example workflow. The Oracle test demonstrates that Forge memory does not hold this Oracle shutdown; existing attribution also counts Scribe-node fragments of Oracle queries as Oracle memory, as documented in the task evidence. I found no second material invariant defect in the inspected cumulative paths. I did not run test lanes or inspect peer review reports; a reviewer should validate the startup panic path and the remaining cumulative acceptance surface independently.
