---
id: SPEC-storage-upload-consolidation
revision: 2
status: approved
---

# Shared uploads and one Bifrost object inventory

## Problem and proposed direction

Artifact files and verification `File` inputs upload bytes to tenant object
storage but use different planning and recording workflows. Artifact files
use the shared storage planner and completion operation, with completed-object
metadata in Postgres. File uploads bypass that completion workflow and have
no inventory record. Verification consequently resolves Artifact files from
Postgres inventory and File bytes from a digest-derived object path.

Use the existing storage lifecycle for both and record completed objects in
`bifrost.storage.objects`. This is the only completed-object inventory, not a
projection of a Postgres inventory. Postgres remains the Card control plane
and upload coordinator. The benchmark branch's canonical execution-input and
result readers remain the verification foundation.

For example, uploading a local File above the configured multipart threshold
uses the same planner and transfer machinery as an Artifact upload. Completion
records it through the existing Scribe outbox. Verification resolves its
declared digest through the Bifrost inventory, then reads the recorded object
location. It does not infer completion from a storage HEAD response.

This change consolidates existing behavior. It does not add a server-side
content-hashing pass, immutable-object promotion, new retention rules, garbage
collection, or a verification queue redesign. A recorded digest is the declared
digest; existing validation of bytes when verification reads them is preserved.

## Delivery baseline and approval

Implementation follows PR 216, then the rebased `change/benchmark-experiments`
branch. Consolidation is a separate follow-up change on their combined result.
The Artifact upload interfaces from PR 216 and the benchmark branch's
verification plumbing must both survive integration.

Nothing has shipped. This change targets a fresh installation of the combined
baseline; preserving existing deployment data or upgrading a deployed inventory
is not required. No data migration, backfill, staged rollout, old-writer drain,
or mixed-version serving support is in scope.

The maintainer explicitly approved the narrowed proposal on 2026-10-27 and
requested this specification, including verification table ownership and
evidence resolution. Approval covers the conditional upload-only ACK behavior
below; it does not authorize unrelated reliability or storage work. No
implementation tasks are created by this specification.
Revision 2 incorporates the maintainer's correction that nothing has shipped
and removes the migration requirements. The maintainer approved proceeding
with this revision by requesting planning after the revision handoff.

## Scope and costs

The change covers upload planning, transfer and completion reuse; one Bifrost
inventory; replacing its former readers; canonical verification evidence
assembly; and the necessary contract projections. It includes no migration work.

An upload-only ACK wait can increase upload-completion latency. Bifrost inventory
resolution adds query work to File verification and replaces the existing
Postgres inventory work for ArtifactRef. Neither cost is claimed to be zero.
No additional acknowledgement dependency may be imposed on other outbox
producers. Existing performance checks must measure the affected paths.

The inventory inherits Bifrost's normal availability and live-source behavior.
Scribe ACK is a durable append acknowledgement, not an Iceberg publication or
unconditional read-availability guarantee. Staged acknowledgement, when required
by the isolation rule, also retains the existing memory-outbox loss window.

## Required behavior

### Shared lifecycle

**REQ-001 — Shared uploads.** Artifact files and verification File uploads use
the existing storage initialization, planner, transfer dispatcher, and completion
workflow. Ownership distinguishes a Card file from a File upload without
creating another upload implementation. The planner selects the supported
local, single-PUT, multipart, or Azure protocol using existing configuration.
Existing file and aggregate limits remain unchanged.

**REQ-002 — One completion owner.** Normal completion and Card reconciliation
use the same storage completion owner. Reconciliation cannot independently mark
an upload or manifest completed while bypassing inventory recording.
Server-written Card envelopes keep their existing write operation and use the
same completed-object recorder; they need no artificial client upload session.
Card activation still requires its envelope written and all declared uploads
completed under the existing validation rules.

**REQ-003 — Existing validation only.** Preserve current size, encryption,
local-upload, permission, and input-read checks. Do not introduce a new
server-side digest-verification pass, checksum protocol, temporary-to-final
promotion scheme, or immutable-write requirement. Completion and inventory
recording do not newly certify an independently verified content digest.

### Completed-object authority

**REQ-004 — One inventory.** `bifrost.storage.objects` is the sole authority for
completed uploaded-object metadata, covering Card files, verification File
uploads, and Card envelopes. Later storage entities must use this owner rather
than create a parallel inventory; implementing those entities is out of scope.

The table is a tenant-scoped, server-managed Bifrost built-in, with the existing
managed attribution columns, normal daily time partitioning, and lookup Blooms
for storage identity, declared digest, and Card binding. Its contract contains:

| Metadata | Meaning |
|---|---|
| Storage identity and location | The tenant-qualified stored object; existing upload identity associates transfer retries where present. No new public object-ID system is required. |
| Backend | The existing configured storage backend. |
| Declared SHA-256 | The existing storage digest representation, normalized for lookup against File declarations. Absent only when the existing envelope write supplies no digest. |
| Size in bytes | The existing completion or envelope-write size. |
| Content type and encryption marker | Existing metadata, nullable when unavailable. |
| Origin | Card file, File upload, or Card envelope; this is not an authorization grant. |
| Optional Card UID and relative path | The exact Card binding; envelope records are distinguishable from listed artifact files. |
| Recording time | The inventory recording instant. |
| Completion or existing deletion fact | Distinguishes an available completed object from one removed by the existing lifecycle. |

Transport retries and reconstructed completion retries identify the same
logical object, rather than create duplicate file-list entries. Existing
deletion facts must not be undone by a delayed replay of the same completion.
This requires no uniqueness service, global digest deduplication, or new
key-value store. Existing Bifrost batch fencing does not by itself provide
logical object deduplication.

**REQ-005 — Replace all inventory consumers.** Artifact listing, Card hydration,
download initialization, cleanup's inventory reads, and verification obtain
completed-object facts from Bifrost. Remove
`wyrd.storage_artifact_metadata` as a serving authority. Existing
Card pointers may continue to identify their objects; they do not provide an
alternate inventory when the Bifrost record is unavailable.

Expected manifests and upload sessions stay in Postgres as declaration intent
and transfer coordination. They must not become another completed-object table,
nor may evidence readers use them as an inventory fallback.

### Outbox delivery and upload completion

**REQ-006 — Existing delivery path.** Inventory records use the existing process
Scribe outbox, local Scribe submission or authenticated peer capture channel,
batching, and Scribe ACK. No second delivery worker, durable delivery queue,
Postgres projection table, peer protocol, or outbox is introduced. The peer
destination allowlist adds only the new server-managed inventory destination.

**REQ-007 — Upload-only ACK wait, otherwise staged.** Prefer an upload-specific
completion notification driven by the existing Scribe ACK. Only the upload
request may wait for that notification. It waits for its inventory write, not
for audit delivery, another producer, an entire outbox drain, Parquet, or Iceberg.
An ACK-dependent success must not be generated for a dropped, rejected, timed-out,
or merely settled write. Waiting ends within the existing request deadline;
an unknown outcome is retryable, not reported as acknowledged completion.

If supporting that notification requires a new waiting barrier or
acknowledgement dependency for another producer, the shipped upload contract
uses staged acknowledgement instead. This is an approved fallback, not an
operator option or a per-request silent downgrade. The selected shipped mode
and its completion guarantee must agree across HTTP/MCP, SDKs, and documentation.
Implementation evidence must establish isolation before selecting ACK-wait mode.

| Completion mode | Meaning of successful completion | Remaining limit |
|---|---|---|
| Upload-only ACK | Transfer completion passed existing checks and Scribe acknowledged its inventory record. | Normal Bifrost query unavailability can still prevent an immediate read. |
| Staged fallback | Transfer completion passed existing checks and its inventory write was staged through the existing outbox contract. | The record may not yet be readable and may be lost before ACK under existing outbox loss semantics. |

Retries reuse the shared upload lifecycle and produce one logical inventory
object. This change does not promise a new crash-safe delivery mechanism or
automatic repair scan beyond existing coordination and reconciliation.

**REQ-008 — Other writers unchanged.** Audit, gateway capture, server spans,
verification inputs/results, and queued-submission handoffs keep their existing
admission, batching, retry, ACK, failure, and return semantics. In particular,
audit never blocks an operation and verification settlement gains no inventory
ACK wait. Ordinary shared-capacity contention is not a new ACK dependency;
nevertheless upload notification must never block the shared writer waiting
for its HTTP caller or for another operation.

### Verification tables and evidence resolution

**REQ-009 — One owner per verification fact.** Preserve the benchmark branch's
three-table verification model and add the shared storage inventory beside it.
Consolidation removes competing copies; it does not merge different kinds of
fact into one wide verification table.

| Authority | Facts it owns | How readers relate it to other facts |
|---|---|---|
| `bifrost.storage.objects` | Completed-object location, declared digest, size, backend, and optional Card binding. | File digest/size declarations or exact Card/path selection resolve inventory objects. |
| `bifrost.verification.execution_inputs` | One logical input record per execution: named declarations or source selectors, plus existing queued activation context. | `execution_id` identifies the record; selectors retain existing source identities. |
| `bifrost.verification.execution_outputs` | Detailed requirements, steps, and output provenance for one result; no copied evidence map. | `result_id` identifies the output and its summary. |
| `bifrost.verification.results` | Payload-free result summary, verdict, and execution/result correlation. | `execution_id` selects inputs; `result_id` selects detailed outputs. |
| Postgres Cards and execution coordination | Card metadata/lifecycle, accepted execution authority, scheduling, leases, status, and existing references. | IDs refer to canonical inputs and results; rows do not retain another input/output payload. |

Filename, declared format, slot name, inline JSON, trace declarations, and group
or scheduled source selectors remain invocation facts in execution inputs.
They are not uploaded-object inventory metadata. A File's declared digest and
size select and constrain an object; they do not establish that it completed
upload or supply an alternate location. There is no second persisted evidence
map in execution outputs, summaries, or the Postgres submission queue.

**REQ-010 — File resolution.** File planning reuses an existing completed
digest/size match only from the tenant's File-upload inventory records. Card
files and Card envelopes do not qualify for this reuse: identical content does
not transfer their ownership or deletion lifetime to a File upload. It no
longer treats an object-storage length match as a completed upload. Verification
resolves the File declaration to the same File-upload inventory and reads the
recorded location. Existing bounded byte reads, length/digest checks, and format
checks remain.

**REQ-011 — Artifact resolution.** ArtifactRef keeps its exact Card identity,
optional relative-path selection, and Card-read authorization. Its file list
and object locations come from the same Bifrost inventory as File resolution.
Resolve required metadata once per execution and reuse it for admission, byte
reads, and workspace selection instead of repeating inventory reads.

**REQ-012 — Canonical readback.** The existing `InputResolver` remains the shared
reader for submitted execution, group resolution, and execution/result
readback. It assembles evidence from canonical execution inputs or their source
selectors; result detail and summary come from their respective tables. Any
returned stored-object metadata comes from the inventory. Fetching execution
status or a judgment does not newly download or re-hash payload bytes.

**REQ-013 — No evidence fallback.** Missing or temporarily unreadable inventory
records produce a typed retryable readiness/availability failure rather than a
storage-path guess or a Postgres inventory fallback. A missing query row does
not prove an object was never uploaded, and cannot authorize deletion. Known
deletion and existing validation failures keep their existing terminal
behavior. Existing caller deadlines and queued retry bounds still apply;
readiness cannot wait indefinitely or fabricate evidence.

**REQ-014 — No added Postgres hot-path work.** File inventory resolution adds no
application-level Postgres lookup or write. ArtifactRef's completed-object
inventory lookup moves out of Postgres; its existing Card metadata and
authorization check remains. Existing target-cache misses, queue coordination,
and Bifrost's own catalog/fence SQL are unchanged. This is not a claim that
every transitive operation in verification is Postgres-free.

### Authorization, public surfaces, and fresh installation

**REQ-015 — Existing authority.** File upload admission keeps operation-level
`verifier:run`; invoking a Verifier keeps its exact invocation permission.
Artifact operations keep Card permissions. Shared endpoints select the owning
operation's permission from server-validated ownership, not a caller-supplied
tenant or arbitrary storage path. Internal inventory queries use the existing
table-scoped server-read mechanism only after domain authorization, with no
requirement that a File caller gain general Bifrost query privileges. Public
SQL inventory queries retain normal Bifrost query authorization. Gate accepts
no public append to the server-managed inventory.

**REQ-016 — Consistent surfaces.** Reuse the existing storage lifecycle over
HTTP and MCP and the shared Rust client projections in Rust, Python,
TypeScript, and CLI. Preserve PR 216's Artifact authoring interfaces, artifact
file-list output, and existing File/ArtifactRef declaration shapes. Evidence
preparation may remain a permission-aware adapter into shared initialization;
it cannot own a separate planner, transfer, or completion workflow. Remove the
evidence-only local route and obsolete contracts without compatibility aliases.
Update generated contracts, authored stubs, and examples through their owners.

**REQ-017 — Fresh-install authority.** A fresh installation creates and uses
only the consolidated completed-object inventory. Remove the obsolete Postgres
inventory and evidence-only routes from the resulting installation contract.
Nothing has shipped, so no existing-object migration, metadata backfill,
admission pause, old-writer drain, or mixed-version compatibility is required.

**REQ-018 — Existing deletion behavior.** Preserve current explicit Card/object
deletion and evidence retention behavior. Reflect an existing deletion in the
same inventory, using the existing deletion/reconciliation owner. Do not add
automatic payload deletion, reference-counting, cross-owner physical sharing,
TTL rules, or a new garbage collector. Iceberg snapshot expiry does not itself
authorize deleting uploaded payloads.

## Invariants and excluded work

- **INV-001 — One completed-object authority.** No completed File or Card object
  has a second serving inventory in Postgres, execution outputs, or sidecars.
- **INV-002 — Tenant isolation.** Shared transport, inventory lookup, object IO,
  deduplication, and reconciliation never widen the verified tenant or operation
  authority. An identical digest in another tenant is not reusable evidence.
- **INV-003 — Upload wait isolation.** No new audit, verification, gateway, or
  other producer wait is introduced to support upload acknowledgement. If that
  cannot be proved, use the approved staged fallback.
- **INV-004 — No validation expansion or regression.** Existing trust-boundary
  and read-time validation stays; new content verification, immutable promotion,
  and checksum guarantees are outside the approved change.
- **INV-005 — Canonical execution evidence.** Execution inputs/selectors,
  detailed outputs, and summaries retain their distinct benchmark owners and
  IDs. Response assembly never becomes another persisted evidence authority.

Also excluded: new storage entities, global physical deduplication, a general
Card cache, new visibility checkpoints, durable outbox repair machinery,
retention changes, and unrelated benchmark or verification optimization.
An implementation discovery requiring any excluded behavior needs a material
specification revision and renewed human approval.

## Acceptance and evidence

These are observations required before completion, not claims that tests have
already run. Real SDK/HTTP/MCP/CLI journeys cover each surface on which the
affected capability ships. Use the repository server and Postgres lifecycle;
backend integration uses the existing local/emulator/cloud lanes as appropriate.

| Criterion | Situation and required observation | Mapped obligations |
|---|---|---|
| AC-001 Shared transfer | Upload Artifact files and File inputs through single-PUT/local and supported multipart protocols. Both use shared planning/transfer/completion; current limits and negative validation outcomes remain. | REQ-001–003, REQ-016, INV-004 |
| AC-002 Inventory ownership | Upload, list, hydrate, download, and replay completion. Bifrost returns one logical object per stored identity; no consumer reads the removed Postgres inventory. Card reconciliation records completion through the same owner. | REQ-004–005, INV-001 |
| AC-003 ACK isolation | Delay or refuse inventory ACK while staging audit, capture, direct verification, and submitted work. Only upload completion acquires the proposed wait; other producers keep their existing behavior. Lost writes are not ACK successes. If isolation fails, prove and ship staged semantics instead. | REQ-006–008, INV-003 |
| AC-004 Evidence consumption | Reuse a completed File and verify it; an identical Card-file digest does not qualify as File reuse. Verify an ArtifactRef and select its workspace. Inventory comes from Bifrost once per execution, byte checks remain, and File introduces no application-level SQL. | REQ-010–011, REQ-014 |
| AC-005 Canonical queries | Read direct, submitted, group, and scheduled evidence/results. Inputs/selectors, outputs, and summaries reconstruct the expected response without copied evidence payloads in other stores. | REQ-009, REQ-012, INV-005 |
| AC-006 Delayed visibility | Delay or make the inventory unreadable after transfer. Verification returns a bounded retryable readiness failure, never guesses a path or consults a second inventory. A corrupt byte read still fails its existing digest check. | REQ-013, INV-004 |
| AC-007 Isolation and permissions | Try foreign-tenant digests, Card paths, uploads, and inventory writes with insufficient authority. No cross-tenant result or privilege widening occurs; normal operation permissions still suffice. | REQ-015, INV-002 |
| AC-008 Fresh installation and deletion | Start a fresh installation and upload Card files, Card envelopes, and File inputs. Only the consolidated inventory serves these objects; the obsolete Postgres inventory and evidence-only routes are absent. Current explicit deletion is reflected and cannot be undone by completion replay. | REQ-017–018 |

Compare upload-completion and verification measurements with the combined
baseline through its existing applicable performance lanes. Demonstrate ACK
isolation structurally and with controlled delayed-ACK tests; do not claim zero
shared-resource contention. No new latency target or relaxed existing budget
is authorized. Verify contracts, tenant boundaries, formatting/lints, and
affected language journeys through the owning `mise` tasks during implementation.
Exact test selectors and the minimal lane closure belong in the later task packet.

## Authority reconciliation and inspected owners

The proposed changes require the following narrow architecture updates with
implementation. Existing wording describes the old behavior; it is not a second
permitted serving contract. Do not use this change to rewrite unrelated sections.

| Authority | Required reconciliation |
|---|---|
| [Wyrd design: Verifier invocation](../../../architecture/wyrd-design.md#verifier) | Replace evidence-only single PUT, local route, and no-metadata wording with the shared lifecycle, inventory, conditional upload ACK, and readiness contract. Preserve invocation and audit semantics. |
| [Wyrd design: Registry lifecycle](../../../architecture/wyrd-design.md#registry-lifecycle) | Keep Card metadata/activation and expected manifests in Postgres; distinguish Card object references from completed-object inventory. |
| [Bifrost design](../../../architecture/bifrost-design.md#public-surface) | Declare the new built-in, scoped reads and internal-only writes; retain the benchmark's inputs/outputs/summary split, Scribe ACK meaning, and live-query limits. |
| [Security posture](../../../architecture/wyrd-security-posture.md#principal-and-credential-lifecycle) | Add only the inventory peer destination and scoped internal read; preserve tenant attribution and the non-blocking audit contract. |

Repository facts were inspected on PR 216's upload implementation and benchmark
head `d86cf238e058c2940027e72f838160a2db431306`. These references explain reuse;
they do not freeze private implementation details or override the combined
baseline:

- `crates/wyrd/wyrd-storage/src/service.rs`: shared planning/completion, the
  evidence-specific planner, and current size/encryption validation.
- `crates/shared/wyrd-client/src/storage/`: shared transfer dispatcher and
  completion; `cards/handle.rs`: PR 216 Artifact entry points.
- `crates/wyrd/wyrd-server/src/components/cards/service.rs`: activation,
  reconciliation, envelope writing, and explicit deletion.
- Benchmark `crates/wyrd/wyrd-server/src/scribe_outbox.rs`: one local/peer
  writer and its existing ACK-gated submission action; upload notification is
  an extension of this owner, not an already-existing generic callback API.
- Benchmark `crates/wyrd/wyrd-server/src/verification/inputs.rs`, `results.rs`,
  and `hosted/evidence.rs`: canonical input/result assembly and the remaining
  File/Artifact inventory split.
- [Spec-driven development](../../spec-driven-development.md), including
  maintainer approval and the independent comprehension pass, governs handoff.

## Open material decisions

None. ACK isolation versus staged fallback is an explicitly approved conditional
contract with a required proof, not permission to change other writers. Private
implementation choices and exact verification commands remain for planning.

## Revision history

| Revision | Date | Status and approval |
|---|---|---|
| 1 | 2026-10-27 | Approved by the maintainer with “Approve” and the request to include verification table consolidation. The independent comprehension pass identified an ambiguous File-reuse scope; the text now explicitly preserves File-only reuse and existing Card deletion lifetimes. The focused recheck found no remaining comprehension gap. Adds no hashing, immutability, retention, or queue-redesign scope. |
| 2 | 2026-10-27 | Approved by the maintainer's request to proceed with planning after the revision handoff. Nothing has shipped. Removes deployment migration, backfill, drain, and cutover requirements and their operations-documentation obligation. Replaces their acceptance criterion with fresh-install inventory ownership; upload, authorization, deletion, and verification guarantees remain. The independent comprehension pass found no contradiction or missing explanation in the revised baseline and migration scope. |
