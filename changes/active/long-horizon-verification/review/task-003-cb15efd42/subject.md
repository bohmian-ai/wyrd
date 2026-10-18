# TASK-003 cumulative review after R2

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003
Branch: change/long-horizon-verification-task-003
Original base: ef3a57b0776dd7bc451273b9176498fcca700550
Immutable candidate / initial checked HEAD: cb15efd429ab50c1ca3e6d847429fff33296e7d3
Full range: git diff ef3a57b07...cb15efd42
Prior candidate: ea317179820b24d8b08ed499454ce2cf4675ce4e
Latest locator: git diff ea3171798...cb15efd42; do not substitute it for cumulative acceptance review.
R2 history: ac25d8c42 prior review packet, ce13c1cce test, 0cecfd8eb evidence/Report F, 2c71addce owner-requested whitespace-only diagnostic cleanup, cb15efd42 evidence correction.

## Authority and prior reviews

Original task: changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md.
Spec: changes/active/long-horizon-verification/spec.md, CURRENT TASK-003 text owner-authorized by Steven and expressly reaffirmed this round. Revision14 draft pre-release schema addition is excluded. Do not reopen resolved approval or approve that addition.
Prior first review: review/task-003-9ffdf753a/{verdict.md,findings-validation.md,evidence-reassessment.md,TASK-003-R1-close-admission-review.md,diagnostician-reports.md,supplemental-diagnoses.md} under the same active change.
Prior second review: review/task-003-ea3171798/{verdict.md,findings-validation.md,followup-review.md,TASK-003-R2-observe-replay-and-record-diagnosis.md}. New evidence there: diagnostician-report-f.md and r2-http-failure-evidence.md. Read these actual files plus appended original/R1/R2 execution evidence as claims to check, not conclusions.
Stable prior IDs FIND-TASK-003-1..6: paired shutdown; continued traffic progress; exact-key pressure/drain/replay; Rust docs; normative audit retention; independent diagnosis evidence. Last verdict retains only3/6, closes1/2/4/5 and original five diagnosis subcases A–E. Previously rejected proposals remain rejected absent new source evidence; do not repackage optional decoder expansion or navigation preferences as new scope. Next unused stable ID is7.
The owner explicitly authorized the two prior diagnostic whitespace strips (lines72/209) before this candidate. This review itself may NOT modify any prior directory.

## Shared starting navigation map

Expand independently; this is not complete coverage or a conclusion. CodeGraph is present. Use codegraph explore first for code navigation; filename-qualified queries may be needed because generic symbol queries return unrelated names. Read actual complete range yourselves; do not store patches/source archives in review directories.

- crates/shared/wyrd-runtime/src/outbox.rs: Outbox/Admission/Reservation; queued/waiting/failed/in-flight accounting, retry, source sends, per-tenant dispatch order, shutdown winner and unspent slots; bound/audit/retry/tenant/drain/concurrent tests.
- Server components/verification/service.rs execute/enqueue/stage_direct and staging_full; HTTP/MCP callers; reserves and existing Submission/RequestKey identity before source staging.
- scribe_outbox.rs ScribeWrite/ScribeSink frames/retry/ACK handoff; audit/span/capture siblings. verification/submissions.rs SubmissionSink and generic Outbox; existing SQL accept_submissions/accept replay owner.
- verification/{runner,claims,operators,mod}.rs result reservation/defer, execution_slots and shared task limits; Operator permit sibling; config/boot and app/server shutdown.
- pg_verification_routes.rs: input_rows, exact-key pressure/drain/replay, NEW distinct same-tenant sentinel plus visible_execution then durable count; existing replay/conflict pattern and per-tenant queue/commit ordering. WyrdTestServer::retained_records and flush_bifrost are reused testing reader/source flush, not new durable receipt.
- Three SDK verify-in-real-time stories and support fixtures: current offered traffic loops, neighbor proof, typed refusal/recovery/source reconciliation; Python baseline/tenant key and TS provider scope; shared client key minting remains per submit call.
- Contract error catalog and HTTP retry detail/header; generated error docs/TS codes; Python/TS native testing capacity option projections.
- Normative Wyrd design/security/reliability finite staging and counted overflow; docs runtime-limits/real-time/errors.
- Diagnosis evidence A–E and NEW F, actual recovered output and stated limitations. Follow source to root/caller and siblings; distinguish requested native-text projection from broader decoder support and unrelated old zero-count checks. Do not accept completion summaries as evidence.

## Review constraints and available checks

All seven discovery agents are fresh this round and independent; do not read other CURRENT discovery reports before completing yours. Prior packet findings and source are mandatory remediation hypotheses. Read full role in .agents/skills/wyrd-task-review/SKILL.md, AGENTS.md, agent rules, spec-driven-development, maintainer-style and applicable routed complete references. Read relevant Wyrd/Bifrost, security and operations authority, testing workflow and manifests/mise lanes.

Write ONLY your assigned report in THIS directory. Production/tests/spec/original task/PRIOR directories/index/HEAD immutable. No commit/staging/merge/deploy. Check HEAD at completion. User's conversational implementer summary is intentionally not provided as evidence. Recorded results in task packets remain claims; actual focused checks can run through mise as coordinated by root. No broad/full suites, SDK rebuilds, synthetic host load or benchmark needed for this review. Root independently ran cumulative git diff --check ef3a57b07...cb15efd42: exit0. Record actual vs claimed execution distinctly. No optional redesign, new schema, receipt/durability/fairness guarantee or unrelated pre-existing debt.
