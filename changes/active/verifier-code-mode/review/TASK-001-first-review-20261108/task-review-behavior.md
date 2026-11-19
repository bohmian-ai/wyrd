# TASK-001 behavior acceptance review

Verdict: **FAIL**.

Subject: base `5234961d51798581a026834cfffd5efaadc226b4` to candidate
`0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`, first cumulative review.
Governing inputs are approved specification revision 2 and the original
`tasks/TASK-001-bounded-code-orchestration.md`. Appended implementation evidence
was treated as reported proof, not authority. HEAD remained the candidate.
No source, task, or specification changes and no Cargo commands were made by
this reviewer.

## Scope and authority

Reviewed the actual cumulative diff, the session/dispatch/Agent/engine path,
hosted tool and Monty drivers, workspace mutation/extraction, contracts and
custom-data decoding, result storage, configuration, and the changed hosted,
Vala, and public Rust proof. Mechanical constructor/generated consumers and
the authority-document changes were included. TASK-002 language/MCP journey
delivery is excluded from this task's acceptance; mechanical projection
correctness remains included.

Read the task-review skill, root rules supplied for this repository,
`changes/spec-driven-development.md`, maintainer/Rust/error guidance, nested
TypeScript/docs rules, and the applicable Verifier and accepted-authority
sections. `.codegraph/` is absent. The approved task does not authorize a
second permission owner, runtime, registry, or worker transport. The candidate
extends the existing ToolSession, registry, Invocation, Sandbox, Workspace,
BoundedQuery and engine settlement owners. New Dispatch/ToolBridge/ToolCalls
state serves shared origins and retry retention; no concrete duplicate
capability workflow was established. The task's required failure and extraction
cleanup behavior agrees with stronger approved authority; the contrary
implementation notes cannot relax it.

## Acceptance matrix

Locations below are repository-relative. `Vala` means
`crates/vala/vala-verify`; `hosted` means
`crates/wyrd/wyrd-server/src/verification/hosted`; `hosted tests` means
`wyrd-server/tests/integration/pg_hosted_artifacts.rs`; `Rust journey` means
`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs`.
Verification marked reported was inspected in source and the task evidence;
independent runs are those recorded in sibling `verification.md`.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001–004; AC-001–003: bridge in Agent step/check, dotted declared names only, existing direct contracts and computation | `AgentExecutor::invoke`, `ToolSession::resolve`, `ToolBridge::resolve`, `Sandbox::exec`, `Invocation::call`; only installed callable handled | Actual-worker boundary test; independent Rust reduction journey proves steps and checks; Vala shared-path test independently passes | PASS |
| REQ-005; AC-003/011: bounded supported value conversion and no partial oversized result | `Estimate::json`, `python_from_json`, Capped result serialization; conversion occurs after record success | Independent pure conversion test passes its signed-i64/finite/Unicode/collection and rejection assertions; host-result conversion refusal leaves incorrect evidence, BEH-003 | FAIL |
| REQ-006–007; INV-001/002/004; AC-004/017: same permission/capability owners and accepted identity | Both origins enter `Dispatch::call` then `Invocation::call`; exact tool audit precedes capability calls; Bifrost uses statement caller and BoundedQuery; no worker identity selector | Independent public permission journey; inspected recovered/delegated hosted tests; existing queue authority/audit paths reused | PASS |
| REQ-008; AC-005 caught branch: safe catchable catalog problems and repair | `answer` preserves structured owner's code/status/safe details; ordinary denied child has no success value | Actual-worker boundary/recovered-authority assertions and public denial/recovery; inspected source | PASS |
| REQ-009; AC-005 uncaught branch, AC-009/018: execution failure remains failed step/inconclusive check and host timeout cannot yield a manufactured final success | `Invocation::code` erases every sandbox failure cause except Busy; `Halt::observe` observes only Busy; Skald returns remaining failures to model | Public failure journey independently passes an assertion that the timed-out step **succeeds**; hosted wait test likewise expects more model work | FAIL, BEH-001 |
| REQ-009–010; AC-006/010/014/018: shared atomic call allowance, final allowance allowed, no refill/refund, uncatchable exhaustion and Busy | `ToolCalls::spend`, `Dispatch::spend`, first-wins Halt, Agent select/before-model, engine retry-spanning ToolCalls | Independent Vala retry-budget and bridge tests, public budget flow; inspected hosted mixed-origin/final-allowance and Busy tests; unchanged queue Busy deferral | PASS |
| REQ-011; AC-007: effective shared non-code cap, cap 1, future association, distinct queries | Dispatch semaphore excludes code.run; one dispatcher for both origins; Monty futures retain call IDs; statement-specific caller | Independent Vala cap 1/2 shared-path proof; actual-worker gather/read/query assertions reported passing | PASS |
| REQ-012; AC-009/010: host waits charge wall time across retries, worker/query cancellation and reuse | Reservation starts before mounts/worker and refunds only unused time; sandbox timeout/drop checkout; query cancellation drop guard | Inspected real-worker cancellation/wait/shutdown tests; independent public failure/reuse journey; timeout status defect is BEH-001 | FAIL, BEH-001 |
| REQ-012–013; AC-008/009: blocking extraction settles as part of cleanup and mutation serialization survives interruption | Extract's blocking closure retains Workspace but its writer and JoinHandle are outside; dropping await detaches blocking work | Required interrupted-extraction proof is absent, and claimed Arc structural equivalence does not establish settlement or writer ownership | FAIL, BEH-002 |
| REQ-013; AC-008/011 normal completion: shared scratch limits, no-overwrite and awaited visibility without whole-program lock | Per-write writer in Sandbox; extraction uses existing Workspace::extract and disk-based work_room; persist_noclobber | Actual-worker workspace mutation assertions reported passing; cancellation exception separately fails above | PASS |
| REQ-014; AC-001/012/016: child output stays out of model messages | ToolBridge calls dispatcher directly, not Skald conversation; only outer code tool result returned | Independent Rust reduction journey inspects upstream messages, single outer exchange, byte reduction and equal leaf evidence | PASS |
| REQ-015–016; AC-009/012: accurate once-only start-ordered records across retries, failed steps and cancelled admission | ToolCalls engine-owned; records start before executing admitted tool; settled unfinished calls become no-value cancellations; record success precedes conversion | Independent Vala failed-step/inconclusive and retry tests; public pending query assertions; conversion failure counterexample BEH-003 | FAIL, BEH-003 |
| REQ-016–017; AC-012/013: additive closed recipients/defaults, check calls, typed/custom-data and retained readback | `wyrd-spec` execution structs/defaults; decode_data transfers tool_calls; engine Judged/Attempted and StoredOutput preserve lists | Round-trip/default/custom-data assertions inspected; independent reduction journey verifies retained requirements/steps equality | PASS |
| REQ-017: capture/privacy and non-Agent checks | Records are part of ordinary outcome output; summary has no new argument field; non-Agent checks leave empty calls; existing storage/read permissions own capture | Static result-builder/source review; dedicated capture-disabled test was not supplied | PASS (static boundary evidence) |
| REQ-018; AC-001/011/016: original evidence citations and exact Verdict/data | Existing Agent verdict_from, reachable source sets and evidence.cited retained; scratch/live reads not added to pinned source declarations | Independent reduction journey checks exact citation/data and retained equality; existing citation/custom-data tests inspected | PASS |
| Startup settings and AC-014: checked positive values, defaults, no Card override | HostedLimits defaults 128/64 KiB and VerificationConfig's existing checked scaled setting path | Independent configuration test passes valid/invalid/default assertions | PASS |
| INV-003/005; AC-015 and non-goals: no recursion, new Card/flag/tool service/store/network/package/OS-specific isolation; authored code unchanged | Declared-only bridge excludes CodeRun; authored CodeExecutor passes no bridge; existing process pool/mounts/allocator reused; no added dependency or migration | Actual-worker only-code and authored-code boundary assertions; inspected diff/worker owner and unchanged isolation regression evidence | PASS |
| AC-016 required concrete efficiency comparison | Fixed 20 notes, dependent list/read/search in REDUCE; same fs.read arguments/digests/bytes, one code.run exchange | Independent real-client/Postgres/worker journey passes equal judgment/citations/data/leaf and <=25% byte assertions; task reports 96,824 vs 595 bytes | PASS |
| Governing architecture reconciliation / compatibility / no unrelated behavior | Verifier record/tool/isolation paragraphs and accepted-authority paragraph updated; mechanical Rust and generated TS changes reflect additive contracts | Static diff review; reported codegen/build/lint gates, independent server build | PASS |

The absence of dedicated extraction and capture tests was not treated as
automatic evidence that production is broken. Extraction fails because its
actual lifecycle contradicts the required guarantee, as traced below. Capture
has static ownership evidence and no source-proven new disclosure here.

## Material findings

### BEH-001 — INCORRECT: uncaught program errors and Python timeouts can become successful steps

**Obligation:** REQ-009, AC-005, AC-009 and AC-018; the task explicitly requires
uncaught failures to fail a step or make a check inconclusive, and host stops
must not manufacture a judgment.

**Location:** `hosted/tools.rs:328–334`, `Vala/src/blocks/tools.rs` Halt::observe
and Dispatch::call; downstream `crates/skald/skald-agent/src/loop_runtime.rs:632–690`.

**Producer-to-consumer trace:** a generated program awaits a denied child
without catching RuntimeError, or a held child exceeds Python's operation
deadline. Sandbox returns `BlockError::Terminal`. Invocation preserves a typed
cause only for Busy; every other failure becomes a plain ToolError. The
common dispatcher records it but its halt observer only recognizes Busy.
Skald therefore appends an ordinary failed tool-result message and performs
the next model turn. A valid final answer then causes AgentExecutor and engine
settlement to produce a successful step, or potentially a normally judged
Agent check, despite the uncaught execution failure.

**Observable proof already in the candidate:** the STALL phase of
`Rust journey::code_mode_failure_releases_session` asserts
`StepStatus::Succeeded` and simultaneously asserts that the outer call's error
contains `time limit`. The independent public journey run passes this exact
assertion. `hosted tests::code_host_waits_obey_deadlines_and_cancellation`
also explicitly expects the model to answer after the timeout and to request
another code.run. These are counterexamples, not proof of the approved failure
contract. The task's evidence note acknowledges the uncaught-error behavior;
it cannot change AC-005.

**Sibling paths:** the same mapping handles generated computation failures,
uncaught nested permission/domain errors and operation timeout. Ordinary
*caught* child errors must remain repairable inside Python; immediate/queued
Busy must retain its separate capacity semantics. Authored action/check code
already propagates BlockError directly rather than returning a model tool
message.

**Smallest testable correction:** preserve the failed generated execution as
an invocation failure through the existing session/Agent/engine error path.
Do not change Skald's general repairable tool behavior or turn caught child
errors into host stops. Keep Busy/exhaustion distinct and retain all admitted
records. Extend the existing worker/public failure tests with an uncaught
denial after a completed child and a held-query timeout for both step/check
outcomes; assert failure/inconclusive, no post-stop model/tool work, no
failed-subject verdict, records retained, and subsequent fresh success.
Correct the existing success-after-timeout assertions. This is a bounded
classification/propagation correction within existing owners, not a new API.

### BEH-002 — INCORRECT: cancelled blocking extraction escapes invocation cleanup and releases its writer early

**Obligation:** REQ-012–013, AC-008–009 and the task's explicit requirement
that cleanup await started blocking extraction, preserve mutation serialization,
and release resources only after it settles.

**Location:** `hosted/tools.rs:263–277`; `hosted/workspace.rs:675–758`;
Sandbox::exec's timeout/Complete paths drop outstanding Calls.

**Trace:** direct or nested fs.extract takes `held.writer().await` into the
async caller, then starts `spawn_blocking(move || workspace.extract(...))`.
The closure owns only Workspace/arguments, not the writer guard. When the
program/attempt times out, is cancelled, or ends with unresolved futures, the
JoinHandle await and `_writer` drop. Dropping a started spawn_blocking handle
does not stop its closure. Invocation cleanup therefore returns while the
closure can still extract and persist a scratch file. The Arc retains the
workspace and its admission permit until later, but it neither awaits the
operation nor retains the mutation lock.

**Consequence/materiality:** the explicitly required cancellation/recovery
path can finish while resource admission remains occupied. Where another
code call or sibling survives within the same invocation, it can acquire the
released writer while extraction's shared work_room/persist sequence is still
running; shared scratch accounting is no longer serialized. This is a
required lifecycle/resource guarantee, not a request for generic hardening.
No new public journal or durable cancellation result is needed.

**Existing-owner comparison:** Workspace already owns scratch admission and
the writer; Invocation already owns the spawn/await. Keeping an Arc alive is
necessary for filesystem lifetime but is not an alternative to operation
settlement or writer ownership. The evidence packet's proposed structural
equivalence is false on those two points. The other blocking fs operations
also retain Workspace beyond a dropped await, so closure/cleanup handling
should be checked at that shared owner rather than adding caller-only guards.

**Smallest testable correction:** retain serialization for the entire bounded
extraction and settle started extraction through existing invocation cleanup
before cleanup is reported complete and workspace admission is reused. Keep
current bounded extraction/no-overwrite/error semantics. Add the task's
focused deterministic interruption proof at the extraction seam: stop after
blocking work starts, request cleanup, show it does not finish/release its
resources before extraction settles, and show no concurrent scratch mutation
crosses the writer boundary or late result is delivered. Then verify next
invocation admission. No new executor, storage owner, or timing-based sleep
proof is warranted.

### BEH-003 — INCORRECT: a bridge conversion failure retains a successful delivered-value record

**Obligation:** REQ-015–016 and AC-011–012: no delivered value means no digest,
zero returned bytes and a safe failure; digest/size describe what actually
reached Python.

**Location:** `Vala/src/blocks/tools.rs:554–564` versus
`hosted/sandbox.rs:489–510` and `python_from_json`/`to_python`.

**Trace and supported producer:** cards.get returns a full typed Card through
GetCardArgs::read and Invocation::to_value. A Prompt's response schema is an
ordinary `serde_json::Value` (`skald-spec/src/prompt.rs:153–157`), checked as
JSON Schema by check_output_schema; e.g. a numeric property's valid maximum
can be `18446744073709551615`. The Card read is bounded and succeeds as JSON.
Dispatch immediately finalizes its nonempty digest/byte count with no error.
Only afterwards Sandbox::answer calls python_from_json, whose integer branch
refuses u64 values above i64::MAX. Python receives RuntimeError instead of that
Card. The leaf record remains successful because answer has no record
completion connection. The existing conversion unit test explicitly asserts
that `u64::MAX` is rejected, independently confirming this conversion boundary.
The same record mismatch applies to any bounded result rejected for excessive
conversion depth.

**Consequence/materiality:** a supported Card read can produce retained
evidence asserting successful delivery of data that Python never received.
This violates Wyrd's central explicit evidence contract independently of
whether the unsigned conversion restriction itself is intended. Generic
serde_json success is not proof that a Python value was delivered.

**Smallest testable correction:** align nested success/failure finalization
with the existing bridge conversion boundary, so an undeliverable result
records safe failure/no digest/zero bytes. Reuse the existing bounded
conversion and flat record owner; do not retain raw output or add a journal.
Add a focused real bridge test reading a Card with the schema value above,
catching the resulting conversion failure (or proving lossless delivery if
the required wire-range conversion is implemented), and checking the exact
recipient/digest/bytes/error contract. Direct successful reads and successful
nested reads must preserve their existing digest equality. This needs only
the existing session/bridge record seam, not per-capability guards.

## Proof integrity and limits

The independently executed tests in `verification.md` pass, including the
three real public journeys. Their actual assertions were inspected rather
than treating a green suite as acceptance. Most new tests protect meaningful
boundaries, and the reduction comparison uses real owners/worker/client.
The timeout journey presently codifies BEH-001. The replacement failed-record
Vala test credibly proves engine settlement but does not prove the omitted
interrupted-extraction lifecycle. The packet's pin-shadowing diagnosis agrees
with the scoped-future fix; yielding in the Busy fixture gives the sibling a
chance to become pending and does not alter production halt behavior. No
separate diagnostician artifact was present in this packet; that evidentiary
workflow question is left to the orchestrator/repository review and is not
counted as another behavioral finding.

No exhaustive fault, OS, language-binding or broad gate reruns were performed
by this reviewer. Configuration/conversion/Vala/public runs were performed by
the orchestrator; remaining reported verification was assessed statically.
No optional design or refactoring requests are proposed. The three findings
above are source-proven contract failures; their focused regression tests
have not been added during this immutable review.
