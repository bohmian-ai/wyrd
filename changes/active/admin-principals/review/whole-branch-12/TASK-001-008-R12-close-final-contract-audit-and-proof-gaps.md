# TASK-001-008-R12 — Close the final contract, audit, and proof gaps

## Route and authority

Implement this remediation with `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete cumulative candidate.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14, status `approved`, SHA-256
  `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`.
- Parent remediation:
  `changes/active/admin-principals/review/whole-branch-11/TASK-001-008-R11-close-r9-r10-findings.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed cumulative candidate:
  `261168087376095fa5ad9d66946e755f3baa8fe4`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-12/findings-validation.md`.
- Remediates: `FIND-admin-principals-R12-1` through
  `FIND-admin-principals-R12-4`.

## Outcome

Finish the existing implementation without new architecture: public token
contracts describe the runtime refresh-token model, direct Bifrost audit rows
retain the already verified credential ID, the existing production-wheel check
actually tests a default build, and the R11 evidence is literal and replayable.

## Findings and decision-complete corrections

### `FIND-admin-principals-R12-1` — the token contract falsely promises machine refresh tokens

`TokenResponse.refresh_token` is the owning Rust schema field, but its
documentation says API-key exchange returns a refresh token. That statement is
copied into JSON Schema and served OpenAPI even though API-key, workload, and
delegated grants correctly return no refresh token and the existing Postgres
test proves no refresh row is written. An independent client therefore receives
a false public contract.

Correct only the field documentation in
`crates/wyrd-spec/src/auth/token.rs`: refresh tokens are returned for initial
human OIDC login and human refresh rotation; machine and delegated grants must
re-present their durable evidence. Regenerate the existing owned schema and
golden. Do not add a response type, field, compatibility behavior, or generator.

### `FIND-admin-principals-R12-2` — Bifrost decisions discard the authenticating credential

The verified principal already carries the access token's `cid`, but the
production Gate and common Oracle audit builders construct `AuditEvent` without
copying it. Direct Service-B reads and writes are consequently retained with a
NULL credential ID, preventing audit from identifying which live API key made
the request.

Reuse `AuditEvent::with_credential_id` in the existing Gate builder and the
single common Oracle builder, passing the credential already present on the
verified principal. This one Oracle correction must cover both normal read and
security decisions. Add no lookup, field, database read, event, or sink.
Delegated tokens intentionally remain credential-less; their actor identity
continues through the existing delegation detail.

Extend the existing
`query::service_b_acts_for_service_a_with_only_a_table_authority` journey rather
than creating another test: assert that B's existing direct native-write and
direct-query audit rows carry B's stored API-key ID while the existing delegated
A/B attribution and no-effect assertions remain green.

### `FIND-admin-principals-R12-3` — the production-wheel boundary check tests a testing build

`check:py-wheel-no-testing` depends on `py:setup`, which intentionally installs
the extension with `--features testing`, then fails when the test-only module is
importable. The check is therefore deterministically testing the opposite build
from the property it claims.

Preserve the single testing-enabled `py:setup` used by development and
integration lanes. Change only the existing `check:py-wheel-no-testing` task so
it builds or installs the default-feature extension before running its existing
negative import assertion. Add no checker, harness, dependency, production
export, or second general setup workflow.

### `FIND-admin-principals-R12-4` — R11 focused evidence is incomplete

The R11 table names an earlier candidate, includes a placeholder selector and
abbreviated command fragments, omits the required focused Python
environment-resolution selection, and cites a three-crate `check:docs` task as
proof for a larger R9/R10 rustdoc inventory. Broad green lanes do not make those
specific claims replayable.

Amend only the R11 evidence record. Identify the code candidate and final
evidence-bearing candidate unambiguously without requiring the record to hash
itself; replace every placeholder or fragment with the literal repository-native
command actually run and its positive count; include the exact focused Python
environment-resolution selection; and record the exact strict-rustdoc command
and affected-package inventory. Reuse credible existing results and rerun only
commands lacking exact evidence. Add no permanent evidence checker or new test.

## Constraints and preserved behavior

- Preserve the approved RFC 8693 `sub=A`, `act.sub=B` flow, directed invoke
  policy, semantic permission intersection, audience binding, and five-minute
  stateless tenant JWTs.
- Preserve database-free tenant/Bifrost request verification and current-state
  platform authorization.
- Preserve one `TenantTokenIssuer`, concrete `TokenVerifier`, shared
  `WyrdClient`, Bifrost facade, policy seam, canonical audit path, and audit
  publisher.
- Preserve delegated tokens without `cid`; do not infer or copy actor
  credentials into a delegated subject token.
- Preserve Python's testing-enabled developer setup and production package
  exclusion of `wyrd.testing`.
- Preserve the audit-publisher `FOR UPDATE NOWAIT` correction and all authorized
  gate-enabling changes.
- Add no auth/cache/revocation architecture, delegation store/permission,
  public API, compatibility behavior, audit sink, database lookup, test harness,
  dependency, or permanent grep/check.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `R12-1` | Runtime issuance remains unchanged; generated JSON Schema and served OpenAPI state that only human OIDC login/rotation receives a refresh token and do not promise one for machine or delegated grants |
| `R12-2` | Direct credential-backed Gate write and Oracle query/security decisions retain the verified credential ID; delegated decisions remain credential-less and retain their existing subject/actor detail |
| `R12-3` | `check:py-wheel-no-testing` passes against a default-feature extension, while the existing testing-enabled setup and test-harness lane still pass |
| `R12-4` | R11 evidence identifies its candidates, contains literal replayable focused commands with positive counts, includes the Python environment-only selection, and records strict rustdoc over the complete affected-crate inventory |

## Focused proof

1. Run the exact existing
   `exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row`
   selector, `mise run codegen:check`, and the served OpenAPI contract proof for
   the corrected `refresh_token` description.
2. Run the exact existing
   `query::service_b_acts_for_service_a_with_only_a_table_authority` selector
   after adding direct-B credential assertions; it must still prove delegation,
   denial-before-effect, direct-B success, and subject/actor attribution.
3. Run `mise run check:py-wheel-no-testing`, then
   `mise run py:test:testing`.
4. Validate every amended R11 selector against current source and rerun only
   results that lack exact credible evidence; run strict rustdoc with
   `-D missing_docs -D rustdoc::broken_intra_doc_links` over the recorded
   affected-crate inventory.

## Broader verification

Run the narrow owning lanes for the touched surfaces:

- `mise run fmt:check`
- `mise run lints`
- `mise run py:format:check`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run test:shared`
- `mise run test:sql`
- `mise run test:bifrost:journey:server`
- `mise run test:bifrost:integration:server`
- `mise run codegen:check`
- `mise run docs:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..<final-candidate>`

The user explicitly authorizes the ancillary gate-enabling changes already in
the cumulative branch. Do not remove or relitigate them. A later review still
uses the immutable cumulative candidate, not only this remediation diff.
