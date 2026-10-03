# TASK-012 round-2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate remained the named commit throughout validation. This pass read
the complete cumulative diff, the round-1 ledger and remediation, and every
round-2 discovery and follow-up report, then independently inspected the cited
owners, callers, sibling consumers, tests, and current source. It ran no broad
journey or aggregate. The lead-decided RFC 7009 form POST, accepted
`webbrowser` Windows-target proof, and rejection of `PLAT-001` were not
reopened.

## Proposal validation

| Discovery proposal or conclusion | Disposition | Source validation |
|---|---|---|
| `REPO-R2-001`; `FOLLOWUP-R2-001` | **REVISED** as the still-open documentation remainder of `FIND-TASK-012-3` | The round-1 remediation materially changed both `Debug::fmt` methods, the fallible `origin` test helper, and three panic-bearing tests without completing the rustdoc required by `AGENTS.md` section 16. The normalized-origin implementation also makes the unchanged public `HttpConfig`/`base_url` prefix prose false, and changed cumulative prose incorrectly says `oauth2` owns revocation. The follow-up correctly narrows the correction to existing documentation and no runtime mechanism. This is not a new cause and therefore does not receive `FIND-TASK-012-5`. |
| Behavior and invariant reports: no new findings; `FIND-TASK-012-1`, `-2`, and `-4` closed | **CONFIRMED**, except for the documentation remainder above | `TokenExchange::exchange` is absent; the only production callers of private `grant` are `platform_session` and `AuthMiddleware::post_token_request`, whose producers construct RFC 8693 or RFC 7523 requests. Device and refresh callers use `oauth2`. `HttpConfig::validate` returns the existing `HttpsOrigin`, consumed by `TokenExchange`, `HttpTransport`, and saved-login canonicalization. The seven exact selectors and one-selected/one-passed results are recorded in TASK-012. |
| Maintainer report: `FIND-TASK-012-3` closed | **REJECTED in part** | The async OAuth owners now carry the important cancellation, partial-progress, idempotency, and retry contracts, but item-by-item inspection confirms the missing rustdoc and false public target prose identified above. The maintainer report otherwise finds no independent maintainability defect. |
| System-resilience report: no finding | **CONFIRMED** | Device redemption, refresh rotation, revocation, cancellation, saved-login locking, local-first logout, and client-only failure containment remain at their existing owners. The candidate adds no retry journal, recovery protocol, service-wide failure path, or availability mechanism. |
| Security domain report: no finding | **CONFIRMED** | All secret-bearing calls converge on the redirect-free adapter; normalized, userinfo-free origins are retained before auth or HTTP clients are built; cross-origin authenticated URLs fail closed; device and refresh have only the vetted-library path; and no secret-bearing diagnostic regression remains. |
| Durability domain report: no finding | **CONFIRMED** | The existing configuration-directory lock, reread-before-refresh, atomic replacement, rotation persistence, and logout ordering remain the only saved-login durability path. No second store, backup token, retry state, or recovery option entered the diff. |
| Platform domain report: no finding | **CONFIRMED** | `webbrowser::open` directly owns launch behavior, the Wyrd per-OS launcher is deleted, `--no-browser` and printed fallback remain, and no WSL branch, launcher option, escaping layer, or platform harness was added. The locked Windows proof is sufficient. |

No proposal requires a new product, public API, compatibility behavior,
security decision, concurrency semantic, persistent state, dependency, check,
setting, or option. The candidate's runtime behavior otherwise follows the
published OAuth conventions and the selected established libraries; no
additional standard-only drift survived validation.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-012-1` | The public arbitrary `TokenRequest` exchange is deleted. Private `grant` is reached in production only from the existing RFC 8693 and RFC 7523 producers; device and refresh use `oauth2`, and the journey no longer drives the raw device form. | **CLOSED** |
| `FIND-TASK-012-2` | `HttpConfig::validate` returns the existing normalized `HttpsOrigin`; `TokenExchange`, `HttpTransport`, and saved-login identity consume it. Userinfo is refused without disclosure and path/query/fragment spellings resolve to the root origin. The remaining false public prose is documentation debt folded into `FIND-TASK-012-3`, not a second runtime-origin defect. | **CLOSED** |
| `FIND-TASK-012-3` | The adapter, OAuth operations, form owner, and fixture helper now document the material uncertain-completion behavior, but the remediation did not document every materially modified Rust item and left public/changed prose inconsistent with the implementation. | **OPEN — REVISED BELOW** |
| `FIND-TASK-012-4` | TASK-012 records all seven exact commands and reports one selected, one passed for each. | **CLOSED** |

## Final deduplicated finding ledger

### FIND-TASK-012-3 — REVISED — VIOLATION: remediation documentation remains incomplete and contradicts the normalized target and revocation owners

- **Discovery sources:** round-1 `FIND-TASK-012-3`, `REPO-R2-001`,
  `FOLLOWUP-R2-001`.
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` make substantive rustdoc mandatory for every
  new or materially modified Rust item, including private methods, helpers,
  and tests; every fallible function needs `# Errors`, and each remaining
  panic needs `# Panics`. Documentation must describe the behavior the
  implementation actually exposes.
- **Exact locations:**
  `crates/shared/wyrd-client/src/auth.rs:636-643,1809-1850`;
  `crates/shared/wyrd-client/src/transport/config.rs:146-165,352-410`;
  `crates/shared/wyrd-client/src/transport/http.rs:131-136`; and the related
  changed ownership comment at `Cargo.toml:101-103`.
- **Producer-to-consumer trace:** `HttpConfig::validate` now reduces every
  configured deployment URL to a userinfo-free `HttpsOrigin` and drops its
  path, query, and fragment. `TokenExchange::new`, `HttpTransport::new`, and
  `canonical_origin` consume that value, while `AuthMiddleware::fmt` and
  `HttpTransport::fmt` expose only the normalized origin. The two `fmt`
  methods were materially changed to that diagnostic boundary but remain
  undocumented. The new fallible `origin` helper has no `# Errors`; the two
  changed config tests and the changed redirect test retain `expect`,
  assertions, and fixture panics without `# Panics`. Meanwhile public
  `HttpConfig`/`base_url` prose still promises a retained route prefix even
  though all callers address root routes. The redirect-test prose and the
  changed workspace dependency comment also attribute RFC 7009 revocation to
  `oauth2`, although the locked implementation constructs the single form
  POST in `TokenExchange::revoke_refresh_token` and sends it through
  `AuthHttp`.
- **Observable consequence:** the explicit hard documentation gate remains
  unsatisfied. More importantly, a public caller following the configuration
  prose can expect `https://host/prefix` to retain `/prefix` although the
  client deliberately sends auth and API traffic to `https://host` root
  routes; the ownership prose sends a maintainer to the wrong component for
  revocation behavior.
- **Decision-complete correction:** change documentation only. Add
  substantive rustdoc to the two changed `fmt` methods stating that debug
  output exposes only the validated, userinfo-free origin; add `# Errors` to
  the existing `origin` helper; add precise `# Panics` sections to the three
  materially changed tests; and update the existing `HttpConfig`/`base_url`
  prose to state that validation retains only the deployment origin and
  discards path, query, and fragment. Correct the changed redirect-test and
  workspace dependency wording so `oauth2` owns device and refresh, while the
  existing redirect-free adapter owns the locked RFC 7009 form POST. Preserve
  all runtime behavior. Add no helper, check, file, option, path-prefix
  compatibility mode, parser, retry state, dependency, or test harness.
- **Focused closure proof:** inspect the corrected existing docs, run
  `mise run fmt`, run
  `mise exec -- cargo clippy --locked -p wyrd-client --all-targets --all-features -- -D warnings`,
  and run `git diff --check`. No runtime test, journey, full language sweep,
  or aggregate is required for this documentation-only correction.

## Validation outcome

The deduplicated ledger contains one bounded, previously assigned finding:
`FIND-TASK-012-3`. `FIND-TASK-012-1`, `FIND-TASK-012-2`, and
`FIND-TASK-012-4` are closed with source evidence. No `FIND-TASK-012-5` is
assigned. The remaining correction is confined to existing documentation and
the narrow proof above; it does not reopen any locked lead decision or require
nonstandard complexity.
