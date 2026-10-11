---
id: SPEC-bifrost-variant-main-integration
revision: 1
status: approved
task_count: 1
---

# Integrate the Bifrost variant with current main

## Approval, objective, and value

This revision transcribes the user's approved integration scope, not a new
architecture proposal. On 2026-10-11 the user requested: “write this as 1
integration task in the new branch (wyrd/bifrost-variant/integrate-main). add 1
spec and 1 integration task,” explicitly requiring all 27 researched decisions
and their evidence. This request adopts the preceding recommendation with the
two explicit corrections below. It authorizes this packet and the described
integration outcome; it does not authorize deployment, deletion of retained
data, merging into main, pushing, or changes outside this contract.

Produce one working integration of the rewritten Bifrost with main's current
Wyrd product, security, verification, client, and tooling behavior. Users must
be able to emit, verify, retain, and query attributable evidence through the
real first-class clients and agent surfaces without losing main's fixes.

There is exactly one implementation outcome and one task. The breadth is
intentional: an engine that compiles but loses its surrounding consumers is
not independently deliverable. Implementation order within that task is not
permission to omit a later consumer or split delivery into partial outcomes.

## Immutable inputs and integration authority

| Input | Pinned commit |
|---|---|
| Main / integration base | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6` |
| Variant source, `origin/wyrd/bifrost-variant/TASK-003` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05` |
| Common ancestor | `5ab92b0039404565e51f4a246863ce90cbd8d11f` |
| Integration branch | `wyrd/bifrost-variant/integrate-main` |

The integration branch starts from pinned main in a separate worktree. The
implementation integrates the pinned variant while preserving provenance;
this packet alone does not perform that merge. Refreshing either input requires
recording and reviewing the new delta rather than silently widening scope.

Authority is explicit:

1. Current user instructions, including the corrections below.
2. Main's Wyrd doctrine, ownership, tenant/security, audit, and contributor rules,
   except the Bifrost decisions explicitly replaced by this approved change.
3. The variant's Bifrost rewrite and approved source contract, composed with the
   compatible main repairs and retained surrounding capabilities enumerated here.
4. This revision's exact resolutions of cross-boundary conflicts.
5. Tests and historical research, which are evidence and cannot redefine scope.

The user clarified: “whatever is in bifrost-variant wins (except for things weve
discussed (heartbeat, poller, etc)).” Therefore source snapshot retention and
deadline clamping win. Do not restore reader pins, uncapped main deadlines, or
the researcher's withdrawn proposal to reject oversized deadlines instead.

The user subsequently instructed: “no get rid of vala.dev.agent_traces for now.”
Keep that table removed even though the source specification includes it.
Its sole-owner CodeAxis behavior is excluded. The retained built-in set is ten.

An unchanged source/base fragment is not an intentional removal of a newer
main capability. Research confirmed this distinction for query parameters,
Workflow files, identity migrations, and provisioning. Preserve those main
capabilities at the source-engine seam; do not replace Bifrost's new owners.

Update the owning architecture and public documentation in the same integration
to remove contradictory old reader-pin, event-time, audit-publication, terminal,
schema, and shutdown descriptions. Preserve main's unrelated protocol prose.

## Required behavior — all 27 decisions

Each REQ corresponds one-to-one to the numbered recommendation accepted by the
user. [Evidence](evidence.md) identifies immutable sources, symbols, history,
direct conflicts, and silent seams. Private helper/file structure is not fixed.

### REQ-001 — One source-authoritative analytical engine

Integrate the source native scans, Variant/shredding, declared timestamp types,
ObjectStore storage, Iceberg v3 publication, cut cache, Scribe recovery, and
distributed executor together. Do not revive replaced analytical scan, pruning,
fixed-IPC, payload-cache, or duplicate storage owners to satisfy old imports.
Rehome compatible newer main behavior in the actual source owner.

### REQ-002 — Source snapshot retention and deadlines

Retain source replacement-time retention, supersession roots, read-cut function,
configured default/max query lifetime, and request deadline clamp. Remove the
superseded per-query SQL reader-pin workflow. Do not introduce main's unbounded
deadline policy or an alternative over-limit rejection policy. Propagate the
source behavior consistently through server and public clients/documentation.

### REQ-003 — Source read-cut cache

Retain tenant-qualified combined snapshot/hot-source cuts, transactional
invalidation, listener liveness, and invalidation-raced fill protection. Normal
watcher cancellation disables and clears caching so queries use Postgres;
membership's extended drain lifetime is not imposed on the cache watcher.

### REQ-004 — Source Scribe durability and recovery

Keep group-fsynced acknowledgements, compressed IPC, durable batch fences,
fenced final-record corruption handling, and publication-aware recovery. No ACK
precedes its durable authority, replay does not duplicate writes, corruption
covered by an acknowledged fence fails closed, and published recovery does not
reupload objects. Wire, WAL, ingress, SQL, and crash proof must agree.

### REQ-005 — Flush includes concurrently owned rows

Port main's flush snapshot ownership repair into source staging/persistence.
Flush cannot complete while another publisher owns a member in its promised
snapshot. Preserve source publication/recovery behavior. Fault controls and
backlog inspections remain table-scoped so audit traffic cannot consume another
table's injected fault or invalidate its proof.

### REQ-006 — Compatible Forge durability and lifetime repairs

Retain main's lease-bounded destructive-effect handling, prepared/submitted
distinction, uncertain-outcome reconciliation, leader-term linearization,
authority revocation on scheduler exit, and same-owner reclaim only after proven
prior-loop quiescence. Retain main's bounded catalog calls and definite-conflict
publication retry policy, including unchanged operation/output identity on
retry. Compose these with source retention and native v3 rewrites; do not
restore old active-reader SQL or publication wall deadlines.
Source cuts remain non-blocking while expiry holds maintenance authority,
including prepared expiry. Do not port main's barriers/refusals for new cuts;
retain its durable uncertain-operation evidence and lease bounds instead.

### REQ-007 — Oracle-owned memory settles correctly

Adapt main's shutdown fixes to source governor/executor ownership. Oracle drain
waits for Oracle governed memory and infallible headroom, including late child
ownership, without waiting for independent Forge reservations. A query slot
can settle while a legitimate child view returns its bytes; successful release
never masks poisoned or abandoned ownership. Keep the source resource model.

### REQ-008 — Role heartbeat and one membership poller

Retain main's final role-owned ClusterTask lifetime and one process membership
poller. Readiness withdrawal never re-advertises readiness, but heartbeat and
discovery continue while outbox/role drain needs them. Teardown joins/stops
them; failed construction and abort terminate their tasks. Do not restore
source's premature process-token cancellation or per-role poller duplication.

### REQ-009 — Scribe capability and routing independent of Oracle

Preserve Scribe-only fragment execution, role-specific readiness/fences,
tenant/body authority, and source assignment shape. Internal writes choose
local or ready remote Scribes from boot-resolved TLS and membership without
requiring a local/ready Oracle. Keep configured decode ceilings, per-peer
deadlines, retry rotation, and stale endpoint eviction. Do not invent a second
peer protocol solely to match remembered ScribePeer/OraclePeer names.

### REQ-010 — One shared non-blocking Scribe outbox

Retain main's process outbox for authorization audit, gateway captures, and
Verifier results. Use destination-grouped bounded Arrow frames and
content-derived batch IDs; retry identical slices, consume/count terminal
rejections, and count observable shutdown loss. Permission completes before
operation/refusal and audit delivery never delays or fails it. No separate
publisher, SQL staging, relay, audit WAL, or public AuditUnavailable error.

### REQ-011 — Combined audit content and credential attribution

Retain main's 11 decision-content fields and direct AuditEvent projection; add
explicit decision `created_at` and source Variant `detail` (12 content fields
before the physical envelope). Preserve nullable credential attribution through
actual Gate/Oracle event builders, not just prepopulated events. No seq/hash
chain columns or sequence ordering. Preserve trusted system-owner rules and
audit Card identity content; `audit_card_ref` is not observation correlation.

### REQ-012 — Six-destination trusted internal writes

Retain main's private mTLS audit/two-capture/three-result write path. Only result
destinations carry exact Verifier attribution; the reserved system owner is
admitted only for audit. Public writes to gateway calls and result tables remain
refused for every principal. Preserve validated attributed UIDs in source
ingress without trusting client-supplied internal scopes.

### REQ-013 — UID-only observation correlation

Use card_uid in native correlation, OTLP attributes, shared contracts, client
encoding, Gate, ingress, ACK decoding, private peer frames, and SDKs. Bound
principals use trusted signed scope; unbound principals resolve at most 32
distinct UIDs in their tenant and require observation-target Cards. Missing
correlation is valid/null; foreign, missing, wrong-kind, malformed, or forbidden
UIDs refuse the complete frame. Authentication stamps publisher/request identity.
Keep legitimate CardRef contracts elsewhere; no global rename or alias.

### REQ-014 — Acknowledged observations activate the correct writer's bindings

Keep main ObservationRunOutbox retries, bulk enqueue, subject/writer matching,
durable replay fence, and lock ordering. Acknowledged observations do not wait
for derived queue writes. Freeze admitted row created_at as input_event_time,
not receipt time; next-day replay reads the same row and activates once. Do
not replace this with source best-effort tasks or activate every subject binding.

### REQ-015 — Main verification authority and runtime lifecycle

Keep main claim hydration, cache/baseline ownership, lease renewal, delayed
schedule clock, queue SQL, direct invocation permissions, and trusted tokenless
reads. LLM judges use the governed gateway under the correct judging authority;
no direct ProviderRegistry bypass. Preserve authenticated writer and application
Run lineage while adapting Variant/timestamp readers. Result delivery remains
on the shared outbox and never delays durable run settlement.

### REQ-016 — Typed Drift/Eval summaries and complete Task evidence

Use source typed drift_report/eval_summary. Preserve existing details as
nullable Variant for Task results only, using the existing AssertionResult
contract; no new Task payload type/table or duplicate summaries. Preserve
nullable/source summary semantics and sensitivity metadata. Result run_id is
the application Run being judged, not the Verifier execution UUID; result and
Verifier-run linkage remain separately attributable.

### REQ-017 — Declared time axes for all retained tables

Remove managed wyrd_event_time and use source types, precision, bounds, and
layout semantics consistently across ingestion, WAL, hot/sealed reads, Forge,
and consumers. card_uid/run_id are correlation-only; server-owned columns stay
unwritable. Dynamic tables default to wyrd_ingested_at or use a valid declared
non-null source-supported timestamp axis. Parent/detail joins use result_id
and cannot assume both rows share a day partition.

| Retained built-in | Declared time axis | Required adaptation |
|---|---|---|
| vala.traces.spans | start_time_unix_nano | Timestamp(ns,+00:00) OTLP instants; preserve exact ordering/bounds. |
| vala.metrics.points | time_unix_nano | Typed point/start/exemplar instants. |
| vala.logs.records | time_unix_nano | Typed event/observed instants. |
| vala.drift.observations | created_at | Timestamp(us,+00:00); retain main window/aggregation semantics. |
| vala.eval.observations | created_at | Variant context/media; preserve writer/Run inputs. |
| vala.verification.results | ended_at | Source typed summaries plus Task Variant details. |
| vala.drift.result_features | wyrd_ingested_at | result_id join; measured window stays domain time. |
| vala.eval.result_items | wyrd_ingested_at | Variant actual/expected; result_id join. |
| vala.gateway.calls | started_at | request_payload/response_payload Variant, preserving payload-read authorization. |
| vala.system.audit_log | created_at | Actual decision timestamp and Variant detail, no chain. |

### REQ-018 — No development agent trace table

Keep vala.dev.agent_traces removed and remove its sole-owner CodeAxis behavior,
registration/docs/test dependencies. Maintain ten canonical built-ins. Preserve
generic Variant/admission proof using supported or dynamic tables; do not
restore a test-only capability. Main eager provisioning/startup checks operate
over the ten source-updated schemas; source idempotent ensure remains available.

### REQ-019 — Preserve main typed query parameters

Keep ordered QueryParam across shared Rust facade, HTTP/gRPC, forwarding,
native bindings, Python, and TypeScript. Bind values in source planning rather
than SQL interpolation; preserve main public argument positions. This source
omission is unchanged ancestry, not intentional API removal. Parameter support
does not change source deadline clamping or source plan/result semantics.

### REQ-020 — Atomic terminal problem protocol

Take source WyrdProblem terminal representation and protobuf error_problem_json
field 9 with old error field 6 reserved. Keep main request params and other
newer error identities. Spec, protobuf, conversions, server streams, client
reconstruction, SDKs, CLI, and MCP must agree. Partial frames followed by failure
cannot become success. Keep revision/body/tenant validation and no compatibility
shim. Descriptor generation follows current main build ownership.

### REQ-021 — Shared types and narrow first-class client projections

Adopt source wyrd-types, schema mapper, timestamp/Variant conversions, row
preflight, and logical JSON rendering. Keep main internal feature gates and
renewable credential pooling identity. Adapt Service declared-table preflight
and registration to the new mapper without dropping its before-Card-transaction
validation/ensure semantics. Clients do not duplicate server/transport owners.

### REQ-022 — Main identity, credentials, SQL trust, and bootstrap

Retain main auth/credential/role/grant/revocation and human/platform separation,
API-key OTLP authentication-before-decode, verified-principal Gate entrance,
external Postgres credentials, and default local storage. New Bifrost SQL obeys
TenantConn/OperatorPool and main role/RLS conventions; no naked pool/connection
API. Keep selected local HTTP/gRPC endpoints and safe persisted client config.
Do not revive obsolete workload/bootstrap grants or Argon2 credential behavior.

### REQ-023 — Retained product owners and ordered shutdown

Keep main Workflow/MCP/gateway/Operator/Trigger/verification/identity/principal
owners, routers, configuration, and tracking connected in state and boot. Drain
accepted Workflows while their dependencies remain available; query tools settle
before run terminal state. Account for tracked calls, drain shared Scribe outbox
while Bifrost accepts, then settle roles/storage and observation queue within
the existing process deadline. Preserve errors and lifecycle settlement rather
than merely restoring a shutdown call.

### REQ-024 — Main migration history plus source forward DDL

Preserve main applied historical migration bytes/checksums and retired audit
tables' drop history. Import only source's three genuinely new retention,
cut-notification, and WAL-fence migrations, adapting role/RLS/search_path/grants
to main. Put genuinely required historical DDL differences in uniquely versioned
forward migrations; comment changes do not justify checksum changes. Keep old
workload/grant deletions: they are unchanged source/base, not new source changes.
Fresh replay and forward migration from main with no old Bifrost objects must
produce equivalent target SQL while preserving identities/Cards/credentials.

### REQ-025 — Fresh-only Bifrost, no implicit destructive transition

Honor source fresh Iceberg-v3-only and no mixed-version/data-migration contract.
Do not rewrite stored fingerprints, relax schema/format refusals, add historical
read conversion, or delete state to make startup succeed. Use isolated fresh
test Bifrost state. Retained old Bifrost deployment/export/reset is outside this
task; main-owned control-plane data is never collateral cleanup.

### REQ-026 — Coherent native graph and retained main simplifications

Take source Arrow/Parquet 60, DataFusion 55.1, object_store 0.14.2, Variant crates,
and pinned analytical forks as a coherent graph; preserve main unrelated deps,
features, profiles, pedantic rules, tier boundaries, and removed Ring/unearned
features. Regenerate combined lock/Hakari output. Carry only shared-storage
helpers needed by source Bifrost; retain main artifact signing/storage workflow.
Do not restore unrelated SignerFuture churn, retired checks/family scripts,
checked-in proto descriptor, or source review-skill/.claude changes.

### REQ-027 — Consumer-complete tests, artifacts, and documentation

Port useful source journeys/Arrow fixtures into main consolidated targets and
current public fixture stories. Reuse main isolated TS server fixture. Cover all
three SDKs and agent surfaces, registering new Variant/time/timestamp cases in
focused lanes and CI selection without silent omission. Regenerate schemas,
stubs, native TS declarations, errors, and proto outputs from source owners;
remove managed_candidates consistently. Update served OpenAPI, runtime MCP,
examples, and owning architecture where their actual contracts change. No
weakened assertions, manufactured tests for static work, or restored obsolete
test binaries/harnesses.

## Invariants and excluded outcomes

| ID | Required invariant |
|---|---|
| INV-001 | Source analytical architecture remains one engine; no alternate reader/storage/executor owner. |
| INV-002 | Tenant, Card, credential, peer fence/body, and object authority never widen on retries or adaptation. |
| INV-003 | Audit is one non-blocking authorization path; engine lineage never becomes permission audit. |
| INV-004 | ACK/replay/publication retain exact durable identities and uncertainty evidence. |
| INV-005 | Role discovery remains live through necessary drain; all owners settle or report counted/visible loss. |
| INV-006 | Observation subject/writer/application Run and Verifier/result identity remain distinct and exact. |
| INV-007 | Declared types/time axes and logical Variant values agree across writers/readers/schema/layout. |
| INV-008 | Partial/failed/cancelled query streams are not successful complete results. |
| INV-009 | Main migration history and control-plane state survive; old Bifrost format is not silently admitted. |
| INV-010 | No old dev table/CodeAxis, publisher, audit chain, deleted scripts, descriptor, or duplicate harness is restored. |
| INV-011 | Main public integrations and all first-class clients consume the same source-authoritative contract. |
| INV-012 | Checks are not bypassed, generated output is not hand-edited, and private mechanics remain implementer-owned. |

Excluded: deployment, main merge/push, destructive reset, retained-data migration,
mixed-version compatibility, new public peer services, new distributed scheduler,
new runtime/graph/transport layers, unrelated auth/storage/UI rewrites, blanket
lint suppression, synthetic host load, or changes to the inferred source-only
cache-timestamp/fleet-deadline concerns. These observations are evidence, not
new merge architecture instructions. A reproduced failure that prevents an
approved acceptance outcome still requires diagnosis and cannot be ignored.

## Acceptance and evidence closure

For every number n from 001 through 027, **AC-n** means REQ-n is satisfied in
the integrated candidate with its consumer closure, success/failure/edge proof,
and no violated invariant. These are independent acceptance obligations, not
permission to mark a group passed after only its happy path.

TASK-001's traceability table maps each AC to the inspected owner, ordered
behavioral scenario or explicit non-TDD proof, and exact focused recipes. The
implementation evidence must record one row per AC with actual source/diff,
test/check result, and any bounded command correction. All 527 source changed
records, all 148 conflicted paths, and all 76 automatically merged overlapping
modifications receive a reviewed disposition; historical L01–L64/S1–S4 are
reconciled rather than replayed blindly. New discoveries extend the ledger.

The final immutable change review, not the task's TDD iteration, runs:

```bash
mise run gate
mise run test:bifrost:journey:scribe:production-geometry
mise run test:bifrost:journey:forge:production-geometry
```

The extra geometry lanes prove production sizes excluded/reduced in the broad
aggregate. Do not repeat its included component lanes as final evidence. Live
cloud/provider and optional capacity benchmarks are not claimed as performed;
their unavailable credentials do not replace repository-managed proof.

Required strongest proof includes real SDK→server→SDK/agent journeys; Rust-only
unit tests support pure ownership/error branches, real Postgres tests prove SQL
and tenancy, and Python/Node lifetime behavior runs in its owning runtime.
Trace a failing Bifrost/server test before diagnosis. Exact named selectors,
features, setup, expected RED and actual GREEN results belong in task evidence.

## Open material decisions and revision history

None for this pinned code integration. Old populated Bifrost deployment is
explicitly excluded, not an unanswered migration decision. Private adaptation,
helper placement, fixture synchronization, and source test relocation are
implementation-owned within the approved outcomes. A material contradiction
or proposed scope/contract change routes through a new spec revision.

| Revision | Date | Status | Basis |
|---|---|---|---|
| 1 | 2026-10-11 | approved | User requested this exact 27-part recommendation as one spec/task; source retention/clamp and no agent_traces corrections retained. |

## Authority links

- [Repository standards](../../../AGENTS.md)
- [Agent rules](../../../architecture/agent-rules.md)
- [Wyrd protocol](../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Bifrost authority](../../../architecture/bifrost-design.md), updated under this change where source replaces main design.
- [Security posture](../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../architecture/references/languages/testing-workflows.md)
- [Canonical integration evidence](evidence.md)
- [One integration task](tasks/TASK-001-integrate-bifrost-variant-with-main.md)
