# Independent findings validation — TASK-012 R1

Recommendation: **FIX_REQUIRED**. Five material findings remain: FIND-012-1, FIND-012-2, FIND-012-6, FIND-012-7 and FIND-012-8. FIND-012-3/4/5 are closed. This is the first remediation review; a two-unsuccessful-round convergence assessment is not yet required.

## Immutable subject, authority and method

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004. Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1. Prior candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46. Candidate: f6fac677cd71208871fa5c21e4035085026a99c4. HEAD remains the candidate. The direct Git delta, cumulative acceptance context, scope.md, comparison.md, verification.md and all five selected discovery reports were inspected. Source was immutable; only this report was written.

Authority is the approved pipeline spec revision 15 (especially REQ-017), original tasks/012-integrate-code-mode.md including OBL-3/6/7/8 and exclusions, the first review's verdict/findings-validation and R1 remediation task including its Obligations/Evidence, root AGENTS.md, changes/spec-driven-development.md, agent_docs/index.md, rust.md and maintainer-style.md, and the applicable Bifrost resource/cancellation and Wyrd Agent/workspace/gateway contracts. The original inherited REQ-027 obligation is already established by the first validated review and R1; this is not new generic allocation scope. Relevant TESTING.md ownership and recorded exact-selector evidence remain applicable. No nested AGENTS.md governs the changed source paths.

CodeGraph preceded code navigation. Actual owners, full correction-owner function bodies, affected producers/consumers, sibling readers, and test assertions were checked independently. A discovery FAIL is not itself proof, and fresh green checks do not prove untested lifetimes. No Cargo, benchmark, synthetic load, broad journey, generator or gate was run by this validator. The fresh review executions in verification.md belong to the orchestrator and are accepted as such, not claimed as this validator's executions.

The current maintainer approval of G1 load-through governs. Older no-cold-read architecture/memory wording cannot reopen that decision. No architecture edit or benchmark is prescribed. Accepted server-clock gateway admission, PostgreSQL coordination time, TASK-013/014 qualification ownership and struck proof exclusions remain intact.

## Specialist applicability and carried coverage

Coverage is complete for this remediation scope. No selected report is missing and no omitted required role was found.

| Material boundary | Independent coverage and validation |
|---|---|
| Live/recovered/frozen state, outbox transfer, media and workspace lifetimes | task-review-invariants.md covers the changed owners and affected unchanged consumers, including Skald render/loop copies. Its R1-3/4/5 closure assertions and R1-2/6 exceptions were independently inspected. |
| Cancellation, deadlines, actual host-work completion and shared-server failure propagation | system-review.md traces ToolCallsOwner/HostWork through engine and queued runner. Existing hosted TaskTracker remains the physical-work owner. The targeted residual below is within this specialist's scope. |
| G1 shared load state, cancellation, invalidation, tenancy and recovery | domain-review-concurrency.md covers all foreground snapshot readers and the existing background owner. The new metadata lifetime defect is validated below with a smaller correction. |
| New/extended lifecycle owners and complete existing workflow alternatives | maintainer-review.md supplies a scoped PASS. Its owner/reuse conclusion is supported: existing RunState, outbox, workspace, evidence/binding and cache workflows are extended; numerical and cancellation correctness are distinct and not waived by that PASS. |
| Cross-tier rules, all changed Rust items, docs, portability and test-repair diagnosis | standards-review.md is complete for runtime, Vala, server and testing plus packet records. The explicit documentation gate remains open. G1 is authorized; G2 separates allocator and deadline conditions without production changes. |
| Security/RBAC/tenant/publication authority | Carry forward first-review domain-review-security.md's assertion-level accepted paths: accepted Caller, per-tool/data grants, audit, nested exclusions, publication replay identity and fresh-session isolation. R1 changes internal custody, not those trust decisions. G1 SQL remains tenant_conn → gateway_snapshot → commit before snapshot use, checked by concurrency/standards. No separate new security review is required. |
| Durable acceptance, claim/lease fences, records and migrations | Carry forward first-review domain-review-records.md: unknown-state rejection, fenced accepted prefix, persisted-before-retirement, retry identity, grouped settlement and migration/RLS evidence. R1 adds byte custody and moves the existing timeout without changing those durable semantics; invariant/system review covers the changed seams. No separate records rediscovery is required. |
| Other original behavior and SDK/generated surfaces | First-review behavior/standards/validation/verdict source/assertion rows carry forward for unchanged contracts. No UI, SDK wire, generated contract, migration or dependency delta requires another specialist. Comprehensive behavior discovery is not required again for remediation. |

There is no material report conflict or untraced common cause requiring follow-up. Unique claims were independently source-validated. No validator-origin new blocker is retained, so no self-validation or additional discovery pass is involved.

## Complete proposal disposition

| Discovery source ID | Disposition | Stable finding / result |
|---|---|---|
| SYS-R1-1 | CONFIRMED | FIND-012-1 remains open: both shared and invocation drains remove active settlement before completion. |
| INV-R1-01 | CONFIRMED | FIND-012-2 remains open at terminal reads/selection and their runner handoff; live/frozen/publication custody subparts close. |
| INV-R1-02 | REVISED | FIND-012-6 remains open for actual media copy demand. Guard lifetime closes. Use a bounded media-only peak promise at the existing binding seam; no generic Skald accounting project. |
| STD-R1-1 | CONFIRMED | FIND-012-7 remains open for the exact changed declarations below. This is an explicit merge gate. |
| CONC-012-R1-1 | REVISED | New FIND-012-8: abandoned load cells accumulate after caller cancellation. Reject the bundled recommendation to introduce CACHE_TENANTS capacity/refusal as necessary remediation; cancellation-safe retirement is smaller and preserves approved cold loading. |

Maintainer-review.md proposes no finding; its empty ledger is independently validated for its assigned ownership scope. No rejected or narrowed-away recommendation creates optional remediation work. G1 itself is a human-directed addition, not retroactively a reviewer finding; FIND-012-8 identifies only its independently discovered cancellation regression.

## Validated retained ledger

### FIND-012-1 — active settlement disappears from the completion barrier

Status CONFIRMED; source SYS-R1-1; classification INCORRECT; incomplete prior closure. Violated obligations: original OBL-3 and R1-1 explicitly require started host work to finish before disposition/checkpoint/terminal progress, including original deadline expiry during settlement.

Exact source: crates/vala/vala-verify/src/blocks/tools.rs:142–150 (Drop), :190–205 (HostWork::settle/drain), :274–280 (ToolCallsOwner::settle). Consumers: engine.rs:883–889 (deadline), :1125–1133 (disposition), :1943 and :2066 (attempt/check settlement); server verification/runner.rs:762–772 (timed compute then settle). Physical producer: hosted/tools.rs:214–228 registers Invocation TaskTracker completion; :421–445 keeps actual blocking work tracked after its waiter ends.

Independent proof: drain pops the owning Settlement from its mutex-protected vector, then awaits it locally. Cancellation drops that future. ToolCallsOwner::Drop transfers only the remaining vector; it cannot transfer the already-popped pending settlement. HostWork uses the same drain and loses the same ownership. A second concurrent drain can also observe an empty vector while the first still awaits real work.

A supported Agent can stop its model/tool wait through authored timeout or session halt while extraction still runs, enter per-attempt settlement, and then encounter the original execution deadline or speculative cancellation. Dropping drive destroys the active completion wait. The following shared drain sees nothing and allows timeout/disposition/queued terminal progress before tracked extraction ends. Similarly an already-waiting disposition can cross the execution deadline; concurrent dispositions can mistake another drain's pending work for completion. Physical permits surviving in TaskTracker do not satisfy the explicit result/checkpoint sequencing boundary.

Smallest correction: extend the existing ToolCallsOwner/HostWork settlement owner so queued **and actively awaited** work remains represented until completion. A cancelled waiter must relinquish observation, not completion ownership; concurrent waiters must all observe unfinished active work. Reuse the existing settlement/TaskTracker completion mechanism and one run-level owner. Do not repair separately at each consumer, add a scheduler/service/durable wait, extend deadlines, or crash shared capabilities. Keeping active completion in the owner is necessary; transferring only unpolled entries cannot close the diagnosis.

Burden: bounded lifecycle correction at the existing owner, preserving spend/producer identity, withheld discarded output, original timeout classification and normal successful settlement. Deletion loses required cancellation custody; retaining the current pop-and-await mechanism at another consumer merely moves the defect.

Focused proof: use current held-work/settling-toolset fixtures, establish that per-attempt settlement is already pending, then expire the original deadline or discard. Also cancel a disposition already awaiting shared settlement and exercise concurrent settlement observers. No corresponding result/disposition/checkpoint/queued terminal may advance until the required work ends; release then preserves the original timeout or discarded-output behavior. Existing R1 engine tests cover cancellation while execute remains pending, not cancellation of an active drain. The queued test covers the outer bridge, not this inner ownership state. Reuse those test owners rather than a new harness or broad journey.

### FIND-012-2 — returned terminal decisions are independent uncharged copies

Status CONFIRMED; source INV-R1-01; classification INCORRECT (remaining part of the former MISSING finding). Violated obligations: OBL-7, REQ-017 and R1's explicit terminal/read custody correction.

Exact source: server verification/state.rs:157–163 (Decision::of), :820–821 (decision), :924–946 (decide). Consumers: runner.rs:540–541 (recovery Offer), :1595–1613 (conclude), :1648 onward (deliver), :1704 onward (result). The owning terminal payload is TerminalSelection.outcome, an ExecutedOutcome with owned details/steps/evidence; this is not an Arc clone.

Independent proof: decision() clones the live terminal without a copy charge. decide() either returns that same uncharged read or creates a Decision and clones its selected/failure data into Held, charging only Held's value. The returned Decision remains an independent allocation. conclude holds it over persisted(...).await, then builds result payload and deliver may wait for outbox reservation. Neither Offer nor returned Decision supplies custody. Frozen checkpoint custody is independent and retires on ACK; live custody cannot pay for a second owned outcome. Ordinary sizable queued outcomes under persistence/backpressure, and recovery/redelivery of an accepted selection, reach this path.

Consequence: sibling hosted/Bifrost admission can reuse bytes still owned by terminal handling. This is the original terminal/read obligation, not new persistence semantics or exact-RSS accounting. RunState's new Charged publication/restore reads show the existing local solution.

Smallest correction: extend the existing charged read/handoff mechanism to terminal selections, obtaining the additional promise before cloning and retaining it with the returned decision through its consumers. Cover both selection and existing/recovered decision paths. Reuse ByteCharge/Charged and the existing outbox charged-transfer mechanism; retain independent custody through result materialization and reservation until the actual result owner takes it over, accounting additional independently materialized result representation at that same handoff. ACK must retire only its own bytes. Do not inflate Held forever, retain old checkpoint charges after ACK, add SQL/ACK waits to mutation, or create another resource owner.

Burden: one missing sibling read/terminal ownership integration, plus its actual next handoff. A charge only around Decision::of that drops before conclude's wait is insufficient; a guard around only recovery misses initial selection. Memory-first selection and mandatory-growth semantics remain as accepted.

Focused proof: hold a sizable newly selected decision and a recovered selected decision across state persistence/ACK and delivery backpressure. Their independent charge must survive ACK, prevent a sibling promise and release only its own share when the copy ends or transfers. Preserve first-selection identity, coalescing and accepted-before-retirement assertions. Current live/publication tests cannot prove this read.

### FIND-012-6 — held media guard covers fewer bytes than its retained representations

Status REVISED; source INV-R1-02; classification INCORRECT; incomplete prior closure. Violated obligations: OBL-7/inherited REQ-027 and R1's explicit charge-before-creation requirement for simultaneous media encodings.

Exact source: hosted/evidence.rs:770–788; Vala blocks/agent.rs:352–358 and :442–521. Actual copy owners: skald-spec/src/prompt.rs:936–972 (binding), :975–1007 (render), :1072 onward (text substitution), :1771–1800 and :1911–2042 (provider encoding); skald-agent/src/loop_runtime.rs:220–254, :372–455; request_builder.rs:76–103, :122–135, :215–263.

Independent quantitative proof: let N be raw bytes and E=4*ceil(N/3) their base64 length. TenantEvidence reserves only N+E, returns an E-byte MediaRef and keeps that same promise for the call even though raw bytes end. For an ordinary single PNG placeholder, the OpenAI payload owns P=E+22 bytes in its data URL. During binding the source E and new P coexist; E+P already exceeds N+E for sizable N. During an ordinary no-text/no-tool held call, the Arc-backed bound Prompt owns P (Agent and invoke share that same Prompt, so they are not counted twice), render clones P into the request, and extract_messages clones P into seed messages. run_loop retains both template and seeds: at least 3P stays resident while the provider is pending. request_from_conversation adds a transient seed copy; Chat rebuild clears then refills template, reaching 4P across these owners. Responses rebuild retains the old input until assigning the new vector and can reach 5P across bound prompt/template/seed/temporary messages/new items. A session's before_model hook additionally clones the template while its run owns the prior representations. Anthropic/Gemini clone inline data rather than adding the OpenAI prefix; repeated matched placeholders independently multiply provider payload bytes. No corresponding ledger growth occurs. These concrete copies prove the gap without guessing transport allocator overhead or counting unrelated outputs.

Consequence: a slow ordinary media call can hold materially more input media than promised while shared sibling work is admitted. Guard strong-count lifetime tests would pass with a zero-byte charge; the hosted assertion N+E confirms only the incomplete promise.

Smallest correction: keep the current internal EvidenceReader → AgentExecutor binding guard lifetime, but make its **media-only promise cover the bounded peak** of the supported binding/render/loop representations, based on matched occurrences and provider representation lengths, before the copies are created. A conservative peak charge held by this existing per-call guard is sufficient; precise allocation-by-allocation hooks are not required. Include the existing seed/rebuild/hook phases that retain the same media, preserve independent existing transport/body charges, and release with the bound prompt/call. Reuse ByteCharge and the existing prompt/binding owners' shape information. Keep MediaRef/wire contracts, provider validation, citations and native-media support unchanged. No generic Skald allocator/registry, new ledger in contracts, transport replacement, persistent record or foreign-library project is authorized.

Burden: bounded internal media demand/lifetime plumbing at the seam already changed by R1. Blindly multiplying raw size by one constant while ignoring supported placeholder multiplicity is insufficient; removing native media regresses the task. The chosen conservative peak approach avoids turning every Skald clone into a new resource interface while still satisfying the explicit input-copy obligation.

Focused proof: extend the existing held-provider seam with a ByteLedger-backed reader and sizable media. Exercise one ordinary occurrence and repeated occurrences under capacity above N+E but below the actual media demand: refuse before the extra copies, or show sufficient held charge prevents a competing promise while the call waits. Preserve completion, cancellation and refusal release assertions and formatting/citations. Exercise the existing tool-session/hook branch when qualifying its bound. No RSS benchmark, generic allocation harness or broad journey is needed.

### FIND-012-7 — mandatory changed-item documentation remains incomplete

Status CONFIRMED; source STD-R1-1; classification VIOLATION. Root AGENTS.md and agent_docs/rust.md explicitly require substantive documentation on every changed Rust item, including private fields/helpers/tests, fallible Errors and possible Panics. This is a mandatory gate, not a severity preference.

Independently verified candidate declarations:

| Exact location | Missing contract |
|---|---|
| runtime/outbox.rs:766 stage_charged, :771 stage_held, :602 send_reserved | Panics for handing a different-ledger ByteCharge to a live charging reserved outbox. Their actual call chain reaches Admission::charge:849–857 → resources.rs:221 ByteCharge::merge's explicit same-ledger assertion. Admission itself documents this. |
| Vala blocks/tools.rs:169 HostWork Debug::fmt | Errors for propagated formatter failure from finish(); nearby outbox formatters supply the concise existing pattern. |
| gateway/snapshots.rs:403 changed snapshots_serve_only_while_listening_and_reload_in_background | Panics for its assertions/expects; adjacent changed reload tests have that section. |
| verification/state.rs:363 new Charged<T> Deref::Target | Associated item's rustdoc. |
| verification/mod.rs:619 new Counted tuple field | Private counter-field rustdoc; documenting the struct does not document its field. |

Production state callers use the same outbox ledger; no new runtime panic defect is asserted. The gate itself and hidden public panic precondition are the consequences. The originally enumerated resources/admission/config/testing omissions are corrected, but additions in remediation remain subject to the same rule.

Smallest correction: document these existing declarations and actual error/panic conditions in place, preserving APIs, visibility and behavior. Batch with the behavioral fixes. Source inspection and formatting close this proof; no new helper/test/check framework or separate style-only round is justified.

### FIND-012-8 — cancelled cold-load cells remain after their callers end

Status REVISED; source CONC-012-R1-1; classification REGRESSION/INCORRECT. This new ID follows FIND-012-7 and identifies only a defect in approved G1. Obligations: preserved bounded gateway state and structured cancellation (Wyrd gateway quota owner; Bifrost Resource and failure invariants), original OBL-8/resource constraints.

Exact source: gateway/snapshots.rs:109 (loads), :162–175 (insert/await), :177–183 (normal-only removal). Related cancellation: invocation.rs:640–644; loader service.rs:920–938. reset/invalidate at snapshots.rs:306–339 affect Entries, not loads. Other foreground model/Batches readers use the same load owner.

Independent proof: a tenant's first miss inserts an Arc<OnceCell> in loads before waiting on tenant SQL. Verification preparation's original deadline drops this wait; ordinary caller cancellation can do likewise. Tokio OnceCell releases its initialization permit, but cannot remove its parent map entry. No post-await removal executes when all callers disappear. Background loading can later warm Entries, causing future hits to bypass loads altogether. Reset does not remove it. Repetition across genuine configured tenants after outage/deadline cancellation therefore accumulates process-lifetime cells/keys unrelated to active work or the bounded snapshot cache. This is a finite per-tenant leak with an unbounded lifetime tenant set, not a claim that one cancellation exhausts memory or that arbitrary tenant IDs can be forged.

Smallest correction: give the existing GatewaySnapshots foreground load entry cancellation-safe **caller lifetime retirement**. The last participating caller ending, including cancellation or panic unwinding, must retire the matching entry; cancellation of one caller while peers still participate must leave their one shared load usable. Entry identity checks must preserve a newer replacement and the existing generation fence. Reuse the existing cache mutex/Arc ownership and installed OnceCell cancellation takeover; this is local lifecycle management, not a new cache service. Preserve shared result/error behavior, retries, original deadlines, tenant binding and successful cold loads.

The discovery's additional LRU capacity/all-slots-pinned refusal recommendation is not necessary to fix this proven retained-state regression. A new cold-load refusal policy and capacity decision would add observable behavior beyond this correction; do not prescribe it. Request-scoped coalescing state can retire with its existing active caller owners instead of becoming another retained tenant cache. No janitor, detached SQL worker, config, arbitrary CACHE_TENANTS gate or new refusal is required. Mere normal-return removal is insufficient, and unconditional removal on any one cancellation could split a still-active same-tenant load.

Burden: one cancellation-safe retirement boundary at the producer rather than guards at invocation/models/Batches. Deleting coalescing would discard approved once-per-tenant concurrent loading. Local lifetime retirement removes the cause without creating a new admission policy.

Focused proof: at the existing cache test seam hold a load, join a same-tenant caller and assert one initializer. Cancel one caller and retain sharing/takeover for the survivor; cancel the last and assert no idle cell remains. Repeat distinct cancelled tenants and assert retained metadata does not accumulate; a later retry/error recovery and normal warm hit must retain approved behavior. Preserve invalidation/generation and tenant isolation assertions. No capacity-refusal test or broad SQL/journey suite is required solely for this lifecycle correction.

## Prior closure and reconciled acceptance

| Obligation / prior finding | Current implementation and assertion evidence | Result |
|---|---|---|
| R1-1 / FIND-012-1 / OBL-3 | Adopted HostWork closes cancellation while execute remains pending; new tests assert no early disposition/deadline result and withheld output. Active-drain cancellation/concurrent observers remain unprotected as proven above. Identity/spend/walk acceptance carries. | FAIL |
| R1-2 / FIND-012-2 / OBL-7 | Held.live and incremental retain cover live/recovered ownership; flush obtains copy charge before clone; stage_charged transfers it; tests assert growth beside held revision, post-ACK live charge, sibling refusal and final release. Charged publications/restore close those reads. Terminal copies remain above. | FAIL |
| R1-3 / FIND-012-3 | stage_json counts/adopts memory/count/scratch before to_vec_pretty; extract counts/adopts output before streaming via serde writer. Source proves ordering; workspace Busy assertions cover source-fits/encoding-does-not, absent destination and balanced successful scratch. Existing staging test covers insufficient process memory. | PASS / CLOSED |
| R1-4 / FIND-012-4 | write separately admits min(new,old), then normal growth, covering full temporary file beside charged original. Failed close retains a full surviving-file charge. writes_charge_process_scratch asserts 450-byte transient success → 300 final, equal-size refusal, original contents and unchanged charge. | PASS / CLOSED |
| R1-5 / FIND-012-5 | Custody returns scratch only on confirmed empty/remove; Drop retains unconfirmed/abandoned charge. Partial-open roots take the same path. Surviving-file test asserts retained bytes/sibling refusal; abandoned test distinguishes release from unrun cleanup; pg cancelled-open and normal reuse selectors pass. | PASS / CLOSED |
| R1-6 / FIND-012-6 | Runtime guard survives real binding/provider call and cancellation; strong-count and hosted N+E assertions prove lifetime, not simultaneous representation demand. | FAIL |
| R1-7 / FIND-012-7 | Original enumerated omissions corrected; exact new residuals above violate the same mandatory changed-item gate. | FAIL |
| R1-G1 / OBL-8 | Shared cache owner, tenant loader and miss coalescing implement directly approved loading; CLI restart/init exact selector passed. Tenant authorization/quota refusal/generation behavior remains accepted. Cancellation metadata retirement fails FIND-012-8. | FAIL only for introduced lifetime regression |
| R1-G2 / affected OBL-12 sandbox proof | Production sandbox unchanged; test lowers allocation budget and separates default-time memory cases from one-second infinite loop, retaining MemoryError/time-limit and post-failure service assertions. Source diagnosis explains the old competing ceilings. Focused exact selector passes; arbitrary process suspension is not newly guaranteed. | PASS |
| OBL-6 workspace/sandbox | Prior fresh-session/suspended-envelope/immutable snapshot evidence carries; changed allocation/replacement/cleanup seams now close R1-3/4/5. | PASS |
| OBL-7 other preserved properties | One shared root, memory-first changes, background coalescing, ACK-before-retirement, group retries/fences and claim backpressure remain accepted from prior source/assertion evidence and changed-owner inspection. Exceptions are FIND-012-2/6. | FAIL for those exceptions |
| OBL-1/2/4/5/9/10/11/13, unaffected OBL-8/12, AC-014 | Carry prior verdict and findings-validation assertion-level rows plus selected first-review security/records/behavior evidence. Internal custody delta does not alter public contracts, durable fences, migration/RLS, thin projections, targets, dispatcher authority, spend identity or retained packet history. Their specific changed dependencies are covered above. | PASS, with prior execution limits retained |
| Mandatory repository rules / non-goals | Broad source/language coverage complete; no new tier dependency, portability or generated-contract delta. Documentation exception is FIND-012-7. Benchmarks, full journeys, multi-pod, architecture edits, excluded proofs and extra policies remain outside this remediation. | FAIL only for docs; non-goals PASS |

## Verification limits and handoff

verification.md records 18 freshly passing focused tests, formatting, both prescribed scoped clippy commands and immutable-range whitespace. Its initially empty selectors and disk-full clippy attempt are explicitly separated from corrected successful executions. Those passes corroborate closed subparts; they do not exercise active settlement cancellation, terminal-decision charge, quantitative provider media copies or cancelled-load metadata retirement. Allocation order, cleanup failure fallback and mandatory docs were checked statically.

No source edits, commits, Cargo executions or subagents were used by this validator. No missing authority, changed subject, required missing specialist or unresolved material decision forces BLOCKED or SPEC_REVISION_REQUIRED. Package only these five retained findings into one bounded R2 task, with the documentation corrections alongside behavior. Review the next delta and these closures plus affected seams, carrying closed FIND-012-3/4/5 and unaffected acceptance forward rather than restarting the integration audit.

Skipped: broad rediscovery, benchmarks and full journey reruns, as directed. Remaining risk is the five source-proven lifecycle/accounting/documentation failures; selected green tests do not establish their closure.
