# Tenancy, Persistent Data, Durability, and Concurrency Review

## Reviewed Boundary

Fresh Wave-1 domain review of immutable candidate
`2c0408b683f7a548cec6dd08b35698d761d33b31` against base
`c5c20754a167e8f4d74a555a720bd51df6179a6f`.

The review is limited to tenancy, SQL transaction ownership, initialization,
tenant provisioning and recovery, authorization-audit commit boundaries,
migration durability, audit staging/publication, and the strict current
Iceberg audit schema. I read the approved `SPEC-admin-principals` revision 10,
all original `TASK-001` through `TASK-008` packets, the complete
whole-branch-05 verdict and validated findings, its data-domain report,
`TASK-001-008-R5-close-validated-findings.md`, and the R5 implementation
evidence. I received no whole-branch-06 conclusion or intended verdict.

## Authority and Source Coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Tenant SQL isolation and transaction ownership | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/v1/00-foundations/sql-foundation.md`; `architecture/wyrd-security-posture.md`; `TenantConn`; `OperatorPool`; platform and auth query owners; provisioning/recovery call paths; tenant-isolation evidence | **PASS.** Tenant state remains under caller-owned `TenantConn` transactions and Postgres RLS. Cross-tenant directory work remains behind the named `OperatorPool`; no third connection abstraction, widened tenant query, or callee-owned tenant commit was found in the reviewed behavior. |
| Deployment initialization | `REQ-020` through `REQ-024`, `INV-005`, `AC-001`; `boot/init.rs`; platform principal, credential, and grant queries; binary and failure/concurrency journeys | **PASS.** The initialization guard, root principal, verifier, and platform grant share one serialized transaction. Disclosure failure rolls the transaction back; the installed guard prevents repeated or concurrent creation from producing a second root or re-exposing a credential. Server startup remains separate from initialization. |
| Tenant provisioning and admission | `REQ-025` through `REQ-028`, `INV-006`, `AC-002`, `AC-007`; `TenantProvisioning`; platform tenant/provisioning queries; tenant lifecycle and admission migrations; failure/retry/concurrency journeys | **PASS.** The directory claim is non-admitted, tenant administration is established under the tenant transaction, and promotion occurs only after the principal, grant, builtin roles, and verifier exist. Failure remains visible and retry adopts the durable tenant identity while retiring an undisclosed earlier credential. Non-active tenants are excluded from live admission. |
| Tenant-administrator recovery | `REQ-032`, `REQ-033`, `AC-006`; `TenantRecovery`; tenant directory and tenant-admin resolution; recovery route and journeys | **PASS.** Recovery is separately authorized, requires an active target tenant, uses that tenant's RLS-bound transaction, reuses the existing tenant-admin principal and grants, and adds only a replacement verifier. It does not widen the platform principal into ordinary tenant access. |
| Authorization decision/effect coupling | `REQ-037`, `AC-009`; canonical audit rules; platform credential, platform identity/status, trusted-issuer, and workload-binding routes; `append_audit`; R5 no-effect correction and real-Postgres proof | **PASS.** Stable not-found, already-absent/revoked, wrong-owner, unchanged, and last-admin-conflict outcomes now commit the already-appended authorization decision. Store failures remain rollback-coupled to both decision and attempted effect, and malformed status syntax is rejected before opening a decision. |
| Audit staging and chain integrity | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `AuditEvent`; `append_audit`; staging rows and migrations; hash preimage tests | **PASS.** One canonical append serializes each tenant chain with `FOR UPDATE`, stages the row and advances the head in the deciding transaction, and includes nullable `credential_id` in the reproducible preimage. Platform decisions use the system-owner tenant through the existing audited operator boundary. |
| Publication, watermark concurrency, and recovery | Bifrost audit-publication authority; analytical reliability and OLAP references; freeze/read/settle SQL; `AuditPublisher`; deterministic range batch identity; Scribe dedup and publication/restart tests | **PASS.** A tenant has one durable frozen inclusive upper bound. Competing or restarted publishers reuse the same range and batch identity; rows above it wait. Settlement advances the monotonic watermark, conditionally clears only the matching bound, and deletes through the watermark in one tenant transaction. Stale completion cannot move progress backward or clear a newer bound. |
| Strict Arrow/Iceberg audit schema | Iceberg and OLAP references; `AuditLogTable`; audit projection; catalog schema comparison; credential publication and reordered-schema proofs | **PASS.** The current fourteen-field content declaration and projection agree exactly, including nullable `credential_id`; registration remains positional and fail-closed on reordered or otherwise mismatched physical schemas. The approved R5 boundary retained deletion of historical application/Iceberg schema compatibility and required only the SQL forward migration. |
| Immutable Vala migration and upgrade | SQL foundation and deployment/release migration authorities; base and candidate migration bytes; `20260910000027_audit_staging_credential_id.sql`; `pg_migration` proofs | **PASS.** `20260802000000_vala_audit_staging.sql` has the same SHA-256 in base and candidate (`ce869581019efb6689d9413efa77245f2381f464127e707458f4026178ca5d84`). The new ordered forward migration adds nullable `credential_id` without rewriting staged rows. Both focused migration proofs pass. |

The complete base-to-candidate diff was inventoried. Source tracing covered the
changed files and reachable owners needed to judge this sensitive boundary;
surfaces outside this domain were not reviewed for acceptance.

## Material Proposed Findings

None.

The prior `DATA-05-1` / `FIND-admin-principals-R5-3` migration violation is
closed: the applied migration is restored byte-for-byte, the schema change is
forward-only, and pre-existing staged rows retain a null credential value.
No reachable tenancy, persistence, durability, or concurrency defect remains
within the approved task boundary.

## Verification Evidence and Limits

- This was a review-only audit. I changed no candidate source and did not run
  broad repository suites.
- I independently ran
  `mise exec -- cargo nextest run --locked -p vala-sql --test pg_migration -E 'test(=pg_tests::shipped_audit_staging_migration_is_immutable)'`:
  one test passed.
- I independently ran the repository-managed Postgres proof
  `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test pg_migration -E 'test(=pg_tests::staged_rows_survive_the_credential_attribution_upgrade)'`:
  one test passed.
- I independently ran the repository-managed real-server proof
  `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=an_authorized_request_that_changes_nothing_still_records_the_decision)'`:
  one test passed.
- The R5 packet records green tenant-isolation, SQL, platform journey twice,
  Bifrost Redux/server/SQL integration, current audit publication, and strict
  schema proofs. Those results are accepted only for their stated selectors.
- The staged-row upgrade test rebuilds the original audit-staging shape and
  applies the exact forward SQL rather than replaying a separately checked-out
  base migration registry. The independent byte-for-byte digest equality
  closes the SQLx checksum premise; the test then directly proves the only new
  DDL over existing data. No broader upgrade claim is inferred.
- Candidate HEAD was rechecked before this report was written and remained
  `2c0408b683f7a548cec6dd08b35698d761d33b31`.

## Overall Result

**PASS**

The candidate satisfies the reviewed tenancy, persistent-data, durability, and
concurrency obligations. The prior immutable-migration defect is closed, and
the R5 no-effect audit correction preserves one durable authorization decision
without weakening effect/store rollback coupling.
