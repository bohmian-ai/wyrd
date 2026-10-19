# TASK-001 cumulative re-review — FIX_REQUIRED

## Immutable subject and authority

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Base and merge base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `9f9f7743311aec0f2191f775e7bf054aca00ff91`.
- Subject: full cumulative base-to-candidate range, including original implementation and R1–R3. The latest fix range was navigation only.
- Authority: approved [spec revision 15](../../spec.md), approval `4a4ea57b9`, [original task](../../tasks/TASK-001-grouped-verification.md), AGENTS.md, agent rules and applicable authorities recorded by the independent reviewers.
- Prior [R1](../task-001-e4b1e313a-20261017/verdict.md), [R2](../task-001-47e0c9ab7-20261018/verdict.md) and [R3](../task-001-ae9c5a6ae-20261018/verdict.md) verdicts, validation ledgers, remediation tasks and evidence were source-checked cumulatively.

The owner's decisions are authority, not findings: identities are uncapped and the ceiling stops payload reads; group timing uses PostgreSQL-anchored managed `wyrd_ingested_at`; membership/count/values share one statement; prefer making invalid states inexpressible. The accepted newest-64/no-continuing-cursor policy remains. Pre-release definitions change in place. TASK-002, TASK-003 and REQ-015 capacity qualification are excluded. The original task's older revision/private mechanics yield to the approved revision and explicit decisions.

## Independent review topology

Seven fresh discovery agents independently inspected the cumulative subject without another current reviewer's conclusions or an intended verdict. Root orchestrated and ran verification; root did not validate its own findings.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/tenancy | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | PASS |
| Data/concurrency | [domain-review-data-concurrency.md](domain-review-data-concurrency.md) | FAIL |

A fresh [focused follow-up](followup-review.md) was required: discovery exposed a unique replay-time path, different closure assessments of queued results, and a common input/result authority cause not fully traced by every reviewer. It resolved those uncertainties, including why an input marker alone cannot establish result-only provenance. A separate fresh structured Ponytail validator independently checked all reports, proposals, callers/writers and corrections. Its [final ledger](findings-validation.md) resolves the disagreements from source rather than vote. All required reports are complete.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence and limit | Result |
|---|---|---|---|
| REQ-001/011: optional group_id across Rust/Python/TS, HTTP/MCP; reserved typed/source slot and renamed success | Shared Observe/Target and typed request; shared slot/server validation; SDK projections | Fresh three-language grouped journeys and MCP; retained schema/registration/OpenAPI proof | PASS |
| REQ-014/001/005: authenticated owner copied without producer Postgres IO; ownerless/unbound invocation allowed | Principal owner accessor; VerificationControl and ResultPayloadBuilder; existing outbox | Producer trace and fresh support/grant/isolation journeys | PASS |
| REQ-002/013: whole-slot/outcome grammar, exact same-site source bindings, ambiguity/cycle/undeclared refusal | SourceReference, EffectiveSpecs/GroupSite/source_binding and BindingProjector | Coding journeys, source/schema/registration proof | PASS |
| REQ-002: aligned ordered values, null absence and result-only evidence | Sources/Payloads and Ended order using managed time then ID | Fresh units and frame/coding journeys assert exact IDs and ID-aligned values | PASS for projection; queued authority below fails |
| REQ-003/004: accepted newest page, fixed cursor, full identity activity and PostgreSQL timing | candidates MAX, uncapped Sources::read, PostgreSQL due.clock, managed ingest column | Four fresh PG seams and duration units; no cursor/cap challenge | PASS for these mechanisms; replay activity below fails |
| REQ-003/004/005: serial immutable snapshots, retry, restart/coalescing and late known result | Existing queue/coverage/leases/unique outstanding fence; stored snapshots | Fresh Rust recovery/late-work and visible-before-selected PG case | PASS for known-row and serialization mechanisms |
| REQ-002/005: queued result must be selected before it enters source activity/values | Earlier group_submissions map used by later analytical read | Known-row seam passes but misses acceptance after the map | FAIL — FIND-1 |
| REQ-001/005/009: refused/unaccepted submissions cannot affect source count, activity or values | Input queue_execution is recorded but discarded by source surfaces; only known refusals excluded | Settled-refusal cases pass but wait for queue acceptance | FAIL — FIND-2 |
| REQ-001/003/004 and J-001: replay has no extra element or idle refresh | Payload/member dedup exists; Ended uses later physical copy times | Current replay occurs before first review; unit duplicates share one time | FAIL — FIND-9 |
| REQ-006/009 task share: one-cut membership/count/value; payload-only ceiling; exact N allowed and overflow inconclusive | Window distinct count and CASE payload gate; synchronous snapshot | Five units, at-ceiling/refusal PG seam and media journeys pass | PASS for read/ceiling mechanism, conditional on eligibility correction |
| AC-006: failed/truncated internal query defers and retries without partial snapshot | Shared ScheduledQueryCaller terminal/EOF owner and test fault | Fresh partial-read poll/commit/retry seam; query regressions | PASS |
| Previous independent selected-payload-omission proof | Separate payload cuts/completeness machinery deleted under owner decision | One statement makes the previous independent-cut invalid state inexpressible | PASS / superseded; no replacement guard required |
| REQ-008: empty each inconclusive globally; normal comparison/decide preserved | Shared Vala engine and existing evaluator | Retained engine proof; fresh empty/frame SDK/MCP scenarios | PASS |
| REQ-010: first baseline, verdict-change-only dispatch, immutable bounded context | Existing transactional queue completion/dispatch; group on_failure refusal | Fresh refund journeys and retained SQL settlement proof | PASS for mechanism; incorrect source can still feed it |
| REQ-011: accepted-owner source grants and ordinary declared tool/query grants | Caller::accepted, authenticated ScheduledQueryCaller, HostedTools | Fresh owner-denial/regrant, coding negatives, MCP and query-lifecycle cases | PASS |
| AC-003: pinned IDs before later span flush, then live query and unchanged IDs | HeldReview fixture synchronization; narrow production live query | Fresh Rust coding journey exercises corrected ordering | PASS; FIND-8 temporal gap closed |
| REQ-011 schema/declaration/status parity and authority prose | Source string/four typed objects; typed coverage/settings/status; design/evaluation | Fresh codegen/lints; retained served schema/type proof | PASS |
| Mandatory rustdoc | Original sites, FrameAgent and Support::tick Errors/partial progress documented | Standards and maintainer source review PASS | PASS; FIND-6 closed |
| INV ownership, tenancy, portability and non-goals | Existing queue/query/registry/evaluator/outbox owners extended; thin SDKs | Fresh deps/tenant checks and cumulative independent audits | PASS apart from source-authority/activity findings |

## Validated finding ledger

| ID | Classification / disposition | Consequence and selected correction |
|---|---|---|
| FIND-TASK-001-1 | INCORRECT / REVISED | A result from a queued execution accepted after the initial SQL map can be treated as direct and pinned before selection. Validate captured result classification with a fresh observation through the same existing TenantConn after the complete read; defer the entire affected stale read. |
| FIND-TASK-001-2 | INCORRECT / REVISED | A submitted input visible before asynchronous acceptance can contaminate a judgment or falsely overflow before later refusal. Use its existing queue_execution discriminator and known non-refused accepted identities before count/value/activity production. |
| FIND-TASK-001-9 | INCORRECT / CONFIRMED | A later physical replay advances through/idle despite unchanged logical membership. Canonicalize earliest-visible logical input/authoritative-result activity consistently before lifecycle/coverage. |

FIND-1 and FIND-2 are one shared source-authority correction in GroupPoller/Sources and the existing queue. FIND-9 is a distinct normalization cause at that same owner. Exact locations, producer-to-consumer evidence, all proposal dispositions and focused proof are in [findings-validation.md](findings-validation.md). No new store, connection, schema, producer lookup, terminal readiness barrier or specification decision is required. Nonmaterial presentation notes are not findings or remediation work.

## Prior finding closure

| Prior ID | Disposition |
|---|---|
| FIND-1 | Old completion watermark/hour defect closed; cursor superseded by owner decision. Known queued null→selected transition closed. Retained only for a newly queued result crossing the initial classification. |
| FIND-2 | Physical prefix, broad payloads, count/list split, capped activity and settled-refusal finality/ceiling defects closed. Independent-payload-omission proof superseded by one-read decision. Retained for pre-acceptance input eligibility. |
| FIND-3 | CLOSED — checked duration arithmetic and PostgreSQL-derived activity. |
| FIND-4 | CLOSED — runtime/schema/generated source forms agree. |
| FIND-5 | CLOSED — accepted owner grants and ordinary audit. |
| FIND-6 | CLOSED — all prior mandatory rustdoc sites, including tick. |
| FIND-7 | CLOSED — active design/evaluation and narrow live-query implementation agree. |
| FIND-8 | CLOSED — registration, grants, MCP media, internal partial-read recovery and now deterministic pin-before-span proof. |

## Fresh verification

All tests/builds/checks used mise. Cargo lanes ran sequentially. All database runs used the prescribed wrapper and both migrations; there were no runtime or selector failures and no synthetic load. Commands below record the exact focused selection:

```sh
mise run build:monty-worker
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::groups::tests::decoding_marks_tuples_over_the_ceiling) | test(=verification::groups::tests::groups_end_count_and_order_by_their_rows) | test(=verification::groups::tests::largest_accepted_durations_judge_without_overflow) | test(=verification::groups::tests::one_read_selects_eligible_rows_and_gates_payloads) | test(=verification::groups::tests::values_unwrap_inputs_and_outcomes)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all -E "test(=pg_verification_groups::a_visible_result_counts_once_its_settlement_selects_it) | test(=pg_verification_groups::finality_anchors_on_the_first_eligible_row) | test(=pg_verification_groups::partial_group_reads_enqueue_nothing_until_a_complete_poll) | test(=pg_verification_groups::refused_submissions_do_not_count_toward_the_ceiling)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes) | test(=verify_in_real_time::grouped_refund_changes_verdict) | test(=verify_in_real_time::grouped_coding_uses_declared_sources) | test(=verify_in_real_time::grouped_sampled_frames_are_bounded) | test(=verify_in_real_time::grouped_conversations_recover) | test(=verify_in_real_time::grouped_reviews_cover_late_work) | test(=verify_in_real_time::grouped_reads_need_the_owner_grant)"'
mise run py:setup
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m "integration and not local_dev" tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes tests/integration/test_verify_in_real_time.py::test_grouped_refund_changes_verdict tests/integration/test_verify_in_real_time.py::test_grouped_coding_uses_declared_sources tests/integration/test_verify_in_real_time.py::test_grouped_sampled_frames_are_bounded tests/integration/test_verify_in_real_time.py::test_grouped_conversations_recover'
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped (support resumes|refund changes verdict|coding uses declared sources|sampled frames are bounded|conversations recover)$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(/^query::/) | test(=verification::pg_tests::grouped_frames_over_mcp_are_bounded) | test(=verification::pg_tests::grouped_verification_discover_act_observe)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query_lifecycle::grpc_lifecycle_controls_are_tenant_scoped_and_audited) | test(=query_lifecycle::denied_describe_is_audited_before_admission)"'
mise run fmt:check
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run py:format:check
mise run py:lints
mise run ts:format:check
mise run ts:lints
git diff --check
```

PASS: 5 group units, 4 PG group seams, 7 Rust, 5 Python, 5 TypeScript grouped journeys, 11 MCP/query cases and 2 query-lifecycle cases: **39 focused tests**. All listed setup/check lanes passed; lints includes both workspace and shipped-server commands, and deps includes its inline-source gate. Check-only formatting preserved immutable source. Build/codegen outputs left tracked files unchanged.

These runs do not execute the newly identified interleavings or replay-after-review path. Counterexamples are independently source-validated, not executed reproductions. Broad unit, typing, docs and OpenAPI aggregates were not rerun this round; prior evidence and source were checked without claiming fresh execution. Capacity, staging pressure and TASK-002/003 were excluded. No commit/merge/push/deploy occurred; only this review directory was written, with candidate/staged/tracked source unchanged.

## Decision

**FIX_REQUIRED.** Route [TASK-001-R4-source-authority-and-replay.md](TASK-001-R4-source-authority-and-replay.md) directly to `$wyrd-implement`. Later review must reassess the full cumulative candidate against revision 15, original task and all retained findings.
