# TASK-003A verdict

**FIX_REQUIRED**

Findings: **FIND-TASK-003A-1**, **FIND-TASK-003A-2**.
Remediation: [TASK-003A-R1-deterministic-checkpoint-proof.md](TASK-003A-R1-deterministic-checkpoint-proof.md), routed to `$wyrd-implement`.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Branch supplied: `task/wyrd-ui-main-integration/TASK-005`.
Base: `43c77a219fd10554e8c8663e980788b70f1a6296` (`43821f9d6^`).
Candidate: `c233b75456fad4f45c772ec242a9d115b4d35c6b`.
Reviewed complete cumulative `git diff 43c77a219fd10554e8c8663e980788b70f1a6296 c233b75456fad4f45c772ec242a9d115b4d35c6b`; base is ancestor, so this equals the first-review triple-dot range. HEAD and tracked source remained unchanged through review. Only this new report directory was written; no implementation, staging or commit was performed.

Approved contract: [spec.md revision 4](../../spec.md), REQ-015 / AC-010; original [TASK-003A](../../tasks/TASK-003A-execution-status-events.md) and [status design](../../design/verify-status-events.md), subordinate to repository/architecture authority. First acceptance review, no prior candidate findings or acceptance carried forward. Prior-finding closure and convergence assessment are inapplicable.

## Independent review coverage

[Review scope](review-scope.md) records the common navigation map, affected contracts, producers/owners/consumers and selection before discovery. All selected roles were separate fresh agents. The orchestrator did not act as a reviewer or validate its own findings.

| Role/report | Result | Coverage |
|---|---|---|
| [behavior-rev](task-review-behavior.md) | Behavioral PASS | Complete task matrix, real caller-to-public-query flows and exclusions; hosted synchronization acceptance revised by focused evidence below |
| [invariant-rev](task-review-invariants.md) | Functional PASS; historical TDD FAIL | Lease/commit/settlement, admission and cancellation siblings, row identity, checkpoint isolation; chronology preserved separately |
| [repo-rev](standards-review.md) | FAIL | All 23 changed files, SQL nested rules, Rust docs, cross-tier ownership/test seams and evidence |
| [system-rev](system-review.md) | FAIL, proposed SYS-1 | Batch failure, missing-reply fallback, shared-outbox failure/recovery and four modified acknowledgement tests |
| [domain-data-rev](domain-review-data.md) | PASS | SQL RETURNING/Admitted, transaction/concurrency/fence/attribution propagation across all commit owners |
| [domain-security-rev](domain-review-security.md) | PASS | Permissions, revocation/admission/PR fencing, tenancy, managed attribution and query authority |
| [followup-rev](followup-review.md) | RESOLVED, proposed FUP-1 | Specific slice/frame-order conflict and present independent diagnosis of all four resequenced tests |
| [ponytail-rev](findings-validation.md) | FIX_REQUIRED | Independent source validation, applicability/coverage, deduplication, correction burden, full reconciled acceptance and final ledger |

The validator confirms specialist coverage is complete. Maintainer specialist omission is valid: no substantial new/replacement owner or abstraction exists; concrete queue result and Scribe payload extensions reuse current owners, and ordinary maintainability is covered by behavior/repo reviewers. UI, SDK/wire/schema/migration/deployment specialists are inapplicable because those surfaces do not change. No selected report is missing.

[Claim comparison](claim-comparison.md) explains why one focused follow-up was necessary: shared lanes serialize slices, while frames inside a slice can submit concurrently. Follow-up and validation correct the broad discovery claim that both hosted two-stall setups reliably skip status rows. No vote determined the findings and no further discovery is justified.

## Reconciled acceptance

The complete requirement/source/assertion matrix is in [independent validation](findings-validation.md#reconciled-acceptance-matrix), with detailed assertions in behavior and domain reports. This table reconciles its outcomes; PASS denotes source/assertion acceptance subject to execution limits, not a fresh integration run.

| Obligation | Implementation and actual proof | Result |
|---|---|---|
| Pending/running cancellation; repeat/stale no-ops | inspection commits before conditional stage; public journey checks exact cancelled histories, inspection, released-worker absence and visible probe control | PASS |
| Claims, retry/exhaustion, park, release/defer and terminal statuses | runner maps only applied committed outcomes; lifecycle histories pin running/retrying/waiting/pending/completed/errored/timed_out, queue-not-waiting and exhaustion | PASS |
| Completion group commit and existing fallback | complete_many commits before replies; trigger-backed tests distinguish batched/single settlement and require one completed row with attempt 1 | PASS |
| Principal/admission/PR/access-loss cancellations | existing SQL UPDATE results carry identities and stored attribution to all existing commit owners; delayed admission, SQL equality and webhook newest-status assertions inspected | PASS |
| Exact status row and ordinary batching/retry | constructor sets sequence 0, event/status/identities/time and null content/correlation; journey checks fields; review-run unit verifies one shared frame, attribution and identical retry identities/bytes | PASS |
| Rollback/stale leases stage no event | postcommit placement and unchanged lease predicates traced; failed group retry and repeated/stale public histories pin no duplicate status | PASS |
| Checkpoint exclusion/recovery, tenant and permission boundaries | checkpoint reader explicitly filters event=checkpoint; queue/schema/fences unchanged; security/data reports trace actual capability and managed attribution owners | PASS production behavior |
| Existing acknowledgement fault proof remains valid | scripted held-producer/drain fixes are sound; two hosted double stalls can select a later status because concurrent same-slice checkpoint already passed | **FAIL — FIND-TASK-003A-2** |
| Changed-item rustdoc | Flusher::run and stored_cancellations lack required panic sections | **FAIL — FIND-TASK-003A-1** |
| No new storage/schema/endpoint/statement/transaction/flusher or excluded UI/direct behavior | cumulative diff extends existing statement results and owner staging only | PASS |
| Independent failure diagnosis | present system/followup reports independently diagnose all four edits; historical consultation/report is unproved | Current evidence available; historical violation retained |
| Test-first execution | task explicitly records late tests and PR production drafted before RED | Historical **VIOLATED**, not code-remediable |

## Validated findings and correction burden

Only independently confirmed/revised findings are retained. Full source traces, classifications, materiality, sibling/consumer analysis, smallest correction and focused proof are in [findings-validation.md](findings-validation.md#deduplicated-actionable-ledger).

| Stable ID | Sources/status | Required bounded correction |
|---|---|---|
| FIND-TASK-003A-1 | STD-001 / CONFIRMED / VIOLATION | Add two accurate `# Panics` sections at completions.rs:187 and pg_verifier_executions.rs:4348; explicit mandatory rustdoc gate, no logic rewrite or new test |
| FIND-TASK-003A-2 | SYS-1 + FUP-1 / REVISED / INCORRECT verification | Replace two hosted fixtures' assumed first-status/second-checkpoint order with existing hosted-memory waiter plus outbox drain, then one stall. Preserve real Python/workspace/resource and running/eventual-outcome assertions; keep production batching/acknowledgements unchanged |

The second finding proves reachable nondeterministic test failure/incorrect barrier selection under ordinary scheduling. It does not establish a silent false PASS or production integrity failure. Existing controls are sufficient; new checkpoint-aware stall machinery, sleep/timeout changes, serialized production frames and weaker assertions are unnecessary.

STD-002 is revised: present independent system/followup reports resolve current missing diagnosis evidence. They cannot establish the claimed historical consultation occurred. No standalone historical-evidence code task is prescribed. BEH-HIST-1 / INV-TDD-1 are one historical TDD violation, not duplicated findings. Later GREEN/mutation checks demonstrate behavior/assertion strength, not past RED. Preserve these qualifications; never fabricate historical evidence.

## Verification and limits

[Verification record](verification.md): review-run `mise run fmt:check`, tenant isolation (writable temporary uv cache), cumulative diff whitespace check and the exact shared-status outbox unit selector all PASS. The unit run selected one test and passed 1/1.

Exact-candidate `mise run test:bifrost:integration:server` was attempted but could not start tests because sandbox access to the Colima Docker socket was denied. No fresh owner PASS or test failure is claimed. Supplied implementation owner result 126/126 precedes final test-only commits a53a91094/c233b7545; touched tests were reported rerun afterward. Recorded lints, journeys and SQL/auth/PR results were inspected as supplied evidence, not rerun by this review.

Hot-path applicability was independently checked: background claim/submission/settlement/PR writers and postcommit control staging add no engine/direct/gateway/Oracle/Scribe-ingest wait. Existing background outbox delivery/custody remains. The mandatory hot-path benchmark gate is therefore inapplicable; no benchmark or zero-overhead claim is made. No new product, public API, architecture, persistent-data or concurrency decision is needed for the retained test/docs correction.

Skipped fresh Docker-based owner/journey execution and broad lint/benchmark repeats. Accepted product limits remain best-effort process-loss status staleness and producer-clock/equal-time ordering. No source was changed or committed.
