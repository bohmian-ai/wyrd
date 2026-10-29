# TASK-002 maintainer review

Result: **FAIL**.

Subject: base `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`, candidate
`5ef2981e66068e8ee5f96d3be15000eea9cbd94f`. First review of the complete
base-to-candidate change. HEAD remained the candidate. No prior TASK-002
acceptance was carried forward and no other reviewer report was consulted.

## Authority and scope

Reviewed the root and affected SQL/Python/TypeScript/docs AGENTS instructions,
`changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, the
applicable Rust owner guidance, approved specification revision 2, and original
TASK-002 obligations independently of its implementation-evidence claims.
Applicable architecture was the Verifier evidence lifecycle in
`architecture/wyrd-design.md:1687`, the storage inventory/internal read contract
in `architecture/bifrost-design.md:1755`, and permission/tenant binding in
`architecture/wyrd-security-posture.md:506` and `:700`. No `.codegraph/`
directory exists.

This specialist covers ownership, abstraction reuse, discoverability, types,
caller/test clarity, and the maintenance consequences of the consolidation.
TASK-003's broader canonical readback and execution-wide metadata reuse remain
excluded. The finding below concerns a new query introduced by TASK-002's
immediate File byte reader, rather than requesting that later work early.

## Changed-surface coverage and reuse evidence

| Surface inspected | Callers, tests, existing alternative, and assessment |
|---|---|
| `wyrd-storage/src/service.rs`: `UploadOwner`, `upload_owner`, `UploadSubject`, `upload_init`, `file_upload_init`, `init_upload`, `persist_upload_init`, `claim_upload_init` | Followed Card HTTP initialization and `VerificationControl::evidence` into the same planner/backend initialization/session persistence. `UploadSubject` carries the formerly Card-only request fields without adding a second durable workflow. File initialization supplies ownership and digest representation, then delegates the complete workflow. Existing `StorageServiceState` and lifecycle stages are reused. Nullable Card ownership reaches the SQL row/insert types, null-safe dedupe, and fresh migration. |
| Storage continuation, `row_path`, pending-session replacement and completion | Read the changed `upload_part_url`, `upload_complete`, `reconcile_upload`, `upload_abort`, `upload_local_blob`, replay, prior-abort, insert and persistence paths in their owning module. `row_path` is the shared discriminator required to accept File namespaces without weakening Card validation. Existing local size/digest verification and completion size/encryption checks survive. Traced Card reconciliation's existing `verify_card_manifests` sibling through the same storage completion and inventory recorder. No second completion engine was added. |
| `tenant_path::{evidence,validate_evidence,validate}` | The existing constructor cannot validate an arbitrary recorded location; the small exact-shape validator is necessary for persisted File paths. It reuses typed tenant/digest parsing and the existing constructor. Unit cases cover own/foreign tenant, shape, case and Card-path refusal. No new generalized path framework. |
| Storage HTTP owner selection and local body admission | `authorize_upload` centralizes persisted ownership selection for part URL, local transfer, completion and abort. Card permission remains on the existing audit helper; File permission reuses `VerificationControl::authorize_evidence`. The local handler uses declared size before the shared local writer. Followed middleware exemption and router projection. The ownership lookup and operation lookup are distinct authorization/behavior boundaries; their existence alone is not an unearned duplicate workflow. |
| `ObjectInventory::{card_objects,file_object,rows,file_fold}` | Read both existing Card folding and new File selection. The shared query transport/authority remains in `rows`; File folding does not fabricate Card-relative metadata or transfer Card lifetime. The File-specific digest/size and path-deletion selection earns its small fold. Compared existing `SystemReadAuthority` and cached verification target identity; the added unconditional identity lookup on execution reads is MAINT-001 below. |
| `VerificationControl::{evidence,authorize_evidence}` and `TenantEvidence::{read,file_object,stored}` | Planning retains limits and operation admission, consults the inventory, and delegates session initialization. Hosted evidence replaces guessed locations with inventory paths while retaining bounded length/digest/format checks. Followed extraction, media and citation consumers and existing Artifact siblings. Repeated evidence resolution outside this immediate change is not used as a reason to demand TASK-003 now. |
| `wyrd-client` `RunEvidence::pin`, `WyrdStorageClient`, upload dispatcher and sources | File paths and memory now call existing `upload_artifact`, including common multipart part minting, backend dispatch and completion. Removed `upload_evidence` was the restrictive second orchestration path. Read the full shared upload workflow and `ArtifactSource`/`FileSource` shape. The Artifact-oriented existing name does not justify a new wrapper or rename. The mock test asserts both single-PUT and multipart completion payloads; public language journeys cover path and memory inputs. |
| Contract and thin projections | Inspected `EvidenceUpload.upload_id`, response documentation, removed local evidence handler/router/OpenAPI entry, MCP descriptor/guide, reference docs and generated llms text. Python `src/verify.rs` and TypeScript native verification inputs already wrap shared Rust File values, so no extra native type/export/stub shape is needed for this internal planning response. File and ArtifactRef authoring shapes stay unchanged. |
| Tests and fixtures | Inspected new route helpers and V1/V2/V3 assertions, relocated five hosted File tests and their shared upload fixture, Artifact fixture visibility adjustment, backend journey extension/registered Card fixture, Rust/Python/TypeScript File journeys, MCP planning/HTTP transfer/execution helper and its existing FrameAgent caller, CLI declare/pull regression, OpenAPI assertion, SQL fixture changes and integration module registration. The hosted tests were moved to a real server/inventory seam rather than silently removed. The MCP helper deliberately exercises the emitted local transfer contract and shares two test callers; it is not a production second dispatcher. |

## Material finding

### MAINT-001 — File reads re-query SYSTEM identity already supplied by the execution owner

Classification: **VIOLATION**.

Changed locations: `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:205`
and `crates/wyrd/wyrd-server/src/components/storage/inventory.rs:312`.

Governing obligation: approved specification **REQ-014**, “File inventory
resolution adds no application-level Postgres lookup or write.” Its exceptions
cover existing target-cache misses and Bifrost catalog/fence SQL, not an added
application query to `wyrd.auth_service_accounts`. The maintainer guide also
requires extending the existing owner rather than making callers repeat its
workflow.

Reachable producer-to-consumer path:

1. `VerificationControl::run_direct` resolves the execution target and takes
   `target.system_principal` at `components/verification/service.rs:526`.
   `verification/cache.rs:65,94` already retain that identity with the cached
   target. At `service.rs:534`, `Caller::executed_by_system(system)` attaches it
   to the execution caller.
2. `verification/hosted/blocks.rs:137` constructs `TenantEvidence` with this
   caller. Thus normal direct executions, including a warm target-cache hit,
   already carry the tenant SYSTEM identity.
3. The newly changed File branch of `TenantEvidence::read` calls its new
   `file_object`, which invokes `ObjectInventory::file_object` without that
   identity. `file_object` enters `rows`.
4. `ObjectInventory::rows` unconditionally acquires `registry_tenant_conn`,
   executes `system_principal_id`, and commits (`inventory.rs:312–321`). The
   query body at `wyrd-sql/src/queries/auth/service_accounts.rs:305` is an
   application SELECT against `wyrd.auth_service_accounts`. Only afterward
   does it call the existing `SystemReadAuthority::resolve` table-scoped read
   mechanism.

The base File reader used storage directly; it did not execute this identity
query. The query was previously reached here only by Card inventory readers.
Reusing that complete helper therefore adds SQL to every File resolution even
when the target and Bifrost table identities are warm. New planning also uses
this helper; that caller has no selected target, so its authority acquisition
must be accounted for separately rather than assuming the execution identity
is available there.

Consequence/materiality: supported File verification now adds a mandatory
Postgres acquisition/read/commit and failure dependency per File resolution,
despite an already resolved identity and the explicit hot-path constraint.
Multiple hosted consumers can repeat it. This is an explicit required boundary,
not a speculative performance optimization or a request for a new cache.
Existing output/journey assertions do not detect additional SQL.

Smallest correction boundary: keep `ObjectInventory` and
`SystemReadAuthority::resolve` as the existing query and table-authorization
owners, and let execution File resolution consume the SYSTEM identity already
carried by its accepted execution caller instead of fetching it again.
Preserve caller domain authorization, tenant matching, narrow table grants,
read auditing and typed unavailability. Do not replace SYSTEM authority with a
caller-wide grant or add another inventory/query implementation. This requires
bounded identity plumbing at the existing owner, not a new execution cache.
The evidence-planning caller's identity source must also be explicitly checked
against REQ-014; this report does not authorize a new cross-owner cache or a
relaxation of that requirement if no existing source is available.

Focused closure proof: exercise a real File verification with a warm target
and registered inventory table and show that File inventory resolution issues
no `system_principal_id` application SELECT, while File reads still succeed
without general Bifrost query permission and foreign tenant access remains
refused. Account separately for allowed target misses/Bifrost catalog SQL and
for the planning path. No new generic query instrumentation framework or broad
suite is required by this finding.

## Verification and limits

Static review compared the recorded Git range and current candidate source,
expanded callers with repository search, and checked HEAD. Review-time
`mise exec -- git diff --check 15f939fc9...5ef2981e6` passed. No Cargo command,
backend emulator or credentialed test was run by this specialist; implementation
verification listed in the task remains a claim, with its assertions inspected
as described above. Real GCS multipart has no independent runtime proof here.

No additional material owner/abstraction finding was identified. Broad
repository-rule compliance and security/data acceptance remain the other
selected reviewers' scopes; this report does not substitute for their reports.
