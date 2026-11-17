# Security, RBAC and tenancy review

Result: **PASS**. No material security-domain findings proposed.

Subject: base `43c77a219fd10554e8c8663e980788b70f1a6296` to candidate
`c233b75456fad4f45c772ec242a9d115b4d35c6b`, complete first-review range.
HEAD still matched the candidate at the end of source inspection. Source was
not edited. Review inputs were the original TASK-003A packet, approved spec
revision 4 (REQ-015/AC-010), finalized status-event design, common review scope,
actual Git diff and owning source. No other reviewer reports or implementation
completion summary were used.

## Authority and coverage

The governing requirements are REQ-015's normal managed tenant/subject/actor
attribution and unchanged queue/lease behavior, the task's requirement to
preserve permissions and tenant isolation, root AGENTS.md, SQL AGENTS.md, and
`architecture/wyrd-security-posture.md`: principal lifecycle (including accepted
execution authority), permission checks, tenant/data isolation and Postgres
tenant binding, and audit integrity/privacy. CodeGraph exploration preceded
source navigation.

| Boundary / obligation | Source evidence and assessment | Result |
|---|---|---|
| Principal revocation retains its authorized tenant and non-enumerating kind selection | `server/src/auth/revoke.rs:77-131` still authorizes/audits `service_accounts:write` before mutation, derives its transaction tenant from the verified principal, rolls back unexpected failures, and stages only after successful commit. `auth/src/revoke.rs:51-99` retains User family locking, suspension and refresh-family revocation, and the service-kind selector/refusal. The new return value contains cancelled execution records, rather than changing authority. | PASS |
| Cancellation remains fenced and tenant-bound | `sql/src/queries/verifier_executions.rs:620-626,3294-3328`: the principal UPDATE retains the subject-or-actor predicate, unfinished-status filter, lease expiry clearing and terminal transition; only RETURNING/decoding changes. Methods continue to accept `TenantConn`, with no tenant predicate or operator bypass added. `server/src/components/verification/service/inspection.rs:419-448,1020-1065` retains coarse admission followed by exact Verifier Run authorization/audit against an execution resolved in the caller's tenant; only an applied cancellation stages a row. | PASS |
| Delayed submission, scheduled/group admission and PR reservation cannot escape revocation | `sql/src/queries/verifier_executions.rs:2046-2184,2244-2302,2804-2863` still fences inserted subjects/actors with the existing share-locking inactive-principal SQL before caller commit. The cancellation records now propagate through `Admitted`. `server/src/verification/submissions.rs:250-348` aggregates all affected records within the tenant batch, commits, then stages. `sql/src/queries/verifier_executions/pull_requests.rs:760-841` retains the PR reservation fence and accepted authority; `server/src/components/github/pull_requests.rs:293-355` carries both supersession and principal-fence outcomes through its existing commit. | PASS |
| PR lifecycle/access-loss cancellation preserves existing authority and worker fences | `sql/src/queries/verifier_executions/pull_requests.rs:99-153,692-739` preserves UPDATE filters and mutations and adds returned identities; settled history is separated from newly cancelled work. `server/src/components/github/service.rs:357-450,725-757,801-814` keeps disconnect authorization, installation locks and access-change transactions; cancellation publication occurs only after their commit. `components/github/pull_requests.rs:266-355` similarly stages deactivation/supersession only after commit. | PASS |
| New status rows retain each execution's attribution without caller-controlled tenant or content | `sql/src/queries/verifier_executions.rs:1001-1035` decodes identities and attribution from the updated execution row. Runner status producers use the claimed run's stored attribution (`server/src/verification/runner.rs:1197-1214,1516-1531`); cancellation producers use returned/stored attribution. `server/src/scribe_outbox.rs:590-623,2244-2261` sets only the specified identity/status/time fields, nulls other payload/correlation, and stages under the commit owner's tenant. It does not attribute work to the revoking administrator or replace the accepted actor with transport identity. | PASS |
| Existing managed Scribe envelope protects the new shared-frame payload | `server/src/scribe_outbox.rs:572-576,1500-1560,1744-1785` routes Status through ordinary rows, retains per-row owner pairs in mixed-principal frames and appends the pair columns. `into_frame:918-967` binds authenticated and transport-principal tenant to the outbox tenant, with row-carried attribution and the reserved internal transport principal. Unchanged `vala-bifrost-redux/src/scribe/ingress.rs:55-88` rejects tenant mismatch/system-owner misuse; `scribe/execution_lanes.rs:1470-1500,1550-1594` validates principal columns and stamps managed values. No new public writer or ingest exemption is introduced. | PASS |
| New rows do not widen public query authority | The public route still uses `server/src/query/service.rs:80-98,171-193,212-230`: Bifrost capability admission, verified principal/tenant context, and retained object-denial audit. Unchanged `vala-bifrost-redux/src/oracle/mod.rs:3607-3640` checks every tenant-bound resolved table against exact Bifrost query permission before data reads. Status rows use the existing ExecutionEvents table and physical tenant binding; no endpoint, grant, policy, registry lookup bypass or change to query consumers is added. Reading these rows remains governed by table query authority, rather than a new implicit Verifier permission. | PASS |

Paths abbreviated with `server`, `auth`, and `sql` above are respectively
`crates/wyrd/wyrd-server`, `crates/wyrd/wyrd-auth`, and `crates/wyrd/wyrd-sql`.

## Proof assessment

Source inspection checked the assertions, rather than relying on test names:

- `wyrd-testing/tests/bifrost/server/verification_runtime.rs:3687-3743`
  compares the publicly queried cancellation row's subject/actor to the stored
  queue pair, checks null content/correlation, and asserts repeat cancellation
  leaves one event.
- The same file at `3854-3911` cancels running work and revokes a submitter with
  running and pending executions, releases the workers, checks retained
  cancellation histories and verifies neither fenced worker published a result.
- At `3929-3966`, a held input acknowledgement delays admission until after
  principal revocation; assertions require a cancelled queue row and exactly one
  cancelled event.
- `wyrd-sql/tests/integration/pg_verifier_executions.rs:4542-4575` freezes PR
  authority before revocation, reserves afterward, and asserts that the
  principal fence cancelled the reservation and returned the stored identity
  and attribution. Existing queue tenant-isolation proof is at `2250`.
- `server/src/scribe_outbox.rs:3582-3660` checks different per-row attribution
  pairs survive shared framing and the same event/frame identities survive
  retry/redelivery.

These are credible focused assertions for the changed security seams. They
are static proof assessment, not an independent claim of execution. No Cargo
command was run by this reviewer; exact-candidate execution and owner-lane
results remain the orchestrator's responsibility. No additional focused check
was requested beyond the planned owner lane. No new standalone cross-tenant
status-row journey was executed; unchanged RLS, frame tenant binding and exact
query permission owners were traced directly. TDD history and broader lifecycle,
performance, resilience and repository-gate compliance remain assigned to the
other review roles and the final validator.

At handoff, the orchestrator reported `fmt:check`, tenant-isolation and
`git diff --check` passing, and the owner lane blocked before tests by Docker
socket denial. The outbox unit check was still compiling. Consequently this
domain PASS is a source-review result with runtime verification unavailable,
not a claim that the integration lane passed on the candidate.

## Findings and limits

Empty proposed finding ledger. The cumulative diff preserves the relevant
permission decisions, non-blocking audit staging, transactional cancellation
fences and tenant capability boundaries while adding postcommit status
projection. Process loss can still lose staged status rows, as REQ-015 explicitly
accepts; these rows provide no authority and cannot reactivate cancelled work.

Skipped dynamic security testing and Cargo checks in this role; no observed
security risk beyond the approved best-effort status-delivery limit.
