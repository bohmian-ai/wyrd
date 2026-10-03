# TASK-012 round-2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate remained the named commit throughout discovery, follow-up, and
structured validation. The complete base-to-candidate range was reviewed; the
round-1 candidate-to-current range was used only to locate remediation changes.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| RFC 8628 device authorization and polling use `oauth2` | `TokenExchange::{device_authorization,device_access_token}` use `oauth2::BasicClient` through `AuthHttp`; the filtered CLI journey is recorded passing. | PASS |
| RFC 6749 refresh uses `oauth2` and preserves locked saved-login renewal | `TokenExchange::refresh` is the only refresh owner; `SavedLogins::renew` retains lock, reread, rotation, and atomic replace; focused saved-login evidence is recorded. | PASS |
| The retained custom form owner serves only RFC 8693 and RFC 7523 | Public `TokenExchange::exchange` is deleted; private `grant` has only platform/session and middleware producers for those grants. | PASS |
| Lead-approved RFC 7009 behavior is preserved | Revocation remains one form POST through the redirect-free adapter; local-first best-effort logout and warnings are unchanged. | PASS |
| Every secret-bearing call is redirect-free | `AuthHttp` uses `reqwest::redirect::Policy::none()`; the exact redirect test is recorded one-selected/one-passed. | PASS |
| One parsed normalized origin owns OAuth, HTTP, and saved-login identity | `HttpConfig::validate` returns `HttpsOrigin`; `TokenExchange`, `HttpTransport`, and `canonical_origin` consume it; focused origin/refusal tests pass. | PASS |
| Userinfo, remote cleartext, and unsafe credential storage fail closed | Existing origin and credential-store owners enforce the boundaries; exact tests are recorded passing. | PASS |
| Browser launch uses `webbrowser` with no Wyrd launcher | `webbrowser::open`, printed fallback, and `--no-browser` remain; the accepted Windows-target `webbrowser` check passed. `PLAT-001` remains rejected. | PASS |
| Shared client ownership and API-key/workload regressions are preserved | No SDK-specific token logic entered the diff; machine-grant and tier evidence is recorded passing. | PASS |
| Every named Rust test has exact nonzero-selector evidence | TASK-012 records six library selectors and the setup-wrapped integration selector, each one-selected/one-passed. | PASS |
| Every new or materially modified Rust item has accurate mandatory rustdoc | Important async OAuth contracts are present, but two changed `Debug::fmt` methods, one fallible helper, and three changed panic-bearing tests remain incomplete; public target and changed revocation-owner prose are inaccurate. | FAIL — `FIND-TASK-012-3` |
| Prohibited/non-goal complexity remains excluded | No compatibility path, new parser, setting, store, recovery protocol, WSL branch, launcher, permanent check, or broad test harness entered the implementation. | PASS |

## Independent review results

| Review | Result | Material conclusion |
|---|---|---|
| Behavior | PASS | No runtime acceptance finding; prior runtime findings closed. |
| Invariants | PASS | Producer-to-sink invariants and prior findings 1, 2, and 4 closed. |
| Repository standards | FAIL | Proposed `REPO-R2-001` for incomplete and inaccurate documentation. |
| Maintainer | PASS | OAuth ownership and principal maintenance contracts otherwise clear. |
| System resilience | PASS | No reachable failure or recovery regression. |
| OAuth/OIDC security | PASS | Redirect, origin, secret, and grant trust boundaries pass. |
| Credential durability/concurrency | PASS | Lock, reread, rotation, atomic replace, and logout ordering pass. |
| Platform portability | PASS | Accepted Windows proof passed; no platform finding. |

## Follow-up decision

A focused follow-up was required because the standards review conflicted with
the behavior, invariant, and maintainer conclusions about documentation
closure. It returned `RESOLVED`: the standards claim is supported, narrowed to
existing documentation only. The missing item-level rustdoc is mandatory under
the repository's hard gate, and the public `HttpConfig` prefix prose has a
concrete caller-visible target consequence. The stale revocation-owner wording
is non-blocking alone but belongs in the same bounded documentation correction.

## Validated finding ledger

Structured Ponytail validation revised the discovery proposals into the
still-open remainder of an existing finding rather than assigning a new ID:

| Finding | Status | Classification | Required correction |
|---|---|---|---|
| `FIND-TASK-012-3` | REVISED, OPEN | VIOLATION | Complete mandatory rustdoc on the two changed `fmt` methods, the fallible `origin` helper, and three changed tests; correct `HttpConfig`/`base_url` origin semantics and changed revocation-owner wording. Documentation only. |

No `FIND-TASK-012-5` is assigned. The detailed validated ledger is in
`findings-validation.md`.

## Prior-finding closure

- `FIND-TASK-012-1`: **CLOSED** — device and refresh have no custom form path.
- `FIND-TASK-012-2`: **CLOSED** — all affected owners consume the same parsed,
  normalized, userinfo-free origin.
- `FIND-TASK-012-3`: **OPEN, REVISED** — important async contracts were added,
  but the item-by-item hard rustdoc obligation and accurate public prose remain
  incomplete.
- `FIND-TASK-012-4`: **CLOSED** — all seven exact selectors are recorded with
  nonzero passing results.

## Verification assessment and limits

The candidate records the required narrow remediation proof: seven exact named
tests, one filtered CLI journey, focused client/CLI Clippy, format, client-tier
checks, unwrap audit, and `git diff --check`. Reviewers independently reran the
two exact origin tests, the exact redirect test, the accepted Windows-target
`webbrowser` check, and range/report diff checks. No full journey suite,
language sweep, `test:rust`, `gate`, or aggregate was run or required; those
remain change-review evidence. This scope is sufficient to establish the one
remaining documentation-only finding and does not limit the verdict.

## Verdict

**FIX_REQUIRED**

The bounded remediation is
`changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`.
