---
id: TASK-012-R1
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 15
parent_task: TASK-012
remediates: [FIND-012-1, FIND-012-2, FIND-012-3, FIND-012-4, FIND-012-5, FIND-012-6, FIND-012-7]
requirements: [REQ-015, REQ-017, REQ-018, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: []
---

# Preserve settlement and continuous resource custody

A developer's verification must stop dispatch at cancellation or its original deadline, settle already-started host work before selecting checkpoint/terminal progress, and keep every retained state, encoding and physical scratch copy on the one process ledger until its owner relinquishes it. This remediation closes the seven independently validated gaps without replacing the existing engine, tools, workspace, provider or persistence workflows.

Approved spec: changes/active/github-verifier-pipelines/spec.md, revision 15, with the inherited code-mode resource/settlement obligations as amended by pipeline revisions 12–15.
Original task: changes/active/github-verifier-pipelines/tasks/012-integrate-code-mode.md, including Maintainer scope direction.
Review: changes/active/github-verifier-pipelines/review/TASK-012-first-review-20261116/verdict.md and findings-validation.md.
Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1 (ee068f5a2^).
Reviewed candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46 on task/github-verifier-pipelines/TASK-012-integrate-code-mode.
The implementation input is this candidate. A later review compares its immutable successor to this candidate, retaining the original cumulative task context.

## Diagnosis and selected corrections

### FIND-012-1: keep settlement ownership across cancellation

The changed engine holds ToolCallsOwner only inside its attempt/check future (crates/vala/vala-verify/src/blocks/engine.rs:1870–1917,2021–2040). Speculative discard calls drop-only Tasks::cancel at engine.rs:1086 and walk.rs:119–120. Original execution expiry drops the timed drive at engine.rs:840–867. HostedTools registered a TaskTracker close/wait settlement at hosted/tools.rs:214–228, but Invocation::blocking at :431–445 has already moved real work into spawn_blocking, which continues after its waiter is dropped. The queued runner separately times out compute at verification/runner.rs:555–561 and then concludes at :620. Thus a supported speculative extraction or deadline expiry can accept disposition/checkpoint/terminal progress before started host work settles. Existing authored inner-timeout tests return into the settling attempt before the outer deadline and do not exercise these drops. OBL-3 explicitly requires settlement even for dropped borrowed futures.

Retain the existing ToolCallsOwner/ToolSession settlement responsibility outside the computation that cancellation discards, and carry that ownership through the queued runner's timeout-to-terminal consumer. Stop further dispatch at discard/deadline, but settle started work before the corresponding disposition/checkpoint/result is accepted. Preserve deadline classification, spend, producer identity, discarded-output invisibility and held writer/permit/byte custody. The existing trackers supply completion; do not replace walk scheduling or add a service, persistence wait or guard in every tool. An engine-only drain that the runner drops again is insufficient. This is one shared lifetime correction, not separate fixes for each consumer.

### FIND-012-2: account live state and charge frozen copies before allocation

server verification/state.rs:272–322 retains Held.state without any ledger charge. RunState::at at :635–659 installs recovered values the same way; settlement/publication/decision/snapshot producers retain or clone payloads at :731–733,777–829,910–935,1108–1123. Writer::flush clones at :364–369 after reserving an item slot, then counts/stages the checkpoint at :421–477. An item reservation is not a byte reservation. Runtime outbox.rs:583–596,819–826 charges only staged custody and returns it at write ACK while live state still exists. A slow dependent, recovered run or held terminal can retain bytes invisible to sibling admission. Coalescing tests prove live changes can also grow behind an older revision. This violates OBL-7 and REQ-017; the current outbox test proves staged charging only.

Extend existing RunState/Writer ownership with the same ByteLedger/ByteCharge, initialized for recovered state and maintained incrementally at retained-value producers. Reuse existing infallible accounting for mandatory growth after admission; do not add engine waits, a new refusal policy, per-mutation whole-state serialization/counting or a queued reservation. Charge a frozen copy before cloning, transfer its custody into the existing outbox without double promising it, and retain live charges independently through checkpoint ACK and terminal/park ownership. Cover snapshots and publication reads at their actual ownership seams. A count limit or retaining every old checkpoint charge forever cannot close this gap. No schema or durability change is needed.

### FIND-012-3: admit JSON encodings before building them

hosted/workspace.rs:308–315 calls to_vec_pretty for ordinary JSON inputs, publication mounts and collection manifests before memory admission. Workspace::extract at :1416–1423 similarly materializes its output Vec before growing its charge. Both direct and nested extraction share that producer. Under memory pressure these supported paths allocate the full copy that the ledger subsequently refuses. Existing eventual-Busy/final-usage assertions miss the ordering; OBL-6/7 requires reserve-before-copy.

At these two existing producers, determine exact pretty-encoding length without retaining bytes, using installed serde_json's writer interface and the existing Checkpoint counting-pass pattern. Apply bounds and acquire/grow the same memory charge before materialization, retaining it through existing write custody. Preserve successful bytes, Busy/limit classifications, extraction no-overwrite and unchanged destination on refusal. A local ordering/custody change is sufficient; no generic allocator or serialization framework is justified.

### FIND-012-4: charge the physical replacement copy

Workspace::write at hosted/workspace.rs:1540–1567 admits only new_size minus replaced_size, then writes a full NamedTempFile beside the unchanged original before rename. Equal-size replacement at full shared scratch admits zero extra bytes while holding two files. The existing test's 100-byte input, 150-byte original and 200-byte replacement fits its final 300-byte ledger but temporarily occupies 450 bytes. This bypasses OBL-6/7's aggregate physical scratch promise and can consume capacity held by another tenant or spill owner.

Preserve logical /work replacement credit, writer exclusion and atomic rename. Separately charge the full temporary file against the existing process scratch ledger before creation/write while the original remains charged, then reconcile/transfer after successful rename/deletion. Refuse Busy before mutation if the temporary copy cannot fit; preserve the original. Retain charges for failed temporary cleanup under FIND-012-5's deletion rule. In-place destructive writes, doubling the logical limit or adding another ledger are not acceptable alternatives. Only this temporary-file lifecycle needs correction.

### FIND-012-5: retain scratch when cleanup cannot confirm deletion

Workspace::drop and remove at hosted/workspace.rs:465–506 log deletion failure but drop scratch regardless. Both recycled successful opens and partially materialized/refused opens can leave files while advertising their bytes as free. A transient filesystem IO/read-only failure followed by recovery without process restart lets later hosted/Bifrost work consume false free capacity. Successful cleanup tests do not exercise this explicit OBL-6 failure boundary. Dropping an unrun cleanup closure also does not prove deletion.

Keep success/failure ownership at existing Workspace cleanup. Return charges only after confirmed deletion/emptying or actual transfer; conservatively retain the whole charge for process lifetime on unsuccessful/unknown cleanup. Reuse the policy already implemented by ChargedSpillFile::drop in vala-bifrost-redux/src/oracle/spill.rs:230–245, together with existing logging and next-boot prefix cleanup. Cover partial opens and abandoned cleanup ownership. Successful emptying still releases once and allows reuse. Do not add a janitor, retry service, persistent registry, partial remeasurement or shared-server termination policy; neither logging alone nor waiting for the next boot protects the current process.

### FIND-012-6: transfer native-media custody to its actual consumer

TenantEvidence::media at hosted/evidence.rs:762–783 grows the Loaded charge before encoding, then returns only MediaRef, dropping that charge before AgentExecutor::bind at vala-verify blocks/agent.rs:315–377 receives it. Provider binding in skald-spec/src/prompt.rs:936–966,1911–2042 copies the encoding into provider payloads retained during Agent invocation at agent.rs:430–504. Ordinary declared image/PDF Agents therefore retain uncharged input while models wait/run. The source shortcut and read-only Loaded test do not satisfy OBL-7/inherited REQ-027's lifetime requirement.

Transfer runtime-only charge ownership through the existing internal EvidenceReader/AgentExecutor binding seam and retain it through the bound prompt/provider call's actual lifetime, including refusal, completion and cancellation. Charge simultaneously retained encoding copies before creation, releasing source bytes as ownership ends. Reuse ByteCharge and the Loaded pattern. Keep pure MediaRef/wire contracts, provider validation and citations unchanged; do not move runtime ledgers into wyrd-spec/Skald wire types or introduce a media workflow. A consumer guard belongs here because the consumer owns the prompt lifetime. Charging only the initial read/encoding is insufficient; bounded internal custody plumbing is required.

### FIND-012-7: close the mandatory changed-item rustdoc gate

The actual diff adds or materially changes these Rust items without required docs/error/panic contracts:

- runtime/resources.rs:39,43–46: ByteLedger.inner, LedgerInner and its fields; :127 try_add error contract; :156–157 ByteCharge fields; :232–234 ProcessResources fields.
- hosted/admission.rs:67,71,79,91: HostedAdmission.inner and Inner/Queue/Waiter type docs; :121 constructor panic contract for tokio::spawn outside a runtime; :335–339 Waiting fields.
- server/config.rs:298: fallible require error contract; target tests :4540,4580 panic contracts.
- wyrd-testing/src/bin/capacity/verification/driver.rs:522,539: new tests lack rustdoc; verification/run.rs:3479 measurement-scope test lacks panic contract.
- wyrd-testing/src/server.rs:6584: changed six-control test lacks panic contract.

These violate the explicit AGENTS.md and agent_docs/rust.md merge gate, not a preference. Document the existing declarations with their real intent/invariants/error/panic behavior in this same remediation. Do not change behavior or visibility, delete tests, suppress gates or write unrelated docs/architecture. Source review closes the documentation proof; no new test/check framework is warranted.

## Write set, preserved behavior and exclusions

Implement at the existing vala-verify engine/tool/evidence-binding owners and affected server runner, state, hosted evidence/workspace owners; extend their existing tests. The additional documentation write set is the exact runtime/admission/config/testing declarations above. Adapt only necessary internal callers/implementations of an altered binding seam, retaining all existing alternate/test readers. No SDK wire projection, migration or public feature change is required by these findings.

Preserve dynamic steps/fanout/repeats/speculation, retry/spend identity, queued recovery and original deadlines, RBAC/audits, direct versus queue-only behavior, fresh reusable sessions, cancellation custody, memory-first progress, background group commits, ACK-before-retirement and claim backpressure. Keep one process ledger, one combined hosted owner and the six target set, with no idle reservation. Keep portable behavior on every supported platform; no OS isolation/failure-test assumptions.

No benchmarks, synthetic load, multi-pod proof, scheduler redesign, persistence service, foreign-library source edit, nested publication extension or new SDK/CLI inspection. The maintainer-struck proof cases remain excluded. PostgreSQL remains the coordination clock and accepted gateway server-clock admission stays accepted. Do not edit architecture/wyrd-design.md, tasks/README.md or tasks/011-code-review-pipeline-tutorial.md. Do not commit, push, configure Git or stash. Do not put links or attribution in written artifacts.

## Acceptance and focused proof

| Finding | Required observable closure at existing test owner |
|---|---|
| FIND-012-1 | Hold already-started host work during speculative discard and original execution expiry. Dispatch stops, but disposition/checkpoint/result does not outrun settlement. After release, discarded output remains withheld or the original timeout result is returned. Cover the queued runner's outer timeout-to-terminal seam as well as the engine. |
| FIND-012-2 | Live/recovered payload bytes stay charged while old revisions are held and after staged ACKs; later growth is included, frozen-copy custody starts before allocation, sibling admission cannot reuse live bytes, and final ownership release returns once. Existing transition/coalescing/spend/fencing behavior is preserved. |
| FIND-012-3 | Staging and extraction refuse insufficient encoding memory before materializing their encoded buffers; successful pretty bytes and destination/no-overwrite semantics remain, with balanced custody. Use an existing-owner observation, not a new allocation framework. |
| FIND-012-4 | Same-size replacement refuses when final logical size fits but temporary physical scratch does not, preserving old content. With room, replacement succeeds and final bytes are charged once; failed temporary cleanup retains custody. |
| FIND-012-5 | Deterministic portable cleanup failure leaves surviving/unknown bytes charged and unavailable to a sibling. Cover partial-open and abandoned-cleanup custody; successful deletion/reuse releases exactly once. |
| FIND-012-6 | Hold a real binding seam's model call pending: encoded/provider payload remains charged and prevents an overlapping promise; completion, refusal and cancellation release exactly once while provider formatting/citations remain correct. |
| FIND-012-7 | Every listed changed declaration has substantive required rustdoc and actual error/panic contracts; no behavior/gate weakening. |

Use existing vala-verify integration, server state/workspace/runner/evidence and runtime outbox test owners. Record exact selectors for new tests after assigning names and before RED. Use local held work/mock provider seams; PostgreSQL is not required merely to prove in-process ownership. Any proof requiring PostgreSQL must run through scripts/postgres/with-test-postgres.sh, and an environment refusal is reported honestly rather than bypassed.

Run each new/changed test through its focused exact command and preserve these existing regression selectors:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::refused_attempts_keep_their_tool_spend)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::admitted_items_charge_process_memory_until_written)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::held_changes_coalesce_into_the_next_revision)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::writes_charge_process_scratch)'
```

The final command's existing final-size-only expectation must be corrected according to FIND-012-4, retaining its final-size and refusal assertions while adding the transient physical-copy case. This review supplies the source-backed diagnosis; do not simply weaken its assertions to clear a failure.

Run Cargo checks sequentially. Run mise run fmt:check, then the owning lints task's scoped equivalent for the actual Rust write set:

```bash
mise exec -- cargo clippy --locked -p wyrd-runtime -p vala-verify -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

No full user-journey suite, gate, benchmark or unrelated package lane is required for this remediation. Those integrated suites belong to change review. Record focused commands/results and remaining environment limits with the finding closures. Route this task directly to wyrd-implement; the next task review retains accepted unchanged coverage and reviews the remediation diff, all seven closures and their affected paths.
