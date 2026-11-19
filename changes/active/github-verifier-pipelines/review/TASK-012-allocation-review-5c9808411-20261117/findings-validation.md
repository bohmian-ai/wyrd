# Independent findings validation

Recommendation: **FIX_REQUIRED**. Seven material findings remain under approved revision 16. This is the first review of the new allocation amendment, not an additional unsuccessful round of the old precharge remedy.

## Immutable subject and procedure

Base `0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd`; candidate `5c980841140721cfaa1945fba1057675805ca77a`. Independently read the actual three-dot diff and immutable source in `/tmp/wyrd-task012-review-5c9808411`; final HEAD matches and its worktree is clean. Original checkout's dirty Oracle fix is excluded. CodeGraph supplied navigation first; immutable source supplied proof. No source/test change, Cargo process, benchmark, commit or push was performed by validation.

Read the invoked current task-review skill, root AGENTS, workflow, testing, Rust/errors/maintainer guidance, architecture authority map and relevant resource paragraphs, approved spec revision 16/amendment/design, original task 012, allocation task and prior review context. Requirements were established independently of scenario labels. The approved amendment explicitly supersedes rejected speculative precharge semantics; historical FIND-012-1 through FIND-012-8 are not recreated under new numbers. Unaffected accepted behavior remains context, not proof of newly changed policy.

All selected discovery reports were read: `task-review-behavior.md`, `task-review-invariants.md`, `system-review.md`, and `domain-review-gateway.md`. Their findings, empty ledgers, assertions and correction boundaries were independently checked. Selected runtime concurrency/lifecycle, Bifrost execution/recovery, gateway custody/identity and primary verification/standards expertise cover the affected boundaries. Primary also inspected the pinned Monty fork dependency, manifests, portability, changed declarations and unchanged durable consumers. No SQL/migration, new wire/SDK projection or UI change requires another specialist. Separate repo/maintainer coverage is adequately assigned to primary and concrete owner specialists; no required report is missing.

Independent validation traced the failing producers through unchanged consumers: Bifrost governor charge to manager usage/targets; fresh Scribe ingress versus accepted persistence; remote peer entry to physical execution/spawn hook; gateway successful capture to object persistence/outbox; buffered HTTP data to consumer-retained Bytes; checkpoint preparation to staged row. Source agreement alone was not used as proof. No unresolved inter-reviewer conflict remains.

## Retained findings

### FIND-012-9 — CONFIRMED / REGRESSION — production Oracle cannot compile

Discovery: SYS-01. Authority: REQ-024, task scenario7 and root AGENTS required build/lints.

Location: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1168–1171,1528–1530,2293`. `Oracle.resources` and its initializer are gated by `test-support`, but production `execute_retained_root` unconditionally reads that field. `wyrd-server/Cargo.toml` enables that feature only through its explicit test-support feature; ordinary server dependency does not. Redux dev/test feature unification explains why tests can compile while the shipped feature set fails.

Trigger: build ordinary shipped wyrd-server. Expected: compile and perform shared query admission. Actual: field does not exist; existing lints log reports E0609 at2293, matching immutable source. Consequence: release server cannot be built. This is source-proven even though no fresh build was run here.

Smallest correction: keep the required production resource capability and initialization unconditional, update its test-only rustdoc, retain actual inspection seams behind their existing feature. Do not enable test-support in the shipped binary. Closure: default-feature server compilation/clippy and applicable lints pass on the successor identity. A new runtime test/harness is unnecessary for this structural defect.

### FIND-012-10 — CONFIRMED / MISSING — Bifrost has no effective bridge to shared pressure ownership/admission

Discovery: SYS-02. Authority: REQ-019/021/023/024, AC-016/018, scenarios3/6.

Locations: redux `resources.rs:909–936,2317–2367,2820–2868,2959–2977`; `scribe/execution_lanes.rs:267,324–355,1725–1736`; runtime `resources/workload.rs:662–720` and `set_local_cap:500`.

Producer/consumer proof: Bifrost's single root `process_charge` starts as an unattributed empty charge and is resized without `attribute`. Its governed bytes contribute to L but never to `workload_bytes(Bifrost)`. Manager admission and demand use attributed operation bytes/live handles. Thus with M=1000, L=950, Bifrost retaining800 and Gateway150, Bifrost is above its equal target but manager reads its usage as0 and admits another query. Retained Bifrost owners also disappear from demand after their short operation handles drop. The already-resolved smaller Bifrost cap is never sent to `set_local_cap`; the only call site is a unit test, so limited shares are not redistributed in production.

Separate path within this same missing integration: fresh Scribe pre-ACK decode/preprocessing calls `on_lane` -> `begin`, bypassing `admit`. With allocator commitment critical but root/local ledger headroom available, new external ingest continues CPU/buffer growth. Governor checks root/local capacity, not shared sampled pressure. Accepted persistence also bypasses fresh admission, which is correct and must be preserved.

Expected: actual Bifrost retained bytes participate once in workload usage/demand, its local cap constrains soft targets, and fresh external work pauses/refuses under applicable pressure while accepted bounded drain proceeds. Actual: runtime arithmetic works only for explicitly attributed owners; these production Bifrost owners are disconnected. Consequence: competitors pause while Bifrost can continue new work, including under critical allocator pressure.

Smallest correction: bridge the existing governor/root custody to the same manager with no second debit, provide its existing cap, and connect fresh pre-ACK Scribe admission at the existing external-work boundary with normal capacity/deadline semantics. Preserve Bifrost inner reservations/pools/class/tenant policy, mandatory persistence/cleanup and infallible growth. Gating all governor growth or every persistence job would introduce a new drain defect and is rejected.

Closure: existing real governor allocations show one Bifrost debit/demand until actual release; contention pauses above-target Bifrost and redistributes a smaller cap. Focused Scribe integration/public append proof uses critical allocator pressure with ledger room, demonstrates new work pause/refusal and accepted persistence drain. Generic manager fixtures alone cannot close this bridge.

### FIND-012-11 — REVISED / MISSING — substantial CPU execution still bypasses shared lanes at three production boundaries

Discovery: GW-01, SYS-03, BEH-CPU-01. Consolidated because all omit required integration of the existing shared CPU dispatcher; retain each concrete owner and closure case. Authority: REQ-019/022/023, AC-017, scenarios4/5; finalized design196–204 expressly includes substantial gateway/verification encoding and actual spawned execution.

1. **Gateway:** server invocation587–591 spawns `CallTask::run`;1409–1427 calls engine.execute directly. Gateway adapter HTTP685–696/763–776 prepares/translates/decodes synchronously; stream838–899/933–982 decodes/translates/frames independently spawned relay work. No lane/context is passed to these phase owners. With all C lanes held, a substantial supported gateway translation/decode still runs on Tokio outside service accounting; nested caller scoping cannot survive the independent task spawn automatically.
2. **Remote Bifrost:** server `oracle/peer_service.rs:346–370,533–590` calls the follower and polls batches directly; redux `oracle/follower.rs:1011–1040` calls execute_stream. The tracer `oracle/telemetry.rs:27–51` wraps only when `Inherited::current` exists; a remote RPC supplies none. A supported distributed query's remote Scribe CPU and its spawned tasks therefore run while that node's C lanes are occupied. The analytical per-graph ingress (`analytical_transport.rs:1375–1433`, `analytical.rs:477–540`) similarly installs identity/runtime without local lane entry and is an affected sibling of the same boundary.
3. **Checkpoint preparation:** server `verification/state.rs:377–380,478–500` uses raw spawn_blocking to serialize the full accepted ExecutionState and encode its Variant row. The separately spawned writer has no inherited lane. Multiple dirty queued executions can start substantial encoding while every shared lane is held. Coalescing/slot bounds do not enforce CPU service admission.

Expected: these real CPU phases acquire the existing process lanes with initiating workload/verified tenant, parallel jobs separately admitted, and waits outside held leases. Actual: major supported work bypasses C and least-service selection. This concerns non-preemptive job/poll boundaries, not exact CPU time or the explicitly approved spill-poll exception.

Smallest correction: connect the named existing phase/remote-entry/writer owners to existing CpuLanes and context/spawn tracer. Keep existing pools, queues, deadlines, authorization/fences, cancellation and output semantics. Mandatory checkpoint preparation uses noncancellable drain identity and remains eligible under memory pressure; CPU admission does not authorize pressure-blocking accepted persistence. Never wrap provider/query lifetimes in one permit or hold a lane over IO/outbox/ACK/SQL waits. No new scheduler or queue is justified.

Closure: focused owner/integration tests hold all lanes, exercise real substantial gateway buffered/translated-stream phases, authenticated remote fragment/stage entry plus spawned work, and actual checkpoint preparation; they must queue before CPU probes/results and complete after lane release with correct identity. Held provider/downstream/persistence waits leave lanes free; accepted checkpoint coalescing/charge/drain semantics remain. Do not add unrelated result/sandbox conversions to remediation solely from unretained observations; comprehensive acceptance still depends on the mapped contract and affected owners.

### FIND-012-12 — CONFIRMED / INCORRECT — successful capture releases custody before its retained content

Discovery: GW-02. Authority: REQ-019/023, AC-015/018 and gateway retained-capture owner.

Locations: gateway `adapter/stream.rs:189–193,252–262`; `engine.rs:280–303,405–417`; server invocation1177–1185/1249–1268/1565–1626 and capture724–726/750–806.

Trigger: successfully finish a substantial captured JSON or generated-speech stream. `StreamCapture::finish` moves events, and `MediaCapture::finish` moves bytes, into ResponseCapture without moving `self.charge`; that remaining field drops on return. StreamEnd/ResponseCapture contain no custody. The server takes content into facts; media bytes become PayloadObjects retained across object get/put/read-back before outbox staging. No intervening charge replaces the released one.

Expected: same retained content keeps the same-ledger custody through transfer until disposal or valid outbox takeover. Actual: successful finish makes live substantial capture appear free; delayed storage extends the gap. Refusal tests only establish discard/release and do not test successful handoff. Aggregate pressure is not a substitute for this already-accounted substantial buffer.

Smallest correction: carry existing charge with successful capture through StreamEnd/facts/object persistence and reconcile/transfer at existing outbox boundary without double debit. Preserve redaction, shedding on refusal, tenant paths, storage failure handling and caller-independent capture. Closure: successful finish retains debit while returned capture lives; held storage retains substantial media custody, then success/failure leads to one takeover or final release. Reuse current capture tests/fixtures.

### FIND-012-13 — CONFIRMED / INCORRECT — buffered HTTP answer custody follows wrapper lifetime rather than retained allocation

Discovery: GW-03. Authority: REQ-019 allocated-capacity/last-reference rule, REQ-023, AC-015/018.

Locations: server routes1494–1516; invocation303–314; gateway engine92–120; server pg_invocation_tests6972–6985.

Trigger: HTTP transport takes Full's data frame then drops its exhausted body while retaining Bytes to send to a slow connection. The map_frame closure holds the sidecar charge but returns the original Bytes unchanged; dropping the body releases debit while that data allocation remains. Existing media test collects into retained `delivered`, verifies bytes, then asserts attribution0, exposing the wrong lifetime expectation. JsonAnswer explicitly preserves reader String spare capacity while custody charges only get().len(), understating available owner capacity.

Expected: one physical buffered allocation counts through the last Bytes reference, using available allocated capacity. Actual: wrapper disposal returns root capacity before consumer disposal, and JSON spare capacity is omitted. Materiality is normal HTTP transport ownership of substantial buffered answers, not a speculative API or per-field heap precision request.

Smallest correction: use allocation-owned custody for buffered payloads at handoff, reusing the existing Frame/Bytes::from_owner approach; preserve zero-copy media where possible and account available owned JSON capacity. Keep clones/slices sharing one charge. Closure: retain actual data after body exhaustion/drop, then clone/slice/drop to prove one debit and last-reference release, for buffered JSON with spare capacity and media. Correct the existing zero-after-collection assertion. Preserve capacity refusal and public Content-Length/wire behavior.

### FIND-012-14 — REVISED / VIOLATION — mandatory owner documentation still contradicts approved behavior

Discovery: BEH-DOC-01; invariant reviewer independently flagged owner conflict. Validator's additional conclude-doc observation was independently confirmed by primary before retention.

Authority: amendment165–167, allocation task99 and root owner-authority/changed-item rustdoc rules. Locations: architecture/wyrd-design2351–2381 (policy-free shared ledger, old session envelope and separate hosted CPU); architecture/bifrost-design1371–1373 (OS-only CPU sharing); changed runner1593–1604 (precharged decision, charge grow/shrink and ledger panic absent from actual1615–1638/1674–1699). Also bounded explicit omissions: new sandbox Waiting::drop739; worker test READS325, DelayedSink fields337–339 and Write::write/flush343/355.

Expected: authoritative resource paragraphs and changed-item docs state the approved consumption/shared-lane policy and real custody. Actual: maintainers are instructed to restore rejected envelope/CPU mechanics, and conclude documents nonexistent charges/panic. This is an explicit correctness/documentation obligation, not subjective style.

The desired behavior is settled by explicit approved revision16 and maintainer decisions; the amendment itself orders authority reconciliation. Updating these bounded owner paragraphs is not inventing new authority or changing the contract to fit unapproved code. Therefore FIX_REQUIRED documentation correction is appropriate, not reinstating old precharges, BLOCKED contradictory mechanics, or SPEC_REVISION_REQUIRED. Do not alter broader architecture to bless any implementation defects.

Closure: content/source review confirms only affected owner paragraphs and changed-item rustdoc match approved behavior and real lifetime/error/panic semantics; no fictional RED or production churn is needed.

### FIND-012-15 — CONFIRMED / EVIDENCE_GAP — final functional and mandatory clean L1 qualification evidence is absent

Discovery: BEH-EVIDENCE-01. Authority: REQ-024/018, AC-018, scenario7 and TASK013 handoff. Location: task885–949 and completion section.

Trigger: approve this candidate for TASK013 now. Expected: affected functional results at the immutable identity and full clean default L1 verification/gateway/all Bifrost experiments compared with compatible history, correctness/validity and no-regression observations. Actual: candidate scenario7 has no results; release lints fail; listed lanes/regressions were unfinished at handoff; required benchmarks were not run. Some intermediate owner results are valid proof of their local assertions but do not constitute final qualification.

Existing external logs now include gateway Rust17/Python8 and server journey62 passing, but use the mutable original worktree with excluded Oracle fix. They cannot certify this committed build. See verification.md for exact provenance and review cutoff. Missing performance proof does not independently prove runtime regression.

Smallest correction: fix implementation defects in a successor, record applicable focused functional results and named selector outcomes bound to it, and supply the prescribed clean L1 records/settings/compatible comparisons when maintainer scheduling authorizes measurement. No production changes are demanded for an evidence gap. Current no-benchmark instruction remains binding: do not run or substitute smoke/dirty/truncated measurements, waive REQ024, or claim qualification. Until required evidence exists scenario7 remains open.

## Reconciled coverage for the verdict

Use these existing references; this table is an acceptance record, not new requirements. PASS here is limited to the identified source/assertion claim, with execution provenance below.

| Existing references | Implementation and actual assertion/execution evidence | Result |
|---|---|---|
| REQ-019/023; AC-015; scenario1 core custody | Runtime existing ByteCharge root plus attribution split/merge/shared-owner/last-drop assertions; existing workspace/spill confirmed-deletion regressions recorded in task | PASS for core mechanism; production handoff exceptions below |
| REQ-019/020; AC-015/016; scenario2 worker/media/checkpoint consumption | Independent real worker telemetry/reuse/reset/stale/cancelled-exit assertions, exact pinned fork reader/reaper source, native Artifact raw+encoded overlap, checkpoint prepared-row owned_bytes and outbox charge assertions; no speculative-envelope requirements reinstated | PASS for selected consumption semantics; checkpoint CPU is FIND-012-11 |
| REQ-020/021; AC-016; scenario3 | Core runtime asserts max observations, .95/1 thresholds, inclusive .90 twice, caps/tenant sharing, reclaim/victim/mandatory exclusion; production Bifrost charge/cap/admission disconnected | FAIL: FIND-012-10 |
| REQ-019/022; AC-017; scenario4 core scheduling | Logical-time busy-lease/tenant-floor/returning-workload assertions; cancellation withdrawal and lane-free admission IO/nesting assertions | PASS for core mechanism |
| REQ-022/024; AC-017; scenario5 actual dispatch | Monty, local Oracle entry/parallel repartition-sort, real spill and Forge/Scribe lane assertions support inspected paths; gateway, remote follower and checkpoint owners bypass lanes; default Oracle fails compile | FAIL: FIND-012-9/11 |
| REQ-019–024; AC-018; scenario6 connected flow | Real combined-server connected attribution, pressure cancellation/Busy deferral and sustained-critical result-drain assertions support primary paths; successful capture/buffer handoffs and Bifrost bridge remain wrong | FAIL: FIND-012-10/11/12/13 |
| REQ-024; AC-018; scenario7 | Partial/intermediate recorded functional proof; missing candidate-final and clean compatible L1 results | FAIL: FIND-012-9/15 |
| INV-001–007; preserved limits/authority/durability/non-goals | No new wire/schema/SQL/UI/SDK behavior; primary traces exact identity/lease/ACK/spend/settlement and fork limits; domain traces quotas/permissions; system preserves pools/spill/WAL/fence and mandatory drain. No new invariant violation proposed beyond retained custody/integration findings | PASS for inspected unchanged/preserved boundaries, without waiving failures above |
| Amendment authority reconciliation / changed-item docs | Stale owner and changed conclude contracts plus explicit new-item omissions | FAIL: FIND-012-14 |

The mandatory wait audit is covered across reports: new waits are process-local pressure/CPU admission, not new SQL/Vault/object-store waits. Scribe staging/fsync and Oracle spill disk IO move off lanes with existing durability order; gateway provider/backpressure/capture-storage and checkpoint outbox/ACK waits stay outside required CPU phases. Corrections must preserve those sanctioned boundaries.

## Rejected/limited proposals and process history

Runtime empty-idle-demand hypothesis was falsified before retention: production min_workers is positive/default1 and retains a real idle worker; zero-worker fixtures/transient lag alone do not establish supported material failure. It creates no remediation. The approved spill-poll exception is not a CPU-cap defect. No new finding is raised for unrelated old result/sandbox conversions, optional refactors, exact RSS accounting or historical TDD repair.

Historical TDD is **VIOLATED**: task records genuine initial RED in some core cycles and production-first/mutation-only additions throughout scenarios2–6. Later tests can establish assertion strength, not historical test-first order. Preserve that record; do not undo correct production behavior or invent RED to repair history.

No fresh executable verification was run because an existing external Cargo runner occupies the shared target and the task forbids overlapping Cargo. Available assertions and candidate-recorded execution were inspected; final build defect and missing qualification remain explicit. No benchmarks were run by instruction. All four required reports exist; no unavailable independent review or unresolved authority prevents the FIX_REQUIRED handoff. The only validator-origin retained addition (false conclude docs) received independent primary confirmation.
