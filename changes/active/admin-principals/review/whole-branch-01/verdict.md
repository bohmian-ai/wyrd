# Admin principals whole-branch review verdict

## Verdict

`FIX_REQUIRED`

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Requested branch | `claude/admin-principals-spec-dfsmjc` (not present locally) |
| Reviewed branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `072cf8b30c7135e8cf15f92da3e371a9c999703c` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7, approved |
| Original tasks | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Reviewed range | Complete base-to-candidate diff: 68 commits, 204 changed files |

The requested `...-dfsmjc` branch does not exist in this checkout. The review
used the checked-out and matching `...-qfsmjc` branch that owns the supplied
change packet. `HEAD` remained the candidate above throughout both review
waves. Only artifacts in this review directory were written.

During verification, another session modified five uncommitted files under
`changes/active/verified-change-contract/`. They are unrelated revision-30
specification work, were preserved untouched, and did not enter the immutable
base-to-candidate review subject or this verdict.

## Wave results

| Role | Result | Report |
|---|---|---|
| `task-rev` | `FAIL` | `task-review.md` |
| `repo-rev` | `FAIL` | `standards-review.md` |
| `domain-rev` — security | `FAIL` | `domain-review-security.md` |
| `domain-rev` — data / tenancy / persistence | `FAIL` | `domain-review-data.md` |
| `domain-rev` — contracts / SDK / CLI / MCP | `FAIL` | `domain-review-contract.md` |
| `ponytail-rev` | completed; ledger non-empty | `findings-validation.md` |

Wave 2 independently traced the complete bodies and callers for the proposed
correction owners, deduplicated the five Wave 1 reports, preserved stable prior
IDs where applicable, and produced 24 validated findings: 14 new findings and
10 still-open prior findings. Post-validation execution supplied the last
candidate regression for independent confirmation. No retained correction
requires a spec revision.

## Acceptance matrix

| Obligation group | Result | Blocking findings |
|---|---|---|
| TASK-001/002 principal model, auth planes, connection ownership, and stable errors | `FAIL` | `FIND-admin-principals-1`, `-2`, `-3` |
| TASK-003 initialization behavior and proof | `FAIL` | `FIND-003-2`, `FIND-003-3` |
| TASK-004 tenant provisioning and lifecycle | `FAIL` | `FIND-004-2`, `FIND-004-3`, `FIND-004-4` |
| TASK-005 tenant administration and isolation | `FAIL` | `FIND-004-5`, `FIND-005-2`, `FIND-006-4` |
| TASK-006 credential lifecycle and recovery | `FAIL` | `FIND-admin-principals-9`, `FIND-006-3`, `FIND-006-4` |
| TASK-007 platform human administration and authentication security | `FAIL` | `FIND-admin-principals-7`, `-8`, `-10`, `-11` |
| TASK-008 shared client, HTTP, CLI, MCP, and generated contract | `FAIL` | `FIND-admin-principals-12`, `-13`, `-14`, `FIND-004-5` |
| Required architecture and operator documentation | `FAIL` | `FIND-admin-principals-4`, `-6`, `FIND-003-3`, `FIND-004-5` |
| Runnable verification and repository provenance | `FAIL` | `FIND-admin-principals-5`, `FIND-TASK-001-10` |
| Explicit non-goals and supplied handoffs | `PASS` | Card cross-space identity and rustdoc-lane widening remain excluded |

## Validated finding ledger

| ID | Class | Validated defect |
|---|---|---|
| `FIND-admin-principals-1` | `VIOLATION` | Platform authorization writes an alternate, unpublished audit table and can separate same-plane allowances from effects. |
| `FIND-admin-principals-2` | `VIOLATION` | New live service/query boundaries export raw `PgPool` and caller-handed SQLx transactions. |
| `FIND-admin-principals-3` | `VIOLATION` | Served error conversions disclose SQL, provider, parser, key-store, crypto, or serialization source strings. |
| `FIND-admin-principals-4` | `MISSING` | Required architecture and security authority amendments did not land, leaving the governing model contradictory. |
| `FIND-admin-principals-5` | `VIOLATION` | New `mise` tasks depend on nonexistent setup and use selectors that can pass after selecting no tests. |
| `FIND-admin-principals-6` | `DRIFT` | A changed fixture comment describes the wrong JSONB predicate and public rustdoc links to a private constant. |
| `FIND-admin-principals-7` | `MISSING` | A registered human platform administrator receives no platform grant and cannot administer. |
| `FIND-admin-principals-8` | `INCORRECT` | Non-transactional, overbroad suspension checks can remove the final usable administrator. |
| `FIND-admin-principals-9` | `MISSING` | Platform credential issue/list/revoke implementation has no served lifecycle surface. |
| `FIND-admin-principals-10` | `VIOLATION` | Runtime platform OIDC discovery, token, and JWKS requests bypass the pinned SSRF policy. |
| `FIND-admin-principals-11` | `INCORRECT` | Invalid platform credential paths perform distinguishable Argon2 work and expose a live-prefix timing oracle. |
| `FIND-admin-principals-12` | `MISSING` | The real-server MCP journey never performs and observes its authorized credential revocation write. |
| `FIND-admin-principals-13` | `INCORRECT` | Administrative OpenAPI errors are untyped and principal revocation ignores its declared request contract. |
| `FIND-admin-principals-14` | `REGRESSION` | Two real-server MCP journeys and adjacent rustdoc still require the pre-principal exact tool catalog, leaving the canonical MCP lane red. |
| `FIND-TASK-001-10` | `VIOLATION` | The branch range contains 58 commits with prohibited AI identity or co-author metadata. |
| `FIND-003-2` | `MISSING` | Initialization lacks required concurrency, failure/retry, secret-capture, and uninitialized-server proof. |
| `FIND-003-3` | `DRIFT` | Dead initialization variant and superseded command/prefix vocabulary remain. |
| `FIND-004-2` | `MISSING` | Tenant list, inspect, suspend, and resume operations do not exist on served/client surfaces. |
| `FIND-004-3` | `INCORRECT` | Cancellation or transition failure can leave a provisioning row that permanently burns the tenant slug. |
| `FIND-004-4` | `MISSING` | Provisioning tests synthesize failure after success and do not prove real failures, cancellation, or concurrency. |
| `FIND-004-5` | `MISSING` | CLI and operator docs cannot deliver the approved create/configure/rotate/recover workflow. |
| `FIND-005-2` | `MISSING` | No real-client journey proves cross-tenant principal/credential refusal without enumeration. |
| `FIND-006-3` | `INCORRECT` | Recovery issues durable credentials for provisioning, failed, or suspended tenants. |
| `FIND-006-4` | `MISSING` | No test proves canonical audit-append failure rolls back a tenant principal/credential mutation. |

Full diagnosis, caller evidence, consequences, and decision-complete minimal
corrections are in `findings-validation.md` and the remediation task.

## Supplied issue disposition

- `auth_e2e::cache_ttl_path_also_flips_verdict` reproduces at the immutable
  base with `WYRD_AUTH_503_VERIFY_UNAVAILABLE` through
  `DelegateError::Database(_)`. It is not a candidate regression and is not in
  the remediation ledger. It independently prevents a claim that the entire
  repository is green.
- `UNIQUE (data_tenant_id, name)` is reachable and blocks same-named Card-bound
  principals across spaces, but predates the candidate and changing it requires
  a separate Card identity/persistent-data decision explicitly outside revision
  7. The constraint and containment lookup remain unchanged here.
- The stale comment at `wyrd-testing/src/server.rs` is candidate drift and is
  retained in `FIND-admin-principals-6`.
- The candidate-attributable private intra-doc link is also retained in
  `FIND-admin-principals-6`; widening a permanent rustdoc lane is rejected as a
  separate and unnecessary CI-policy decision.

## Prior-finding closure

- `FIND-003-1`, `FIND-004-1`, `FIND-005-1`, `FIND-006-1`,
  `FIND-006-2`, `FIND-006-5`, `FIND-006-6`, `FIND-007-3`,
  `FIND-007-5`, `FIND-007-9`, `FIND-008-3`, and `FIND-008-5` remain closed
  for the behavior their prior reviews accepted.
- The configuration-time portion of `FIND-007-4` remains closed;
  `FIND-admin-principals-10` is the distinct runtime re-resolution path.
- `FIND-008-7` is reopened and folded into `FIND-004-5` because the current
  source contradicts its claimed documentation closeout.
- Prior `FIND-004-7` is subsumed by `FIND-004-3`; prior `FIND-005-3` remains
  folded into `FIND-004-5`.

## Verification limits

- Static review covered the complete diff and the callers/full bodies of every
  retained correction owner. `git diff --check` and previously recorded focused
  lanes are green but cannot exercise the missing paths above.
- The orchestrator ran `mise run gate`. Formatting, workspace-hack generation,
  all-feature/all-target Clippy, skills sync, migrations, and seven of nine
  Bifrost lanes passed, including Python 35/35 and TypeScript 16/16. The gate
  stopped at the failed Bifrost aggregate, so later repository groups did not
  run.
- The Redux lane's missing shared-target `.rlib` build failure was concurrent
  build interference: an isolated `mise run test:bifrost:integration:redux`
  passed 978/978.
- The isolated Rust SDK journey passed 16/16 on rerun. The isolated MCP journey
  deterministically failed 2/9 because its exact catalogs omit the newly added
  principal tools; that candidate regression is `FIND-admin-principals-14`.
- Independently, the base-reproduced auth failure means repository-wide green
  is not currently demonstrated and cannot be attributed to this candidate.

## Remediation

`TASK-001-008-R1-close-whole-branch-findings.md` in this directory. Route it
to a fresh `$wyrd-implement` agent. The branch owner separately performs the
required history-metadata rewrite after the implementation tree is complete.
