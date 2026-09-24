---
id: TASK-007-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-139, REQ-140, REQ-141, REQ-142, REQ-145, REQ-147, AC-029, AC-030, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-1, FIND-TASK-007-9, FIND-TASK-007-10, FIND-TASK-007-11, FIND-TASK-007-12]
---

# TASK-007-R2 — Close Remaining Operator Delivery Findings

## Authority and immutable subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r2/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`

## Outcome

Finish the existing Operator connection and delivery implementation by making
its public key error fully selector-free, enforcing exact representable key
versions, bounding the complete rewrap pass, redacting key-owner diagnostics by
construction, and isolating provider wire mechanics. Preserve all working
security, persistence, delivery, SDK, and retry behavior.

## Diagnoses and required corrections

### `FIND-TASK-007-1` — selector-free public remediation

R1 made `OperatorKeyUnavailable` detail and internal logging selector-safe, but
the derive-backed public remediation still names the environment, file, and
Vault lookup templates. HTTP and MCP return that remediation in the complete
problem document, so the approved prohibition on exposing key locations is not
closed.

Change only the existing catalog remediation to a selector-free instruction to
restore the configured Operator key provider and retry. Preserve the error
code, status, title, constant detail, details object, and retry classification.
Extend the existing complete-problem redaction proof so generic templates and
concrete sentinels are both absent.

### `FIND-TASK-007-9` — whole-pass rewrap deadline

Rewrap now runs independently of delivery and bounds each tenant transaction,
but `rewrap_pass` starts its deadline before awaiting cross-tenant referenced
key-version discovery and applies no deadline to that SQL future. A lock wait
or stalled statement can therefore outlive the configured pass ceiling and
prevent the interval loop from starting its next pass.

Apply the remaining duration from the existing `RewrapBudget::pass` to the
discovery operation. On expiry, cancel that future, use the existing
selector-free timeout diagnostic, and return control without mutating rows.
Preserve `OperatorPool` discovery, `TenantConn`, tenant timeouts, `SKIP LOCKED`,
secret-version fencing, shutdown cancellation, per-tenant old-key reuse, and
delivery independence. Do not introduce another scheduler, cache, or budget.

### `FIND-TASK-007-10` — exact active key version

Configuration accepts every `NonZeroU32`, while persisted key versions are
`i32`. `OperatorKeys::active_version` silently maps every value above
`i32::MAX` to `i32::MAX`. Boot, new seals, metadata, and rewrap can therefore
use a different external key than the operator configured.

Reject values above `i32::MAX` in the existing `OperatorKeysConfig::validate`
boundary for both TOML and environment input. Once validated, preserve the
value exactly in `OperatorKeys` under a named invariant. Do not widen the SQL
schema or create another version type.

### `FIND-TASK-007-11` — redacted key-owner Debug

`VaultKeysConfig`, `OperatorKeysConfig`, and `OperatorKeys` derive `Debug`.
Although token bytes are masked, ordinary debug formatting still emits Vault
address, directory, mount, prefix, and token-file selectors, including through
the public `WyrdServerConfig` owner. This violates the repository's mandatory
custom-redacted `Debug` rule for secret-bearing structs.

Replace those three derived implementations with the existing repository
redaction pattern. Retain only source kind, active version, and whether Vault
configuration/client state is present; omit all selectors and token material.
Preserve cloning, deserialization, construction, and runtime behavior. Add no
wrapper or logging abstraction.

### `FIND-TASK-007-12` — provider wire isolation

The live generic delivery module directly constructs Slack and PagerDuty JSON,
parses Slack provider responses, and owns provider-specific transient codes.
This makes provider protocol changes modify the same module that owns leasing,
retry, SSRF, and credential ordering, contrary to the repository's explicit
provider-module boundary.

Keep `OperatorWorker`, `OperatorDelivery`, screened/pinned transport, response
bounds, status/retry classification, credential attachment order, endpoints,
and limits in their existing owners. Move only Slack- and PagerDuty-specific
request construction and response interpretation into focused private provider
modules. Do not add traits, a registry, a generic adapter framework, or a new
dependency.

## Constraints and preserved behavior

- Preserve providers, routes, permissions, encryption/AAD, persistence schema,
  retry ceilings, deadlines, success boundaries, and stable public error code.
- Preserve forced RLS, transactional audit, production Vault HTTPS, owner-only
  mounted secrets, boot readiness, SSRF screen/pin ordering, redaction, shutdown
  recovery, dispatch fencing, and delivery availability during rewrap.
- Preserve exact Python and TypeScript contracts, runtime OpenAPI/MCP surfaces,
  and generated-artifact ownership.
- Keep environment/file allowances limited to their approved development and
  explicit single-tenant profiles.

## Non-goals

- No new provider, public API, error type, version abstraction, SQL schema,
  scheduler, cache, secret backend, transport, or dependency.
- No redesign of Operator delivery or rewrap scheduling.
- No provider trait, registry, generic framework, or unrelated refactor.
- No change to credentialed Slack/PagerDuty smoke as release evidence.

## Acceptance criteria

| Finding | Required closure |
|---|---|
| `FIND-TASK-007-1` | The complete public problem contains no generic or concrete env/file/Vault selector or secret, while stable code/status/detail and retry behavior remain unchanged. |
| `FIND-TASK-007-9` | A stalled cross-tenant discovery returns within the existing pass budget without mutation; a later pass succeeds after the stall clears, and delivery remains independent. |
| `FIND-TASK-007-10` | `i32::MAX` is accepted and preserved exactly; `i32::MAX + 1` is rejected from TOML and environment input before boot or provider access. |
| `FIND-TASK-007-11` | Debug formatting of all three types, including transitively through server configuration, exposes no selector/token sentinel and retains only approved operational facts. |
| `FIND-TASK-007-12` | Slack/PagerDuty wire fields, response parsing, and provider-declared transient codes live only in private provider modules; existing shared transport/policy ownership and behavior remain unchanged. |

## Focused and broader proof

Add the smallest focused assertions described above: extend the existing key
redaction test; add boundary configuration and redacted-`Debug` tests; add one
deterministic Postgres stalled-discovery test; and retain existing provider
unit/journey coverage while proving wire details are confined to their private
modules. Use exact `mise exec -- cargo nextest run` selectors for every named
Rust test and the repository-managed Postgres wrapper where required.

Then run the original TASK-007 verification set for the touched owners:
SQL/shared/Wyrd families; server, CLI, MCP, Rust/Python/TypeScript journeys;
Python and TypeScript type checks; code generation; tenant/client/PyO3/unwrap
boundary checks; format, lint, and `git diff --check`. The credentialed live
provider smoke remains gated release evidence.

Route this task directly to `$wyrd-implement`.
