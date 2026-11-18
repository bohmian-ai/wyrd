# TASK-012 R2 verdict

FIX_REQUIRED

The sole retained finding is FIND-012-2, narrowed to result-representation accounting before construction. FIND-012-1/6/7/8 close; FIND-012-3/4/5 remain closed. Terminal decision copies and postconstruction delivery custody are accepted. The result precharge still uses compact decision JSON size, which does not bound the Variant-backed result it builds before adjusting the ledger.

## Immutable subject and scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Branch: task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1.
First candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46.
Prior candidate: f6fac677cd71208871fa5c21e4035085026a99c4.
Candidate: 0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd.
Reviewed delta: git diff f6fac677c 0ae00f429, with continuous ancestry and cumulative original-base context. HEAD remained at the candidate through final verification. The actual range has seven commits; user explicitly confirmed 01c2434fd is a separately approved workflow change alongside TASK-012, audited here without expanding product acceptance.

Approved authority: changes/active/github-verifier-pipelines/spec.md revision 15; original tasks/012-integrate-code-mode.md; first and R1 review verdicts/validation; both remediation contracts, especially review/TASK-012-R1-review-f6fac677c-20261116/TASK-012-R2-complete-settlement-and-copy-custody.md. Dirty packet files were read at the candidate commit. Root/nested applicability, topic/architecture/testing authorities and review methodology are recorded in scope.md and the independent reports.

This second remediation review covers the five prior closures, latest delta and affected producers/consumers/invariants/tests. Source-backed unaffected original acceptance and prior closed findings carry; changed ownership reopens settlement, terminal/media copies, gateway retirement and documentation. No whole-task discovery restart occurred. Current benchmark, host-load, full-journey, gate, multi-pod, architecture-edit and struck-proof exclusions remain. TASK-013/014 own combined-server qualification. G1 cold read-through remains approved; PostgreSQL coordination time and accepted server-clock gateway admission remain unchanged.

## Independent coverage and comparison

scope.md records selection before discovery, the common navigation map, boundaries, owners, consumers and omitted-role rationale. findings-validation.md independently validates that coverage, every selected report, all empty ledgers and carried evidence.

| Report | Assigned changed boundary | Result |
|---|---|---|
| task-review-invariants.md | State/terminal/result/outbox custody and media occurrence/representation peak | FAIL: INV-R2-01 only |
| system-review.md | Active host-work settlement, cancellation/concurrent observers, engine/queued progress and waits | PASS |
| domain-review-concurrency.md | Gateway participant lifetime, peer/last-caller retirement, identity, generation, tenant and error paths | PASS |
| standards-review.md | All changed Rust declarations/tests, tiers, SQL capability, portability and five-file workflow delta | PASS for source; historical TDD unproven |
| findings-validation.md | Fresh independent source/materiality/correction, closure, selection and acceptance validation | FIX_REQUIRED recommendation |

R1 maintainer whole-workflow reuse acceptance carries: R2 locally extends the same ToolCallsOwner/HostWork, RunState/Charged/outbox, evidence/binding and GatewaySnapshots owners rather than installing substantial replacement orchestration. Selected specialists and validator independently verified these local extensions and existing alternatives; another maintainer pass is not required. Security/records-domain source/assertion evidence carries for grants/audit/publication identity, tenant SQL, accepted prefixes/fences/ACK order/group settlement and migrations; no changed policy/schema/durable authority invalidates it. Unchanged public projections, targets and workspace producers need no new comprehensive behavior, UI or SDK review.

comparison.md records the proposal reconciliation. There is no material inter-reviewer conflict, untraced common cause or validator-origin blocker requiring focused follow-up. The sole unique proposal received fresh source validation; agreement or a test name was not substituted for proof. No selected report or required specialist is missing. The separately approved workflow change creates no scope blocker.

## Validated finding

FIND-012-2 / REVISED / INCORRECT; discovery source INV-R2-01. Original OBL-7, REQ-017 and R2-2 expressly require accounting before independent result materialization.

Exact producer: verification/runner.rs:1627–1636, using retained decision size from state.rs:356–359. Actual result/weight owners: verification/results.rs:185–190,317–341,618–623 and scribe_outbox.rs:2075–2082. Both initial and recovered selections use this conclude path. The decision's own charge remains independent, but the second result promise copies its compact JSON size. result()/build_as then allocates the output Variant and both StagedRows; fit accounts their actual weight only afterward.

A supported captured output array of 100,000 zeros is 200,001 compact JSON bytes but needs 500,008 Variant value bytes for that array alone under the actual pinned encoder, before outer metadata/rows. The input is well within the supported Variant limit, and 10,000 zeros also demonstrate expansion. Existing terminal-copy proof puts its large value in evidence, which StoredOutput excludes, so it cannot test this result representation. Sibling admission on another runtime thread can promise the missing bytes while encoding runs; no await is needed for that shared-ledger race. This is format demand at an explicit existing boundary, not speculative allocator overhead or new hardening.

Minimum correction: compute a conservative result-representation bound from borrowed selected/frozen inputs at the existing result builder before build_as. Reuse nonretaining serialization/counting, conservative pinned-format scalar/container/key/header/offset bounds and actual two-row cell/text/correlation/trigger demand. Take the additional same-ledger infallible promise first, then reconcile to exact weight and preserve current reservation/takeover. Do not use arbitrary decision-JSON multiplication, materialize a second result to measure, add a generic allocator/ledger or refusal policy, inflate live custody, or repair unrelated builder callers.

Focused closure is an actual supported numeric-array result whose measured write exceeds the old proxy and fits the new preconstruction bound, with independent decision custody and unchanged transfer/release. Source inspection proves admission precedes construction. Complete source proof, format calculation, materiality, correction burden and acceptance are in findings-validation.md. No new stable ID, rejected proposal or optional recommendation creates further work.

## Reconciled acceptance and closure

| Obligation / finding | Accepted evidence and exception | Result |
|---|---|---|
| R2-1 / FIND-012-1 / OBL-3 | Owed keeps pending futures listed while polled, transfers the same entries and serializes concurrent observers; tools/four engine assertions and queued bridge prove held-work order/classification | PASS / CLOSED |
| R2-2 terminal/read subparts | Held charges initial/recovered copies before clone; Charged survives ACK/persistence; state assertions pin sibling refusal, independence, recovery and identity | PASS |
| R2-2 result before allocation / FIND-012-2 | Compact decision proxy underbounds the supported Variant/row representation; post-build fit is too late | FAIL |
| R2-2 result after construction | Promise remains across reserve_until and transfers once through stage_charged; existing outbox proof pins takeover/release | PASS |
| R2-3 / FIND-012-6 | Occurrence count precedes binding; real reader precharges N+E+six provider-shaped copies; phase trace shows conservative peak, one/two occurrence held/cancelled seam and real-ledger pg assertions | PASS / CLOSED |
| R2-4 / FIND-012-8 | Participant join/drop is serialized, matching identity protects replacement, peers retain takeover/sharing; cancelled-tenant/error/retry/cache assertions and existing generation/listener tests | PASS / CLOSED |
| R2-5 / FIND-012-7 | Original omissions and all new/changed item docs independently inspected; source rules, fmt and scoped lints pass | PASS / CLOSED |
| FIND-012-3/4/5 / OBL-6 | R1 encoding preadmission, transient replacement and failed/abandoned cleanup source/assertion closure carries; no changed producer | PASS / CLOSED |
| OBL-7 remaining invariants | One ledger, memory-first transitions, background coalescing/group retry, ACK-before-retirement and claim fences/backpressure carry and changed handoffs preserve them | PASS except retained result bound |
| OBL-1/2/4/5/9/10/11/13, unaffected OBL-8/12, AC-014 | Prior validated source/assertion rows carry; current specialists check affected consumers/dependencies; no invalidating public, target, schema, grant, projection or workspace delta | PASS with prior execution limits |
| Non-goals / accompanying workflow change | Exclusions honored; user-approved workflow delta audited separately and consistent with current invoked instructions | PASS |

## Behavioral proof and historical TDD

Fresh review verification: 24 distinct focused tests pass, including all eleven prescribed regression commands, eight new tests, four additional gateway cache selectors and the production real-ledger media test. fmt:check, both prescribed scoped clippy commands and immutable-range whitespace pass. verification.md records exact selectors and results, including the incomplete-output Postgres attempt that was repeated successfully and is not counted as passing proof. Candidate-recorded broader suites are separate evidence, not claimed as fresh runs.

Runtime tests establish their assertions. Before-allocation ordering, supported Variant expansion, runner handoff lifetime, and media peak/hook/other-provider coverage use source proof. The new media runtime case is Chat without a tool session; the independent phase trace covers the hook conservatively. No RSS/benchmark measurement is required for this local static bound. No new production test, mutation or source edit was made by the review.

Original TASK-012 explicitly records production-before-test process violations for 5b358f452, 35fd76607, 91c48a972, b2afb89a8 and b78edf183; those remain visible and are not erased by verified behavior. R2 evidence records only fix-reverted mutation RED. It proves assertion sensitivity but neither pre-production test-first order nor a production-first chronology for R2. R2 TDD compliance is UNPROVEN. Do not invent historical RED, undo correct fixes, or prescribe code changes to repair execution history. Remaining implementation must resume genuine test-first discipline. Separate workflow approval does not establish historical execution order.

## Convergence and bounded handoff

Two remediation reviews have not yet passed, but the scope has converged from seven original findings, then five R1 residual/new findings, to one remaining result producer. R2 closes active settlement, media accounting, gateway retirement and docs, and closes terminal-copy/delivery subparts. The remaining issue is an incomplete fix of the expressly prescribed before-materialization result boundary: the implementation used a bound for a different representation. It is not a fix-induced regression, newly discovered unrelated original defect or expanded expectation. The correction needs format-derived sizing rather than more downstream guards.

The next review is limited to FIND-012-2's preconstruction estimate, queued producer/actual result handoff and directly changed tests/docs. Carry every other closed finding and unaffected accepted behavior. Do not rediscover media/settlement/cache/workspace or require broad qualification to close it.

TASK-012-R3-precharge-result-representation.md beside this verdict packages this sole independently validated correction. Route directly to wyrd-implement. No new product, public API, architecture, security, concurrency-policy or persistent-data decision is required. No implementation, merge, commit, push, deployment or Git configuration occurred; only reports and the remediation task were written. Existing local task/design edits remain unchanged.
