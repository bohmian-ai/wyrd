---
id: TASK-012-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-011, REQ-012, REQ-013, REQ-021, INV-005, AC-004, AC-005, AC-007]
depends_on: []
parent_task: TASK-012
remediates: [FIND-TASK-012-3]
---

# Close the remaining TASK-012 documentation contract

## Authority and immutable review subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Review base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Reviewed candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Validated ledger: `changes/active/oidc-production-readiness/review/TASK-012-r2/findings-validation.md`

This task is routed directly to `$wyrd-implement`. It does not reopen the
approved specification or any lead decision.

## Outcome

Finish the existing documentation correction so the changed client code states
its actual normalized-origin and revocation ownership contracts and every
materially modified Rust item satisfies the repository's hard rustdoc rule.
Change documentation only. Add no runtime behavior, compatibility mode, check,
file, option, helper, parser, dependency, test, fixture, or harness.

## Issue diagnosis — FIND-TASK-012-3

Round 1 correctly added cancellation, partial-progress, idempotency, and retry
contracts to the principal OAuth operations. It did not complete the same hard
documentation obligation for every Rust item materially modified by that
remediation:

- `crates/shared/wyrd-client/src/auth.rs:636-643` changes
  `AuthMiddleware::fmt` from the raw base URL to the normalized origin, but the
  method has no rustdoc describing the redacted diagnostic boundary.
- `crates/shared/wyrd-client/src/transport/http.rs:131-136` makes the same
  material `Debug::fmt` change without rustdoc.
- `crates/shared/wyrd-client/src/transport/config.rs:352-359` replaces a
  boolean test helper with fallible `origin`, but omits the mandatory
  `# Errors` contract.
- The materially changed tests at
  `transport/config.rs:361-410` and `auth.rs:1809-1850` retain explicit
  `expect`, `expect_err`, assertions, or fixture panics without the mandatory
  `# Panics` contracts.
- Public `HttpConfig` and `base_url` prose at
  `transport/config.rs:146-165` still says the configured value is a retained
  route prefix. The implementation now intentionally reduces the value to
  `scheme://host[:port]`; `TokenExchange`, `HttpTransport`, and saved-login
  identity discard path, query, and fragment. A caller following the prose can
  therefore expect requests under `/prefix` while the client sends them to
  root routes.
- Changed prose in the redirect test and `Cargo.toml:101-103` says `oauth2`
  owns revocation. The locked implementation instead constructs RFC 7009 as
  one form POST in `TokenExchange::revoke_refresh_token` and sends it through
  the redirect-free adapter. The manifest comment is non-blocking in isolation
  but must not remain false while this same bounded documentation is corrected.

The code and narrow verification otherwise satisfy TASK-012. No runtime fix,
additional test, or new mechanism is required.

## Intended correction outcome

The existing documentation accurately tells maintainers and public callers
that the client retains only a validated, userinfo-free deployment origin;
that debug output exposes only that origin; that the cited helper and tests
have their repository-required error and panic contracts; and that `oauth2`
owns device and refresh while the shared redirect-free adapter carries the
lead-approved RFC 7009 form POST.

## Decision-complete recommendation

Update only the existing documentation at the cited owners:

1. Add substantive rustdoc to `AuthMiddleware::fmt` and
   `HttpTransport::fmt` stating that their diagnostics expose the normalized,
   userinfo-free origin and no credential-bearing URL spelling.
2. Add `# Errors` to the existing `origin` helper, naming propagation of
   `HttpConfig::validate` refusals.
3. Add precise `# Panics` sections to the two changed configuration tests and
   the changed redirect test, naming the assertions, unexpected acceptance or
   success, fixture failures, normalization mismatch, hit-count mismatch, and
   redirect replay conditions that panic as applicable.
4. Correct the existing `HttpConfig` and `base_url` prose to say the supplied
   deployment URL is normalized to its origin and its path, query, and fragment
   are discarded before client endpoints are built. Do not preserve or add a
   path-prefix mode.
5. Correct the changed redirect-test and workspace dependency wording:
   `oauth2` owns device and refresh; the existing redirect-free adapter owns
   the RFC 8693/RFC 7523 form requests and the locked RFC 7009 form POST.

This closes the defect at the existing documentation owners. It preserves the
standard OAuth implementation, the shared target authority, and all adjacent
runtime behavior without downstream guards or a new enforcement mechanism.

## Constraints, preserved behavior, and non-goals

- Preserve RFC 7009 revocation as one form POST through the redirect-free
  client. Do not use `oauth2::revoke_token` or add an HTTPS/loopback branch.
- Preserve `webbrowser = 1.2.4`, `webbrowser::open`, `--no-browser`, the printed
  fallback URL, the accepted Windows-target proof, and rejection of PLAT-001.
- Preserve `HttpConfig::validate` returning the existing `HttpsOrigin` and the
  root-origin behavior for auth, HTTP transport, and saved-login identity.
- Preserve device and refresh on `oauth2`; RFC 8693 and RFC 7523 remain the
  retained standard form requests through the existing adapter.
- Preserve redirect refusal, saved-login locking, atomic replacement,
  middleware cache/single-flight, explicit credential precedence, tenant
  selection, and local-first best-effort logout.
- Add no path-prefix compatibility mode, runtime code, helper, option, setting,
  check, test, fixture, harness, dependency, feature, state, or file beyond
  this remediation artifact.
- Do not run or require any journey, language sweep, `test:shared`,
  `test:rust`, `gate`, or aggregate. Those belong to change review.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-012-3` | Both changed `Debug::fmt` methods have substantive diagnostic-boundary rustdoc; `origin` has `# Errors`; the three changed panic-bearing tests have precise `# Panics`; public target prose states origin normalization and path/query/fragment removal; changed ownership prose accurately distinguishes `oauth2` from the redirect-free adapter; no runtime or configuration behavior changes. |

## Focused proof

Inspect the corrected existing docs, then run only:

```text
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-client --all-targets --all-features -- -D warnings
git diff --check
```

These are the narrowest lanes covering the documentation-only Rust and
manifest write set. No runtime test, journey, full language sweep, or aggregate
is required.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-012-3` | Rustdoc on the diagnostic boundary of `AuthMiddleware::fmt`, `HttpTransport::fmt` and `TokenExchange::fmt`: each prints only the normalized origin. `# Errors` on the test helper `origin`. `# Panics` on `remote_cleartext_malformed_and_unsupported_targets_are_refused`, `https_and_loopback_http_are_accepted` and `token_exchange_never_follows_a_redirect`. `HttpConfig` and `base_url` now describe origin normalization and that path, query and fragment are discarded. The redirect test and the `Cargo.toml` `oauth2` comment now say `oauth2` owns device and refresh, while the adapter carries the RFC 8693, RFC 7523 and RFC 7009 form POSTs. The `transport_config_http` and `transport_config_enum` schemas (published copy and test golden) were regenerated with `cargo run -p wyrd-client --example gen_schemas`; the change is description text only. | `mise run fmt`; `cargo clippy --locked -p wyrd-client -p wyrd-cli --all-targets --all-features -- -D warnings`; `cargo doc --locked -p wyrd-client --no-deps` (exit 0); the `transport` target's `schema_drift` tests (4/4); `git diff --check` | PASS |

No runtime or configuration behavior changed.

`cargo doc` still prints two private intra-doc link warnings, both on lines this remediation did not touch: `bifrost/facade.rs:130` and the module doc at `saved_login.rs:8`.
