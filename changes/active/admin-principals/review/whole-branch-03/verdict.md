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

## Post-review owner decisions

After this review, the owner approved revisions 8 through 10 and superseded two
remediation directions without changing the reviewed candidate or the
`FIX_REQUIRED` verdict:

- `utoipa`, its typed annotations, the runtime `WyrdApiDoc`, `/openapi.json`,
  focused contract tests, and user documentation are required. The checked-in
  YAML snapshot, YAML endpoint/feature/dependency, emitter, OpenAPI
  codegen/drift wiring, snapshot parser, and unimplemented release OpenAPI
  digest are deleted; no second schema owner is added.
- Required Python `.pyi` and TypeScript `.d.ts` declarations remain generated
  public SDK surfaces for typing, editor ergonomics, and documentation.
- Machine grants issue no refresh token and `wyrd-client` continues to
  re-exchange durable API-key/workload credentials. Refresh rotation is reserved
  for human OIDC sessions and the eventual HTTP-only UI/BFF session boundary.

The remediation packet is authoritative for these revised correction outcomes;
`findings-validation.md` remains the historical validation record.

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
| Complete generated administrative contract | OpenAPI omits exact administrative types and reachable stable errors | FAIL — `FIND-admin-principals-13` |
| Allowed decision and same-plane effect are atomic | `/auth/issue-key` commits its allowance first | FAIL — `FIND-005-1` |
| Executable self-hosted operator workflow | CLI journey and operator page skip tenant configuration | FAIL — `FIND-004-5` |
| Coherent renewal model | Machine grants mint unused refresh tokens while User refresh rotation is unfinished | FAIL — `FIND-admin-principals-R2-3` |
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
| `FIND-admin-principals-13` | INCORRECT | Keep and repair `/openapi.json` as the one `utoipa` contract; delete the explicitly enumerated checked-in/YAML/codegen/release duplicates without deleting documentation, SDK declarations, or boundary checks. |
| `FIND-005-1` | VIOLATION | Commit issue-key allowance, issuance, and issuance evidence once. |
| `FIND-004-5` | MISSING | Prove and document tenant configuration in the real operator CLI journey. |
| `FIND-admin-principals-R2-3` | INCORRECT | Stop machine refresh issuance; complete attributed User refresh rotation for human sessions. |
| `FIND-admin-principals-R2-4` | INCORRECT | Durably commit human-session replay family revocation and its audit before returning 401. |
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
