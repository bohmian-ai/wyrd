# TASK-001 verdict: FIX_REQUIRED

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
- Reviewed the complete base-to-candidate range. The merge base equals the specified base; `base...candidate` and `base..candidate` therefore identify the same cumulative changes. Candidate source remained unchanged throughout review.
- Original task: [TASK-001-grouped-verification.md](../../tasks/TASK-001-grouped-verification.md).
- Approved behavior: [spec.md](../../spec.md), revision **13**, as explicitly selected by the user. Both commits contain a revision-14/draft header; the history records revision-13 approval and says revision 14 leaves requirements/acceptance unchanged while adding a schema-reset exception. This review uses the approved revision-13 requirements, excludes that draft exception as authority, and records the header mismatch as a limitation. It neither approves revision 14 nor invents retained-table upgrade scope.
- TASK-002 schedule binding inputs, TASK-003 outbox admission/pressure/tenant progress, and REQ-015 capacity qualification are excluded. TASK-001 discovery scan progress and source-read bounds remain in scope.

Applicable repository, architecture, security, testing and language authorities are mapped in the independent reports. The orchestrator reconciles those reports and packages only the independently validated ledger; it does not substitute its own findings.

## Independent review results

| Required role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security and tenancy domain | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | FAIL |
| Persistent data and concurrency domain | [domain-review-data-concurrency.md](domain-review-data-concurrency.md) | FAIL |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

All required reports are present. Discovery was independent; only the completed fresh independent behavior report entered validation. No reviewer filled multiple roles.

Follow-up was required because the data report proposed SQL-prefix incompleteness that the invariant report withheld, and the standards permission PASS conflicted with the security report. The fresh follow-up traced the shared query boundary. It confirmed mixed-tuple overread and the wrong permission authority, and limited the SQL-prefix claim to a changed-visibility interleaving. The validator independently resolved both conflicts. Correct transport-terminal handling is preserved; it is not a finding.

## Reconciled acceptance matrix

PASS means the scoped source and available evidence support the obligation; it does not certify unexecuted checks. FAIL rows identify the validated closure boundary. Implementation and verification details are retained in the linked independent matrices and validation ledger.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/011 optional `group_id`, same spelling, ordinary invocation and omission | Shared Observe/Target, VerificationRequest, Python/TS projections, managed rows | All 15 SDK grouped stories; MCP; served OpenAPI rerun green | PASS |
| REQ-001 reserved slot refusal and renamed-slot success across required registration surfaces | Shared `validate_slot` reserves typed/source `group_id` | Unit validation exists; required SDK/HTTP/MCP registration proof missing | FAIL — finding 8 |
| REQ-014 authenticated owner metadata without producer lookup/write | `Principal::owner_service_uid`, existing input/result staging and runner attribution | Producer-to-sink source trace; support/ownerless/UID paths | PASS |
| REQ-002 whole-slot/outcome grammar only; no child/status/reason extension | Existing token parser extended with SourceReference; shared validation | Parser and child/cycle/unbound negatives inspected | PASS |
| REQ-013 exact source binding resolution, same occurrence and participating ambiguity | EffectiveSpecs/source_binding/GroupSite; BindingProjector | Main implementation present; required reference-form and ambiguity/missing-target journeys incomplete | FAIL — finding 8 |
| REQ-002 deterministic order, alignment, authoritative queued results and genuine missing-value nulls | Ended/Payloads and queue selected_results | Frames/coding source and unit proof; late-result closure required separately | PASS for existing projection; findings 1–2 govern selection |
| REQ-003/004 one outstanding group, immutable snapshots/retry, fenced settlement | Queue partial unique index, SKIP LOCKED, stored GroupCoverage/inputs, lease fence | SQL and Rust worker/replica/restart proof | PASS |
| REQ-003–005 uncovered/late activity reaches subsequent serial whole-group review | Discovery filters before queue coverage | Existing tests do not force pre-completion-time late visibility | FAIL — finding 1 |
| REQ-004 retained history recovers and bounded discovery progresses | One-hour source cutoff and repeated earliest-64 page | Short restart passes; historical outage/page starvation not covered | FAIL — finding 1 |
| REQ-004 pinned Trigger settings and safe timing over accepted domain | GroupEndSettings; Ended and discovery arithmetic | Ordinary boundaries pass; accepted extreme duration panics | FAIL — finding 3 |
| REQ-003 finality excludes activity without producer refusal or deletion | Background eligible-row filter; ordinary producer path | Refund finality journey rerun green | PASS |
| REQ-005 existing outbox durability/loss semantics; no new receipt/lifecycle store | Existing ScribeOutbox and queue extended | Source trace; Rust frames flush-before-restart variant | PASS |
| REQ-006/009 bounded logical membership, overflow before payload reads, no partial snapshot | Raw-row skip and broad payload statements bypass intended selected membership | Normal 20/21 case passes; mixed tuples/raw cap/changed-cut gaps remain | FAIL — finding 2 |
| REQ-008 empty Assert each is inconclusive everywhere; ordinary nonempty decide preserved | Shared evaluation engine empty-selection rule | Engine proof and SDK empty/frame stories | PASS |
| REQ-002 explicit live Agent span reads, no automatic expansion | Trace IDs projected; existing hosted bifrost.query tool path | Main coding flow passes; non-trace, pin-before-late-span and independent-denial proof absent | FAIL — finding 8 |
| REQ-011 source reads preserve accepted owner's table/payload permission | GroupPoller manufactures SYSTEM grants before snapshot reads | Existing tool denial tests exercise a different later consumer | FAIL — finding 5 |
| INV-003 exact tenant/owner/subject/Verifier/group isolation and nullable owner invocation | Pinned tuple predicates, tenant query/control boundaries | Wrong-owner/tenant/new-version/ungrouped stories rerun green | PASS; does not waive finding 5 |
| REQ-010 baseline, unchanged verdict, changed verdict and replay-safe dispatch; ordinary on_failure | Existing queue completion/previous-group/dispatch transaction | Refund journey and SQL settlement proof | PASS |
| REQ-011 bounded execution/binding inspection and existing agent routes/scopes | Shared status contracts, existing HTTP/MCP owners | MCP/OpenAPI rerun green | PASS |
| REQ-011 schemas/declarations describe accepted input forms | Rust string source versus phantom generated TS object | Codegen reproducibility does not establish acceptance parity | FAIL — finding 4 |
| AC-004/006 agent media negatives and deliberate failed/truncated group reads | Existing media/query/runtime owners available | Added MCP story is JSON-only; no deliberate group read failure/retry journey | FAIL — finding 8 |
| AGENTS §16 mandatory rustdoc | New duration methods, tuple fields and test module lack required docs | Source-validated omissions | FAIL — finding 6 |
| Active authority agrees with approved tool exception | Runtime/site permit bifrost.query; design/reference still prohibit tools | Source/prose comparison | FAIL — finding 7 |
| INV-001–005 structural/non-goal boundaries, server ownership, portability and one write path | Existing scheduler/queue/query/evaluation owners extended; thin SDKs | Cumulative diff and owner/caller audit | PASS apart from explicit findings above |

Literal `on_failure: []` acceptance and import-placement observations did not establish a blocking consequence under this skill and are not remediation findings. Missing Python/TS restart controls alone are not a separate defect; the Rust process-owned frame proof is credible. The validator narrowed the original broad proof proposal accordingly.

## Validated finding ledger

The full source chains, discovery IDs, evidence, corrections and focused proofs are in [findings-validation.md](findings-validation.md). These are the only retained findings:

| ID | Status / classification | Location and observable consequence | Selected correction owner |
|---|---|---|---|
| FIND-TASK-001-1 | REVISED / INCORRECT | `verification/groups.rs:53–57,158–178,217–240`: completion-time suppression, age cutoff and repeating first page hide retained uncovered groups | Existing group discovery/binding cursor plus queue coverage; bounded continuing scan |
| FIND-TASK-001-2 | REVISED / INCORRECT | `verification/groups.rs:159–168,263–308,376–438`: physical-row skip loses required outcomes; payload queries fetch excluded sources and can omit selected values | Existing logical source selection and selected-results authority, constrained complete payload reads |
| FIND-TASK-001-3 | REVISED / INCORRECT | `card/trigger.rs:94–115`, `verification/groups.rs:218–220,636–644`: legal large durations repeatedly panic shared scheduling | Existing timing owner with elapsed/checked arithmetic |
| FIND-TASK-001-4 | CONFIRMED / INCORRECT | `card/verifier/mod.rs:128–176`, generated `card-types.ts:2364,2429–2431`: typed authoring permits undecodable source object | Source-owned schema projection and existing generation pipeline |
| FIND-TASK-001-5 | REVISED / VIOLATION | `verification/groups.rs:130–150`: manufactured SYSTEM grants bypass accepted owner's source-read permissions | Caller::accepted and authenticated scheduled-query owner; retain Drift exception |
| FIND-TASK-001-6 | CONFIRMED / VIOLATION | Duration Errors docs, SourceText/Forward fields and new test module lack mandatory rustdoc | Existing changed items; batch documentation correction |
| FIND-TASK-001-7 | REVISED / VIOLATION | Design `:1200,1405–1406` and evaluation reference `:55–68` contradict approved check-Agent query behavior | Current protocol/reference descriptions |
| FIND-TASK-001-8 | REVISED / MISSING | Existing SDK/MCP story owners omit explicit registration, coding/live-read, agent-media and query-failure acceptance variants | Existing real-server story/provider/query-fault owners; exact focused cases |

All 22 discovery/follow-up proposals have an explicit validation disposition. Duplicate causes are consolidated. Rejected portions are omitted, not converted to optional recommendations. There is no supplied prior verdict or FIND ledger to close; this is the initial cumulative review. No validated correction requires a new material specification decision.

## Independent verification and limits

All Cargo-backed lanes ran sequentially through mise. Prerequisites passed: `mise run build:monty-worker`, `mise run py:setup`, `mise run ts:build:testing`.

Actual commands, including the managed Postgres/migration boundary:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_conversations_recover)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes) | test(=verify_in_real_time::grouped_refund_changes_verdict) | test(=verify_in_real_time::grouped_coding_uses_declared_sources) | test(=verify_in_real_time::grouped_sampled_frames_are_bounded)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::grouped_verification_discover_act_observe)" && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_openapi_contract::grouped_verification_contract_is_served)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes tests/integration/test_verify_in_real_time.py::test_grouped_refund_changes_verdict tests/integration/test_verify_in_real_time.py::test_grouped_coding_uses_declared_sources tests/integration/test_verify_in_real_time.py::test_grouped_sampled_frames_are_bounded tests/integration/test_verify_in_real_time.py::test_grouped_conversations_recover'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped (support resumes|refund changes verdict|coding uses declared sources|sampled frames are bounded|conversations recover)$"'
```

| Run | Actual selected count | Result |
|---|---|---|
| Rust recovery | 1; 125 skipped | PASS, 25.029 s |
| Rust union of four exact expressions | 4; 122 skipped | PASS; support 9.275 s, coding 15.543 s, frames 31.348 s, refund 51.646 s |
| MCP | 1; 23 skipped | PASS, 8.564 s |
| Served OpenAPI | 1; 270 skipped | PASS, 0.988 s |
| Python exact node IDs | 5 | PASS, 112.51 s |
| TypeScript exact prose-name alternatives | 5; 19 skipped | PASS, 120.58 s |

Managed Wyrd/Vala migration tests passed in each wrapper. There were no failing tests to diagnose. `git diff --check` passed. The appended fmt/lint/codegen/boundary/docs/language-check claims were inspected but not independently rerun during this source review. No aggregate, capacity lane, synthetic host load, production edit or test edit was performed.

Green results establish the exercised test bodies, not the uncovered interleavings or omitted required variants. The changed-cut SQL-prefix path is source-validated, not an executed reproduction. No dedicated failed/truncated group-read journey exists. These limits are carried into the remediation rather than called PASS.

## Disposition

**FIX_REQUIRED.** Route [TASK-001-R1-close-grouped-verification-gaps.md](TASK-001-R1-close-grouped-verification-gaps.md) directly to `$wyrd-implement`. It packages all eight validated findings under approved behavior. No implementation, merge, push, deployment or review-artifact commit was performed.
