---
id: SPEC-long-horizon-verification
revision: 17
status: approved
---

# Long-horizon and heavy verification

## Objective and authority

Verify accumulated behavior by grouping ordinary Verifier executions under a
generic caller-chosen `group_id`. A turn Verifier judges one turn through today's
`run.observe.verify(...)` or `run.observe.submit(...)`. A group Verifier
declares precisely which recorded inputs or requirement verdicts of other
bound Verifiers it receives, and runs when a Trigger ends the group.
The application chooses what to send and when: fan-out is ordinary verify/submit
calls sharing group_id, with per-item execution/retry on the existing queue and
aggregation at group_end. Every Verifier remains a pure versioned function over
closed inputs.

Revision 16 recorded the owner's explicit approval of the simple grouping
proposal and cut list, followed by the instruction to implement it as one
TASK-003a on the merged TASK-001–003 worktree. It supersedes older grouping
lifecycle requirements and the packets under `review/`; those packets are not
implementation or review authority. TASK-002's scheduled samples and TASK-003's
bounded staging outcomes remain required. TASK-003a delivers functional and
documentation closure; REQ-015/AC-009 benchmarking follows it and is not claimed
complete by this task.

Revision 17 records the owner's subsequent explicit approval of this flow and
its ACK/visibility, post-ACK crash-loss and durable-backlog tradeoffs:
record input/result evidence in Bifrost, update one PostgreSQL scheduling row per
group binding, move its due time on new evidence, claim expired rows in bounded
batches, pin available evidence, enqueue execution and consume the scheduling
row after successful handoff. With no new activity there is no scheduling row
and no further Bifrost read of that group. This replaces revision 16's repeated
Bifrost discovery and its prohibition on a group scheduling table.

Current implementation base: `ba92ac9b9` on
`change/long-horizon-verification-task-001`. Extend the existing
`VerificationScheduler`, `GroupPoller`, `VerifierExecutionQueue`, source binding
resolution, ScribeOutbox and SDK journeys. Remove group-specific completion
reconciliation rather than replacing these owners. Ordinary `.verify` executes
inline and stages inputs/results; `.submit` stages inputs and the acknowledged
batch later enters the PostgreSQL queue. Neither request path uses PostgreSQL.

## Scope, invariants and non-goals

Terminology: a group runs an ordinary Verifier execution. There is no separate
review concept, entity, queue, state or operation in the verification grammar.
Use “group Verifier”, “group Verifier execution” and “verification” throughout
production code, contracts, fixtures, tests and product documentation.

- **INV-001 — Wyrd-owned machinery.** Group identity, membership, ordering,
  end/revisions are independent of tracing, a provider, external
  queue or producer runtime. Telemetry is optional evidence. Callers provide
  the evidence the invoked Verifier declares; grouping never requires OTel.
- **INV-002 — Designed for load.** Hundreds of requests/s per deployment and
  bursty multi-tenant load are invariants. Every staging buffer, runnable
  queue, task set and source-reference read uses existing finite resource bounds.
  Refuse excess work
  before staging excess work; outbox crash loss remains explicitly accepted
  under REQ-005, rather than being disguised as durable acceptance.
- **INV-003 — Server ownership and tenancy.** Rust server owners retain durable
  behavior; `wyrd-spec` stays IO/async/PyO3-free. SDKs project the shared
  `wyrd-client`. TenantConn/RLS, storage authorization, accepted authority
  and exact owner/source identities apply to grouping and every injected read.
- **INV-004 — Immutable judgments.** A group revision is one fixed visible-evidence
  snapshot; each group execution pins the selected values
  it actually reads. Results and historical revisions remain immutable.
  Explicit agent tool queries read live data, outside the pinned snapshot.
  Media uses existing uploaded-file digests/locators.
  Recording read provenance does not inject that provenance into user slots.
- **INV-005 — Existing platform rules.** macOS/Linux on x86_64/arm64 use portable
  facilities. All internal Scribe writes use ScribeOutbox. Permission checks
  block; audit writes never block or fail the operation. Lifecycle mechanics
  are lineage, not fabricated authorization audits.

In scope: grouping ordinary verify/submit executions, explicit source references,
background pending-group scheduling/execution, owner isolation, existing outbox durability,
process bounds/tenant progress, non-vacuous assert each and REQ-012's
scheduled binding simplification. There is no grouping-specific source-count ceiling.

Unchanged: ordinary inline slot declarations and invocation flow, the 14 Card
kinds, Source read-only behavior, Window/Drift scoring and schedules apart
from REQ-012, and checks/timeouts/retries except the explicit extensions below.
Traces continue to be exported using today's telemetry path.

**Pre-release schema constraint.** Wyrd has never shipped, so no deployment
holds retained state that must survive this change. Built-in Bifrost tables
(`execution_inputs`, `results`) change their definitions in place; an existing
development deployment is reset (fresh Postgres catalog and object storage), not
upgraded. The fingerprint and physical-schema checks stay unchanged, and a
server started against old-schema tables keeps refusing at startup with the
existing `FingerprintMismatch`. No retained-table upgrade, schema-evolution path,
expand-and-contract overlap or upgrade journey is in scope. The first release
that preserves retained state owns built-in table evolution.

Non-goals: evidence-only append/upload producer verbs; a contribution ontology;
deployment-wide quota coordination or strict allocation fairness;
group execution serialization or result-adoption protocols; universal source-binding attribution;
Verifier-internal action fan-out or step checkpoints; server-owned video/MP4
extraction, segmentation or new media formats; fan-out resource/coverage budgets;
list-of-media slots for a single synchronous verify over many files;
automatic trace expansion/span snapshotting; source child selectors or reason inputs;
authored concurrency tuning or precise token/dollar/elapsed budgets;
a new aggregation-policy language; a new grouping Card kind; a new collection input kind; stateful/streaming
Verifiers; DAG loops or arbitrary workflow engines; tracing-based group
identity; running the application's agent; exactly-once provider calls;
provider-specific media pipelines; privacy erasure/delete-by-key/redaction/
legal hold; extra durability machinery. Human checks are out of scope;
future human checks are attributable review.

## Required behavior and public contracts

### REQ-001 — Grouped verify/submit executions

**OD-004 — owner resolved:** a caller chooses an optional opaque `group_id`,
such as a conversation, order or issue ID. It is scoped to the tenant and
owner Service, not an application Run ID, trace ID or Wyrd-minted token.
Every SDK's verify and submit accept an optional argument named `group_id`:
Python `observe.verify(..., group_id="order-123")` and
`observe.submit(..., group_id="order-123")`, with equivalent optional
`group_id` arguments in Rust and TypeScript. It is not an argument on
`state.run`.

Reserve `group_id` as a Verifier input slot name across all surfaces and
input declaration forms, including whole-slot source-reference declarations.
Registration refuses `spec.inputs.group_id` using the existing contract
`WyrdError::Validation` family and slot-validation error shape: rule `slot`,
path `inputs.group_id` within the Verifier spec. The refusal identifies the
reserved field and its collision with the SDK argument. Reuse the existing
`validate_slot`/`invalid` validation pattern; this is not a global identifier
restriction or a Python-only client check. No alias, automatic rename or
fallback treats that keyword as evidence. The author must rename the slot.
Other current argument names are not newly reserved: Python's verifier is
positional-only and its remaining collectors permit slot keywords; Rust and
TypeScript keep slot names inside their input maps. REQ-011's optional argument
still applies to every SDK and the wire independently of slot names.

`VerificationRequest` carries the optional wire field `group_id` for both
execute and enqueue; HTTP/MCP project that same spelling. The stored column
is also `group_id` on execution-input/result records and the submitted execution
queue as applicable. The only new grouping control state is the pending scheduling row in REQ-004. It is never renamed to
`group`, `key`, `grouping_key`, `session` or a language-specific alias.
Omitting `group_id` preserves ordinary ungrouped invocation; no value is
inferred from Run or trace identity.

`run.observe.verify(name, inputs)` executes the turn Verifier now;
`run.observe.submit(name, inputs)` queues an asynchronous Run verification.
Both transport the evidence declared by that Verifier's slots using the
existing upload machinery. Neither is an evidence-only write. A tool/human/
payment producer records a fact by verifying/submitting it to a bound Verifier
whose slots declare that fact, not by appending arbitrary group data.

Attribution is derived under REQ-014 from existing request/authentication
context. Source input/result rows carry nullable `group_id` and `owner_card_uid`, plus
existing subject/Verifier/run/principal identities. No source binding is
resolved on the hot path. Background group lookup excludes rows without a
matching owner/subject/Verifier binding. Services cannot mix through shared
`group_id`; component ambiguity is refused only where used in group lookup.
Producers coordinate only on that caller value, never a server resource ID.

Grouping adds no synchronous Postgres write, group row, counter, acceptance
sequence, revision row or readiness state to verify/submit. Pending scheduling
rows are updated only in the background after Scribe acknowledgement. Both use today's
ScribeOutbox input/result path. Source values are selected later from
visible attributed rows; pending scheduling rows select which groups need work. Persist original input declarations and existing
execution/subject/Verifier/principal identities, with `group_id` and
authenticated owner metadata. Existing binding columns
remain available for existing activated executions; this change does not require
populating them for every direct or submitted turn.
No revision is allocated by a producer call.

Existing enqueue idempotency remains: replay returns the same execution,
conflicting content settles with the existing queue error, and duplicate physical
rows do not become additional source executions. A conflicting submission can
already have a visible input row before the queue refuses it; grouping does not
consult acceptance state to hide that row. A deliberate second verify is another execution.
Authorization precedes disclosure and staging.

### REQ-002 — Cross-Verifier input grammar and ordered values

Extend the existing reference grammar with these roots:

```text
group_input_reference :=
    verifiers.<name>.inputs.<slot>
  | verifiers.<name>.requirements.<requirement>.outcome
```

`<name>` is the Verifier Card's `metadata.name`, not a file stem, alias,
registry search, UID or latest-version selector. Names, slots and requirement
IDs use their existing identifier rules. Requirement lookup uses the declared
requirement ID, including a step-local requirement's globally unique ID.
A source reference selects a whole slot or one requirement's outcome only.
Child fields, indexes, wildcards and `.reason` suffixes are refused at the
source declaration with a field-specific registration error. Reasons remain
inspectable in existing results, not selectable as source inputs. No source
status root, step output or whole-result selector is added. After resolution,
checks use existing local selectors, e.g. Assert each over
`inputs.turns[*].field`, subject to today's position restrictions.

A group Verifier declares named input slots as these reference strings.
Today's typed slots remain valid for ordinary Verifiers:

```yaml
# turn-verifier.yaml: existing typed inputs
metadata: {name: turn-verifier, ...}
spec:
  inputs:
    turn: {type: json}
    trace: {type: trace}
---
# conversation-verification.yaml: selected recorded data only
metadata: {name: conversation-verification, ...}
spec:
  inputs:
    turns: verifiers.turn-verifier.inputs.turn
    # traces: verifiers.turn-verifier.inputs.trace
    # verdicts: verifiers.turn-verifier.requirements.on-topic.outcome
```

At group execution, each reference returns an ordered list with one element
per visible execution of the resolved source binding in that group revision. Order
is ascending input row time, then `execution_id` to break ties, using
the source ingest timestamps rather than a Postgres acceptance counter. A
result-only execution uses its earliest visible result row time for this ordering. References
to the same binding use the same source execution order. A group snapshot containing no executions of that binding yields an empty list, not invented evidence or a vacuous pass.

For an input reference, each element is that execution's whole supplied slot
value; JSON declarations unwrap to their original JSON values. For an outcome
reference, each element is its requirement outcome from the selected visible
result. Select the latest visible result by `wyrd_ingested_at`, then result ID;
this is independent of PostgreSQL settlement. Multiple physical copies of one
logical input/result identity count once; the earliest ingest time of that
identity supplies timing. An input is identified by execution ID and a result
by result ID. Different executions with equal values remain different evidence.
Nothing is filtered by outcome. Missing selected inputs/results are
null in their aligned execution positions, never omitted or fabricated verdicts.
A completed failed judgment remains `failed`; an absent result remains null.
No per-execution source path projection or inner wildcard semantics are added.

A trace-slot source returns the trace ID recorded in that execution's trace
declaration, using the existing trace-ID wire representation. It never fetches
spans, materializes span documents or pins span contents in the group snapshot.
The group input is an ordered list of trace IDs (or null absent slots).

A group Verifier needing spans uses an Agent check declaring the existing
`bifrost.query` built-in (`BuiltinTool::BifrostQuery`), querying by those IDs.
Existing tenant, tool-invoke and `bifrost:query:read`/payload authorization and
bounded query/tool-output behavior apply; no trace-specific group fetch errors
are introduced. Accepted trade-off: spans are read live by the agent, outside
the pinned group snapshot. Late ingestion or retention can change query results
between attempts; frozen source IDs do not make this span analysis reproducible
from the snapshot alone.

Downstream steps/checks access the resolved values as `inputs.<group-slot>`;
the user receives only the selected values, without automatic wrappers,
conversation text, metadata, verdicts or span documents. New group references are
declarative input sources, not permission for arbitrary cross-Verifier reads
from actions. Existing local roots and selector restrictions remain; the
new source declaration uses only the two forms above. Invalid roots,
missing slots/requirements and unsupported suffixes are field-specific
registration failures. Existing local check-selector validation is unchanged.

### REQ-003 — Available-evidence executions and retention

Each group execution pins the complete selected values and source execution
identities it actually reads. Repeated polls of the same snapshot enqueue no
additional execution. New visible evidence is eligible for a new whole-history
snapshot, including while an earlier group execution is pending, running or
retrying. Never mutate earlier inputs or rerun the individual source Verifiers.
There is no outstanding-group gate, persisted revision number, coverage watermark,
source-count comparison, completeness judgment or result-adoption protocol.

Ordinary snapshot idempotence belongs to the existing execution record: a
content digest identifies one snapshot for a binding/group. It is not a source
acceptance ledger. Existing queue claims, leases, retries and settlement remain;
no new group crash-recovery protocol or recovery guarantee is introduced.

There is no permanent finality cutoff. A day-three or later row can reopen a
group and refresh its expiration. Existing evidence retention still applies;
the next snapshot uses whatever history remains visible. Nothing restores lost
outbox data or expired source rows. Revision labels, if displayed, derive from
immutable execution history rather than allocating a revision counter.

### REQ-004 — Pending group scheduling and one group-end duration

The public Trigger activation is exactly:

```yaml
runs_on:
  kind: group_end
  after: 30m
```

`after` is the required existing positive TriggerDuration, without a default.
It replaces idle/max_age/finality, rejected through existing unknown-field
validation without aliases. Frozen settings retain after and exact sources.

There is one PostgreSQL pending scheduling row per `(tenant, binding_id,
group_id)`, containing `due_at`. The binding identifies the group Verifier,
owner and subject; the row contains no evidence, source execution status,
completion inventory or revision counter. Evidence remains in Bifrost under
existing retention. No start/close producer operation is added.

After Scribe acknowledges a grouped source input or result, the existing
background Scribe writer resolves matching registered group bindings from their
frozen source tuples and batches an upsert of their pending rows. SQL computes
`due_at = statement_timestamp() + after`. A subsequent acknowledged input/result
for the same group/binding updates that same due time. It never deletes source
rows or rewrites previous execution inputs. Request handlers perform no SQL or
binding lookup. No separate notification queue, event broker or activity ledger
is added; failed post-ACK SQL uses the existing bounded outbox retry path, whose
identical Scribe batches are absorbed by existing batch fences.

The group loop polls pending scheduling rows, not Bifrost history and not every
Verifier. Tenant RLS and the binding's exact owner/subject/source identities still
apply. Use existing default one-second runtime poll interval; ordinary cron
scheduling and group snapshot work run as separate bounded loops under the
existing VerificationScheduler owner so a slow group read cannot stall cron.
Each loop visits at most 64 overdue tenants per pass and handles at most 32 due
items per tenant, oldest due first. A group item is one binding/group, not one
binding followed by an unbounded set of its groups. Group snapshots are processed
sequentially per process; no overlapping passes or unbounded task spawning.

For one group item, acquire an existing verification-capacity permit without
waiting, then open TenantConn and claim the oldest due scheduling row with
`FOR UPDATE SKIP LOCKED LIMIT 1`. With no permit, claim no row. Hold that row
lock only through the bounded Bifrost snapshot query and queue handoff. In one
transaction, pin/enqueue the successful snapshot using ordinary snapshot
idempotence, delete that pending row, and commit. An identical snapshot conflict
also completes the handoff and consumes the row. Release the capacity permit
and database connection before any Verifier/provider execution. Query or SQL
failure rolls back: the pending row remains for a later pass, with no partial
execution or synthetic verdict. Other replicas skip the locked item.

An arrival updating that row during snapshot/handoff waits in the background on
the SQL row lock. After the handoff commits, its upsert creates a fresh pending
row with a new due time. Thus consuming an older scheduling item cannot delete
new activity. This uses ordinary transactions, not generation counters, claim
leases, recovery sweepers or per-source lifecycle state. Crash before commit
rolls back; after commit the ordinary execution queue owns the pinned execution.
There is no additional group crash-recovery protocol.

One successful Bifrost statement reads the selected group's currently visible
input/result rows together and pins their values. No Bifrost discovery scan,
time checkpoint, payload reread of inactive groups, PostgreSQL source settlement
filter or post-read completeness check is added. Lazy missing tables retain
ordinary handling. Query admission, memory/output limits and deadlines remain.
A failed, degraded or truncated query is not a group snapshot.

The existing execution_slots setting supplies a single process-local capacity
pool shared by queued execution and group snapshot preparation (default 64).
An executing job holds a permit until completion; a group snapshot holds one only
until handoff. Full capacity stops group claims, leaving pending rows in SQL.
This bounds active work, not the durable PostgreSQL backlog: pending scheduling
rows and queued executions can accumulate when arrivals exceed throughput.
There is no distributed quota or new authored concurrency option.

The first acknowledged source record starts scheduling; further records postpone
it by after. Continuous activity can postpone execution indefinitely. Background
retries can postpone a due time; the snapshot digest still prevents duplicate
execution. Polling delay, resource contention and source visibility lag are
accepted. Scribe acknowledgement is not a query-completeness checkpoint: a later
visibility change alone does not generate a scheduling notification. A snapshot
uses what is visible when it runs; the next acknowledged source activity causes
the next execution. No visibility barrier or reconciliation scan is promised.

Existing source validation, exact-version/ambiguity/cycle checks and permissions
remain. There is no max-age override, permanent finality or in-flight-source wait.

### REQ-005 — Existing outbox and queue behavior stays intact

`.verify` authorizes and runs inline, returns its verdict and stages input/result
rows on ScribeOutbox. `.submit` stages its input and returns an execution ID;
only after Scribe acknowledges the input does the existing batched handoff
insert `verifier_executions`. The runner claims, evaluates, settles and stages
results as today. Neither request path reads or writes PostgreSQL.

Grouping reads visible Bifrost evidence, without consulting submission acceptance,
selected PostgreSQL result IDs, pending work or settlement status. A visible
result can participate before its queue execution settles. A staged input can
participate even if a later queue decision refuses that submission. Failed
visible outcomes participate; absent outcomes occupy aligned null positions.
Existing source authorization and TASK-003's pre-staging pressure refusal remain.

Abrupt death or an expired shutdown deadline can lose unacknowledged outbox rows.
A returned verdict or execution ID proves no durable group membership. There is
no additional WAL, receipt, acceptance inventory, repair or reconciliation store.
Pin successful snapshot values on the existing queued group execution; ordinary
whole-execution retries reuse them and never reread Bifrost. Group execution
history and normal queue retry remain, without a group recovery protocol. The
post-ACK scheduling upsert is background SQL, never request-path SQL.

### REQ-006 — Process bounds, existing admission and tenant progress

Bound process staging bytes/items before accepting excess source rows, plus
query output, group snapshot bytes and ordinary active execution work.
Use existing resource admission and tenant claim rounds. Refuse excess producer
work with a typed retryable pressure error and retry timing before staging;
a pressure-refused call stages no source row. Audit remains non-blocking. Source outbox
crash loss remains REQ-005's accepted boundary. No distributed quota ledger,
replica-invariant quota, active-share or no-overtaking guarantee is introduced.

The hot path authorizes, validates bounded inputs, copies the authenticated
owner UID without IO, and stages through ScribeOutbox. Direct verify still
judges inline; submit defers. No additional synchronous Postgres lookup or
write is allowed, including binding resolution. Batch existing row writes;
long evaluations hold no lifetime Postgres connection or in-memory group timer.

At 300 4-KiB turns/s, source content is about 99 GiB/day before overhead.
At 300 five-minute executions/s, unbounded occupancy would be 90,000: enforce
finite process work limits and existing admission instead of accepting unlimited
compute. The later AC-009 benchmark tests a 1,000/s burst with bounded refusal;
TASK-003a proves the functional pressure/drain behavior at normal test concurrency.
Bound pending scheduling claims per tenant pass and subsequent snapshot reads; query count
depends on due groups and bounded batches, not an exact statements-per-tick
contract. Bound query work/output and defer further groups on pressure; never judge a
truncated group or materialize all group payloads in API memory.

Reuse existing tenant claim rounds so a runnable tenant B progresses during
A's burst when resources/dependencies permit. Test that progress, not exact
shares or latency guarantees. Adding WYRD_TARGET replicas adds their local
capacity; it does not create cluster-wide quotas. Existing Postgres claims,
leases and fencing govern ordinary queued execution retries; they do not serialize group revisions;
source outbox-only rows can be lost. Existing Oracle/Scribe admission remains
owned by those services, not a new verification admission system.

### REQ-008 — Caller fan-out and non-vacuous assertion each

The application decides what evidence to send and when. Fan-out consists of N
ordinary verify/submit calls sharing group_id, each judged by a per-item
Verifier as its own execution. Existing queue admission, parallel execution and
whole-execution retries apply per item. A group Verifier reads selected recorded
values through verifiers references at group_end. No step-level action each,
child execution expansion, checkpoint contract, server media extraction or new
media formats are added.

An existing Assert check's `each` selecting zero values is inconclusive, never
passed. This applies everywhere: ordinary inline Verifiers, step requirements
and group Verifiers, including a referenced binding with zero source executions.
Its requirement reason is `empty_selection`. A nonempty assertion retains its
existing comparison behavior. Overall verdict still follows existing decide:
for the journey's single requirement with decide=all, empty means inconclusive,
all passed means passed, and any failed comparison means failed. This is not a
new aggregation language or a list-of-media input contract.

### REQ-009 — Retired group-specific source ceiling

The owner approved removal of the deployment source-execution ceiling,
`source_execution_limit_exceeded`, synthetic ceiling verdicts and public ceiling
metadata. REQ-006's ordinary process/query bounds remain. Keep this ID retired
so earlier packets cannot silently restore its old requirement.

### REQ-010 — Verdict changes in settlement order

Group bindings use `on_verdict_change` only; ordinary bindings retain
`on_failure`. The first settled group verdict establishes the baseline and
creates no dispatch, even when failed. A new settled verdict differing from the
previous settled verdict of the same binding/group creates the existing
transactional Operator dispatch. Equal verdicts create none; changes to/from
inconclusive and failed-to-passed are included.

Revisions can run and finish out of admission order. Compare in settlement
order, not admission order or a notion of the newest evidence winning. Use a
short transaction on the existing binding row to make previous-verdict comparison,
completion and dispatch insertion atomic; never hold this serialization during
queries, evaluation, provider calls or retries. This is ordinary settlement
atomicity, not an outstanding execution gate. A replayed completion never
recomputes a verdict transition or creates a new logical dispatch. Existing
lease fencing and dispatch uniqueness remain; delivery is still at least once.

Context contains bounded group_id, prior/current result IDs, old/new verdicts
and summary, without raw inputs/traces/media or secrets. Delivery does not rewrite
the result. Out-of-order completion can produce a later notification about an
older snapshot; this is an accepted tradeoff, not grounds for adoption machinery.

### REQ-011 — Public surfaces and removed operations

Keep today's `/v1/verification/execute`, `/enqueue`, `/executions/{id}`,
binding inspection and their existing MCP projections. SDK verify/submit
remain the producer workflow; the existing evidence-plan/upload transfer is
part of satisfying the invoked Verifier's slots, not a new evidence-only
group write. Every SDK verify/submit carries optional `group_id`; the wire
`VerificationRequest`, HTTP/MCP projection, stored column and inspection
field use exactly `group_id`. The server validates it within tenant and owner
Service scope. REQ-001 records the resolved OD-004 contract.
Owner recording uses existing authentication under REQ-014, without IO or
attribution wire/SDK fields; background binding eligibility follows REQ-013.

Execution/binding inspection must expose the caller `group_id`, owner
context, pinned Trigger settings, execution/result IDs and
visible source identities through bounded projections. Revision numbers, if
displayed, are derived; no replacement state or revision field is required.
Metadata inspection does not make input payloads readable without permission.
This spec selects no new group read/close route names.

Remove the proposed append/close/create-as-first-append surface and all
`/sessions/*` routes and MCP tools. There is no `type: session` or collection
input kind, collection wrapper, per-contribution check or evidence-appended
activation. A group input is a selected ordinary list from REQ-002, not a
new uploaded evidence kind. Bindings gain no activation `scope` and no
input-selector mapping; group selection lives on the Verifier. REQ-012
separately removes schedule-binding `inputs`.

Existing authorization applies to invocation and every evidence read; a
`group_id` grants no authority. A group reference cannot widen the accepted
execution's permission to another Verifier, binding, table or payload. Reads
of sensitive recorded inputs/results and trace spans require their existing
permissions. Verified producer attribution is recorded, not user-asserted.
MCP reads remain discoverable and writes require explicit scopes plus domain
permission. Audit uses the existing non-blocking path.

Typed errors cover permission denial, unknown/ambiguous reference binding,
idempotency conflict, size, process overload and availability; no text parsing.
SDKs, served OpenAPI, runtime MCP catalogs, generated declarations/schemas and
authored Python stubs agree. No compatibility aliases for removed proposals
are added.

### REQ-012 — Verifier-owned scheduled samples

The Verifier is the single place samples features are declared. A schedule
binding supplies every samples slot of its exact bound Verifier and has no
`inputs` field. Its subject still comes from the binding's subject Card and
its window from the schedule. Claim/retry/recovery preserve those exact
identities without retaining a second binding-owned selector map as authority.

Every samples slot is required: `required: false` on a samples slot is invalid.
To skip a drift check, do not bind the Verifier containing it; do not omit a
slot through binding configuration. Each scheduled evaluation accounts for
every declared drift requirement using the Verifier's own feature/check
configuration. Missing or insufficient samples retain the existing
inconclusive behavior; they do not authorise dropping a requirement or
passing a reduced subset. Existing infrastructure-error semantics remain.
Optional JSON, media and trace slots are unaffected.

Registration refuses any `inputs` field on a schedule binding, including an
empty map, with a field-specific error identifying that binding's `inputs`
path. It refuses `required: false` at the samples slot's `required` path.
There is no compatibility alias, implicit rewrite or silently ignored field.
The authored binding is:

```yaml
verified_by:
  - verifier: ./latency-shift.yaml
    runs_on: ./daily.yaml
    on_failure: [./alert-on-call.yaml]
```

Fixtures, examples, documentation, generated schemas/types and all SDK
projections must use this contract. Group input references in REQ-002 and
inline input bundles remain distinct from scheduled samples. This change
removes redundant scheduled wiring, not the existing Window/Drift path.

### REQ-013 — Same-owner binding resolution and data isolation

At registration of the containing Service, resolve each
`verifiers.<name>...` only against Verifiers bound on that same exact owner
Service or the same component occurrence as the group binding. Match the
target Verifier Card's `metadata.name` after resolving binding `Ref` values,
whether authored via a path, same-request sibling or an already registered
Card elsewhere. Pin the matched binding identity and its exact Verifier
version/UID. Never resolve against only locally declared files, another
Service, a whole tenant's registry, a component alias or the latest version.

Zero matches or more than one match is a field-specific registration refusal
at the group's declaring input, including ambiguity across versions/spaces or
multiple same-name bindings. Refuse absent slot/requirement IDs against that
exact pinned spec. Reject malformed references without persisting an invalid
binding. Do not use declaration location as an access boundary.

At runtime resolve eligibility in the background group query using the pinned
source bindings: tenant, authenticated owner Service UID, subject card_uid,
verifier_card_uid and group_id must match. Source input/result binding_id is
not required or used to infer a different caller route. Null owner, wrong owner,
wrong subject/Verifier or another group excludes the row. The exact source
version is the one pinned by that binding, not the latest registration.

The producer does not record which occurrence/binding route it intended.
If multiple bindings would make the same recorded tuple ambiguous for a group
reference, refuse that participating lookup at registration; never choose a
first match or invent occurrence provenance. Limit this check to bindings
participating in group lookup, not every binding in the Service. Calls with
the same recorded owner/subject/Verifier tuple have the same group eligibility
regardless of whether invoked by a bound-name convenience or directly by UID.

### REQ-014 — No-IO owner recording and background binding eligibility

Every verify/submit row records nullable owner_card_uid from the authenticated
principal's already available owner Service card_uid, plus existing subject
card_uid, verifier_card_uid, run_id and verified principal IDs. This is a
metadata copy, not an owner/binding lookup. Add owner_card_uid to the input
schema (results already have it); retain it with recorded inputs/results through
existing submission processing. `Principal::card_ref` is an IO-free source of
Card identity; only an available owner Service identity is used. No owner is
inferred by querying parents of an Agent Card or a registry. Without an
available owner Service UID, record null and permit ordinary invocation.

No universal source binding_id propagation to inputs/results/queue, new tuple
lookup, synchronous read/write or SDK attribution field is required. Existing
binding columns and activation behavior remain for their current uses. A call
with owner identity but no background binding match still executes; eligibility
is determined only under REQ-013 when group data is queried. Binding is not
an invocation permission or automatic reaction. Existing verifier:run and
payload authorization remain; direct verify has no automatic dispatch.

Ambiguity validation is REQ-013's group-reference check only. Unrelated duplicate
tuples across occurrences are not globally banned by this change.

### REQ-015 — Capacity qualification and synchronous overhead

Extend the existing `crates/wyrd/wyrd-testing/src/bin/capacity/` binary via
`mise run bench:capacity`. Do not create another benchmark or put it in a gate
or verify lane. Retain its existing production mix, normal-load SLOs, one/two
replica qualification, bounded command lifetime and target/capacity Markdown/
JSON evidence. New cases belong to this same opt-in benchmark/report; profiling
remains diagnostic, not authoritative performance evidence.

The owner target is synchronous assertion-only direct verification overhead
**p99 < 0.001 seconds**, using the existing
`wyrd_verification_engine_overhead_seconds` with mode=direct (engine elapsed
minus measured external waits). It is not client latency, total request time,
a sum of phase percentiles or a replacement metric. Preserve the existing
capacity **p95 < 0.010 seconds** SLO and its original scope. Judge and report
both: the new p99 target is stricter for the specified assertion-only cases;
passing the older p95 check cannot excuse missing it.

Use matched warmed release runs at a common normal offered load, with the same
assertion-only Card, JSON evidence, authenticated owner, host/topology and
background mix. Separate no-group and grouped measurement windows; no unrelated
direct workload may contaminate their metric samples. Collect at least 1,000
overhead samples per measured direct case. Sum histogram bucket deltas across
replicas before calculating p95/p99; never average replica percentiles. Retain
counts, buckets/resolution and units. An upper bound at 1 ms does not prove
strictly below 1 ms: insufficient resolution/samples cannot pass. Extend the
same metric/report resolution if needed, not its meaning or threshold.

Use repeat baseline/grouped comparisons and disclose a noise envelope derived
from repeated baseline measurements on that same setup, with the method fixed
before judging the grouped delta. Report that envelope and the delta in time
units; a repeatable positive delta beyond noise fails. A baseline already
missing 1 ms is pre-existing work **in scope of this change**, not a waiver:
report it and leave overhead qualification failed until addressed. The engine
metric excludes work outside its intervals; report existing request/phase and
client timing alongside it, and retain REQ-014's no-new-IO check rather than
claiming this metric alone proves all request-path costs absent.

AC-009 defines five mandatory cases. Preserve existing normal-load error/SLO
judgments. Only the explicitly overloaded case expects retryable pressure;
label it separately so expected refusals do not become normal-load successes.
Missing evidence, unfinished drains or skipped cases cannot establish PASS.

## Acceptance criteria and concrete user journeys

Journeys use real Rust/Python/TypeScript SDK → WyrdTestServer +
repository-managed Postgres → client paths, actual storage and engine execution.
Each shipped capability runs through all three SDKs; mixed casts additionally
prove shared caller `group_id` values across languages. Agent-facing HTTP/MCP journeys
discover the existing verification tools, invoke and inspect under the same
contracts. Local deterministic provider responses are allowed; a replacement
in-process engine is not the journey.

Eligible calls in J-001–005 authenticate through their Card-bound owner Service;
owner UID is copied from authentication without IO; binding eligibility is
resolved in the background, never supplied by an SDK.
Unbound invocations still run; only matching owner/subject/Verifier tuples
are eligible for group lookup.
Journeys pass `group_id=` on verify/submit and configure `kind: group_end`
on group bindings. Fixtures identify the common owner Service/component and
flush source rows and commit their pending updates before polling in basic flows, and use the available-evidence
variants below for absent results. Database-derived pending due times and schedule cursors, never a process
clock, simulate elapsed days; tests do not sleep for days or use producer
timestamps to advance server time.

### AC-001 / J-001 — Resumable e-commerce conversation

**Maps:** REQ-001–006, REQ-010–011, REQ-013–014; INV-001–005.

Python chat, TypeScript support and Rust order producers share
`group_id="order-123/chat-7"` under the same authenticated owner. A source turn
Verifier records six delivery-request, order-status, resumed-request, human-handoff,
acceptance and replacement-confirmation values through ordinary verify/submit.
A group source `turns: verifiers.turn-verifier.inputs.turn` and `after: 30m`
produce one passed group Verifier execution with all six ordered values once evidence is quiet.
An unchanged poll creates no execution; a later turn refreshes expiration and
produces a new immutable group Verifier execution over available history. Settings are per binding;
a second binding with `after: 15m` does not change the first.

Keep exact source/version and owner isolation, successful enqueue replay identity,
conflicting-content queue error, permission denials, ordinary ungrouped behavior
and field-specific `inputs.group_id` registration refusal across typed/source
slots. Do not assert that a post-staging queue conflict hides an already visible
input: REQ-005 explicitly accepts its participation. Keep other valid Python
keyword slots usable. Inspection returns group_id, source identities and pinned
values without old timing/coverage/ceiling fields.

### AC-002 / J-002 — Refund failure three days later

**Maps:** REQ-001–006, REQ-010–011, REQ-013; INV-001–005.

Support, payment and returns producers use `group_id="order-456/refund"`,
`refund-turn` and `after: 30m`. The group Verifier slots select whole fact inputs and named
valid-fact outcomes. Day-zero promise/accepted-order evidence passes and sets
the baseline without dispatch. Day-three overdue/returned-but-unrefunded facts
refresh expiration; after 30m plus cadence/visibility, a group Verifier execution over available history
fails and creates passed-to-failed dispatch. An additional failed revision creates
no repeat reaction; corrected paid-in-full facts can pass and dispatch recovery.
Prior snapshots/results remain immutable. There is no day-three max-age bypass
or eventual finality cutoff.

Keep one group Verifier execution running, add evidence and expire its `after` period. A new execution
may be enqueued before the first completes. Prove immutable inputs and settlement-
order comparison in both completion orders, including completion replay. Do not
assert serial execution, coalescing until settlement or newest-revision adoption.

### AC-003 / J-003 — Coding work without tracing

**Maps:** REQ-001–006, REQ-008, REQ-010–011, REQ-013; INV-001–005.

Rust bridge, Python test runner and TypeScript editor record task prompt, edits,
test results and final diff using ordinary calls with `group_id="issue-812/attempt-1"`
and `after: 30m`. Individual no-secrets/forbidden-command checks return immediately.
The group selects declared turn inputs and safety outcomes: clean work passes,
exit-one tests or forbidden commands fail, missing required evidence follows
existing inconclusive rules. Wyrd runs none of the producer's commands.

Flush a submitted input while its result is absent. After `after`, the snapshot
contains the input and aligned null outcome; a declared check decides what that
means. A later visible result refreshes expiration and appears in the next
snapshot even if PostgreSQL settlement has not selected it. With multiple visible
result IDs, REQ-002's deterministic latest-visible rule applies.

Retain the original optional telemetry branch: selected trace-slot references
return IDs only. An explicitly authorized check Agent can use existing bounded
`bifrost.query` to read live spans. A span appearing after pinning may be visible
to that query without changing pinned IDs. Missing tool/query/payload permissions
are ordinary typed denials; no automatic span expansion or fallback is added.

### AC-004 / J-004 — Application-sampled media

**Maps:** REQ-001–006, REQ-008, REQ-010–011, REQ-013–014; INV-001–005.

The application samples twenty JPEG/PNG frames for `group_id="door-42"`, uploads
and submits them individually through existing APIs, and records an ordinary
JSON event fact. A group selects the frame Verifier's person-wearing-badge
outcomes plus the event fact, with `after: 2m`. After the source results are
visible and quiet, twenty passed outcomes pass; a failed frame fails; an event
fact with zero frames produces `[]` and inconclusive/empty_selection. Existing
per-frame whole-execution retry remains, without new group recovery tests or a
fanout engine. Foreign-owner and denied calls retain existing isolation.

There is no special twentieth/twenty-first execution boundary. Twenty-one frames
within ordinary query resources are evaluated normally. A missing result is null;
a later result can create another group Verifier execution. No counters, source ceilings,
checkpoints, video processing or new media formats are introduced.

### AC-005 / J-005 — Many conversations, pressure and existing Drift

**Maps:** REQ-001–006, REQ-008, REQ-010–014; INV-001–005.

Many owner-isolated conversations use grouped verify/submit alongside heavy queued
work in tenants A/B. Exercise real grouped calls through TASK-003's finite staging:
pressure gives the existing typed retryable refusal before any source row is
staged; after drain retry succeeds; runnable B progresses during A's continuing
burst. Accepted work drains through existing owners, audit remains non-blocking,
and no additional queue, distributed quota or strict fairness claim is added.
A separate existing scheduled Window/Drift check fails a resolution-score drop
from 0.95 to 0.60 against threshold 0.80. Group revisions do not replace Drift.
Functional proof uses normal bounded test concurrency, not host stress or capacity
qualification; offered-rate/performance measurements remain AC-009's later work.

### AC-006 — Contract, available snapshots and unchanged platform behavior

**Maps:** REQ-001–006, REQ-008, REQ-010–014; INV-001–005.

Prove all shipped SDKs and discover-act-observe HTTP/MCP paths use the same
`group_id`, source grammar, `after` setting and inspection projection. Resolve
paths, registration siblings and registered CardRefs to exact pinned versions.
Wrong tenant, owner, subject, version or group cannot contribute. Ownerless calls
still run without contributing. Reversed result completion must preserve common
input/outcome execution ordering, null alignment and failed values. Physical
identity duplicates do not add list elements; independent equal-valued executions
do. Only declared slots appear, with no source child selectors or reason roots.

An expired pending scheduling row selects one successful query and immutable snapshot. Visible input
without result is usable immediately; later evidence yields another revision;
unchanged snapshots do not rerun, and a consumed pending row causes no further
Bifrost read without a new acknowledged source input/result. Include newly acknowledged rows with equal/older ingest
timestamps: PostgreSQL scheduling notifications, not timestamp cursors, select work. Retention and
outbox loss limit available history; neither implies recovery. Failed/truncated
queries enqueue no partial or synthetic result and retry through the same poller.
Ordinary query resource refusal is not a subject failure or an inconclusive
Verifier judgment. No custom source-execution ceiling remains.

Keep real journey proof of producer SQL absence, owner copying, permissions,
successful submit replay, ordinary ungrouped inline/document/trace/media checks,
empty-each semantics, Window/Drift, non-group Operators and group verdict-change
baseline/replay behavior. SDK types, served OpenAPI, runtime MCP schemas, authored
Python stubs and generated declarations agree. There is no group start/close API.

### AC-007 — Schedule bindings supply every Verifier samples slot

**Maps:** REQ-012; INV-003–005.

Through each Rust, Python and TypeScript SDK, register a subject, a schedule
Trigger and an exact drift Verifier with two samples slots (`latency` and
`error_rate`), each declaring its corresponding feature, and two drift
requirements. Bind the Verifier with only `verifier`, `runs_on` and optional
`on_failure`. Use WyrdTestServer plus repository-managed Postgres and the real
sample-write → scheduled execution → result-read path.

1. Record a sufficient window of `latency=120` and `error_rate=0.01` samples.
   Deterministic Custom metric checks declare latency above 100 as failed and
   error rate above 0.05 as failed; use `decide=all`.
2. Let the schedule activate and read its execution through another client.
   It reaches `completed/failed` with both declared requirements present:
   latency failed, error rate passed. Result provenance names the exact
   Verifier/subject and scheduled window; no binding selector supplies or
   filters either series. Retry/reclaim retains the same requirement set.
3. Repeat with sufficient `latency=80` samples and insufficient `error_rate`
   samples. The execution reaches `completed/inconclusive`: latency passed,
   error rate inconclusive. The latter is never silently omitted to produce
   a pass over the latency requirement alone.

Registration negatives use public SDK/HTTP registration: a schedule binding
with an `inputs` map, even an exact repetition of the Verifier or `{}`, is
refused with a field-specific error pointing to that binding's `inputs`.
A samples slot with `required: false` is refused at that slot's `required`
field. No invalid Card/binding persists and no execution is activated. SDK
validation may fail earlier, but direct HTTP must prove the server refusal.

Existing scheduled-drift journeys pass in Rust, Python and TypeScript with
the binding field removed, retaining scoring, window, insufficient-data and
failure-Operator behavior. Supporting registration tests cover field paths;
served OpenAPI, generated schemas/types, fixtures, examples and docs agree
that a schedule binding accepts no `inputs` and samples slots are mandatory.
These obligations describe future implementation proof, not tests run while
editing this specification.

### AC-008 — No-IO owner recording and background eligibility

**Maps:** REQ-014, REQ-001, REQ-005, REQ-013; INV-003–005.

Use real verify/submit journeys across all SDKs plus HTTP/MCP with one Service,
subject and exact source/group bindings. Send no attribution fields.

1. Invoke the turn Verifier through verify and submit. After drain, inputs/
   results record the authenticated owner Service UID and existing subject,
   Verifier, run and principal identities. Prove attribution adds no synchronous
   binding/owner lookup or Postgres write; submit retains its no-Postgres path.
   Do not require source binding_id or new owner/binding queue columns.
2. A principal with verifier:run but no owner Service invokes the same subject,
   Verifier and group_id through direct HTTP and enqueue: invocation succeeds,
   owner is null and the background query excludes those rows. A different
   Service owner is recorded as itself and excluded. An owner-matched call
   without a matching source binding still executes but is excluded later.
3. An owner-matched direct UID invocation of the same eligible subject/Verifier
   tuple is included just like the bound-name call, even without binding_id.
   Bound-name routing cannot establish provenance absent from recorded rows.
4. Refuse ambiguous source lookup at the group's declaring reference, including
   two participating occurrences with the same recorded tuple. Duplicate tuples
   unused by any group lookup remain allowed under existing registration rules;
   this change adds no global uniqueness ban.

Deny missing invocation/payload permission, never missing binding alone.
Direct verify has no automatic reaction. Replayed queued work preserves recorded
owner metadata through existing processing. Background eligibility/authorization
must remain tenant-isolated and pinned to the declared exact versions.

### AC-009 — Existing capacity benchmark covers overhead and grouping load

**Maps:** REQ-015, REQ-001–006, REQ-008, REQ-010, REQ-013–014; INV-001–005.

Run the extended bench:capacity through the existing real public Rust-client
capacity driver/server deployment; SDK functional parity stays in J-001–005,
not three new performance harnesses. Store case verdicts plus raw measurement
inputs in its existing Markdown/JSON report artifacts. Record revision/build,
hardware, replica/tenant counts, workload/payload sizes, warmup/window/sample
counts, process bounds, group/source counts, Trigger/poll settings, achieved
traffic, errors, memory/CPU and drain times so runs can be compared. Group
fixtures use short declared after durations to finish within the
existing command lifetime; no extra long-running harness or host stress tool.

| Case | Report | Pass/fail |
|---|---|---|
| 1. Baseline direct verify, assertion-only, no group_id | Overhead p95/p99, sample/bucket evidence and existing request/client timings at the matched load. | Correct judgments, normal-load SLOs and p99 < 1 ms. A current-baseline miss is explicitly marked pre-existing work in scope, not excluded from the verdict. |
| 2. Same direct verify with group_id and authenticated owner metadata | The same metrics, grouped-minus-baseline overhead delta, repeated-baseline noise envelope, and evidence of recorded metadata. | Both overhead SLOs hold and the positive delta does not exceed the disclosed noise envelope. No additional producer Postgres IO; insufficient measurement resolution cannot pass as zero delta. |
| 3. Grouped submit at rising load | Offered/accepted/completed executions/s, normal capacity knee, queue depth/age, throughput plateau, refusals by code, client latency and all backlog drain times. | Identify the throughput ceiling; below it retain normal-load SLOs. Saturation is reported as the limit, not hidden as acceptable loss. After stopping load, existing backlogs drain within 60 s; normal-load unexpected refusal/loss/wrong judgment fails. |
| 4. Many groups × N source executions and group_end | Explicit group/N sweep at representative N values; expiration-to-result p50/p95/p99; pending scheduling SQL and Bifrost snapshot query count, latency and rows/bytes read, plus group queue/drain evidence. | Every eligible fixture gets its correct whole-group result, without truncated inputs or bypassing ordinary query bounds. Resource refusal defers group verification rather than creating a synthetic verdict. Ordinary due-group results complete within the existing 60-second drain allowance after their configured due point; report latency separately, without inventing a sub-ms group-latency SLO. |
| 5. Burst above the measured capacity | Per-process staged bytes/items against bounds, memory time series/peak, accepted/refused counts and typed retryability/retry timing, drain time and per-tenant completion timeline. | Staging stays within finite process bounds; warmed memory plateaus while overload continues, stays inside the declared process envelope and returns toward baseline after drain. Excess requests receive typed retryable refusal before staging; no unexpected loss/errors. An admitted tenant B completes work during A's continuing burst using existing claim rounds. Existing backlogs drain within 60 s after stopping the burst. No strict distributed fairness/quota claim. |

Use the original J-005 workload's 300/s qualification and 1,000/s ten-second burst as offered points
alongside the measured knee; bounded refusal is valid above provisioned capacity.
For the memory case, repeated equal bursts must not increase retained staging
or post-drain memory in proportion to offered requests; disclose baseline
variation and envelope, rather than claiming byte-identical RSS. CPU/memory
remain reported-only for the old normal steps; the new overload case expressly
judges their boundedness. Declared incomplete-evidence results are
correct judgments, not failed execution errors. Expected overload refusals are
counted and judged only in the overload case; existing normal-load zero-error
rules and client queue drain <=1 s remain unchanged. Produce an overall extension
PASS only with all five case criteria satisfied, alongside the unchanged legacy
capacity verdict. No benchmark result is claimed merely from specification edits.

## Required boundaries, accepted tradeoffs and completion boundary

Keep existing owners: pure `wyrd-spec` contracts; shared `wyrd-client` and thin
SDKs; Rust server registration/poller/runner; `wyrd-sql` binding/queue settlement;
Bifrost evidence and admission; Vala checks; existing Operators. No new group
evidence store, execution queue, lifecycle service, recovery worker or client-owned
durable logic. The single pending scheduling table in REQ-004 is authorized.

Accepted limits are part of the requirement, not review defects:

- Visible evidence is not an acceptance ledger. Missing, delayed, refused-after-
  staging, attempt and retained rows follow the explicit selection rules.
- Continuous arrivals can postpone group end indefinitely. No max-age override.
- Late rows reopen groups without a finality cutoff; evidence retention still
  limits history. Reusing a group_id intentionally refers to that same history.
- Distinct snapshots can overlap. Settlement order governs reactions; older
  evidence can finish later. No serial group execution gate or newest-result adoption.
- No new group crash recovery. Existing queue retries persist pinned executions;
  outbox loss remains loss. An errored snapshot is not automatically rediscovered
  as new work solely because it failed; ordinary execution retries own that work.
- Query limits can defer a pending group indefinitely until resources/data
  change. No truncated group verification and no synthetic source-limit verdict. Benchmarking
  measures this capacity boundary rather than adding preventive machinery now.
- Acknowledgement triggers scheduling but does not guarantee query completeness.
  A later visibility change alone does not trigger another execution. No scan or
  barrier repairs this; subsequent acknowledged evidence schedules another snapshot.
- Scheduling updates can wait on the bounded snapshot transaction's row lock;
  this delays only background work, never the originating response or permission.
- Runtime permits bound active work, not durable SQL backlog. Repeated source
  notifications can postpone expiration; unchanged snapshots still execute once.
- Optional Agent span queries remain live and outside pinned source values.

Reviewers must assess these declared semantics, not restore removed mechanisms
as safeguards for these accepted limits. Security, tenant boundaries, trust-boundary
validation, finite resources, ordinary errors and immutable evidence remain mandatory.

TASK-003a closes functional AC-001–008 and documentation against the merged
TASK-001–003 baseline. It preserves TASK-002/003, updates architecture and user
examples, regenerates contracts, and records focused passing evidence. REQ-009
is retired. REQ-015/AC-009 remain required subsequent opt-in benchmarking work;
TASK-003a must leave its existing driver usable and scenarios configured with
`after`, but neither implement new benchmark cases nor claim performance PASS.
The owner explicitly accepted the tradeoffs above. No open material behavior
decision remains for TASK-003a. Its revision-17
implementation details define the pending-row, batching and capacity seams.

## Revision history and authority links

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Initial behavioral specification from owner notes and agreed architecture; post-merge runtime facts rechecked. Owner resolved OD-001: evidence-storage content write and Postgres state commit precede receipts/terminal results, with durability supplied by the deployment's stores. Owner assigned session lifecycle settings to Triggers, with longest-value composition. Owner resolved OD-002: existing evidence retention, no finality deletion, checkpoint scratch removed at terminal state and privacy erasure out of scope. Owner resolved OD-003: human checks remain out of scope; future human checks are attributable review. Specification approval remains outstanding. |
| 2 | draft | Owner removed redundant schedule-binding `inputs` and `required: false` on samples slots; Verifier declarations supply all scheduled samples and drift requirements. Added REQ-012, AC-007 and binding/doctrine impact; repository facts rechecked, including the current optional-slot omission path. Other decisions unchanged; specification approval remains outstanding. |
| 3 | draft | Owner replaced evidence-only accumulation with grouped verify/submit executions and explicit cross-Verifier input references. Reworked REQ-001–006, REQ-010–011 and journeys; added REQ-013 for same-owner binding resolution. Retained REQ-007–009, REQ-012 and verdict-change reactions. Removed append/close, contribution input wrappers, binding scope and per-contribution activation. Open OD-004/005 and source-completion policy OD-006; approval outstanding. |
| 4 | draft | Owner fixed server-derived attribution using existing authentication and subject/Verifier fields, preserving unbound invocation and query-only binding isolation. Added REQ-014/AC-008, tuple-ambiguity refusal and excluded unbound verify in J-001. Recorded the missing input owner column and existing queue-shape constraints; no attribution wire/SDK change. Window submit sample recording is correct existing behavior, not a gap. Open decisions otherwise unchanged; approval outstanding. |
| 5 | draft | Owner resolved OD-004: optional `group_id` on verify/submit in every SDK, VerificationRequest and stored columns, scoped to tenant and owner Service. Updated prose and journeys to the exact spelling; no state.run argument or alternate field alias. OD-005/006 remain open; approval outstanding. |
| 6 | draft | Owner resolved OD-005: `TriggerActivation` gains `kind: group_end` with idle/max_age durations, retaining Trigger-owned finality. One Postgres lifecycle row per tenant/owner Service binding/group_id is upserted by eligible grouped verify/submit; existing scheduler claims and queue handle group ends, revisions and finality. Recorded hot-path cost and scheduler race/recovery acceptance coverage. OD-006 remains open; approval outstanding. |
| 7 | draft | Owner resolved OD-006: group evaluation waits for all referenced-binding source executions to become terminal, bounded by the revision max_age deadline; all sources remain selected regardless of status/outcome. Added `verifiers.<name>.status` reusing existing execution lifecycle values, aligned null positions for absent results, durable immutable readiness/cap snapshots and SDK/HTTP/MCP journey variants. No open material decisions remain; approval outstanding. |
| 8 | draft | Owner rejected per-call Postgres writes and replaced group lifecycle/readiness with fixed-cadence binding-cursor Bifrost discovery. Removed group/revision rows, producer counters/sequences, terminal-source waits, synchronous receipts/finality refusals and their journey assertions. Revisions follow uncovered visible input/result rows; results are activity, finality bounds re-opening, and outbox crash loss/poll-plus-flush latency are accepted. Status references project existing results.execution_status only. Prior conflicting decisions are superseded; approval outstanding. |
| 9 | draft | Owner accepted all seven complexity findings. Replaced distributed quotas/strict fairness with process bounds and tenant progress; parallel numbered revisions/adoption with serial coalesced group executions; universal binding attribution with no-IO owner recording and background eligibility; resource budgets with existing concurrency/byte admission plus one persistent dispatch ceiling; new aggregates with existing decide and counts; longest-duration composition with per-binding pinned settings. Dropped status references and cut-only fields/errors/journey assertions. Retained generic each, checkpoints and MP4 requirements with fan-out design open as OD-007; approval outstanding. |
| 10 | draft | Owner resolved OD-007: caller fan-out is ordinary per-item verify/submit sharing group_id, aggregated at group_end. Removed REQ-007 checkpoints, internal step each, MP4/extraction/segmentation, fan-out budgets/coverage and their cut-only errors, ACs, journey steps and boundaries. Reworked REQ-008/009 for globally non-vacuous Assert each and one deployment source-execution read ceiling. Rewrote J-004 as 20 application-sampled JPEG/PNG submissions with isolation, zero-frame, retry and ceiling negatives across SDKs/HTTP/MCP. No open material decisions remain; approval outstanding. |
| 11 | draft | Owner accepted all four second-review cuts. Trace-slot sources return recorded trace IDs only; optional J-003 spans are read live by an Agent using existing bifrost.query authorization, not snapshotted. Source grammar is whole slots or requirement outcomes only; removed child projections/reason selection and cut-only assertions. Group bindings use on_verdict_change alone, first result baseline; ordinary on_failure unchanged. Replaced fixed query-count/B/P claims with bounded discovery and snapshot reads. No open material decisions remain; approval outstanding. |
| 12 | draft | Owner added REQ-015/AC-009: extend existing opt-in bench:capacity and wyrd_verification_engine_overhead_seconds for assertion-only direct p99 <1 ms while preserving p95 <10 ms and existing normal-load SLOs. Require baseline/grouped noise comparison, grouped-submit throughput/drain, group-end read/latency/ceiling and bounded-overload/tenant-progress evidence. Baseline misses remain work in scope; expected overload refusals are isolated from normal zero-error judgment. No gate/verify lane or separate benchmark added; approval outstanding. |
| 13 | approved | Owner reserved group_id as a Verifier input slot name to avoid the new Python keyword argument collision. Updated REQ-001 and AC-001 registration negatives across SDK/HTTP/MCP, including typed/source declarations and renamed-slot success. Rechecked existing SDK signatures: no other equivalent argument-name collision or reserved-slot blacklist; reuse validate_slot/invalid and WyrdError::Validation rule/path shape. No other reserved names added. Owner explicitly approved revision 13; this entry records that approval without changing the revision or requirements. |
| 14 | draft | Plan-readiness PR-001 found no transition for existing managed verification tables. Owner confirmed nothing has ever shipped: added the pre-release schema constraint (in-place definition change, reset not upgrade, checks unchanged, no upgrade journey). Requirements and ACs otherwise unchanged; approval outstanding. |
| 15 | approved | TASK-001 review found group timing compared PostgreSQL's clock with server-process row times (`recorded_at` stamped at enqueue, `ended_at` by the runner), which drift across replicas; AGENTS §15 makes PostgreSQL the only time source, client-produced timings excepted. Owner approved REQ-004 row times as the managed `wyrd_ingested_at` from Bifrost's PostgreSQL-anchored clock for source order, first/newest, idle/max_age, finality and coverage, and journeys advance time through binding durations and cursors only. Carries revision 14 unchanged. Owner explicitly approved this revision. |
| 16 | approved | Owner approved the simple visible-evidence grouping proposal and cuts and requested TASK-003a on merged TASK-001–003. One after duration, fixed cadence, pinned visible snapshot, late-row revisions, overlapping execution with settlement-order reactions; no PG source reconciliation, outstanding gate, hard finality, public coverage or custom source ceiling. TASK-002/003 retained; all original functional scenarios remain; benchmarking follows functional closure. |
| 17 | approved | Owner approved explicit background group scheduling: one pending PostgreSQL row per tenant/binding/group, post-ACK due-time upsert, bounded due-row polling, one visible snapshot, transactional execution handoff and row consumption. No repeated Bifrost discovery. New arrivals reschedule; evidence is retained. Ordinary cron and group loops are independent; existing execution capacity also gates snapshots. ACK/query-visibility, SQL backlog and background retry limits are explicit. |

Authorities: [AGENTS.md](../../../AGENTS.md),
[agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md),
[doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md),
[reference router](../../../architecture/references/README.md),
[evaluation](../../../architecture/references/domain/evaluation.md),
[architecture patterns](../../../architecture/references/architecture/patterns.md),
[analytical reliability](../../../architecture/references/domain/analytical-operations-reliability.md),
[agent harness](../../../architecture/references/languages/agent-harness.md),
[spec workflow](../../../architecture/references/languages/spec-driven-development.md).
Inputs: [owner decisions](owner-notes.md), [proposal](advise-proposal.md),
[agreed architecture](architecture-diagram.html). The owner model recorded in revision 3 supersedes the earlier accumulation
model in these inputs.
