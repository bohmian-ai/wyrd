# TASK-003 independent findings validation

Verdict recommendation: **FIX_REQUIRED** for pre-benchmark code acceptance.
REQ-020 remains intentionally pending until the maintainer runs the final
benchmark after code acceptance. Its current absence is not a finding.

Reviewed base `74034459d468fd0adf277e3b51c32b1a676e8e55` through candidate
`c0900bff523e83d3666658440daa81355067cb34`. HEAD and the approved revision-4
spec/task SHA-256 values still match `subject.md`. The intentionally uncommitted
packet supplies authority; unrelated working-tree benchmark/capacity edits are
excluded. No reviewed source or original packet was changed.

This is a fresh validation context, separate from all discovery reviewers. I
read the approved requirements, task evidence, AGENTS, task-review procedure,
spec-driven workflow, maintainer style, relevant testing guidance and Bifrost
ingest/custody authority. I consumed all three selected discovery reports,
used CodeGraph before source tracing, and checked the proposed findings against
production control flow, dependencies and actual assertions. No newly discovered
blocking finding is added by this validator.

## Retained findings

### FIND-TASK-003-1 — ready completion is serviced after unrelated preparation

**CONFIRMED B1; INCORRECT.** REQ-015 and REQ-019; task scenario 2.

Location: `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:1542–1563`,
`:1601–1604`, `:4071–4103`, `:4132–4183`.

With N fencing, a mailbox wake can leave N's completed stage ready while the
scheduler has eligible work for the other slot. The next loop iteration drains
the mailbox and calls `dispatch_next` before it polls N. If dispatch succeeds,
the loop even continues before reaching `select!`. Dispatch calls synchronous
generation sizing and `plan_group_write` before installing the next stage.
The biased older-first select therefore does not establish priority over the
work preceding that select. `drain_mailbox` consumes until observed exhaustion
without a service chunk or intervening completion poll; instantaneous mailbox
capacity does not bound the drain's total work under concurrent refill.

Expected: service a ready head completion before new preparation, with bounded
mailbox/preparation work. Actual: finite admitted unrelated backlog and next
preparation can delay an already-ready visibility/ACK completion. This is a
direct explicit contract violation; it does not depend on infinite ingress or
an elapsed-time performance claim. Existing independent-ACK proof holds younger
IO but does not make ready head completion compete with preparation.

Smallest correction: fix completion priority and bounded mailbox service in the
existing shard loop. Preserve owned futures, control barriers and FIFO. Closure:
a deterministic coordinator test makes the older completion ready with next-group
input queued and observes visibility/ACK before next preparation starts; a finite
backlog exceeding a service chunk must still permit that completion. No generic
scheduler, timing threshold or benchmark is required.

### FIND-TASK-003-2 — blocked identities count toward the eligible collection target

**CONFIRMED B2; INCORRECT.** REQ-015–016; task scenario 2.

Location: `scribe/shards.rs:4151–4158`, `:3868–3876`, `:645–682`.

N owns batch A while fencing. Queue 31 retries of A at its held table and one
eligible B at another table before the 1 ms deadline, with N still incomplete.
`scheduler.len()` counts all 32 queued requests. `GroupCollection::due` therefore
ends the wait and clears the deadline; only afterwards does `pop_group` filter
the held table, producing the one-request B group. The source comment identifies
the shortcut but does not make it consistent with the approved contract.

Expected: collection target and deadline follow eligible collected requests;
held identities cannot satisfy the target. Actual: a supported retry burst
forces small groups before any specified eligible target/deadline/completion
condition. The pure timer test accepts counts directly, and the real retry test
has too few requests to expose this intersection.

Smallest correction: base collection on the eligible group at the existing
scheduler/collection owner, preserving held-table FIFO and progress for other
tables. Closure: controlled input with the held retries and B does not dispatch
solely because total queued count reaches 32; additional eligible arrivals join
without resetting the first eligible deadline, and predecessor completion still
dispatches immediately. Keep already-ready draining above the soft target and
the existing byte/64-request limits. Tests may use controlled time for the pure
collection rule and real owner handshakes for connected behavior.

### FIND-TASK-003-3 — architecture describes a hard 32-request cap

**CONFIRMED B3, classification REVISED from DRIFT to INCORRECT documentation.**
REQ-015 and task's required aligned authority update; AGENTS owner consistency.

Location: `architecture/bifrost-design.md:925–927`.

For more than 32 eligible requests already ready, approved behavior drains to
existing limits, including 64 requests. The newly edited authority instead says
the free slot collects “up to 32 requests” or the deadline, whichever comes first.
That describes a hard cap; current `pop_group` follows the approved 64 cap.
The contradiction matters because future implementations must follow this owner.

Smallest correction: change only the affected paragraph to explain that 32 ends
waiting for new arrivals, while ready eligible requests drain to existing
limits. Content review against REQ-015 is sufficient. No production change or
new test is required solely for this documentation correction.

### FIND-TASK-003-4 — healthy interleaving lacks connected multi-tenant/table proof

**CONFIRMED B4; EVIDENCE_GAP.** REQ-014–015, REQ-018–019; task scenario 2.
The maintainer explicitly confirmed during review that the current write logic
must be tested with multiple tenants and tables. This is recorded as
maintainer-required closure of the existing coverage gap, not independent
discovery of a new product guarantee.

Location: `scribe/shards.rs:12185–12300` (healthy stream), `:12302–12400`
(retry/other table), `:12410–12500` (two-tenant partial failure), and scheduler
unit tests around `:11019–11158`.

The healthy stream builds every append from `fixture.key`, so it exercises one
tenant/table. The retry test adds another table in that tenant. The two-tenant
test proves fail-closed behavior after partial acceptance, not healthy tenant/
table interleaving. Scheduler unit fairness cannot establish how collection,
two-slot retention, real tenant SQL, insertion and ACK combine under that input.
No concrete implementation defect is alleged by this evidence gap.

Smallest correction: extend the existing real-WAL/Postgres pipeline fixture with
one clear healthy stream using at least two tenants and multiple tables through
the same production shard. Use repeated stage reached/release handshakes to prove
later WAL progress during older fencing, repeated slot reuse and independent
ACKs under competing tenant/table input. Assert per-table FIFO and progress,
ordered fencing/visibility, rows under their correct tenant/table, held-identity
exclusion and settled per-request accounting. These observations may share one
cohesive test or a small number of readable cases; do not multiply every fault
across every tenant/table combination. Change production only if this exposes a
defect. No benchmark or timing threshold substitutes for the connected test.

### FIND-TASK-003-5 — failed live COMMIT reconciliation has no bound executable proof

**CONFIRMED INV-1; EVIDENCE_GAP.** REQ-017 and REQ-019; task scenario 3.

Location: `scribe/shards.rs:5454–5471` (fresh reconciliation failure),
`:13015–13112` (available-Absent resolution test), and scenario 3's unbound
claim about existing ambiguity poisoning tests.

An older synced group's COMMIT returns an ambiguous error, then its fresh tenant
connection or exact lookup is unavailable while younger work is submitted.
Expected: poison readiness, retain uncertain ownership and charges without ACK,
drain submitted younger IO and never fence that younger group. Source contains
the intended poison calls, but the deferred-trigger test repeatedly resolves
Absent successfully, then recovers; it never takes the failed-reconciliation
branch. The two-tenant partial failure test exercises the downstream poison
consumer from a different producer. Existing insertion owner assertions and
replay authority tests do not drive this live transition. I independently
traced the live helper and searched its uses/assertions; the claimed existing
one-group live failed-lookup test could not be identified.

Smallest correction: provide the exact existing executable assertion and valid
candidate-bound run if it exists; otherwise add one focused production-shard
test with real WAL/Postgres driving ambiguous COMMIT followed by unavailable
fresh reconciliation. Assert retained/unanswered groups and charges, no younger
fence, and drained submitted IO through the existing fail-closed owner. Reuse
the existing fixture and fault boundary; no outage matrix, retry redesign or
production refactor is justified by this evidence gap. Record the exact focused
selector and result, replacing the unsupported evidence claim.

## Reconciled coverage matrix

| Requirement / scenario | Source, assertions and execution evidence | Result |
|---|---|---|
| REQ-014 / 1–2: repeated bounded overlap | Older/newer slots, `is_writing` guard, oldest-only fence; four-group production-loop WAL/PG handshake assertions. Orchestrator independently ran exact overlap selector: PASS 1/1, nextest run `22116042-a9f1-4d86-8ff0-b5b737c8bdd5`. | PASS for established mechanism; connected tenant/table extension tracked by finding 4. |
| REQ-015 / 1–2: completion, visibility, collection, fairness | Completion inserts, records mirror, publishes snapshot then ACK; independent ACK assertion under younger IO hold. Timer/FIFO unit assertions retained, but run-loop ordering and eligible-count violations remain. Owner text contradicts soft target. | FAIL: findings 1–4. |
| REQ-016 / 2–3: identities and acceptance | Scheduler-held identity filtering, real same-identity retry/other-table WAL counts and fence/visible-row assertions; unchanged batched tenant SQL and retained-version arbitration. | PASS for inspected existing acceptance/exclusion paths; finding 4 extends connected healthy coverage. |
| REQ-017 / 3: failure/recovery | Younger IO refusal does not retract older acceptance; two-tenant partial commit test proves retained charges/no younger fence and canonical replay. Deferred-trigger test proves retryable Absent resolution. Live failed-reconciliation proof is unbound. | FAIL: finding 5 (evidence only). |
| REQ-018 / 3: lifecycle, resources, custody | Retained stage futures, generation/rotation/control/pressure barriers, one pending-byte projection, shutdown/cancellation and request-charge assertions. Specialist traced segment registration/references and persisted contiguous frontier through unchanged cleanup consumer. | PASS for inspected established paths; tenant/table connected settlement included in finding 4. |
| REQ-019 / 1–4: deterministic evidence | Actual coordinator and WAL/SQL reached/release assertions detect serial execution and ACK join barriers; fixed submission counts guard extra work, not measured CPU gains. Missing priority/eligible collection and connected multi-tenant/failed-resolution proof remain. | FAIL: findings 1, 2, 4, 5. |
| REQ-020 / 5: final independent benchmark | Original task leaves maintainer execution pending and names only the approved baseline. Current user instruction requires code acceptance first. | UNRESOLVED by deliberate sequencing; task remains open, no blocker for this sequencing. |
| Repository/collateral checks / 4 | Repo specialist traced Sandbox retained-input charge/admission and public test controls; real queued deferral still asserted at its owning seam. Forge final assertions still require zero Forge bytes/spill and only accounted Scribe bytes. Root independently obtained `ts:typecheck` and `ts:napi:check` PASS. Task reports final lib 1010/1010 three times and prior scoped journey/lint evidence. | PASS within stated evidence limits; no retained collateral finding. |

The selected boundaries cover the affected contracts and producers/consumers:
primary behavior/maintainability, state/lifecycle/durability specialist, and
cross-tier collateral specialist. Separate domain/system/maintainer reviewers
would duplicate these concrete assignments; no materially affected boundary
requires an additional reviewer merely for a vote. No selected report is missing.

Receipt cleanup is not retained as a new evidence gap solely because no cleaner
runs in the two-slot fixture. The specialist traced actual custody references,
segment rollover and contiguous safe-frontier protection. This is structural
proof backed by unchanged cleanup journey evidence; the abbreviated task claim
about generation closure must not be mistaken for the entire safety argument.

## Evidence and process limits

This validator ran no Cargo or benchmark and changed no production code. Actual
source/assertion checks are independent; full-suite/journey run counts remain
implementation-record evidence, with root's focused overlap and TS checks
independent execution during review. Neither the passing focused overlap nor
the reported full suite satisfies the specific missing assertions above.

TDD compliance remains **VIOLATED** for the disclosed code-before-test scenarios.
Keep that history in the verdict. No finding demands impossible historical RED
recreation or production churn to repair it. The remediation uses ordinary
test-first increments for new corrections and evidence-only tests where code
already behaves correctly. Measured performance and final completion remain
pending the maintainer's later benchmark.
