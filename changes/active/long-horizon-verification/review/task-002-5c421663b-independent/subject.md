# Immutable review subject

- Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-002
- Base: ef3a57b0776dd7bc451273b9176498fcca700550
- Candidate: 5c421663b43d7deeca135134d6b07cf49a11b83f
- Branch: change/long-horizon-verification-task-002
- Task: changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md
- Specification: changes/active/long-horizon-verification/spec.md; user explicitly identifies approved revision 13, REQ-012, AC-007, INV-001..005. Both pinned commits contain revision 14 draft, whose revision note concerns pre-release managed-table upgrades. Review the scoped obligations under the user's explicit authority; flag any material ambiguity rather than silently approving draft additions.
- Initial HEAD equals candidate; working tree clean before this report directory was created.

## Shared navigation map (starting points, not coverage conclusions)

Contract/authoring: wyrd-spec VerificationBinding, InputSlot::Samples, VerifierSpec::validate_schedule/validate_slot, envelope.rs::spec_from_kind_value; shared wyrd-loader validation; generated TS card-types; card/mod.rs and verifier_fixtures tests. Inspect shared parser reuse and effects on every Card kind.

Registration: wyrd-server components/cards/resolve.rs::check_activation and binding validation; service registration, refs/reference visitors, client state/auth issuance fixture consumers. Callers are loader, SDKs, raw HTTP and MCP. Follow existing invalid-card/invalid-envelope error paths.

Persistent state/runtime: wyrd-sql queries/verification.rs binding inserts, verifier_executions.rs claim SQL and ExecutionInput::Scheduled; migration 20261017000000_drop_verification_binding_inputs.sql. Follow scheduler -> claim -> runner -> DriftEngine::checks/score -> retained result/operator settlement, exact Verifier/subject/window identities, TenantConn/RLS and lease/reclaim paths.

Proof/consumers: scheduled_drift_alerts_operator Rust/Python/TS journeys, shared wyrd-testing verification harness, pg_verification_bindings, pg_verifier_executions, pg_card_registration_route, pg_verification_runtime, pg_verification_routes, pg_operator_delivery; deleted optional omission test. Fixtures under fixtures/cards and fixtures/invalid; CLI fixtures; docs and architecture wording. Diff has 54 files; inspect complete range independently.

## Rules and limits

Read the invoked .agents/skills/wyrd-task-review/SKILL.md and governing repository authority. CodeGraph exists; use codegraph explore before locating/reading code. Read complete diff with git diff base...candidate. The task's Implementation Evidence is available verification claims, not independent source proof. Do not use the user's implementer summary as evidence. No production/test edits, no commits, no full journey suites, no mise run gate. Write only your assigned report under this directory. Commands may create normal build artifacts but must not mutate tracked subject. If Postgres cannot run in this sandbox, explicitly report that; never imply a check passed without execution. Coordinate runtime checks to avoid duplicate heavy compilation; source review is primary.

## Sensitive domains selected

- Persistent data and tenancy: binding-column removal, SQL claims, exact frozen identity and transaction/RLS behavior.
- Verification/evaluation: scoring every pinned drift requirement, missing-data aggregation, baseline/window semantics and sibling submitted execution behavior.

No material permission mechanism change is apparent from the navigation map; reviewers must expand it if source demonstrates otherwise.
