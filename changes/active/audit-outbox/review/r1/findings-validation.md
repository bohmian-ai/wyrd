# Structured Ponytail finding validation

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Reviewed range: `base..candidate` only
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 1
- Original tasks: `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Supplemental user-approved authority: high-throughput acknowledgement means
  receipt; audit is eventually consistent within seconds; the server-owned
  outbox retries dependency slowness, drains on graceful shutdown, and may lose
  counted unflushed work on hard process termination.

The repository has no `.codegraph/` directory. `HEAD` matched the candidate
before validation. No later or out-of-range commit was used as candidate
evidence.

## Validation status

**COMPLETE.** Every proposed finding was checked against the candidate source,
its callers, sibling consumers, applicable authority, and task proof. The
deduplicated ledger retains ten findings. `FIND-AUDIT-OUTBOX-1` requires
`SPEC_REVISION_REQUIRED`; the remaining retained findings are bounded source,
documentation, or verification corrections once that authority conflict is
resolved. The accepted eventual-consistency, receipt-before-durability, and
hard-kill loss windows are not findings.

## Authority resolution

The supplemental principle is higher-priority user authority and establishes
that transient dependency slowness must be retried. Revision 1 nevertheless
states the contrary in REQ-003 (a failed commit loses the event), the
expensive-to-reverse durability decision, AC-002's permanent-loss proof, and
the task implementation evidence. Implementing retry under the current packet
would therefore change approved durability and concurrency semantics rather
than perform a reversible local fix. The retry defect is confirmed, but its
correction is `SPEC_REVISION_REQUIRED` before implementation remediation.

## Proposal disposition

| Discovery proposal(s) | Validation | Result |
|---|---|---|
| `BEH-001`, `INV-REV-001`, `SYS-001`, `SEC-TEN-001`, `CONC-001`, `DOMAIN-DUR-001` | `CONFIRMED` | One root cause: failed acquire/append/commit batches are settled as lost instead of retained for retry. Retained as `FIND-AUDIT-OUTBOX-1`; correction needs a spec revision. |
| `SEC-TEN-002`, `FU-ADMIN-001` | `REVISED` | The six admin paths are reachable, but the proposed shared-helper correction is invalid: `authorize_service_accounts_write` has eight callers, and `auth/revoke.rs` enriches its returned event before staging. Retained as `FIND-AUDIT-OUTBOX-2` with a route-local earliest-stage correction. |
| `BEH-002`, `INV-REV-002`, `STD-001`, `STD-002`, `MAINT-01`, `FU-DOC-001` | `REVISED` | The stale authority, public schema prose, OpenAPI descriptions, and touched Rust/client docs share one incomplete contract-migration root cause. Historical decode values, reserved proto identifiers, and immutable migration comments are excluded. Retained as `FIND-AUDIT-OUTBOX-3`. |
| `BEH-003`, `INV-REV-003`, Oracle portion of `FU-VERIFY-001` | `CONFIRMED` | The cited Oracle peer test is healthy-path only and does not inject commit failure or assert the `bifrost` counter. Retained as `FIND-AUDIT-OUTBOX-4`. |
| `BEH-004`, `INV-REV-004`, `SYS-003`, capacity portion of `FU-VERIFY-001` | `REVISED` | Required capacity evidence is absent. The missing `bench:capacity` name is integration-state drift, not a reason to add an alias in this candidate. Retained as verification finding `FIND-AUDIT-OUTBOX-5`. |
| `SYS-002`, `FU-TENANT-001` | `REJECTED` | REQ-002 requires bounded concurrent tenant commits and the candidate reserves at most one blocked writer connection per tenant. REQ-003 expressly permits loss when the one process queue is full. The spec does not require per-tenant admission quotas or reserved quiet-tenant capacity. Adding either would introduce a new concurrency/resource-allocation decision, not repair a stated revision-1 obligation. |
| `STD-003` | `CONFIRMED` | The materially changed canonical append still carries forbidden manual tenant predicates on a `TenantConn` path and documents an impossible bypass-RLS caller. Retained as `FIND-AUDIT-OUTBOX-6`. |
| `STD-004` | `CONFIRMED` | Candidate-added signatures use fully qualified types despite the mandatory import-and-bare-name rule. Retained as `FIND-AUDIT-OUTBOX-7`. |
| `STD-005` | `CONFIRMED` | The broad change modified the TypeScript native test binding, declaration, and integration test without the required aggregate/TypeScript evidence. Retained as `FIND-AUDIT-OUTBOX-8`. |
| `MAINT-02` | `CONFIRMED` | New permanent test rustdoc embeds active-packet requirement IDs forbidden by repository rules. Retained as `FIND-AUDIT-OUTBOX-9`. |
| `MAINT-03` | `CONFIRMED` | Gate, Oracle, and peer traits say `append_*` although the operations only stage into the outbox. Retained as `FIND-AUDIT-OUTBOX-10`. |

## Final deduplicated finding ledger

### FIND-AUDIT-OUTBOX-1 — accepted batches are discarded on transient dependency failure

- **Discovery sources:** `BEH-001`, `INV-REV-001`, `SYS-001`,
  `SEC-TEN-001`, `CONC-001`, `DOMAIN-DUR-001`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** The controlling user-approved outbox principle
  requires retry on dependency slowness; graceful shutdown must keep flushing
  accepted work until its deadline. Only unflushed hard-kill loss is accepted.
- **Exact location:** `crates/vala/vala-sql/src/audit_outbox.rs:271-308`;
  contrary proof is encoded at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:871-1018`.
- **Producer-to-consumer evidence:** `AuditOutbox::stage` increments `pending`
  and sends the event. `AuditOutboxWriter::dispatch` removes a tenant batch from
  `waiting` and spawns `commit_tenant`. Any `tenant_conn`, append, or commit
  error is only logged/counted; `commit_tenant` returns `()`. `settle` then
  removes the tenant task and decrements all batch events from `pending`, so the
  batch has no remaining owner and neither later recovery nor graceful
  shutdown can replay it. Gate, Oracle, auth, admin, gateway, and verification
  all feed this same path.
- **Observable consequence:** A brief Postgres timeout, failover, connection
  reset, or transaction interruption permanently removes accepted audit
  decisions while the process remains alive and the dependency can recover.
- **Ponytail result:** Do not add a WAL, relay, second queue, or downstream
  guards. Reuse the existing `AuditOutboxWriter`, per-tenant queue, connection
  ceiling, and `pending` accounting. The correction belongs at the failed-batch
  owner.
- **Decision-complete correction:** `SPEC_REVISION_REQUIRED`. Revise REQ-003,
  REQ-007, AC-002, and the expensive-to-reverse durability decision to define
  which SQL/dependency failures remain retryable, which failures (if any) are
  terminal event defects, retry/backoff and connection bounds, per-tenant order,
  failure-counter semantics, and what shutdown does at its deadline. After
  approval, retain a retryable failed batch at the front of its tenant's
  existing queue, do not decrement `pending` or permit later same-tenant work
  to overtake it, retry without holding a connection during backoff, and keep
  sibling tenants dispatchable. Queue-full admission loss and hard-kill loss
  remain as approved.
- **Focused closure proof:** A Postgres-backed test must fail an actual append
  or connection attempt transiently, restore the dependency without restarting
  or restaging, and prove the original events commit exactly once in gap-free
  order while another tenant progresses. Shutdown must drain a recovered batch
  before its deadline and report a still-failing remainder at the deadline.

### FIND-AUDIT-OUTBOX-2 — six admin allowances can escape before staging

- **Discovery sources:** `SEC-TEN-002`, `FU-ADMIN-001`
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-005 and INV-001 require the known permission
  verdict to be staged before the operation proceeds or refuses.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/admin/routes.rs`
  at `296-334`, `375-385`, `443-453`, `523-546`, `592-605`, and `658-668`.
- **Producer-to-consumer evidence:** `authorize_service_accounts_write`
  returns an allowed `AuditEvent`; each listed handler then performs fallible
  work before `AuditOutbox::stage`. All six can fail acquiring `TenantConn`;
  issuer creation can also fail after screened OIDC discovery and sealing, and
  workload-binding creation can fail during projection. Denials are already
  staged by `authorize_recording_denial`. The shared helper cannot stage all
  allowances itself because its other callers include API-key issuance and
  principal revocation, and revocation adds operation-specific detail first.
- **Observable consequence:** An authorized admin attempt can return an error,
  and issuer creation can perform outbound discovery, without any audit event
  for the evaluated allowance.
- **Ponytail result:** No new helper or abstraction is needed. Move each
  existing `stage` call to the first statement after the allowed event is
  returned and any required detail is complete.
- **Decision-complete correction:** In the six admin handlers, stage the
  returned allowance before discovery, sealing/projection, tenant connection
  acquisition, or other fallible work. Preserve denial staging, request
  validation that occurs before a permission is evaluated, SSRF screening,
  tenant binding, and operation errors.
- **Focused closure proof:** Force issuer discovery failure and tenant
  connection acquisition failure after an allowed check, settle the outbox,
  and assert exactly one allowed event with the expected tenant, principal,
  operation, resource, and request ID. Existing success/denial tests must stay
  green.

### FIND-AUDIT-OUTBOX-3 — live authority and public contract prose still describe the removed audit model

- **Discovery sources:** `BEH-002`, `INV-REV-002`, `STD-001`, `STD-002`,
  `MAINT-01`, `FU-DOC-001`
- **Status:** `REVISED`
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-003, REQ-004, AC-006, T03, `AGENTS.md` §16,
  and the one-outbox/no-other-WAL authority require docs and generated
  contracts to match non-blocking process-outbox behavior.
- **Exact location:** representative owning sources are
  `architecture/wyrd-security-posture.md:358-387`,
  `architecture/operations/README.md:25,50`,
  `architecture/operations/reliability-and-recovery.md:31-36,146-150,220-222`,
  `architecture/operations/runbooks.md:116-145`,
  `architecture/references/architecture/patterns.md:243-267`,
  `architecture/references/doctrine/architecture-constraints.md:106-122`,
  `architecture/references/languages/rust-core.md:601-607`,
  `architecture/references/domain/vala-architecture.md:50-52,77-80`,
  `crates/wyrd-spec/src/vala/api.rs:2422,2450-2456`,
  `crates/wyrd-spec/src/vala/audit_detail.rs:1`,
  `crates/wyrd/wyrd-auth/src/callback.rs:165-173`,
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:42`,
  `crates/wyrd/wyrd-server/src/auth/callback.rs:41-47`,
  `crates/wyrd/wyrd-server/src/auth/revoke.rs:25-33`,
  `crates/wyrd/wyrd-server/src/components/platform/routes.rs:1-6,79`,
  `crates/wyrd/wyrd-server/src/components/platform/credentials.rs:61-79,106-112,122-138,186-207`,
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:110-147,195,300,359,425,525,592`,
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:339-343`,
  `crates/wyrd/wyrd-server/src/components/principals/routes.rs:402-410`,
  `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:152-165,1267,1392,1436,1469,1504,1545,1575,1613,1647`,
  `crates/shared/wyrd-client/src/principals/handle.rs:132-135`, and
  `docs/src/content/docs/bifrost/architecture.svx:313-328`. The false
  `AuditEvent` source prose is propagated to both generated
  `bifrost_audit_event.json` schema copies.
- **Producer-to-consumer evidence:** The changed runtime owners call
  `AuditOutbox::stage`, which cannot return a commit error and does not share
  the protected operation's transaction. Utoipa consumes route annotations to
  serve OpenAPI; schema generation consumes `AuditEvent` rustdoc; SDK and
  maintainer consumers read the Rust/client documentation; operators follow
  the operations authority. Those consumers currently receive mutually
  incompatible transactional, fail-closed, Oracle-WAL/relay, and error
  promises.
- **Observable consequence:** Clients are told to handle audit-caused 500/503
  responses that no longer exist; maintainers can reintroduce forbidden
  transactional exceptions or Oracle WAL/relay infrastructure; readiness and
  incident procedures name components and log fields the implementation does
  not have.
- **Ponytail result:** Delete obsolete promises rather than add compatibility
  behavior. Correct owning source prose and regenerate; do not add another
  documentation layer or name-ban check.
- **Decision-complete correction:** Align every live authority and touched
  public/internal contract with the one process outbox, eventual staging,
  non-refusal, current log fields, and no Oracle WAL/relay or transactional
  surface exception. Correct the `AuditEvent` source rustdoc and regenerate
  both schemas. Remove audit-caused error text from Utoipa and Rust/client docs,
  then strengthen the served OpenAPI assertion to reject that language. Keep
  `AuditErrorCode::AuditUnavailable` and its schema value solely for retained
  history decoding, keep proto number/name 6 reserved, and do not edit
  historical migration comments.
- **Focused closure proof:** Focused searches must leave no live same-operation
  transaction, fail-closed, Oracle acceptance-WAL/relay, or audit-caused
  request-error claim outside the approved historical exceptions. Run
  `mise run docs:check`, `mise run codegen:check`, the proto drift check, and
  the served OpenAPI contract test.

### FIND-AUDIT-OUTBOX-4 — AC-002 lacks Oracle commit-failure proof

- **Discovery sources:** `BEH-003`, `INV-REV-003`, Oracle portion of
  `FU-VERIFY-001`
- **Status:** `CONFIRMED`
- **Classification:** `MISSING`
- **Violated obligation:** AC-002 explicitly names Oracle as a surface family
  that must remain behaviorally unchanged and increment the surface counter
  under an injected audit commit failure.
- **Exact location:** the claimed evidence at
  `changes/active/audit-outbox/tasks/02-one-outbox.md:120` points to
  `crates/wyrd/wyrd-server/src/oracle/peer_audit.rs:109-164`.
- **Producer-to-consumer evidence:** The cited test stages two healthy peer
  events, successfully shuts down the outbox, and reads committed rows. It
  neither makes the SQL append fail, drives an Oracle request through failure,
  nor reads `audit_outbox_commit_failures_total{surface="bifrost"}`. The Gate
  write in the existing journey uses the same metric label but is not an
  Oracle consumer.
- **Observable consequence:** The explicit Oracle family acceptance
  obligation has no evidence that the concrete Oracle-to-outbox composition
  keeps the Oracle result independent from audit persistence and reports the
  failure.
- **Ponytail result:** Reuse the existing Oracle server/test harness, staging
  failure injection, and `AuditCommitFailures`; do not build a new harness.
- **Decision-complete correction:** Add the smallest real Oracle query or peer
  boundary case that injects failure in the shared outbox append and asserts
  the Oracle operation's non-audit result. After `FIND-AUDIT-OUTBOX-1` is
  specified and fixed, restore the dependency and prove the retained decision
  commits once; assert the `bifrost` failure counter according to the revised
  metric semantics.
- **Focused closure proof:** Run the exact named Oracle failure test and its
  owning Oracle/server integration lane; the test must fail if Oracle starts
  awaiting or propagating audit persistence.

### FIND-AUDIT-OUTBOX-5 — required capacity acceptance evidence is absent

- **Discovery sources:** `BEH-004`, `INV-REV-004`, `SYS-003`, capacity portion
  of `FU-VERIFY-001`
- **Status:** `REVISED`
- **Classification:** `MISSING`
- **Violated obligation:** AC-005 requires the canonical capacity run to pass
  every judged saturation step and two-replica scale-out.
- **Exact location:** `changes/active/audit-outbox/spec.md:146-149` and the
  command evidence in all three task packets. Candidate `mise.toml:507-530`
  defines predecessor capacity tasks but no `bench:capacity`.
- **Producer-to-consumer evidence:** The change was motivated by the prior
  two-replica throughput failure. Unit, SQL, and journey tests exercise
  correctness but do not measure the benchmark's sustained or scale-out
  verdicts. No task evidence records the required run or result artifact.
- **Observable consequence:** The task has no acceptance proof for its primary
  throughput and scale objective.
- **Ponytail result:** Do not add an audit-only benchmark or a compatibility
  alias in this task branch. Use the repository-owned canonical lane when the
  integration state supplies it.
- **Decision-complete correction:** Integrate the candidate with the authority
  that owns `bench:capacity`, run the exact canonical lane against this
  cumulative implementation, and preserve the judged one-replica sustained and
  two-replica scale-out result. A reduced-duration smoke or predecessor lane is
  not closure evidence.
- **Focused closure proof:** A recorded green `mise run bench:capacity` artifact
  with every judged saturation step and the two-replica scale-out verdict.

### FIND-AUDIT-OUTBOX-6 — canonical TenantConn append repeats tenant filtering

- **Discovery source:** `STD-003`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` makes RLS through
  `TenantConn` the tenant boundary and forbids manual per-query tenant filters.
- **Exact location:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:60-75,88-97,165-174`.
- **Producer-to-consumer evidence:** Production calls
  `ValaPostgres::tenant_conn(tenant)` and passes only `&mut TenantConn` to
  `append_audit_events`; test-support wrappers do the same. Nevertheless the
  chain-head `SELECT` and `UPDATE` repeat `data_tenant_id` predicates, and the
  new rustdoc justifies a bypass-RLS boundary that this signature cannot
  represent. Cross-tenant callers would require `OperatorPool`, and none calls
  this function.
- **Observable consequence:** The canonical write path carries two tenant
  authorities and documents an impossible caller, preserving the exact drift
  the repository rule prohibits.
- **Ponytail result:** Delete the redundant predicates and false bypass
  rationale. Reuse RLS and `TenantConn`; do not introduce a second SQL entry
  point. Supplying `data_tenant_id` for inserted rows remains necessary and is
  not the redundant filter.
- **Decision-complete correction:** Let RLS select/update the one visible
  chain-head row, retain the `TenantConn`-derived value for inserts, and retain
  `SYSTEM_OWNER` by acquiring its ordinary tenant connection rather than
  widening SQL.
- **Focused closure proof:** Run the audit SQL integration tests and the tenant
  isolation check; add or retain a two-tenant assertion showing one tenant
  cannot select or update the other's chain head.

### FIND-AUDIT-OUTBOX-7 — candidate-added signatures hide type ownership

- **Discovery source:** `STD-004`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` requires top-level
  imports and bare type names in signatures.
- **Exact location:** `crates/vala/vala-sql/src/audit_outbox.rs:276`,
  `crates/wyrd/wyrd-server/src/state.rs:622`,
  `crates/wyrd/wyrd-server/src/auth/callback.rs:761`, and
  `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:3719-3727`.
- **Producer-to-consumer evidence:** These newly added or changed signatures
  spell `tokio::task::JoinError`,
  `vala_bifrost_redux::oracle::OracleRuntimeInspection`,
  `serde_json::Value`, and peer/error types inline even though their modules
  already have top-level dependency manifests.
- **Observable consequence:** The changed modules violate the repository's
  mandatory ownership/readability convention and hide dependencies in method
  declarations.
- **Ponytail result / correction:** Add the existing types to the appropriate
  top-level `use` blocks (or test-module block) and use bare names. No wrapper,
  alias type, or refactor is warranted.
- **Focused closure proof:** `mise run fmt`, `mise run lints`, and a focused
  search of the changed signatures for the fully qualified spellings.

### FIND-AUDIT-OUTBOX-8 — broad and TypeScript verification is incomplete

- **Discovery source:** `STD-005`
- **Status:** `CONFIRMED`
- **Classification:** `MISSING`
- **Violated obligation:** `AGENTS.md` §11-§12 requires `mise run gate` for an
  intentionally broad multi-owner change without one complete capability gate;
  changed TypeScript/native declarations require their owning build, typecheck,
  unit, and integration proof.
- **Exact location:** task evidence across
  `changes/active/audit-outbox/tasks/01-publication-progress.md`,
  `02-one-outbox.md`, and `03-remove-audit-unavailable.md`; changed surfaces are
  `sdks/wyrd-sdk-ts/native-testing/src/lib.rs`,
  `sdks/wyrd-sdk-ts/testing/index.d.ts`,
  `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts`, and
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts`.
- **Producer-to-consumer evidence:** Recorded commands cover many Rust, SQL,
  server, Python, docs, and codegen lanes but not `mise run gate` or the
  TypeScript build/typecheck/unit/integration set. `codegen:check` verifies the
  generated error constant but does not compile the changed native-testing
  binding/declaration and surviving TypeScript consumers.
- **Observable consequence:** Completion evidence does not establish that the
  changed private N-API harness surface and TypeScript declarations compile and
  agree at runtime.
- **Ponytail result / correction:** Run the existing aggregate rather than
  inventing another lane or duplicating every component as final proof. Record
  a green `mise run gate`; run a separate specialized lane only if inspection
  of the then-current aggregate proves it lies outside the gate.
- **Focused closure proof:** Green `mise run gate`, with the task record naming
  its TypeScript build, typecheck, unit, and integration coverage.

### FIND-AUDIT-OUTBOX-9 — permanent test rustdoc embeds disposable task IDs

- **Discovery source:** `MAINT-02`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` and Maintainer Style
  forbid task/plan identifiers in permanent code.
- **Exact location:** `crates/vala/vala-sql/tests/pg_audit_outbox.rs:83-85,131-132`.
- **Producer-to-consumer evidence:** This new test target names `AC-003`,
  `INV-002`, `AC-007`, and `REQ-002`. The active packet is removed at change
  completion, while the adjacent prose already states the behaviors.
- **Observable consequence:** Future maintainers inherit opaque identifiers
  whose defining document is intentionally absent from the current tree.
- **Ponytail result / correction:** Delete only the parenthesized IDs; keep the
  behavior-focused descriptions and tests unchanged.
- **Focused closure proof:** A focused search finds no audit-outbox spec/task
  IDs in permanent source or test documentation; the SQL integration target
  still compiles.

### FIND-AUDIT-OUTBOX-10 — staging collaborators retain durable-append names

- **Discovery source:** `MAINT-03`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** The task's one-write-path invariant and Maintainer
  Style require operation names to expose the real stage-versus-append
  boundary.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/gate/mod.rs:258-299`,
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:833-891`, and
  `crates/vala/vala-bifrost-redux/src/oracle/peer.rs:729-742`, plus their
  implementations and callers.
- **Producer-to-consumer evidence:** `GateAudit::append_write_decision`,
  `OracleAudit::append_read_decision`/`append_security_violation`, and
  `PeerSecurityAudit::append_*` all return after `AuditOutbox::stage`; only
  `vala-sql::append_audit_events` performs the database append. These trait
  methods and test doubles are the sibling consumers a maintainer finds when
  tracing the supposed single append.
- **Observable consequence:** Source search presents several apparent audit
  appenders and hides the non-durable receipt boundary, making the one-write-
  path invariant harder to verify and easier to violate.
- **Ponytail result / correction:** Mechanically rename these trait methods,
  implementations, test doubles, and callers from `append_*` to `stage_*`.
  Keep the real SQL owner named `append_audit_events`; do not add a compatibility
  alias.
- **Focused closure proof:** Compile the affected Gate, Oracle, peer, and
  server targets, and verify production `append_audit*` references resolve only
  to the canonical `vala-sql` writer path.

## Explicitly rejected or accepted windows

- Process-local acknowledgement before durable commit, seconds-scale eventual
  visibility, deadline-bounded graceful shutdown remainder, and counted loss
  after hard process termination are approved behavior and were not reported.
- The noisy-tenant global-capacity proposal is rejected for this revision:
  bounded cross-tenant commit scheduling is implemented, while full global
  queue loss is explicitly authorized. Per-tenant admission fairness would
  require new approved concurrency/resource semantics.
- Retained `AuditErrorCode::AuditUnavailable`, its generated schema value, the
  reserved gRPC enum number/name, and historical migration comments are
  necessary compatibility/history artifacts, not live audit-refusal contracts.
