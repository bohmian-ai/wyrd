# TASK-006 Wave 1 Security, Tenancy, and Authorization Review

## Result

**PASS**

The immutable candidate preserves tenant isolation across observation activation,
continuous Eval input reads, media resolution, result publication, and retained
audit publication. The four prior security-relevant findings
`FIND-TASK-006-3`, `FIND-TASK-006-7`, `FIND-TASK-006-8`, and
`FIND-TASK-006-9` are closed. No material exploitable security, authorization,
tenancy, privacy, or audit-integrity defect was found in the reviewed boundary.

## Reviewed boundary

Immutable subject:

- base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- candidate: `3593bbc31273673f87159315fbf66a73562d3c99`

The review traced:

1. authenticated Eval observation admission through Scribe acknowledgement and
   tenant-RLS run activation;
2. continuous Eval observation and trace reads under the stored tenant SYSTEM
   principal and Oracle object authorization;
3. media URI scheme, tenant path, MIME/kind, effective-body size, and native
   provider-content handling;
4. dependency error handling through diagnostics, persisted run state, and the
   public run projection;
5. the closed SYSTEM result-table write matrix and exact Verifier Card scope;
6. audit attribution for allowed and denied Eval reads; and
7. the changed audit publisher's tenant concurrency, frozen-range replay, and
   settlement behavior.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Observation attribution and enqueue tenancy | TASK-006 Scenario 4; REQ-077; INV-015; `AGENTS.md` and agent rules for `TenantConn`/RLS | `verification/observations.rs`; `tables/eval/observations.rs`; Gate's first-commit hook; `VerifierRunQueue::enqueue_observation`; SQL inactive/cross-tenant tests | PASS — tenant comes from authenticated `AuthContext`, subject UID is re-derived from signed Card scope, and binding lookup/run insertion use tenant RLS. Replay invokes no second activation. |
| Internal Eval read identity and authorization | Approved spec revision 36; security posture principal/token rules; remediation `FIND-TASK-006-7`; Oracle authorization/audit rules | `EvalReadAuthority::resolve`; `BifrostReader`; `ScheduledQueryCaller`; `system_principal_id`; `continuous_eval_read_authority_fails_closed`; `continuous_eval_reads_ordered_bounded_trace_evidence` | PASS — the reader resolves the active stored UUIDv7 SYSTEM principal under tenant RLS, creates only two table-UID-scoped read grants, and uses Oracle's normal object decision and audit path. Missing identity, wrong tenant, and missing table scope fail closed. No user, credential, role, public grant, or general Bifrost grant is fabricated. |
| Media authorization and bounded bytes | REQ-131; AC-027; remediation `FIND-TASK-006-3`; security posture tenant-storage rules | `TenantMedia::resolve`; `tenant_path::{strip_bucket,validate}`; `StorageHandle::get_object_bounded`; resolver/storage/provider tests and foreign-tenant journey | PASS — backend scheme, tenant prefix, path shape, MIME/kind, metadata fast limit, and authoritative `limit + 1` streamed body limit precede base64/provider invocation. Cross-tenant, missing, unsupported, and oversized input is refused. |
| Sensitive data and error exposure | VerificationError secret-free contract; telemetry payload-safety authority; remediation `FIND-TASK-006-9` | `failure`/`failed`; `ReadError::outcome`; `TenantMedia::{refused,storage_failed}`; runner fixed-text failures; unit log capture and `continuous_eval_failures_publish_only_stable_errors` | PASS — persisted/public errors contain stable codes and fixed operation text. Media diagnostics omit URI, object key, backend error text, and sentinel values. Raw non-locator causes stay only in the explicitly protected structured diagnostic boundary required by the remediation. |
| Trace resource authorization and bounds | Bifrost tenant/query rules; remediation `FIND-TASK-006-8` | `BifrostReader::spans`; scheduled-query/Oracle path; ordered/bounded trace and overflow journeys | PASS — the query is tenant-authorized, has a closed event-time interval, total ordering, fixed `TRACE_SPAN_LIMIT + 1` sentinel ceiling, and rejects overflow before task/provider execution. |
| SYSTEM result writes | REQ-086; INV-015; security posture least-privilege rules | Gate `authorize_record_write`, `is_verification_result_table`, and frame Card-scope validation; recorded principal integration/unit evidence | PASS — only SYSTEM can write the closed result-table set, SYSTEM is denied every other table, and every result row must carry a UID-bearing Verifier inside the exact signed scope. The Eval read authority is separate and tokenless. |
| Audit attribution and durability | Security posture audit integrity; agent rules single audit path | `ScheduledQueryCaller::record_object_denial`; Oracle allowed decision path; SYSTEM read journeys; `AuditPublisher`, `TenantCycles`, and stalled-tenant/replay journeys | PASS — allowed and denied reads name the stable stored SYSTEM principal. Publication evaluates no permission and emits no recursive audit. Per-tenant frozen ranges, deterministic Scribe dedup, watermark settlement, and RLS transactions remain intact; one blocked tenant cycle does not consume another tenant's free publication slot. |
| Supply chain | Repository dependency rules | Candidate `Cargo.toml` and `Cargo.lock` diff | PASS — only existing workspace dependencies were wired into server/testing crates; no new external package, version, source, feature, or credential-bearing configuration was introduced. |

Applicable authority read: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-security-posture.md`, the reference router and its
architecture/evaluation/telemetry/OLAP slices, the approved specification
revision 36, original TASK-006, prior r1 verdict/security report/validated
ledger, and TASK-006-R1.

## Prior security-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-006-3` | The storage owner streams at most `limit + 1` retained bytes; `TenantMedia` rejects the sentinel overflow before encoding or provider work. The stale-metadata resolver test and storage backend matrix recorded in TASK-006-R1 directly cover the former check/use gap. | CLOSED |
| `FIND-TASK-006-7` | The fabricated UUIDv7 `User` path is gone. The stored tenant SYSTEM principal is resolved through RLS, carries exactly the two registered Eval-input table scopes, and is used by normal Oracle authorization/audit. Real journeys cover stable attribution across restart plus missing, wrong-tenant, and under-scoped refusal. | CLOSED |
| `FIND-TASK-006-8` | Trace reads now have lower and upper time predicates and a server-owned row ceiling with one overflow sentinel. The overflow journey proves no provider call. | CLOSED |
| `FIND-TASK-006-9` | Stable fixed public messages replace dependency `Display` text. Unit and real-server tests inject provider, locator, and SQL sentinels and prove public/persisted absence; media diagnostic capture also proves locator redaction. | CLOSED |

## Material findings

None.

## Positive controls

- Authenticated tenant and signed Card scope, not Arrow payload fields, select
  the tenant and observed Card.
- Tenant SQL uses `TenantConn`; no new handler-level `OperatorPool` escape or
  caller-selected tenant predicate was introduced.
- Oracle remains the authoritative object-level decision point and audits both
  allowed and denied internal reads under the persisted SYSTEM identity.
- Media URI text never reaches the provider; only validated native media bytes
  do.
- Result-write authority and Eval-read authority remain distinct least-privilege
  capabilities.
- Public and durable errors are stable and secret-free at the reviewed paths.
- Audit publication retains the single canonical staging-to-Scribe path and
  deterministic replay fence.

## Verification limits

- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; direct Git-object, source, caller, and test inspection was used.
- This time-bounded domain slice did not rerun the broad or Postgres-backed
  lanes. It inspected their exact test bodies and the command/results recorded
  in TASK-006-R1, including the SYSTEM read-authority journeys, media bounded
  read tests/backend matrix, stable-error journey, principals lanes, Bifrost
  server journey, format, lints, codegen, and boundary checks.
- No live cloud object store or external provider was contacted. The candidate's
  emulator matrix and local mock-provider journeys are the available evidence.
- The named candidate remained `HEAD` and unchanged throughout this review.
