# TASK-002 Review Verdict — R10

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R9 and their verdicts and validated
  finding ledgers

The candidate commit and tree remained unchanged through both review waves.
The only writes were the R10 review artifacts.

## Verdict

**FIX_REQUIRED**

The cumulative candidate satisfies the task's observable behavior, security,
tenancy, persistence, concurrency, and verification obligations, and prior
`FIND-TASK-002-1` through `FIND-TASK-002-22` remain closed. It does not yet
satisfy the repository's hard Rust documentation rule: Wave 2 independently
confirmed two bounded source-documentation violations as
`FIND-TASK-002-23` and `FIND-TASK-002-24`. Both corrections are local comments
and require no specification or architecture decision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006, INV-001: untrusted route key and state-only callback routing | `HumanConnections::begin_login`, `WyrdPostgres::{resolve_tenant_slug,login_state_tenant}`, one-use state, refusal and same-issuer isolation journeys | PASS |
| REQ-007, INV-004: PKCE, nonce, exact redirect, screened provider IO, complete fail-closed OIDC verification, and replay refusal | Callback/verifier owners plus refusal journey and focused claims/algorithm/`azp` proofs | PASS |
| REQ-008, INV-002, INV-003: exact `(issuer, sub)` User identity, no email linking or privileged default, and current tenant-role mapping | Callback identity and role owners; login/provider-switch, same-email, unmapped-user, role-sync, and concurrent-callback proofs | PASS |
| REQ-013, AC-005: machine credentials remain independent | Existing API-key/workload grant owners and machine-independence journey | PASS |
| REQ-014 through REQ-016, AC-003/006/007: tenant isolation, tested replacement, provenance-bound renewal cutoff, five-minute access snapshots, and current mappings on issuance | Active-connection revision binding, forced RLS, family/connection locks, provider-switch/cutoff and deterministic overlap proofs | PASS |
| REQ-017: canonical redacted audit and fail-closed session establishment | Role-sync, issuance, replay, and revocation audit owners share required transactions; injected audit-failure proofs | PASS |
| Packet-local login/callback contract and retired public authorization-code exchange | Typed begin/callback contracts, sealed completion, fixed callback response, removed grant/client/CLI compatibility paths, served OpenAPI proof | PASS |
| Required real-server journeys and focused negative/concurrency evidence | Four named TASK-002 journeys, recorded full identity lane, fresh OIDC claims and concurrent callback proofs, SQL/OpenAPI evidence | PASS |
| Explicit non-goals and downstream TASK-003/TASK-004 boundaries | No provider-token bearer, email linking, platform fallback, instant-revocation promise, new machine model, BFF completion, or CLI handoff implementation | PASS |
| Prior `FIND-TASK-002-1` through `FIND-TASK-002-22` | Wave 2 revalidated every corrected owner against the cumulative candidate | PASS |
| Repository Rustdoc: materially modified token-contract test | `new_grant_variants_reject_unknown_fields` was narrowed but has no required item rustdoc (`FIND-TASK-002-23`) | **FAIL** |
| Repository Rustdoc: materially modified auth-routes module | Module documentation omits mounted tenant human login initiation (`FIND-TASK-002-24`) | **FAIL** |

## Wave results

| Review | Result | Finding disposition |
|---|---|---|
| Task implementation | PASS | Empty proposed ledger |
| Repository standards | FAIL | Proposed `REPO-TASK-002-1` and `REPO-TASK-002-2` |
| Security/OIDC/RBAC domain | PASS | Empty proposed ledger |
| Tenancy/data/durability/concurrency domain | PASS | Empty proposed ledger |
| Structured Ponytail validation | FIX_REQUIRED | Both proposals confirmed as `FIND-TASK-002-23` and `FIND-TASK-002-24` |

## Validated finding ledger

| Finding | Status | Classification | Required correction and closure proof |
|---|---|---|---|
| `FIND-TASK-002-23` | CONFIRMED | VIOLATION | Add concise rustdoc to `crates/wyrd-spec/src/auth/token.rs::new_grant_variants_reject_unknown_fields` explaining the surviving `jwt-bearer` and `refresh_token` unknown-field contract after authorization-code retirement. Keep its body unchanged; inspect the source and run `git diff --check`, `mise run fmt`, and `mise run lints`. |
| `FIND-TASK-002-24` | CONFIRMED | VIOLATION | Update only `crates/wyrd/wyrd-server/src/components/auth/routes.rs` module rustdoc to include tenant human login initiation alongside its other three auth surfaces. Preserve routing and run the same static/broader checks. |

## Prior-finding closure

Wave 2 independently re-read each previously corrected owner and found
`FIND-TASK-002-1` through `FIND-TASK-002-22` **CLOSED**. The new findings are
documentation-only and do not reopen or overlap those behavior, security,
tenancy, concurrency, schema, tracing, or ownership corrections.

## Verification limits

- Fresh Wave 1 evidence: the exact shared OIDC claim-validation test and the
  repository-managed Postgres concurrent callback role-serialization test
  passed; cumulative diff hygiene passed.
- Fresh standards evidence: code generation, tenant isolation, raw-pool,
  unwrap, and Clippy-allow checks passed.
- The task and R1-R9 records retain broader green identity, principals, SQL,
  docs, format, lint, and focused overlap evidence. Those expensive lanes were
  not all repeated in R10.
- Compilation and behavioral tests cannot establish complete private
  test/module rustdoc, so their green results do not close the two findings.
- Live provider qualification, TASK-003 BFF completion, and TASK-004 CLI
  handoff remain outside this task review.

## Remediation

Implement
`changes/active/oidc-production-readiness/review/TASK-002-r10/TASK-002-R10-rustdoc-contract-corrections.md`
through `$wyrd-implement`, then review the complete original base-to-new-
candidate range again.
