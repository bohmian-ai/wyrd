# Data, tenancy, and audit domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 14
- Review scope: migrations, `TenantConn`/`OperatorPool` ownership, principal
  and credential persistence, authorization-decision transactionality and
  attribution, allowed-no-effect decisions, canonical audit staging and
  publication, and the retained `FOR UPDATE NOWAIT` publisher correction.

## Review Findings

### Critical

None.

### Important

- **`DATA-R12-01` · INCORRECT** —
  [`crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs:48`](../../../../../../crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs)
  and
  [`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:197`](../../../../../../crates/wyrd/wyrd-server/src/oracle/query_audit.rs)
  discard the verified principal's `credential_id` when they build Gate write
  and Oracle read/security decision events, even though both contexts retain the
  same `Principal` and the ordinary HTTP audit builder correctly projects its
  credential at `wyrd-server/src/audit/mod.rs:49-60`; consequently a direct
  Service-B token minted from an API key commits Bifrost decisions with
  `credential_id = NULL`, so retained audit cannot distinguish which of B's
  concurrently valid credentials performed the read or write, contrary to
  `REQ-037`, `AC-009`, and the security posture's explicit credential-rotation
  requirement. Reuse `AuditEvent::with_credential_id` in both existing event
  builders with `auth.principal.credential_id` /
  `context.principal.credential_id`; do not add a field, sink, lookup, event, or
  database read, and preserve `None` for a delegated token because delegated
  JWTs intentionally carry no credential. Extend the existing
  `service_b_acts_for_service_a_with_only_a_table_authority` journey to select
  `credential_id` for B's direct allowed native write and direct query decision,
  assert both equal B's stored API-key id, and retain the existing assertions
  that delegated decisions name A and B through the delegation chain.

### Suggestions

None.

## Authority and source coverage

| Boundary | Authority | Source/evidence inspected | Result |
|---|---|---|---|
| Tenant SQL isolation | `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `REQ-031`, `INV-007` | `TenantConn`; tenant principal, credential, grant, and admission queries; RLS migrations and isolation proofs | PASS — tenant-owned rows remain behind forced RLS and tenant operations use `TenantConn` |
| Platform SQL ownership | `architecture/agent-rules.md` two-boundary rule; revision-14 platform model | `OperatorPool`; platform principal, credential, grant, tenant lifecycle, provisioning, and recovery paths | PASS — cross-tenant directory work remains explicit operator work and no tenant query was moved onto the operator pool |
| Principal and credential durability | `REQ-001`–`REQ-011`, `REQ-020`–`REQ-028` | Current migrations; principal/credential queries; issue, list, revoke, provisioning, recovery, and token-exchange callers | PASS — verifier-only persistence, one-time plaintext return, scoped revocation, and current-state platform checks remain intact |
| Transaction ownership and allowed-no-effect decisions | `architecture/agent-rules.md`; `REQ-037`; `AC-009` | Principal, credential, tenant-status, registration, delegation, and provisioning service/route transactions | PASS — effects share the owning same-plane transaction with allowed audit; specified no-effect outcomes commit their one truthful decision without resource changes |
| Canonical audit persistence and attribution | Audit doctrine and security posture; `REQ-037`, `AC-009` | HTTP, platform, auth issuance/delegation, Gate, and Oracle event builders; `append_audit`; staging hash; retained projection | **FAIL — `DATA-R12-01`**; storage and publication preserve the optional credential field, but two reachable Bifrost producers fail to supply it for direct credential-backed callers |
| Audit publication concurrency | Audit doctrine; explicit human disposition | `AuditPublisher`; `freeze_publication_range`; `settle_publication`; watermark/bound transactions; replay tests | PASS — retain the `FOR UPDATE NOWAIT` correction: lock contention ends only the affected tenant's cycle, while the committed frozen bound preserves deterministic replay and Scribe deduplication |
| Audit and principal schema compatibility | Explicit human disposition: Wyrd has not shipped | Current migrations and final schemas | PASS — no predecessor-schema compatibility or migration choreography is required |

## Open Questions

None. The correction uses data already present in both verified request
contexts and the existing optional audit column.

## Verification Notes

- This was a static review of the immutable commit range; no source or test
  code was modified and no lane was rerun.
- The R11 evidence records a green aggregate gate and focused delegation,
  Bifrost, SQL, tenant-isolation, audit, and publication lanes on the candidate.
- The primary R11 journey retrieves B's API-key id and asserts it on the token
  exchange row, but its native-write query selects only `outcome`,
  `principal_id`, and `detail`; its direct-B query decision is not checked for
  credential attribution, so the green journey cannot detect
  `DATA-R12-01`.
- The accepted five-minute self-contained-JWT revocation window was not
  reopened.
- Mutable worktree edits to `mise.toml` and test scripts appeared during the
  review; all source conclusions above were taken from candidate commit objects,
  and those worktree edits were neither reviewed nor altered.
- Candidate identity was rechecked as
  `261168087376095fa5ad9d66946e755f3baa8fe4` before writing this report.

## Overall result

**FAIL** — one bounded, reachable audit-attribution defect remains
(`DATA-R12-01`).
