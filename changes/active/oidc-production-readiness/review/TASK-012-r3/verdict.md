# TASK-012 round-3 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Round-2 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate remained the named commit through discovery, structured
validation, and final focused verification. The complete cumulative
base-to-candidate range was reviewed; the latest remediation diff was used only
to locate and verify the final documentation closure.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-011 / AC-004: CLI device login uses standard RFC 8628 behavior, opens through the platform library, prints no credential, and preserves `--no-browser` | `TokenExchange::{device_authorization,device_access_token}` use `oauth2` through `AuthHttp`; `LoginFlow::run` prints the verification URL and calls `webbrowser::open` only when enabled. The filtered CLI journey is recorded passing. | PASS |
| REQ-012 / AC-004: all first-class SDKs share saved-login selection and renewal, with explicit precedence, tenant/newest selection, file locking, reread, rotation persistence, and local-first logout | `ClientConfig::resolve_credential`, `SavedLogins`, `SavedLoginSource`, and `AuthMiddleware` remain the shared Rust owners. Recorded Rust, Python, TypeScript, concurrent-renewal, and CLI proofs pass; focused tenancy and origin tests were rerun in round 3. | PASS |
| REQ-013 / AC-005: machine credential paths remain independent and shared | API-key, workload, delegation, and platform callers still converge on `AuthMiddleware` and the constrained private form owner. The exact fixture-backed API-key test and workload journey are recorded passing. | PASS |
| REQ-021: device and refresh use `oauth2`; retained standard forms are limited to RFC 8693, RFC 7523, and the locked RFC 7009 exception | The former public arbitrary-form exchange is deleted. Device and refresh have only the typed `oauth2` methods; revocation is one form POST through the redirect-free adapter as directed. | PASS |
| Secret-bearing requests do not follow redirects | One `AuthHttp` client uses `reqwest::redirect::Policy::none()` for every grant and revocation request. Focused redirect proof passes with no target replay. | PASS |
| One parsed, normalized, userinfo-free deployment origin owns OAuth, authenticated HTTP, and saved-login identity | `HttpConfig::validate` returns `HttpsOrigin`; `TokenExchange`, `HttpTransport`, authenticated absolute-URL checks, and `canonical_origin` consume it. Origin/refusal and tenant-selection tests pass. | PASS |
| Credential storage and renewal remain durable under concurrent local processes | The existing directory lock, reread-before-refresh, atomic replacement, restrictive permissions, and shared remove path remain intact. The process-level concurrent-renewal evidence is recorded passing. | PASS |
| Browser launch uses `webbrowser` without Wyrd shell or platform machinery | The hand launcher is deleted, `webbrowser = 1.2.4` is pinned in the CLI owner, the accepted Windows-target dependency check passes, and `PLAT-001` remains rejected. | PASS |
| Mandatory documentation and generated public descriptions match the implemented contracts | R1 documents OAuth ownership and uncertain completion. R2 completes diagnostic-boundary rustdoc, helper errors, test panics, origin-only public prose, and accurate revocation ownership wording. Source and generated schema copies agree; focused Clippy, rustdoc, and schema drift pass. | PASS |
| Every named Rust test has exact nonzero-selector evidence | TASK-012 records six exact library selectors and the setup-wrapped integration selector, each selecting and passing one test. | PASS |
| Dependency, SDK ownership, and non-goal constraints remain intact | `oauth2` remains exactly 5.0.0 with default features off; no language-specific token logic, second store, compatibility path, configurable browser command, WSL branch, retry journal, new check, or broad harness entered the implementation. | PASS |

## Independent review results

| Required report | Result | Material findings |
|---|---|---|
| `task-review-behavior.md` | PASS | none |
| `task-review-invariants.md` | PASS | none |
| `standards-review.md` | PASS | none |
| `maintainer-review.md` | PASS | none |
| `system-review.md` | PASS | none |
| `domain-review-security.md` | PASS | none |
| `domain-review-durability.md` | PASS | none |
| `domain-review-platform.md` | PASS | none |
| `domain-review-tenancy.md` | PASS | none |

All required reports are present and complete. One initial tenancy reviewer was
unable to write its required artifact and was not accepted as evidence; a fresh
independent replacement produced the required report. No unavailable reviewer
or missing report was converted into a verification limitation.

## Follow-up decision

No focused follow-up was needed. The discovery reports materially agree, their
proposed finding union is empty, and they reveal no conflicting claim,
unreviewed reachable path, or unresolved common source from the prior
remediations.

## Validated finding ledger

`findings-validation.md` independently validated the discovery reports and
actual source. Its final deduplicated ledger is **empty**. No
`FIND-TASK-012-5` is assigned.

`PLAT-001` remains rejected under the locked lead decision and is not a
finding, optional recommendation, or verification limit.

## Prior-finding closure

- `FIND-TASK-012-1`: **CLOSED** — the arbitrary custom form entry is deleted;
  device and refresh are reachable only through `oauth2`.
- `FIND-TASK-012-2`: **CLOSED** — OAuth, authenticated HTTP, and saved-login
  consumers use the same parsed, normalized, userinfo-free origin.
- `FIND-TASK-012-3`: **CLOSED** — the R1 and R2 changes together satisfy the
  item-level rustdoc, async lifecycle, diagnostic, public target, panic/error,
  and ownership documentation obligations.
- `FIND-TASK-012-4`: **CLOSED** — all seven named Rust tests have exact
  one-selected/one-passed evidence.

## Verification assessment and limits

The cumulative task record contains the focused CLI, Rust, Python, TypeScript,
concurrent-renewal, workload, owner-lane, boundary, code-generation, and exact
selector evidence required for the runtime change. Round 3 added only narrow
verification applicable to the final documentation/schema write set:

- `mise run fmt:check` — passed;
- `mise exec -- cargo clippy --locked -p wyrd-client --all-targets --all-features -- -D warnings` — passed;
- `mise run check:transport-schema-drift` — 73 passed, including all four schema-drift tests;
- `mise exec -- cargo doc --locked -p wyrd-client --no-deps` — passed with the two previously recorded warnings in untouched `bifrost/facade.rs` and `saved_login.rs` module documentation;
- cumulative and review-directory `git diff --check` — passed;
- focused round-3 security, tenancy, origin, credential-selection, and accepted Windows dependency checks — passed as recorded in the domain reports.

No full identity journey suite, language sweep, `test:shared`, `test:rust`,
`gate`, or other aggregate was run or required. The narrow proof directly
covers the write set and does not prevent a conclusion.

## Verdict

**PASS**

The resulting repository satisfies the original TASK-012 exactly. The
implementation follows standard OAuth 2.0/OIDC conventions through the vetted
libraries selected by the task, preserves the locked RFC 7009 and platform
decisions, closes every prior finding, and adds no unearned mechanism or
unrelated change.
