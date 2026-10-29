# Integrated change audit — FIX_REQUIRED

Verdict: **FIX_REQUIRED**. Storage-specific handler/docs
gates and fixture setup need correction; inherited Scribe expectations and
integrated test-observer defects also block the combined candidate,
FIND-CHANGE-001–011. The fresh integrated gate failed; all requested focused
diagnostics are reconciled below. This review grants no completion.

## Immutable subject and independence

Base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`.
Target: `462ebee97c81772103054f7d090053ac7054bafa`.
Authority: approved `SPEC-storage-upload-consolidation`, revision 2, original
TASK-001/002/003, applicable root/SQL/SDK/docs AGENTS, agent guides, spec-driven
development, TESTING, and owning registry/Verifier, Bifrost durability/public
surface and security sections. No CodeGraph index exists. The read-only [delivery context](delivery-context.md)
records the PR stack and maintainer’s current delivery guidance; it does not
change the immutable subject or authorize branch/source operations.

One fresh reviewer, independent of implementation and integration, inspected
the Git base-to-target implementation diff, surrounding owners and callers,
original task/evidence records, final task verdicts and their underlying
closure/verification evidence. Source remained unchanged. The reviewer ran
read-only Git/navigation checks, including a clean range whitespace check;
root owns all serialized runtime verification. The review directory contains
reports only. No source archive/diff, commit, staging, merge, push or deployment
was created by this audit.

## Delivery correspondence and task-review closure

| Accepted subject | Correspondence to target | Integrated audit treatment |
|---|---|---|
| TASK-001 at `5e7dec69732ba6d4d05399aeffd6f4d9c683745d`, `review/TASK-001-R2/verdict.md` PASS | Direct ancestor, including all five remediation closures | Reopened its subsequently modified Card getter, inventory read, shared upload and route seams. Hydration still requires the recorded envelope; compensation still commits Failed/pending before deletion ACK. Completion, deletion dominance and scoped-read guarantees survive. |
| TASK-002 at `793912475795951d440b8d072c99a9957ad18464`, `review/TASK-002-R3/verdict.md` PASS | Direct ancestor; subsequent executable changes are TASK-003's inventory/evidence reuse, not a replacement of transfer/session/sweeper code | Independent session paths, persisted ownership, expiry recovery and all three prior findings remain closed. Its review missed newly affected handlers' tracing rule: FIND-CHANGE-001. |
| TASK-003 at `57219dee3713e817ace6deac5e91ffc673a5a811`, `review/task-003-r1-57219dee3/verdict.md` PASS | Direct ancestor; target adds review evidence, no later source change. MCP blob `272708fa68765ba19c67dd9af589203863161a98` is unchanged | Canonical owners, execution memo, bounded readiness and one-line documentation closure survive. The maintainer withdrew benchmark remediation; timing remains unmeasured. |

The merge `15f939fc917b22efa5e5e4d12fd1f97be807a0a7` has parents
`5e7dec697` and `db29ee915`. Its benchmark-rev19 source is a separate delivered
baseline, not consolidation work invented by these three tasks. Comparing
the merge base to `db29ee915` finds 29 changed paths; every path except
`architecture/bifrost-design.md` is byte-identical at the integrated target.
That authority combines the benchmark durability paragraph/state removal with
the inventory additions. No benchmark source conflict rewrite is hidden by
task ancestry.

The separate committed rev19 supplement identifies group WAL submissions,
WAL-v8 segment recycling, per-shard manifest watermarks folded from staged
records, removal of redundant `Published`, ordered concurrent staging,
unstaged-backlog admission, RetryInfo, and capacity-client retries. Its scaling
and future optimization work remains open in its own packet; completing the
storage change must not complete or delete that packet. This audit inspected
the actual delivery's durability effects: SLICE/COMMIT ordering and the
single sync/fence-before-ACK boundary remain; recycling tags stale tails and
fails closed past acknowledged fences; per-shard watermarks avoid suppressing
another shard's lower shared LSNs; `StagingOrder` prevents later-generation
records hiding an earlier unstaged generation; retirement folds before removal.
The inventory uses the same unchanged Scribe acknowledgement boundary.
Backlog refusal is normal shared-capacity contention and does not introduce
an upload-dependent wait for sibling producers.

## Shared owners and cross-task seams

The preparation endpoint is now a permission-aware adapter to
`wyrd_storage::service::file_upload_init` and shared `init_upload`. The shared
client's `RunEvidence::pin` invokes `upload_artifact`, including multipart
part URL and completion handling. Card and File paths share persisted sessions,
planner and transfer protocols; unique File session paths prevent one same-
digest initialization or abort from revoking another.

Storage `upload_complete` and `reconcile_upload` share `finish_upload`, keeping
size/encryption/local validation. HTTP completion and Card reconciliation both
record their returned facts through `ObjectInventory::record`. Server envelope
writes retain their existing operation and use that recorder without fake
sessions. Reconciliation records all required files and envelope before
activation. Existing manifest/session rows retain coordination duties; they
do not serve as an alternate inventory.

`ObjectInventory` is the one completed-object reader/recorder. Its table uses
normal managed attribution, daily partitions and storage/digest/subject Blooms.
Card readers fold replayed facts into one file per path and a Card deletion
dominates every completion; File lookup includes only same-tenant `file_upload`
facts of the declared digest/size and excludes deleted paths. Hydration checks
the retained envelope pointer against the returned envelope before IO. Listing,
download initialization, fitter and hosted Artifact readers use this owner.
The old SQL module is deleted and a fresh installation drops its table; only
historical migration and absence-test references remain.

The existing process outbox gains a closed Object variant/destination and a
row-specific one-shot sender. `ScribeSink::tally` sends only on that row's
successful frame acknowledgement; rejected/unencodable/dropped writes cannot
notify success. The send cannot await its HTTP receiver. Record waiting is
bounded to half the configured request timeout, preserving retryable unknown
outcomes. Local and mTLS peer paths remain the same writer. Audit/result/
capture/span staging and queued submission handoff keep their previous return
and settlement boundaries. Experiments directly hold the Object stream;
queued handoff isolation is established structurally rather than by a combined
delayed-ACK public-API experiment.

TenantEvidence keeps execution-owned File and exact-Artifact metadata and
shares it between admission, payload read, citations, selection and workspace.
Artifact reuse still makes the caller's audited Card decision; exact identity
and optional UID remain in the key. File execution reads use the already
resolved SYSTEM executor, avoiding a new application registry query. Bifrost's
own catalog lookup/fence SQL remains within the explicitly permitted boundary.
Byte reads still validate bounded length, digest and format. Missing/unreadable
File inventory maps through existing readiness errors and bounded queued
attempts. InputResolver remains the shared canonical readback owner; result
status/readback does not call TenantEvidence or perform new byte IO.

## Acceptance matrix

Source references below are relative to the repository root. Verification
abbreviations: I1 = TASK-001 original evidence and R2 fresh Card/SQL closure;
I2 = TASK-002 original backend/surface evidence and R2 runtime/session/sweeper
closure carried into final R3; I3 = TASK-003 original V1–V7 plus independently
rerun metadata/readiness/no-byte-read tests and final immutable documentation
closure. G = fresh integrated aggregate and exact diagnostic follow-ups, detailed in
[verification.md](verification.md) and below. PASS rows describe the stated
implementation and positive scenarios, not a claim that the aggregate passed.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 shared uploads and existing limits | storage `service.rs::init_upload`; verification preparation; client `verify/mod.rs::pin` → `upload_artifact` | I2 shared File/local/backend journeys, client multipart request assertions; G | PASS |
| REQ-002 one completion owner and activation | storage `finish_upload`; Cards `verify_card_manifests`, `write_card_blob`, `commit_card_activation` | I1 Card round trip/reconciler recovery, I2 shared completion | PASS |
| REQ-003 preserve existing validation, no new certification | shared completion/head/local checks and TenantEvidence `stored/read`; inventory stores declared digest | I2 authority/size/digest limits and hosted corruption/format proofs | PASS |
| REQ-004 sole attributed daily inventory, replay identity and deletion dominance | `tables/storage/objects.rs`; ObjectInventory `fold/file_fold` | I1 layout/catalog/attribution, Card replay/deletion; I2 File-origin/reuse proofs | PASS |
| REQ-005 all old consumers replaced | Cards `card_files/hydrate_card/card_file`; fitter; TenantEvidence; deleted SQL query module | I1 Card/CLI/fitter/fresh-install proofs; source reference audit | PASS |
| REQ-006 existing outbox and peer allowlist | ScribeWrite::Object, ScribeTable::Objects; capture peer only adds this destination | I1 outbox/peer/source/security and reserved-write proof | PASS |
| REQ-007 genuine isolated bounded ACK mode | inventory `record`, outbox `tally` | I1 controlled ACK loss/refusal/drop/timeout test + healthy real API completion. Combined queued delay proof is structural | PASS |
| REQ-008 unchanged sibling producer settlement | no new sibling wait; object notification is synchronous; existing schedule/submission paths unchanged | I1 source producer tracing; existing outbox/runtime regression evidence; G | PASS |
| REQ-009 separate canonical fact owners | unchanged input/output/result tables and InputResolver; inventory beside them | I3 canonical ownership runtime/group/schedule/readback proofs | PASS |
| REQ-010 origin/tenant/size scoped File reuse and recorded reads | inventory `file_object/file_fold`; TenantEvidence `file_object/stored` | I2 reuse denial for uncompleted/Card/envelope/foreign/wrong-size/deleted records; hosted byte checks | PASS |
| REQ-011 exact authorized Artifact and one metadata resolution | Cards get-objects route + TenantEvidence ArtifactKey/inventory memo | I3 metadata-once and workspace/selection/hostile identity tests | PASS |
| REQ-012 shared canonical readback without bytes | InputResolver `evidence/slots/result/direct`; existing service/runner/group consumers | I3 deleted-payload readback and Rust/Python/TS/MCP status journeys | PASS |
| REQ-013 bounded readiness without guess/fallback or deletion inference | inventory missing-table/row handling, manifest readiness; TenantEvidence retryable admission | I1 missing envelope/file visibility; I3 failed-query/direct/queued recovery/exhaustion; I2 corrupt-read negative proof | PASS |
| REQ-014 no new File application SQL | execution caller carries SYSTEM; inventory `executed_by`; existing Bifrost table authority | I3 execution metadata test uses an unregistered executor and exactly two inventory reads; source trace of direct/queued callers | PASS |
| REQ-015 operation ownership, tenant authority and reserved writes | upload_owner under TenantConn; authorize_upload; domain Card checks; table-scoped SystemReadAuthority; Gate denial | I1 scoped-read/public-append; I2 exact File grant/Card/foreign/outsider audited negatives; I3 Artifact authorization | PASS |
| REQ-016 consistent shared projections and removed obsolete surface | EvidenceUpload.upload_id; shared client; HTTP/MCP descriptors; docs; old evidence-only route/planner deleted | I2 Rust/Python/TS/MCP/CLI/OpenAPI/codegen/napi evidence; G | PASS |
| REQ-017 fresh installation, no legacy inventory or aliases | drop migration + nullable Card owner session migration; old query module/route removed | I1 fresh-install test requires DSNs and proves table absent, coordination tables present | PASS |
| REQ-018 existing deletion and durable recovery | explicit/claimed cleanup recorder; failure commits Failed/pending before ACK; fold deletion dominance | I1 explicit deletion, compensation refusal/retry/replay and cleanup regressions | PASS |
| INV-001 one completed-object authority | removed SQL owner; canonical inputs carry declarations, not inventory | I1/I2 reference/route audit, I3 payload owner test | PASS |
| INV-002 tenant and operation isolation | persisted session ownership, validated paths, scoped context, origin-specific File lookup | I1/I2/I3 foreign tenant/Card/exact-grant/public-write negative journeys | PASS |
| INV-003 no sibling wait | object-only sender and bounded waiter; unchanged sibling stage interfaces | I1 controlled stream test and producer tracing, G | PASS |
| INV-004 validation preserved without new hashing/promotion | shared existing completion and bounded read checks | I2 local/remote protocol negatives; hosted digest/format tests | PASS |
| INV-005 canonical execution evidence | InputResolver/table owner split unchanged; per-execution memo is not durable | I3 canonical direct/submitted/group/scheduled readback | PASS |
| AC-001 shared local/single/multipart protocols | same planner/client/completion for both owners | I2 local, S3 multipart, GCS single PUT, Azure block blob. Real-GCS multipart missing; G | PASS within disclosed backend proof |
| AC-002 inventory round trip and replay | one inventory across Card consumers and reconciliation | I1 completed_objects_use_one_inventory, deleted_objects_survive_completion_replay | PASS |
| AC-003 ACK isolation and no false success | isolated Object notification + unchanged siblings | I1 inventory_ack_is_isolated; queued path source proof; public completion healthy proof | PASS with explicit experimental limit |
| AC-004 consumed/reused File, Card separation, Artifact workspace and SQL boundary | File-origin lookup and execution metadata memo | I2 origin/reuse/backend proofs; I3 evidence_metadata_is_resolved_once | PASS |
| AC-005 canonical query assembly on every mode/surface | unchanged InputResolver, result owner split and SDK projection | I3 V3–V7, groups and scheduled canonical ownership evidence; G | PASS |
| AC-006 delayed/unreadable readiness, bounded retries and corruption | existing retryable mappings and byte-check owner | I1 visibility/hydration tests; I3 inventory_readiness_preserves_execution_bounds and corruption cases | PASS |
| AC-007 tenant/permission hostile flows | owner-authorized transport and scoped inventory reads | I1/I2/I3 tenant, Card path, append, exact-grant and outsider negatives | PASS |
| AC-008 fresh install, deletion and stale completion | removed SQL authority/route; shared deletion recorder/fold | I1 fresh schema + compensation/replay; I2 File shared lifecycle and OpenAPI absence | PASS |
| Constraint: preserve PR216 Artifact authoring and benchmark canonical plumbing | existing Cards/client APIs retained; InputResolver and distinct tables preserved | I1 CLI/Card journeys, I2 backend journeys, I3 canonical readback | PASS |
| Constraint: request/queued bounds and nonblocking audit | half-timeout record/read budget; existing engine/queue retries; no audit wait | I1 ACK experiment; I3 three-attempt exhaustion and later recovery | PASS |
| Constraint: current user excludes benchmark remediation | committed maintainer decision; no performance claim in this audit | Timing deliberately unmeasured, no new run/workload required | PASS |
| Non-goal: no deployment migration/backfill/mixed-version serving | additive immutable drop/owner migrations produce fresh-install contract only | I1 fresh-install schema proof | PASS |
| Non-goal: no checksum protocol, independent hashing pass, immutable promotion | declared digest remains declared; existing validation owners unchanged | Source audit and I2 local/read-time negative proofs | PASS |
| Non-goal: no new inventory, delivery worker/queue/outbox/protocol or repair scan | extends existing ObjectInventory/Scribe destination and lifecycle | Source owner tracing across all changed call sites | PASS |
| Non-goal: no retention/GC/reference counting/cross-owner sharing/new storage entity/global dedup | unique File session objects; existing Card cleanup only | I2 independent-session and Card-versus-File reuse tests; source audit | PASS |
| Non-goal: no general Card cache, visibility checkpoint, queue redesign or unrelated optimization | memo lifetime is one TenantEvidence execution; existing queue/readback unchanged; benchmark changes separately delivered | Source audit + ancestry and separate packet reconciliation | PASS |
| Repository: changed handler instrumentation with scrubbed arguments | storage routes part_url/abort/local_blob lack instrument attributes | Direct source at lines 161/338/403; complete/download adjacent pattern | FAIL — FIND-CHANGE-001 |
| Repository: exact restart resource accounting gate | cluster test expects only footer budget; owning architecture and storage precharge both footer and range budgets | Fresh Wyrd family: 2418 pass, one failure at cluster.rs:3013, actual 322122546 vs expected 161061273 | FAIL — FIND-CHANGE-002 |
| Repository: documentation of new items | first MCP DOCUMENT has no rustdoc; three new rev19 test/helper items have no required panic contract | Direct source/diff inspection, concrete item inventory below | FAIL — FIND-CHANGE-003 |
| Repository: complete affected MCP journeys | staging-pressure test faults the one-slot shared outbox before new inventory-dependent Card setup | Fresh ignored Bifrost slice fails during register_fixture, before either intended pressure assertion | FAIL — FIND-CHANGE-004 |
| Repository: configured publication claim bound | unchanged tests assume old fixed WAL-worker budget; actual shared assembler/worker uses persistence CPU threads | Both lifecycle concurrency journeys fail at six versus four | FAIL — FIND-CHANGE-005 |
| Repository: correct retained/fault observation boundaries | inherited tests await the wrong producer or install faults before setup settles | Three observed Bifrost failures, detailed below | FAIL — FIND-CHANGE-006 |
| Repository: failed replay leaves operator evidence exactly intact | old writer shutdown trims recycled tail after premature test snapshot | Exact-byte assertion fails; identical3458-byte prefix at lengths53467/3458 | FAIL — FIND-CHANGE-007 |
| Repository: canonical SDK retained payload read | inherited Rust helper queries removed outcome summary field | Rust custom-verdict journey QueryInvalidSql | FAIL — FIND-CHANGE-008 |
| Repository: exact pressure refusal population | inherited SDK tests omit origin=group input from total | Rust observes7 vs6; same source omission in Py/TS | FAIL — FIND-CHANGE-009 |
| Repository: canonical read shape without stronger ACK timing | inherited global query counter includes lawful deferred input attempt | Group occurrence journey observes15vs14 and explicit evidence_unavailable deferral | FAIL — FIND-CHANGE-010 |
| Repository: portable terminal interruption security proof | inherited helper observes a hung-up slave after session leader exit | Full aggregate and exact CLI guided selector fail ENOTTY; master positive/negative controls pass on Darwin/Linux | FAIL — FIND-CHANGE-011 |
| Repository: format/lint/contracts/boundaries/full shipped journeys | format/lints/contracts/boundary tasks pass; six aggregate tasks fail | Fresh G and exact follow-ups below | FAIL — findings and explicit limits |

## Material finding

### FIND-CHANGE-001 — changed upload handlers omit required operation spans

Classification: **VIOLATION**, confirmed from immutable source and the actual
diff. Obligation: root AGENTS Code rule, server handlers require
`#[tracing::instrument]` with scrubbed arguments; TASK-002 owns these changed
public storage projections under REQ-015/016 and AC-007.

Locations: `crates/wyrd/wyrd-server/src/components/storage/routes.rs:161`
(`part_url`), `:338` (`abort`), `:403` (`local_blob`). All three materially
change persisted-owner authorization, and local transfer also changes body
admission. Each function begins directly after `#[utoipa::path(...)]`, with
no instrument attribute. Authorization/owner-read/body-read failures occur
before the instrumented storage service and therefore have no required
handler operation span. Generic HTTP request instrumentation does not satisfy
the explicit rule. This is a mandatory gate, not optional telemetry hardening
or a claim that the permission behavior is incorrect.

Minimum correction: three conventional scrubbed attributes following adjacent
`complete` and `download_init`; exclude state, Caller/credentials, headers,
and body bytes from automatic recording. No new tracing framework, helper,
test harness, public contract or durable behavior is justified. Closure proof:
inspect the attributes and complete handler scope; normal format/lint and
patch whitespace checks suffice. No fabricated runtime test for attributes.

### FIND-CHANGE-002 — restart test expects only one of two cache reservations

Classification: **INCORRECT**, confirmed mandatory red gate. Location:
`crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:3013`,
`cluster_restart_rederives_same_plan_from_retained_snapshot`.
Fresh Wyrd family runs 2419 tests: 2418 pass; this one fails with governed
bytes 322122546 versus expected metadata-cache bytes 161061273.

The trace shows the same 3-CPU/3221225472-managed-byte plan before and after
restart, no restored WAL work, and the preceding zero-Scribe assertion passes.
Architecture `bifrost-design.md:1032–1035` requires whole-budget charging for
both caches. `BifrostStorage::with_oracle_caches` at storage/mod.rs:163–171
reserves footers and ranges; the resolved defaults are each managed/20.
Both this cache owner and test are unchanged in the actual base-to-target
range: this is inherited foundation test drift, not a storage-authored or
new merge-induced resource defect. The observed total is exactly those two reservations, not evidence of an
extra resource owner or leak. The mandatory gate must pass before completion;
no baseline comparison or test weakening is appropriate.

Minimum correction: keep the exact clean-node accounting assertion, comparing
with metadata-cache plus data-cache budgets from the existing storage policy;
update its explanation and preserve every restart/plan/WAL/pool/zero-Scribe/
no-active-query assertion. Corroborate with the exact restart test on repository
Postgres with tracing, then normal format/lint/whitespace checks. No production
resource change is justified.

### FIND-CHANGE-003 — direct new-item documentation omissions

Classification: **VIOLATION**, confirmed root AGENTS/agent_docs mandatory
changed-item gate. TASK-002 introduces local `const DOCUMENT` in MCP
`verification.rs:1224`, `file_inputs_use_shared_uploads`, with no rustdoc.
The later TASK-003 readback fixture has the accepted one-line purpose doc;
this earlier sibling does not. The declared bytes and consumer are clear,
so the correction is the same concise fixture-purpose comment.

Three new items enter through the separately delivered rev19 merge and also
fall under the actual integrated-range gate: Scribe admission
`unstaged_backlog_at_its_bound_refuses_as_busy` (:309), replay `test_shard`
(:1409), and WAL `retired_segment_is_recycled_and_its_stale_tail_ends_replay`
(:4542). They directly call expect/assert without a `# Panics` contract.
Add concise descriptions of their fixture/assertion failures and routing-width
invariant. No executable change or repeated benchmark/runtime proof is needed.
Inherited trait-method contracts, untouched debt and spelling-only changes are
excluded. These concrete omissions are batched with the existing functional
gate correction, avoiding a separate style-only round.

### FIND-CHANGE-004 — MCP pressure fault prevents required setup

Classification: **INCORRECT**, confirmed mandatory red gate. Location:
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:2458`,
`staging_pressure_is_retryable`, capacity-one builder at :2461.
The fresh gate trace shows a successful Verifier followed by a Service whose
inventory Object is unacknowledged (`timed_out=false`) during fixture
registration. Existing declaration initialization compensation makes it Failed;
the test receives RegistryArtifactVerifyFailed before invoking execute or
enqueue. It therefore proves neither intended typed pressure assertion.

Capacity one sets reservable slots to zero and leaves only one unreserved
slot for audit/span/Object staging. The new acknowledged envelope write can
legitimately be shed at that configured bound. No false ACK or changed
verification reservation behavior is demonstrated. Changing production
compensation or merely raising the test capacity would not preserve the
test's intended deterministic pressure contract.

Maintainer-directed remediation clarification: retain a small MCP contract
check, not the one-record queue scenario. Existing outbox and HTTP tests cover
capacity, refusal and recovery. Finish ordinary fixture setup, then use the
existing local Reservation API solely to exercise execute/enqueue's refusal
projection. Preserve the stable code, retry hint and existing zero-input-row
assertion. Release the guard before final flush/readback/shutdown. No new
harness, projection, configuration, load or recovery scenario is required.
This clarification changes the correction scope, not the observed setup failure
or its source evidence. The exact existing MCP selector supplies closure proof.

The full Python lane fails the same
setup class at `test_verify_in_real_time.py:1042` during support declaration:
capacity six leaves only two unreserved slots, inventory ACK is shed, and
normal compensation makes registration no longer pending. Rust/TypeScript
have the identical capacity-six fixture assumption. Their correction must
preserve actual two-slow-judgment/four-reserved-slot saturation after public
setup, not merely increase capacity. No late pressure/Reservation control is
currently projected in Python/TypeScript. R3 explains why a narrow private
projection of the existing Reservation owner may be earned, its cleanup
obligation, and its cost against existing fixture/capacity alternatives;
reversible binding/lifetime mechanics remain implementor-owned. No
production admission or durability change is demonstrated. Their separate
canonical input-population defect is FIND-CHANGE-009.

Storage-owned remediation:
[TASK-CHANGE-R1-review-gates.md](TASK-CHANGE-R1-review-gates.md), ready for
`$wyrd-implement`. No specification revision is required.

## Additional mandatory integrated-gate findings

These are test-observer or expectation defects, not supported-use production
regressions inferred from red tests. The actual diff leaves the test bodies in
FIND-CHANGE-002/005/006/007/008/009/010 unchanged (apart from line movement in the
Rust SDK file). Source ownership and the delivered merge effects determine
routing; this review did not run historical baselines.

### FIND-CHANGE-005 — publication tests use the obsolete worker budget

**INCORRECT**, mandatory red gate. Both
`wyrd-testing/tests/bifrost/scribe/lifecycle.rs:179,256` fail at six in-flight
claims against fixed four. Their fixture explicitly assumed WAL IO workers.
The separately merged boot composition now supplies persistence CPU threads;
`PersistenceRuntime::build_staging` and `start` use the identical configured
count for assembler and one shared worker, while the shared assembler still
caps globally outstanding claims. The object-write guard measures one
`publish_claim`, not each artifact. Public default geometry resolves to six
on this host; no architectural fixed-four contract exists.

Minimum correction: derive the test budget from the same public default
`ScribeRuntimeConfig::persistence_cpu_threads`, offer twice that many tables,
and retain exact budget/exact-once/flush/value assertions. Preserve observed
parallel overlap when the configured budget exceeds one; one-slot geometry
must prove its actual one-slot bound. No arbitrary widening or production
budget change. Exact lifecycle selectors and closure criteria are in R2.

### FIND-CHANGE-006 — faults/readback precede their relevant producer boundary

**INCORRECT**, three mandatory red tests, source-backed observer defects:

- `scribe/telemetry.rs:653`, `scribe_hot_path_telemetry_reconciles`, reads
  through an audited query then freezes Scribe without first settling the
  process outbox. Its late audit row can remain in a new active memtable,
  invalidating the age-tick observation of the already frozen generation.
- `scribe/write_read.rs:1196`,
  `acknowledged_rows_survive_stage_pressure_and_restart`, occupies the shared
  root after an audited readability query, then calls `flush_bifrost`, which
  first awaits outbox settlement. Trace shows that audit write refusing for
  60 seconds before the intended stage-fault proof can run.
- `server/query.rs:877`,
  `generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`,
  sends to a remote Oracle that owns the permission decision but awaits only
  the ingress outbox before expecting its retained audit decision. Existing
  cluster `await_audit_retained` explicitly closes this remote-owner gap.

Minimum correction: use the existing settlement owner before each test fault
or exact retained read. Stage-pressure setup needs only
`settle_bifrost_outbox`, not a full flush that would publish the generation
before the fault. Preserve exact age/publication, WAL/restart, row,
terminal decision, cancellation and cleanup assertions. No sleeps, larger
limits, producer semantics or availability changes. R3 supplies exact tests.

### FIND-CHANGE-007 — failed-replay snapshot includes preceding writer shutdown

**INCORRECT**, mandatory red gate at
`server/owner_inspection.rs:365`, `failed_wal_replay_is_role_local`.
The test corrupts/snapshots a still-live writer then calls `restart_bound`,
which gracefully shuts that writer down. The delivered recycled-segment
owner trims stale allocated tails during `close_all_streams`: the captured
files have identical first 3458 bytes and lengths 53467 versus 3458. This is
not evidence that failed replay rewrote acknowledged data.

Minimum correction: call the existing public Scribe shutdown before
corruption/snapshot, then retain normal `restart_bound`. Shutdown is
idempotent and STOPPED returns immediately. Keep the full exact-byte
comparison and role-local readiness, published-read and no-ACK-write proof;
never replace it with a prefix comparison. R2 provides exact selector.

### FIND-CHANGE-008 — inherited SDK helper queries an obsolete payload column

**INCORRECT**, mandatory Rust SDK red gate at
`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:1493`,
`retained`, called by `custom_verdict_data_survives_readback`.
Its unchanged base helper queries `to_json(outcome)` from results, whose
canonical summary column is verdict. Custom verdict requirement/step data
belongs to `execution_outputs.output`; Python/TypeScript siblings already
read that owner. QueryInvalidSql prevents the intended custom-data proof.

Minimum correction: use canonical output SQL and a typed output projection
of requirements/steps; do not decode output as a full VerificationOutcome.
Retain exact custom data/bounds and the separate full status equality, with no schema
alias or production SDK change. R3 supplies exact selector.

### FIND-CHANGE-009 — SDK total omits its generated group execution input

**INCORRECT**, mandatory Rust SDK red gate at
`verify_in_real_time.rs:3259`, `verification_staging_refuses_and_recovers`:
seven observed rows versus six (`4 + offered`, offered=2).
`verification/groups.rs::record` stages exactly one `InputRecord::Group`
for a new group execution, in addition to its submitted turn. Tenant totals
are two direct burst + offered normal + recovered + grouped turn + group
execution, exactly `5 + offered`. Python :1110 and TypeScript :1115 carry
the identical omission in inherited test bodies.

Minimum correction: exact accepted population plus one identified group
execution input. Preserve no refused-call input, typed retry pressure,
neighbor progress, recovery and one-turn group evidence; do not hide extra
rows behind a loose bound or alter capacity. R3 supplies focused closures.

### FIND-CHANGE-010 — global query count assumes immediate handoff visibility

**INCORRECT**, mandatory red gate at
`server/verification_runtime.rs:7575`, `group_lookup_preserves_source_occurrences`:
15 successful Oracle profiles versus the assumed14. The existing trace shows
short-group execution `01a18819-1e34-7581-b7ae-1adabf4c5f97` deferred at
14:38:29.980067 with `evidence_unavailable`: its own ACKed input record is not
visible yet. Its subsequent claim repeats that one read. Deferrals do not
spend attempts, as required. A global exact query count across concurrent
scheduling therefore asserts stronger ACK visibility than approved authority.

Minimum correction: retain global runtime work as informational, and preserve
exact source/read-shape proof at the settled canonical owner. After existing
flush/snapshot refresh, wrap each existing SDK status read in `ScanEvidence`:
ordinary turns need one canonical three-table join; each group adds one source-input
and one selected-output read, exactly three. Preserve both exact ordered
source occurrence maps, verdicts and six completed executions. No widened
query allowance, hardcoded 15, new instrumentation or production handoff change.
R3 supplies the exact selector.

### FIND-CHANGE-011 — PTY echo observer is invalidated by session-leader exit

**INCORRECT**, mandatory local-dev journey red gate at
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py:1016`,
`interrupt_hidden_question`. Exit130 succeeds; querying the slave after
`communicate` reaps the controlling-session leader gets Darwin ENOTTY25.
This is an invalid post-exit observer, not evidence that EchoGuard failed.

Minimum correction: observe the retained PTY master. Independent read-only
Darwin arm64 and Linux probes prove both controls after child exit 130: master
observes ECHO enabled when restored and ECHO disabled when intentionally unrestored.
This retains security detection. Keep ECHO-off before interruption, ECHO-on
after it, both SIGINT/SIGTERM, no disclosure and no files/installation. No
supervisor, new fixture, swallowed ENOTTY or CLI restoration change. R3 gives
the exact guided setup command. The aggregate's later installed-CLI commands
were not executed after this failure and remain an explicit verification limit.
No environment dump from the failed pytest output is reproduced here.

### Routing by owning scope

| Packet | Owning authority and scope | Findings |
|---|---|---|
| [R1 storage gates](TASK-CHANGE-R1-review-gates.md) | Storage rev2 TASK-002 affected handlers and first MCP fixture | 001, storage subset of003,004 |
| [R2 inherited Scribe gates](TASK-CHANGE-R2-inherited-scribe-gates.md) | Benchmark rev19 delivered resource/WAL/staging correctness, root documentation and testing gates; no measurements/optimizations | 002, merged subset of003,005,007 |
| [R3 integrated test boundaries](TASK-CHANGE-R3-integrated-test-boundaries.md) | Root testing gates and canonical payload/producer owners; inherited tests, no new product contract | inherited SDK subset of004,006,008,009,010,011 |

## Fresh integrated verification

The root ran `mise run -c gate` once on the immutable target, sequentially,
without source or test edits. It completed in 3760.75 seconds, exit 100: six
tasks failed (Wyrd family, Bifrost/MCP, Rust SDK, Python integration,
TypeScript integration, local-dev). Full details and raw-log identity are in
[verification.md](verification.md); no environment values are reproduced.

| Relevant fresh slice | Observed result |
|---|---|
| Format, Clippy, dependencies, tenant/storage/CI boundaries, Rust docs, examples, codegen | Passed |
| Wyrd / Skald / Vala / shared | 2,418 passed, 1 failed / 335 passed / 1,176 passed / 819 passed |
| Bifrost/MCP ignored journeys | 266 passed, 8 failed |
| Rust SDK full journeys | 136 passed, 2 failed |
| Python integration | 331 passed, 1 failed, 2 skipped |
| TypeScript integration | 257 passed, 2 failed, 3 skipped |
| Storage backend/local/permission journeys | S3 multipart, GCS single PUT, Azure multipart, local roundtrip and raw-path/bad-byte/permission negatives passed; emulator handle CRUD passed |
| Installed CLI | Six Rust journey selectors passed; guided Python setup fails at the post-exit slave observer before later inner commands run |
| Exact installed CLI guided diagnostic | Same ENOTTY slave observer failure after exit130 (1.13 seconds) |
| Exact eleven Rust diagnostics, fresh Postgres | Ten failures reproduced; group source-occurrence/query-count journey passed when no deferred input-read occurred |
| Exact TS policy/staging diagnostics, sequential files | Policy review every action/check passed; staging exact seven-versus-six failure reproduced |
| Exact Python staging diagnostic | Fixture support declaration fails again before pressure assertions (7.61 seconds), same no-longer-pending compensation outcome |

The focused group PASS supports FIND-CHANGE-010's timing diagnosis: the full
gate's trace explicitly records the one lawful visibility deferral absent in
the passing focused run. It does not erase the failing global-count assumption.
The TS policy journey's full-gate Busy could originate in hosted workspace or
Python-worker admission, since service maps EngineError::Busy to that message.
Its typed underlying reason was not retained in the full failure display and
its exact focused run passes. No concrete producer defect or corrective source
change is established. This remains a verification limit; no blanket retry,
worker-limit change or invented finding is proposed.

## Verification limits and remaining handoff

The fresh aggregate failed as recorded above. Actual gate test selection
includes the test-support scenarios through the family feature union: hosted
Artifact consumers, group selectors, File readiness/exhaustion and fitter
recovery are already observed passing in the raw log. Canonical no-payload-IO
readback, File origin/reuse, sole-inventory and canonical payload-owner scenarios
also passed. No duplicate focused suite
is necessary when the aggregate supplies these nonzero selected tests.

Existing evidence explicitly lacks real-GCS multipart and provider-emulator
File-sweeper abort; other emulators cover shared S3 multipart, GCS single PUT
and Azure block blob, and File sweep dispatch reaches the existing abort owner
in source/local tests. No new backend implementation is introduced. These
limits must remain explicit; they are not reported as passing live-cloud proof.
Current environment has no GCS credentials or configured `WYRD_STORAGE_URL`
live-storage target. AWS/Azure credential names are present; this report does
not claim their credentials are unavailable or that a live-cloud lane ran.
ACK isolation's queued-handoff proof is structural, public controlled delay is
not claimed. Snapshot/query availability can lag ACK; the memory outbox does
not acquire a new crash-repair guarantee. Timing is unmeasured under current
maintainer instruction; no speedup/latency claim is supported.

Completion is withheld until FIND-CHANGE-001–011 close on a new immutable target
and required focused closure proof and unexecuted journeys are reconciled.
Disclosed live-cloud/proof limits remain explicit; they do not authorize new
benchmark work or turn optional evidence into an additional remediation gate.
The current immutable review is complete. On closure, the completion
payload should preserve shared lifecycle/session ownership, sole Bifrost
inventory, ACK/query distinction and isolation, canonical verification owners,
tenant/permission rules, deletion dominance and recovery, fresh-install-only
scope, the maintainer's excluded benchmark work, the three accepted task
identities, base/target and remediation identities, and actual final verification
limits. The reviewed narrow architecture updates already carry the lasting shipped
contracts; the separate benchmark-experiments packet must remain active.
