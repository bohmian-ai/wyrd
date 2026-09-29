# TASK-002 r2 — Domain review: security, tenancy, and audit

Reviewer: fresh Wave 1 `domain-rev`. Immutable subject: base
`c8bb490ad814c0c7770cac33ed7779897ff776e4`, cumulative candidate
`a000c201ae86f584fd5b80349f375e087902fd78`.

## Reviewed boundary

I traced the cumulative TASK-002 security boundary through:

- registered Service principal projection, test-only credential issuance, API-key
  exchange, signed Card-scope minting, and the SDK journey credentials;
- immutable Run/Card correlation, dynamic-table description, client caching,
  Gate admission, Scribe scope validation, and server stamping of tenant,
  publisher, and subject identity;
- reserved, unknown, and under-privileged table-description refusals before
  queue admission, including their stable errors and canonical audit evidence;
- the single audit staging/publisher path, tenant-scoped freeze transaction,
  three-second chain-head lock timeout, rollback, retry, and publisher error
  classification; and
- the `test-support`-only audit-publication suppression seam used to preserve
  staging evidence during the denied-describe journey.

Primary authority coverage included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, the
reference router and applicable architecture, OLAP/reliability, observation,
and testing references, approved specification revision 32, TASK-002,
`architecture/logic/{run_api,table_schema}.md`, the complete cumulative diff,
the TASK-002 r1 reviews/verdict/remediation packet, and the production owners
and tests cited below.

## Authority and obligation coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Authenticated tenant, publisher, and subject identity | REQ-118/121/145, INV-007; Wyrd security posture; Bifrost table/row identity | **PASS.** Tenant and publisher come from the verified principal. Present Card correlation is authorized against the signed UID-bearing scope before Scribe stamps `card_uid`; caller rows cannot author managed tenant, publisher, or subject columns. The remediation does not widen this path. |
| Table-description authorization and non-disclosure | REQ-127/128/145, AC-025; `wyrd-server/src/bifrost/service.rs:226-268` | **PASS.** `bifrost_table:read` is checked and audited before namespace parsing or catalog access, so an under-privileged caller cannot distinguish valid, invalid, built-in, or dynamic names. The audit tenant is the verified caller tenant. |
| Unknown, reserved, and unauthorized refusal before admission | REQ-128, AC-025; `wyrd-client/src/bifrost/facade.rs:316-330`; SDK journeys; `pg_bifrost_e2e.rs:2577-2615` | **PASS.** All three SDK journeys now reach the real server for a fresh unknown `vala.datasets.*` name and assert `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`. The under-privileged real-server case asserts `WYRD_PERMISSION_403_DENIED_RBAC`, one denied `vala.bifrost.describe` row naming the resource, no cached table, and zero producers. A failed describe returns before cache insertion. Reserved names remain locally refused. |
| Canonical authorization audit | REQ-145, INV-007; agent rules single-writer and same-decision-transaction rules; `wyrd-server/src/audit/mod.rs:195-237` | **PASS.** The describe route reuses the canonical fail-closed audit owner. A denial commits through the caller tenant's `TenantConn` before the public refusal; inability to append denies the operation. The test-only publisher pause changes retention timing only and creates no alternate audit writer. |
| Audit freeze timeout and caller-owned transaction semantics | Agent rules `TenantConn` lifecycle; Bifrost audit contract; `vala-sql/src/queries/audit_staging.rs:199-286`; `wyrd-server/src/audit/publication.rs:229-240,340-367` | **PASS.** PostgreSQL `55P03` is no longer converted to `Ok(None)`. It propagates as `SqlError`, the borrowed transaction rolls back on drop, `publish_tenant` cannot report `Idle`, and `publish_logged` emits the existing structured failure warning while the next sweep retries the unchanged range. |
| Test-support production isolation | Security posture production composition; `wyrd-server/src/app/server.rs:620-634` | **PASS.** Audit-publication suppression is compiled only with `test-support`, defaults off, and is selected only by the test-server builder. The production branch always constructs the publisher from application state; no route, environment variable, or production configuration exposes the switch. |
| Secrets and dependencies | Security posture secret rules; cumulative manifests and lockfile | **PASS.** Test API keys are generated, stored only as hashes, and returned only by explicit test-harness credential methods. No credential is logged or committed. Manifest changes use already workspace-pinned dependencies; the lockfile adds dependency edges but no new third-party package/version. |

## Prior-finding closure

| Finding | Source closure | Proof closure | Result |
|---|---|---|---|
| `FIND-TASK-002-9` | `freeze_publication_range` keeps the transaction-local three-second timeout but directly maps the timed-out `FOR UPDATE` error to `SqlError`; the publisher maps it to `AuditPublicationError::Staging` and never reaches its idle branch. | `held_chain_head_times_out_explicitly_and_retries_unchanged` holds the tenant head beyond the timeout, observes an explicit error, proves no in-flight bound was committed, then releases the holder and freezes the unchanged `(1, 1)` range in a fresh transaction. Independently rerun: **1 passed, 8 skipped**. | **CLOSED** |
| `FIND-TASK-002-12` | The real SDK journeys add unknown dynamic-table calls. The shared `writer_table` owner inserts into its cache only after a successful server describe. The server authorizes and audits before catalog lookup. | Rust, Python, and TypeScript recorded journeys assert the real not-found code. `denied_describe_is_audited_before_admission` uses a real under-privileged principal and server, asserts the stable RBAC denial, exact denied resource/outcome evidence, no cache entry, and zero producers. Independently rerun: **1 passed, 16 skipped** after the migration proof passed. | **CLOSED** |

The narrower r1 validation is preserved: TASK-002 does not own AC-030's full
authorization matrix, and this review does not require object-specific table
ACLs that the approved architecture does not define. Expanding the remediation
to repeat Card-scope, publisher-stamping, cross-tenant, or every allow/deny
journey already assigned to AC-030 would be out of scope rather than additional
TASK-002 closure.

## Security aspects of all remediation findings

| Finding | Security/tenancy disposition |
|---|---|
| `FIND-TASK-002-1` | Existing `QueueConfig` re-export and configured startup add no credential or tenant selector; startup still uses the same authenticated client and fixed-table permission checks. |
| `FIND-TASK-002-2` | Exact ordered schema validation strengthens the pre-ingest trust boundary and does not accept or reinterpret managed identity columns. |
| `FIND-TASK-002-3` | Python rejects non-string top-level mapping/dataclass keys before `json.dumps` can change feature identity. Pydantic still supplies JSON text and Rust remains the durable validator. |
| `FIND-TASK-002-4` | TypeScript rejects omission/coercion, unsafe numbers, non-plain objects, and cycles before native admission; no dependency or executable deserializer was introduced. |
| `FIND-TASK-002-5` | Runtime-local OpenTelemetry IDs are correlation only, never authentication, authorization, tenant, Card, or request authority. Explicit IDs still win and malformed pairs fail validation. |
| `FIND-TASK-002-6` | The owner-local miss gate rechecks the cache after locking and caches only a successful authenticated describe; denial and cancellation leave no authorization result or producer behind. |
| `FIND-TASK-002-7` / `FIND-TASK-002-8` | Terminal lifecycle fencing and same-handle ambiguous retry prevent a replacement writer from stranding or replaying retained work under a new handle; they do not widen credentials or scope. |
| `FIND-TASK-002-9` | Closed as above; lock timeout is explicit, bounded, rollback-safe, and retried through the one publisher. |
| `FIND-TASK-002-10` / `FIND-TASK-002-11` | Documentation/import-only remediation changes no security boundary. |
| `FIND-TASK-002-12` | Closed as above; real unknown and RBAC-denied descriptions fail before cache/producer admission with stable errors and canonical denial evidence. |

## Finding proposals

None. No Critical, High, Medium, or Low security finding remains within the
reviewed TASK-002 boundary.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None.

### Positive Controls

- The describe permission decision precedes namespace validation and catalog
  lookup, preventing table-existence disclosure to an under-privileged caller.
- Denied describes use the canonical tenant audit path and fail closed if that
  audit append cannot commit.
- A describe result is cached only after successful authenticated metadata IO;
  denied and unknown results create neither a cached destination nor a writer
  producer.
- Tenant identity is always derived from verified credentials and passed to the
  tenant-qualified catalog/audit owners; table names and CardRefs do not select
  tenant state.
- Signed Card scope remains UID-bearing and is validated before managed
  `card_uid` stamping; `principal_id` and `data_tenant_id` remain server-owned.
- The audit timeout preserves the single frozen-bound/watermark protocol and
  converts lock contention into an explicit retryable failure rather than a
  false idle result.
- Test-only audit-publication suppression is absent from production
  composition and does not introduce a second sink or publisher.
- No secret, unsafe deserialization, command/SQL injection, path traversal,
  redirect, CORS, webhook, cryptographic, or production environment-variable
  surface was added in the reviewed change.

## Verification evidence and limits

Executed against candidate `a000c201ae86f584fd5b80349f375e087902fd78`:

- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run
  --locked -p vala-sql --test pg_audit_staging -E
  'test(=pg_tests::audit_staging::held_chain_head_times_out_explicitly_and_retries_unchanged)'`:
  **1 passed, 8 skipped**.
- `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run
  db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-client
  --test pg_bifrost_e2e -P journey --run-ignored=all -E
  'test(=denied_describe_is_audited_before_admission)'"`: migration proof
  **1 passed, 16 skipped**; denied-describe proof **1 passed, 16 skipped**.
- `git diff --check c8bb490ad814c0c7770cac33ed7779897ff776e4..a000c201ae86f584fd5b80349f375e087902fd78`:
  clean.

The candidate records the broader `verify:bifrost`, shared/Rust SDK, all three
language journey and unit/type lanes, codegen, boundary, format, and lint lanes
green. I inspected their relevant source and evidence but did not repeat those
broad suites. The under-privileged test calls the shared `writer_table` seam
directly rather than duplicating the same denial in all three SDKs; the three
public SDK journeys separately prove their real unknown-table projection. That
is the smallest evidence set required by FIND-12 and does not claim AC-030's
future full authorization matrix.

## Overall result

**PASS.** `FIND-TASK-002-9` and `FIND-TASK-002-12` are closed. The cumulative
candidate preserves credential-derived tenancy, signed Card scope, fail-closed
RBAC/audit behavior, and the one audit publisher; no exploitable security,
authorization, tenant-isolation, secret-exposure, or audit-transaction finding
remains in TASK-002.
