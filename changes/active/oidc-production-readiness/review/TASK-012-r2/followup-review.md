# TASK-012 round 2 focused follow-up review

## Immutable subject and uncertainty

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Prior remediation candidate used only to locate round-1 changes:
  `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

This follow-up resolves only the conflict between `standards-review.md`, which
proposes `REPO-R2-001`, and the behavior, invariant, and maintainer reports,
which conclude that `FIND-TASK-012-3` is closed. It does not reopen the locked
RFC 7009 form POST, Windows-target `webbrowser` proof, or rejection of
`PLAT-001`.

## Paths and authority inspected

- `AGENTS.md:681-730`, especially section 16's requirement that every new or
  materially modified Rust item, including private methods, test helpers, and
  tests, have substantive rustdoc; every fallible function have `# Errors`;
  and every remaining panic have `# Panics`.
- `architecture/agent-rules.md:21-35`, which makes that documentation rule a
  hard acceptance criterion and names missing or placeholder rustdoc as
  `BLOCK_BEFORE_MERGE`.
- `architecture/references/languages/maintainer-style.md`, especially
  “Documentation: explain what a maintainer cannot infer” and the review
  threshold.
- `architecture/references/languages/rust-core.md:754-772`, which expressly
  includes private helpers, test helpers, and test functions and requires
  error and panic contracts.
- Complete cumulative and remediation diffs for `Cargo.toml`,
  `crates/shared/wyrd-client/src/auth.rs`,
  `crates/shared/wyrd-client/src/transport/config.rs`, and
  `crates/shared/wyrd-client/src/transport/http.rs`, plus the complete current
  bodies of each cited item.
- The original task, round-1 remediation task, and round-1 validated ledger,
  particularly `FIND-TASK-012-2` and `FIND-TASK-012-3`.

## Evidence resolving the conflict

### Items to which the hard rustdoc rule applies

| Item | Change evidence | Required documentation | Resolution |
|---|---|---|---|
| `AuthMiddleware::fmt` (`auth.rs:636-643`) | The remediation changes the diagnostic field from the raw `exchange.base_url` string to the normalized `exchange.origin`. That changes the redaction/target invariant exposed by `Debug`. | The method is materially modified and needs substantive rustdoc stating that diagnostics expose only the validated, userinfo-free origin. It is not fallible and needs neither `# Errors` nor `# Panics`. | `standards-review.md` is correct. This is not optional style under the repository's explicit hard gate. |
| `HttpTransport::fmt` (`transport/http.rs:131-136`) | The remediation changes the diagnostic value from raw `base_url` to normalized `origin`. | The materially modified method needs the same substantive diagnostic-boundary rustdoc. It is not fallible and needs neither `# Errors` nor `# Panics`. | `standards-review.md` is correct. |
| `origin` (`transport/config.rs:352-359`) | The remediation replaces boolean helper `accepts` with a new helper returning `Result<HttpsOrigin, WyrdClientError>`. | Its existing sentence explains its role, but the new fallible helper also requires `# Errors`, naming propagation of `HttpConfig::validate` refusals. It has no panic and needs no `# Panics`. | `standards-review.md` is correct. |
| `remote_cleartext_malformed_and_unsupported_targets_are_refused` (`transport/config.rs:361-389`) | The remediation adds userinfo cases, error extraction through `expect_err`, and a non-disclosure assertion. | The materially modified test needs `# Panics` for an unexpectedly accepted target or a refusal that discloses userinfo. | `standards-review.md` is correct. |
| `https_and_loopback_http_are_accepted` (`transport/config.rs:391-410`) | The remediation changes the test from boolean acceptance to exact normalized-origin comparisons and adds default-port/query/fragment cases. | The materially modified test needs `# Panics` for validation failure or a mismatched normalized origin. | `standards-review.md` is correct. |
| `token_exchange_never_follows_a_redirect` (`auth.rs:1809-1850`) | The remediation changes the exercised owner from deleted public `exchange` to private `grant`, which is part of closing the duplicate-grant finding. | The materially modified test needs `# Panics` for fixture/construction failure, an unexpectedly successful redirected call, wrong redirect hit count, or any replay at the target. | `standards-review.md` is correct. |

The maintainer, behavior, and invariant reports correctly establish that the
important async OAuth owners now describe cancellation, partial progress,
idempotency, and retry behavior. They do not establish the broader remediation
acceptance criterion that *every* new or materially modified Rust item has the
required rustdoc. `FIND-TASK-012-3` therefore is not fully closed.

### Public `HttpConfig` contract prose

`HttpConfig::validate` now deliberately reduces `base_url` to a root
`HttpsOrigin`, and `TokenExchange::new` and `HttpTransport::new` retain that
value. The approved remediation explicitly requires path, query, and fragment
spellings to resolve to the root deployment origin and forbids a path-prefix
compatibility mode. However, `transport/config.rs:146-165` still tells public
callers that `base_url` is a “common prefix” and that ingest routes are appended
to it.

Those `HttpConfig` and `base_url` doc lines were not textually changed in the
remediation diff, so they are not missing-rustdoc violations merely because
they exist near changed code. They nevertheless became false as a direct
consequence of the changed validation/retention contract. A caller following
the public prose may provide `https://host/prefix` and expect requests under
`/prefix`; the implementation sends requests to root routes instead. That is a
concrete public-contract and deployment-target consequence, not style-only
cleanup. The smallest correction is prose on the existing type/field stating
that the configured deployment URL is normalized to its origin and that path,
query, and fragment are discarded. No option, compatibility mode, parser,
check, or test is warranted.

### Revocation ownership wording

Two changed cumulative descriptions incorrectly attribute revocation to
`oauth2`:

- `Cargo.toml:101-103` says the dependency owns device, refresh, and revocation
  grants, although the locked implementation sends RFC 7009 as one form POST
  through the redirect-free adapter.
- `auth.rs:1809-1811` describes “the `oauth2` refresh and revocation,” although
  only refresh is constructed by `oauth2`; revocation and the retained RFC
  8693/RFC 7523 path are form POSTs through the same adapter.

The Cargo comment is changed in the cumulative task but is not Rustdoc and has
no independent runtime or public-contract consequence. It is non-blocking by
itself. The test prose is on a materially modified test and is part of the
incomplete rustdoc contract. Because a bounded documentation correction is
already required, both stale ownership descriptions should be corrected in
that same boundary. This does not reopen revocation or justify a separate
finding or round; it makes the comments match the locked conventional
implementation.

## Proposed finding

### FOLLOWUP-R2-001 — `REPO-R2-001` is supported with a narrowed correction boundary

- **Classification:** `VIOLATION`.
- **Violated obligation:** `AGENTS.md` section 16, the corresponding hard rule
  in `architecture/agent-rules.md`, and remediation acceptance criterion
  `FIND-TASK-012-3` require complete, accurate rustdoc on every new or
  materially modified Rust item.
- **Locations:** `crates/shared/wyrd-client/src/auth.rs:636-643,1809-1850`;
  `crates/shared/wyrd-client/src/transport/config.rs:146-165,352-410`;
  `crates/shared/wyrd-client/src/transport/http.rs:131-136`. The related
  non-blocking stale ownership comment is `Cargo.toml:101-103`.
- **Evidence and consequence:** two materially changed `fmt` methods have no
  rustdoc; the new fallible helper lacks `# Errors`; three materially changed
  tests with explicit panic paths lack `# Panics`; the public configuration
  prose promises a retained path prefix that the required origin normalization
  discards; and changed revocation wording misstates which owner builds the
  request. This leaves the repository's explicit hard documentation gate red
  and gives callers a false target contract.
- **Smallest testable correction:** update only the existing documentation:
  document the two `fmt` diagnostic boundaries; add `# Errors` to `origin`;
  add precise `# Panics` sections to the three changed tests; state on
  `HttpConfig`/`base_url` that validation retains only the deployment origin;
  and remove the incorrect attribution of revocation to `oauth2` from the
  changed test prose and workspace dependency comment. Do not change runtime
  behavior, add a documentation check, add an option, preserve path prefixes,
  or alter the locked revocation implementation. Source inspection, formatting,
  and the existing narrow `wyrd-client`/`wyrd-cli` lint command are sufficient
  proof; no journey or aggregate is required.

The absent rustdoc sections and false public target prose are material under
the governing authority. The Cargo comment would be style-only/non-blocking in
isolation and is included only because the same bounded documentation
correction is already necessary.

## Result

**RESOLVED**

The reports conflict because the passing reviews assessed the async OAuth
maintenance contracts but did not apply the explicit item-by-item rustdoc gate
to every remediation-modified method, helper, and test. `REPO-R2-001` remains
supported, with the correction bounded to existing prose and no invented
mechanism, setting, file, option, compatibility behavior, or additional broad
verification.
