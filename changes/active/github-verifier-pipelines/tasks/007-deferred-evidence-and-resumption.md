---
id: TASK-007
status: proposed
approved_spec_revision: 4
requirements: [REQ-008, REQ-009, REQ-011, REQ-016]
acceptance: [AC-005, AC-006, AC-007, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-001, TASK-003, TASK-006]
---

# Accept deferred evidence and resume the same execution

Implement **approved-deferred-inputs.md → Approved publication matching and
subscription lifecycle / No new PostgreSQL tables** and
**approved-runtime-and-github.md → Durable waits and resumption**. Use
**contracts.md → Deferred input publication / Errors, inspection and delivery**.
This is ordinary typed input dependency, not a Trigger, wait action or broker.

## Input and subscription source

Add `deferred: true` only to typed JSON/media slots; omitted/false and samples/
grouped inputs retain existing paths. Existing action/reference validation finds
consumers from ordinary input refs; no extra subscription authoring/depends_on.
Task 003's active selected binding establishes subscription to each named
input even without a running execution. Task 006 supplies accepted PR repository/
head/policy/authority for consumers. Publication alone never starts a run.
New slot forms join task 001's queued-only validation before any direct step.

Publishers use `wyrd evidence publish ci_results --file results.json` or
`client.evidence.publish(name, value, repository=None, commit=None)`. CLI/SDK infer
remote and commit from Git; explicit source overrides support absent checkout or
CI merge revision. Resolve remote through server verification of stable connected
host/repository_id. Raw HTTP is the approved
`POST /v1/verification/evidence/publish` request, matching tenant + repository +
commit + input. The publisher does not supply a Repository policy version,
Verifier ID, execution ID, group_id or retry key; tags are limited to commit.
Internal consumer matching still uses accepted execution context. Git metadata
grants no permission.

## Selected custody and dependency flow

Reuse `EvidencePlanRequest`, client `verify::File` and storage's plan/upload/
completion first. Only completed verified File/Artifact handling or validated
canonical JSON reaches publication. Extend existing evidence/subject permission
checks; do not grant unrelated execution/Card mutation rights or invent a new
permission category. Validate authorized subscribers' declared type/format/schema
before accepting. Identical matching identity/content replays its receipt;
conflicting replacement refuses.

Existing Bifrost `verification/execution_inputs` requires execution and Verifier
IDs, so it cannot hold pre-execution evidence by faking those IDs. Extend the
existing Bifrost verification/Scribe custody owner for standalone accepted
publication and subscriber acceptance, with existing storage File custody. Keep
SQL for binding/queue conditional coordination only; no payload mirror/new table.
Persist which existing subscribers accepted the publication internally so later
subscribers do not receive historical evidence. Do not expose subscription
generations, reactivation or rollback APIs. No eligible subscription returns
explicit unmatched without retained publication; temporary uploads keep normal
cleanup. Existing subscribers without execution retain accepted evidence under
normal retention; future matching executions may consume it.

For queued/running consumers, accepted evidence releases only actual dependent
steps; independent work continues. For waiting consumers, stage continuation of
the same ExecutionId through existing SubmissionSink/queue. Completed/cancelled/
superseded execution never reopens. Execution initialization and recovery read
accepted matching evidence as well as notifications; a lost notification cannot
lose an ACKed input. Existing executions keep their exact policy pins even when
source evidence matches another eligible policy subscriber.

Extend `vala-verify`'s existing logical execution state and `server/verification/
state.rs::RunState` with nonterminal blocked dependencies and accepted facts.
The sole execution state owner consumes them; do not introduce a second writer
racing running branches. When no branch is active/runnable, ACK the canonical
pending checkpoint **before** retiring current queue delivery and releasing
runner capacity, renewable lease, workspace/Python/model invocation resources.
Use existing runner/SQL waiting/readiness coordination, not a sleeping block or
continuous polling execution. Every true continuation has stable internal delivery
identity so retry reacquires queued capacity for the same execution and does not
replenish attempts, spend, accepted work or original deadline. A pending queue
row's bounded canonical-state check repairs ACK→readiness gaps after restart.
Authority loss, deadline and task 006 supersession fence continuation while parked.

Publication positive receipt follows durable canonical evidence acceptance, not
upload/staging/notification or completed review. Persist enough coordination for
every accepted obligation to be rediscovered. Recovery returns exact accepted
state, authoritative absence or explicit unavailability; do not start fresh on
an unavailable read. Missing deferred evidence remains bounded by the original
execution deadline and cannot pass.

## Outputs, consumers and surfaces

Return approved evidence receipt/status/count/timestamp and waiting inspection
with deferred names. Task 008 reuses this checkpoint/park/continuation lifecycle
for decisions; task 009 projects waiting without recomputation. Ship HTTP/shared
client/all three SDKs/CLI/MCP publish and read projections, scoped writes,
schemas/errors, upload-based examples and diagnosis here. Amend
**wyrd-design.md → Verifier** and **bifrost-design.md → Table and row identity /
Durability and visibility / Ingest / Public surface** for standalone custody,
accepted facts, ACK and parked state; update relevant authorization guidance.

## Connected proof

**AC-005:** Through task 002 connection and task 003 declaration, publish real
JSON/completed media before consumer, waiting, running and with no execution.
Observe subscription-backed retained evidence then task 006 admission consuming
it without republish. Exercise no subscription, new subscriber after unmatched/
accepted publication, identical/conflicting retry, wrong tenant/repository/commit/
type and different exact policy pins for the same source. Assert typed receipts,
explicit unmatched/no retained replay, exact content and no terminal reopening.

**AC-006 (deferred portion):** With an independent branch, park only consumers;
when all block observe released task 001 resources/lease, publish and resume the
same ExecutionId with settled work/spend/retries/deadline unchanged. Supersede
through task 006 or revoke while parked; old acceptance cannot release work.

**AC-007 (evidence/park):** Controlled crash before/after upload completion,
publication ACK, pending checkpoint ACK, retirement, continuation commit and
recovery read. After positive receipt observe exact accepted evidence and owed
continuation without producer replay; lost notifications are repaired. Unavailable
canonical state never becomes empty/fresh state. Assert through public journey
where observable, and real-dependency integration for the exact crash fences.

**AC-011 (evidence/wait):** Follow source-inference/override and publication docs,
inspect waiting state and durable receipt using shipped surfaces.

Tier 1: matching Rust/Python/TypeScript SDK stories and MCP journeys using README
runners, WyrdTestServer/Postgres and actual 003→006 outputs. Tier 2: Bifrost/Scribe
acceptance and queue recovery at existing verification/server and SQL integration
owners via `mise run test:bifrost:journey:server`, `mise run test:sql` and the
affected `test:vala` lane; tier 3: reference/slot/schema/matching branches. Add
new focused commands only after tests exist. Run README contract/type/boundary
checks. Task 008 owns the decision-specific half of AC-006/007.
