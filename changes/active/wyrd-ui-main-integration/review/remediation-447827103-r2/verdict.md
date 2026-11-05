# R2 remediation review verdict

**FIX_REQUIRED — FIND-TASK-004-10 only. FIND-TASK-004-1 through -9 are CLOSED.**

The implemented R2 recovery, documentation and histogram-selection corrections satisfy their prior findings. The separately included credential gate-fix introduces one conditional stable-error regression. No privilege escalation or production data-loss finding is established.

## Immutable subject and authority

Repository `/home/thorrester/Documents/GitHub/wyrd`.
Original base `9692bfd06c6bdf2585e88c791884a90669a5dc26`.
Prior candidate `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`.
Current candidate `4478271030da79e7e2eb09fe108fbf1ed10145d8`.

Reviewed complete `git diff 0c2bfe70d 447827103`: four commits, 22 changed files, +886 / −52, all pending F7/F8/F9 closure and affected contracts/callers/consumers/tests. The original cumulative range remains context. Commit447827103's credential/password/testing/MCP gate fixes were explicitly supplied for review and are accepted scope, not automatically rejected as unrelated drift.

The only initial tracked working change was the separately authorized `.agents/skills/wyrd-task-review/SKILL.md` update. It supplies the active workflow, is preserved and is excluded from the immutable committed implementation subject. Final HEAD still matched candidate; no implementation or prior reports were edited.

Authority: approved [spec.md](../../spec.md) revision27 and continuing caller-authorized deviations, root/nested contributor rules, owning architecture and topic guides. Original [TASK-004](../../tasks/TASK-004-observe-components.md), historical [TASK-022](../../tasks/TASK-022-metrics-page.md) and [Bloom task](../../tasks/TASK-004-revision/TASK-04-bifrost-bloom-pruning.md), with recorded owner transfers, remain context. Retain original [review/ledger](../range-9692bfd06-ed82f63bf-20261104/findings-validation.md), prior [verdict](../remediation-0c2bfe70d-r1/verdict.md)/[ledger](../remediation-0c2bfe70d-r1/findings-validation.md), and [R2 task/evidence](../remediation-0c2bfe70d-r1/TASK-004-R2-recovery-boundary-and-histogram-selection.md). Deviation approval introduces no renewed approval gate and does not waive explicit public-error rules.

## Independently validated specialist applicability and coverage

[scope.md](scope.md) records the pre-discovery contract applicability map: owners, producers, consumers, selected/omitted specialists and evidence. Four fresh discovery agents received that map and independently expanded it without other reviewer conclusions. A separate fresh validator checked every proposal, closure and specialist omission. The orchestrator routed inputs and wrote artifacts from the validated ledger; it did not validate its own findings.

| Required role / report | Material contract coverage | Result |
|---|---|---|
| [Behavior review](task-review-behavior.md) | Complete changed behavior, F9 per-view/per-plot choices/history, password fixture/setup allowlists/MCP expectation diagnoses, owner reuse and preserved negative assertions | Scoped PASS, no proposed findings |
| [System review](system-review.md) | F7 post-sync retained target observation, unanswered admission, old-owner termination and detached WAL IO completion before replacement, recovery data assertions | PASS, F7 CLOSED |
| [Security domain review](domain-review-security.md) | Credential issuance/extraction and previously missing shared-contract coverage: Caller/tenant/self/admin/recent-login/ceiling, list/revoke/profile siblings, fixture legitimacy and principals.me | FAIL, SEC-R2-01 |
| [Standards review](standards-review.md) | All22 changed surfaces, F8 and every changed Rust item, test-support cfg, typed/public errors, actual SQL capabilities, test-gate integrity and generated applicability | FAIL, REPO-R2-1; F8 CLOSED |
| [Fresh independent validation](findings-validation.md) | Actual source/dependency proof, all closures, minimum corrections/materiality, contract coverage and omitted roles | FIX_REQUIRED, one revised consolidated F10; coverage PASS |

The validator independently accepted omissions: unchanged metric SQL/shaping/keyed arithmetic have exact prior invariant evidence; current simple host bindings add no new complex state policy. Prior persistent-data evidence covers unchanged context identity/ordered admission/replay, and the current system review directly traces the changed observation/stop boundary. There is no new persistent format/fence/transaction/concurrency policy requiring another data-domain/invariant role. Existing owners and small extensions do not warrant a separate maintainer role. These are source-backed contract decisions, not conclusions from filenames or an empty ledger.

Credential security was expressly reopened: pre-base origin did not supply independent prior acceptance, and the changed gate-fix affects extraction/authorization/error contracts. The selected security reviewer and validator traced that missing boundary and affected unchanged dependencies. No required specialist/report is missing. No focused follow-up was needed: security/standards proposals share one traceable cause without material disagreement; the validator narrowed claims from actual authority/source. No new validator-originated finding required another source validator. [claim-comparison.md](claim-comparison.md) records routing.

## Reconciled acceptance and prior closure

| Obligation | Source/focused proof and carry-forward basis | Result |
|---|---|---|
| F1: typed histogram identities, no invalid additions/folding | Exact R1 SQL→shaper→recorded-series Chart trace and distinct-bound/null-marker/six-group fixtures; current host still passes keyed props | CLOSED / PASS |
| F2: busiest cap/totals/apart/index labels | R1 query rank/count/selected-ID proof and real21-route/apart fixture, unchanged current producers | CLOSED / PASS |
| F3: missing mean differs from actual zero | R1 known-sum/contributor SQL and null-preserving shaper, mixed/baseline/zero real query fixtures, unchanged | CLOSED / PASS |
| F4: percentile estimates/lower-bound qualifiers | R1 per-value text through recorded plot/tooltip and conservative summaries; no current numerical/tooltip changes | CLOSED / PASS |
| F5: increments before windows with preserved integer/gauge/delta semantics | R1 actual query reset/decrease/bucket/delta/unsafe-integer assertions and unchanged SQL plans | CLOSED / PASS |
| F6: complete accepted-context index with stable series ids | R1 complete row/cross-slice/process suppression and derived identity trace, seven-context/repeat/Service-release proof; current fault hook changes no key/write/ACK | CLOSED / PASS |
| F7: actual durable unanswered interruption and completion before replacement | Current append→SyncWal→retained-group→watch, target match/early-answer refusal, independent abort registry, join and closed-lane drain. Fresh exact5 cases pass and preserve nonvacuous recovered rows/index coverage | CLOSED / PASS |
| F8: mandatory changed-item documentation | CONTEXT purpose docs added; current standards/validator inventory verifies newly introduced watch/publisher/subscriber/task/barrier/test items and applicable errors/panics | CLOSED / PASS |
| F9: separate per-view/per-plot histogram choices/history | Existing picks map keyed by current Metric.input and latency/rate prefix, controlled Chart callbacks; expanded journey hides line, regroups/apart and uses back/forward. Fresh unit22 and current-harness browser1 pass | CLOSED / PASS |
| Credential authority/lifecycle/tenancy besides extractor failure | Current security/validator trace self/admin/recent-human/ceiling/expiry/no-store, tenant-bound target and owned list/revoke/profile; fresh lifecycle1 and refusal2 pass | PASS except F10 |
| Password/test-helper/MCP gate corrections | Viewer seed satisfies existing User-role owner; actual setup helpers are real-login bootstrap or nonexchangeable timestamp-tie fixtures; actual ME registration matches exact expectations. Fresh password1/controls1/MCP3 pass | PASS |
| Stable public HTTP refusal mapping | Bare Bytes rejects before handler under supported configured ceiling >2MiB, returning text/plain outside Wyrd mapper | FAIL / F10 |
| Unchanged dev/sort/raw integer/fingerprint/tagged WAL/Bloom/fork/permission boundaries | Exact original/R1 source-backed matrices and unchanged actual dependencies; no blanket credential carry-forward | Accepted by carry-forward |
| Previous latency/AC040 and human visual acceptance | Prior disclosed misses/uncertainty retained; no new performance experiment or human signoff | Unchanged disclosed limitations, no added optimization/approval gate |

Detailed exact requirement/source/test mappings and specialist applicability conclusions are preserved in [findings-validation.md](findings-validation.md). Current green tests establish their assertions; no executed case claims the new high-limit scenario was tested.

## Validated retained finding

**FIND-TASK-004-10 — REVISED, REGRESSION / VIOLATION; sources SEC-R2-01 and REPO-R2-1.**

At `crates/wyrd/wyrd-server/src/components/principals/routes.rs:403`, `issue_credential` now takes bare Bytes. Supported configuration can raise `limits.body_bytes` above2MiB. The outer Wyrd body limiter then accepts a2MiB+1 authenticated application/json request below that configured limit, reconstructing the body. Locked Axum-core0.5.6 subsequently applies its independent default2MiB Bytes limit and returns plain-text413 before the handler runs. Request-ID middleware enriches existing problem responses and does not normalize this response. Prior captured JsonRejection reached the existing canonical Wyrd mapper.

This violates explicit stable public HTTP error/code obligations and skips the handler's intended access-before-validation order. It creates no credential or cross-tenant access. No independent audit-log violation is retained: the extractor made no route permission decision to audit. Default1MiB deployments are protected by the canonical outer limiter; ordinary stream failures are also mapped there. This conditional path is proved from actual mounted routing/configuration and locked dependency source; no live high-limit probe was run.

**Smallest correction:** capture `Result<Bytes, BytesRejection>` at the handler boundary; after existing authorization and tenant-local principal resolution, convert rejection via `JsonRejection::from` and existing `body_rejection`, then retain normal empty/typed-body logic. This reuses the existing error owner; no new parser/code/config/middleware/limit increase or global normalizer. A captured-Bytes pattern already exists in gateway routes, whose provider-specific envelope must not be copied.

**Focused proof:** use existing `WyrdTestServerBuilder::with_limits_for_test` to mount the actual route with a configured ceiling >2MiB. Test authenticated2MiB+1 input below that ceiling against allowed and refused/foreign targets, asserting canonical problem+json/code and intended refusal precedence. Retain canonical over-configured-limit response and existing normal User/Service/system/tenant lifecycle cases. This exact diagnosis, correction burden/alternatives and proof are in the validated ledger and self-contained task.

## Verification and limits

Independent root executions recorded in [verification.md](verification.md):

- UI unit22 across both affected files.
- Recovery/refusal/post-sync exact5 cases.
- SYSTEM/foreign-tenant credential-refusal cases2 and User-key lifecycle1.
- Password-window fixture1 and public-controls1.
- MCP connectivity/discovery journeys3.
- Authenticated Metrics browser journey1 after rebuilding the native test harness against current server and current UI/BFF.
- Fresh repository SQL/Vala migrations for the DB-backed selections and immutable range whitespace check passed.

Supplied implementation UI check/build, fmt/clippy modes and historical broad-gate diagnoses remain distinguished from fresh independent runs. No separate historical diagnostician artifact was supplied; current behavior/security/standards source reviews independently established fixture/tool/ordering causes and preserved assertion strength. No reports falsely claim that artifact was inspected.

No high-limit live request probe, broad gate, new performance workload, second-restart expansion or manual visual signoff was run. Existing tests use existing lower/default limits and do not disprove F10. No generated input changed. The user-authorized local skill edit remains the only tracked working change; review artifacts are new untracked reports. No implementation, commits, pushes, merges or deployment were performed.

## Convergence and next handoff

This is the second unsuccessful remediation review. Previous incomplete F7 proof/F8 docs and fix-induced F9 selection are now fully repaired. The only remaining finding is introduced by separately included credential gate-fix447827103: moving from captured JsonRejection to bare Bytes changed rejection ownership, masked by the default lower global limit. It is neither an unresolved metric defect nor a new hardening expectation. Consolidate it into one small route-boundary correction and focused test; do not restart accepted discovery.

Execute [TASK-004-R3-credential-body-errors.md](TASK-004-R3-credential-body-errors.md) through `$wyrd-implement`. Next review retains all candidates/original task/prior ledgers/remediations and checks `447827103..<new-candidate>` for F10 extraction/refusal closure and directly affected consumers. Carry F1–F9 and this independently validated specialist evidence unless actual changed contracts/dependencies or new material evidence invalidate it. No specification revision or new approval decision is required.
