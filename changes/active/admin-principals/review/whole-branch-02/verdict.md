# Admin principals whole-branch re-review verdict

## Verdict

`FIX_REQUIRED`

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Reviewed branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `5293546f33b3a5fd9de529098e23ea70d472c412` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7, approved |
| Original tasks | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review | `changes/active/admin-principals/review/whole-branch-01/` |
| Reviewed range | Complete base-to-candidate diff: 100 commits, 263 changed files |

`HEAD` remained pinned to the candidate throughout both review waves. Reviewers
changed only files in this review directory.

The branch owner explicitly approved the verified-change-contract work included
in the cumulative candidate. The proposed drift finding for that work was
therefore rejected and is not part of this verdict.

## Wave results

| Role | Result | Report |
|---|---|---|
| Task completion | `FAIL` | `task-review.md` |
| Repository standards | `FAIL` | `standards-review.md` |
| Security | `FAIL` | `domain-review-security.md` |
| Data, tenancy, persistence, and concurrency | `FAIL` | `domain-review-data.md` |
| HTTP, client, CLI, MCP, and generated contracts | `FAIL` | `domain-review-contract.md` |
| Independent Ponytail validation | Completed; 16 findings retained | `findings-validation.md` |

Wave 2 independently checked the five reports against the source and callers,
rejected unsupported claims, preserved prior IDs for recurring roots, and
consolidated the remainder into sixteen bounded findings. None requires a spec
revision.

## Acceptance matrix

| Obligation group | Result | Blocking findings |
|---|---|---|
| TASK-001 principal and credential model | `FAIL` | `FIND-admin-principals-2`, `-3`, `FIND-admin-principals-R2-3`, `-4`, `-6` |
| TASK-002 auth contexts and plane isolation | `FAIL` | `FIND-admin-principals-1`, `-2`, `FIND-admin-principals-R2-2`, `-3`, `-5` |
| TASK-003 deployment initialization | `FAIL` | `FIND-003-2`, `FIND-004-5` |
| TASK-004 tenant provisioning and lifecycle | `FAIL` | `FIND-admin-principals-1`, `-2`, `FIND-004-3`, `FIND-admin-principals-R2-5` |
| TASK-005 tenant administration and isolation | `FAIL` | `FIND-admin-principals-3`, `FIND-005-1`, `FIND-admin-principals-R2-3` |
| TASK-006 credential lifecycle and recovery | `FAIL` | `FIND-004-3`, `FIND-004-5`, `FIND-admin-principals-R2-3`, `-4`, `-6` |
| TASK-007 platform human administration | `FAIL` | `FIND-admin-principals-1`, `-4`, `-8`, `FIND-admin-principals-R2-2`, `-3` |
| TASK-008 HTTP/client/CLI/MCP projection | `FAIL` | `FIND-admin-principals-13`, `FIND-003-2`, `FIND-004-5` |
| Repository provenance | `FAIL` | `FIND-TASK-001-10` |
| Explicit non-goals and supplied handoffs | `PASS` | Verified-change work is approved; Card cross-space identity and rustdoc-lane widening remain excluded |

## Validated finding ledger

| ID | Class | Validated defect |
|---|---|---|
| `FIND-admin-principals-1` | `VIOLATION` | Same-plane platform allowances still commit before their mutations. |
| `FIND-admin-principals-2` | `VIOLATION` | Exported platform SQL APIs still expose raw SQLx transactions and broad connection owners. |
| `FIND-admin-principals-3` | `VIOLATION` | Three live error paths still publish internal source text. |
| `FIND-admin-principals-4` | `MISSING` | Active security/design authorities still contradict the implemented two-plane principal model. |
| `FIND-admin-principals-8` | `INCORRECT` | Last-admin protection counts unpinned or stale federated identities as usable recovery paths. |
| `FIND-admin-principals-13` | `INCORRECT` | OpenAPI omits live auth/admin routes and does not faithfully declare problem media and stable errors. |
| `FIND-004-3` | `INCORRECT` | A post-credential-commit provisioning interruption can leave an undisclosed usable credential before retry. |
| `FIND-005-1` | `VIOLATION` | Issuer, binding, and principal-revoke allowances commit separately from their tenant mutations. |
| `FIND-003-2` | `MISSING` | Tests bypass the actual `wyrd-server init` process contract and one-time secret output. |
| `FIND-004-5` | `MISSING` | Total platform-credential-loss recovery has no deployment-operator command or journey. |
| `FIND-TASK-001-10` | `VIOLATION` | The range still contains noncompliant Git identities and prohibited Claude trailers. |
| `FIND-admin-principals-R2-2` | `INCORRECT` | Platform session/context drops principal kind and audit hard-codes humans as `GlobalAdmin`. |
| `FIND-admin-principals-R2-3` | `MISSING` | Canonical audit cannot identify the credential that authenticated a decision. |
| `FIND-admin-principals-R2-4` | `INCORRECT` | TenantAdmin exchange returns refresh tokens that the refresh path always rejects. |
| `FIND-admin-principals-R2-5` | `INCORRECT` | Tenant admission is cached with principal epoch, delaying suspend and resume effects. |
| `FIND-admin-principals-R2-6` | `INCORRECT` | Tenant API-key failure paths do distinguishable Argon2 work and expose a timing oracle. |

The full evidence, caller paths, severity, rejected proposals, and minimal
Ponytail corrections are in `findings-validation.md`.

## Prior-finding closure

The following whole-branch-01 findings are closed:

- `FIND-admin-principals-5`, `-6`, `-7`, `-9`, `-10`, `-11`, `-12`, and `-14`;
- `FIND-003-3`, `FIND-004-2`, `FIND-005-2`, `FIND-006-3`, and the credential-route
  portion of `FIND-006-4`;
- all older closures listed in `findings-validation.md`.

The stable IDs in the validated ledger above remain open or were revised to the
smaller current defect. `FIND-admin-principals-R2-1` is rejected because the
branch owner approved the verified-change-contract inclusion.

## Supplied issue disposition

- `auth_e2e::cache_ttl_path_also_flips_verdict` remains a base-reproduced red
  with `WYRD_AUTH_503_VERIFY_UNAVAILABLE`; it is not candidate-attributable.
- `UNIQUE (data_tenant_id, name)` remains a spec-owner handoff because changing
  Card identity is outside revision 7.
- The stale `wyrd-testing/src/server.rs` comment is fixed.
- The `wyrd-sql` private intra-doc warning is fixed. Widening the permanent
  rustdoc lane remains a separate repository-cost decision.

## Verification evidence and limits

Passing review-time evidence:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `RUSTDOCFLAGS='-D warnings' mise exec -- cargo doc --locked -p wyrd-sql --no-deps`
- `mise run codegen:check` in a detached candidate worktree
- `mise run docs:check` in a detached candidate worktree
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:bifrost:journey:mcp`
- `mise run test:cli:journey`

The first `mise run test:platform:journey` run passed 27/28 and failed the
suspend/resume case with `WYRD_AUTH_401_CREDENTIAL_REVOKED`. The exact isolated
test and a second full run passed, demonstrating nondeterminism rather than a
deterministic failure. `FIND-admin-principals-R2-5` is independently established
by the production five-second cache and missing tenant-wide invalidation; its
closure proof must use a nonzero TTL.

Per approved VER-003, the broad `mise run gate` was not run and is not required
for this change. Green lanes do not override the source-proven findings above.

## Remediation

Route `TASK-001-008-R2-close-re-review-findings.md` in this directory to a fresh
`$wyrd-implement` agent. A later `$wyrd-task-review` must review the complete
cumulative candidate again.
