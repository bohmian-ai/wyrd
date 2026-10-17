# Immutable subject and shared navigation map

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002
Base: ef3a57b0776dd7bc451273b9176498fcca700550
Candidate: df148bd287d3e9fcae861470fe9c27bd576f2eeb
Branch: change/long-horizon-verification-task-002
Review cumulative `git diff ef3a57b0776dd7bc451273b9176498fcca700550...df148bd287d3e9fcae861470fe9c27bd576f2eeb`, not only latest fix.

Approved scope: changes/active/long-horizon-verification/spec.md revision 13, REQ-012, AC-007, INV-001..005. Pinned file contains revision 14 draft; explicit user approval is only revision-13 scope. Do not approve draft additions.
Original task: changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md
Prior review: changes/active/long-horizon-verification/review/task-002-5c421663b-independent/
Remediation and historical evidence: TASK-002-R1-decoder-docs-and-activation-guidance.md inside prior review; original task evidence appended. Treat claims as unverified.
Prior stable findings: FIND-TASK-002-1 (decoder rustdoc); FIND-TASK-002-2 (activation guidance).

## Navigation map (starting points, not complete coverage)
- Contract: wyrd-spec card/verifier/mod.rs VerificationBinding, InputSlot; validate.rs binding validation; envelope.rs decode/spec_from_kind_value; error.rs SpecTriggerActivationMismatch. Consumers include Card deserialization, loaders, references, generated TS card-types.ts and API error docs.
- Registration: wyrd-server components/cards/resolve.rs check_activation and projection; service registration transaction. Tests pg_card_registration_route, pg_verification_routes.
- Scheduled execution: wyrd-server verification/drift.rs DriftEngine::checks, runner.rs; wyrd-sql queries/verification.rs bindings and queries/verifier_executions.rs ExecutionInput/claim; migration 20261017000000_drop_verification_binding_inputs.sql. Existing exact identity/tenant transaction/lease owners must remain authoritative.
- Proof: wyrd-spec card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path; SQL pg_verification_bindings/pg_verifier_executions; three SDK scheduled_drift_alerts_operator tests; wyrd-testing verification helpers/runtime; CLI fixtures, MCP verification; guides and invalid fixtures.
- Latest behavioral remediation: dfbcc946b; evidence-only commit df148bd287. Compare prior candidate 5c421663b43d7deeca135134d6b07cf49a11b83f to current for finding closure, while auditing full range.

## Rules and limits
Read AGENTS.md, architecture/agent-rules.md, reference router and applicable authorities including design, doctrine, spec-driven development, maintainer style, TESTING.md. CodeGraph exists: use codegraph explore before locating/reading code, then supplement source as needed. No source edits, no tests edits, no commits. Write reports ONLY in this new review directory. Other reviewers are working concurrently; do not alter their reports. No full journeys, no mise run gate. Orchestrator owns runtime verification to avoid contention. Reviewers inspect source and verification evidence; clearly distinguish historical claims from independently executed proof. Expand and verify this map against source. No intended verdict.
