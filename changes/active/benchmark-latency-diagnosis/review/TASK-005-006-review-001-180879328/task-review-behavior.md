# Independent behavior review — TASK-005 and TASK-006

Result: **FAIL** for both tasks, for the proposed material findings below. This is discovery, not the final validated verdict.

Subject: `0c81715f2...180879328ac9dde94d7504855e821980e5db0f18` in `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`. First review; no earlier acceptance carried forward. Reviewed current revision-7 spec with TASK-005's revision-6 extension and TASK-006's revision-7 obligations. The complete range and changed-file inventory defined discovery, not the supplied implementation summary. Untracked work excluded. Candidate HEAD remained pinned at inspection.

## Scope and evidence

Used CodeGraph first for Agent, Prompt, result-reader and Agent-executor navigation, then read changed bodies/callers and actual Git diffs. Inspected root/nested rules, workflow, maintainer guide and relevant Rust/error/surface guidance. Public contract authority is `architecture/wyrd-design.md`; task/spec requirements below are explicit acceptance obligations. Governance acceptance is inspected at `GatewayUsage`/`GatewayLedger`/invocation/owner boundaries, with independent invariant, data, security and resilience reports expected to provide specialist coverage. This report does not claim to replace those reports.

Primary traced paths:

- Explicit process startup → `wyrd_benchmark::install` / `enabled` → SDK `Observation` → HTTP attempt/body/decode observations → process-qualified `Population` → `Record`/canonical renderer.
- Capacity profile selection → `ProfileMode` → driver/server targets → step-scoped CPU capture and cumulative heap capture → retained role/mode/binary/window evidence. Driver remains outside `Workload::processes()` server resource accounting.
- WyrdState direct Verify → local model fixture → hosted Agent resolve/bind/run → declared Verdict validation/citation filtering → engine `ExecutedOutcome` → unchanged stored summary/output/input shape → compact response.
- Explicit HTTP/MCP/SDK details → shared `VerificationControl::read` → `InputResolver::result<OutcomeDetails>` → authored `Check<Authored>` decoding. Ordinary status uses `VerificationOutcome`, and no SDK default status/wait calls details.
- HostedBlocks' retained `Arc<PreparedSchemas>` → AgentExecutor → effective post-render/post-hook schema selection in `run_loop` → exact schema validator; declared Verdict and custom-data bounds still checked separately.
- Agent `new`/`from_resolved`, `with_prompt`, reference-backed construction, public prompt reassignment, `prompt_ref`, `to_spec`, `to_card`, YAML/wire projection and cloning.
- Gateway invocation local admission/settlement → usage batching/sync/apply → owner background worker/drain. Replay and failure details are assigned to the selected specialists, rather than assumed from healthy-path test counts.

Existing-owner comparison: observations extend the existing recorder/snapshots, profiles extend the experiment owner and existing sampler, details extend the execution reader and SDK handle, exact validators extend Skald/Vala owners and HostedBlocks lifetime, and governance extends Gateway's owner/tracker and SQL capability. I found no independent behavioral reason to replace these owners or add another registry/store/transport. The lazy reference issue below is within Agent and can be corrected there.

## Acceptance matrices

PASS below means source-backed implementation plus inspected focused test/evidence claims supports this review row; it does not claim a test was rerun here. Rows requiring specialist reconciliation are marked explicitly.

### TASK-005

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-017; AC-013: explicit client/server sole recorder, ordinary clients inactive | `wyrd-benchmark/src/lib.rs::install,enabled,Observation::start`; `capacity/main.rs::start_benchmark`; `capacity::install_tracing`; server `app/metrics.rs` startup selection; shared-client plain recorder dependency | Capacity startup/default-off/conflicting-recorder tests and recorded task checks; `enabled` false without bench | PASS |
| REQ-003/017/018; AC-013/014: logical roots, attempts, outcomes, process/cohort distinctions | Client `transport/http.rs` wraps logical calls and separate send attempts, body/decode and replay; `Observation::Drop` preserves unfinished root as cancelled; Cohort metadata; `Population` roles and reconciliation | Focused observation, transport replay and driver phase/process tests inspected; complete release-server lifetime census remains unverified | FAIL: BEH-005-01 |
| REQ-005/006/018; AC-014: honest controlled-path waits, original boundaries retained | Gateway invocation spawn/Answer sent timestamps; provider ExchangeTimer header/body intervals; Verification setup phase; model diagnostics explicitly describe loopback fixture and excluded TLS/auth/retry/remote paths | Existing exchange and Gateway diagnostic tests and model journey evidence; diagnostics label overlapping phases rather than sum percentiles | PASS |
| REQ-019; AC-015: symmetric CPU/heap modes and historical compatibility | `profile.rs::{ProfileMode,Profiler,HeapProfiler}`; experiment step targets include Driver separately from server accounting; shared heap behind default-off bench; historical Role/Mode defaults | Mode/role/window/profile-manifest tests; task retains both role captures, source-line caveat and cumulative heap/warmup bias | PASS |
| REQ-020; AC-016: real local Gateway Agent diagnostic | Verification model steps are opt-in declarations; `workload.rs` judge graph; `run.rs::call` requires pass plus citation; PacedUpstream returns cited Verdict/usage | Local full-path Gateway model journey; recorded success/upstream/citation populations; assertion controls remain distinct | PASS |
| REQ-021; AC-017: owner-local safe reductions | Canonical borrowed string visitor preserves serializer fallback; Google supplied token omits network client; `PLAIN_VERDICT` LazyLock; borrowed inline Agent resolution | Canonical Unicode/escape/duplicate/depth/numeric tests; Google source/client tests; concrete profile/source rejection table for tracing/hosted setup/capture | PASS |
| REQ-003/007/011/022; AC-018: qualified observer and complete canonical performance evidence | Existing recorder/Population/Record supports role and diagnostic regeneration | TASK-005 lines 706–725 explicitly says qualification unresolved/not passed; many U→M envelope misses and invalid M Verification; lines 778–783 outstanding census/calibration | FAIL: BEH-005-01 |
| REQ-023; AC-019: useful compact default on direct/status/wait/typed reads | `VerificationOutcome` has verdict/requirements; `RequirementResult` omits check/provenance; `ExecutedOutcome::compact`; all SDK/default readers use compact contract | `compact_results_preserve_judgments_without_configuration`, typed-data and SDK key/equality journeys | PASS |
| REQ-024/025; AC-020: faithful authored details and unchanged stored facts | `Check<F>`, `AgentSpec<F>` with authored form; `StoredOutput` still requirements/steps; ResultPayloadBuilder and InputResolver retain input/output/summary; direct staging preserves input declarations independently of response | `execution_details_preserve_authored_checks_without_compilation`, retained-details route test and SDK Bifrost row comparisons; historical authored data remains structurally decoded | PASS |
| REQ-026; AC-021: explicit absence, authority and asynchronous publication | Details calls same `VerificationControl::read` authority and Bifrost caller as status; `execution_not_found`; no default detail IO or durability wait | Visibility/read-authority test covers unknown, missing, read/run-only denial, tenant mismatch and audit. Security specialist must reconcile shared authority coverage | PASS, subject to specialist reconciliation |
| REQ-027; AC-022: all public projections/consumers migrate | HTTP sixth operation, unscoped MCP details tool, shared-client handle, Python mapping/typed sibling and stub, TS native/wrapper/types, Rust exports, docs; old full-field journeys explicitly read details | R6 tests across SDK/MCP/OpenAPI; valid R0→R1 unprofiled/heap and separately disclosed invalid R0 CPU pass, response byte and CPU/heap comparisons | PASS for contract migration and disclosed compact comparison; observer prerequisite remains BEH-005-01 |

### TASK-006

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003/005/011/018/033; AC-025: canonical saved Gateway diagnostics | Verification `run.rs::{set_gateway,component_totals}` and declaration diagnostics recover Gateway phases/waits, mark assertion cases inapplicable | `diagnostics_include_gateway_phases_and_waits`, existing canonical regeneration tests and recovered distributions | PASS |
| REQ-024/032; AC-025: reuse exact preparation without changing validation | HostedBlocks owns schemas across executions; exact serialized schema keys with object-root setting; loop runtime selects outgoing post-hook schema; AgentExecutor still validates declared Verdict and filters citations; CodeExecutor still uses verdict_from | Effective-schema bound/replaced/repair tests; schema identity/eviction test; Verdict/custom-data tests and SDK negative journeys | PASS |
| REQ-028/029; AC-023: local atomic approximate governance and answers before SQL | `GatewayLedger::admit` synchronous, `GatewayUsage::admit` under memory lock, current limit/budget view; invocation in-memory settlement before buffered answer | Paused-SQL admission test and Gateway cached-governance journey; independent invariant/data reviews required for all races and persistence | PASS at traced local path; subject to specialist reconciliation |
| REQ-030/031; AC-024: background reconciliation, refusal/recovery, cancellation/drain | `GatewayUsage::{sync,batch,apply,run,flush}`, snapshots admitted/reloads, owner refresh/drain, tracked CallTask completion; bounded tenants/entries/sync concurrency | Retry/remote-update and stale/restart/policy tests. I do not infer full pending/drain correctness from these healthy-path passes; selected invariant/system/data reports own adversarial closure | PASS only at inspected paths; specialist reconciliation required |
| AC-026: owner-native changes and no extra service/global quota/WAL | Existing Gateway worker/task ownership and SQL TenantConn; no separate coordinator/store/transport | Source/manifests and focused recorded lanes | PASS, subject to specialist reports |
| Bench-only phase timing never active in production | `vala-verify/blocks/agent.rs::phase_start` calls enabled before Instant; phase_end short-circuits None before clock/histogram; ordinary recorder build has no install and constant-false enabled; production dependencies do not enable bench | Source/cfg/manifests inspected; task reports ordinary cargo feature tree and production build; phase test bench dependency is dev-only | PASS for activation/default-production behavior; standards review covers manifest closure |
| Owner-local reductions preserve supported runtime/Card behavior | Authored Prompt snapshot equality preserves prior request/model/provider mutation detection; render/bind share equivalent substitution; lazy Agent reference depends on first read | Existing render/schema/Agent tests do not exercise public reassignment around first reference read | FAIL: BEH-006-01 |
| REQ-022/033; AC-025: final implemented Agent reductions have attributable performance proof | Prompt serialization, render substitution and lazy reference reductions are present | TASK-006 lines 425–427 explicitly says final reductions 1–3 unmeasured; retained 15:02 reference predates them | FAIL: BEH-006-02 |

## Proposed findings

### BEH-005-01 — Required measurement qualification remains incomplete

Classification: **MISSING**. Task: TASK-005.

Obligation: REQ-022 / AC-018 and task bounded protocol lines 626–632 require qualifying U→M collection cost before attributing optimization, within the explicit nonregression envelope. Task Scenario 2 also requires the existing all-engine lifetime census. These are acceptance proofs, not an optional demand for tighter performance.

Locations: TASK-005 lines 706–725 (U→M misses and unresolved qualification), 778–783 (unverified real-server census/calibration and host-confounded comparisons); implementation proof owners `capacity/tests/pg_tests.rs:597` and `verification/run.rs:1900`.

Evidence: U→M lists misses in client/server latency, CPU or throughput for every control family, then states the qualification is unresolved, not passed, with M Verification INVALID. The reported M→O changes on assertion/payload controls also remain unresolved under concurrent host load. The two independent-reference release tests were attempted concurrently, then refused on host-busy retries, so no completed success exists. The later R6 compact response comparison is a useful separately disclosed result but does not retroactively establish the instrumented collector's qualification prerequisite.

Consequence/materiality: the task can return plausible canonical distributions and even valid runs without the required independent proof that measurement windows/process populations and observation cost meet their acceptance conditions. Declaring task completion would waive its explicit measurement-quality gate. This is not a claim that production is slower or that the 0.5 ms target failed.

Smallest correction: use existing measurement/qualification owners and retained evidence first; complete the two exact existing release-server proofs sequentially with the repository Postgres lifecycle on an idle host, and resolve the U→M qualification explicitly using comparable retained evidence. If those records cannot distinguish host load from observer overhead, present the smallest current-code observation-cost comparison for separate authorization and use the existing collector/canonical reports; do not rebuild historical baselines, relabel invalid records, or add another measurement system. If a collector defect is revealed, correct that owner and rerun its focused proof. Record an honest failed/unresolved result until the acceptance proof actually closes. A new spec revision is not needed to complete this existing obligation.

Burden: no new runtime machinery or broad suite; existing tests and retained milestone inspection are the first rung. Any necessary fresh measurement keeps the approved authorization boundary. Focused closure: completed exact census/calibration selectors plus canonical evidence satisfying the original observer-cost acceptance, or a still-failing task row. The review ran none of these load-generating tests.

### BEH-006-01 — A read now changes which Prompt public reassignment exports

Classification: **REGRESSION**. Task: TASK-006.

Obligation: TASK-006 preserves supported Agent behavior while applying owner-local latency reductions; spec INV-001/003 preserves product/identity and execution semantics outside selected changes. Agent's pre-candidate `prompt_ref` is explicitly preserved separately from its public resolved runtime `prompt`; `with_prompt` is the operation retargeting both. This finding preserves that existing distinction and does not require a new public mutation contract.

Locations: `crates/skald/skald-agent/src/agent.rs:262–274,303–308,588–590,816–823`. Consumers include `to_spec`, `identity_card`, `to_card`, `to_wire`, YAML projection, `cascade_children` and clones.

Reachable failing path:

1. Build `Agent::from_resolved("a", A)` (or `Agent::new(A)`). The new OnceLock is empty.
2. Assign the already-public field `agent.prompt = B`.
3. Call `to_spec()` or `to_card()`: candidate initializes and exports B.
4. Repeat the same construction and assignment, with one intervening read `agent.prompt_ref()` (or `to_spec`, `card_ref` with metadata, `cascade_children`, or cloning a warmed Agent). The candidate instead freezes A and continues exporting A after assignment.

Before the change both sequences preserved A as the stored reference; an explicit `with_prompt(B)` retargeted it. Reference-backed construction still immediately preserves its reference, so the new read-order behavior is specific to the optimized resolved constructors. The runtime uses current B either way. A read consequently determines which inline Prompt a subsequent public Card export/declaration carries, independent of the final runtime fields. This can persist a different Prompt body without an explicit retarget operation. It is an observable behavior change, not merely an untested cached accessor.

Smallest correction boundary: Agent's lazy reference source. Preserve the construction/explicit-retarget Prompt independently of the mutable public runtime field, using its existing cheap shared Prompt ownership as the lazy source; derive inline native data only when read. Keep reference-backed construction's stored reference and preserve `with_prompt` / `try_with_prompt` retargeting. Do not privatize/remove the public field, eagerly restore the per-check deep copy, or make each downstream exporter guess whether a cached value is stale. These alternatives either change API/semantics or defeat the selected optimization. Exact private field layout remains implementation-owned.

Burden/proof: one owner-local correction, no new service/dependency/cache policy. Add a focused existing Agent construction/projection regression that compares both first-read orders and an initialized clone after direct public reassignment, then confirms explicit retarget exports B. Require byte/typed equality of exported specs and unchanged runtime prompt; existing reference-backed round-trip and construction tests remain applicable. No test was added or run during this immutable review.

### BEH-006-02 — The final three Agent reductions have no performance closure

Classification: **MISSING**. Task: TASK-006.

Obligation: REQ-033, AC-025 and TASK-006 acceptance explicitly require implemented reductions with attributable correctness **and performance** evidence. The authorization boundary remains intact.

Locations: TASK-006 lines 365–408 (reductions/evidence) and 425–427 (explicit final BLOCKED/unmeasured status); source owners `skald-spec/src/authoring.rs` compiled authored snapshot, `skald-spec/src/prompt.rs::render/substitute`, `skald-agent/src/agent.rs` lazy reference.

Evidence: saved unprofiled `20261111T150216Z-0626a7b2d` reports Agent block 403–428 µs at c1 and predates the three reductions. CPU-only localization justifies candidate selection but does not measure their elapsed/throughput effects. Task text explicitly states all three gains remain unmeasured. The reported 319/319 and 128/128 correctness tests do not supply the missing performance observation.

Consequence/materiality: the user cannot determine whether the final implementation reduced the Agent interval, moved CPU/allocation cost, or regressed another required path. A research/profile handoff is explicitly incomplete under this task. This is a missing approved outcome, not a claim the reductions are wrong or a demand for a guaranteed speedup.

Smallest correction: after correctness remediation, retain the final candidate/build identity and request the existing bounded unprofiled direct-Verify experiment on an idle host with standard repetitions and unchanged fixture/recorder settings. Reuse the task's four declared assertion/model control steps, saved reference and canonical renderer; prove comparability before attribution and disclose identity/method gaps. Do not rerun historical baselines or use sampled CPU shares as measured savings. Keep model-c1/c64 Agent phases, Gateway waits, both process CPU, byte/completion/provider populations and accepted-work drain visible. If comparability requires an additional new reference, present that bounded experiment for authorization rather than silently broadening it.

Burden/proof: existing command and report machinery, no new implementation framework or full journey suite. Closure is attributable final-candidate canonical comparison for every standard repetition, explicitly reporting gains/regressions/unresolved observations. A new spec revision is not a prerequisite. This review did not run or authorize measurements.

## Diagnoses and non-findings

- TASK-005's Azurite explanation explicitly says cause unconfirmed and no fix applied. The diff only tolerates prior tracing installation in an existing storage test; it does not change multipart behavior or weaken an assertion. Do not turn a possible VM clock explanation into a proven cause or prescribe a storage fix without the named HEAD error/date evidence. This is not independently proven a TASK-005 implementation regression.
- The c64 provider-header fixture delay is plausible from the shared eight-worker driver runtime, but CPU shares and absence of a Gateway semaphore do not prove scheduling causation. No production Gateway change is justified from that hypothesis. The existing fixture observation is the appropriate distinguishing evidence.
- The compact R6 c64 tail increase is expressly disclosed, remains under its objective, and is not by itself proof of a code defect; no speculative latency fix is prescribed.
- Schema reuse is exact-key immutable preparation, not an authorization/evidence cache. Retained details use typed authored decoding, not a wire trusted flag. No optional redesign proposed.

## Verification limits

No source changes, builds, tests, load, benchmarks or profiles ran in this review. Reported test passes remain implementation evidence claims, supported by inspected tests/source rather than independently rerun results. Local retained TASK-005 milestone artifacts exist; the user and task correctly disclose host confounding, source-label caveats, cumulative heap scope and missing proof. Specialized governance/RLS/drain/security/rule coverage must be reconciled by the orchestrator and fresh validator before final acceptance. Recheck candidate immutability at verdict time.
