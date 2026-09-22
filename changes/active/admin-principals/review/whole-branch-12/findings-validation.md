# Findings validation — cumulative admin-principals through R11

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14,
  SHA-256 `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`
- Original tasks: `TASK-001` through `TASK-008`, with cumulative remediation
  through `TASK-001-008-R11`

The candidate remained unchanged during validation. Mutable worktree edits to
`mise.toml` and test scripts are not in the candidate and were not used as
evidence. The user-authorized gate-enabling and ancillary changes were reviewed
for reachable regressions, not classified as drift. The approved five-minute
self-contained-JWT revocation window was excluded.

## Validation method

The complete base-to-candidate change inventory, approved specification,
cumulative tasks, applicable repository authorities, every Wave 1 report, and
the immutable candidate source were inspected. CodeGraph was used before source
inspection to trace the relevant call paths. Each proposed finding was checked
for task obligation, production reachability, all callers of the correction
boundary, existing owners that can close it, and the smallest direct proof.

## Wave 1 proposal disposition

| Wave 1 proposal | Disposition | Validation |
|---|---|---|
| `TASK-REV-1` — generated token-response documentation says API-key exchange returns a refresh token | **CONFIRMED** | `TokenResponse.refresh_token` is the schema owner, and its false API-key statement appears verbatim in the checked-in JSON schema and runtime OpenAPI projection. Runtime issuance correctly returns `None`, but `REQ-048`, `AC-013`, and `AC-018` also require the public contract to describe that behavior. Retained as `FIND-admin-principals-R12-1`. |
| `DATA-R12-01` — Bifrost Gate and Oracle audit omit the authenticating credential | **CONFIRMED** | Both production builders receive a verified `Principal` whose `credential_id` is populated from the JWT `cid`; Gate's sole authorization call and Oracle's read/security calls reach those builders, which leave `AuditEvent.credential_id` at `None`. The specification explicitly requires every authorization decision to name its authenticating credential. Retained as `FIND-admin-principals-R12-2`. |
| `TEST-1` — the production-wheel boundary check installs the testing build it must reject | **REVISED** | The failure is deterministic: `check:py-wheel-no-testing` depends on `py:setup`, `py:setup` builds with `--features testing`, and that feature registers the exact `wyrd._wyrd.testing` module imported by `wyrd.testing`. Preserve the single testing-enabled developer setup; make this existing check build the default extension before its existing import assertion. Retained as `FIND-admin-principals-R12-3`. |
| `TEST-2` — R11 evidence is not exact and replayable | **REVISED** | The record contains a placeholder selector, abbreviated commands, no positive focused Python environment-only selection, and a three-crate `check:docs` result presented as proof for a ten-crate R9/R10 inventory. The correction is evidence-only and must not invent a checker or rerun commands for which credible exact results already exist. Retained as `FIND-admin-principals-R12-4`. |
| Repository standards, auth/security, SDK/contract, and Bifrost reports | **VALIDATED EMPTY** | These reports proposed no findings. Independent inspection found no contradiction requiring another finding; the accepted revocation window and authorized ancillary scope remain excluded. |

## Final deduplicated ledger

### `FIND-admin-principals-R12-1` — CONFIRMED — INCORRECT — the token contract falsely promises machine refresh tokens

- **Wave 1 source:** `TASK-REV-1`.
- **Violated obligation:** specification `REQ-048`, `AC-013`, and `AC-018`
  require API-key and workload grants to return no refresh token and require
  generated contracts to describe the shipped model accurately.
- **Exact location:**
  `crates/wyrd-spec/src/auth/token.rs:108-113`, generated
  `crates/wyrd-spec/schemas/auth_token_response.json`, and the matching schema
  golden/runtime OpenAPI projection.
- **Evidence and reachability:** `TokenResponse` derives both `JsonSchema` and
  `ToSchema`, so its field documentation is copied into independently consumed
  JSON Schema and served OpenAPI. The candidate description explicitly lists
  `wyrd_api_key` as issuing a refresh token, while
  `TenantTokenIssuer::issue` returns no refresh token for `TenantGrant::ApiKey`
  and the existing Postgres test proves no refresh row is created.
- **Observable consequence:** an independent client generated or implemented
  from Wyrd's public contract is told to expect a machine refresh secret that
  the server intentionally never returns.
- **Decision-complete correction:** correct only the owning field documentation
  to reserve refresh tokens for initial human OIDC login and human refresh
  rotation and to state that API-key, workload, and delegated grants re-present
  their durable evidence. Regenerate the existing owned JSON schema and golden;
  do not add a new response type, field, compatibility path, or generator.
- **Focused closure proof:** rerun the existing exact
  `exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row`
  selector, `mise run codegen:check`, and the existing served-contract owner (or
  an assertion in it) proving the `refresh_token` description no longer names
  API-key exchange.

### `FIND-admin-principals-R12-2` — CONFIRMED — INCORRECT — Bifrost decisions discard the authenticating credential

- **Wave 1 source:** `DATA-R12-01`.
- **Violated obligation:** specification `REQ-037` and `AC-009`, plus the
  canonical audit rule, require every authorization decision to name the
  principal and credential that authenticated it.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs:48-58` and the common
  Oracle event builder at
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:190-209`.
- **Evidence and reachability:** `TokenVerifier` parses the access-token `cid`
  into `Principal.credential_id`; Gate passes that principal through its sole
  reachable `authorize_record_write -> GateAudit::append_write_decision` call,
  and Oracle passes the same principal through its read and security decision
  calls. Both production event builders call `AuditEvent::new`, whose default
  credential is `None`, without applying the existing
  `AuditEvent::with_credential_id` method. The ordinary HTTP audit builder
  already projects the same principal field correctly.
- **Observable consequence:** direct Service-B reads and writes made with one
  of several live API keys are retained with `credential_id = NULL`, so audit
  cannot identify which credential performed the operation or support
  credential-rotation investigation.
- **Decision-complete correction:** in each existing production builder, reuse
  `AuditEvent::with_credential_id` with the credential already present on the
  verified principal. Keep the common Oracle builder as the single correction
  point for both read decisions and security violations. Add no lookup, field,
  database read, event, or sink. Delegated tokens continue to record `None`
  because their principal intentionally carries no credential; actor identity
  remains in the existing delegation detail.
- **Focused closure proof:** extend the existing
  `query::service_b_acts_for_service_a_with_only_a_table_authority` journey to
  select `credential_id` for B's already-existing direct native write and
  direct query decision, assert both equal B's stored API-key id, and retain the
  existing delegated A/B attribution and no-effect assertions. No new harness
  or parallel journey is warranted.

### `FIND-admin-principals-R12-3` — REVISED — REGRESSION — the no-testing production boundary check tests a testing build

- **Wave 1 source:** `TEST-1`.
- **Violated obligation:** `AGENTS.md`'s Python boundary requires the production
  wheel to exclude test-only harness behavior, and the named repository check
  must exercise the property it claims.
- **Exact location:** candidate `mise.toml:875-879` and `978-989`, with feature
  registration at `sdks/wyrd-sdk-python/src/lib.rs:110-116` and the public
  import at `sdks/wyrd-sdk-python/python/wyrd/testing/__init__.py:3`.
- **Evidence and reachability:** running `mise run check:py-wheel-no-testing`
  first executes `py:setup`, whose `maturin develop --features testing` enables
  the optional `wyrd-testing` dependency and registers
  `wyrd._wyrd.testing`; the check then fails precisely when that import
  succeeds. The check is a directly runnable repository contract even though
  the current aggregate gate does not include it.
- **Observable consequence:** the named production-boundary check is
  deterministically red and provides no evidence that a default Wyrd Python
  build excludes the harness.
- **Decision-complete correction:** preserve the one testing-enabled
  `py:setup` used by developer and integration lanes. Change only the existing
  `check:py-wheel-no-testing` task so it builds/installs the default-feature
  extension before executing its existing negative import assertion; do not
  add another checker, harness, dependency, or production export.
- **Focused closure proof:** `mise run check:py-wheel-no-testing` passes against
  the default build, then `mise run py:test:testing` passes against the existing
  testing-enabled setup.

### `FIND-admin-principals-R12-4` — REVISED — VIOLATION — R11's required focused evidence is incomplete

- **Wave 1 source:** `TEST-2`.
- **Violated obligation:** R11 `Focused closure proof` lines 268-298 and
  `Broader verification` lines 300-336 require literal repository-native
  commands, positive selected counts for every named test, and strict rustdoc
  for every affected crate.
- **Exact location:**
  `changes/active/admin-principals/review/whole-branch-11/TASK-001-008-R11-close-r9-r10-findings.md:338-365`.
- **Evidence and reachability:** R11-2 records
  `test(=exchange_api_key::pg_tests::<name>)` rather than an executable command;
  R11-6 and R9-2 contain command fragments rather than literal repository-native
  invocations; the Python R11-4 focused selection omits the named
  environment-only construction test; and R11-7 cites `mise run check:docs`,
  whose candidate definition covers only `wyrd-spec`, `wyrd-auth-issue`, and
  `wyrd-auth-verify`, while claiming a ten-crate R9/R10 inventory. Broad lane
  success does not establish these specifically required selections or the
  private/test-item documentation inventory.
- **Observable consequence:** a reviewer cannot replay the claimed focused
  closures or establish that R11-7's complete affected-crate documentation
  boundary was checked, despite the implementation's otherwise strong broad
  coverage.
- **Decision-complete correction:** amend only the R11 evidence record: identify
  the code candidate and evidence-bearing candidate unambiguously without a
  self-referential hash requirement; replace every placeholder or fragment with
  the literal command actually run and its positive count; add the exact named
  Python environment-resolution selection; and record the exact strict-rustdoc
  command/package inventory that covers every R9/R10-affected crate. Reuse
  credible existing results and rerun only commands for which no exact result
  exists. Add no permanent evidence checker or new test.
- **Focused closure proof:** inspect every amended selector against the current
  source and positive count, run any previously unproved selector, run strict
  rustdoc over the recorded affected-crate inventory, and finish with
  `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..<new-candidate>`.

## Result

Four bounded findings remain. All corrections reuse existing owners and tests;
none requires a specification revision, new public API, architecture, store,
permission, policy engine, transport, audit path, dependency, or test harness.
