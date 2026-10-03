# TASK-012 round 1 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `TASK-012-R1-client-oauth2-closure.md`

The candidate remained at the named commit through discovery, focused follow-up,
and structured validation. Review artifacts are outside the immutable candidate.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-011 / AC-004 device login uses `oauth2` and the CLI preserves `--no-browser` | `TokenExchange::{device_authorization,device_access_token}` and `LoginFlow::run`; recorded filtered CLI journey | PASS for the primary path |
| REQ-012 saved-login renewal remains under the existing file lock and atomic store | `SavedLogins::renew` calls the `oauth2` refresh method while retaining lock, reread, reuse, rotation, and write ordering; recorded concurrent-renewal journey | PASS |
| REQ-012 logout is local-first and revokes best-effort | CLI removal precedes one lead-approved RFC 7009 form POST through the redirect-free client | PASS |
| REQ-013 / AC-005 API-key, workload, delegation, and platform paths remain shared | `AuthMiddleware` and `TokenExchange` remain the Rust owner; no SDK-specific token logic entered the diff | PASS |
| Device and refresh have no parallel hand POST/decode path | Public `TokenExchange::exchange(&TokenRequest)` still accepts `DeviceCode` and `RefreshToken`; the CLI journey executes that path | FAIL — `FIND-TASK-012-1` |
| FIND-TASK-004-5: one parsed target decision for every shared caller | `HttpConfig::validate` parses only for validation, then `TokenExchange` and `HttpTransport` retain the raw spelling | FAIL — `FIND-TASK-012-2` |
| FIND-TASK-004-9: canonical saved-login origin and userinfo refusal | Saved-login persistence and selection correctly use `canonical_origin`, but explicit credential, platform, refresh, and shared transport construction bypass that normalization | FAIL at the shared target boundary — `FIND-TASK-012-2` |
| FIND-TASK-004-10: unsafe credential directories fail closed | Existing `0o022` directory check and recorded saved-login tests | PASS |
| FIND-TASK-004-13: secret-bearing 307/308 responses are not followed | One `AuthHttp` uses `reqwest::redirect::Policy::none()`; recorded refresh/JWT/revocation redirect test | PASS |
| FIND-TASK-004-14: Wyrd-owned Windows launcher is removed | `webbrowser::open` replaces the per-OS launcher; the accepted `webbrowser` Windows-target check passed | PASS |
| Exact dependency and ownership constraints | `oauth2 = 5.0.0` with default features off; `webbrowser = 1.2.4`; dependencies remain in their narrow owners | PASS |
| Mandatory documentation for every materially changed Rust item and uncertain async completion | New adapter field/associated types, changed network methods, and the changed token fixture helper are incomplete or placeholder-documented | FAIL — `FIND-TASK-012-3` |
| Every named Rust test has exact nonzero selector evidence | Seven named tests are covered only by family/target evidence in the task record | FAIL — `FIND-TASK-012-4` |
| Prohibited complexity remains excluded | No second credential store, configurable browser command, redirect allowlist, language-specific grant logic, or compatibility route was added | PASS |

## Independent review results

| Required report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | PASS | none |
| `task-review-invariants.md` | FAIL | `INV-012-001` |
| `standards-review.md` | FAIL | `REPO-001`, `REPO-002` |
| `maintainer-review.md` | FAIL | `MNT-TASK-012-1`, `MNT-TASK-012-2` |
| `system-review.md` | PASS | none |
| `domain-review-security.md` | FAIL | `SEC-1` |
| `domain-review-durability.md` | PASS | none |
| `domain-review-platform.md` | FAIL | `PLAT-001` |

All required discovery reports are present and complete.

## Follow-up decision

`followup-review.md` was required because discovery materially disagreed about
the generic form exchange, shared target normalization, and the WSL behavior of
the approved browser dependency. It returned `RESOLVED`:

- the broad form exchange is reachable task drift;
- the shared target-normalization and userinfo gap is production-reachable;
- `PLAT-001` is outside the locked native-Windows/library decision and must not
  enter the ledger or cause invented WSL machinery.

## Independently validated finding ledger

`findings-validation.md` independently inspected the source and returned four
bounded findings:

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-012-1` | CONFIRMED | DRIFT | Constrain the custom form owner to RFC 8693 and RFC 7523; device and refresh remain only on `oauth2`; preserve the RFC 7009 exception. |
| `FIND-TASK-012-2` | REVISED | INCORRECT | Reuse one parsed, userinfo-free normalized origin for `HttpConfig`, `TokenExchange`, and `HttpTransport`. |
| `FIND-TASK-012-3` | REVISED | VIOLATION | Complete required rustdoc and uncertain-completion contracts on the changed OAuth owners and fixture helper. |
| `FIND-TASK-012-4` | CONFIRMED | VIOLATION | Run and record exact selectors for the seven named Rust tests. |

`PLAT-001` was rejected. No rejected proposal is retained as advice or a
finding. None of the retained corrections requires a new product, public
option, architecture, security model, compatibility mechanism, concurrency
semantic, or persistent-data decision.

## Prior-finding closure

This is the first TASK-012 review, so there are no earlier TASK-012 finding IDs
to preserve. Of the TASK-004 findings routed into this task, `-10`, `-13`
(client), and `-14` are closed. The saved-login portion of `-9` is closed, but
the shared effective-target portion of `-5`/`-9` remains open through
`FIND-TASK-012-2`.

## Verification assessment and limits

The task records passing filtered identity journeys, `test:shared` (732 tests),
the `wyrd-cli` library target (60 tests), codegen/N-API checks, client-tier
checks, workspace-hack, unwrap audit, format, lints, `git diff --check`, and the
accepted Windows-target `webbrowser` check. Reviewers started no competing
Cargo or `mise` processes and did not run full journey suites or aggregates.

Those results credibly cover the positive paths, but they do not close the
four validated gaps. In particular, family-lane evidence does not satisfy the
repository's exact-selector rule. Remediation is limited to focused owner tests,
the single filtered CLI journey, exact named selectors, and narrow write-set
format/lint/boundary checks. Full journey sweeps and aggregates remain reserved
for change review.

## Verdict

**FIX_REQUIRED**

Remediation is decision-complete in
`changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`.
