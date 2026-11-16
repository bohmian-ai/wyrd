# Independent findings validation

Recommended verdict: FIX_REQUIRED. Seven deduplicated material findings remain.

Subject: repository /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004; branch task/github-verifier-pipelines/TASK-012-integrate-code-mode; base c24e1db8f6641cca9912376a4587e63e2fe74dd1 (ee068f5a2^); candidate 9016393004dc0eb319075245c5f6a1d7a6c2cd46. This is the first review of the complete immutable base-to-candidate range. No prior TASK-012 findings, remediation or acceptance are carried forward. Historical code-mode reviews remain historical evidence, not acceptance of the integration.

The actual diff, original task including its Obligations and Maintainer scope direction, approved pipeline spec revision 15, amended inherited code-mode contract, root rules, applicable Rust/maintainer guides and owning resource/security/deployment contracts govern this validation. All eight discovery reports, scope.md, comparison.md and verification.md were read. CodeGraph preceded code navigation. Proposed paths were traced independently through actual producers, full correction-owner function bodies, callers, sibling writers and assertions; agreement between reports was not used as proof.

## Specialist applicability and coverage

Coverage is complete for this task review. The selected behavior, invariant, repository, maintainer, system, security/tenancy, concurrency and persistent-records reviews all supplied their required reports. The materially changed workflow runs through the existing engine, recorded tools, hosted workspaces/sandbox, shared process resources, background checkpoints/outbox and SDK projections. That justifies all five general roles and the three sensitive-domain roles. The reports cover unchanged dependencies needed to assess those changed contracts, including walk cancellation, Skald media binding, runner terminal handling, SQL fences and Bifrost spill cleanup.

No additional domain role is required. Performance tooling and thin SDK projections have source/assertion coverage in the behavior, standards and maintainer reports; functional concurrency/resource ownership is separately reviewed. No UI surface is changed. Benchmark qualification is explicitly excluded by the current instruction, and missing benchmark runs cannot block this task. The struck readiness/manifests/crashed-worker/extra-control/MCP-inspection/warm-direct-SQL/replica proof cases remain excluded. The accepted gateway server-clock admission is not reopened. No architecture edit or multi-pod qualification is prescribed.

The security report's empty finding set is credible for its assigned permission, accepted-authority, nested-exclusion, publication-identity and fresh-session boundaries. It does not waive the distinct resource/lifecycle findings below. The source-backed report coverage and inspected assertions support the unaffected acceptance rows; there is no missing selected report or unavailable authority. No follow-up discovery is required: the reports have no material authority conflict, and the differing cancellation examples trace the same owner lifetime. Unique claims have been independently inspected rather than voted away. No new blocker originating in this validation is retained.

## Proposal dispositions

| Discovery proposal | Disposition | Final boundary and validation result |
|---|---|---|
| BEH-012-1 | CONFIRMED | FIND-012-1: speculative cancellation drops the only host-settlement owner. |
| INV-03 | REVISED | FIND-012-1: original execution expiry has the same cause; the existing queued runner also races compute against the deadline and must not drop required cleanup. |
| BEH-012-2 | CONFIRMED | FIND-012-2: live queued state has no charge; the new frozen copy is charged only after cloning. |
| INV-01 | CONFIRMED | FIND-012-2: same state/copy custody owner. |
| RECORDS-1 | CONFIRMED | FIND-012-2: checkpoint ACK releases staged custody while live state remains. |
| CONC-01 | CONFIRMED | FIND-012-3: stage_json and fs.extract allocate the encoded Vec before admission. |
| MAINT-02 | CONFIRMED | FIND-012-3: same staging producer; keep the correction focused on encoding admission/custody, without adding a generic allocation framework. |
| CONC-02 | CONFIRMED | FIND-012-4: delta-only scratch admission misses the full temporary replacement. |
| MAINT-01 | CONFIRMED | FIND-012-4: same physical-versus-logical scratch ownership. |
| CONC-03 | CONFIRMED | FIND-012-5: failed cleanup returns surviving scratch charges. |
| INV-04 | CONFIRMED | FIND-012-5: same successful-open and partial-open cleanup owner. |
| SYS-012-1 | CONFIRMED | FIND-012-5: same local failure affects subsequent shared scratch admission. |
| MAINT-03 | REVISED | FIND-012-6: media admission must retain custody across internal binding; do not place runtime ledger state in pure wire MediaRef. |
| INV-02 | CONFIRMED | FIND-012-6: the internal evidence/binding owner is the correct lifetime boundary. |
| STD-012-1 | CONFIRMED | FIND-012-7: independently verified additions/changed tests omit mandatory rustdoc or error/panic sections. |

No proposal was rejected. Overlapping proposals create one correction per owning invariant, not separate remediation for every reported caller.

## Final ledger

### FIND-012-1 — Dropped engine futures abandon host-work settlement

Status: REVISED. Sources: BEH-012-1, INV-03. Classification: INCORRECT. Obligations: TASK-012 OBL-3 and required composition; inherited code-mode REQ-012. These explicitly require already-started host work to settle before checkpoint/result/resource release even when a borrowed block future is dropped.

Locations: crates/vala/vala-verify/src/blocks/engine.rs:840–867 (outer timed drive), :1086 (speculative cancel), :1870–1917 (attempt-local owner/settle), :2021–2040 (check equivalent); crates/vala/vala-verify/src/walk.rs:119–120 (drop-only cancel); crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:214–228 and :431–445 (registered tracker settlement and tracked blocking work). Affected queued consumer: crates/wyrd/wyrd-server/src/verification/runner.rs:555–561 independently times out self.compute, then :620 concludes/selects the terminal lifecycle. Direct service.rs:660 awaits engine.run and maps its DeadlineExceeded result.

Independent evidence and reachability: AgentExecutor::invoke registers ToolSession settlement into the invocation's ToolCalls. HostedTools::open supplies a future that closes and waits for its TaskTracker. Invocation::blocking moves the writer/permit and work into spawn_blocking, which continues after its awaiting future is dropped. The engine's ToolCallsOwner exists only inside attempt/judge. On a false speculative guard, Tasks::cancel removes the running future, dropping that owner and its pending settlement; drive immediately creates the disposition control future. A speculative Agent performing a bounded fs.extract can therefore still be working while disposition/checkpoint/dependents/result advance. Separately, run's timeout drops drive and those owners when the original execution deadline expires. The queued runner's outer timeout can drop the same compute stack and proceed to conclude, so moving a drain solely beneath that timeout would leave the supported expiry path uncorrected. No surrounding execution owner awaits these registered settlements in these paths. Retained workspace/permit Arcs prevent some early frees but do not provide the required sequencing.

Consequence/materiality: supported speculative cancellation and original-deadline expiry can accept checkpoint/terminal progress while started host work remains outstanding. This is the task's explicit dropped-future boundary, not a claim that all Rust future abandonment must synchronously join work. Caller abandonment has no returned result to await, but cleanup/resource custody must still remain owned. The existing authored inner-timeout test waits for settlement because it returns into attempt before the separate sixty-second outer deadline; it does not cover either failing path.

Decision-complete correction: retain the existing ToolCallsOwner/ToolSession settlement responsibility across cancellation of the work it owns. Stop further dispatch at speculative discard or the original deadline, retain required cleanup outside the cancelled computation, and await that cleanup before the corresponding disposition/checkpoint/terminal result is accepted. Preserve borrowed walk scheduling, producer/attempt identity, discarded-output invisibility, original timeout classification and spend, and host-owned byte/writer/permit custody. Apply the same lifetime rule through the queued runner's timeout/terminal consumer so it cannot discard the cleanup that the engine now retains. No changed deadline, new engine, background execution service, per-tool guard, SQL wait or persistence ACK belongs in this correction.

Correction burden: one shared execution/settlement lifetime correction with its existing direct/queued consumers. Removing cancellation or waiting for speculative useful computation would change supported semantics; retaining bytes alone leaves the explicit ordering false. The existing tracker is the completion source and is sufficient; no new scheduler or domain policy is required.

Focused closure proof: use the existing settling-toolset integration owner for (a) speculative discard while already-started host work is held and (b) the original execution deadline expiring during work/settlement. Assert dispatch stops, no disposition/checkpoint/result outruns release, and after release the discarded contribution stays withheld or the result stays DeadlineExceeded. Cover the queued timeout-to-terminal consumer with its existing in-process/runner seam so an outer timeout cannot bypass the barrier. Do not add broad journey or benchmark requirements.

### FIND-012-2 — Live state and its frozen copy lack continuous memory custody

Status: CONFIRMED. Sources: BEH-012-2, INV-01, RECORDS-1. Classification: MISSING. Obligations: TASK-012 OBL-7; pipeline REQ-017 retained memory and memory-first transitions; inherited REQ-027 copy/transfer accounting.

Locations: crates/wyrd/wyrd-server/src/verification/state.rs:272–322 (Held/Writer), :364–369 (clone before charge), :421–434 (stage), :464–477 (frozen-copy weight), :635–659 (live/recovered state), :731–733 (snapshot), :777–829 (terminal retention), :910–935 (publication), :1108–1123 (settlement). Shared charge/release consumer: crates/shared/wyrd-runtime/src/outbox.rs:583–596 and :819–826.

Independent evidence and reachability: Held and Writer own ExecutionState but no ledger or ByteCharge. RunState::at installs recovered state without one; begin/settle/control/publication/decision retain growing records/payloads under the same owner. Runner retains the Arc during compute, persistence and terminal handling, with only execution-count permits around it. Writer::reserve promises an item slot; Outbox::reserve merely checks existing reserved byte use and increments item count. It does not reserve bytes for the next copy. Writer then clones live state before Checkpoint::new counts its weight and Reservation::stage reaches process charging. Checkpoint::COPIES describes frozen state/JSON/Variant/prepared custody, not a separate live-state charge. Outbox release returns all staged bytes after write, while Held still owns its live values and subsequent mutations can coalesce behind an older revision. The existing coalescing assertion explicitly keeps three live invocations while only the first revision is held.

Consequence/materiality: ordinary queued outputs, JSON publications, recovery and selected terminal outcomes can occupy memory invisible to shared hosted/Bifrost/body admission between ACKs, and a full new frozen copy exists before its charge. A concurrency count does not bound those retained payload bytes. Both failures directly contradict the explicit one-ledger/charge-before-copy obligation; no throughput threshold or hostile input is needed.

Decision-complete correction: extend existing RunState/Writer custody with the same process ByteLedger/ByteCharge for live/recovered state throughout its lifetime, and account retained growth at the existing mutation producers. Reuse infallible accounting for mandatory already-admitted growth, suppressing competing fallible admissions without adding a wait or new refusal contract. Do not recount/serialize/copy the whole growing state per mutation. Give a new frozen copy its additional charge before cloning, then transfer that custody to the existing outbox owner without a second promise for the same allocation. Keep live custody independent so ACK retires only frozen/encoding/outbox bytes. Cover recovery, snapshots/publication reads and terminal custody at the same ownership seams; do not retain every historical checkpoint charge forever or add guards in sibling admission consumers.

Correction burden: incremental accounting and ownership handoff in the existing state/outbox workflow. A fixed queued share, lower execution count or permanently retained outbox charge cannot accurately represent simultaneously live and frozen state. No schema, persistence service, synchronous lease/SQL check or durability guarantee changes are required.

Focused closure proof: retain variable live state while an older revision is held, acknowledge all revisions while that live owner remains, and show live growth stays charged, sibling admission cannot promise it again, ACK releases only frozen/encoding custody, and final owner retirement releases once. Exercise recovered state and charge-before-freeze through the existing state/outbox owners. Preserve the fast-transition and coalescing assertions.

### FIND-012-3 — Workspace encodings are allocated before memory admission

Status: CONFIRMED. Sources: CONC-01, MAINT-02. Classification: INCORRECT. Obligations: TASK-012 OBL-6/7 reserve-before-copy; inherited REQ-027 independent encoding charges.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:308–315 (stage_json) and :1416–1423 (extract).

Independent evidence and reachability: Workspaces::open sends ordinary JSON inputs to stage_json; JSON publications and collection manifests reach the same helper through stage_publication/open. stage_json calls serde_json::to_vec_pretty before memory.try_charge, file count and scratch admission. Workspace::extract charges/reads its source, invokes the shared extractor, then materializes the whole pretty JSON Vec before held.try_grow. Both direct and nested fs.extract reach this same owner through Invocation's tracked blocking dispatcher. Therefore under ordinary shared-memory pressure a supported mount/extraction builds its full extra encoding even when admission will refuse it. Per-file/extraction ceilings bound one operation, not aggregate concurrent encodings.

Consequence/materiality: the bytes that the process ledger refuses have already been allocated, potentially beside other consumers' promises. This fails the specific ordering obligation; final Busy and zero-after-return assertions do not prove prevention. No generic exact-RSS instrumentation or extractor redesign is implied.

Decision-complete correction: determine the pretty encoding length without retaining it, using serde_json's existing writer interface and the repository's Checkpoint counting-pass pattern. Enforce limits and take/grow the existing memory charge before constructing each encoded buffer, retaining its charge through the existing write custody. Apply at both staging and extraction producers, preserving exact successful JSON bytes, Busy classification, shared extraction, destination no-overwrite, and unchanged content on refusal. No second serialization framework, allocator, resource authority or per-consumer guard is justified.

Correction burden: two sibling producer orderings using installed serde and the current ledger. Merely checking Vec length afterward cannot satisfy pre-admission; removing JSON mounts/extraction would remove required behavior. Private factoring remains implementation-owned.

Focused closure proof: in existing workspace tests, leave enough memory for source/previous owners but insufficient for the new encoding, and observe refusal before buffer materialization for JSON staging and extraction. Also retain successful content and balanced-lifetime assertions. Use a local owner-level observation, not a new benchmark or allocation-testing framework.

### FIND-012-4 — Atomic replacement undercharges its temporary scratch file

Status: CONFIRMED. Sources: CONC-02, MAINT-01. Classification: INCORRECT. Obligations: TASK-012 OBL-6/7; inherited REQ-027/INV-010 actual owned/writing scratch and no double promises.

Location: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:1540–1567, with existing admission owner :1071–1094.

Independent evidence and reachability: Workspace::write validates the target, then admits only new bytes minus replaced length. It creates a NamedTempFile in the workspace root and writes the entire replacement while the original remains, then persist renames over it. A same-size replacement at a full process scratch ledger asks for zero growth while temporarily holding two full files. The existing writes_charge_process_scratch scenario even replaces 150 bytes with 200 beside 100 input bytes under a 300-byte ledger; it asserts final 300-byte custody, missing the temporary 450 physical bytes. The writer lock prevents concurrent workspace mutation but cannot account that second file. Extraction is new-file/no-overwrite and admits its full output; Python writes have their own mediated path. All model fs.write callers use this owner.

Consequence/materiality: normal atomic replacement can consume disk promised to another workspace or Oracle/Forge spill and bypass aggregate scratch refusal. The logical /work final-size rule is correct, but distinct from temporary physical ownership. This is an explicit required transient-copy boundary.

Decision-complete correction: preserve atomic staging and logical replacement credit, while separately charging the same process scratch ledger for the complete temporary file before creating/writing it while the old file remains charged. Transfer/reconcile custody only after rename/deletion, returning only bytes no longer present. If the temporary copy cannot fit, use the existing busy refusal before mutation and keep the original unchanged. Retain scratch through failed temporary cleanup under the same deletion policy as FIND-012-5. Do not use destructive in-place writes, count the logical /work limit twice or add a second ledger.

Correction burden: one temporary-file lifecycle in the existing Workspace writer. A delta-only promise or weaker failure atomicity is insufficient; no general reservation policy or new file service is needed.

Focused closure proof: same-size replacement refuses at the process-scratch boundary when final size fits but old-plus-temporary cannot, preserving the old file; with temporary room it succeeds and ends charged once for final bytes. Exercise failed write/cleanup without prematurely returning physically retained scratch. Existing workspace tests are sufficient.

### FIND-012-5 — Failed cleanup returns charges for surviving workspace files

Status: CONFIRMED. Sources: CONC-03, INV-04, SYS-012-1. Classification: INCORRECT. Obligations: TASK-012 OBL-6 deletion/transfer release boundary; OBL-7 shared scratch; inherited REQ-027/INV-010; Bifrost Resource and failure invariants.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:465–506, both the nonrecycled removal and failed empty/recycle branches. Existing alternative: crates/vala/vala-bifrost-redux/src/oracle/spill.rs:230–245, ChargedSpillFile::drop.

Independent evidence and reachability: Workspaces::open gives a production Workspace the shared ScratchRoot charge before staging/growth. On the last owner dropping, successful opens empty/recycle; partial/refused opens remove their root. In both cases remove_dir_all failure only logs. Failed emptying falls back to the same remove helper and then drops scratch unconditionally; the partial-open branch drops the local charge on return. A transient read-only filesystem or IO failure can leave the charged files in place. The still-running process then advertises their bytes as free; after writes recover without a process restart, subsequent hosted/spill admissions can consume that false capacity. Next-boot prefix cleanup has not yet happened. The closure-drop shutdown shortcut similarly cannot certify deletion.

Consequence/materiality: an explicitly handled cleanup failure can accumulate uncharged orphan scratch and affect unrelated verification or Bifrost on the shared filesystem. The contract explicitly includes cleanup failure; this is not a new demand that cleanup always succeed or that the shared service crash on local failure.

Decision-complete correction: let the existing Workspace cleanup owner retain its charge until confirmed deletion/emptying or actual ownership transfer. On unrecoverable/unknown cleanup, conservatively retain the whole charge for the process lifetime, using the existing ChargedSpillFile policy with logging and next-boot cleanup. Cover both opened and partially materialized roots, and ensure dropping an unrun cleanup closure cannot advertise surviving bytes as free. Successful cleanup still returns exactly once and permits reuse. No janitor, retry service, durable registry, exact partial remeasurement or process-wide termination is required.

Correction burden: local success/failure ownership at existing cleanup, with one established policy already in the same candidate. Deleting reuse alone leaves the same remove failure; merely logging or waiting for boot does not protect the running process.

Focused closure proof: deterministically leave charged workspace bytes undeleted, finish/drop cleanup, and assert those bytes remain charged and cannot be granted to a sibling. Retain successful cleanup/reuse release assertions, and cover partial-open custody through the existing workspace owner. Use portable failure simulation; no new server/crash-control surface or broad suite.

### FIND-012-6 — Native media loses its charge at the evidence-to-prompt handoff

Status: REVISED. Sources: MAINT-03, INV-02. Classification: INCORRECT. Obligations: TASK-012 OBL-7 and inherited REQ-027 retained-input/new-encoding ownership. No current task exclusion exempts native media.

Locations: crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:762–783; internal consumer crates/vala/vala-verify/src/blocks/agent.rs:315–377 and :430–504. Supporting provider copy owners: crates/skald/skald-spec/src/prompt.rs:936–966 and :1911–2042.

Independent evidence and reachability: TenantEvidence::stored returns Loaded with bytes plus ByteCharge. media grows that local charge before base64 encoding but returns only MediaRef, so the charge drops before AgentExecutor::bind receives it. bind_media_mut then copies the base64 into the provider request: OpenAI builds a data URL, Anthropic and Google clone the encoded string. The bound prompt remains in the Agent run and Invoked during the action/check. Neither MediaRef nor the prompt/invocation carries the reader's charge. The source shortcut explicitly acknowledges early release; model-call semaphores and per-file ceilings are not a memory owner. Ordinary declared image/PDF media placeholders make this path reachable without any nested-tool feature.

Consequence/materiality: supported media calls, including fanout or slow models, retain encoded payloads after shared admission has returned their capacity. Other consumers can receive promises against still-owned bytes. This is explicit ownership through a producer/consumer handoff, not speculative media throughput tuning.

Decision-complete correction: transfer a runtime-only charge alongside media across the existing internal EvidenceReader/AgentExecutor binding seam and retain custody for the bound prompt/provider invocation's actual lifetime, including completion, refusal and cancellation. Charge simultaneous owned encoding copies before they are created, returning source bytes only as they are relinquished. Reuse ByteCharge and the Loaded ownership pattern; keep pure wire MediaRef and provider payload semantics unchanged. Do not put a resource ledger in wyrd-spec/Skald wire contracts or create a media service, permanent reservation or per-provider authorization path. The consumer retains a guard here because it actually owns the bound prompt lifetime; this is an ownership handoff, not a downstream validity check.

Correction burden: bounded internal evidence/binding lifetime plumbing. Removing native media would regress supported Agents, and charging only before encoding leaves the returned ownership gap. No product/public-wire/security policy or persistent format decision is needed.

Focused closure proof: bind native media through the existing Agent/evidence seam, hold the model call pending, and assert encoded payload custody remains charged and blocks an overlapping promise that cannot fit. Completion/cancellation must release exactly once; existing provider binding/format and citation behavior must remain.

### FIND-012-7 — Required rustdoc is missing on changed Rust items

Status: CONFIRMED. Source: STD-012-1. Classification: VIOLATION. Mandatory authority: AGENTS.md Code rustdoc gate and agent_docs/rust.md Rustdoc explicitly include private fields/helpers/test helpers/tests, fallible # Errors and possible # Panics. Missing rustdoc blocks merge regardless of a subjective style/materiality preference.

Independently confirmed changed locations:

- crates/shared/wyrd-runtime/src/resources.rs:39,43–46: ByteLedger.inner, LedgerInner and its fields have no rustdoc; :127 try_add lacks # Errors; :156–157 and :232–234 ByteCharge and ProcessResources fields lack docs.
- crates/wyrd/wyrd-server/src/verification/hosted/admission.rs:67,71,79,91: HostedAdmission.inner and Inner/Queue/Waiter type docs are absent; :121 new calls tokio::spawn but omits # Panics for construction without a runtime; :335–339 Waiting fields lack docs.
- crates/wyrd/wyrd-server/src/config.rs:298: new require is fallible and lacks # Errors; target derivation/refusal tests :4540 and :4580 lack # Panics despite assertions/expect paths.
- crates/wyrd/wyrd-testing/src/bin/capacity/verification/driver.rs:522 and :539: two new tests have no rustdoc; verification/run.rs:3479 new measurement-scope test lacks # Panics.
- crates/wyrd/wyrd-testing/src/server.rs:6584: changed six-control test lacks # Panics.

Evidence/materiality: these concrete types/helpers/tests are additions or material changes in the actual immutable diff, not unchanged surrounding debt. Item/field comments elsewhere do not document these declarations. Constructor-level workflow text on HostedAdmission says to construct in a runtime, but the fallible panic contract remains absent on new. The mandatory repository gate independently makes this blocking; no runtime product defect is inferred from documentation alone.

Decision-complete correction and burden: document these existing items at their owners with actual intent/invariants and the relevant error/panic contracts, batching the corrections with the behavioral remediation. Preserve code/validation/tests; do not add wrappers, suppress warnings, delete tests or rewrite unrelated documentation/architecture. This is a bounded docs correction and not a style-only remediation round.

Focused closure proof: source review of the exact changed declarations and error/panic behavior, plus format/lint checks scoped to the remediation's real write set. No new tests or documentation-check framework are needed merely to prove comments exist.

## Reconciled acceptance matrix

PASS means credible scoped source/assertion evidence, together with the recorded implementation checks; it does not claim this validator reran every integrated suite. FAIL rows identify the precise remaining contract, without discarding accepted independent subproperties.

| Obligation | Reconciled implementation and assertion evidence | Result |
|---|---|---|
| OBL-1 complete staged port and protected files | Pinned port record covers carried/adapted/excluded paths; behavior report's Git comparisons preserve inherited history and protected paths; actual review range is fixed. | PASS |
| OBL-2 contract/target/control reconciliation within current directions | Operations target table, config role owner and six-control projections align with revisions 13–15; current no-architecture-edit decision supersedes old two-pool wording. No architecture correction requested. | PASS |
| OBL-3 identities, borrowed scheduling and settlement | Identities and unchanged walk scheduling preserved; inner authored-timeout assertions establish their path, but speculative/original-deadline drop paths fail FIND-012-1. | FAIL |
| OBL-4 common dispatcher, budgets, permission, recipients/citations, call identity, exclusions | ToolBridge filters code.run/fs.write/publish; Invocation dispatch uses accepted authority and shared audit/capability owners; inspected nested cap-one, provider-call-ID, denied-call and citation/model-byte assertions are concrete. Lifecycle failure remains assigned to OBL-3. | PASS |
| OBL-5 spend/retries/deadlines recovery | Maximum recorded spend restores refusal/interruption allowance; scoped focused unit test passes; persisted assertion distinguishes review=2, judge=0, refusal=3/released. No synchronous spend write introduced. | PASS |
| OBL-6 reusable/streamed/charged workspace and sandbox | Fresh-session, suspended envelope, immutable snapshot and writer-exclusion evidence accepted; FIND-012-3/4/5 invalidate allocation/transient scratch/deletion coverage. | FAIL |
| OBL-7 continuous ledger, state/copies/encoding/custody and memory-first persistence | One root, cache retained-reference and outbox staged charging proven; FIND-012-2/3/4/5/6 leave explicit custody gaps. Fast/coalesced transitions, batched retry and ACK ordering remain accepted. | FAIL |
| OBL-8 direct deadlines, capture/batching/cache, queue-only and authz/inspection | Source fixes arrival deadline before lookup, direct has no RunState, shared services gate queue-only work and read/approve authority; report assertions cover refusal and retained detail separation. FIND-012-1 concerns cleanup ordering, not deadline replenishment. | PASS |
| OBL-9 six targets, refusal before side effects, one combined owner/no reservation | Actual config/bootstrap/service owner mapping and HTTP/MCP refusal assertions support direct/worker/Bifrost capabilities; shared sandbox/model owner has no idle reservation. Struck operational proofs remain excluded. Resource correctness exceptions are OBL-6/7. | PASS |
| OBL-10 dependency/fork/diagnostic constraints | Crates.io Monty, excluded fork split and diagnostic feature/projection containment are source-backed; check:deps passed. No benchmark outcome inferred. | PASS |
| OBL-11 generated/authored parity, history and six controls | Historical recipient/check-call defaults and current stub/native/wire correspondence inspected; candidate records generation/package checks. Extra Rust/native hook rejection remains excluded. | PASS |
| OBL-12 pipeline/negative journeys and provider retry | Shared existing queue/extraction/approval/delivery paths preserved; assertions retain same waiting execution, source/access fences and no recomputation after delivery retry. Repairs use legal modes/targets and database clock. | PASS |
| OBL-13 inherited packet/status/dispositions | Inherited review history retained; R2 superseded/R3 open, amendment and pending qualification not relabeled as passing. | PASS |
| AC-014 migration constraints/RLS/cancellation | No production migration delta in this task range; records report's contextual main comparison has permitted additions/no rewrites and inspected forced RLS/fences. Fresh replay is recorded task evidence, not reproduced under denied Docker. | PASS with stated execution limit |
| Mandatory repository rules | Cross-tier/purity/error/projection/source coverage complete; changed-item rustdoc fails FIND-012-7. | FAIL |
| Explicit non-goals | No benchmark/multi-pod/architecture rewrite/new inspection surface/scheduler/foreign-library source modification or excluded proof is required by this ledger. | PASS |

## Verification limits and handoff

This validation performed static diff/source/caller/assertion checks only. No Cargo command, PostgreSQL command, generator, benchmark or source mutation was executed. The review's executed check evidence is in verification.md and the reports: dependency and tenant-isolation checks, Python/TypeScript lint, and three focused spend/coalescing/outbox unit tests passed. PostgreSQL wrapper provisioning was denied by Docker socket permissions before the database test ran. That does not invalidate the concrete source-proven defects or create a need to rerun broad suites in remediation. Recorded implementation lane results remain recorded evidence, not independently reproduced qualification.

No prior finding closure or convergence assessment applies to this first review. No source/spec decision conflict requires SPEC_REVISION_REQUIRED: the correction boundaries implement already-approved ownership, sequencing and documentation obligations. Package all seven findings into one bounded remediation task for the existing implementation workflow, preserve their IDs, and review closure plus the remediation delta/affected paths next. Do not restart discovery across the entire port absent new material evidence.
