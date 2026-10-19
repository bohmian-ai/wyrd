# TASK-001 cumulative re-review: FIX_REQUIRED

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`.
- Reviewed the full cumulative base-to-candidate range. The merge base equals the base, so `base...candidate` and `base..candidate` cover the same changes. The preceding candidate's remediation diff was navigation only. HEAD remained immutable; production code and tests were not edited.
- Original task: [TASK-001-grouped-verification.md](../../tasks/TASK-001-grouped-verification.md).
- Approved specification: [spec.md](../../spec.md), revision **13**, under explicit user authority. The checked-in draft-14 header/history discrepancy remains recorded; this review does not approve that draft's schema-reset exception or invent retained-upgrade scope.
- Prior review: [verdict](../task-001-e4b1e313a-20261017/verdict.md), [validated findings](../task-001-e4b1e313a-20261017/findings-validation.md), and [TASK-001-R1 with evidence](../task-001-e4b1e313a-20261017/TASK-001-R1-close-grouped-verification-gaps.md).
- The explicit user amendment recorded in `f983d7b95` supersedes the earlier finding-1 scan-progress recommendation: first source within pinned finality, newest activity first, only the newest bounded page per poll, accepted `ponytail:` ceiling, no continuing cursor. This accepted discovery limit is not reflagged.
- TASK-002 schedule inputs, TASK-003 outbox admission/pressure/tenant progress, and REQ-015 capacity qualification are excluded. The source-execution ceiling and grouped-read failure proof remain TASK-001 obligations.

The orchestrator established the subject, coordinated verification, compared reports and packaged the independent validator's ledger. It did not substitute its own findings.

## Independent reports and follow-up decision

| Required role | Report | Result |
|---|---|---|
| Fresh behavior reviewer | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Fresh invariant reviewer | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/tenancy domain | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | PASS |
| Data/concurrency domain | [domain-review-data-concurrency.md](domain-review-data-concurrency.md) | FAIL |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

All seven discovery reports and independent validation are complete. Discovery reviewers did not receive other current-round conclusions. No reviewer filled multiple roles.

No focused follow-up was needed. Reports traced the repeated ceiling defect to the same count/identity/refusal owner rather than leaving a common source unexplored. Closing the original documentation sites while finding a new helper omission is complementary coverage, not conflicting source evidence. Validation resolved the proof-facility reachability qualification from dispatch source: the existing HTTP after-batch switch does not affect GroupPoller's internal query path. No unresolved conflict remains.

## Reconciled cumulative acceptance matrix

PASS identifies source-supported scoped behavior with the cited proof; it does not certify unexecuted lanes. Full source/caller matrices and limits are retained in the reports.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/011 optional exact `group_id`, ordinary omission and existing producer routes | Shared Observe/Target/request; Python/TS/native projections; managed rows | SDK support/coding, Rust grouped stories, MCP and served contract | PASS |
| REQ-001 reserved typed/source slot and renamed-slot success | Shared slot validation and server validate_request → VerifierSpec::validate | Three SDK coding registration cases; raw HTTP refusal/non-persistence | PASS |
| REQ-014 authenticated nullable owner with no new producer Postgres IO | Principal owner accessor, existing staging/result projection | Producer-to-sink source trace; ownerless/UID/owner isolation stories | PASS |
| REQ-002 minimal whole-slot/outcome grammar and no implicit span expansion | Existing token parser, SourceText, shared validation and projection | Child/status/reason refusals; trace-ID and declared-query stories | PASS |
| REQ-013 exact same-site binding/name/version resolution, ambiguity/cycles and isolation | EffectiveSpecs/source_binding/GroupSite; BindingProjector; exact tuple predicates | Sibling/external/missing/ambiguous/cycle fixtures and newer-version/tenant/owner negatives | PASS |
| REQ-002 ordered aligned values, authoritative queued outcomes and genuine nulls | Ended/Payloads and queue selected_results | Unit mapping/order proof; coding/frames and null→actual late-work story | PASS for value projection; selection governed by finding 2 |
| REQ-003/004 serial outstanding work, immutable snapshots/retries and history | Binding claims, queue unique index/coverage, stored inputs and lease fence | SQL settlement proof; Rust late-work, frames and recovery stories | PASS |
| Amended finding 1: finality-bound newest-page discovery, queue coverage authority | Removed completion watermark/slack; checked finality HAVING/order/page | Late-work/coalescing and null→actual follow-up; accepted page bound | PASS |
| REQ-004 safe accepted duration domain and pinned settings | Checked timestamps and elapsed comparisons; GroupEndSettings | Maximum-duration unit and neighboring ordinary binding journey | PASS |
| REQ-003 finality remains background eligibility, no producer refusal/deletion | Discovery/Ended filter; ordinary producer path unchanged | Refund finality story | PASS |
| REQ-005 existing outbox loss/visibility and no new receipt/lifecycle store | Existing ScribeOutbox and queue ownership | Static owner trace; published frame restart positive variant | PASS |
| REQ-006/009 bounded actual eligible membership and pre-payload overflow | Preliminary counts classify before refusal and later identity cut | Stable 20/21 cases pass; refused-row and changing-cut counterexamples remain | FAIL — FIND-TASK-001-2 |
| REQ-004/006 incomplete selected payload coverage defers/retries | Existing exact-ID statements and complete guard | SQL-string/value units do not exercise incomplete selection recovery | FAIL — finding 2 closure proof |
| REQ-008 empty each is inconclusive everywhere, nonempty decide preserved | Shared evaluator empty-selection rule | Engine proof; SDK/MCP zero-selection cases | PASS |
| REQ-002/011 live check query is explicitly declared and ordinarily authorized | Narrow validator/runtime tools; existing HostedTools/query owner | Independent tool/query denials, non-trace/no-judge paths and live spans | PASS |
| REQ-011 source reads preserve accepted owner grants and audit | Caller::accepted plus authenticated ScheduledQueryCaller | No capability/unrelated scope/regrant journey and denial audit | PASS |
| REQ-010 baseline/change-only reactions, replay fence, ordinary on_failure | Existing transactional completion/previous verdict/dispatch | SQL settlement proof; refund dispatch/history story | PASS |
| REQ-011 bounded inspection, HTTP/MCP and generated/runtime parity | Shared status/settings/coverage; source-owned schema union | Served OpenAPI, schema/decode tests, TS negative type evidence/typecheck | PASS |
| AC-004 agent media failure/zero/ceiling/isolation/scopes | Existing transfer/upload/queue owners | New MCP media journey and three SDK frame stories | PASS for stable membership; finding 2 remains |
| AC-006 deliberately partial/truncated grouped read, no partial enqueue, later retry | Correct shared terminal consumer and group error propagation | Grant denial is before-stream; no deliberate partial group-read proof | FAIL — FIND-TASK-001-8 |
| Server owns registration rules; shared artifact-manifest validation | Server/loader call same pure validator before writes | Focused HTTP conflicting-path and spec tests | PASS |
| AGENTS §16 required rustdoc | Original listed omissions fixed; new FrameAgent::submit lacks Panics | Source-local upload assertion versus helper docs | FAIL — FIND-TASK-001-6 |
| Active protocol/evaluation guidance matches approved narrow tool exception | Design/evaluation text synchronized | Validator/hosted source and prose comparison | PASS |
| INV-001–005 ownership, tenancy, portability, one write path and non-goals | Existing scheduler/query/queue/evaluator/Operator owners; thin SDKs | Full cumulative owner/caller/dependency audit | PASS apart from explicit selection/proof/docs findings |

The additional registration fixes extend existing server authority and remove duplicate loader-only validation; reviewers found no separate pipeline or unrelated product change. The only dependency delta is installed workspace `sha2` as an MCP dev dependency for evidence hashing.

## Prior-finding closure and final ledger

| Stable ID | Cumulative disposition |
|---|---|
| FIND-TASK-001-1 | CLOSED under explicit user amendment; late-work proof and removed false watermarks inspected. No cursor demand survives. |
| FIND-TASK-001-2 | PARTIAL / OPEN. Old raw-row skip and broad payload predicates corrected; eligible membership and cross-cut ceiling remain wrong, with incomplete-selected-coverage proof absent. |
| FIND-TASK-001-3 | CLOSED. Checked/elapsed arithmetic; extreme-duration and neighboring-work proof. |
| FIND-TASK-001-4 | CLOSED. Four typed objects or source string; schema/runtime/served/type-negative parity. |
| FIND-TASK-001-5 | CLOSED. Accepted-owner query authority, denial/regrant/audit proof; Drift exception preserved. |
| FIND-TASK-001-6 | Original sites CLOSED; OPEN only for new FrameAgent helper's missing panic contract. |
| FIND-TASK-001-7 | CLOSED. Active design/evaluation matches declared bifrost.query-only exception. |
| FIND-TASK-001-8 | PARTIAL / OPEN only for deliberate partial grouped-read rejection/retry proof. Other required registration, coding and agent-media variants are present. |

The independent validator dispositioned all 11 discovery proposals, deduplicated shared causes and retained only:

| Finding | Validated status / classification | Evidence, consequence and selected correction |
|---|---|---|
| FIND-TASK-001-2 | REVISED / INCORRECT | `verification/groups.rs:139–147,246–297,320–358,631–637,749–752`: refused rows create false overflow; identities from a later cut can exceed the earlier count and be read/judged without an overflow marker. Make actual bounded eligible membership authoritative before payload reads, reusing queue refusal/selected-result authority and existing limit outcome. |
| FIND-TASK-001-6 | CONFIRMED / VIOLATION | MCP `verification.rs:1547–1555,1585–1588`: new FrameAgent::submit asserts upload success but lacks required Panics docs. Add the panic contract without changing behavior. |
| FIND-TASK-001-8 | REVISED / MISSING | Grouped-read proof still substitutes pre-stream RBAC refusal for partial delivery/no-enqueue/retry. Prove the actual internal group/query/queue boundary with focused local test plumbing; the HTTP fault switch does not reach it. |

Full discovery IDs, producer/caller traces, rejected portions, exact evidence, correction ladders and closure proofs are in [findings-validation.md](findings-validation.md). There is no new finding ID or material decision requiring specification revision. Rejected demands for a continuing cursor, broad suites, a general fault harness or a broken-terminal diagnosis are not remediation advice.

## Independent verification

All Cargo-backed commands ran sequentially through mise. Prerequisites passed: `mise run build:monty-worker`, `mise run py:setup`, `mise run ts:build:testing`. Canonical `mise run ts:typecheck` also passed, including its production/testing native build dependencies.

Actual focused test commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::groups::tests::groups_end_count_and_order_by_their_rows) | test(=verification::groups::tests::over_ceiling_counts_join_the_group_end) | test(=verification::groups::tests::largest_accepted_durations_judge_without_overflow) | test(=verification::groups::tests::reads_select_only_listed_membership) | test(=verification::groups::tests::values_unwrap_inputs_and_outcomes)'
mise exec -- cargo nextest run --locked -p wyrd-spec --features server --lib -E 'test(=card::verifier::tests::input_slot_schema_matches_decoding) | test(=card::verifier::tests::served_input_slot_schema_matches_decoding) | test(=registry::submission::tests::validate_artifact_manifest_refuses_repeated_and_nested_paths)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes) | test(=verify_in_real_time::grouped_refund_changes_verdict) | test(=verify_in_real_time::grouped_coding_uses_declared_sources) | test(=verify_in_real_time::grouped_sampled_frames_are_bounded) | test(=verify_in_real_time::grouped_conversations_recover) | test(=verify_in_real_time::grouped_reviews_cover_late_work) | test(=verify_in_real_time::grouped_reads_need_the_owner_grant)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::grouped_frames_over_mcp_are_bounded) | test(=verification::pg_tests::grouped_verification_discover_act_observe)" && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_openapi_contract::grouped_verification_contract_is_served) | test(=pg_card_registration_route::conflicting_artifact_paths_are_refused)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes tests/integration/test_verify_in_real_time.py::test_grouped_coding_uses_declared_sources'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped (support resumes|coding uses declared sources)$"'
```

| Run | Actual selected / skipped | Result |
|---|---|---|
| Group reader units | 5 / 547 | 5 PASS |
| Source/served schema and artifact manifest | 3 / 764 | 3 PASS |
| Rust grouped journeys | 7 / 121 | 7 PASS; late-work 43.976 s, owner-grants 20.602 s |
| MCP support/media | 2 / 23 | 2 PASS; 8.158 s / 21.572 s |
| Server served schema/artifact refusal | 2 / 270 | 2 PASS; 1.044 s / 1.099 s |
| Python modified stories | 2 | 2 PASS, 20.16 s |
| TypeScript modified stories | 2 / 22 | 2 PASS, 25.29 s |

All 23 selected test cases passed; migration setup passed for each Postgres wrapper. There were no failing tests to diagnose. `git diff --check` and tracked `git diff --exit-code` passed. Other appended fmt/lint/codegen/boundary/docs claims were source-reviewed but not independently rerun. No synthetic load, broad aggregate or capacity lane was run.

Passing stable-input bodies do not disprove a count/identity visibility interleaving or a settled conflict at the ceiling. Those counterexamples are source-validated, not claimed executed reproductions. Existing terminal tests support the consumer's contract, but missing grouped partial-read and incomplete-selected-coverage proof remains explicit. Reviewers' static PASS rows are reconciled with these limits, not treated as runtime execution claims.

## Disposition

**FIX_REQUIRED.** Route [TASK-001-R2-source-selection-and-read-proof.md](TASK-001-R2-source-selection-and-read-proof.md) directly to `$wyrd-implement`. It packages only the three retained findings and batches documentation with behavioral/proof correction. Review performed no implementation, merge, push, deployment or artifact commit.
