# TASK-002 system boundary review

Reviewed base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a` to candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`, against approved
`SPEC-stored-secret-encryption` revision 1 and TASK-002 scenarios 3–5.
Scope: Gateway secret preparation, cache/reload/admission topology, failure and
recovery, shared owner lifetime, and changed hot-path waits. This is independent
discovery; findings require separate validation. Source remained immutable.

## Boundary conclusion

The common crypto path, background preparation, warm memory-only secret
resolution, generation/listener fencing and snapshot lifetime are present.
One concrete failure-response defect remains: after a background unwrap has
failed and installed its unprepared snapshot, calls receive 502 rather than the
explicitly promised retryable 503. Proposed finding SYSTEM-001 below.

## Authority and paths inspected

Read the review skill, subject, root AGENTS.md, relevant spec/task sections,
changes/spec-driven-development.md authority/scope rules, TESTING.md test tiers
and focused runners, agent_docs/index.md, maintainer-style.md, Rust and error
guidance. Applicable security authority is architecture/wyrd-security-posture.md
stored-secret ownership/context; REQ-003–005 and REQ-009 govern facade use,
preparation and lifetime. The user's review request explicitly states 503 during
key-service outage; TASK-002 scenario 5 records that same maintainer decision.
No nested AGENTS.md exists under these Gateway source directories.

Read cumulative diff and current source for Gateway managed.rs, credential.rs,
snapshot.rs, engine.rs; server components/gateway owner.rs, snapshots.rs,
service.rs, invocation.rs and affected PG invocation tests; verification owner
and Operator-worker diffs to distinguish background delivery from hot verify;
KMS Transit/remote error mapping. Inspected Gateway compatible journey assertions
and harness restart/settling/request behavior.

## Coverage and evidence

| Assigned contract / scenario | Source and proof | Assessment |
|---|---|---|
| REQ-001/003/004; scenario 3/4: managed envelopes use common owner and tenant/name/provider context | ManagedSecretBinding::seal/open/context in wyrd-gateway/src/managed.rs delegates to WyrdKms and derives authenticated context from trusted identity. Server service.rs credential_snapshot uses row name/provider and verified tenant. PG managed-credential test copies another tenant's envelope then verifies refusal without dispatch and source tenant success. | Covered structurally; execution reported in task. |
| REQ-009; scenario 5: warm calls add no remote unwrap or snapshot DB check | load_through at snapshots.rs:253 returns cached Arc; CredentialResolver::resolve at credential.rs:138 reads prepared SecretString. It retains external environment/Vault source handling. compatible.rs:936 journey records Transit decrypt counts after deploy/rotation/restart and verifies warm calls add zero. Existing warm governance tests cover in-memory admission, and no new PG call was introduced into warm secret resolution. | Covered structurally and by inspected assertions; task records 19/19 Gateway journeys and 87/87 Gateway lib tests. |
| REQ-009; scenario 5: remote cold miss must not await KMS | snapshots.rs:256–268 restricts caller-waited loading to a local facade. A remote managed-secret miss queues reload and returns ServiceUnavailable. compatible.rs:1045 cold journey asserts 503 exact code and zero upstream requests while Transit stalls. | Covered for initial cold miss. |
| REQ-009; scenario 5 and explicit user decision: outage after reload remains retryable 503 until preparation recovers | reload installs unprepared snapshot before returning its own 503; the background worker consumes that error. Cached invocation instead reaches generic resolver failure and 502. The cold journey tests two calls before stalled unwrap completes, then restarts with a working endpoint. | FAIL: SYSTEM-001. |
| REQ-009: notification, generation fencing, fail-closed listener loss, writing-server warming | Owner refresh_worker concurrently selects listener, reload loop, usage worker and shutdown. snapshots.rs invalidation/reset increments generation; insert requires matching generation/live; failed/fenced reload remains queued; reset clears snapshots. service.rs warm follows successful commit. Existing snapshot tests assert raced loads cannot cache, listener loss clears entries and unrelated tenants keep serving; PG managed credential test waits another replica reload then verifies revocation refusal. | Covered; unchanged core generation semantics preserved. |
| REQ-009: admitted operations keep immutable snapshot and exact prepared value | Tenant snapshot and prepared SecretString share one Arc owned by cache/admitted call. Reload prepares a new object from its own committed row envelope and never copies old prepared secret into new failed snapshot. Credential resolver has no separate DEK cache. Engine operates on CallInput snapshot; newly installed cache cannot mutate admitted Arc. | Covered structurally; existing invocation tests exercise snapshot policy replacement and managed credential revocation. |
| Recovery/topology | Retryable KMS failures are requeued and worker delays one second between failed passes. Provider calls are bounded. New snapshots replace failed state after backend recovery without new resource writes; listener loss prevents caching; restarting resolves persisted envelopes. Old cached plaintext is not reused on a reload. | Source supports recovery, but current cold test recovers by process restart and does not demonstrate a completed failed reload followed by in-process recovery. Include that path in SYSTEM-001 closure proof. |

These are boundary coverage assessments, not a second full task acceptance
matrix. The orchestrator/validator reconciles them with other reviewers.

## Added or moved waits

| Path | Wait changed | Authority / result |
|---|---|---|
| Warm Gateway invocation, managed credential | Removes per-attempt local opening; reads prepared process memory. No new PG, Scribe, object storage, Vault or KMS wait. | REQ-009 memory-only warm calls. |
| Cold Gateway invocation / invalidation miss | Existing snapshot catalog statement is retained; load_through passes only a local facade (file KEKs already loaded in memory). Remote unwrap is not awaited by the caller. | Sanctioned one snapshot reload on miss; TASK-002 explicitly requires separating shared load consumers. |
| Background snapshot reload | Adds remote KMS unwrap after tenant snapshot transaction commit. No connection/row lock is held during remote preparation, and no mutex spans await. | REQ-009 background preparation; not caller-waited hot path. |
| Writing server cache warm | Adds remote unwrap through existing post-commit warm/reload, with bounded retries in existing owner. | REQ-009 preserves writing-server warming; administration is outside listed hot paths. |
| Gateway credential submission | Sealing moves ahead of tenant transaction acquisition through audited_prepared. | REQ-008 remote calls outside locks/transactions. Administrative persistence remains existing owner. |
| Operator worker delivery / rewrap | Replaces old crypto owner with facade opening/rewrap in background worker. | Background delivery/maintenance, not synchronous verify hot path. Domain reviewer owns detailed transaction fences. |

No added/moved hot-path Scribe/object-storage/Vault wait was found. Existing
external Gateway references still use their existing per-attempt external reader
and are explicitly excluded from this stored-secret cutover. Provider clients
and key material are constructed at process composition, not per Gateway call.
No benchmark was run: root AGENTS.md and task explicitly exclude a new benchmark
requirement absent maintainer request for this change.

## Proposed blocking finding

### SYSTEM-001 — INCORRECT: a completed failed preparation changes retryable 503 into 502

**Requirement/authority:** REQ-009 runtime lifecycle and TASK-002 scenario 5,
specifically the maintainer outage behavior recorded there and repeated in the
current user's request: calls on that credential return retryable 503 until the
key service recovers. Unrelated credentials must remain usable; do not solve
this by rejecting the whole tenant whenever any credential is unprepared.

**Starting conditions and trigger:** A running server with a managed Gateway
credential and a live snapshot listener reloads that tenant after a notification,
or after its cold call queues a reload. The configured Transit/KMS endpoint
returns a retryable failure (e.g. HTTP 500) promptly, allowing that background
reload to finish. Invoke a deployment that uses that managed credential while
the backend remains unavailable.

**Exact path/evidence:** service.rs:1057–1068 records `secret: None` and sets a
transient aggregate retry flag. snapshots.rs:360–368 installs that snapshot
before returning ServiceUnavailable to its background caller. The aggregate
retryability is then lost from runtime credential state. A new caller hits
snapshots.rs:253–254 and bypasses the only pending-preparation 503 branch at
261–266. credential.rs:138–139 returns CredentialError::Unavailable; engine.rs:
868–881 turns every resolver error into a BeforeDispatch failed attempt;
invocation.rs:962–965 responds with GatewayUpstreamUnavailable. The catalog
at wyrd-spec/src/error.rs:1109–1117 defines that variant as HTTP 502,
`WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE`.

**Expected/actual/consequence:** Expected exact retryable service-unavailable
503 for the affected managed credential with no provider dispatch. Actual 502
misclassifies the server's pending key preparation as upstream failure after
the first failed reload. No credential leaks and no upstream dispatch were
found; the concrete defect is the contracted response/retry classification.

**Proof limitation that concealed it:** compatible.rs:1045
`cold_call_waits_retryably_while_a_managed_secret_is_prepared` uses a one-minute
stalled endpoint, asserts two early calls, then restarts against the working
fixture. Neither call requires a failed reload to have completed; it never
observes an installed unprepared snapshot, nor same-process recovery. The
reported GREEN therefore does not falsify this source-backed path.

**Smallest testable correction boundary:** Preserve the transient preparation
state through the existing Gateway credential/snapshot and execution error
boundary so calls that cannot use the affected credential receive the existing
ServiceUnavailable classification. Keep existing fallback behavior and healthy
credentials/tenants available; keep warm calls memory-only, old prepared material
uncarried on failed reload, and background retry ownership. No new cache, TTL,
provider requests during invocation, or global barrier is necessary.

**Closure proof:** A focused Gateway journey with repository Postgres, real
server and controlled wrapping/provider fixtures should force a completed
retryable unwrap failure (not only an in-flight stall), then assert exact 503
code and zero provider dispatch on repeated calls. Exercise an unrelated usable
credential, restore the same wrapping endpoint in the same process, and verify
background reload restores use without a resource edit/restart. Existing
Gateway journey runner and exact selectors own this proof. Add assertions to the
existing owner tests only where needed; do not broaden into full suite or
benchmark work.

## Process and verification limits

No commands that build or execute Rust were run by this specialist; discovery
used immutable source, assertions, committed task execution records and current
user evidence. No claim of a fresh rerun is made. Live AWS/GCP and Linux startup
checks remain the user's accepted CI limits, outside this boundary. Retain
reported TDD violations for scenarios 2/3 separately from behavioral verdict;
this report does not demand historical RED reconstruction or code churn.
