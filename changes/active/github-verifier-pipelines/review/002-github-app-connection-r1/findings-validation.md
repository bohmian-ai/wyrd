# Independent Task 002 findings validation

Reviewed immutable base `395274d58bf76a2075f245ca3a5c1bd044925027` and candidate `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`. This is the first review. The human-approved revision 4 packet amendment at `46af20f00` governs the dedicated table and shared login-state design. The recommendations below use the approved specification and contract, the App design supplement, and the owning repository rules. No prior finding or matrix row carries forward.

**Recommendation — SPEC_REVISION_REQUIRED.** FIND-002-16 requires a decision on the approved GitHub Enterprise Server support and the security of the verified association. FIND-002-1 through FIND-002-15 are independently actionable implementation or repository corrections within existing decisions. They use the existing provider, tenant slot, login state, screened transport and test owners.

## Validated ledger

Paths in this section use the prefixes in the behavior matrix. `service.rs`, `app.rs` and `routes.rs` are in `crates/wyrd/wyrd-server/src/components/github/`. `slots.rs` is `crates/wyrd/wyrd-sql/src/queries/github_installations.rs`. The verification references distinguish focused candidate runs from source-proven failure paths.

### FIND-002-1 — REVISED · INCORRECT · An old slot can hand out a token after committed revocation

B-001, INV-1, REP-GH-04, MNT-004, SYS-01, SEC-1, PG-1. REQ-001, AC-001, INV-006, O19/O22/O23 and Security Posture outbound credentials.

`service.rs:635–668` reads and commits a plain tenant snapshot, then returns the provider's bearer directly. The row lock in `fence_and_release` at :333–342 covers writers, not this detached decision. A can wait on an already-sent mint, disconnect can commit, B can associate the same installation, and the provider can still grant A's old request because Wyrd disconnect does not uninstall the App. A tenant-side deselection has the same failure if GitHub still allows the repository. Association refresh, repository removal, suspension and deletion are the other writers of this slot. The token facility is an explicit Task 002 output even though only the server test calls it today. Task 006 introduces the first repository-linked queued record.

Make the scoped credential handoff conditional on the same active slot revision after provider IO. Use the existing tenant slot read and revision as the final authorization decision. Discard an obsolete result. A released, replaced, suspended or deselected association refuses. If a still-authorized repository sees unrelated revision movement, a bounded fresh decision/mint can continue under the current revision. Avoid handing out an old response after an ABA and preserve unaffected access. Update the inaccurate lock contract at :325–329. The Wyrd authority boundary is releasing a credential to work. An already-sent request can finish at GitHub with its result discarded. Keeping SQL transactions out of the bounded provider wait preserves shared pool availability. Extra durable state and queue changes add no proof.

Use the existing server/mock integration to pause a mint response, commit disconnect and assert the pending caller receives no token when released. Exercise matching selection loss and an unrelated repository control. The current SRV :132–178,281–293 assertions cover only ordered calls and an unrelated pending execution.

### FIND-002-2 — REVISED · INCORRECT · Stored GitHub identity is lost at consumption

B-002, INV-2, REP-GH-03, MNT-003, SYS-03, SEC-2, PG-2. REQ-001, INV-001, O4/O6/O18/O19 and the connection contract.

`slots.rs:124–143,234–249` and the migration's unique index treat the host and installation as one identity. The callback persists that host and webhooks route by the pair (`service.rs:497–506,570–585)). View :177–198 labels any saved installation and repositories with the newly configured host/App. Mint :640–668 sends old numeric IDs to the current App's API (:369–386 in `app.rs`). Two GHES hosts can grant identical numeric pairs. A tenant could obtain B's credential on A's selection. Even absent colliding IDs the GET misattributes the verification. The same producer/consumer pattern affects an outstanding browser transaction. Begin :255–267 stores the issuer, OAuth client ID and redirect URI. The SQL bind and consume decode all three, yet setup :369–393 and callback :439–451 drive the current App with the old state. On a released or first slot a second provider can complete the prior transaction if setup and exchange use the new configuration. A callback after a replica switch can also send an old provider code and its verifier to the new endpoint before provider refusal.

Use the saved host to restrict active view/credential use. Release continues to work for an old-host slot, and the normal verified flow is the recovery path. At the existing GitHub transaction trust boundary compare the saved issuer/client/callback to current values before issuing an OAuth redirect or committing consumption and exchanging code. Use the existing typed transaction refusal and leave OIDC predicates/owner intact. These are comparisons against fields already persisted. No host management or provider discovery is required. Prove old-host view and mint refuse with overlapping mock IDs, old begin/setup and bound callback cannot drive the replacement provider, and an authorized release and verified reconnect still work.

### FIND-002-3 — CONFIRMED · INCORRECT · Startup accepts a public key as a signing key

B-003, MNT-001. REQ-001 and O3/O16. `config.rs:2439–2453` and `app.rs:148–156` both parse through `EncodingKey::from_rsa_pem`. Pinned `jsonwebtoken` 9.3.1 `src/pem/decoder.rs:172–179` accepts RSA public PEM. `src/crypto/rsa.rs:39–44` first requires a signing pair during actual signing. The test public PEM already in the tree can pass boot and fail the first App-authenticated request as provider unavailable. Malformed text at CFG :6938 only covers parse failure.

Prove the parsed key can sign with the installed RS256 implementation during typed config resolution, and give the App that validated key. This removes duplicate parsing and avoids implementing a parser or trusting the PEM label. Exercise the existing public half as a typed field-specific refusal, alongside the valid private fixture and signed request. Keep PEM material out of errors and Debug.

### FIND-002-4 — CONFIRMED · INCORRECT · OAuth web exchange negotiates the wrong JSON media type

B-004, SEC-3. REQ-001, AC-001 and O7/O8/O10. Both browser flows converge on the web token exchange in `app.rs:221–241`, which uses the REST vendor media header from :438–448 and only decodes JSON. GitHub documents form encoding by default and `Accept: application/json` for JSON. Its GitHub App example uses that value. The discovery probe confirmed a difference on invalid synthetic credentials, which supports negotiation behavior but supplies no successful live authorization. The mock :531–555 ignores Accept, so its JSON success masks the defect. State is already consumed before this request. [GitHub App web exchange example](https://docs.github.com/en/apps/creating-github-apps/writing-code-for-a-github-app/building-a-login-with-github-button-with-a-github-app) · [GitHub OAuth response formats](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)

Negotiate documented JSON on the web exchange using the same screened client and keep REST media/version headers on the REST API. Require the expected Accept header in the existing mock and drive the callback through it. A new decoder or provider library would add machinery to a standard negotiation.

### FIND-002-5 — REVISED · INCORRECT · Out-of-order suspension events can strand active access

INV-3, SYS-02, SEC-4 (suspension portion). AC-001, O21/O22 and the connection contract. `service.rs:587–595` assigns suspension from the arriving action and `slots.rs:69–78` skips equal writes. If a newer unsuspend arrives while Wyrd's slot is still active, the older suspend later fences every repository indefinitely. GET and mint read the wrong state and ordinary retries never check the provider. The existing verified refresh preserves `suspended_at` and cannot recover this sequence. Ordered delivery tests do not expose it. [GitHub webhook ordering](https://docs.github.com/en/webhooks/testing-and-troubleshooting-webhooks/troubleshooting-webhooks#webhooks-deliveries-are-out-of-order)

Use the existing App installation lookup to observe current suspension on either signed transition. Read the exact routed slot and revision, release the tenant transaction before provider IO, and apply through the existing suspension owner conditional on that revision. A stale observation must be discarded and refreshed against the still-matching association, with bounded contention and existing typed failure on exhaustion. Advance the private fence for a successful observation even when the boolean agrees, preserving public timestamps for an unchanged state. Otherwise an old suspended response can overwrite a newer active observation whose no-op never fenced it. A provider failure or absent App installation supplies no observation. Keep ordinary deletion/release separate, retain ownership during real suspension and preserve the existing request-scoped failure/redelivery behavior. Prove reverse deliveries and a paused older observation overtaken by a newer matching observation, with the final view and credential decision matching current provider state. No new delivery store is warranted.

### FIND-002-6 — CONFIRMED · INCORRECT · A routed removal can miss a concurrent verified selection

PG-3. O10/O21/O22 and AC-001. `service.rs:494–508` takes the provider selection then conditionally commits against the revision captured at begin. When the old slot selects A and the callback has read A+B, a signed removal of B routes correctly to that slot at :572–603 but `slots.rs:81–96` only advances revision on overlap with the stored selection. The service commits and acknowledges the no-op at :609, allowing the callback to reintroduce B. Current provider denial at mint cannot repair the durable view. SQL :482–488 explicitly asserts the incorrect no-write behavior for an unselected repository. This is distinct from stale suspension because subtraction itself is correct and the loss is in the missing callback fence.

Advance the existing private revision on a nonempty signed removal for the exact associated host and installation even without old overlap. Continue subtracting only named repositories, preserving unrelated selection and avoiding artificial public timestamp changes when selection is unchanged. Prove with real SQL: capture the binding for A, apply removal of B, then attempt to write A+B using the old binding. Assert stale and A remains selected. If the callback commits first, ordinary subtraction removes B. An unassociated installation has no route by the approved lookup contract. This fix uses the existing slot and adds no pending claim.

### FIND-002-7 — CONFIRMED · INCORRECT · Production screening rejects a configured private GHES

SYS-04. O4 and the approved App design's configured GitHub Enterprise Server support. At `boot/mod.rs:1320–1329` the App inherits the identity-provider policy. `config.rs:137–150` and `wyrd-auth-oidc/src/screening.rs:174–190,214–218,241–244,322–328` block all private-range addresses in production. A reachable, TLS-valid operator-configured GHES on private networking can boot but fails every exchange. Current custom-host tests parse config or use development. The Security Posture rule for user- or tenant-supplied URLs does not own operator App origins, while its GitHub-specific requirement still owns screened HTTP.

Select the existing internal-capable screened policy for deployment-supplied App origins, with production HTTPS enforced by GitHub configuration and metadata/link-local denial, pinning, TLS, redirects, proxies and bounds preserved by the screened transport. Leave tenant-supplied provider policy with its existing owner. Test through the actual GitHub policy selection with a production HTTPS private address and a blocked metadata destination. This needs no new network policy type or knob.

### FIND-002-8 — CONFIRMED · VIOLATION · Setup gives a browser binding credential without no-store

SEC-5. Security Posture :415–418 applies to this credential-bearing response. `routes.rs:189–207` sends the raw HttpOnly binding in Set-Cookie and matching state in Location with no cache directive. The router does not add one. The cookie flags restrict browser use but do not instruct caches. The binding is consumed as a credential by the matching principal at :256–267. [RFC 9111 §7.3](https://www.rfc-editor.org/rfc/rfc9111.html#section-7.3)

Use the existing response owner `auth/oauth.rs:140–151`, as `auth/browser.rs:172–184` does. Assert the served setup response's no-store directive, binding cookie and redirect together. The connect receipt carries a state that still needs binding and current authority. The successful callback only clears the binding cookie. Neither supplies the setup credential pair.

### FIND-002-9 — CONFIRMED · DRIFT · Local HMAC duplicates an installed primitive

MNT-002. The root repository reuse ladder applies to the cryptographic owner. `crates/shared/wyrd-crypt/src/lib.rs:449–480` implements key normalization, SHA-256 pad expansion and tag comparison. The only production consumer is the App verifier at `app.rs:198–211`. The fixture signs using the same primitive. `Cargo.lock:3821–3828` already resolves RustCrypto `hmac` 0.12.1 with the same digest generation as the owner. Existing RFC vectors show the present code works for its exercised inputs. They do not earn a local cryptographic implementation.

Keep the shared crypto owner and implement the existing signing/verification functions with the resolved HMAC implementation and constant-time full-tag verification. Remove the new direct subtle edge if unused afterwards. No new resolved crypto package or external App dependency is needed. Keep the independent RFC 4231 short/long key and wrong/truncated tag assertions and signed webhook proof.

### FIND-002-10 — CONFIRMED · VIOLATION · Changed Rust items lack required or accurate documentation

REP-GH-01, MNT-005. Root AGENTS and the Rust guide explicitly gate changed items. Spot checks across the complete standards inventory establish undocumented module declarations (`wyrd-client/src/lib.rs:22`, `wyrd-spec/src/lib.rs:19`, `components/github/mod.rs:10–12`), associated type `login_state.rs:372`, test constant SQL :33 and missing panic descriptions in CLI :21–56, SRV :24–40, CFG :6790 and RJ :24–122. `slots.rs:169–171` mislabels the result as committed. The SQL helper accepts a tenant transaction and cannot commit. Only its caller at `service.rs:531` commits. The callback owner at :398–417 omits that failure or cancellation after consumption requires fresh authorization.

Batch the concrete missing documentation with behavior corrections. Use the full REP-GH-01 inventory and check other changed test declarations while moving their markers. State the caller's commit obligation, spend-on-failure boundary, and actual lock/credential ordering. Prove by changed-item documentation inspection and the narrow format/lint gates. The correction adds no behavior or style-only round.

### FIND-002-11 — CONFIRMED · VIOLATION · Selected SQL tests report green without PostgreSQL

REP-GH-02, PG-4. `wyrd-sql/AGENTS.md:72–77` forbids silently skipping a PostgreSQL test. All five SQL tests use a local env guard at :35–38 and return before `PgFixture::start` at :129–132,213–216,298–301,400–403,536–539. The independently passing run used real Postgres. The defect allows future miswired invocations to succeed without assertions.

Delete the guard and use the existing fixture's failure path. Run the same exact tests with the repository Postgres lifecycle and a representative direct selection without the environment, asserting visible failure rather than success.

### FIND-002-12 — CONFIRMED · VIOLATION · Real-server journey placement bypasses required source markers

REP-GH-05. Root AGENTS and the Rust guide require `mod pg_tests` or `pg_*` file placement for tests with a server or Postgres. The CLI real bound-server case at CLI :69–86 and the four SDK cases at RJ :147–149,266–268,458–460,545–547 are top-level in files without the marker. MCP :10–11 already uses the required module. Test lane selection is distinct from source placement under TESTING :225–228.

Mark the two existing modules or files and update their actual test registration/selectors in `tests/cli.rs`, the SDK integration target and `mise.toml:186–194` where applicable. Keep current real-server provisioning and existing lane inclusion, and run the exact cases. The correction needs no new binary, ignore flag, or suite.

### FIND-002-13 — CONFIRMED · VIOLATION · The published setup instructions are invalid shell and lack connected proof

REP-GH-06. O30, AC-011 connection portion and `docs/CONTRIBUTING.md:20–25`. The published guide at DOC :48–55 and :64–67 contains bare angle-bracket placeholder assignments that the shell parses as redirects. CLI :19–30,88–123 runs the verbs with environment injected in Rust and does not run the actual setup snippets. The range adds no corresponding `examples/` source. O26's published permissions and served paths are correct.

Use valid substitutable shell values and source the setup and tenant commands from a minimal local example under the existing docs convention. Drive that example's setup and CLI path with existing App credentials supplied by the local mock and the real-server CLI journey, then check the docs and generated outputs through their owners. Testing only the verbs leaves the observed shell/setup defect untouched.

### FIND-002-14 — CONFIRMED · VIOLATION · Changes to clear failed tests lack independent diagnoses

B-005. The implementation skill's failure-diagnosis rule and task review's explicit evidence gate apply. Task evidence :245–253 records the Card fixture baseline, DELETE auth header and MCP catalog corrections after failures. It supplies implementer diagnoses but no independent diagnostician report or reference. The changed assertions remain substantive. Source confirms that the Card fixture creates a root Service Card, the test helper uses the Wyrd access token header and the MCP catalog owner advertises the listed verifier operations. Those checks do not establish the required independent failure evidence.

Recover contemporaneous independent reports if they exist. Otherwise record a fresh independent read-only diagnosis from the available failure records and committed diff, identifying any unavailable original traces as a historical limit. The before-edit sequencing cannot be recreated. Preserve the corrected substantive assertions and attach the evidence to the task. No new runtime code or broad test lane is needed.

### FIND-002-15 — REVISED · VIOLATION · The proof record lacks some required exact named commands

REP-GH-07. Root AGENTS, TESTING :91–111 and spec-driven development :374–381 require exact executable selectors for named proof. Independent verification closes the original HMAC/OpenAPI examples and the TypeScript format, lint and type gates. It also adds passing exact real-provider OIDC evidence for the behavior. The task still explicitly names checks such as the four config cases, the SQL binding and claim assertions, the server token integration, the Rust SDK scenarios, the CLI case and the in-process OIDC session/audit assertion. The command block uses regexes, abbreviations and lane summaries. Those runs are affirmative current behavioral evidence and do not substitute for the explicit named-command rule. Record and run only remaining exact named selectors under the existing wrappers, grouped where they share a target. For cases amended by the remediation, use their final selectors and retain the historical evidence separately. No broad suite is required for this evidence issue. The disclosed initial absence of RED remains historical. Focused remedial RED/GREEN evidence applies to the new failure scenarios, without rewriting the historical record.

### FIND-002-16 — REVISED · INCORRECT · GitHub Enterprise Server does not supply the required authorization-code binding

Independent discovery separately validated by [focused PKCE review](followup-pkce-validation.md). REQ-001, AC-001, INV-001, O4/O6/O10, and the approved App design :99–102 require conventional configured host support, verified association and the actor/tenant/browser boundary. The candidate adds a claim that all App callbacks redeem with PKCE at Security Posture :508–511. The approved behavior does not prescribe PKCE as the implementation.

`service.rs:255–277,369–394` generates a per-transaction verifier and uses it for both entry paths. `app.rs:185–195,221–247` always sends the challenge and verifier. The callback `service.rs:424–451` validates the Wyrd principal, state and browser, then exchanges the supplied code. Its resulting access token supplies the user authority for App ownership, repository proof and the association commit at :452–531. GitHub's official documentation gates the App and OAuth PKCE feature to GitHub.com and Enterprise Cloud. Its documented Enterprise Server App grant has no equivalent transaction-binding value. The mock enforces PKCE, so the journey supplies no proof of this Enterprise Server property. [GitHub PKCE feature definition](https://github.com/github/docs/blob/main/data/features/pkce_support.yml#L1-L5) · [GitHub App flow source](https://github.com/github/docs/blob/main/content/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app.md) · [GitHub Enterprise Server App grant](https://docs.github.com/en/enterprise-server@3.22/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app#using-the-web-application-flow-to-generate-a-user-access-token)

Under the documented non-PKCE protocol, the OAuth server can exchange a stolen, unused code issued for the same App/client and fixed redirect. A connection writer can substitute it into their own valid GitHub-begun Wyrd transaction and retain their own state and browser binding. The token then proves the other user's installation and repository access. An eligible unclaimed installation that matches the hint or is uniquely accessible can be associated to the writer's tenant. Exact callback URLs and the client secret authenticate the deployment and its request. Neither binds the provider code to the browser transaction. This is the confidential-client injection path in RFC 9700. It needs possession of the unused same-client code. The evidence establishes the provider contract gap and protocol-level consequence, with no live assertion about how an appliance responds to extra parameters. [RFC 6749 §§3.1–3.2](https://www.rfc-editor.org/rfc/rfc6749.html#section-3) · [RFC 9700 §§2.1.1 and 4.5](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.5)

The owning decision is whether to require provider support for binding the code to its original transaction, narrowing the approved host scope where necessary, or preserve the promised Enterprise Server support with a provider-backed association protocol that demonstrates equivalent verified authority. Neither a silent PKCE downgrade nor a host exclusion is an implementation-owned choice. The documented App grant has no OpenID Connect nonce proof, and the registered exact callback does not admit a transaction-specific redirect URI as a substitute. A documentation-only change cannot establish authorization. Once the contract is decided, require proof of successful legitimate association and refusal of another transaction's same-client authorization code for every supported provider capability.

## Proposal dispositions

| Proposal IDs | Disposition |
|---|---|
| B-001, INV-1, REP-GH-04, MNT-004, SYS-01, SEC-1, PG-1 | Revised as FIND-002-1. Credential handoff is the decision boundary. Existing queue behavior passes its current scope. |
| B-002, INV-2, REP-GH-03, MNT-003, SYS-03, SEC-2, PG-2 | Revised and consolidated as FIND-002-2. Existing transaction configuration fields also govern the staged browser steps. |
| B-003, MNT-001 | Confirmed as FIND-002-3. |
| B-004, SEC-3 | Confirmed as FIND-002-4. |
| INV-3, SYS-02, SEC-4 | Revised as FIND-002-5 for suspension. Rejected SEC-4's separate delayed-removal-after-completed-reselection correction. Signed repository removal is an access-loss fence applied to existing selection when routed. The approved contract has no event/version ordering between a loss, possible provider re-addition and a completed later selection. Keeping an ID because GitHub currently allows it would conflate current provider access with the tenant selection lifecycle. Reconstruction needs additional authoritative history and precedence. The proven removal-before-in-flight-publication failure is FIND-002-6. Rejected additional mandatory clearing during callback refresh as unnecessary once signed suspension uses the authoritative provider read. |
| PG-3 | Confirmed as FIND-002-6 for an already routed slot. A first/released slot has no route under the approved definer boundary. A second provider read only moves that interval. No new pending route, provider history or cleanup decision is required by this task. |
| SYS-04 | Confirmed as FIND-002-7. The tenant-supplied URL rule is distinct from the deployment-controlled App. |
| SEC-5 | Confirmed as FIND-002-8 for the credential-bearing setup response. |
| MNT-002 | Confirmed as FIND-002-9. |
| REP-GH-01, MNT-005 | Confirmed and grouped as FIND-002-10. |
| REP-GH-02, PG-4 | Confirmed as FIND-002-11. Independent real-Postgres proof remains valid. |
| REP-GH-05 | Confirmed as FIND-002-12. |
| REP-GH-06 | Confirmed as FIND-002-13. Connected CLI verbs prove only part of AC-011. |
| B-005 | Confirmed as FIND-002-14. The absence of original RED is a separate historical exception. |
| REP-GH-07 | Revised as FIND-002-15, limited to remaining exact named proof. TypeScript gates and the exact examples from the original proposal are independently closed. |
| Independently discovered GHES PKCE gap | Revised as FIND-002-16 after separate focused source validation. No claim that a live appliance rejects the extra parameters. |

## Specialist applicability and coverage

| Role | Independently validated applicability and coverage |
|---|---|
| Behavior | Required for the first review. It traces operator config, both browser flows, typed projections and connected assertions. Its matrix covers O1–O35 including O15a–f and adds O36 for non-goals and O37 for test correction evidence. Reconciled below. |
| Invariants | Required. Tenant/actor/browser transaction identity, host-scoped slot authority, revision writers and token handoff cross the browser, SQL and provider owners. Its report traces all three consumers, cross-flow state and the shared queue seam. |
| Repository standards | Required. The diff spans SQL, pure Rust contracts, auth, runtime, test runners, generated declarations, both native SDKs and docs. The report maps the root and four touched nested authorities, directly checks SQL capability signatures and covers test source layout and rustdoc. |
| Maintainer | Required. The new App and connection service own separate provider and tenant policies. The report compares the existing screened transport, browser/OIDC owners, crypto and fixture owners. App protocol requires a dedicated owner because OIDC discovery, nonce and trust do not fit an App installation. The service's durable association outcome differs from login. Reusing tenant transactions, login-state predicates and response/permission owners is sufficient for the common primitives. No extra loader or generic workflow is justified. |
| System resilience | Required. Boot, split-replica callbacks, dependency outages, provider waits, local release and webhook failure change runtime behavior. The report confirms optional boot isolation, bounds and commit/restart recovery and identifies outage/retry behavior without depending on live GitHub. |
| Security domain | Required. Authenticated principals, App and user credentials, state binding, egress screening, webhook signatures and cache policy are materially affected. The report covers normal permission and audit paths, token scope, signature-before-lookup and OIDC isolation. |
| PostgreSQL domain | Required. A new persisted authority, inverse uniqueness, forced RLS, conditional writes, one-use state and a definer route span tenant isolation, concurrency and persistent data. The report checks the actual migration, SQL capability, restoration and both claim races. This coherent SQL boundary covers the concurrency and tenancy specialties. |
| Focused follow-up | Required for the disputed webhook overlap. The report traced first/released no-route behavior, routed removal before conditional publication, and suspension's stale action/observation. It narrows the correction to existing routable state. |
| Focused PKCE validation | Required before retaining the new material proposal. A fresh reviewer established the GitHub App-specific documented feature boundary, verified the protocol and traced the resulting authority into the existing callback. |
| Omitted domains | No Bifrost storage mutation, model execution or UI diff. Existing Scribe permission audit remains staging-only and the unchanged generic queue does not take GitHub identity. Behavior/system/security trace those boundary handoffs. No prior carry-forward is assumed. |

## Reconciled acceptance matrix

Abbreviations are as in the task and behavior review. These dispositions update each behavior row and combine only rows with common evidence. Positive tests assert specific behavior under the mock or repository infrastructure. A PASS may coexist with an adjacent mandatory repo correction.

| Obligation | Candidate evidence and verification | Disposition |
|---|---|---|
| O1 | CFG :6819–6842 and :7006–7044 validate env, file, TOML and redaction. Four focused config cases passed. | PASS |
| O2 | RJ :559–573 refuses GitHub without config and successfully reads Cards. PY and TS also assert typed unavailability. | PASS |
| O3, O16 | CFG tests cover partial and malformed config but public PEM is parseable and boot does not sign. | FAIL FIND-002-3 |
| O4 | CFG custom/default origin and RJ host evidence cover unchanged mock config. Saved identity, production private address selection and the authorization capability for supported Enterprise Server hosts fail. | FAIL FIND-002-2, FIND-002-7, FIND-002-16 |
| O5 | Served OpenAPI asserts five paths, typed schemas and webhook security. SRV :218–238 exercises the tenant API. | PASS |
| O6 | RJ :164–177 and SQL :607–638,683 cover committed receipt, TTL, browser, actor and tenant. Current setup and consume omit the saved provider identity check. Enterprise Server cannot bind its authorization code to that state under the documented App grant. | FAIL FIND-002-2, FIND-002-16 |
| O7, O8 | RJ :164–199,227–238 completes both local paths. Both call the exchange whose mock always returns JSON. | FAIL FIND-002-4 |
| O9 | RJ :174–177 observes not connected after setup binds the hint. | PASS |
| O10 | RJ :193–199,357,498 checks provider installation, repository intersection and negative cases. OAuth negotiation, concurrent acknowledged removal and Enterprise Server code authority fail. | FAIL FIND-002-4, FIND-002-6, FIND-002-16 |
| O11–O14 | RJ :209–214,298,316,368 and SQL :616–654 show spoof refusal, consumption, browser/actor and tenant failures. PY :118 and TS :116 also reject a foreign principal. | PASS |
| O15, O15a | Migration enforces both unique constraints. SQL :317–381 commits exactly one winner under both contention shapes. RJ :404–411,439 tests conflict and same-installation refresh. | PASS |
| O15b–O15d | SQL :560–574,661 and RJ :386–398 show separate OIDC/GitHub consume. SQL :183,195 and RJ :427 show old callback refusal after release/reconnect. SQL :425,450 and :236 establish force-RLS flags and row isolation. | PASS |
| O15e | OIDC trust table and predicates retain their behavior. The exact real-provider runs prove tenant login, refusal and candidate connection test sign-in. The in-process browser callback asserts session creation and audit. | PASS |
| O15f | SQL :248–269 and RJ :492–493 establish genuine suspension keeps the unique claim. Wrong ordering of suspension is covered in O21. | PASS |
| O17 | CFG redaction, SRV :199–201 durable-row check and RJ :240–250 protect key/token disclosure. Setup's separate binding credential cache rule fails. | PASS for the promised key/token redaction. Repo gate FAIL FIND-002-8. |
| O18 | The typed view exposes only the approved shape, but can project the wrong host's association. | FAIL FIND-002-2 |
| O19 | SRV :132–178 asserts scope and sequential refusal. Old host and post-provider stale decisions remain reachable. | FAIL FIND-002-1, FIND-002-2 |
| O20 | SRV :133–143,296 asserts one repository and exact three permissions. :201 verifies no stored token. Callback drops the user token at service :495. | PASS for scope/transience. Authority is O19. |
| O21 | Bad signature is rejected before SQL, and RJ :478–536 proves ordered lifecycle updates. Reordering and the routed removal's callback race fail. | FAIL FIND-002-5, FIND-002-6 |
| O22, O23 | SRV :161–178,281 and RJ :444 prove sequential refusals and claim reuse. A concurrent mint can survive committed access loss. | FAIL FIND-002-1, FIND-002-5, FIND-002-6 as applicable. |
| O24 | SRV :283–293 proves ordinary pending work survives. No current queued execution takes GitHub repository identity. Task 006 adds that input to the existing owner. | PASS for the current queue scope. Source-IO failure is O22/O23. |
| O25 | RJ :253–255 preserves Card count and SRV :203 asserts zero bindings. | PASS |
| O26–O28 | Guide specifies configured permissions/events and served URLs. Required architecture sections exist. SRV :271–279 proves allowed/denied audit decisions and the writer permission is shared with identity connections. | PASS for these obligations. Implementation deficiencies and setup proof are separate. |
| O29 | Shared Rust handle and thin projections, served HTTP plus Rust/Python/TS, CLI and MCP journey assertions. MCP :95,166–185 proves discovery and scoped writes. TypeScript required gates passed. | PASS for surfaces. Repo gate FAIL FIND-002-12 for test placement. |
| O30 | CLI journey proves verbs and typed JSON through the server. Published setup shell snippets and local example proof are absent/invalid. | FAIL FIND-002-13 |
| O31 | SQL conditional write assertions pass under the managed database. The test-local env guard can bypass them, and prior no-overlap proof expects the unsafe removal behavior. | FAIL FIND-002-6, FIND-002-11 |
| O32–O33 | Migration adds one table with forced RLS and separated OIDC state. SQL :436–479 proves pinned search path, narrow resolution, restoration and no ordinary cross-tenant read. | PASS |
| O34 | Begin, association and disconnect responses follow commit. RJ :193–199,225,444 observes durable committed state. The missing credential fence is captured by O23. | PASS for commit ordering. |
| O35 | App signing, webhooks and tenant policy have the approved owner, with no GitHub library or old pin. Installed HMAC reuse remains an explicit repository issue. | PASS for ownership. Repo gate FAIL FIND-002-9. |
| O36 | Closed request shape, one new table, no UI, Cards/bindings, PR admission or source mutation, no new permission category or operator credentials supplied by tenants. | PASS |
| O37 | Corrected fixture/test assertions have no independent diagnosis report in the immutable record. | FAIL FIND-002-14 |
| Repository proof and rustdoc | Changed-item docs are explicitly mandatory. Passing narrowed commands, TypeScript gates and OIDC proof close their observed coverage. Exact named proof still needs executable selectors as described above. | FAIL FIND-002-10, FIND-002-15 |

## Verification boundaries

The candidate HEAD and committed source were unchanged for validation. See [verification.md](verification.md) for the independent gate results and exact fixture-backed OIDC checks. The task's evidence supplies Python and TypeScript journey execution and remaining broader gates. No live GitHub App credentials were used. Positive local runs cannot establish the specific provider semantics or controlled interleavings described in the ledger. Historical missing RED remains documented. Corrected behavior requires focused evidence against each failure, with unrelated already-proved behavior carried forward.
