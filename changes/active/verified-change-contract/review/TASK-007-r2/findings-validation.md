# TASK-007 R2 Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior verdict and ledger: `changes/active/verified-change-contract/review/TASK-007-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`

The candidate remained the stated commit throughout validation. The complete
cumulative base-to-candidate diff, every Wave 1 report, the prior stable
finding ledger, and the changed owners and callers named below were inspected.
No reviewed source was modified.

## Independent source and caller validation

CodeGraph is unavailable because this checkout has no `.codegraph/` index, so
the immutable candidate was traced directly.

| Boundary | Complete reachable path inspected | Result |
|---|---|---|
| Public key failure | `OperatorKeys::{key,seal,open,rewrap_tenant}` -> `From<KeyError> for WyrdError` -> `WyrdError::as_problem_json` -> HTTP and MCP error projection -> SDK/CLI callers | The safe constant `detail` is followed by catalog `remediation` that publishes all three selector forms. The existing sentinel test serializes the complete problem but rejects only concrete sentinel values, not the generic selector templates. |
| Active key version | environment/TOML deserialization -> `OperatorKeysConfig::validate` -> boot `attach_config_fields` -> `OperatorKeys::active_version` -> `verify_active`, `seal`, and `rewrap_tenant`/`rewrap_pass` | `NonZeroU32` admits values above `i32::MAX`; validation accepts them and `active_version` silently substitutes `i32::MAX`, changing the selected external key and persisted SQL version. |
| Secret-bearing debug | public `WyrdServerConfig` -> `VerificationConfig` -> `OperatorKeysConfig` -> `VaultKeysConfig`, and `AppState`/worker ownership through `OperatorKeys` | These newly added types derive `Debug`. `SecretString` masks only the token value; address, directory, mount, prefix, and token-file selectors remain printable. No current production log formats these owners, but the applicable hard rule explicitly requires custom redacted `Debug` for every secret-bearing struct, and the public configuration types make the formatting path reachable. |
| Provider wire | `OperatorWorker::attempt` -> `OperatorDelivery::send` -> inline Slack/PagerDuty body construction -> `slack`/`pager_duty` -> common screened `post_json`, status, and bounded-body helpers | Slack and PagerDuty request/response protocol details are reachable in every matching failed-dispatch attempt and remain in the generic worker module rather than provider modules. |
| Rewrap budget | `OperatorWorker::run` -> independent `rewrap` loop -> `OperatorKeys::rewrap_pass` -> `referenced_key_versions` -> per-tenant timeout/transaction | Delivery is no longer blocked by rewrap, but the pass deadline is established before an unbounded cross-tenant SQL discovery await and is enforced only after that await returns. Cancellation on worker shutdown exists; cancellation at the configured pass deadline does not. |

## Wave 1 finding decisions

| Wave 1 source ID | Decision | Disposition |
|---|---|---|
| `TREV-R2-001` | **CONFIRMED** | Retained as new `FIND-TASK-007-10`. The path is reachable from either accepted configuration input and changes exact key identity rather than merely formatting it. |
| `REPO-R2-1` | **CONFIRMED** | Retained under prior `FIND-TASK-007-1`, not assigned a new ID. R1 removed concrete selectors from detail/logs, but the same prohibited selector disclosure remains in the public problem remediation. |
| `REPO-R2-2` | **CONFIRMED** | Retained as new `FIND-TASK-007-11`. This is an explicit changed-code repository requirement, not speculative logging hardening. |
| `REPO-R2-3` | **CONFIRMED** | Retained as new `FIND-TASK-007-12`. The provider branches are live delivery paths and directly violate the provider-module boundary. |
| `DR-DEL-R2-1` | **REVISED** | Retained under prior `FIND-TASK-007-9`. R1 closed delivery starvation and bounded tenant work, but did not make cross-tenant discovery part of the finite pass bound. |

`domain-review-security.md` proposed no findings. Its empty ledger was checked
against the same source paths; no additional security-domain finding is added.
The five retained issues are independent: changing the public catalog string
does not redact Rust `Debug`, key-version validation does not bound SQL
discovery, and provider isolation does not alter either security boundary.

## Final deduplicated finding ledger

### FIND-TASK-007-1 — CONFIRMED — VIOLATION: public key-unavailable remediation discloses secret-provider selectors

- **Wave 1 source:** `REPO-R2-1`
- **Prior identity:** This is the still-open public-error portion of R1
  `FIND-TASK-007-1`, whose obligation prohibited secret-provider selectors in
  responses, errors, and diagnostics.
- **Violated obligation:** TASK-007 Scenario 4 and its acceptance criteria
  prohibit secret selectors in public errors; `architecture/wyrd-design.md`
  lines 1395-1396 say errors carry only redacted metadata and never key
  locations.
- **Exact location:** `crates/wyrd-spec/src/error.rs:1696-1704`, projected by
  `WyrdError::as_problem_json` at `crates/wyrd-spec/src/error.rs:3476-3486`.
- **Evidence:** The stable error's public `remediation` names
  `WYRD_OPERATOR_KEK_V<version>`, `<dir>/v<version>`, and the full Vault KV v2
  selector template. HTTP and MCP return the complete problem document. The
  R1 proof at `components/operators/keys.rs:841-914` checks concrete sentinel
  values but permits these generic selectors.
- **Observable consequence:** Every authorized caller receiving the routine
  503 learns the deployment's key lookup contract and selector topology despite
  the approved selector-free error boundary.
- **Decision-complete correction:** Reuse the existing derive-backed error
  catalog and change only `OperatorKeyUnavailable` remediation to a
  selector-free instruction to restore the configured Operator key provider
  and retry. Preserve its code, status, title, constant detail, details object,
  and retry classification; do not add surface-specific error handling.
- **Focused closure proof:** Extend the existing selector-redaction test over
  the complete RFC 9457 problem and assert that the environment, file, and
  Vault selector templates, concrete sentinels, and secret bytes are absent
  while code/status/detail remain unchanged.

### FIND-TASK-007-9 — REVISED — INCORRECT: the rewrap pass bound excludes cross-tenant discovery

- **Wave 1 source:** `DR-DEL-R2-1`
- **Prior identity:** R1 `FIND-TASK-007-9` remains partially open. Independent
  scheduling, tenant transaction bounds, cancellation on shutdown, and
  per-tenant old-key reuse are closed; only the whole-pass elapsed bound is
  retained.
- **Violated obligation:** REQ-147 and the R1 correction require finite
  per-pass and per-tenant elapsed-time bounds for bounded rewrap work.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:506-546` and
  `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:327-348`.
- **Evidence:** `rewrap_pass` records `deadline`, then awaits
  `referenced_key_versions(operator)` without a Tokio timeout or PostgreSQL
  statement bound. Remaining budget is first checked after discovery returns.
  The worker's outer `select!` cancels on shutdown, not when `budget.pass`
  expires. The current slow-rewrap journey stalls Vault reads after discovery
  and therefore cannot close this path.
- **Observable consequence:** A blocked or stalled discovery statement can
  keep one rewrap pass alive beyond its configured two-minute ceiling, stop
  rotation progress, and postpone the interval loop's next pass. Delivery
  remains available because R1 moved rewrap beside the claim loop.
- **Decision-complete correction:** Reuse the existing `RewrapBudget::pass`
  deadline and apply its remaining duration to cross-tenant discovery as well
  as tenant work. On expiry, cancel the discovery future, emit the existing
  selector-free timed-out diagnostic, and return control to the interval loop
  without row mutation. Preserve `OperatorPool` discovery, `TenantConn`,
  tenant timeouts, `SKIP LOCKED`, secret-version fencing, and per-tenant
  old-key reuse; add no scheduler, cache, or second budget.
- **Focused closure proof:** Deterministically hold the connection table behind
  a conflicting database lock (or equivalently stall this exact discovery
  query), run a pass with a short budget, and prove it returns within that
  budget with no mutation. Release the stall and prove a later pass completes;
  retain the existing slow-provider/other-tenant delivery journey as the
  broader availability proof.

### FIND-TASK-007-10 — CONFIRMED — INCORRECT: oversized active key versions are silently changed

- **Wave 1 source:** `TREV-R2-001`
- **Violated obligation:** REQ-147 requires one exact active positive key
  version to select the external key and be persisted with the wrapped DEK;
  key handling must fail closed rather than select a different version.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1839-1881,
  1949-1958,2844-2852` and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:268-272`.
- **Evidence:** TOML and `WYRD_OPERATOR_KEK_ACTIVE_VERSION` deserialize into
  `NonZeroU32`, so `2147483648..=4294967295` are accepted. Validation does not
  check the PostgreSQL `i32` range. `active_version` converts with
  `unwrap_or(i32::MAX)`, collapsing every oversized configured version to
  `2147483647`; boot verification, sealing, persisted metadata, and rewrap all
  consume the substituted value.
- **Observable consequence:** A deployment can query and use a different
  external key than the one configured, and distinct configured versions can
  collapse onto the same stored version. If that unintended key exists, boot
  and writes succeed under false rotation metadata.
- **Decision-complete correction:** Use the existing
  `OperatorKeysConfig::validate` boundary to reject `active_version` above
  `i32::MAX`, matching the existing PostgreSQL column without schema or public
  contract change. After validation, make `OperatorKeys::active_version`
  preserve that exact value under a named invariant instead of saturating.
- **Focused closure proof:** A focused configuration test accepts
  `i32::MAX`, rejects `i32::MAX + 1` from both TOML and the environment before
  boot/provider access, and proves the maximum accepted value is returned
  unchanged by `OperatorKeys`.

### FIND-TASK-007-11 — CONFIRMED — VIOLATION: secret-bearing key owners expose selector-rich derived Debug

- **Wave 1 source:** `REPO-R2-2`
- **Violated obligation:** `AGENTS.md` lines 199-201 require redacted custom
  `Debug` implementations for secret-bearing structs; the Operator authority
  additionally excludes key locations from diagnostics.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1797-1818,
  1830-1854` and
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:220-228`.
- **Evidence:** `VaultKeysConfig`, `OperatorKeysConfig`, and `OperatorKeys`
  derive `Debug`. `SecretString` masks its own token bytes, but derived output
  still includes the Vault address, directory, mount, prefix, token-file path,
  and transitively the complete key configuration. The configuration module is
  public and `WyrdServerConfig` also derives `Debug`, so ordinary formatting of
  the public owner reaches this output even though no current production log
  does so.
- **Observable consequence:** The required redacted-by-construction diagnostic
  boundary is absent: ordinary debug formatting of the public server
  configuration or key owner discloses exact secret-provider locations.
- **Decision-complete correction:** Replace the three derived implementations
  with the repository's existing custom-redacted `Debug` pattern. Emit only
  nonsensitive operational facts needed for diagnosis (source kind, active
  version, and whether a Vault client/config is present), and omit address,
  directory, mount, prefix, token-file, and token. Preserve `Clone`,
  deserialization, and all runtime behavior; do not add a wrapper or logging
  abstraction.
- **Focused closure proof:** Format sentinel-bearing instances of all three
  types, including through `WyrdServerConfig`, and assert that no selector or
  token sentinel appears while source kind and active version remain useful.

### FIND-TASK-007-12 — CONFIRMED — VIOLATION: Slack and PagerDuty wire details remain in the generic delivery module

- **Wave 1 source:** `REPO-R2-3`
- **Violated obligation:** `AGENTS.md` lines 354-357 require provider-specific
  request and response details to stay behind provider modules.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/operators.rs:519-669`, with common
  transport at `:735-758`.
- **Evidence:** The live `OperatorDelivery::send` branches construct Slack and
  PagerDuty provider JSON directly; `slack` parses Slack's `ok`/`error`
  response and transient code set in the same 1,200-line module; and
  `pager_duty` owns the Events v2 send boundary there. No Slack or PagerDuty
  provider module exists. Every matching failed dispatch reaches these paths
  through `OperatorWorker::attempt`.
- **Observable consequence:** A provider protocol change necessarily edits the
  generic leased-worker, retry, SSRF, and credential-ordering module, contrary
  to the required review and ownership boundary.
- **Decision-complete correction:** Keep `OperatorWorker`, `OperatorDelivery`,
  the screened/pinned client, response-body bound, status/retry classifier,
  credential attachment order, endpoints, and all limits in their current
  owners. Move only Slack- and PagerDuty-specific request construction and
  response interpretation behind focused private provider modules. Do not add
  traits, a provider registry, a generic adapter framework, or a dependency.
- **Focused closure proof:** Existing provider unit and Postgres journeys stay
  green, and source inspection shows Slack/PagerDuty wire field names,
  response parsing, and provider-declared transient codes confined to their
  provider modules while common transport and policy remain single-owned.

## Ponytail disposition

Each correction stops at an existing owner or native mechanism: one catalog
string, one existing configuration validator, custom `Debug`, the existing
pass timeout, and two private provider modules over the existing transport.
No retained finding needs a new dependency, public API, persistence schema,
provider framework, scheduler, cache, or specification decision.

## Verification limits

- Validation was static and time-bounded. It inspected the cumulative diff,
  all Wave 1 reports, the applicable authorities, and the complete owners and
  callers above, but did not rerun the broad Cargo/Postgres/Python/TypeScript
  lanes already recorded green by Wave 1.
- No existing test stalls the cross-tenant discovery query or exercises the
  active-version upper bound, generic remediation selectors, or redacted debug
  output; those missing focused proofs are part of the retained corrections.
- Credentialed Slack/PagerDuty smoke remains gated release evidence and is not
  relevant to rejecting or retaining these source-structure and bound defects.
