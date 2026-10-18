# TASK-003 immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003
Branch: change/long-horizon-verification-task-003
Base: ef3a57b0776dd7bc451273b9176498fcca700550
Candidate: 9ffdf753ab11ce9b638c906388d3690a6aede249
Range: git diff ef3a57b07...9ffdf753a (complete cumulative range).
Original task: changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md
Spec requested: changes/active/long-horizon-verification/spec.md revision 13.
Authority discrepancy: the file in both base and candidate says revision 14 / draft; text says revision 13 was explicitly approved and revision 14 adds only pre-release table-schema rules. Clarification requested from caller. Do useful source review under the unchanged TASK-003 obligations, record the authority limit; do not silently approve draft revision 14.

## Shared starting navigation map

Expand this map independently against the source; it is neither complete coverage nor a conclusion. CodeGraph is present; use codegraph explore before grep/read for code navigation. Every reviewer reads the cumulative diff itself; do not store patches or source archives here.

- Shared runtime outbox.rs: Outbox, Admission, Reservation, writer pending/inflight/retry/drain ownership, OutboxSink callers and audit staging.
- Server components/verification/service.rs: VerificationControl execute/enqueue/stage_direct; direct input/result staging, SubmissionOutbox reservation.
- Server scribe_outbox.rs: ScribeWrite variants, ScribeSink batching, Scribe ACK submission handoff, span/capture/audit sibling writers.
- Server verification/{submissions,runner,claims,operators,mod}.rs: SubmissionSink, ClaimLoop, RuntimeLimits, result reservation, deferral, active task cap; shared Operator consumer.
- Server config.rs and boot/mod.rs: scribe.outbox_capacity startup wiring; testing/server.rs and Python/TS native-testing knobs.
- wyrd-spec/error.rs and server/http/error.rs: stable staging-full error, retry detail and HTTP header; generated SDK codes/docs.
- SDK Rust/Python/TS verify-in-real-time journeys; Python conftest/support and TS support/{server,assistant} fixture changes; MCP tests/bifrost/mcp/verification.rs; server pg_verification_runtime tests.
- Documentation/design: architecture/wyrd-design.md submission paragraph; docs runtime limits, real-time verification, generated errors.

## Evidence and review constraints

Available implementation evidence is appended to the original task (claims only; independently inspect source/tests). No raw verification logs are supplied. Do not use the conversational implementation summary as acceptance evidence. User authorizes review reports only; production source, tests, configuration and HEAD must stay immutable. Tests may be run through mise if useful; no live-provider credentials, synthetic load or broad suite required. Record checks actually run separately from recorded claims. Check HEAD remains candidate at completion. All reports belong to this directory, assigned individually. No prior TASK-003 review/remediation exists here (only plan-readiness review).

Read AGENTS.md, architecture/agent-rules.md, architecture/references/languages/{spec-driven-development,maintainer-style}.md; route all applicable complete references through architecture/references/README.md. Wyrd design/doctrine and Bifrost design govern applicable runtime boundaries. Review criterion is exact task acceptance, no optional redesign or unrelated pre-existing debt. No reviewer may read another discovery review before completing its own report.

## Superseding owner authorization — 2026-10-17

Steven explicitly authorized current spec.md TASK-003 text and excluded revision 14's draft pre-release schema addition. The prior authority blocker is resolved. See updated verdict.md and evidence-reassessment.md; historical discovery reports remain preserved. Supplied Reports A/B were copied verbatim to diagnostician-reports.md and close FIND-6 diagnoses 3/5, leaving only 1/2/4. One remediation task is issued in TASK-003-R1-close-admission-review.md.
