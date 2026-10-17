# Data integrity, tenancy and concurrency review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
- Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c` (HEAD verified unchanged).
- Specification: `changes/active/local-dev-setup/spec.md`, revision 1.
- Task: `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
- Overall: **FAIL**, for the bounded required failure-proof gap DATA-001. No implementation data-integrity defect was established.

## Reviewed boundary and authority

The reviewed boundary is the composite gateway operation from CLI/MCP/HTTP submission through `GatewayAdministration`, tenant SQL, credential sealing and referential locking, together with sibling individual put/revoke/delete and runtime snapshot readers. This report does not assess local filesystem configuration or other reviewers' findings.

| Boundary | Authority | Source coverage and assessment |
|---|---|---|
| One server-owned atomic operation | Spec REQ-006, INV-001; task Scenario 2; AGENTS §§3/9; agent-rules transaction ownership; architecture references `patterns.md` and `architecture-constraints.md` | Cumulative service diff and full relevant service bodies: `apply_deployments`, `put_credential`, `put_deployment`, `write_credential`, `write_deployment`, `audited`. One acquired `TenantConn` is passed through all writes, one final commit; helper errors leave rollback to the transaction owner. PASS in source. |
| RLS tenancy and tenant-qualified identity | Spec INV-003; security-posture principles and Tenant and data isolation; design Two administration planes | `ServerPostgres::tenant_conn`, SQL `TenantConn::{acquire,commit}`, gateway query slots, gateway administration migration and FORCE RLS policies. Caller supplies verified data tenant; SQL inserts use the bound tenant; credential FK includes tenant, name and provider. No new raw pool or hand-filtered path. PASS in source. |
| Source assignment and sealed values | Spec REQ-005/006, INV-002/003/005; security-posture credential protection | `validate_source`, `seal`, `credential_snapshot`, `credential_view`, `ProviderCredentialWriteSource`/source views, keyring migration, actual stored-row assertions. Managed values use the existing tenant/name/provider-bound sealer. Environment/external sources retain their operator assignment checks. Responses and ordinary Debug do not expose submitted values. PASS in source. |
| Concurrent reference validity | Spec REQ-006 and AC-004; task Scenario 2; existing SQL ownership | SQL `gateway_credential_for_share`, credential upsert/revoke/delete, deployment upsert/delete, composite FK RESTRICT constraints; service callers. Deployment validation holds FOR SHARE until commit; revokes/rotations/deletes conflict with that lock. Revocation queued ahead of an apply is read after its commit and rejected. Provider changes cannot orphan another deployment because of the composite FK. PASS in source. |
| Consistent readers and independent families | Spec REQ-006, INV-004/005 | Runtime `snapshot` uses one `SNAPSHOT_SQL` statement over credentials/deployments/policies; new collection operation never mutates policy/pricing/accounting families or names omitted from its body. No intermediate uncommitted pair is exposed. PASS in source. |
| Error behavior and proof | Spec AC-004; task Scenario 2 and P4; AGENTS §11; testing-workflows | P4, existing credential/reference/concurrency tests, composite CLI journey and real MCP journey inspected. Application-level later failure, missing sealer, denied caller, foreign reference, success/redaction and queued revoke are covered. Required SQL-failure rollback after earlier writes is absent. FAIL (DATA-001). |

Additional authorities consulted: AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development, maintainer-style, testing-workflows, relevant design/doctrine and security sections. No CodeGraph directory exists; ordinary source navigation was used.

## Existing owner and sibling mutation tracing

The existing alternative was to invoke `put_credential` and `put_deployment`, but each intentionally acquires and commits its own transaction. That cannot satisfy the required atomic operation. The candidate instead extracts their existing transaction-composable work into two private methods on the existing dependency-owning administration service. It does not create a second durable owner or store. `put_credential` and collection apply are the production callers of `write_credential`; `put_deployment` and collection apply are the production callers of `write_deployment`.

Repository-wide symbol search confirms gateway credential upsert and deployment upsert production calls remain confined to these shared helpers. Revoke and delete remain sibling methods using the same tenant SQL owner. New collection apply uses the same source validator/sealer and reference lock as the individual operations. The migration's RESTRICT FK remains the authority for provider replacement and deletion; no application-side compensating write or new concurrency mechanism appears. Policy mutations use their pre-existing separate policy-row lock, unaffected by collection apply.

A commit may precede a lost response; retry uses named upserts. The inspected code does not promise that lost acknowledgement means rollback. Deadlock/storage refusal may abort a contended transaction but introduces no committed partial result or new retry store. No additional machinery is warranted by this review.

## Proposed finding

### DATA-001 — Required SQL-failure rollback proof is missing

- **Classification:** MISSING (required durable-failure verification).
- **Violated obligation:** Spec AC-004 expressly requires all-or-none behavior on SQL failure and focused Postgres failure/concurrency proof; original task Scenario 2 explicitly includes SQL failure and rollback after an earlier would-be write.
- **Exact changed location:** `crates/wyrd/wyrd-server/src/components/gateway/pg_administration_tests.rs:1552–1569` within `gateway_apply_is_atomic_and_tenant_scoped` (test starts at line 1495). Production boundary being proved: `service.rs:378–386`.
- **Evidence:** P4's only later failure calls `write_deployment` with an already revoked credential; service code rejects it before `upsert_gateway_deployment`. Missing sealing key and permission denial also fail before SQL mutations. The foreign reference and queued-revoke branches similarly return validation errors. CLI `deployment_document_applies_local_environment` and MCP `agent_administers_redacted_gateway_configuration` use missing references, again application-level errors. Neither these tests nor the existing individual replacement/reference tests cause SQL failure inside a composite apply after an earlier mutation. The P4 baseline asserts resource names only, with exclusively new attempted names, so it also cannot detect loss of a pre-existing credential value under a failed rotation.
- **Reachable consequence and limit:** This is a proof gap, not evidence that the current TenantConn transaction is incorrectly implemented. The new public operation rotates existing credentials and can hit a real FK rejection during a later credential provider update. Required proof currently never exercises that database-aborted transaction after an earlier rotation; a regression that commits the earlier rotation in this path would pass the added tests. Existing individual-operation proof cannot establish composition of multiple writes into this operation.
- **Smallest correction:** Extend the existing P4 Postgres test using existing service/query fixtures and actual FK authority. Seed an existing managed credential whose submitted rotation would change its value, then submit that rotation before a second credential update that changes the provider of an already referenced credential. Have submitted deployments reference each submitted credential so shape validation passes. The second credential update must reach the existing ON UPDATE RESTRICT FK and return the existing conflict. Assert after refusal that the earlier credential's redacted metadata and stored envelope remain unchanged and its original value remains usable through the existing resolution owner; assert the referenced credential/deployment and unrelated resources remain unchanged. This requires no new injection framework, durable store, production guard or implementation redesign.
- **Focused closure proof:** Run the existing exact P4 command after adding this scenario: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped)'`. Keep the current revoke/tenant/redaction checks. No broad journey aggregate is required to close this test-only correction.

## Verification and limits

Source-reviewed the complete changed contract/service/test diffs and followed existing SQL, migration, transaction, source, sealing, snapshot and sibling callers. P4 has a real lock barrier rather than a timing sleep. Existing individual replacements have held-row last-writer proof, but it is not a new composite SQL-failure test. Available implementation evidence claims P4 and the focused public journeys passed; no captured command logs were supplied.

No Cargo, build, lint, test or codegen commands were launched, per shared-target coordination instructions. No external example working tree was used as immutable evidence. Only this report was written; implementation remained unchanged.
