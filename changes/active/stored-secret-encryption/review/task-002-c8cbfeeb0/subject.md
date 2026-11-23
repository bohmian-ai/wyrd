# TASK-002 immutable review subject

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
Candidate: `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24` (`revised-encryption`).
Initial tracked working tree clean. No `.codegraph/` directory; use normal source tools.
Read actual diff with `git diff 451aa0c4d...c8cbfeeb0`; source must remain unchanged.
Spec: `changes/active/stored-secret-encryption/spec.md`, approved revision 1.
Task: `changes/active/stored-secret-encryption/tasks/TASK-002-standardize-secret-owners.md`.
Requirements: REQ-001–005, REQ-008–009, REQ-012, limited to assigned cutover (admin additions remain TASK-003/004).

## Authority and evidence

Read root AGENTS.md, changes/spec-driven-development.md, agent_docs/maintainer-style.md,
TESTING.md and relevant guides in agent_docs/index.md. Nested rules: docs/AGENTS.md,
sdks/wyrd-sdk-python/AGENTS.md, crates/wyrd/wyrd-sql/AGENTS.md where applicable.
Security: architecture/wyrd-security-posture.md stored secrets and tenant isolation;
architecture/wyrd-design.md Operator; architecture/operations/deployment-and-release.md
configuration authority; architecture/operations/reliability-and-recovery.md backup/restore.
Evidence is recorded in the original task scenarios, with predecessor TASK-001 review
under review/task-001-r1-451aa0c4d-rereview/. Inspect assertions and commit history.
User explicitly accepts live AWS/GCP and Linux production startup checks deferred to CI.
User reports scenario 2/3 production preceded tests: retain TDD violation separately.
Root AGENTS.md forbids benchmark requirements absent current explicit maintainer request;
task explicitly says no benchmark requirement. Do audit every added/moved hot-path wait.

## Navigation map (starting points, not coverage)

Composition/config: wyrd-server src/boot/mod.rs, config.rs; wyrd-cli src/init.rs,
wyrd-testing src/server.rs; KMS config.rs/lib.rs.
Auth: wyrd-auth src/sealing.rs, connections.rs, pg_resolvers.rs, platform_login.rs,
platform_authz.rs; server boot/issuer.rs, components/admin and platform/identity.rs.
Gateway: wyrd-gateway src/managed.rs, credential.rs, snapshot.rs; server
components/gateway/{service,snapshots,owner}.rs; callers reload/load_through/warm.
Operator: server components/operators/{secrets,service}.rs; verification/operators.rs;
wyrd-sql queries/operator_connections.rs and existing maintenance consumers.
Persistence: three changed initial migrations and auth/platform/operator SQL queries.
Proof: CLI tests/pg_tests.rs; server integration identity_e2e, platform_admin_e2e,
pg_operator_connection_routes, pg_operator_delivery; auth pg_resolvers PG tests;
wyrd-testing tests/gateway/compatible.rs; Python test_local_development.py and
examples/respond-to-a-failed-verification/{dispatches.py,dispatches.ts,dispatches.rs}.
Provider/CI/docs: wyrd-kms tests/integration, mise.toml, kms-integration-cloud.yml,
scripts/server/startup-*.sh, docs reference stored-secret-encryption and failed-verification guide.

## Assignments

behavior-rev: full requirement/scenario acceptance, init/journey/docs/provider/CI proof,
all repository obligations not assigned below; task-review-behavior.md.
domain-rev: security/tenancy/context and persistent-data/concurrency fences for auth,
platform and Operator consumers including remote IO outside transactions, maintenance;
task-review-domain.md.
system-rev: Gateway preparation/cold/warm/reload, key-service outage/recovery,
generation/listener/admitted snapshot lifetimes and hot-path waits; task-review-system.md.
Fresh ponytail-rev validates all reports, coverage, proposed findings and specialist choice.
Specialist boundaries include owner/abstraction review; no separate maintainer/repo reviewer
because primary owns broad rules and shared owner simplicity. No extra invariant reviewer:
concurrency and lifecycle are explicitly assigned to domain/system reviewers.

Do not run overlapping Cargo commands. Discovery is source/evidence review; ask root
before executing Cargo. Reports only are writable; no implementation changes.
