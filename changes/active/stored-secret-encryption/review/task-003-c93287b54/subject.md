# TASK-003 review subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`.
- Candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`.
- Diff: `git diff a5b1d5417...c93287b54` (10 commits in the actual range; the request's count of 9 is descriptive, not a different boundary).
- Approved contract: `changes/active/stored-secret-encryption/spec.md`, revision 1 at candidate; the current request explicitly confirms HTTP/CLI scope, excluded SDK/MCP exposure, and HTTP-only human/platform renewal.
- Original task: `changes/active/stored-secret-encryption/tasks/TASK-003-resource-rotate-and-revoke.md` at candidate.
- No prior TASK-003 review; prior TASK-001/002 reviews are predecessor evidence only.
- No `.codegraph/` directory exists; ordinary source navigation applies.

## Workspace boundary

HEAD initially equals the candidate. Existing user modifications to `AGENTS.md`, `TESTING.md`, and the `wyrd-implement`, `wyrd-plan`, and `wyrd-spec` skill files are preserved. Current contributor instructions apply; production source, spec, task, and tests are reviewed at the pinned candidate. Review writes are restricted to this new report directory. Candidate identity is rechecked before verdict.

## Navigation and assignments

- `behavior_rev`: complete requirement/scenario and assertion matrix, HTTP/CLI consumers, excluded SDK/MCP surfaces, documentation, repository rules, TDD/evidence history. Report: `task-review-behavior.md`.
- `domain_rev`: five owner read/renew/CAS paths; SQL tenant capabilities, authorization, lifecycle races, transaction scope, redaction, outage handling, refresh/propagation, hot-path applicability. Report: `task-review-domain.md`.
- Fresh `ponytail_rev` follows discovery to validate every proposed finding and reconcile coverage independently. Report: `findings-validation.md`.
- Root orchestrates and records the reconciled verdict; it does not discover or validate implementation findings.

Owners: Operator control/secrets/routes; Gateway administration/routes/managed credentials; auth HumanConnections and PgIssuerResolver; admin identity/routes; platform identity. SQL owners are auth human connections/trusted issuers, gateway credentials, and platform identity. Consumers are internal Operator client/CredentialWriter and three CLI command owners. Proof resides in changed server library PG tests, server integration journeys, CLI journeys, task evidence, and predecessor owner tests.

Security, concurrency, persistent state, and lifecycle expertise are assigned together to the domain specialist because they share the same CAS/refresh boundary. No separate invariant/system reviewer is selected for the same paths. No broad repo or maintainer specialist is selected: owner extensions and limited projections remain with the primary reviewer, with no new subsystem or abstraction layer.
