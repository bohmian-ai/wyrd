# Admin principals whole-branch review 03 — verdict

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 7,
  and `tasks/TASK-001-*.md` through `TASK-008-*.md`
- Validated ledger: `findings-validation.md` in this directory

The candidate remained pinned throughout review. The owner waived all of
`FIND-TASK-001-10`, including AI trailers and historical Claude author or
committer identities. The bundled verified-change-contract work is approved
cumulative scope and is not drift.

## Verdict

**FIX_REQUIRED**

Twelve bounded implementation findings remain. Fresh verification is green,
including the five exact selectors required by `VER-002`, but those passing
tests do not exercise the retained upgrade, rollback, contract, secret, target,
and boundary defects.

## Acceptance matrix

| Obligation | Evidence | Result |
|---|---|---|
| Principal and credential persistence/lifecycle | SQL and principal integration lanes; cumulative source review | PASS |
| Two-plane authorization and tenant admission | Platform journeys twice; source review | PASS |
| Narrow SQL ownership | `TenantProvisioning` and `TenantRecovery` retain `WyrdPostgres` | FAIL — `FIND-admin-principals-2` |
| Safe public errors | Two admin conflict mappers expose physical constraints | FAIL — `FIND-admin-principals-3` |
| Complete generated administrative contract | OpenAPI stable-code, response-schema, and revoke prose gaps | FAIL — `FIND-admin-principals-13` |
| Allowed decision and same-plane effect are atomic | `/auth/issue-key` commits its allowance first | FAIL — `FIND-005-1` |
| Executable self-hosted operator workflow | CLI journey and operator page skip tenant configuration | FAIL — `FIND-004-5` |
| Authenticating credential attribution | Refresh rotation and successor access context erase the refresh-row id | FAIL — `FIND-admin-principals-R2-3` |
| Refresh replay containment | Reuse revocation and audit roll back on the error return | FAIL — `FIND-admin-principals-R2-4` |
| Audit identifies the acted-on resource | Platform audit uses generic, missing, or discarded targets | FAIL — `FIND-admin-principals-R3-1` |
| Platform OIDC first-login interoperability | Accepted trailing-slash issuer is stored noncanonically | FAIL — `FIND-admin-principals-R3-2` |
| Secret-safe public and CLI types | Tenant issuer secret is held by debug-visible `String` fields | FAIL — `FIND-admin-principals-R3-3` |
| Mandatory Rust contracts | New fallible/panicking items omit required rustdoc sections | FAIL — `FIND-admin-principals-R3-4` |
| Compatible retained-audit upgrade | Existing fingerprint and historical null hashes are incompatible | FAIL — `FIND-admin-principals-R3-5` |
| Exact named proof under `VER-002` | Five corrected exact selectors passed under repository Postgres | PASS; `FIND-admin-principals-R3-6` closed during review |
| Provenance | Explicit owner waiver | WAIVED — `FIND-TASK-001-10` |
| Approved verified-change cumulative content | Explicit owner approval | PASS |

## Wave results

| Review | Result |
|---|---|
| Task review | FAIL |
| Repository standards review | FAIL |
| Security/RBAC review | FAIL |
| Data/tenancy review | FAIL |
| Contract/CLI/MCP review | FAIL |
| Ponytail validation | Completed; 13 retained roots before fresh exact proof |

The validator rejected the proposal to retain allowances for effects that never
occurred and rejected an unrequired full platform-identity CLI expansion. After
validation, the orchestrator ran the five corrected exact selectors; all
passed, closing the evidence-only thirteenth root without a code change.

## Validated finding ledger

| Finding | Classification | Required outcome |
|---|---|---|
| `FIND-admin-principals-2` | VIOLATION | Remove broad `WyrdPostgres` ownership from live provisioning/recovery owners. |
| `FIND-admin-principals-3` | VIOLATION | Keep physical constraint identifiers server-side. |
| `FIND-admin-principals-13` | INCORRECT | Publish typed bodies, exact reachable stable codes, and truthful generated prose. |
| `FIND-005-1` | VIOLATION | Commit issue-key allowance, issuance, and issuance evidence once. |
| `FIND-004-5` | MISSING | Prove and document tenant configuration in the real operator CLI journey. |
| `FIND-admin-principals-R2-3` | INCORRECT | Preserve the consumed refresh credential id in rotation and successor context. |
| `FIND-admin-principals-R2-4` | INCORRECT | Durably commit replay family revocation and its audit before returning 401. |
| `FIND-admin-principals-R3-1` | INCORRECT | Record the exact platform operation target, including truthful retry identity. |
| `FIND-admin-principals-R3-2` | INCORRECT | Persist and compare one canonical platform issuer value. |
| `FIND-admin-principals-R3-3` | VIOLATION | Reuse the existing secret-bearing types and redacted debug behavior. |
| `FIND-admin-principals-R3-4` | VIOLATION | Complete required `# Errors` and `# Panics` documentation. |
| `FIND-admin-principals-R3-5` | REGRESSION | Add a narrow compatible audit-log evolution and preserve legacy null hash preimages. |

## Prior-finding closure

Closed at this candidate: `FIND-admin-principals-1`,
`FIND-admin-principals-4`, `FIND-admin-principals-8`, `FIND-004-3`,
`FIND-003-2`, `FIND-admin-principals-R2-2`,
`FIND-admin-principals-R2-5`, and `FIND-admin-principals-R2-6`.

Reopened or narrowed: `FIND-admin-principals-2`,
`FIND-admin-principals-3`, `FIND-admin-principals-13`, `FIND-005-1`,
`FIND-004-5`, `FIND-admin-principals-R2-3`, and
`FIND-admin-principals-R2-4`. New validated roots are
`FIND-admin-principals-R3-1` through `R3-5`; `R3-6` closed through fresh exact
proof. `FIND-TASK-001-10` is waived in full.

## Verification and limits

Fresh passing evidence:

- `mise run fmt:check`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run lints`
- strict `cargo doc --locked -p wyrd-sql --no-deps` with warnings denied
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice, 32/32 each run
- `mise run test:bifrost:journey:mcp`, 9/9
- `mise run test:cli:journey`, 24 passed and 5 expected ignored
- `mise run test:sql`: 122 wyrd-sql, 4 fixtures, 113 vala-sql, and 2 storage
- `mise run codegen:check` and `mise run docs:check` in detached worktrees
- the five exact `VER-002` tests named in `findings-validation.md`, each 1/1

Per approved `VER-003`, the broad repository gate was not required or run.
Therefore this review does not claim every test in the repository was executed.
The base-reproduced auth cache failure, Card-name constraint, stale testing
comment, and possible rustdoc-lane widening remain excluded exactly as recorded
in the validated ledger.
