# TASK-002-R1 Gateway system review

Subject: prior candidate `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24` to candidate
`52f69ed2da3846733be0d314c6627c95d308e8dd`; cumulative base remains
`451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`. Reviewed the actual latest diff and
relevant cumulative implementation. HEAD remained the candidate. This specialist
wrote only this report and performed no build or test execution.

## Boundary conclusion

**FIND-TASK-002-3 is closed in source and inspected consumer assertions.**
The prior completed-failure path now retains the preparation reason through the
snapshot, resolver, engine and response mapper. The checked-in journey exercises
a completed failed reload, repeat refusal, unrelated availability and recovery in
the same process. No additional blocking implementation defect or evidence gap
is proposed within this boundary. Root coordinates the current exact journey rerun;
its execution result belongs in verification.md and the reconciled verdict.

## Authority and method

Read the task-review skill, new subject, root AGENTS, applicable agent_docs
index/rust/errors/maintainer guidance, TESTING, approved spec revision 1, original
TASK-002 scenario 5, prior system report and validated finding, and R1 scenario 3
and handoff. Governing behavior is REQ-009 background preparation/lifecycle and
the explicit maintainer outage response, together with shared secret ownership
and exact context under REQ-003–005 and security posture Stored secrets. Reviewed
relevant security and Wyrd Gateway authority. There are no nested source rules
under the touched Gateway directories. Benchmarks are not required under current
root AGENTS/spec absent an explicit current request.

Focused source map: wyrd-gateway credential.rs, engine.rs, snapshot.rs and sibling
adapter error classifications; server components/gateway service.rs, snapshots.rs,
owner.rs, invocation.rs and ledger.rs; testing TransitFixture and compatible.rs
closure journey plus unchanged harness and resilience proofs. Also inspected all
new FailureClass/CallExecution consumers so closure does not merely stop at the
new resolver branch. Prior source-backed cache/listener and immutable admitted
snapshot coverage remains valid; this remediation does not replace those owners.

## Closure and affected behavior

| Promise / affected path | Source and actual proof checked | Assessment |
| --- | --- | --- |
| Completed failed preparation returns retryable 503 | service.rs:1021–1085 constructs each credential separately and sets its pending flag only when open returns KmsError::Unavailable. The aggregate retry flag still queues background retry, while per-credential pending survives in the installed snapshot. credential.rs:143–150 returns Pending instead of generic Unavailable. engine.rs:896–906 maps it to CredentialPending, and :855–860 marks a failed execution pending when no attempt was billable. invocation.rs:964–970 projects the existing ServiceUnavailable code/status. | Original defect corrected at the existing boundary. |
| No provider dispatch or false provider billing for pending credentials | attempt returns before dispatched.store or ProviderDispatch::dispatch. engine.rs:842–849 records this failure nonbillable; Accounted::new in ledger.rs:143–187 prices only billable attempts and releases unused token/budget holds under existing policy. invocation.rs:1476 derives dispatched from billable attempts; buffered settlement remains the original in-memory owner. | No new provider work or accounting wait; no charge for an undispatched pending attempt. |
| Deployment breaker and fallback | engine.rs:945–950 retains existing BeforeDispatch breaker behavior but routes CredentialPending straight to Next::Continue without record_failure. The plan walk proceeds to authorized fallback, including non-retry-safe operations because no provider has received the request. Success returns the normal finish result; provider refusal, timeout/cancellation and mixed dispatched failures preserve their prior response precedence. | Pending state neither poisons the deployment breaker nor prevents healthy fallback. Clear source branches establish these structural properties; adjacent resilience execution is recorded in R1. |
| Unrelated resources remain available | Pending state lives on the one GatewayCredentialSource; no tenant-global unavailable check is added to cached invocation. The closure journey configures acme-first with no credential and asserts 200 while the managed credential returns 503. | Existing resource-local behavior preserved and asserted through HTTP consumer. |
| Same-process recovery | snapshots.rs:360–368 retains install-before-requeue and retry signaling. owner.rs refresh_worker continues retrying failed reload passes. A later successful open builds a new snapshot with prepared secret and pending false. The journey restores the same Transit endpoint, invokes without a resource edit/restart, and checks actual upstream authorization contains the submitted key. | Required recovery path is now asserted, not inferred from restart. |
| Snapshot security/lifetime | No separate DEK or plaintext cache is added. Each fresh snapshot is prepared from its exact stored envelope and retains its SecretString inside the immutable admitted Arc. Failed reload does not copy earlier opened material; generation/listener fencing stays in existing install/reset/watch. | Prior lifecycle proof remains valid. |
| Cold path and warm no-extra-key-request behavior | load_through remains unchanged: cache hit returns memory, remote cold miss loads its one catalog snapshot without opening through remote KMS, queues reload and returns retryable unavailable. Managed resolve only reads the admitted snapshot. The existing warm request-count journey is unchanged and its rerun with closure/resilience tests is recorded in R1. | No remote dependency added to warm or cold invocation. |

The new execution boolean does not override a provider refusal or successful
fallback. It is set on final failed fallthrough only when at least one pending
credential exists and every attempt is nonbillable. Interrupted execution keeps
TimedOut/Cancelled precedence. The streamed completion path initializes the flag
false because opening a stream already required a successful dispatch. This
preserves the surrounding response/accounting contract rather than promoting all
upstream failures to a key-service 503.

## Consumer proof and execution evidence

`wyrd-testing/tests/gateway/compatible.rs:1092`
`failed_managed_secret_preparation_stays_retryable_and_recovers` starts the real
journey server/Postgres with TransitFixture and first proves the managed key is
usable. It then changes the same fixture to HTTP 500, triggers reload through an
unrelated deployment write, observes the remote decrypt request, and waits until
the old usable snapshot has been replaced. Two direct calls assert exact 503
`WYRD_SERVER_503_SERVICE_UNAVAILABLE`. Recorded upstream requests still contain
only the original successful call. A deployment that needs no credential returns
200. Restoring that same endpoint and invoking again succeeds; the provider sees
the submitted key exactly twice across initial/recovered use. The fixture's
availability toggle changes only HTTP responses, without swapping endpoint,
configuration, process or persisted credential.

The pre-fix RED in R1 scenario 3 records `left: 502, right: 503`; this corresponds
to the diagnosed old code path, not a fixture/build error. The recorded GREEN
selector covers the new failure case, cold miss, warm request-count test and
existing resilience paths: 12/12. R1 also records Gateway units 78/78 and server
Gateway library cases 87/87. This specialist inspected their assertions and the
changed production paths but makes no claim of independently executing those
recorded runs. Root is separately refreshing the exact new journey, and should
bind its result to this candidate before final acceptance.

The new test does not directly inspect internal breaker or billing counters.
Those narrow structural changes are visible in the engine/ledger branches above;
no disconnected mock or test name is used as a substitute for the actual required
503/recovery consumer proof. No extra test or new machinery is requested here.

## Added or moved wait audit

Compared with `c8cbfeeb0`, the Gateway remediation adds **no wait on PostgreSQL,
Scribe, object storage, Vault or KMS**, and moves none into invocation:

| Path | Current wait and authority |
| --- | --- |
| Managed credential resolution during admitted Gateway call | Snapshot memory lookup/clone only. The pending flag/error projection adds no IO, lock across await, client or runtime construction. |
| Warm snapshot load | Existing memory cache hit; no PG or remote KMS wait. |
| Cold/invalidation miss | Unchanged sanctioned single snapshot catalog statement; remote preparation remains queued rather than caller-awaited. |
| Background reload/retry | Existing remote unwrap after snapshot transaction commit. This remains background work and is not a new wait in R1. |
| Post-commit administrative warming | Existing facade open/reload behavior retained. Administration is outside the defined hot paths. |
| Accounting and capture | Existing pure/in-memory pricing and settlement; background persistence/outbox/capture ownership unchanged. The error class does not add a synchronous evidence/audit write. |

The cumulative TASK-002 wait inventory in the prior system report remains
applicable: remote preparation was moved into the existing background owner;
existing external secret references remain excluded from this encryption cutover.
This report does not impose benchmark qualification withdrawn by the maintainer.

## Process and limits

R1 scenario 3's recorded RED/GREEN supports TDD PASS for this corrective scenario;
original TASK-002 scenarios 2/3 remain historically VIOLATED. Current tests cannot
rewrite that history. Live AWS/GCP and Linux-only production startup remain the
maintainer's accepted CI limits. No source edits, broad reruns or live cloud
checks were performed by this specialist. No new findings are proposed.
