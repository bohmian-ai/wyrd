# TASK-003 cumulative remediation review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003
Branch: change/long-horizon-verification-task-003
Base: ef3a57b0776dd7bc451273b9176498fcca700550
Immutable candidate: ea317179820b24d8b08ed499454ce2cf4675ce4e
Cumulative range: git diff ef3a57b07...ea3171798
Prior candidate: 9ffdf753ab11ce9b638c906388d3690a6aede249
Fix locator range: git diff 9ffdf753a...ea3171798 (includes committed prior review reports); source remediation commits 8d960f6ff, b6de87ffe, ba12fe8f1, efbb596c6, ea3171798; a162396b5 records prior review.

## Authority and prior packet

- Original task: changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md.
- Spec: changes/active/long-horizon-verification/spec.md. Steven explicitly authorized its current TASK-003 text; revision 14's draft pre-release schema addition is excluded. This authority was resolved in the prior review and expressly reaffirmed for this review. Do not reopen that blocker or approve the excluded addition.
- Prior review: changes/active/long-horizon-verification/review/task-003-9ffdf753a/{verdict.md,findings-validation.md,evidence-reassessment.md}.
- Remediation: that prior directory/TASK-003-R1-close-admission-review.md, including appended evidence. Diagnosis evidence: diagnostician-reports.md and supplemental-diagnoses.md there. These are inspectable claims, not conclusions to accept.
- Stable prior IDs FIND-TASK-003-1..6: shutdown pair ownership; continuing-offered-traffic progress proof; exact-key pressure/drain/replay proof; touched Rust docs; normative audit retention prose; independent diagnosis evidence. Preserve IDs and trace closure to current source; next unused ID is 7.

## Shared starting navigation map

Expand independently; this map is neither complete coverage nor a conclusion. CodeGraph exists: use codegraph explore before locating/reading code. Read actual cumulative range yourself, not only latest fixes. No diff/source archive may be stored in review directories.

- wyrd-runtime/src/outbox.rs: Outbox, Admission, Reservation, writer queued/waiting/failed/in-flight release, retry/shutdown. Latest shutdown finalization and concurrent_shutdown_releases_reserved_losses_once; existing bound/audit/retry/tenant/drain tests.
- Server components/verification/service.rs: execute/enqueue/stage_direct, staging_full, two-row direct admission and submission reservations; HTTP and MCP consumers; changed Errors rustdoc.
- Server scribe_outbox.rs: ScribeWrite, ScribeSink, audit/span/capture siblings, batching, ACK submission handoff, constructors. verification/submissions.rs: SubmissionSink using same generic outbox and existing SQL replay.
- verification/{runner,claims,operators,mod}.rs: result reservation/deferral, execution_slots, shared ClaimLoop/Operator lifecycle and active limits. Config/boot capacity wiring.
- HTTP pg_verification_routes.rs: existing enqueue/replay helpers, new staging_refusal_recovers_the_same_enqueue_identity; retained inputs readback and execution cardinality. wyrd-testing/src/server.rs: retained_records widened visibility, existing query reader and test-only lifecycle/setup controls.
- Rust/Python/TS verify-in-real-time pressure stories: continued offers, neighbor wait, refusal/drain and source reconciliation; shared client verify/mod.rs key generation; Python conftest/support and TS support/{assistant,server} fixtures from original implementation.
- Contract error catalog, HTTP Retry-After, generated TS codes/docs, Python/TS native-testing capacity option projections from original diff.
- Architecture security-posture/reliability recovery updated bounded overflow promises; Wyrd design submission paragraph; runtime limits/real-time verification/error docs.
- Prior supplemental-diagnoses.md recovered/retrospective reports plus execution evidence; require source-backed closure, preserve historical limits. Inspect any new failure-driven adaptation for required diagnosis evidence without repeating the same gap per caller.

## Review constraints and evidence

All seven discovery roles are fresh this round and independent; do not read another current discovery report before finishing your own. Prior findings/reports are mandatory remediation hypotheses. Read full role in .agents/skills/wyrd-task-review/SKILL.md; AGENTS.md, agent rules, spec-driven-development, maintainer-style and applicable routed complete references. Server/Bifrost design, security and operations govern relevant boundaries.

User authorizes new reports ONLY in this directory. Never edit production, tests, spec, original task or prior review directory. No commits, staging or Git state changes. HEAD must remain candidate. No intended verdict. Do not rely on conversational implementer summary (not provided). Evidence appended to original task and remediation is claims to falsify. Independent focused checks may run through mise; coordinate Cargo with root, do not run broad/full suites or synthetic host load. Record actual commands/counts separately from claims. Source interleaving proof and static documentation checks may be sufficient; do not invent runtime tests for prose. No optional redesign or unrelated pre-existing debt.
