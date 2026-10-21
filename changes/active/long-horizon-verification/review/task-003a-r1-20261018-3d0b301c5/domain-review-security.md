# Domain review — authorization, identity and evidence trust

**Result: FAIL.** One bounded security-journey regression remains (SEC-001). No production authorization bypass, tenant escape, or unsafe authority substitution was found.

## Immutable subject and scope

- Base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
- Latest-fix base: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Authority: approved revision-18 spec, original TASK-003a at the cumulative base, prior verdict/findings and its owner-amended R1 task. Implementation appendices are evidence only.

Reviewed the cumulative authenticated producer → Scribe activity → pending claim → owner authority → authorized snapshot → pinned execution → settlement/reaction path, plus terminal refusal/status readback and shared reference/input decoding. HEAD was checked as the candidate. Only this report is written. No current-round sibling report was read.

## Authority and boundary coverage

Authorities applied: AGENTS §§2, 4, 9–11; agent-rules (TenantConn/OperatorPool, blocking permissions, nonblocking audit, effective-value validation); wyrd-design grouped verification and public status contracts; wyrd-doctrine ownership and client/server boundaries; wyrd-security-posture principal lifecycle, accepted authority, query object authorization and tenant trust; bifrost-design tenant-qualified storage and query admission; spec INV-003/005 and REQ-001/004/006/010/011/013/014; reference router, spec-driven-development, maintainer-style and agent-harness. The task-review domain role governs this report.

| Boundary | Source proof and assessed consequence | Result |
|---|---|---|
| Authenticated producer attribution | `components/verification/service.rs:620–682,693–730`: authorize before staging; source owner comes from `caller.principal.owner_service_uid()`, not a request owner/tenant field. Submission captures AcceptedAuthority. `verification/runner.rs:825–876` keeps the submission's accepted owner or binding owner. | PASS |
| ACK-to-pending trust | `scribe_outbox.rs:276–298,765–805,998–1064`: only grouped input/result variants with typed owner/subject/Verifier/group yield activity; lost items are excluded. Tenant comes from the outbox slice. Scheduling occurs after ACK and before consuming submission reservations. Audit/capture/spans cannot produce activity. Boot supplies the real PostgreSQL owner; recording-only gateway/peer-audit constructor edits pass None and add no production bypass. | PASS |
| Exact owner/source and tenant selection | `verifier_executions.rs:191–235,1720–1787`: parameterized arrays match frozen owner and exact subject/source-Verifier tuples, tenant-bound through TenantConn. Pending table uses forced RLS and tenant-bound FK (`20261018000000_verification_group_pending.sql:10–40`). Cross-tenant due enumeration accepts OperatorPool; it supplies no evidence authority. `components/cards/service.rs:1315–1445` retains source-resolution ownership and exact UID freezing. | PASS |
| Actual owner identity versus accepted authority | `queries/verification.rs:102–118,517–562` retains the actual principal ID even while inactive. `claim_due_group` copies it independently of `active`. `verification/authority.rs:216–311` distinguishes typed principal/tenant/connection rejection from missing issuer/SYSTEM, corrupt state and SQL/store failures. Recovery uses current grants through the existing tenant issuer (`wyrd-auth/src/issuance.rs:559–647`). Nothing falls back to SYSTEM or another owner. | PASS |
| SYSTEM attribution without escalation | `ExecutionAuthority::background` (`verifier_executions.rs:653–690`) stores the owner's AcceptedAuthority and SYSTEM only in attribution. `Caller::accepted` (`components/auth/caller_extractor.rs:65–84`) copies that principal and tenant unchanged and sets executed_by. `groups.rs:115–129` and `runner.rs:451` use that caller. Existing narrow Drift SystemReadAuthority is not used to authorize grouped source reads. | PASS |
| Authorized evidence query | `groups.rs:115–159,237–312`: owner, group and exact source tuples filter both query arms; SQL string values use the SQL AST literal renderer (`drift.rs:140–142`). `query/scheduled.rs:74–87,132–228` dispatches the accepted caller through the public service. `query/service.rs:205–228` checks coarse capability and preserves Oracle's object denial/audit. Unrelated table/schema grants cannot substitute for source-table grants. | PASS |
| No partial evidence acceptance | `groups.rs:175–202` accumulates batches locally but returns them only after successful terminal completion; query failure, denial, degradation or truncation returns before decode/pin/enqueue. `ScheduledQueryCaller::consume_to_terminal` requires terminal validation and clean EOF. Lazy missing-table fallback does not catch typed Denied. Missing tables can become empty evidence; denied tables cannot. | PASS |
| Terminal refusal transaction and lifecycle | `scheduler.rs:363–493`: active-owner/query auth rejection enters refuse_group; unavailable infrastructure/read/non-auth enqueue refusal retains pending. Error insert and consume share one TenantConn commit; errors/cancellation roll back. `INSERT_GROUP_ERROR_SQL` (`verifier_executions.rs:258–283`) records actual owner/SYSTEM IDs, status errored and structured error without authority, inputs, snapshot or result. It invokes no engine or completion/Operator path. | PASS |
| Refusal cannot become runnable authority | New refused-shape constraint requires authority-free rows to remain errored, attempts zero, unleased, settled and result-free. CLAIM_SQL only admits pending/retrying or expired running rows; retry/release/termination require a matching running lease. `ClaimedRow` still requires AcceptedAuthority. No new empty-authority or special runnable input variant exists. `pg_verifier_executions.rs:2897–2983` explicitly asserts attribution, no claim/dispatch/snapshot and rejection of authority-free conversion to pending. | PASS |
| Authorized latest/status visibility | `BINDING_TARGETS_SQL:531–540` includes terminal refusals in latest execution selection. `VerificationControl::get_binding:175–200` and `get_execution:216–263` authorize/audit cards:read before TenantConn lookup. SQL status exposes error/IDs/provenance, not AcceptedAuthority or pinned values; result content still requires the caller's authenticated Bifrost query. A refusal has no result pointer and exits without a result read. Existing HTTP and MCP consumers reuse these services. | PASS |
| Frozen evaluation and reactions | PinnedGroup digest and enqueue preserve selected values and accepted authority; claim/retry do not refresh grants. Runner removes only the retired source-ceiling short circuit. Completion keeps running-lease fencing, same binding/group previous-result comparison and replay suppression (`verifier_executions.rs:1988–2078`). Errored pre-admission records never enter completion and never dispatch Operators. | PASS |
| Shared manual decode acceptance | `reference.rs:140–169` still tries Ref, Sibling, Inline then string Path; each embedded type retains its own validation. All affected inline owners were enumerated: Agent prompt, check-Agent, Trigger, on_failure/on_verdict_change Operators, visitor/composition/registration consumers. Server `collect_inline_ref` (`components/cards/resolve.rs:760–776`) continues rejecting loader-only paths and checking sibling membership. `InputSlot`'s visitor (`card/verifier/mod.rs:264–346`) dispatches strings through SourceText and maps through a tagged, deny_unknown_fields remote mirror with matching required/default fields. No arbitrary deserialization execution, filesystem read or trusted-identity field is introduced. The new error-preservation machinery does not bypass duration/slot validation. | PASS |
| Duration/input boundary | `trigger.rs:94–132` validates positive bounded counts before exact authored duration reaches SQL; standalone and inline Trigger decoding use the same type. No runtime SQL cap remains. Raw route tests and SDK source assert over-year writes do not persist. Reserved group_id remains a server/spec slot validation, independent of typed versus source slot form. | PASS |
| Real security regression journey | Existing `grouped_reads_need_the_owner_grant` still expects grant restoration alone to resume a consumed denied item; its trace-only phase creates no new pending item and therefore does not exercise object denial. | FAIL — SEC-001 |

## Prior finding closure within this domain

- FIND-TASK-003a-1: source closes the security-relevant revision-18 replacement: owner identity survives inactivity; typed rejection differs from infrastructure failure; denial produces an attributable terminal non-runnable error atomically with pending consumption. Server P1 source covers inactive/retired/query-denied cases, missing issuer and refusal-insert rollback. SQL P2 covers terminal shape/claim exclusion; Rust P3 reads the terminal error through latest-binding/status. The unchanged sibling security journey is the new regression below, not a re-opening of the superseded blanket-retention recommendation.
- FIND-TASK-003a-2: bounded parse occurs at both Trigger forms, SQL uses the accepted value exactly, and manual inline decoding propagates the real validation cause; no security validation was weakened to satisfy error assertions.
- FIND-TASK-003a-3/4/5: foreign processes and stronger ACK/pressure tests add no trusted identity source or security-control bypass. Their full cross-runtime/concurrency acceptance belongs to the other assigned review roles. Inspected grouped-support/live-query/isolation assertions remain relevant source evidence.
- FIND-TASK-003a-6 and HUMAN-CLEANUP 1–3: removed DueGroup fields supplied no authorization decision; exact source predicates, activity gate and enqueue target/readiness checks remain. Fixture/helper renames and prose cleanup add no authority.

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-001 — REGRESSION / missing effective security proof.** `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:3276–3289`. Governing obligation: revision-18 REQ-004 explicitly makes real query denial terminal and says reactivation alone does not resurrect pending work; TASK-003a preserves existing auth rules and requires connected caller/test closure. The existing `grouped_reads_need_the_owner_grant` writes its only two source facts while the owner has no query grant, waits eight seconds with a three-second after, and verifies a retained query denial. The changed scheduler now records a terminal error and consumes that item. The test then swaps to a trace-only query grant without sending more source activity; this phase can observe no new result without attempting any query, so it no longer proves the object-scope denial advertised in its rustdoc. Finally it restores workload and calls `Deployment::group_execution`, which only returns Completed with a visible result (`support/mod.rs:386–417`). With pending consumed and no new ACK, that wait times out. `group_executions` (`verify_in_real_time.rs:2776–2803`) reads only Bifrost results, so the preceding empty-result checks mask the terminal SQL execution.

  **Consequence:** a repository security journey is incompatible with the new production lifecycle and fails under normal scheduling, while its unrelated-scope denial phase is vacuous. This is a test/acceptance regression, not an assertion that production grants were widened.

  **Smallest correction:** update this existing journey to inspect latest-binding/execution status after each real denial, asserting the structured 403 error and absent snapshot/result/dispatch. Create matching source activity while the trace-only role is installed so its object denial actually runs, and assert that refusal too. After restoring workload, send new matching source activity to schedule the successful snapshot; adapt source-count/value expectations to those actual facts. Preserve the owner-attributed denial audit and no-provider-call checks. Reuse current SDK/status/group helpers; no production lifecycle or authorization change is needed.

  **Focused closure:** run exactly one selector with tracing and the managed Postgres wrapper, sequentially with any other PostgreSQL work:

  ```bash
  WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_reads_need_the_owner_grant)"'
  ```

  This reviewer requested root coordination for this rerun and did not launch it. The finding is source-established; runtime outcome remains to be independently confirmed.

### Low / Defense In Depth

No optional hardening is proposed.

### Positive Controls

- Actual stored owner identity and accepted grants remain distinct; SYSTEM adds attribution only.
- Denied/incomplete queries cannot enqueue partial source evidence or substitute an empty accepted snapshot.
- TenantConn/RLS and exact frozen source predicates independently bind scheduling and snapshots.
- Authority-free refusal records are terminal and excluded from ordinary claim/retry/Operator paths.
- Status readback retains cards:read and tenant enforcement; result payloads retain separate query authorization.
- Shared decoders retain type/unknown-field/duration checks and server-side unresolved-path rejection.

## Verification limits

This is a static review. No source changes, heavy build, PostgreSQL test, provider call, deployment exercise, dependency vulnerability scan or benchmark was performed. Candidate-supplied R1 evidence reports focused P1/P2/P3 and decoder/spec checks passing; that evidence was compared with current source assertions, not independently rerun here. The `grouped_reads_need_the_owner_grant` selector is absent from the supplied focused run ledger. No manifest/lockfile changes in the cumulative range create a new dependency audit obligation.

Accepted ACK/visibility, notification crash loss, retention/outbox loss, postponement, overlap/settlement-order and backlog limits are excluded. REQ-015/AC-009 remain pending. No new protocols, hard-finality guarantee or speculative safeguard is requested.

**Final result: FAIL. Proposed material finding ledger: SEC-001 only.**

Skipped runtime verification; the concrete remaining risk is a failing and partly vacuous security journey after the approved terminal-denial change.
