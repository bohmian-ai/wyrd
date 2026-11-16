# Task 001 behavior review

Overall: **FAIL**. Proposed source-local findings: `BEH-LANE-WORKER-OVERFLOW`, `BEH-UNRESOLVED-RECOVERY-GATE`.

## Immutable subject and coverage

Base `395274d58bf76a2075f245ca3a5c1bd044925027`; candidate `34fb45bb387582d2aedfee77fb4e09fa2753b80d`. First review of the complete cumulative diff, including all production, harness, fixture, journey and documentation changes. No carried-forward acceptance. HEAD matched the candidate during review. Source was not modified and this reviewer ran no Cargo commands.

Read the original task and its obligation/evidence ledger, approved spec revision 3 (REQ-015/016, AC-010, INV-001/005/007), approved runtime design capacity/eligibility/waits sections, root and docs AGENTS, architecture authority map and Verifier Agents/tools/workspaces owner, agent_docs index/Rust/errors/maintainer-style, TESTING and spec-driven workflow. CodeGraph was used before source navigation; queries returning unrelated symbols were followed by targeted source reads. Other reviewers' conclusions were not inputs.

Traced config/environment -> boot -> HostedBlocks lanes -> direct control/queued runner -> shared engine -> CodeExecutor/HostedTools/gateway caller; direct target resolution before engine construction; sandbox/workspace admission and drop; runner claim/defer/reclaim and original deadline; caller identity/subject preservation through the unchanged GatewayInvocation call. Inspected changed tests/fixtures and the actual assertions in the SDK isolated-capacity journey. Independently inspected the failing orchestration test, its shared scripted reply, sibling milestone-based test and ancestor f04384dbf.

## Acceptance matrix

`J` below means `sdks/wyrd-sdk-rust/tests/integration/isolated_capacity.rs`; its assertions were read directly. Fresh orchestrator verification in `verification.md` records the managed-Postgres journey passing in 26.801s and the four exact server tests passing. Implementation-reported checks are distinguished from those fresh results.

| Obligation | Implementation evidence | Verification evidence and concrete assertion | Result |
|---|---|---|---|
| O1 queued execution saturation preserves direct execution | service.rs:508 takes direct_executions; runner owns separate capacity | J:288–313 holds queued model admission, witnesses queued wait holding the only execution slot, verifies direct assert, observes second queued execution Pending | PASS |
| O2 queued workspace saturation preserves direct workspace execution | blocks.rs:247,270–275 selects lane.workspaces | J:328–338 holds queued workspace; direct writer's `written` requirement passes | PASS |
| O3 queued Python saturation preserves direct Python | blocks.rs:290 selects lane.sandbox for CodeExecutor | J:315–325 holds queued Python; direct `measured` requirement passes | PASS |
| O4 queued model saturation preserves direct model calls | blocks.rs:283–286 installs selected model permits | J:288–296 witnesses queued MODEL_WAIT and direct `answered` passes before release | PASS |
| O5 queued execution waits and progresses | existing runner semaphore retained | J:302 Pending assertion, then both queued results passed after drop(model_held) | PASS |
| O6 queued workspace waits and progresses | existing workspace Busy propagates to runner Defer | J:335 observes DEFERRED for execution ID; `written` passes after release | PASS |
| O7 queued Python waits and progresses | existing sandbox Busy propagates to runner Defer | J:322 observes DEFERRED for execution ID; `measured` passes after release | PASS |
| O8 queued model waits and progresses | caller.rs:129–145 waits on selected semaphore before GatewayInvocation | J:290 MODEL_WAIT queued, J:312 answered after release | PASS |
| O9 no borrowing | blocks.rs lane match selects distinct retained owners | J:360–391 direct Python/workspace 503 while queued twin passes; direct model waits until its own permit releases; J:415–420 direct execution 503 | PASS |
| O10 routing includes continuations | runner.rs:821 sets Queued for every construction; service.rs:582 Direct | J:323–338 queued deferrals complete while corresponding direct resource is held | PASS |
| O11 isolated limits and original ceiling use typed/env validation | config.rs:1798–1827 and boot/mod.rs:1398; worker sum in hosted/mod.rs:141 is unchecked | Fresh config test verifies ordinary overrides, zero/negative/text/scaled-overflow and inconsistent memory cases, but omits worker-count sum overflow described below | FAIL |
| O12 defaults preserve combined capacities and queued ceiling | HostedLimits defaults 2+3 Python, 4+12 workspaces, 4+12 waiters, two warm workers; runtime default 64/5min | Fresh lanes_split_the_memory_ceilings asserts (5,16,16,2); production_defaults asserts (64,5min) | PASS |
| O13 direct eligibility before first step | resolve calls direct_eligibility before returning target; admit/build/run happen later | Fresh eligibility test asserts Window -> VerificationInputUnsupported and assert-only -> Ok; existing Window journey reported passing with expected refusal | PASS |
| O14 ordinary tenant/subject/accounting path retained | caller.rs:147–170 constructs unchanged request with captured Caller/subject and calls GatewayInvocation | J:348–353 asserts exactly six retained gateway rows and every subject equals capacity-desk UID, queried through tenant client | PASS |
| O15 authority/operator docs | architecture Verifier section; configure-verification-runtime; verification-runtime-limits; llms projections | Read actual changes: reservation/shared-resource boundary, no borrowing, original deadline and approval period documented; docs:check reported exit 0 | PASS |
| O16 required fmt/lints/deps/diff checks | Task evidence records each required command | fmt clean, lints/check:deps exit 0 reported; fresh cumulative diff --check exit 0 | PASS |
| O17 one engine | both callers build through HostedBlocks::engine; Lane contains resources, no separate evaluator | J exercises both modes through SDK; same Blocks/VerificationEngine composition read | PASS |
| O18 cancellation of lane waits | caller.rs select cancellation arm; owned permit scope covers gateway call | J:393–398 asserts ExecutionTimedOut with direct model held, then direct Agent passes after release | PASS |
| O19 ordinary direct graph regression boundary | engine/executors unchanged except resource selection | Existing seven hosted_agent_tools and Window journey reported pass; source assertions preserved. The separate recorded orchestration failure is handled as the additional mandatory gate below, not misrepresented as direct-graph failure | PASS |
| Additional: no new generic scheduler/service/queue/schema/platform enforcement | diff adds resource ownership within existing HostedBlocks; no Cargo dependency, SQL migration or new process service | Complete changed-file inventory and caller tracing; existing queue transition owner retained | PASS |
| Additional: tenant authority/evidence/budgets/deadline preserved on retry | Caller::accepted unchanged, selected lane only changes concrete resources; runner still derives deadline from remaining/age and restores state | Read runner construction/attempt and gateway call; J gateway rows and deferred completion; existing durable-hosted owner tests reported passing | PASS |
| Additional: mandatory affected verification gates cannot be waived because failure predates task | task evidence:129–137 leaves twice-failed speculative recovery test unchanged specifically because outside write set | Independent source diagnosis below confirms missing ordering; this is an unresolved red gate under AGENTS Testing | FAIL |

## Existing owners and correction burden

The capacity change extends the correct existing owner. HostedBlocks already retained Sandbox and Workspaces and constructed both direct and queued engines; a private Lane groups separate instances of those owners plus one native Tokio semaphore. Keeping the existing single sandbox/workspace instances could not reserve capacity without changing their admission ownership. The queue's execution semaphore stays in VerificationRuntime; a new scheduler is unnecessary. ServerWyrdGatewayCaller is already the common tenant-authorized gateway adapter, so attaching selected admission there preserves all request projection/accounting rather than duplicating provider transport. Other callers retain lane None. The typed configuration owner and ExecutionMode already exist. No new dependency or speculative extension was found.

The test holds use existing semaphores rather than synthetic load. Real SDK requests execute resource-specific fixtures through WyrdTestServer/Postgres and read results through SDK. The test harness extends existing LogCapture because server work runs on other runtime threads. Nextest isolates test processes; its process-global subscriber does not mutate production telemetry behavior. The SDK target placement differs from the task's proposed wyrd-testing server target, but preserves the stronger required SDK/server/Postgres/SDK proof and reuses the existing SDK deployment owner. No correction is needed merely to move the test.

## Proposed findings

### BEH-LANE-WORKER-OVERFLOW — INCORRECT

**Obligation:** O11's typed/environment refusal of inconsistent limits and the preserved Python pool memory ceiling. Root AGENTS forbids panics on input; the runtime reference promises startup refusal naming inconsistent settings.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/mod.rs:141` (`HostedLimits::lanes_fit`), producer `config.rs:1803–1807`; affected consumer `hosted/sandbox.rs:85–103`.

**Source-proven scenario:** on every supported 64-bit platform, `WYRD_VERIFICATION_QUEUED_PYTHON_WORKERS=18446744073709551615` is a positive u64 and fits usize, so existing set_scaled accepts it. With the direct default of 2, lanes_fit performs usize addition before any checked conversion/multiplication. In checked builds it panics while loading config rather than returning ConfigError. With wrapping arithmetic the sum becomes 1, so the 2GiB pool check incorrectly passes. Sandbox receives the unbounded queued worker maximum; its next unchecked `workers + python_waiters` wraps to 11 with the default 12 waiters. The declared sum that should have been refused therefore reaches pool construction with a false memory-fit result. No request-specific downstream boundary repairs that invariant.

**Materiality:** this is an explicit invalid-configuration/memory-ceiling obligation, not a demand to support that worker count. The failure defeats exactly the startup validation this task changes and the new combined-pool safety calculation. Checked multiplication after an unchecked addition is insufficient.

**Sibling producers/consumers:** both DIRECT and QUEUED PYTHON_WORKERS env variables feed the same HostedLimits; both Sandbox::start calls use lanes_fit, as does configuration validation. Python waiter counts also feed the sandbox capacity addition. Workspace/model/execution counts use different independent owners and are not needed to establish this defect.

**Smallest correction:** use checked addition at the shared HostedLimits combined-worker calculation so overflow returns false through the existing config Invalid / sandbox Spawn refusal paths. Keep existing worker memory math and lane ownership. Review the adjacent sandbox admission sum at its owner and refuse an unrepresentable capacity rather than wrapping. No new config type, validator framework or downstream guard is needed.

**Closure proof:** an owning pure test supplies usize::MAX for one lane's workers and a positive other lane and asserts `!lanes_fit()` without panic; the existing config test sets the corresponding env count and asserts the normal named Invalid refusal. A focused capacity-overflow case should assert Sandbox refuses unrepresentable workers+waiters if that arithmetic remains reachable. This needs no load test or new dependency.

### BEH-UNRESOLVED-RECOVERY-GATE — VIOLATION

**Obligation:** AGENTS Testing: “A red gate is yours to fix whoever turned it red; don't spend effort proving a failure predates you.” Task preservation of cancellation/ordinary engine behavior requires credible affected verification.

**Location:** `changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md:129–137`, unresolved test `sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs:2049–2159`.

**Evidence and independent diagnosis:** the supplied evidence records the same affected test failing twice, with assertions at 2114 and 2159, and explicitly declines correction because it predates this diff. Reading the test confirms it uses Desk::start and submits three LATE-DECISION cases without holding COMPARE. Shared reply returns BOOTSTRAP and COMPARE immediately. Ancestor f04384dbf removed the COMPARE LATE-DECISION delay and updated speculative_disposition_controls_consumer_admission to order races with HeldUpstream/reached, but did not update this recovery case. The recovery case only stalls the consolidator (and a later EXTRA reply for excluded cases), so it never establishes that the extra invocation/publication exists before the comparison decides. Its 2114 assertion requires recorded speculation; its 2159 tuple requires an invocation attempt. Either can fail when compare wins. This explanation matches both reported failures and does not depend on a new lane starvation claim.

**Consequence/materiality:** the affected recovery regression gate remains unreliable/red, so it cannot prove that a takeover preserves recorded admitted/excluded content under this changed resource composition. This is an explicit required gate, not a production regression accusation or unrelated refactor request.

**Smallest correction:** reuse the existing HeldUpstream and reached milestones in this same test to establish its documented late-decision precondition. Ensure admitted extra completion is accepted before releasing compare; for excluded cases establish the intended running invocation/publication milestone before the decision. Preserve cancellation, takeover and retained-publication assertions. Do not restore timing delays, relax assertions, increase sleeps/timeouts or change engine behavior. Keep correction within this test and already-existing test helpers.

**Closure proof:** run the exact speculative_recovery_cannot_resurrect_content managed-Postgres selector and the existing sibling speculative_disposition_controls_consumer_admission selector. Establish source-backed ordering rather than relying on a lucky green repetition. The orchestrator's fresh exact recovery-test rerun also FAILED on the immutable candidate (exit 100, 16.857s, nextest run `a1ff9224-6e3a-4d3e-843d-c1328da801ca`; command/result in verification.md). Its exact panic text was unavailable in the bounded output, so the observed failure does not independently distinguish the two recorded assertion branches. Both branches are explained by the source race above.

## Process exceptions and verification limits

The recorded mutation RED is credible: forcing Direct to use queued resources makes the held queued model admission block the direct Agent, which J detects. It does not establish that implementation followed the required test-first sequence; the task candidly records production-before-test. This historical ordering cannot be undone by rewriting source now, so no separate machinery or remediation round is proposed for it. Preserve the accurate exception record.

No test assertion, timeout, retry, ignore or allow was changed in this diff to hide the diagnosed recovery failure. Existing ignored journey annotations were not newly invented as skips; the new journey uses the existing managed Postgres convention. The failure is nevertheless not waivable under the explicit red-gate rule. The claimed read-only diagnostician report was not supplied separately; this report independently establishes the causal source path rather than treating that claim as proof.

Fresh checks and implementation-reported results are recorded above. This review did not rerun docs, lints, the entire owner slices or the platform matrix; no new SDK operation was introduced, so Python/TypeScript mirror journeys are not required for this capacity proof. Sensitive shared lifecycle/concurrency and cross-tier standards remain subject to the separately selected specialists. No source was changed.
