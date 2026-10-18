---
id: SPEC-long-horizon-verification
revision: 15
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

Revision 13 retains the owner's replacement of the earlier
evidence-append model. Current owner instructions override the older
[proposal](advise-proposal.md), [notes](owner-notes.md) and
[diagram](architecture-diagram.html) wherever they differ. The owner explicitly approved revision 13.
Revision 8 supersedes earlier Postgres lifecycle,
readiness and receipt guarantees with the owner's existing-outbox durability
and background-query model. Revision 9 applies all accepted complexity-review
cuts. Revision 10 resolves OD-007 by keeping fan-out entirely in the caller
workflow; internal fan-out, checkpoints and media-processing extensions are cut.
Revision 11 narrows source references, removes automatic trace expansion and
overlapping group reactions, and permits bounded multi-query snapshot reads.
Revision 12 adds load and overhead qualification through the existing opt-in
capacity benchmark, with both overhead targets retained. Revision 13 reserves
`group_id` as a Verifier input slot name to avoid the Python keyword collision.
Revision 14 records that Wyrd has never shipped: managed table schemas change
in place with no retained-table upgrade.

### Rechecked repository facts and required gaps

| Inspected source | Fact and consequence |
|---|---|
| `crates/wyrd-spec/src/card/verifier/reference.rs::{ReferenceRoot,PathSegment,Reference::parse}` | Existing roots are `inputs.<slot>`, `steps.<id>.output`, `steps.<id>.status`, `steps.<id>.requirements.<requirement>.outcome\|reason`, and `requirement.id\|statement`. Fields, indexes and wildcards parse, but child selectors are limited to inputs/step outputs and wildcard use is restricted by validation. There is no `verifiers` root today. |
| `crates/wyrd-spec/src/card/verifier/mod.rs::{VerifierSpec,VerificationBinding}`, `crates/wyrd-spec/src/reference.rs::Ref` | Input slots currently have typed bodies, not reference-string declarations. Binding `verifier` is a `Ref`: authored path, registration sibling, or registered CardRef, resolved to exact identity. New group input strings and binding-local name resolution are contract extensions. Declaration location cannot determine eligibility. |
| `crates/shared/wyrd-client/src/observe/mod.rs::{Observe::verify,Observe::submit}`, `src/verify/mod.rs::{Target::execute,Target::enqueue,RunEvidence::pin}` | The client resolves a bound Verifier by name, projects its slots, and hashes/plans/uploads declared files as part of verify/submit. Run submit enqueues; verify executes inline. No new evidence-only producer verb is needed. |
| `crates/vala/vala-bifrost-redux/src/tables/verification/{execution_inputs,results}.rs`, `wyrd-server/src/verification/results.rs::ResultPayloadBuilder::{inputs,build}` | `vala.verification.execution_inputs.inputs` stores pinned input declarations; `vala.verification.results.outcome` stores requirement IDs, outcomes/reasons and the result breakdown. They have execution/Verifier/binding correlation, and results have owner correlation. These are the source data, not a new contribution store. Retries may leave multiple result payloads; the completed execution's selected result identity is authoritative. |
| `wyrd-server/src/components/verification/service.rs::{execute,enqueue,stage_direct}`, `src/verification/runner.rs`, `wyrd-runtime/src/principal.rs::Principal::card_ref` | Direct verify already reads Postgres; enqueue stages an input row without Postgres IO. Current builders set owner/binding to None. REQ-014 copies an authenticated owner Service UID without IO; group eligibility is resolved only in the background. Existing subject/Verifier/run/principal identities remain. No new producer Postgres read or write is permitted. |
| Verification table schemas, `wyrd-sql/src/queries/{verification,verifier_executions}.rs` | Results already have nullable owner/binding fields; execution inputs lack owner_card_uid. Add the owner and group_id metadata required by REQ-001/014. No universal per-source binding_id propagation or global tuple-uniqueness rule is required. Ambiguity refusal is limited to bindings used by group lookup. |
| `vala-verify/src/blocks/scope.rs::Scope::resolve`, `blocks/engine.rs::json`, `wyrd-server/src/verification/hosted/tools.rs::Invocation::call` | Generic trace references retain InputDeclaration::Trace containing the trace ID; automatic span fetch belongs to existing trace checks. Group sources project the recorded ID only. BuiltinTool::BifrostQuery already invokes the caller-authorized bounded query owner, including tool:invoke and bifrost:query:read; span queries remain live, not materialized group inputs. |
| `crates/vala/vala-verify/src/{blocks/engine,walk}.rs`, `wyrd-server/src/verification/{runner,leases,claims,submissions}.rs` | The engine is vala-verify; runtime contracts live in wyrd_spec::verification. Existing async claims, leases, retries and settlement provide per-item execution/retry for separate submit calls. This change adds no checkpoint or internal fan-out engine. Existing staging is unbounded and enqueue is staging-only; bounded process staging remains required. |
| `VerifierSpec::validate_schedule`, `wyrd-server/src/verification/drift.rs::DriftEngine::checks`, `wyrd-sql/src/queries/verifier_executions.rs` | Supplied schedule selectors must equal their samples slot's feature set; optional slots can currently be omitted. Runtime uses frozen slots only for check-key presence; series come from the check, subject from `card_uid`, window from the schedule. Claim freezes `schedule_inputs` into `ExecutionInput::Scheduled {window, slots}`. REQ-012 removes the duplicate mapping and omission path. |
| `wyrd-spec/src/card/trigger.rs::TriggerActivation`, `wyrd-server/src/verification/scheduler.rs::VerificationScheduler`, `wyrd-sql/src/queries/verifier_executions.rs::{DUE_BINDING_SQL,SCHEDULE_CLAIM_DELAY,schedule_due}` | Only schedule activation exists today. The scheduler claims `wyrd.verification_bindings.next_run_at` via Postgres time and SKIP LOCKED with a 30-second schedule claim delay, then enqueues existing executions. REQ-004 extends this binding cursor to fixed-cadence Bifrost group queries, without per-call Postgres writes or group rows. |
| `vala-bifrost-redux/src/tables/verification/results.rs`, `wyrd-server/src/verification/results.rs::ResultPayloadBuilder::summary` | Results already have execution_status but the builder emits completed. Revision 9 drops the status reference; selected requirement outcomes or null suffice. No execution-status tracking extension is required. |
| `wyrd-runtime/src/outbox.rs::Outbox::stage`, `wyrd-server/src/verification/claims.rs::ClaimLoop::claim_round`, `verification/hosted/blocks.rs::STEP_CONCURRENCY`, `vala-verify/src/blocks/engine.rs::decide`, `wyrd-sql/src/queries/verifier_executions.rs::complete` | Outbox staging is unbounded today; a process bound is required work. Claims visit tenants in rounds; hosted step concurrency is four. Existing decide supports all/pass_rate and keeps known failures decisive for all; settlement fences an execution and creates dispatches transactionally, but does not serialize a whole group. Reuse these owners without claiming distributed quotas or existing group serialization. |
| `vala-verify/src/assertion.rs::evaluate_item_mode`, `skald-spec/src/media.rs::MediaFormat` | evaluate_item_mode currently returns passed when its array is empty (zero failures); REQ-008 changes that to inconclusive. JPEG/PNG already exist; MP4 is absent and remains out of scope. Existing per-file upload and per-execution queue retries suffice for J-004. Source references and their read ceiling remain extensions. |
| `mise.toml::bench:capacity`, `wyrd-testing/src/bin/capacity/{report,evidence}.rs`, `wyrd-server/src/verification/telemetry.rs`, `app/metrics.rs` | The existing opt-in capacity binary runs a four-tenant production mix over one/two release replicas, writes target/capacity reports, and belongs to no gate/verify lane. It judges direct overhead p95 < 10 ms (1,000 samples in the one-replica sustained step), >=95% offered traffic, zero errors, client queue drain <=1 s and backlogs <=60 s. CPU/memory are currently report-only and every refusal fails normal steps. Overhead subtracts measured external waits from engine intervals; report/evidence currently expose only overhead p95, using histogram bucket upper bounds. Verification buckets include 0.5 and 1 ms. REQ-015 adds p99 and overload-specific evidence without weakening the original checks. |
| `sdks/wyrd-sdk-python/src/observe/mod.rs::{PyObserveHandle::verify,submit,inputs}`, `python/wyrd/observe/__init__.pyi`, `examples/support-desk-app/python/desk.py`, `wyrd-client/src/observe/mod.rs::Observe`, `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Observe`, `wyrd-spec/src/card/verifier/validate.rs::{validate_slot,invalid}` | Python uses `(verifier, /, *value, **named)` and the example passes slots as keywords. Positional-only verifier and variadic collectors do not reserve their names as slot keywords. Rust takes verifier/Inputs; TypeScript takes verifier/inputs/options with slots in the inputs map, so these argument names do not collide. No existing reserved-slot blacklist was found: SlotName validates identifier syntax and validate_slot checks slot constraints. Reuse its field-specific WyrdError::Validation pattern (rule=slot, path=inputs.group_id) for the new group_id refusal, not a language-specific guard. |
| `architecture/bifrost-design.md`, existing evidence upload | Scribe ACK is a WAL/batch-fence boundary; the process outbox can lose unstaged/unacknowledged rows on abrupt crash. Oracle visibility can lag and overlap. Revision 8 accepts this evidence availability for grouping, deduplicates visible identities and pins each evaluated snapshot; it does not claim complete durable membership. Digest-addressed evidence storage already exists; sampling/segmentation belongs to the application. |

Paths abbreviated with `wyrd-server` and `wyrd-sql` are under
`crates/wyrd/`; `vala-verify` and `vala-bifrost-redux` are under `crates/vala/`
and `wyrd-spec` is under `crates/`.
Existing verification route/runtime and Rust/Python/TypeScript journeys are
reuse evidence, not proof of the new model. Rechecking these facts found
the owner-metadata and minimal source-reference gaps above. Attribution
is a server-side extension, not a missing client-supplied binding identity.

## Scope, invariants and non-goals

- **INV-001 — Wyrd-owned machinery.** Group identity, membership, ordering,
  end/revisions and recovery are independent of tracing, a provider, external
  queue or producer runtime. Telemetry is optional evidence. Callers provide
  the evidence the invoked Verifier declares; grouping never requires OTel.
- **INV-002 — Designed for load.** Hundreds of requests/s per deployment and
  bursty multi-tenant load are invariants. Every staging buffer, runnable
  queue, task set and source-reference read is bounded. Refuse excess work
  before staging excess work; outbox crash loss remains explicitly accepted
  under REQ-005, rather than being disguised as durable acceptance.
- **INV-003 — Server ownership and tenancy.** Rust server owners retain durable
  behavior; `wyrd-spec` stays IO/async/PyO3-free. SDKs project the shared
  `wyrd-client`. TenantConn/RLS, storage authorization, accepted authority
  and exact owner/source identities apply to grouping and every injected read.
- **INV-004 — Immutable judgments.** A group revision is a fixed set of
  source execution identities; each group execution pins the selected values
  it actually reads. Results and historical revisions remain immutable.
  Explicit agent tool queries read live data, outside the pinned snapshot.
  Media uses existing uploaded-file digests/locators.
  Recording read provenance does not inject that provenance into user slots.
- **INV-005 — Existing platform rules.** macOS/Linux on x86_64/arm64 use portable
  facilities. All internal Scribe writes use ScribeOutbox. Permission checks
  block; audit writes never block or fail the operation. Lifecycle mechanics
  are lineage, not fabricated authorization audits.

In scope: grouping ordinary verify/submit executions, explicit source references,
background group discovery/re-judgment, owner isolation, existing outbox durability,
process bounds/tenant progress, non-vacuous assert each, one source-execution
read ceiling and REQ-012's scheduled binding simplification.

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
parallel judgments of one binding/group; universal source-binding attribution;
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
queue as applicable. There is no grouping control-state table. It is never renamed to
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
sequence, revision row or readiness state to verify/submit. Both use today's
ScribeOutbox input/result path. Group membership is discovered later from
visible attributed rows. Persist original input declarations and existing
execution/subject/Verifier/principal identities, with `group_id` and
authenticated owner metadata. Existing binding columns
remain available for existing activated executions; this change does not require
populating them for every direct or submitted turn.
No revision is allocated by a producer call.

Existing enqueue idempotency remains: replay returns the same execution,
conflicting content is refused, and duplicate physical rows do not become
additional source executions. A deliberate second verify is another execution.
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
# conversation-review.yaml: selected recorded data only
metadata: {name: conversation-review, ...}
spec:
  inputs:
    turns: verifiers.turn-verifier.inputs.turn
    # traces: verifiers.turn-verifier.inputs.trace
    # verdicts: verifiers.turn-verifier.requirements.on-topic.outcome
```

At group execution, each reference returns an ordered list with one element
per execution of the resolved source binding in that group revision. Order
is ascending input row time, then `execution_id` to break ties, using
the row times REQ-004 defines rather than a Postgres acceptance counter. A
result-only execution uses its earliest visible result row time for this ordering. References
to the same binding use the same source execution order. A group snapshot containing no executions of that binding yields an empty list, not invented evidence or a vacuous pass.

For an input reference, each element is that execution's whole supplied slot
value; JSON declarations unwrap to their original JSON values. For an outcome
reference, each element is its requirement outcome from the authoritative
result. Nothing is filtered by outcome. Missing selected inputs/results are
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

### REQ-003 — Query-derived revisions, finality and retention

One binding/group has at most one outstanding group execution, including
pending, running and retrying work. It pins a visible-evidence snapshot and
coverage on the existing execution/result. While that work is outstanding,
new source rows coalesce; they do not launch a parallel review or mutate its
inputs. After settlement, the next poll evaluates uncovered rows using a new
whole-group snapshot. Prior turn checks are not rerun.

History is identified by immutable execution/result IDs and source coverage.
Any displayed revision numbering is derived from that history, not a persisted
revision field, allocation counter, replacement flag or latest-result adoption
protocol. Later settled judgments follow earlier ones serially; there are no
competing group results. Use the existing queue/settlement boundary to enforce
one outstanding execution across replicas and dispatch verdict changes under
REQ-010. No group lifecycle or revision table is introduced.

Finality is measured from the earliest visible attributed source row using
the effective Trigger duration. Rows whose server activity time is at/after
group start + finality do not reopen the group or expand subsequent snapshots.
This is a background eligibility bound, not a verify/submit refusal: calls
still run and record their rows. Remove `session_finalized` and any synchronous
finality lookup. A row visible late but timed within finality may still be
judged; a result timed beyond finality does not reopen the group.

Finality deletes nothing. Source evidence, group snapshots/results and existing
idempotency receipts follow existing evidence retention; no group-specific
retention or tenant-default finality is added. Retention and outbox loss can limit discoverable history; no separate
group-start/membership store restores unavailable rows.

### REQ-004 — Group-end Trigger lifecycle

**OD-005, decided:** add `kind: group_end` as the second `TriggerActivation`
variant alongside `schedule`. A group Verifier's binding uses this activation
to run the group judgment when the group ends. Its end-timing configuration is:

```yaml
runs_on:
  kind: group_end
  idle: 30m
  max_age: 24h
```

`idle` and `max_age` are durations. The previously decided Trigger-owned
`finality` remains part of lifecycle configuration; the example above shows
end timing only and introduces no finality default.

That Trigger owns `idle`, `max_age` and `finality`, e.g. 30 minutes, 24 hours
and 30 days. They do not belong on Verifier specs, invocation input slots or
a tenant-default finality policy. Each binding uses its own pinned Trigger's
`idle`, `max_age` and `finality`; no longest-duration composition or shared
lifecycle negotiation occurs. Those pinned settings are inspectable. Idle is
measured from the newest visible source input/result row, max_age uses the
first source row, and finality follows REQ-003.

Reuse `VerificationScheduler` and the existing binding Trigger infrastructure.
A `group_end` binding uses `wyrd.verification_bindings.next_run_at` as a fixed
poll-cadence cursor. Each claimed tick performs bounded Bifrost discovery over
the referenced source bindings' attributed input/result rows, then bounded
snapshot reads as needed, scoped to tenant, owner and `group_id`. No Postgres
group/lifecycle row exists. Discovery/snapshot reads deduplicate execution/result
identities and compare source coverage
against existing group results; the group's own rows are not source activity.

A group is due when its newest source row (input or result) is older than
`idle`, or its first source row is older than `max_age`, and no group result
covers its newest eligible source row. Apply REQ-003 finality. Row times are
each input and result row's managed `wyrd_ingested_at`, which Bifrost stamps
from its PostgreSQL-anchored clock; first/newest include both surfaces. No
group decision reads a server process clock: PostgreSQL is the only time
source, and the producer-owned `recorded_at` and `ended_at` remain result
facts, never group timing. A result is new activity: it postpones idle and, if not already
covered, can create another revision. There is no terminal-source readiness
barrier, pending-turn tracking or additional max_age wait after discovery.
At the cap, use whatever rows are visible, with missing values as REQ-002 says.

Each due group with no outstanding execution enqueues one group Verifier
execution on the existing queue; arrivals during that execution coalesce.
Reuse binding claims/cursor coordination and existing queue idempotency/fencing
across replicas; no other execution for the same binding/group is enqueued until the
outstanding execution settles, including while its result awaits visibility.
Background execution/coverage state belongs to the
existing queue/results, not a per-source or per-group lifecycle table. Failed
query/admission leaves discovery retryable on subsequent ticks; never interpret
a truncated/failed query as a complete group result.

Group-end latency is the poll interval plus Bifrost flush lag, in addition to
the configured idle/max_age and any execution backlog. The fixed cadence and
existing scheduler delay must be observable; no exact wall-clock seal or
complete-evidence guarantee is promised. Registration still rejects self/cyclic
reference dependencies. A retry is another attempt at the same snapshot.

There is no per-turn Trigger or binding activation-scope field. Turn checks
are today's direct verify/submit invocations. Ordinary bindings/inline bundles
with no group-end Trigger do not implicitly acquire revisions or require a
`group_id`. There is no explicit-close operation in this revision's
producer workflow. Scheduled Window activation remains separate and REQ-012
continues to govern its samples.

### REQ-005 — Existing outbox acceptance and evidence availability

Revision 8 supersedes OD-001's content-plus-Postgres receipt requirement and
OD-006's durable per-source readiness/status tracking. Verify/submit keep
today's hot path: declared file transfer as today, then existing input/result
rows staged through ScribeOutbox with nullable group_id and authenticated
owner columns; no binding lookup.
No per-call Postgres write, group membership commit, retained-result barrier,
counter or lifecycle write is added. Enqueue still means async verification;
its existing background queue handoff remains, rather than a new synchronous
queue commit or receipt guarantee. Window submit still records samples.

Turn rows lost from the outbox on abrupt crash are absent from group evidence,
with the same durability as today's direct verify. A successful direct result
or submit response does not establish durable discoverable group membership.
Scribe acknowledgement and eventual Bifrost visibility retain today's meaning;
no extra WAL, durable receipt domain or reconciliation store is introduced.
Flush/shutdown behavior, counted losses and non-blocking audit remain existing
owner behavior. Durable storage only protects data that actually reached it.

Group execution uses the rows visible in its discovered snapshot, not an
inventory of accepted calls. Deduplicate identities, preserve same-binding
list alignment, and use the selected authoritative queued result rather than
abandoned attempt payloads. Missing selected result values are null. Non-passing
visible data remains included; no outcome filter or implicit platform failure
is introduced. A later visible result is activity that can trigger a new
revision within finality; it never mutates an already evaluated snapshot.

Pin selected evidence and coverage on existing group executions/results so
whole-execution retries use the same inputs. This background execution guarantee does not make source outbox staging durable. Postgres
continues to coordinate existing background queue claims/leases
and serial group settlement/reactions where required. Producer calls add no
Postgres dependency; direct verify retains its existing reads.

### REQ-006 — Process bounds, existing admission and tenant progress

Bound process staging bytes/items before accepting excess source rows, plus
query output, group snapshot bytes and ordinary active execution work.
Use existing resource admission and tenant claim rounds. Refuse excess producer
work with a typed retryable pressure error and retry timing before staging;
a refused call stages no source row. Audit remains non-blocking. Source outbox
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
compute. Test a 1,000/s burst with bounded refusal. Bound discovery per binding tick and subsequent snapshot reads; query count
depends on due groups and bounded batches, not an exact statements-per-tick
contract. Bound query work/output and defer further groups on pressure; never judge a
truncated group or materialize all group payloads in API memory.

Reuse existing tenant claim rounds so a runnable tenant B progresses during
A's burst when resources/dependencies permit. Test that progress, not exact
shares or latency guarantees. Adding WYRD_TARGET replicas adds their local
capacity; it does not create cluster-wide quotas. Existing Postgres claims,
leases and fencing recover queued work and serialize binding/group execution;
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

### REQ-009 — Source-execution read ceiling

One deployment-set ceiling bounds how many distinct source executions each
verifiers reference may read per group snapshot. Count eligible source
execution IDs after owner/binding eligibility and identity deduplication, across
visible inputs/results, not physical rows or retries. References to inputs and
requirements of the same source Verifier see the same execution membership and
alignment. The ceiling has no authored Verifier/binding override or dispatch,
item, byte or derived-output budget system.

Exactly the configured number is permitted. Exceeding it yields a completed
group execution with verdict inconclusive and reason
`source_execution_limit_exceeded` exposed in the affected requirement reasons
and result summary, naming the reference and ceiling. Do not evaluate a truncated list or infer a subject failure from the ceiling.
Record the result and source-query bounds for inspection; never eagerly read
unbounded payloads merely to detect overflow. This is a result reason, not a
producer refusal: ordinary source verify/submit still follows existing admission.
Group source-row coverage for background re-judgment remains REQ-003–005; no
fan-out coverage counts or policy are introduced.

### REQ-010 — Group verdict changes and Operators

Keep `on_verdict_change` for J-002's passed→failed refund reaction and recovery.
With one outstanding execution per binding/group, compare each settled result
with its preceding settled result. No concurrent revision adoption is needed.
Immutable execution/result IDs and coverage identify history. An unchanged
verdict causes no repeated group reaction; the first result is the baseline.

Group bindings use `on_verdict_change` alone, reacting between settled verdicts
including changes to/from inconclusive and failed→passed. The first result,
even failed, establishes the baseline and dispatches nothing. Registration
refuses `on_failure` on a group_end binding at that field; use existing contract
error families. No combined-list or cross-list deduplication rule is added.
Ordinary bindings retain their existing `on_failure` behavior.

Extend existing transactional execution settlement/dispatch creation to retain
the preceding settled group result and unique verdict-change dispatch intent. Settlement replay creates no second logical dispatch; delivery
remains at least once through existing Operators. Context contains bounded
group_id, prior/current result IDs,
old/new verdicts and summary—not raw inputs/traces/media or secrets.
Operator delivery does not rewrite the result.

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
context, pinned Trigger settings, execution/result IDs, source coverage and
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
Finality is background eligibility, not an invocation error.
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
flush source rows before discovery in basic flows, and use the visibility/cap
variants below for absent results. Binding durations and cursors, never a process
clock, simulate elapsed days; tests do not sleep for days or use producer
timestamps to advance server time.

### AC-001 / J-001 — Resumable e-commerce conversation

**Maps:** REQ-001–006, REQ-010–011, REQ-013–014; INV-001–005.

Actors: Python chat backend, TypeScript human support console, Rust order
service; eligible calls authenticate through the Card-bound support Service
and invoke `turn-verifier` on its same component. The console/order adapters
use that Service authority; an unbound principal is also exercised below.
A `conversation-review` on that occurrence declares
`turns: verifiers.turn-verifier.inputs.turn`. Its group-end Trigger has
idle=30m, max_age=24h, finality=30d. All grouped calls use `group_id="order-123/chat-7"`.
Optional trace slots may be supplied using today's exports, but the group
receives no trace IDs unless it declares a trace-slot source. It never
receives spans automatically.

1. Python verifies a JSON turn asking for delivery help with `group_id="order-123/chat-7"`, receiving
   its immediate passed result. Rust submits an order-status turn and polls
   its queued execution to completed/passed. Both inputs/results and
   authenticated owner, subject and Verifier identities are retained; visible
   inputs use recorded-time/execution-ID order.
2. The shopper leaves and returns within idle. Python verifies the resumed
   request; TypeScript verifies human handoff and acceptance; Rust submits
   replacement-confirmed. No producer discovers a Wyrd resource ID first.
3. After the newest source input/result row's idle deadline and a poll, the
   Trigger queues one group judgment, displayed as revision 1. The group receives exactly six
   turn values in recorded-input order: delivery request, order status, resumed
   request, human handoff, customer acceptance and replacement confirmation.
   Its deterministic review requires confirmed replacement and resolution.
   Polling returns completed/passed and the exact selected ordered inputs.
4. SDK and MCP inspection through another process returns the same `group_id`,
   effective lifecycle, source execution identities and retained result.

Edges/negatives: concurrent first invocations on two replicas are discovered as
one visible group; no producer revision row is written. Replay the same enqueue request/key: original execution,
no extra list element or idle refresh; conflicting replay is refused.
During the open group, an unbound principal with `verifier:run` uses existing
HTTP execute with the same Verifier UID, subject and caller `group_id`. It returns
completed/passed and persists input/result with null owner; the group
review still receives only the six bound values. A new verify call is a new
execution. Deny unauthorized invocation or payload
read without changing membership. Use the same Verifier UID and caller `group_id`
through another owner Service: its turn never appears. A second group binding
with idle=15m, max_age=48h, finality=60d uses those settings independently; it
does not change the first binding's 30m/24h/30d settings. New accepted verification after end
and before finality is discovered as revision 2; rows timed after finality
remain recorded but do not reopen the group or delete old records. No explicit-close/append
flow is used.

Registration negative (REQ-001): through Rust, Python and TypeScript Card
registration plus direct HTTP/MCP registration, submit an otherwise valid
Verifier declaring `spec.inputs.group_id` as a JSON slot. Expect the existing
validation-family error with rule `slot` and spec-relative path
`inputs.group_id`; no Card/binding persists and no execution is activated.
Repeat for a group Verifier whose `group_id` input is a valid source-reference
string: it is refused at the same field. A local authoring refusal may precede
the request, but direct HTTP proves server enforcement. Rename the slot to
`turn` and register successfully; Python verify/submit with `turn=...` and
`group_id="order-123/chat-7"` records the turn and grouping metadata separately.
Keep otherwise-valid slots such as `verifier` and `value` usable as Python
keywords after the positional Verifier name, and as named inputs in Rust/TS;
this change adds no wider reserved-name list.

Persist: ordinary inputs/results, authenticated owner and existing caller/subject/
Verifier context, visible source coverage, pinned inputs and immutable result IDs,
and
ordinary authorization audit decisions.

### AC-002 / J-002 — Refund failure three days later

**Maps:** REQ-001–006, REQ-010–011, REQ-013; INV-001–005.

Actors: Python support backend, Rust payment service, TypeScript returns
console, all invoking `refund-turn` bound on one refunds Service occurrence.
`refund-review` declares `facts: verifiers.refund-turn.inputs.fact` and,
when checking turn judgments, explicitly declares
`checks: verifiers.refund-turn.requirements.valid-fact.outcome`.
Verify/submit use `group_id="order-456/refund"`; group-end Trigger idle=30m, max_age=24h,
finality=90d. The review passes an accepted refund promise with no overdue
fact; authoritative overdue/returned-but-unrefunded facts fail it.

1. Day 0: verify the refund promise and submit the accepted order fact with
   `group_id="order-456/refund"`; await both source results completed/passed. Idle ends revision 1.
   Group result R1 is completed/passed and establishes the baseline, without
   a verdict-change dispatch.
2. Day 3: payment service submits `refund_overdue` with `group_id="order-456/refund"` and waits
   for its valid-fact check. Its newly visible rows make revision 2 eligible. A burst
   of payment and return rows before the next due snapshot coalesces.
3. A background tick finds uncovered rows; group max_age has already elapsed
   on day 3, so it need not wait another idle period. The group receives all
   prior and new selected facts,
   not only the delta, and returns completed/failed as R2. R1 remains passed,
   immutable history; R2 is the next settled result. One `passed→failed` Operator dispatch is
   persisted and eventually delivered.
4. Another unchanged failed group revision produces no repeat reaction;
   corrected paid-in-full facts can yield failed→passed and a recovery
   dispatch. Settlement replay creates no extra logical dispatch.

Edges: hold the first group execution queued/running while payment/return
rows arrive. Subsequent ticks create no second outstanding execution for that
binding/group. The first completes with its pinned values; after settlement,
the next poll coalesces uncovered rows into one next whole-group snapshot.
It may dispatch a verdict change relative to the preceding settled result.
No persisted revision numbers, replacement flags or concurrent result adoption
are involved. A late refund result is new activity within finality. At finality,
verify/submit still works, but rows timed beyond the bound never reopen the
group. Retain both result IDs and source rows under existing retention.

### AC-003 / J-003 — Coding-agent work without tracing

**Maps:** REQ-001–006, REQ-008–011, REQ-013; INV-001–005.

Actors: Rust coding-agent bridge, Python test runner, TypeScript editor;
same coding Service component, verify/submit use `group_id="issue-812/attempt-1"`. They invoke
`coding-turn` with a JSON `turn` on every call and any declared file evidence, never an
append operation. Its checks include no secrets written and no forbidden
command. `coding-review` declares
`turns: verifiers.coding-turn.inputs.turn` and
`safety: verifiers.coding-turn.requirements.no-secrets.outcome`.
Trigger: idle=30m, max_age=24h, finality=30d.

1. Verify the task prompt and tool-call results; submit edit/test/final-diff
   JSON evidence through the turn Verifier, uploading any declared file slots
   within those calls. The producer has no OTel provider or trace IDs.
2. Every source verify returns its result now; each submit is polled to its
   own result. A secret/forbidden-command fixture yields failed turn judgment
   immediately, rather than waiting for group end. Wyrd does not run the
   producer's commands.
3. With complete sources, idle ends the group. The review judges task,
   edits, test exit code/report and final diff and returns completed/passed
   for the clean fixture. Tests exit 1 or a recorded forbidden command yield
   failed; missing required task evidence follows existing inconclusive rules.
4. In the optional telemetry variant, another producer exports spans as today
   and coding-turn supplies its declared trace slot. The review declares
   `trace_ids: verifiers.coding-turn.inputs.trace` and an Agent check using
   `bifrost.query` to select bounded spans from `vala.traces.spans` by those
   IDs. The accepted caller holds the declared tool-invoke permission and
   `bifrost:query:read` plus existing table/payload access. For the fixture,
   spans record a successful test tool call: the local deterministic agent
   queries them and returns passed. The review snapshot contains only the
   recorded trace IDs, never span documents. A variant without trace_ids or
   the span-reading check performs no query and the non-trace journey passes.

   Prove live-read semantics: pin group inputs before a later span is flushed,
   then let the agent query after it becomes visible. It can read that span;
   the pinned input list remains the same IDs. Deny the tool-invoke or
   bifrost:query:read grant in separate variants: the existing tool query is
   refused with its ordinary structured authorization error, with no automatic
   span-fetch fallback. These are existing query failures, not new group errors.

Edges: an accepted late test verification opens a new revision and re-runs
only the group review, not previous turn checks. Replayed enqueue adds no
source execution. Oversized tool evidence and unauthorized trace reads are
typed refusals/failures, not passes. Raw producer attribution is not added to
the selected input arrays. In an absent-result variant, the review declares
`checks: verifiers.coding-turn.requirements.tests-pass.outcome`. Flush the test
turn input while its queued execution has no visible result. Before idle, no
group is due; a visible result resets idle. At max_age, a snapshot with no
result includes its input and a null tests-pass outcome. A deterministic Code
check requiring a passed tests-pass outcome returns completed/failed. Flush its eventual
completed/failed result within finality: new activity produces a next revision
with the actual `failed` requirement value, without filtering the failed test
or editing history.

Persist: source inputs and requirement results, original file digests,
group input selections, provenance and immutable execution/result history. The
producer's application runtime and tracing remain optional machinery.

### AC-004 / J-004 — Application-sampled person-detection media

**Maps:** REQ-001–006, REQ-008–011, REQ-013–014; INV-001–005.

Actors: a user's person-detection application with Rust, Python and TypeScript
producer adapters, and an HTTP/MCP review agent, under one authenticated owner
Service. The application detects event `door-42` and itself samples 20 JPEG/PNG
frames. Wyrd does not detect the event, sample frames or accept a video.

Register `frame-verifier` with one media slot `frame` accepting jpeg/png and
Agent requirement `person-wearing-badge`; deterministic local provider responses
judge each file. Bind `event-review` on the same Service occurrence with:

- `outcomes: verifiers.frame-verifier.requirements.person-wearing-badge.outcome`;
- one Assert `each` selecting `inputs.outcomes`, comparing each value to `passed`;
- `decide=all`, group_end idle=2m, max_age=30m, and a pinned finality=30d.

Also bind an ordinary JSON `event-fact` Verifier and declare
`event: verifiers.event-fact.inputs.event` on the review. The application's
ordinary verify of `{event_id: "door-42", detected: true}` with
`group_id="door-42"` supplies a discovery row, including in the zero-frame
variant. It is not a Wyrd create/upload verb or implicit evidence injection.
A group with no source rows at all cannot be discovered; do not fabricate one.

1. Through each SDK, submit the 20 files individually to frame-verifier using
   `group_id="door-42"` and existing per-file upload. Retain 20 execution IDs;
   no list-of-media slot or internal fan-out call is used. The existing queue
   supplies parallelism and retry. Await completed/passed frame results and
   drain/flush them into Bifrost for the positive fixture.
2. After the newest input/result row is older than 2m, a binding tick discovers
   the group (or max_age=30m makes it due). It queues one event-review with 20
   ordered `passed` values. Poll/inspect through SDK and HTTP/MCP: the group
   execution is completed/passed; its pinned inputs contain exactly those 20
   outcomes and the declared event fact, with matching source execution IDs.
3. Kill a worker while one frame execution is running, with the other 19
   already completed. Existing lease/retry recovers only that execution using
   its original uploaded file. No successful frame is re-enqueued, no source
   identity duplicates, and the whole frame execution may repeat. Once its
   result is visible, idle restarts and the group can judge the complete set.
   No step extraction or checkpoint resume guarantee exists.

Negatives/edges, repeated across Rust/Python/TypeScript and agent HTTP/MCP:

- One of 20 frame outcomes is failed: the group assertion and verdict are
  failed. Failed frames remain in the selected list.
- Submit only the event fact and zero frames: the frame-outcome reference is
  `[]`; the group is completed/inconclusive with reason `empty_selection`.
- Another Service submits the same frame Verifier UID and group_id: its rows
  never enter this group's outcomes. Denied invocation/upload/read is refused
  under existing permissions, and rejected calls add no evidence.
- Configure the source-reference ceiling to 20. Twenty frames pass the limit;
  21 eligible distinct frame executions yield completed/inconclusive with
  reason `source_execution_limit_exceeded`. No first-20 judgment is reported.
  Duplicated physical rows or enqueue replay of one of 20 do not exceed it.
  The event reference reads one execution independently, not a combined 21.
- A result absent at max_age stays a null outcome position under REQ-002;
  retain existing comparison semantics rather than inventing frame lifecycle
  status or dropping that execution. A later visible result can trigger the
  next serial group review within finality.

Persist ordinary per-frame inputs/results, file digests, authenticated owner
and source identities, queue retry state, and immutable group inputs/result
coverage. No frame counters, checkpoints, derived-media catalog, new media
formats, dispatch budget or fan-out coverage fields are required.

### AC-005 / J-005 — Retail chatbot peak load and existing Drift

**Maps:** REQ-001–006, REQ-008–013; INV-001–005.

Actors: Python chatbot backends, TypeScript chat consoles and Rust order
systems in tenants A/B. Each turn invokes `turn-verifier` through verify or
submit with its conversation/order `group_id` (e.g. `group_id="conversation-123"`); the same-occurrence group review
declares `turns: verifiers.turn-verifier.inputs.turn` and optionally selected
requirement outcomes. Group Trigger idle=30m, max_age=24h, finality=30d.
A distinct existing Window Verifier receives `resolution_score` samples
through today's Window path and runs on a schedule; REQ-012 removes only
its redundant binding mapping.

1. Drive many conversations and source executions through all three SDKs.
   Complete the sources; observe one group review per ended revision with
   exact selected arrays while other heavy async executions take minutes.
2. Fill one server process's finite staging bound using A's burst. A new invocation gets retryable
   typed pressure with timing before staging source rows.
   Retry the same enqueue identity after drain: one acceptance/array element.
3. Keep accepted B work runnable through A's burst. Using existing resource
   admission and tenant claim rounds, observe B complete while A is still
   active and process memory/work stays bounded. Add a replica: each process
   retains its own bounds; assert no cluster-wide share or quota. Kill a
   worker/API replica: queued work recovers, while outbox-only loss is allowed.

4. Feed resolution scores 0.95, later 0.60, to the existing scheduled Custom
   metric check below 0.8. Read a failed Drift result with its subject/window.
   Cross-conversation regression stays Window/Drift, not a streaming group
   Verifier or an automatic new source reference.

Edges: deny unscoped MCP invocation, cross-tenant `group_id`/payload reads and
ambiguous binding lookup. More idle ticks/restarts create no duplicate group
review. Refused calls stage no source rows. A source-reference ceiling breach
returns inconclusive with its explicit reason; empty assertion selection is
inconclusive. Insufficient Window samples preserve existing behavior.

Capacity qualification offers 300 4-KiB source invocations/s and a 1,000/s
ten-second burst, with multiple tenants/`group_id` values and cheap turn versus slow
group/media checks. Record hardware/topology, configured finite bounds,
accepted/refused counts, queue bytes/age, memory, latency and B's progress.
Above provisioned capacity require bounded refusal; do not promise arbitrary
acceptance on arbitrary hardware. No synthetic host load is needed.

### AC-006 — Durability, reference isolation and compatibility

**Maps:** REQ-001–006, REQ-008–011, REQ-013–014; INV-001–005.

Register source Verifiers by local path, same-request sibling and registered
CardRef (including a Card registered elsewhere), each bound on the group's
owner occurrence. All valid forms resolve the actual metadata.name and exact
pinned version, not declaration location. Zero and multiple same-name matches,
missing slots/requirements, malformed roots/unsupported source suffixes and
dependency cycles are field-specific registration refusals with no invalid binding persisted.

Through real verify/submit, populate the same source Verifier UID under
another owner Service, a non-matching subject/Verifier tuple and another
`group_id`. Assert none enters the selected lists. Record owner and existing
identities without source binding resolution; the background query matches
them to its exact pinned bindings. Re-register a newer source Card: old pinned
lookup does not silently select the newer Verifier version.

In Rust, Python and TypeScript, exercise both verify and submit with optional
`group_id`; the HTTP/MCP request, persisted `group_id` column and inspection
response preserve the same value. Omit the argument/field in an ungrouped
variant: invocation still works without creating group membership. Neither
SDKs nor wire/storage projections use an alternate name for `group_id`.

Use executions with reversed completion order to prove recorded-input ordering
and parallel whole-slot/outcome-list alignment. Checks navigate nested JSON via
existing local fields/indexes and Assert each wildcard selectors after resolving
the whole source list. Refuse source declarations ending in `.field`, `[0]`,
`[*]` or `.reason`; no source-path/schema projection validation is required.
Only declared slots appear. A trace-slot reference returns recorded trace IDs;
creating/pinning that source list performs no span query. J-003 tests explicit
live Agent tool queries with existing permissions. Authorize recorded sensitive
inputs/results through SDK and HTTP/MCP. Cover REQ-005's visibility/cap behavior.

Through each SDK and HTTP/MCP, invoke grouped verify/submit with visible
source inputs and completed/passed, completed/failed or absent results. Prove
no additional producer Postgres read or write for grouping/attribution, and no
lifecycle, revision, counter or readiness state. Inspect
the binding next_run_at cadence; discovery and subsequent snapshot reads
respect configured resource bounds, without asserting a statement count. Before
idle/max_age no execution is due;
a result row arriving before idle postpones it. At idle or max_age, enqueue
one snapshot using only visible rows. Parallel input/outcome lists retain
every eligible visible source identity; absent results give null outcomes,
completed failed results give failed outcomes. The deterministic all-tests-pass
fixture fails over missing/failed values; the platform does not invent a verdict.

Flush a result after a group result within finality: it is uncovered activity,
produces the next whole-group snapshot and preserves prior history. Burst rows
before the next snapshot coalesce. Rows timed after finality are retained but
do not trigger a revision. Polling an already covered group is a no-op.
Race scheduler replicas and crash/retry around background enqueue: the extended queue/settlement coordination permits only one outstanding execution
per binding/group, including while its result is awaiting visibility. Arrivals
coalesce and are judged only after the preceding execution settles; another
binding/group remains independently runnable. No revision/lifecycle table
or pending-source inventory exists. Ownerless/non-matching rows are excluded.

Exercise Bifrost lag, repeated/overlapping physical rows and failed/truncated
queries: identity deduplication prevents duplicate list entries; lag delays or
changes a later discovered snapshot, not a false completeness claim. Retry
failed discovery without partial judgment. Kill the API before outbox drain:
a staged turn row may be absent permanently; its returned direct result does
not require recovery into the group. Flush/drain in the positive variant and
prove discovery/readback. Do not assert node-loss or content+Postgres receipt
durability. Existing whole-execution retries reuse pinned group inputs;
process bounds and tenant progress remain J-005 obligations.

Drive an ordinary inline JSON Verifier and a step requirement whose Assert
each selects `[]`: both requirement outcomes are inconclusive/empty_selection,
with no vacuous pass; the single-requirement decide=all result is inconclusive.
Repeat with nonempty all-passing and one-failing arrays to retain comparison
semantics. J-004 proves empty group-source lists and the ceiling end to end.
For both source forms (whole input slot and requirement outcome),
exercise exactly the deployment ceiling and ceiling+1 unique eligible executions;
the latter is inconclusive/source_execution_limit_exceeded without truncated
values. Duplicate rows/retries and excluded owners do not consume extra entries.

For group_end registration, refuse any on_failure field with a field-specific
error; valid on_verdict_change registration succeeds. An initially failed group
result dispatches nothing and establishes the baseline; a subsequent changed
verdict dispatches once through existing Operator delivery, while an unchanged
verdict dispatches nothing. Ordinary on_failure fixtures remain unchanged.

Preserve ordinary inline/document/trace/media and non-group Operator behavior,
including direct analysis with no dispatch. Retain Window/Drift behavior with
REQ-012 authoring changes. SDKs/served OpenAPI/MCP catalogs/generated types/
Python stubs and fixtures agree. No append/close or scope API survives.
Support journeys with pure reference/order/empty-assert/ceiling tests and real-Postgres
race/lease tests; do not substitute these for real client→server→client proof.

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

**Maps:** REQ-015, REQ-001–006, REQ-008–010, REQ-013–014; INV-001–005.

Run the extended bench:capacity through the existing real public Rust-client
capacity driver/server deployment; SDK functional parity stays in J-001–005,
not three new performance harnesses. Store case verdicts plus raw measurement
inputs in its existing Markdown/JSON report artifacts. Record revision/build,
hardware, replica/tenant counts, workload/payload sizes, warmup/window/sample
counts, process bounds, group/source ceiling, Trigger/poll settings, achieved
traffic, errors, memory/CPU and drain times so runs can be compared. Group
fixtures use short declared idle/max_age durations to finish within the
existing command lifetime; no extra long-running harness or host stress tool.

| Case | Report | Pass/fail |
|---|---|---|
| 1. Baseline direct verify, assertion-only, no group_id | Overhead p95/p99, sample/bucket evidence and existing request/client timings at the matched load. | Correct judgments, normal-load SLOs and p99 < 1 ms. A current-baseline miss is explicitly marked pre-existing work in scope, not excluded from the verdict. |
| 2. Same direct verify with group_id and authenticated owner metadata | The same metrics, grouped-minus-baseline overhead delta, repeated-baseline noise envelope, and evidence of recorded metadata. | Both overhead SLOs hold and the positive delta does not exceed the disclosed noise envelope. No additional producer Postgres IO; insufficient measurement resolution cannot pass as zero delta. |
| 3. Grouped submit at rising load | Offered/accepted/completed executions/s, normal capacity knee, queue depth/age, throughput plateau, refusals by code, client latency and all backlog drain times. | Identify the throughput ceiling; below it retain normal-load SLOs. Saturation is reported as the limit, not hidden as acceptable loss. After stopping load, existing backlogs drain within 60 s; normal-load unexpected refusal/loss/wrong judgment fails. |
| 4. Many groups × N source executions and group_end | Explicit group/N sweep including the configured source ceiling and ceiling+1; idle-expiry-to-result p50/p95/p99; Bifrost discovery/snapshot query count, latency and rows/bytes read, plus group queue/drain evidence. | Every eligible fixture gets its correct whole-group result, serially per binding/group, without truncated inputs or unbounded reads. Ceiling+1 yields inconclusive/source_execution_limit_exceeded. Ordinary due-group results complete within the existing 60-second drain allowance after their configured due point; report latency separately, without inventing a sub-ms group-latency SLO. |
| 5. Burst above the measured capacity | Per-process staged bytes/items against bounds, memory time series/peak, accepted/refused counts and typed retryability/retry timing, drain time and per-tenant completion timeline. | Staging stays within finite process bounds; warmed memory plateaus while overload continues, stays inside the declared process envelope and returns toward baseline after drain. Excess requests receive typed retryable refusal before staging; no unexpected loss/errors. An admitted tenant B completes work during A's continuing burst using existing claim rounds. Existing backlogs drain within 60 s after stopping the burst. No strict distributed fairness/quota claim. |

Use J-005's 300/s qualification and 1,000/s ten-second burst as offered points
alongside the measured knee; bounded refusal is valid above provisioned capacity.
For the memory case, repeated equal bursts must not increase retained staging
or post-drain memory in proportion to offered requests; disclose baseline
variation and envelope, rather than claiming byte-identical RSS. CPU/memory
remain reported-only for the old normal steps; the new overload case expressly
judges their boundedness. Deliberate ceiling/incomplete-evidence results are
correct judgments, not failed execution errors. Expected overload refusals are
counted and judged only in the overload case; existing normal-load zero-error
rules and client queue drain <=1 s remain unchanged. Produce an overall extension
PASS only with all five case criteria satisfied, alongside the unchanged legacy
capacity verdict. No benchmark result is claimed merely from specification edits.

## Required boundaries and architecture impacts

Extend existing owners: `wyrd-spec` reference/Verifier/wire contracts;
`wyrd-client` invocation context; `wyrd-server` registration, background
group discovery/activation and runner orchestration; `wyrd-sql` control state;
`wyrd-storage` durable evidence; `vala-verify` closed-input evaluation;
Scribe/Bifrost recorded inputs/results; existing Operator delivery. Do not
replace complete workflows with a second queue or group evidence pipeline.
Private helpers, encoding, indexes and batching mechanics remain implementation
choices within these owners.

| Owning authority | Required change |
|---|---|
| Wyrd design/doctrine Verifier inputs and reference grammar | Add whole-slot and requirement-outcome source references only; checks reuse existing Reference selectors. Preserve exact-version background resolution and trace IDs without automatic span expansion. No source child paths, reason/status roots or implicit inputs. |
| Invocation and verification record contracts | Optional group_id and authenticated owner_card_uid metadata on existing rows. Copy owner identity without IO; keep existing subject/Verifier/run/principal IDs. No additional producer Postgres read/write, per-call binding resolution or universal binding propagation. Reserve group_id in Verifier input declarations using existing field-specific slot validation; invocation group_id remains metadata. |
| Group lookup ambiguity | Validate only bindings participating in group reference lookup. Match owner/subject/Verifier tuples in the background; refuse ambiguity there without a global tuple-uniqueness rule. |
| Verifier execution durability/operations | REQ-005 preserves existing ScribeOutbox staging and crash-loss semantics; no per-call Postgres write or receipt barrier. Keep bounded admission and background execution recovery. Audit remains non-blocking through one outbox. |
| Trigger/binding contract | Add group_end and poll binding cursors for bounded discovery followed by bounded snapshot reads. Each uses its own pinned idle/max_age/finality. One outstanding execution per binding/group coalesces arrivals through existing queue/settlement extension; no revision table or adoption protocol. |
| Scheduled samples contract | Keep REQ-012: no schedule-binding inputs, samples cannot be optional, every drift requirement is judged; update claim-time authority, fixtures, schemas and docs with no alias. |
| Operator contract | Group bindings accept on_verdict_change only; first result is baseline. Extend existing settlement/dispatch creation over serial group judgments. Ordinary bindings retain on_failure; no cross-list group deduplication rule. |
| Evaluation contract | Fix empty assertion each to inconclusive everywhere and cap source executions read per reference with an explicit inconclusive reason. Caller fan-out uses existing verify/submit, media formats and queue retries; no checkpoint, step-action fan-out, video-processing or budget contract. |
| Retention | Existing evidence retention only; finality deletes nothing. No new privacy-erasure policy. |
| Capacity benchmark | Extend the existing capacity binary, report and overhead metric with REQ-015/AC-009 cases. Retain p95 <10 ms, add assertion-only direct p99 <1 ms and grouped delta/noise evidence. Opt-in bench:capacity stays outside every gate/verify lane; no parallel harness. |

Grouping deliberately uses available Bifrost evidence, with poll/flush lag and
possible source outbox loss. Selected snapshot values are closed and reproducible;
they do not prove every accepted turn was retained. Agent span queries are live
and their returned spans are not frozen by that snapshot. Background queue and
result coverage coordinate retries, not a new hot-path membership inventory.

## Open material decisions

None remain. The owner resolved OD-007: fan-out is N ordinary verify/submit
calls sharing group_id, with application-chosen evidence and existing per-item
queue parallelism/retry, aggregated through references at group_end. Internal
fan-out, checkpoints, server video processing and fan-out budgets are removed.

Revision 14 is a **draft** awaiting owner approval.

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
