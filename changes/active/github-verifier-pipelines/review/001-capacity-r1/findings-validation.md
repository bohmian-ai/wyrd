# Independent findings validation — TASK-001

Recommended verdict: **FIX_REQUIRED**. Retained IDs: **FIND-001-1, FIND-001-2, FIND-001-3, FIND-001-4, FIND-001-5**.

## Subject, independence and coverage

Base `395274d58bf76a2075f245ca3a5c1bd044925027`; candidate `34fb45bb387582d2aedfee77fb4e09fa2753b80d`. This is the first cumulative acceptance review, with no earlier accepted area or closure evidence carried forward. HEAD matched the candidate during validation. Only this report was authored; no source, test, Cargo, or configuration mutation was performed.

Read the common scope and applicability map, original task and O1–O19 ledger, approved specification revision 3 and runtime design, all six selected discovery reports, claim comparison, focused follow-up and verification report. Consulted CodeGraph before source navigation, then actual cumulative diffs, full correction-owner bodies, producers, affected consumers and relevant existing test patterns. Applicable authority includes root AGENTS, docs AGENTS, spec-driven development, agent_docs index/Rust/maintainer rules, TESTING and the architecture Verifier resource-composition owner. Discovery agreement was not treated as source proof.

Validation traced configuration through `set_scaled`, `VerificationConfig::apply_limits_env`, `HostedLimits::lanes_fit`, `Sandbox::start` and `HostedBlocks::start`; both production `Execution` producers and lane-selected engine composition; direct eligibility/admission; model-call production in Skald through the server caller and existing gateway cancellation/settlement; the full new capacity story, its fixture/control/result/readback helpers and runtime-target alternatives; the entire failing recovery-test body, response producer and existing milestone-driven sibling; and the full materially extended defaults test. The selected reports provide cumulative coverage of remaining workspace/Python checkout, claim/defer/reclaim, documentation, fixtures and migrated existing test consumers. Their assertions and changed owner composition were cross-checked rather than accepted from test names.

### Specialist applicability independently reconciled

| Contract | Required coverage and validation |
|---|---|
| Acceptance and direct/queued user behavior | Fresh behavior report covers the complete task, O1–O19 and additional constraints. Present and complete. |
| Shared resource ownership, permits, cancellation and reclaim | Invariant report required and present. Distinct retained lanes, exhaustive mode selection, direct request RAII and existing queued claim owner justify its selection. |
| Sensitive concurrency | Separate concurrency-domain report required and present. It traces actual semaphore/workspace/checkout ownership and both claim and direct lifetimes, not merely leaf calls. |
| Startup, deadlines, shared-process failure and shutdown | System report required and present. Its inner-call timeout finding is retained below; its outer execution-deadline acceptance is compatible with that finding. |
| Ownership/abstractions and test harness | Maintainer report required and present. Extending existing HostedBlocks, Sandbox, Workspaces and gateway caller is earned; no separate engine, scheduler or provider client was introduced. Its placement acceptance does not override the explicit SDK-test authority. |
| Cross-tier rules, tests, docs and generated parity | Standards report required and present. It covers all 34 changed files across server/testing/SDK test/fixtures/architecture/docs. Its three rule findings are independently reconciled below. |
| Security, tenancy and persistent state | Separate security/persistence discovery is not required for this task. Changed code selects process-local resources around the unchanged accepted Caller, tenant-bound evidence/registry and GatewayInvocation. It introduces no SQL capability, schema, identity/permission decision, credential or durable handoff. Existing accepted authority, checkpoint and original deadline consumers are traced by behavior/invariant/concurrency reports; the gateway request retains subject and normal accounting. Model wait affects duration, covered by system review, rather than creating a new financial authority. No missing sensitive contract requiring another specialist was found. |
| Python, TypeScript, UI | No production SDK operation, projection, lifetime, UI or corresponding source is changed. Language/UI specialist omissions are justified. The literal real-client journey remains mandatory but a duplicate capacity story in each SDK would add no new operation proof. |

All selected reports are available. The focused follow-up was justified by a material conflict over proof ownership and established an existing feasible owner; no extra pass solely for a unique proposal was needed. No additional blocker was discovered by this validator, so no self-validated new finding is retained.

## Deduplicated validated ledger

| Stable ID | Discovery IDs | Status | Classification |
|---|---|---|---|
| FIND-001-1 | BEH-LANE-WORKER-OVERFLOW | REVISED | INCORRECT |
| FIND-001-2 | SYS-001 | CONFIRMED | REGRESSION / INCORRECT |
| FIND-001-3 | BEH-UNRESOLVED-RECOVERY-GATE, STD-001 | REVISED | VIOLATION |
| FIND-001-4 | STD-002, FOLLOWUP-001 | CONFIRMED | VIOLATION |
| FIND-001-5 | STD-003 | CONFIRMED | VIOLATION |

Every proposal is accounted for above. Matching reports are consolidated by violated invariant, not counted twice. No rejected proposal creates optional work. The revisions narrow correction scope or qualify proof; they do not waive the defects.

### FIND-001-1 — Combined worker addition can bypass the memory-fit refusal

**Violated obligation:** O11, normal typed/environment refusal of inconsistent lane capacities; preserved combined worker memory ceiling; AGENTS prohibition on input-driven panics.

**Exact owner:** `crates/wyrd/wyrd-server/src/verification/hosted/mod.rs:140–144`, `HostedLimits::lanes_fit`. Producer: `config.rs:1798–1817,1964–1977`; consumers: configuration consistency checks at `:1935` and `hosted/sandbox.rs:80–104` for each lane.

**Independent source proof:** `set_scaled` accepts a positive u64 after checked unit multiplication and conversion to usize. On all supported 64-bit targets, a queued worker value of `18446744073709551615` therefore passes parsing. The default direct worker count is 2. `lanes_fit` adds these usize values before its fallible conversion and checked multiplication. Checked-overflow builds panic while validating input. Wrapping builds calculate 1 worker, whose default 384 MiB fits the 2 GiB ceiling, so configuration incorrectly accepts the impossible aggregate. Both lane startups call the same faulty predicate. Its wrapped worker sum allows the huge queued limit to reach the sandbox pool/admission constructor; that later consumer is not a repair of the false aggregate invariant.

**Reachability/materiality:** this is a malformed operator value the task explicitly promises to refuse, not a requirement to run an enormous worker pool. The unchecked addition defeats the newly changed memory-safety validation itself. No allocation or host load is needed to establish the failure.

**Smallest safe correction:** make the aggregate addition checked in `HostedLimits::lanes_fit`, treating an unrepresentable sum as false before conversion/multiplication. Retain that shared predicate as the single source used by config and both Sandbox startups and retain the existing named ConfigError / PoolError refusal paths. This closes the witnessed invalid state at its producer. No new config framework, count type, repeated consumer guard or pool redesign is needed. The discovery recommendation to add an adjacent sandbox guard is not necessary to close this worker-sum finding: once the shared predicate refuses, this huge worker value cannot reach that addition. This revision does not prescribe a separate sweep of all semaphore limits.

**Burden and proof:** a local checked-arithmetic correction plus focused assertions. Extend the owning pure memory test with one lane at usize::MAX and the other positive; assert false without panic. Extend existing environment validation with the supported-width maximum worker value and assert ordinary named refusal. Run those exact existing unit selectors; no process/load test is needed for this arithmetic property.

### FIND-001-2 — Model-lane waiting does not spend the call's existing timeout

**Violated obligation:** preserve existing budgets/deadline behavior; `WyrdGatewayCall.timeout` is “Remaining time the call may take; the caller enforces it” and `WyrdGatewayCaller` requires completion within it (`crates/skald/skald-agent/src/gateway.rs:43–53`). This is distinct from O18's whole-execution cancellation.

**Exact owner:** `crates/wyrd/wyrd-server/src/components/gateway/caller.rs:128–170`, `ServerWyrdGatewayCaller::call`; the new semaphore wait precedes forwarding the untouched `call.timeout` at `:164`.

**Independent producer-to-consumer proof:** HostedBlocks binds this caller to the selected model semaphore (`hosted/blocks.rs:284–286`). Skald's actual provider adapter supplies ten minutes and a never-cancelled token (`skald-agent/src/gateway.rs:24,86–93,150–160`). The Agent timeout is optional (`loop_runtime.rs:157–163`). A busy model semaphore waits only for admission or that token; only afterward does GatewayInvocation receive the complete original duration. Existing gateway `govern` installs its cancellation child/drop guard and tracked settlement after ordinary admission (`invocation.rs:459–514`). Neither owner accounts for the preceding lane wait. Whole execution timeout still drops the wait, but cannot implement a shorter call bound when the operator has selected the explicitly supported two-hour execution ceiling (`config.rs:4387–4388`).

**Reachable/material scenario:** set queued model calls to 1 with at least two active queued executions and a long original ceiling. Ordinary Agents without shorter authored timeouts share that permit. A second call waiting nine minutes and then taking two minutes upstream succeeds after eleven minutes despite carrying a ten-minute call budget. Sustained contention can leave the call waiting past ten minutes before any gateway dispatch. This retains an active queue slot overdue and can start chargeable provider work after the call's original deadline. No new suspension feature, arbitrary adversarial input or provider-isolation promise is involved. Default shorter execution ceilings merely hide the regression.

**Smallest safe correction:** enforce one original per-call deadline around lane admission and the existing governed invocation in `ServerWyrdGatewayCaller::call`. Expired waits return the existing ProviderError timeout without entering GatewayInvocation; admitted calls cannot obtain a fresh full budget. Use existing Tokio timeout/cancellation and owned-permit drop behavior. Leave GatewayInvocation in charge of its authorization, route, provider cancellation, tracked accounting and settlement. Do not move it, reimplement its lifecycle, add retries/config, or shorten/replenish the enclosing execution deadline. A guard belongs at this caller because it owns both the supplied timeout contract and the newly inserted wait; downstream gateway-only guards cannot see time already spent there. Preserve `lane: None` callers and native request/refusal/response behavior.

**Burden and focused proof:** local deadline enforcement, no dependency or scheduler. Reuse the existing gateway caller fixture in `components/gateway/pg_invocation_tests.rs` (real caller setup and recorded dispatch already exist around `gateway_agent_calls_start_only_on_a_compatible_adapter`). Use a short call duration and held lane permit: expiry returns timeout with zero dispatch, and releasing the hold permits a subsequent call. Also release admission after part of the budget and hold provider completion beyond the original remaining budget; require timeout at the original bound, rather than a fresh full interval. Exercise cancellation/permit cleanup with existing controls and assert the following call can proceed. Run exact new/affected owner tests through the managed Postgres lifecycle; no ten-minute test or production-scale timing experiment is justified.

### FIND-001-3 — Required recovery gate is red because its late-decision setup has no ordering

**Violated obligation:** AGENTS Testing and implementation-execution require diagnosing and fixing a red affected gate regardless of ancestry. The task Evidence explicitly leaves this affected failure unchanged because it predates the write set.

**Exact owner:** `sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs:2065–2194`, `speculative_recovery_cannot_resurrect_content`; response producer `reply` at `:233–264`; task evidence at `tasks/001-isolate-queued-capacity.md:129–137`.

**Independent source proof:** the test documents that comparison decides after admitted extra review finishes, or while excluded extra review is in its second call. In fact, it starts a default Desk, stalls only consolidator and the excluded EXTRA second response, and submits all three cases without holding COMPARE. `reply` immediately answers BOOTSTRAP and COMPARE. Thus compare can become known before extra starts or before its intended publication/invocation milestone. Later assertions demand the speculative disposition (`:2114`), an extra invocation attempt (`:2159` onward) and retained publication. None of the test's producers establishes those required preconditions. The existing sibling `speculative_disposition_controls_consumer_admission` uses HeldUpstream, holds COMPARE and checks `reached` public invocation progress before release; it supplies the missing native mechanism.

**Verification/materiality:** task evidence records two failures at different relevant assertions; the fresh exact managed-Postgres rerun in verification.md fails again (nextest run `a1ff9224-6e3a-4d3e-843d-c1328da801ca`, exit 100). Its bounded output does not expose the exact fresh assertion. Consequently this validation confirms the source race and current red gate, not a claim that the fresh trace independently proves which branch won or a new production recovery bug. Retaining an unreliable required takeover test cannot demonstrate preserved recovery under the changed runtime composition; the explicit gate cannot be waived as pre-existing.

**Smallest safe correction:** repair this test's late-decision setup with the existing HeldUpstream and `reached` milestone mechanism. Hold comparison until the admitted extra action is accepted as completed; for discarded/blocked cases, establish the extra invocation's publication and intended second-call/running state before comparison is allowed to decide. Preserve the existing consolidator stall, lease takeover, no-reexecution counts, mounted-content, original-attempt and retained-publication assertions. Keep the mocked transport response semantics and production engine unchanged. Do not restore arbitrary delays, raise timeouts, loosen assertions or add recovery machinery. The defect is an unestablished producer ordering, so a downstream assertion guard would hide it.

**Burden and focused proof:** bounded test setup adaptation using its sibling's existing controls, with source-visible happens-before ordering. Preserve the diagnosis alongside actual failure evidence, then run the exact recovery selector and directly related `speculative_disposition_controls_consumer_admission` selector under the managed Postgres lifecycle. Do not require a broad SDK suite for this remediation or treat lucky repeated success as a substitute for ordering. This revision consolidates STD-001 and the behavior proposal and preserves the fresh-trace limitation.

### FIND-001-4 — Capacity-composition proof crosses the SDK test boundary

**Violated obligation:** TESTING.md public SDK story rules (`:331–340`), the narrower WyrdTestServer SDK control contract (`crates/wyrd/wyrd-testing/src/server.rs:186–204`), and the original task's expressly selected runtime-composition test owner (`tasks/001-isolate-queued-capacity.md:65–72`).

**Exact locations and path:** registered SDK story `sdks/wyrd-sdk-rust/tests/integration/main.rs:19–20`; `isolated_capacity.rs:153–160,240–249,405–412`. The story holds actual hosted lane and direct-execution permits via `.server().state().verification.blocks()` and repeatedly polls formatted server logs with sleeps. These are runtime steering/inspection facilities, outside the three controls the owner contract permits in every SDK journey. The calls execute on every invocation of this registered story, so this is not dormant test support.

**Materiality and conflict resolution:** mandatory test ownership and public-example boundaries are violated even though the real resource assertions pass. The SDK story is coupled to internal composition and log wording, and the intended server capability lane lacks this proof. Behavior/maintainer reports correctly establish that the underlying real-client proof and resource holds are useful; their preference for reuse of SDK support does not waive the explicit narrower contract. The follow-up resolves the competing feasibility claim: the target already has real Card declaration, graph pull, WyrdState hydration and verify/submit/readback journeys. `wyrd_ai` directly reexports these `wyrd_client` types (`sdks/wyrd-sdk-rust/src/lib.rs:128–132`). Using them from the runtime target preserves AC-010's actual SDK/client → WyrdTestServer/Postgres → client behavior. No approved product decision requires SDK package membership.

**Smallest safe correction:** move this one composition story into the existing `wyrd-testing` `server` integration target under verification-runtime ownership (`tests/bifrost/server/main.rs` already registers `verification_runtime`). Adapt its small story-local provisioning to existing bound-server/bootstrap/native-client and Card declare/pull/hydrate patterns (`verification_runtime.rs:5381–5452,7851–7910`). Use the target's existing `wyrd-client`, `wyrd-server` test support, `wyrd-spec`, wiremock and tempfile dependencies. Public gateway administration and the existing CredentialWriter can provision the mock route without importing CLI/SDK integration support. Keep fixture YAML at its current shared repository location and retain real client execution/readback. Remove the SDK module/source registration once the proof is registered in its owner. Do not create a second reusable harness, add a reverse SDK dependency, expose extra sanctioned SDK controls, or merely hide the internal accesses behind a wrapper.

The existing lane holds and live LogCapture remain valid runtime-tier mechanisms. Preserve all distinct queued execution/workspace/Python/model saturation cases; direct results through the real client; pending/deferred/continuation progress; reverse no-borrowing; cancellation/release; and tenant/subject gateway row assertions. A pure semaphore unit test, EngineScript, seeded outcome or direct engine call would not close AC-010. Keep ordinary SDK stories in place; duplicating the composition story there is unnecessary.

**Burden and focused proof:** fixture/import adaptation is larger than a path-only move, but existing source demonstrates every required operation without a new harness/dependency. Deleting the proof or changing the SDK contract is not a smaller valid correction. Compile the existing `server` target, then run the moved test's exact nextest selector under the managed Postgres lifecycle. Inspect preservation of all current assertions and genuine client round trips. Existing mise `test:bifrost:journey:server` already selects this binary, so no lane/target addition is needed; the complete lane belongs to final integrated change review, not this bounded remediation.

### FIND-001-5 — Changed defaults test lacks its mandatory panic contract

**Violated obligation:** root AGENTS changed-item rustdoc rule, including `# Panics`; `agent_docs/rust.md:233–241` explicitly covers private/test items and makes placeholder/missing contract documentation blocking.

**Exact owner and source proof:** `crates/wyrd/wyrd-server/src/verification/mod.rs:626–665`, `production_defaults_are_the_specified_ceilings`. The candidate materially extends this test to pin queued slots and the five-minute ceiling. Its updated purpose documentation has no `# Panics`, while its assertions intentionally panic when default invariants change. The full body retains other default assertions; documenting only the newly added assertion is insufficient to describe the item's contract. Nearby changed memory/config/eligibility tests demonstrate the applicable documented-panic pattern.

**Materiality/correction:** this is an explicit mandatory gate, not an invented documentation preference. Add the missing panic contract describing failure of the asserted default ceilings/invariants, retain every assertion, and bundle it with substantive fixes above. No new helper/test, separate style round or changes to unchanged neighboring items are warranted. Closure is source inspection and the exact existing defaults-test selector already used in fresh verification.

## Reconciled acceptance matrix

`J` denotes the full new capacity story inspected above. Fresh J and four exact server units pass in verification.md; other supplied checks remain implementation-reported evidence, not newly rerun here. A behavior PASS below does not waive the separate mandatory proof-owner/gate findings.

| Obligation | Source and observable proof | Result |
|---|---|---|
| O1 execution isolation | Direct request owns `admit_direct`; queued owner remains runtime ClaimLoop. J seats the only queued slot on the held model call, then direct assert succeeds while second queued result is Pending. | PASS |
| O2 workspace isolation | HostedBlocks selects distinct Workspaces. J holds queued workspaces while direct writer's `written` requirement passes. | PASS |
| O3 Python isolation | Selected sandbox used by CodeExecutor and HostedTools. J holds queued Python while direct `measured` passes. | PASS |
| O4 model isolation | Distinct model semaphore before existing GatewayInvocation. J observes queued wait and direct `answered` passes. | PASS |
| O5 queued execution wait/progress | Existing runtime slot stays owned across process execution. J asserts Pending before release and both queued requirements pass after release. | PASS |
| O6 workspace defer/progress | Existing Busy→Defer owner retained. J requires exact execution deferral then `written` passes after release. | PASS |
| O7 Python defer/progress | Same existing deferral path. J requires exact execution deferral then `measured` passes after release. | PASS |
| O8 queued model wait/progress | Owned lane acquisition. J witnesses MODEL_WAIT while held and `answered` passes after release. | PASS |
| O9 no borrowing | Exhaustive lane selection, independent direct permits. J asserts direct workspace/Python/execution 503 and direct model waits while queued lane remains free. Queued twins complete. | PASS |
| O10 mode/reclaim routing | Sole runner engine initializer sets Queued; direct sets Direct. J resumed Python/workspace runs pass while corresponding direct admission remains held. | PASS |
| O11 typed/env limits and original ceiling | Normal values reach retained owners and boot runtime builder; fresh config unit checks both full lanes and two-hour deadline plus malformed/scaled/inconsistent inputs. Combined worker overflow remains unrefused: FIND-001-1. | FAIL |
| O12 defaults | Fresh memory/default units assert total workers/workspaces/waiters/prewarm `(5,16,16,2)` and queued `(64,five minutes)`. Mandatory panic documentation is separately FIND-001-5. | PASS behavior |
| O13 pre-step eligibility | `resolve` calls direct_eligibility before admission/engine; fresh pure test refuses Window and accepts ordinary graph; supplied Window journey asserts stable refusal. Future queued-only forms belong to their introducing tasks. | PASS |
| O14 ordinary tenant/subject/accounting | Selected caller passes unchanged captured identity and subject into GatewayInvocation. J reads six gateway rows under the tenant and asserts every subject is the declared Service UID. | PASS |
| O15 owner/operator documentation | Design and both operator pages describe separate resources/no borrowing/shared downstream capacity/original ceiling including approvals; actual settings match config. Supplied docs:check includes generated parity. | PASS |
| O16 explicit fmt/lints/deps/diff checks | Supplied fmt/lints/check:deps pass; fresh cumulative diff check passes. Broader affected red verification remains separately mandatory. | PASS for named checks |
| O17 one engine | Both callers use HostedBlocks::engine; Lane composes existing resource owners; no alternate engine introduced. | PASS |
| O18 execution cancellation | Owned model permit and enclosing execution deadline cancel pending future; J asserts exact ExecutionTimedOut while held, then successful direct Agent after release. This does not prove per-call timeout preservation. | PASS |
| O19 ordinary direct graphs | Existing engine/executors retained; supplied hosted_agent_tools and Window owner journeys pass, with J exercising actual Agent/tool/code flows. Separate recovery gate remains red. | PASS within direct-graph scope |
| Additional task requirement: existing per-call deadline/budget behavior | Skald supplies finite timeout but new wait does not spend it; FIND-001-2. | FAIL |
| Additional mandatory affected test gate | Fresh exact recovery selector remains red and setup lacks required ordering; FIND-001-3. | FAIL |
| Additional proof ownership/public SDK test boundary | Client proof exists and passes, but runtime access/log polling is registered in the SDK tier; FIND-001-4. | FAIL |
| Additional changed-item rustdoc gate | Defaults test's panic contract missing; FIND-001-5. | FAIL |
| INV-001 accepted authority, evidence and tenancy | Existing accepted Caller, tenant evidence/registry, route and GatewayInvocation permission/accounting owners retained; no identity/SQL contract change. J attribution and traced producers/consumers corroborate. | PASS |
| INV-005 original persisted execution state | Runner's functional composition diff is Queued mode; accepted authority/checkpoint restoration and original remaining-age deadline remain with existing owners. No retry budget replenishment introduced. Separate per-call duration regression above remains unresolved. | PASS for persisted execution state |
| INV-007 and task non-goals | No new durable queue, table, generic scheduler, process service, platform enforcement, host-load test or completion SLA; no future extraction/deferred/approval action introduced. | PASS |

The obligation ledger omits the mandatory test boundary, rustdoc and affected-red-gate obligations and does not distinguish inner model-call timeout from whole-execution cancellation. These gaps are reconciled explicitly above rather than silently inheriting its all-PASS table. The historical production-before-RED process exception is accurately recorded: later mutation RED proves sensitivity, not historical test-first order. No current source correction can reverse that chronology; it creates no separate remediation machinery.

## Verification limits and handoff

Fresh verification establishes four selected server units and the exact capacity journey pass, and the exact recovery journey fails. It does not prove overflow refusal, inner-call elapsed-budget behavior or corrected proof placement. Supplied formatting/lints/dependency/docs and broader owner-slice results are credible recorded evidence but were not independently rerun by this validator. No new Cargo checks, platform matrix, synthetic load, broad gate or whole journey suite were run here. Static diagnoses above distinguish these limits from actual proof.

One bounded remediation can package the five owner corrections, including the panic documentation. It needs no new product decision, spec revision, service, persistent contract, dependency or approval. Preserve the approved reservation-only scope, existing gateway lifecycle and real client proof. Remediation closure should inspect the original cumulative subject, each focused correction and affected consumers; no prior accepted behavior needs rediscovery absent a fix impact or new evidence.

Completion HEAD: `34fb45bb387582d2aedfee77fb4e09fa2753b80d` (unchanged).
