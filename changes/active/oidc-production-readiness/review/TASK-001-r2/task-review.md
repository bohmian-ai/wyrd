# TASK-001 r2 task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Candidate tree: `4c2e2b73f4dac882f02dd08290bb6ec102bce075`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The candidate commit matched the supplied identity before this review. The
complete base-to-candidate range was inspected; prior review conclusions and
the candidate's implementation-evidence tables were treated as claims to
verify, not authority.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: OIDC remains optional; keyless startup works only when no provider ciphertext exists | Optional `HumanConnections`; `rewrap_sealed_secrets` inventories the existing three provider-secret stores and fails keyless boot on any `remaining` ciphertext | `boot::sealing_boot_pg_tests::keyless_boot_refuses_only_when_ciphertext_is_stored` is present; candidate reports the focused test and connectionless journeys green | PASS |
| REQ-002: one Active and at most one Candidate per tenant; tenants sharing an issuer remain isolated | RLS on `auth_human_connections`, tenant-scoped partial unique indexes, and the shared tenant slot advisory lock | Admin/rotation journeys cover two tenants, same issuer, and concurrent activation; candidate reports `check:tenant-isolation` green | PASS |
| REQ-003: authorized typed headless lifecycle API with redacted reads and replica-visible durable state | Six typed `/v1/identity/oidc/*` routes delegate to `HumanConnections`; every lookup is durable and tenant-scoped | Admin and rotation journeys exercise stage/read/test/activate/deactivate/remove and cross-replica visibility | PASS |
| REQ-004: callback, discovery, issuer, JWKS, and client authentication are qualified fail-closed | `probe_candidate` uses screened discovery, usable JWKS, an exact origin/path/state callback redirect, and accepts only a 4xx `invalid_grant`; real login uses `HumanConnections::require_callback` | Independently reran both probe unit tests: 2/2 passed; configured-callback Postgres test and provider journeys are present | PASS |
| REQ-005: secrets are validated, sealed, redacted, and safely rotatable | `ConnectionInput::validate` separates presence from content; views omit secret fields; `SealedSecretRewrap` uses CAS across tenant/workload/platform stores; runbook requires a post-writer-roll verification pass | Independently reran `secret_presence_follows_the_method`: passed; rotation journey covers late K1 write and K2-only serving; OpenAPI redaction assertion is present | PASS |
| REQ-017: authorization decisions are canonical, redacted, and fail closed with their mutation | Lifecycle methods append the evaluated event under the slot lock; testing records the pre-network decision, re-evaluates after IO, and appends that event with the stamp | Focused successful/denied/audit-failure stamp tests are present; admin/rotation journeys inspect canonical audit and rollback | PASS |
| Legacy Human trust migrates exactly once without damaging workload bindings or silently carrying unsafe grants | Migration preflight rejects multiple Human rows, default roles, audience mismatch, unsupported auth, and missing required secret; it copies Human trust and retains a workload-referenced row as Workload | `human_connection_upgrade_preflight` covers valid binding preservation and both prescribed refusal cases; candidate reports `test:sql` green | PASS |
| Replacement, deactivation, and removal stop old login and renewal after the lifecycle transaction commits | Login state and refresh rows carry `HumanConnectionBinding`; initial issuance and refresh successor creation take the same slot lock and require exact id/revision Active | Issuance and refresh focused tests plus `tenant_connection_session_cutoff_journey` cover replacement/deactivation/removal and the controlled in-flight callback race | PASS |
| Tenant SQL capability boundary is preserved in the remediated owner | `HumanConnections` and `PgLoginStateStore` own `WyrdPostgres`; connection workflows acquire `TenantConn`; no raw pool remains in those remediated owners or signatures | Static source inspection passes; the prescribed `check:from-pools-allowlist` does not execute successfully on this tree (`TASK-REV-R2-001`) | FAIL |
| Public contracts, served OpenAPI, stable errors, generated TypeScript codes, and documentation agree | Contract types and error catalog are registered; OpenAPI routes include all six operations; docs describe one-active lifecycle, callback, redaction, and safe key rotation | Candidate reports `test:principals:integration`, `codegen:check`, and `docs:check` green; source assertions are present | PASS |
| Required Rust documentation and import/signature style close prior findings 8 and 9 | Added/materially modified owner, query, test, and contract items carry intent/invariant/error documentation; cited qualified declarations were imported | Static diff/source inspection of the cited closures | PASS |
| Identity journey uses the prescribed default feature set and exact selection guard | All list/run commands at `mise.toml:660-678` omit `--all-features`; filtered selection must equal one and the unfiltered lane checks all three required journeys | Candidate reports three filtered and one 23/23 unfiltered runs | PASS |
| Non-goals remain excluded | No UI, hosted signup, commercial hook, compatibility route, new auth method, proxy configuration, secret inventory service, rotation coordinator, or second rewrap engine entered the range | Complete changed-file and cumulative-diff inspection | PASS |
| Completion evidence is reproducible from the immutable candidate | Focused tests and most claimed lane sources exist | One explicitly required claimed-green boundary command exits nonzero on the immutable tree (`TASK-REV-R2-001`) | FAIL |

## Prior-finding closure

| Prior finding | Closure | Result |
|---|---|---|
| `FIND-TASK-001-1` | Exact callback origin/path, single matching state, and code/standard-error qualification in `callback_redirect_qualifies` | CLOSED |
| `FIND-TASK-001-2` | `client_auth_outcome` accepts only client-error `invalid_grant` and rejects every ambiguous response | CLOSED |
| `FIND-TASK-001-3` | Login uses the deployment callback from `HumanConnections`, not request scheme/host | CLOSED |
| `FIND-TASK-001-4` | Shared `ScreenedHttp` uses reqwest's native `.no_proxy()` before address pinning | CLOSED |
| `FIND-TASK-001-5` | Exact connection binding persists through login and refresh and is rechecked under the lifecycle lock | CLOSED |
| `FIND-TASK-001-6` | Connection and login-state owners use `WyrdPostgres`/`TenantConn`; static closure passes, but its prescribed boundary command is not reproducible | CLOSED WITH VERIFICATION FAILURE |
| `FIND-TASK-001-7` | Candidate stamping uses a fresh real decision after provider IO and commits it with the stamp | CLOSED |
| `FIND-TASK-001-8` | Cited and surrounding added Rust items now carry substantive rustdoc | CLOSED |
| `FIND-TASK-001-9` | Cited declarations use module imports and bare type names | CLOSED |
| `FIND-TASK-001-10` | All identity list/run commands use default features | CLOSED |
| `FIND-TASK-001-11` | Keyless boot inventories stored provider ciphertext and fails before readiness if any exists | CLOSED |
| `FIND-TASK-001-12` | Contract and runbook require a post-roll pass; journey proves a late K1 write is recovered before K2-only serving | CLOSED |
| `FIND-TASK-001-13` | Public rejects every present secret; secret methods reject missing or empty values | CLOSED |

## Proposed material finding

### TASK-REV-R2-001 — VIOLATION: a required boundary gate is not runnable from the immutable candidate

- **Violated obligation:** The remediation task requires
  `mise run check:from-pools-allowlist`; `AGENTS.md` requires every targeted
  check for the touched surface to pass and makes a red gate blocking regardless
  of authorship. The candidate's implementation evidence specifically records
  this command as exit 0.
- **Location:** `scripts/checks/from-pools-allowlist.sh:20-39`.
- **Evidence:** On candidate `bb4895d8e`, `mise run
  check:from-pools-allowlist` exits nonzero because the script invokes `rg`
  over `crates/ python/`, but neither the base nor candidate tree contains a
  top-level `python/` directory. The observed terminal error is `rg: python/:
  No such file or directory (os error 2)`. The script is under `set -e`, so the
  claimed boundary proof is not reproducible and the subsequent chained
  `git diff --check` did not run in that invocation.
- **Observable consequence:** The required client/pool boundary gate is red on
  a clean candidate checkout, so the change cannot satisfy the repository
  completion standard and its claimed verification transcript is inaccurate.
- **Required testable correction:** Repair the existing check's stale search
  root without adding an allowlist entry or weakening its scan of the live Rust
  source, then rerun `mise run check:from-pools-allowlist` and record its actual
  successful result. Because `from_pools` is a Rust constructor and the live
  scanned implementation is under `crates/`, removing only the nonexistent
  top-level `python/` positional root is the smallest correction.

## Verification limits

- Independently executed focused contract and provider-probe tests: 3/3 passed.
- `git diff --check base..candidate` passed when run separately.
- `mise run check:from-pools-allowlist` failed as described above.
- Long Postgres/provider journeys and broad format/lint/codegen/docs lanes were
  not rerun within this review's time budget. Their committed source and the
  candidate's recorded outcomes were inspected, but that record is not treated
  as independent execution evidence.

## Overall result

**FAIL**

The implementation closes the 13 prior behavioral and structural findings,
but the immutable candidate does not pass one verification command explicitly
required by the remediation task and repository completion standard.
