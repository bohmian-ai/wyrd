# Security and tenancy review

Result: **FAIL** (one bounded proposed finding, SEC-001).

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
Candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
Approved authority: `changes/active/bifrost-source-consistency/spec.md`, revision 2; original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`.

Reviewed the cumulative base-to-candidate range, rather than the latest evidence commit, for the security/tenancy boundary. The report covers changed capture authority, peer context and assignment propagation, authorization ordering, release authority, and SQL tenant isolation. Other changed surfaces (capacity reporting, workflow/draft-spec prose, documentation, and the test-only span-permit correction) do not introduce an additional executable security boundary in this domain audit. They are not used as substitutes for the implementation review.

The candidate HEAD was rechecked and remained the recorded commit. The only working-tree additions were review reports. No other reviewer's report or conclusions were read.

## Governing authority and source coverage

| Boundary | Applicable authority | Source and caller coverage | Assessment |
| --- | --- | --- | --- |
| Capture identity | Spec INV-002, AC-002; task scenario 2; Bifrost design source selection; AGENTS tenancy/server rules | `scribe/tail_rpc.rs`: wire binding, capture, retain, open, release; `ScribeImpl::tail_service`; `memtable::LiveCapture::take/read` | Query, tenant/table and incarnation checks present; deadline issue SEC-001 |
| Public authorization before discovery | Security posture Authorization and policy; Bifrost admission/read sequence; agent rules | `OraclePlanner::read_and_materialize` authorizes all resolved identities before materialization; `Oracle::prepare_query_attempt`, `classify_one_build`, `build_physical_root`, `list_live_routes`; follower resolver | No new path from unprivileged public caller to a foreign capture found |
| Private discovery and release | Security posture Peer identity and distributed Oracle; Bifrost private-plane contract | Server `grpc/scribe_tail.rs`; `RegistryTailStreamDiscovery::{discover,list_one,list_local,admit,release}`; tonic transport and protocol conversion | Existing mTLS-only trusted-member model retained; release additionally matches query and full table binding |
| Fragment authority | Spec INV-002; peer context receiver validation; no mixed-version compatibility requirement | `ScribeProviderCut`, `assignment_authority.rs`, proto and conversion, `LiveScribeExec` assignment construction, `ScribeFragmentExecutor::execute`, `AuthenticatedFollowerContext`, `ScribeTailResolver::resolve` | Capture ID included in v10 assignment digest; checked query ID reaches the source; epoch/table/projection checks remain |
| Tenant catalog revisions | Security posture Tenant and data isolation; agent rules SQL capability boundaries | New migration in full, trigger, cut function and restart function; `OracleTableCuts::read/group_table_cut`; `file_list_writer::insert_artifact_set_fenced/node_publication_revisions`; `ScribePublicationReconciler`, publication/startup callers | New table enables and forces RLS; tenant policy has USING and WITH CHECK; restart cross-tenant read is a named operator operation, with caller-owned node identity |
| Existing integrity enforcement | Spec technical boundaries preserve independent integrity checks; security posture tenant tripwire | Follower signed closure/schema resolution, `live_leaf`, `exec.rs` staged lease change, existing hot-Parquet tenant footer path | No deleted tenant footer or row-isolation check found |
| Negative and isolation evidence | Spec AC-002; task verification inventory; AGENTS testing workflow | Capture unit tests; `published::prove_unvalidated_captures`; peer-network fragment security fixture/tests; gRPC tail smoke changes; `file_list_publications_advance_their_node_revision` | Existing checks support the bindings above; expiry test proves eventual removal only |

Authority read included `AGENTS.md`, `architecture/agent-rules.md`, reference router, spec-driven development and maintainer-style references, security posture, applicable Bifrost/Wyrd design and doctrine context, OLAP serving and analytical operations/reliability references. CodeGraph was used before source navigation.

The peer contexts are **unsigned**, as explicitly specified by the current Bifrost design and `PeerContext` protocol: mTLS authenticates a cluster process, and context/body digests bind the receiver's interpreted operation. Task references to signed assignments do not create a new requirement for cryptographic signatures. Compromised cluster members are explicitly outside this model; this review does not propose additional machinery for that threat.

The existing workflow is extended at its established owners: Oracle's roster/discovery, Scribe's shared tail service and memtable/staged leases, the current assignment digest, and Vala SQL catalog functions. No parallel tenant identity resolver, transport, or permission decision was introduced. `CaptureTable` supplies storage for a newly required retained identity; the previous execution-time snapshot did not retain such an identity. The defect below belongs to that existing capture owner, not to another token validator or downstream query guard.

## Proposed finding SEC-001 — Capture expiry is timer cleanup rather than an enforced authority bound

- Classification: **INCORRECT**.
- Violated obligations: spec **INV-002** (deadline binds each capture; an expired reference cannot authorize a read), **INV-006** (bounded capture lifetime), **AC-002**, and task scenario 2 (“A capture reference is foreign, substituted or expired: Refuse access before reading its data”).
- Changed locations: `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:468` (`RetainedCapture`), `:805` (`retain`), and `:892` (`open_capture` lookup).

### Producer-to-consumer evidence and reachable consequence

`RegistryTailStreamDiscovery::discover` derives the capture expiry from the original query deadline and passes it through the private listing RPC (or local service). `FetchLiveTailService::capture_active_streams` converts that time to a monotonic deadline, then captures and sizes the table. That local capture/sizing work is synchronous; the deadline is checked when entering the method or waiting for unsettled publication, not again before retention. Normal capture work or scheduling can therefore cross the deadline.

`retain` starts an independent Tokio expiry task at lines 814–820, **before** inserting the retained entry at lines 821–829. The entry stores query, tenant/table, source references and the task's abort handle; it does not store the deadline. On a multithreaded runtime, if expiry is already due (or becomes due between spawn and insertion), the task can remove an absent ID, finish, and then the caller inserts the entry. No later expiry task remains to remove it. A late or lost listing response relies on expiry as its cleanup backstop, so that retained memory/staged lease can survive indefinitely without another release. This does not require a forged request, compromised peer, or synthetic host load.

Independently of that registration race, an entry whose deadline passed but whose expiry task has not yet been polled remains readable. `open_capture` checks the exact service stream, range ordering, query ID and full `TenantTableBinding`, then reads the capture at lines 903–906. It has no deadline available to reject the request. Therefore expiry depends on scheduling the cleanup task first. The production consumer is `ScribeTailResolver::resolve` → `LiveTailSource::open` → `open_capture`; peer context checks happen before async catalog/schema resolution and cannot supply the absent capture-local deadline check at the eventual open. A valid fragment can start before the deadline and reach that open after it. This finding does **not** claim the leader must accept a successful public terminal after its own deadline: its timeout checks may reject the result. The separate explicit requirement is that an expired source reference must not authorize the source read, and expiry must remain a reliable retention backstop.

All callers/writers of this retained authority were inspected: insertion is only `retain`; ordinary production opens go through the follower resolver; release is either the local discovery release or the private `ReleaseCapture` adapter, both using `release_capture`; expiry is the spawned removal; `ScribeImpl` supplies a process-shared table across tail-service instances. None stores or enforces the missing deadline elsewhere in that table. Foreign-query/table release protections are correct and should be preserved.

### Smallest correction and focused closure proof

Keep `FetchLiveTailService`/`CaptureTable` as the sole owner. Retain the monotonic deadline as part of the capture's authority, reject elapsed capture authority before projecting/reading its sources, and make retention publication and expiry registration safe against the removal-before-insert interleaving. Refuse a capture already elapsed before publishing its identity, with the existing failure vocabulary. Preserve open-read leases, query/table checks, stream checks, explicit idempotent release, and ordinary successful capture behavior. No new public contract, token scheme, background sweeper, dependency, or separate lifecycle owner is needed.

Extend the existing narrow capture tests rather than introducing another harness:

1. A retained capture whose clock has passed its deadline must refuse a new open even before the expiry worker gets another poll; use the owning paused-time fixture or an already-due retained deadline to make the scheduling boundary deterministic.
2. Deadline expiry during capture/retention must not publish a usable identity or leave a retained entry/lease behind after cleanup settles. Exercise the registration/expiry boundary deterministically, rather than relying on a repeated timing race.
3. Preserve the existing successful open, foreign query/ID refusal, foreign release no-op, explicit release, and opened staged lease survival checks.

`an_unreleased_capture_expires_at_its_deadline` currently sleeps for twice the deadline before checking removal, so it proves neither immediate authority rejection nor registration safety. `published::prove_unvalidated_captures` covers publication retry, injected unavailable fragment, cancellation and listing failure; it deliberately delegates expired-reference proof to that unit test and does not close this gap.

## Verification assessment and limits

The task records checks at `051be28a3` and final evidence at candidate, with reported Bifrost units, server/SQL/redux integrations, peer-network journeys, capture journey, codegen and tenant-isolation gates. Source review supports the existing non-expiry tenant and assignment protections. The added SQL integration explicitly checks a second tenant's revision independence; the migration's forced RLS supplies the actual enforcement, rather than merely relying on a green scanner.

No Cargo, benchmark, live server, or adversarial network execution was run in this static domain pass, as directed. The proposed expiry interleavings are source-derived and require the focused closure proof above. No claim is made that existing passing tests reproduced them. Capacity acceptance and unrelated range contents remain for the other required reviewers and final validator.

No cross-tenant read/write regression or assignment-substitution path was found in the changed source. The retained finding is a capture-local deadline/lifetime failure; independent validation must confirm or reject it before the orchestrator includes it in the final ledger.
