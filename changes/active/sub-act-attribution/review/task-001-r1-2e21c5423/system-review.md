# System resilience review

Result: **PASS**

## Subject and authority

Independent cumulative runtime review of base
`b22718f2cf5da0ebcb7f26bc376157e5eec7768f` → candidate
`2e21c5423036460d5da2ca22a51efde63635e352`, against TASK-001, approved spec
r6, the prior reviewed findings and R1. The remediation locator diff
`69687f648..2e21c5423` was used to locate closure; it did not replace the
cumulative runtime audit. HEAD still matched the candidate at completion;
the only working-tree additions were the new review directory. No source was
changed and no Cargo or runtime commands were run.

Used CodeGraph first, then expanded actual producers, lifecycle owners,
consumers and tests. Governing authority consulted: AGENTS.md,
architecture/agent-rules.md, reference router, spec-driven-development,
maintainer-style, testing-workflows, applicable identity/verification/Workflow
sections of wyrd-design.md and wyrd-doctrine.mdx, applicable ingest, built-in,
outbox and failure sections of bifrost-design.md, security-posture's accepted
authority and audit boundaries, and analytical-operations-reliability.
The approved nonblocking outboxes and process-local Workflow execution are
explicit exceptions to stronger durability; this review does not require a
new persistence or recovery mechanism.

## Deployed paths and ownership

The serving process owns HTTP/gRPC/MCP authentication and composition. Its
verification runtime supervises scheduler, runner, fitter and Operator tasks.
Tenant-scoped Postgres owns the durable Verifier queue and catalog; Scribe
owns acknowledged analytical bytes and their recovery. A server without
local Scribe uses the existing mTLS peer capture service. This change adds
attribution and accepted authority to these owners, rather than adding a
service, relay, identity store or execution runtime.

| Runtime path | Producer → consumer evidence | Availability and recovery assessment |
|---|---|---|
| Realtime request → Verifier → judge → gateway | `components/verification/service.rs`, `verification/direct.rs`, `verification/eval.rs`, `components/auth/caller_extractor.rs::Caller`, `components/gateway/workflow.rs`, `invocation.rs`, `ledger.rs` | The effective Principal remains the permission-bearing value; `executed_by` changes only the recorded actor. Judge refusal stays an operation failure and cannot spend SYSTEM permissions or ambient credentials. The existing gateway cancellation and provider timeout remain in force. |
| Manual queue acceptance → replacement worker | `queries/verifier_runs.rs::enqueue_manual` captures `AcceptedAuthority`, serializes it with the pair during insertion; `CLAIM_RUN_SQL:289–323` returns `r.authority`; `ClaimedRunRow::into_claimed:2273–2293` carries it unchanged; Eval constructs `Caller::accepted(&run.authority, system)` | Queue leases and accepted authority survive process replacement in Postgres. Claim/retry do not recover current subject grants or require the initiating bearer token. Pair-scoped idempotency replay returns the stored operation instead of rewriting its authority. |
| Scheduled binding acceptance | `verification/mod.rs:380–392` composes BackgroundAuthority into the existing scheduler; `scheduler.rs::schedule_tenant`; queue `claim_due`/`schedule_due:1416–1535` | Owner resolution, insertion and cursor advance share one tenant transaction. Missing authority skips the occurrence observably. SQL errors roll back that occurrence; the tenant error is logged and other tenants continue. Cancellation before commit selection rolls back; a selected commit is awaited before closure. |
| Observation acknowledgement → accepted runs | `verification/observations.rs::ObservationEnqueue::acknowledged` → generic Outbox → `ObservationRunSink::write:106–145` → queue `lock_observation_bindings:1812` / `enqueue_observation_runs:1865` | Acknowledged observation ingest does not wait for run insertion. A database error retains the outbox slice for retry. Missing/inactive/foreign owner or missing SYSTEM refuses only affected bindings with structured diagnostics; valid siblings still insert. The binding/record unique identity absorbs a repeated transaction after an unknown commit. |
| Queued execution and settlement | `verification/runner.rs::run`, `attempt`, `settle:407–496`; shared ClaimLoop/LeaseRenewal; `verification/mod.rs::run:260–300` | Lost leases stop work and cannot stage a stale settlement. Shutdown drains then releases work; abrupt loss leaves a reclaimable lease. Settlement retries with backoff while the lease holds. A capability task crash degrades its slot and restarts it, rather than intentionally taking unrelated server capabilities offline. |
| Tokenless input reads | `verification/authority.rs::SystemReadAuthority::resolve` / `serving:176–185`, Drift and Eval readers, AuthorizedQueryContext and peer Oracle context | SYSTEM retains only table-UID-scoped read grants. Subject and historical chain affect audit attribution alone and survive context serialization. A catalog/Oracle outage yields the existing read/engine failure and retry outcome, without a fallback to general query permission or ambient identity. |
| Results, audit, captures and tenant spans → local/peer Scribe | `scribe_outbox.rs::Rows`, `frames`, `identify:795–828`, `write:852–899`; `ScribeBatch::into_frame`; `grpc/capture_peer.rs::batch`; Scribe ingress/decode | Frames group by pair, request, schema and destination; batch identity includes both principal IDs. Local and peer paths use the same frame conversion. Repeated identical slices retain batch identity after lost acknowledgements. Transport identities do not replace payload audit identities. |
| Public ingest → WAL → replay | Gate AuthContext → ScribeIngressFrame → execution_lanes stamping, persistence and replay | The pair is derived before stamping and persists in row bytes. WAL replay does not re-resolve actor grants or infer identity from trace context. Existing policy boundaries and public rejection of internal result/capture tables remain. No old/new-schema rolling compatibility is promised for this unshipped defining-schema change. |
| Tenant provision/startup inventory | `TenantProvisioning::establish_and_promote:281–305` → `establish_tenant_administration` and `BuiltinTables::ensure_tenant:75–95`; `boot/mod.rs:499–505` → `BuiltinTables::reconcile:112–118`; catalog `ensure_builtin` and existing registration/upsert | Authenticated provisioning passes its operator pair; callerless startup passes the sentinel. Administration precedes built-ins, which precede active promotion. A provisioning failure remains visibly incomplete/retryable; startup reconciliation still fails boot before role activation if a required table cannot be ensured, as existing architecture requires. Repeats preserve the first creation pair. |
| Hosted and loaded local Workflows | shared client hydration retains root metadata UID; `workflow/local.rs::SelectedRoutes::dependencies` reuses PublicWyrdGatewayCaller with `(None, card_uid)`; hosted host retains captured caller authority | No new durable Workflow or cross-pod recovery is introduced. Local registered calls now enter the existing Card-scope check; an out-of-scope refusal is local to the call and prevents provider dispatch. This behavior is explicitly approved by r6. Missing loaded Card identity is not synthesized from a principal. |
| Span export and Operators | `wyrd-telemetry/export.rs::TenantRoutes::rebind`, ServerSpanSink; existing Operator worker permission rechecks and run-linked dispatch state | Judge subtree routing keeps siblings on their request route and remains telemetry-only. Export still uses the nonblocking permit/drop path. Operator delivery leases/retries/rechecks are not replaced by attribution IDs. |

## Failure and recovery evidence

* **Postgres outage at acceptance.** BackgroundAuthority returns database
  failures as errors, not identity absence. The observation sink propagates
  them to its retrying outbox; scheduler transactions fail without advancing
  the cursor. Nonretryable owner absence is distinguished from dependency
  failure. No new error path deliberately shuts down the shared process.
* **Identity refusal within an observation batch.** At
  `observations.rs:120–139`, `None` owner and unsuccessful owner/SYSTEM
  resolution converge on one binding-specific warning including tenant,
  binding, owner Card and optional owner. Accepted siblings collect separately
  and are inserted/committed after the loop. This closes the previous
  silent-consumption path without retrying permanently refused bindings or
  inventing authorization audits for engine mechanics.
* **Process interruption and expiry.** The accepted document is stored in the
  run, not the runtime/cache/token. The claim statement only changes lease,
  attempt/status and scheduling fields, and reads the stored document back.
  Expired lease reclaim therefore retains the same authority. SYSTEM's active
  identity is read separately to perform execution; its absence cannot supply
  replacement authority. Missing SYSTEM during input setup/result construction
  fails the run path rather than fabricating an actor.
* **Cancellation and unknown commit.** Scheduler cancellation keeps its existing
  occurrence linearization point. Observation outbox shutdown can cancel a
  transaction while commit outcome is unknown; the existing durable
  binding/record identity makes later repetition safe. Generic Outbox shutdown
  counts pending loss after its deadline. This remains the approved in-memory
  handoff, not a promise to reconstruct unwritten requests after a hard kill.
* **Scribe saturation, unavailable peer, timeout or lost acknowledgement.**
  `ScribeRefusal::from_scribe`/`from_code:324–348` keeps saturation and
  transient/unavailable/timeout errors retryable. `ScribeSink::write` retries
  the same slice; `identify` includes attribution, so different pairs cannot
  share an identity accidentally. Terminal rejection logs/counts the affected
  writes and continues later frames. A pod reaching no Scribe logs/counts
  loss without blocking the originating decision. The runner is composed only
  when its outbox reaches Scribe, preserving the existing deployment guard.
* **Result staging versus SQL settlement.** The unchanged fenced settlement
  stages once before commit. An uncertain commit can leave a staged result
  while another claim later re-executes the run; the existing source documents
  that case. Unacknowledged result/audit loss after abrupt death remains
  explicitly permitted. This attribution change neither proves nor requires
  exactly-once cross-store settlement.
* **Catalog failure and restart.** Failed built-in creation remains a
  provisioning/startup failure at the existing lifecycle boundary. Successfully
  created rows survive and the catalog's idempotent ensure reuses them on
  retry. Passing a different creator on retry does not rewrite the existing
  pair. No new fail-closed boundary was moved from one component to the entire
  serving process.

## Prior finding closure relevant to this review

| Prior item | Current source and proof assessment | Result |
|---|---|---|
| FIND-TASK-001-4 / former SYS-001 observation silent refusal | `observations.rs:106–145` now logs every unaccepted binding, including a missing owner. `eval_verification::background_owner_subject_system_actor` deletes D, moves E to another tenant, suspends C, checks their diagnostics, then removes SYSTEM and checks every refusal with total accepted-run count unchanged. Healthy A/B siblings had already queued, and the permitted A judge still ran. | Closed |
| FIND-TASK-001-1 creation provenance/lifecycle | Defining SYSTEM SQL takes explicit creator pair and uses `ON CONFLICT DO NOTHING`; provisioning forwards the operator pair to SYSTEM and built-ins; startup supplies unresolved sentinel. Existing catalog registration/upsert preserves first creation fields. P1/P2 reported empty-schema and provisioning/startup/repeat assertions are relevant to these paths. | Closed within system scope |
| FIND-TASK-001-6 SYSTEM read history | `SystemReadAuthority::serving(&AcceptedAuthority)` carries accepted chain through the existing query context without changing principal/grants. Drift/Eval both pass run authority. Reported real delegated read test plus serialization proof cover the corrected seam. | Closed within system scope |
| FIND-TASK-001-7 queued/realtime proof | New reloaded-authority proof expires the initiating token and invokes the real protected gateway with claimed authority after a grant addition; provider count is zero for the denied call and a fresh B token succeeds. Scheduled proof executes real Drift and checks input audit/result pair. Realtime proof covers further delegation without restoring excluded permissions. | Relevant gaps addressed, with limits below |

## Proof assessment and limits

I inspected the relevant assertions and implementation; the R1 task records
successful focused execution and sensitivity checks. Those are reported
verification, not independent reruns in this review.

`queued_subject_authority_survives_restart` replaces VerificationRuntime
owners around a scripted retry/completion and checks unchanged durable JSON,
pair and result. It does not kill an OS process or itself exercise an expired
lease. `reloaded_authority_cannot_widen_gateway_access` separately proves the
expired-token + changed-grant + real-claim + protected-gateway refusal seam;
it intentionally invokes the gateway seam with claimed authority rather than
running an actual queued Eval judge. Source connects that same
`Caller::accepted` construction to Eval execution. These complementary proofs
support the changed snapshot boundary without claiming a new crash test.

The observation negative journey captures real diagnostic events and checks
healthy/refused binding outcomes; its expected-owner assertions cover the
formerly silent missing-owner branch. The scheduled journey uses real
scheduler/Drift/input read/result behavior. Existing Scribe outbox retry and
terminal-refusal tests target the shared reliability owner with pair-aware
frames; peer adapter tests check pair preservation and invalid input before
Scribe work. No live-cloud outage, rolling deployment, hard-kill, resource
qualification or production availability measurement was performed.

## Proposed findings

None. The cumulative candidate preserves the approved recovery and sibling
availability boundaries, and R1 closes the prior observable refusal defect.
No speculative durability, retry store, permission refresh, or shared-server
failure policy is required.
