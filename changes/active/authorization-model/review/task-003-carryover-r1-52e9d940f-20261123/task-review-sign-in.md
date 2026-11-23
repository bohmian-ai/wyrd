# Sign-in admission and BFF boundary review

Reviewed candidate `52e9d940fd57c49a3ab5084fb319171e21eafb37` against prior
`b7206eafefbf7e981898eca3fc22a5d5ae76b815`. Scope: new REQ-020/AC-015,
R1 source-forwarding finding -7, and the affected public-error consumers of -8.
This is independent discovery; findings require the separate validator.

Approved on-disk revision 9 is pinned to spec blob
`55689bae96c62e78b1ea8d765a32629664b4e603`; owning security-posture input is
`0210235fb05322ef6e888526c4c9ba6bec02b234`. Both hashes were checked and neither
file was changed. The committed spec is revision 8. No implementation file was
modified. Root AGENTS, SQL AGENTS, the review skill, workflow, testing guide,
maintainer style, Rust/errors/agent-surface guides, human-sign-in authority and
relevant deployment authority were read. CodeGraph was consulted first.

## Navigation and coverage

| Promise / seam | Source and actual proof inspected | Assessment |
|---|---|---|
| Admission precedes tenant/provider resolution and pending state | `auth/browser.rs:238–286`, platform `identity.rs:850–863`, `auth/cli_login.rs:92–111`: parsed request passes to shared admission before the existing session/login/authorization owner. Browser origin validation precedes admission without resolving a tenant/provider or writing state. | Source supports the server promise. |
| Shared 300/hour source window, independent signup | `wyrd-auth/src/social.rs:1450–1470` uses `sign_in` pseudonym scope, shared `SignupAdmissionScope::SignIn`, limit 300 and one-hour duration. SQL `queries/platform/signup.rs:271–299` atomically increments `(scope,key)` with Postgres statement timestamps. Existing migration admits the new scope, as revision 9 expressly permits. | Source supports replica/tenant/route sharing; no new limiter/table. |
| Trusted source, IPv6 /64, keyed storage | `auth/source.rs:45–64` trusts only configured peers' single valid header; mapped IPv4 normalizes. `wyrd-auth/src/admission.rs:12–20` groups IPv6 /64; `PlatformKey::pseudonym` derives through the platform key. | Shared owner reused. Existing source and pseudonym tests cover pure normalization/separation. Production BFF handoff still has S2 below. |
| 301st refusal and prohibited side effects | `signup.rs:2201–2289`: 300 unknown-tenant starts split between browser/device are followed by exact 429/code/wait assertions for browser company/social, platform and device. Combined pending-row count and provider request count stay unchanged. Browser refusals set no cookie. | Assertions prove the HTTP admission/refusal boundary, rather than simply checking a response exists. The existing BFF action consumer loses this result: S1 below. |
| Replica reuse, another source, separate signup budget | `signup.rs:2292–2348`: a second replica refuses the exhausted source; another source completes social sign-in into the same workspace; the refused source starts signup, and a signup-throttled source starts sign-in. | Credible focused proof. Different tenants share the same source key structurally; no duplicate tenant matrix needed. |
| Missing source/key, unavailable counters, password/root unaffected | `signup.rs:2351–2414` checks missing source and keyless browser/platform/device 503; exact keyless browser/platform Retry-After 1 and device absence; pending counts unchanged; root login succeeds. `signup_refuses_without_its_derivation_key` asserts social-start 503/no cookie/no admission window and successful password login. Existing signup unavailable-counter tests rename the same table and assert 503/no provider request; the new sign-in path reuses that exact SQL operation and `store_error` mapping before proceeding. | Runtime assertions and shared-source tracing support fail-closed behavior. No direct sign-in counter-table-outage assertion exists; this is explicitly reused dependency proof, not a claim that the new journey executes that case. |
| Stable disabled-method 403 | `WyrdError::MethodUnavailable`, browser `method_unavailable` at :820 and social `signup_unavailable` use the derive-backed 403 variant. Affected signup/method-withdrawal assertions pin `WYRD_AUTH_403_METHOD_UNAVAILABLE`. | Rust direct HTTP correction is present. UI safe projection still substitutes generic denied metadata (same missing projection seam as S1). |

Execution: this specialist ran no Cargo/build/test commands. The caller reports
`ui:check` 101 PASS and six exact signup journeys PASS, including the new
sign-in journey in run `97730818-58ed-4553-9df4-521ef8ccd83a`; canonical receipts
belong in `verification.md`. Source assertions above were independently read;
caller receipts are not represented as specialist executions. The task records
REQ-020 TDD VIOLATED because production preceded the journey; later GREEN does
not erase that history. Cycle 11's intentional RED and deferred cycles 12–20
are outside this acceptance scope.

## Proposed findings

### SIGNIN-S1 — Existing sign-in page actions turn the new admission refusal into 502

- **Classification:** INCORRECT; material public consumer seam.
- **Authority:** revision-9 REQ-020 second bullet (browser sign-in refusal is
  429 with `WYRD_AUTH_429_SIGN_IN_ATTEMPTS`, positive whole-second retry detail
  and `Retry-After`); repository `agent_docs/errors.md` public errors must project
  the catalog exactly; the existing BFF is the caller of this same browser start.
- **Exact location:** `wyrd-ui/src/lib/server/problem.ts:56–59`, with missing
  variant in `crates/wyrd-spec/examples/gen_schemas.rs:389–476`; caller
  `auth/browser-sessions.ts:213–217`; exposed at
  `routes/t/[tenantKey]/login/+page.server.ts:64–67,80–83`.
- **Starting condition / trigger:** a visitor exhausts the source's sign-in
  window, then uses the existing company SSO or social sign-in page action.
- **Expected:** the person receives the server's admission refusal and usable
  retry interval, so the same required failure remains recognizable through
  its shipped consumer.
- **Actual / evidence:** `BrowserSessions.begin` calls `refuse`, which calls
  `upstreamProblem`. `ui_problem_examples.json` has neither SignInAttempts nor
  a 429 fallback. `safeProblem` returns the internal example for the new code;
  `upstreamProblem` therefore returns the upstream example (502). The actions
  return that 502 and safe upstream body; they also do not relay Retry-After.
  Even adding the example alone loses the interval, since `safeProblem:31`
  keeps dynamic retry detail only for `passwordAttempts`. The direct
  `forwardAuth` route does preserve headers/body; it is a different consumer.
- **Consequence:** a normal limit reached in the existing UI is reported as
  backend failure, and the user cannot determine when to retry. Existing UI
  tests exercise password throttles and do not cover this new producer output.
- **Smallest testable correction:** extend the existing generated safe-problem
  projection for the affected sign-in error, retain its bounded retry detail,
  and relay Retry-After from the existing actions as password action already
  does. Prove one exhausted-source action refusal through the current BFF
  harness; no new error framework or complete status-code matrix. The same
  projection owner should preserve the changed MethodUnavailable 403 metadata
  where encountered, rather than relabeling it as RBAC denial.

### SIGNIN-S2 — Source forwarding remains collapsed in the official image's Node path

- **Classification:** INCORRECT; incomplete closure of R1 finding -7.
- **Authority:** REQ-014 trusted-source rule, REQ-020 first bullet; R1 bounded
  correction 7 explicitly covers the production Node-to-Rust path with distinct
  trusted visitors. `architecture/operations/deployment-and-release.md:171–175`
  specifies deployment-configured ingress/header; :319–325 owns the official
  nginx + Node + Rust topology. This concerns that existing supported topology,
  not an invented proxy deployment.
- **Exact location:** `wyrd-ui/src/lib/server/upstream.ts:59–61`, with missing
  source-restoration wiring at `docker/official/extras/entrypoint.sh:31–35`.
- **Starting condition / trigger:** run the official image behind the trusted
  ingress that restores distinct visitor addresses, configure
  `WYRD_TRUSTED_INGRESS` to include the co-located proxy/BFF as required by
  `HttpConfig` and set `WYRD_CLIENT_ADDRESS_HEADER` to the restored header.
  Use ordinary Node-rendered login page actions. No adapter-specific
  `ADDRESS_HEADER` was supplied: neither the image startup nor deployment
  authority supplies/requires that setting.
- **Expected:** the restored trusted visitors reach Rust as distinct sources,
  and their windows remain independent.
- **Actual / evidence:** `stateSource` replaces the restored header with
  `getRequestEvent().getClientAddress()`. Official startup sets
  `PROTOCOL_HEADER` and `HOST_HEADER`, but no `ADDRESS_HEADER`. The installed
  adapter-node handler defaults that variable to empty (:20) and then returns
  `req.connection.remoteAddress` (:145–149), which is nginx's loopback peer for
  every UI request. Rust trusts that BFF peer and accepts the forwarded loopback
  value, collapsing visitors again. The new journey supplies
  `ADDRESS_HEADER=x-forwarded-for` and `XFF_DEPTH=1` at
  `identity_ui_e2e.rs:190–194`; therefore it proves a separately configured Node
  process, not the source restoration that the actual image starts with.
  Both Debian and Rocky images use the shared entrypoint. Direct `/auth/*`
  nginx routes bypass Node and cannot establish page-action correctness.
- **Consequence:** one visitor's sign-in/password flood can throttle other
  visitors using the official UI despite distinct addresses restored by trusted
  ingress. The original production source-collapse defect is not fully closed.
- **Smallest testable correction:** finish the source-restoration handoff in
  the existing deployment/BFF owner so adapter-node consumes the trusted source
  established by that deployment before `stateSource` forwards it. Preserve
  rejection of browser-forged addresses; do not blindly trust a public header.
  Align the focused existing Node journey's startup configuration with the
  supported image configuration. No second limiter, proxy service or broad
  new deployment matrix is needed.

## Wait and scope audit

Admission performs platform-key loading at most once per process, then one
atomic operator-pool counter upsert per start. It holds no tenant transaction
across provider calls, introduces no task fanout, sleeps, polling, or blocking
lock on the request path, and does no provider work after an admission error.
Same-source SQL updates serialize naturally at the one counter row. This is a
source-based hot-path wait audit; no benchmark was requested or run.

Skipped new fault frameworks, duplicate route/tenant/IPv6 test matrices and
unrelated platform/setup work. Findings above are source-backed discovery;
the separate validator must check them before retention.
