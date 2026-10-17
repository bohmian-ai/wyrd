# Security, RBAC, and tenancy domain review

Result: **PASS** for this domain. No material proposed findings.

## Immutable subject and reviewed boundary

Reviewed cumulative `f2a726c90edbf8956c7b91c843379175ee20eac0...de69cb69b68eced72f7d760681d27253e013f8a1` exclusively in detached `/tmp/wyrd-task005-review-de69cb69b`. Recent `99882cf82...de69cb69b` located remediation owners without replacing cumulative review. HEAD remained the candidate and the detached tree was clean. No `.codegraph/` exists there; CodeGraph was skipped. No current-round peer reports or mutable original-tree source were consulted.

Scope: authenticated admission, exact permissions and signed subject scope, evidence object isolation, accepted/delegated authority across deferred execution and protected tools, Oracle attribution/authorization, reserved writes, and caller-trace sampling/tenant routing. Resource admission, extraction correctness, provider conformance, and persistent-result correctness have other domain owners; this review assesses their authority/tenant seams.

Authority: frozen AGENTS.md and agent-rules; reference router; spec-driven-development, maintainer-style, architecture-constraints, agent-harness and telemetry-observations references; wyrd-security-posture; applicable identity/verification/trace/storage sections of wyrd-design, wyrd-doctrine and bifrost-design; approved spec r9 REQ-007/010–015, INV-003/004/007, AC-005/006; original TASK-005; prior security report, findings-validation, r9 verdict, R1 and queued-trace follow-up. Prior findings are closure hypotheses. The owner says nothing has shipped: no upgrade migration or historical-catalog requirement is introduced. Draft agent-runtime-composition is not approval.

## Source coverage and assessment

| Boundary / obligation | Candidate source and traced consumers | Assessment |
|---|---|---|
| Verified caller and shared HTTP/MCP admission | `components/auth/caller_extractor.rs:45–104`; VerificationControl; verification routes; `mcp/verification.rs:129–242` | Caller derives principal, tenant and chain from verified authentication. Tool arguments cannot supply trusted authority. HTTP/MCP converge on VerificationControl; discovery does not replace invocation checks. |
| Exact invocation and signed queued subject | `components/verification/service.rs:459,573–580,782–818`; existing Principal/CardRefScope owners | Exact Verifier RBAC precedes scope comparison and staging. Signed effective scope must contain queued subject UID. An editor actor cannot widen the subject. One allow/deny is staged without waiting. Explicitly preserved direct-execution scope exception remains unchanged. |
| Evidence staging and local upload | `service.rs:323–419,833–868`; `routes.rs:38–52,163–203`; `wyrd-storage/src/service.rs:510–575`; shared client File pin/upload | covers_operation admits exact UID grants for preparation; execute/enqueue still require selected UID. Local upload takes caller tenant, bounds body, verifies digest and atomically stores tenant-qualified content. Remote backends keep single PUT and do not mount local PUT. |
| Evidence consumers | `hosted/evidence.rs:121–272`; registry and Artifact inventory readers | Files read caller-tenant digest keys. Artifact reads reuse audited Card/inventory owners and validated tenant paths. Bounded reads check expected length, digest and sniffed format before use. No arbitrary cross-tenant object key comes from input. |
| Deferred authority, retry and scheduling | `verification/submissions.rs`; SQL `verifier_executions.rs:1095–1207,1940–2008`; `runner.rs:375–450,561–641`; `verification/authority.rs` | Submission persists accepted principal/chain without credentials. TenantConn owns insertion/claim. Caller::accepted restores accepted authority and SYSTEM changes actor only. Scheduling recovers owner authority in its tenant at acceptance; missing owner/SYSTEM has no fallback. No request-path SQL authorization replacement. |
| Tools and model/Card reads | `hosted/blocks.rs:105–154`; `hosted/tools.rs:198–313`; `hosted/registry.rs`; gateway caller/invocation | Each engine owns its caller/evidence/workspace. Exact tool invoke is audited before dispatch; Cards/Bifrost reuse domain owners. Models pass through existing governed gateway under accepted authority. Tool/model arguments cannot replace authentication. Filesystem/code tools retain invocation mounts and sandbox capabilities. |
| Trace/result reads and Oracle attribution | `hosted/traces.rs`; `hosted/mod.rs:24–40`; `query/service.rs:173–186`; query/scheduled; Oracle `mod.rs:744–778,3489–3510`; service status/result reads | Caller evidence/result reads use ordinary Oracle permissions. Each concurrent statement gets fresh request ID with unchanged principal/tenant/chain/executor. Oracle authorizes every resolved table before data reads; attribution does not replace permission principal. |
| Narrow scheduled read authority | `verification/authority.rs:83–195`; `verification/drift.rs:832–913` | Scheduled drift gets only registered observations table UID grants. Serving the accepted subject changes attribution/history, not authority; it cannot become general Bifrost access. |
| Reserved writes and peer seam | Gate reserved table predicate/test; `scribe_outbox.rs:258–320,890–935`; `grpc/capture_peer.rs:44–114` | Public Gate refuses execution inputs/results even for wildcard grants. Existing outbox owns internal writes and queues submissions only after acknowledged inputs. Private peer adapter accepts closed ScribeTable destinations, rejects SYSTEM_OWNER outside audit, and remains mounted on established mTLS peer boundary. |
| Deferred trace sampling/routing | telemetry `context.rs:111–232`; `export.rs:294–390,408–469,505–572`; runner trace/claim; gateway caller/invocation | Typed Cause stores caller sampling separately from operator choice. Attempt starts as independent linked trace with sink/link installed before start. Route tenant/pair comes from accepted authority and tenant SYSTEM, never cause IDs or attributes. Route keys contain server-generated span IDs as well as trace IDs. Added GenAI catalog names retain loop continuity and existing tenant-copy filtering. |

The existing owners suffice: VerificationControl, PermissionSet, audited registry/storage readers, gateway, Oracle table scopes and shared telemetry route/sink. No additional scope resolver, authorization store, query path, audit sink or transport is needed.

## Prior finding closure

| Prior stable finding | Independent source closure | Supporting proof inspected | Result |
|---|---|---|---|
| FIND-TASK-005-1 (SEC-01) | enqueue supplies subject UID to authorize_run. Signed effective scope is checked synchronously before Submission/staging and denial uses shared audit. Direct and scheduled behavior preserved. | `pg_verification_routes.rs:546–630`: own subject succeeds, unrelated Card-bound and delegated editor-actor requests fail, audited decisions and queue count, unbound caller succeeds. | CLOSED |
| FIND-TASK-005-4 (SEC-02) | Both evidence endpoints call authorize_evidence using covers_operation(Verifier, Run); exact selected UID still checked later. | `pg_verification_routes.rs:670–794`: exact-grant plan/upload, wrong digest, repeat upload, direct File judgment, enqueue acceptance, different Verifier denial, no-grant audited refusals. | CLOSED |
| FIND-TASK-005-6 (SEC-03; r9 revised bounded authorization) | r9 approves local PUT; conditional publication and caller-tenant/digest/bounds/storage semantics match it. Shared evidence admission closes remaining exact-grant gap. | Same local test; served OpenAPI/conditional router source; r9 revision history. | CLOSED |

No other prior finding is reopened as a security defect. Unshipped deployment changes do not require compatibility migrations; resource-capacity findings are not new authorization requirements.

## Verification limits

Static review only. No builds, broad gates, live providers, credentials, source edits, or test edits. R1 records focused red/green and passing module/SDK checks; these are supplied results, not fresh runs. Inspected queued sampling tests at `pg_verification_runtime.rs:3030–3163`, reused inbound-trace tenant isolation at `:3506` onward, and three SDK submitted-trace journeys support the shared routing mechanism.

Two tenants queueing under the same external cause ID were not independently executed here. Isolation follows accepted-authority routing and fresh attempt IDs, supported by the existing same-inbound-trace test. Exact-grant route proof demonstrates queued File acceptance, not its completion; broader File/submit journeys and recorded R1 checks supply that separate behavior evidence. No cloud infrastructure was exercised; plan authorization precedes backend selection in the shared helper. No universal sandbox or compromised-peer resistance is claimed.

## Proposed findings and result

Proposed ledger: empty. **PASS** for security/RBAC/tenancy of the cumulative candidate, subject to the stated proof limits. This domain report goes to independent validation; it is not an overall task verdict.

Skipped fresh runtime/cloud checks and unrelated provider/extraction audits. Remaining risk is the recorded proof limit; no reachable authority or tenant escape was identified.
