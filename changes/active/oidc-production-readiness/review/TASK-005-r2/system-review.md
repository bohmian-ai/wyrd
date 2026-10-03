# TASK-005 system-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior verdict: `changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The candidate was at the stated commit before this report was written. The
repository has no `.codegraph/` directory, so this review used the cumulative
Git diff, direct source inspection, and existing tests. The cumulative
base-to-candidate diff changes architecture and operator documentation,
generated documentation and its prose generator, Rust/TypeScript/Python
documentation, and review evidence. The changed Rust and TypeScript lines are
comments or rustdoc only; the Python generator changes only emitted Markdown.
No serving handler, process lifecycle, persistence path, retry policy, timeout,
health check, deployment manifest, or executable authentication behavior is
changed. The remediation therefore has no runtime or deployment effect of its
own.

## Deployed-path evidence

| Deployed path | Runtime owner and topology | Documentation assessment |
| --- | --- | --- |
| Candidate test and activation | `HumanConnections` owns the tenant connection lifecycle and persists the exact-revision test stamp for 15 minutes (`crates/wyrd/wyrd-auth/src/connections.rs:67-68,428-506`). `activate` takes the tenant slot lock, checks the persisted stamp and recovery API key, retires the old Active connection, and promotes the Candidate in one transaction, with no provider call (`connections.rs:509-584`). All serving replicas read the durable Active connection rather than holding a deployment-local activation state. | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:111-127` now accurately separates the provider-backed test from provider-I/O-free activation. It says an outage blocks a new test and later login but is not re-probed while the 15-minute stamp is current, and tells operators to confirm health before replacing a working connection. This closes prior `SYS-001` / `FIND-TASK-005-6` without adding a probe, retry, or availability mechanism. |
| Browser session across BFF replicas | `BrowserSessions` encrypts the refresh token or recovery API key into a Secure, HttpOnly, SameSite=Lax cookie using a key derived from the deployment web-app client secret (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:98-125,172-187,249-261`). Any replica can decrypt it; the per-replica access-token cache is optional and a miss renews through the token endpoint (`browser-sessions.ts:293-334`). The two-replica integration journey deliberately begins and completes on different replicas and asserts an encrypted session cookie (`production-auth.integration.test.ts:156-228`). | `docs/src/content/docs/self-hosting/authentication.svx:11` now distinguishes the stateless API request boundary from the BFF-owned encrypted cookie and correctly states that rotating the common BFF secret invalidates cookies. `sso-and-oidc.svx:137` preserves the 12-hour cookie-session boundary and does not promise server-side browser-session durability. |
| Logout and interrupted revocation | Logout deletes the browser cookie and local access-token cache before attempting RFC 7009 revocation; only refresh-token sessions are revoked, and failure is logged without token material and does not fail logout (`browser-sessions.ts:336-357`). The already-issued self-contained access token is unaffected until its bounded expiry. | `authentication.svx:11` and `sso-and-oidc.svx:137` accurately describe best-effort revocation, no IdP logout, and access-token validity through expiry. They do not claim that a revocation dependency outage preserves the browser cookie or instantly withdraws an access token. |
| OAuth request and response surfaces | The tenant router owns `/auth/token` and its five tenant grants (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:43-189`); the separately mounted platform router owns only `/auth/platform/token` (`components/platform/routes.rs:54-142`). Device authorization returns its RFC 8628 object and revocation returns an empty no-store `200` (`auth/cli_login.rs:43-90,265-307`). `OAuthError` gives protocol-native refusals their RFC body and no-store headers; only conversion from `WyrdError` logs a Wyrd catalog code (`auth/oauth.rs:1-14,105-164`). | `authentication.svx:21-31` now preserves the tenant/platform plane split. `sso-and-oidc.svx:139-152`, `architecture/wyrd-security-posture.md:199-221`, and the OAuth module rustdoc distinguish token, device-authorization, and revocation success shapes and accurately scope Wyrd-code logging. During a store or upstream outage, the documented protocol response remains endpoint-native rather than suggesting a Problem Details fallback. |
| Workload trusted-issuer boot and runtime administration | Boot refuses any Human trusted issuer before provider or database I/O, fails a never-seeded unreachable workload issuer closed, but retains an existing durable row when discovery is temporarily unavailable (`crates/wyrd/wyrd-server/src/boot/issuer.rs:90-137,140-175`). Runtime create likewise refuses Human before discovery and seals a secret-bearing workload issuer before insert (`components/admin/routes.rs:297-349`). `issuer_write_from_trusted` requires the sealing key only for `SecretBasic`/`SecretPost`; `Public` and `PrivateKeyJwt` carry no shared secret (`crates/wyrd/wyrd-auth/src/pg_resolvers.rs:673-706`). Boot's rewrap owner refuses keyless startup when persisted provider or workload-issuer ciphertext exists (`crates/wyrd/wyrd-server/src/boot/mod.rs:1520-1549`). | `docs/src/content/docs/concepts/cloud-identity.svx:11-22,73-121` now presents this surface as workload-only and states the actual secret-bearing failure boundary. `docs/src/content/docs/self-hosting/configuration.svx:78-84` accurately makes the sealing key conditional on a stored shared secret. The wording does not turn a transient outage for an already-seeded issuer into a restart-wide outage or imply that Human federation can fall back to this store. |

## Failure and recovery assessment

| Failure or interruption | What stops | What remains available and state that survives | Recovery and amplification assessment |
| --- | --- | --- | --- |
| Provider fails before or during a candidate test | The new test cannot complete, so no fresh stamp is written. | The prior Active connection and its durable state remain unchanged. A prior stamp on the exact Candidate remains usable only until its database-clock expiry. | Restore the provider and complete a new real sign-in when needed. Activation performs no retry or liveness check, so it cannot amplify the outage, but operators must not treat a current stamp as fresh health evidence; the corrected guide says exactly that. |
| Provider fails after a successful test but before activation | New sign-ins and a new test are unavailable. | The exact Candidate stamp remains current for up to 15 minutes; the old Active connection remains live until activation commits. Headless recovery remains independent. | Activation can still atomically replace the old connection. The guide explicitly warns operators to confirm provider health before doing so, accurately exposing the availability tradeoff without changing approved behavior. |
| Server process crashes during activation | The in-flight request fails. | PostgreSQL transaction atomicity leaves either the old Active connection or the fully promoted Candidate; it does not leave the old connection retired without promotion. The durable stamp and connection rows survive process replacement. | Retry from a replica after recovery using current revision/state. There is no deployment-local lease or health loop to amplify the crash. |
| BFF replica restarts or a request lands on another replica | Only the in-memory access-token cache is lost or missed. | The encrypted browser cookie survives and any replica with the same deployment client secret can decrypt it. The API server and unrelated tenants remain available. | The receiving replica renews through Wyrd. If Wyrd is unavailable, that browser request fails upstream; discovery failures are evicted from the BFF configuration cache and retried on a later request (`browser-sessions.ts:128-163`). |
| Revocation endpoint is unavailable during logout | Refresh-token revocation may not complete. | The BFF clears the cookie and local cached access token first. Other services, tenants, and API operations remain available; an already-issued access token remains valid only until expiry. | The user is locally logged out and the warning contains no token. There is no retry storm or process-fatal path; this is the approved best-effort boundary reflected in the docs. |
| Workload IdP discovery is unavailable at boot | A never-seeded issuer blocks boot; an already-seeded issuer is retained with a warning. | Existing durable issuer state survives a transient provider outage. Human login trust remains in its separate connection store. | Restoring discovery permits a later seed/update. The docs no longer route Human setup through this boot surface or omit the sealing-key prerequisite for secret-bearing issuers. |
| Sealing key is absent while ciphertext exists | Authentication installation fails before serving with that unreadable durable trust state. | No partially usable process advertises readiness; secretless deployments remain valid. | Restore a key capable of opening the stored ciphertext and restart. The documentation scopes this fail-closed behavior to stored provider/workload-issuer secrets rather than a nonexistent browser-session ciphertext store. |

No changed documentation adds a retry, health probe, or dependency fallback that
would amplify these failures. The corrected boundaries keep refusal local to
the affected request or fail boot only where durable secret integrity cannot be
established; they do not instruct operators to crash the shared server for a
request-local provider failure or to substitute another tenant or platform
issuer.

## Affected capabilities and proof

- Human connection lifecycle and recovery: source inspection of the complete
  stamp, activation, transaction, and serving-replica paths establishes that
  the corrected activation text matches the deployed behavior.
- Web UI continuity: source and the existing two-replica integration journey
  establish cookie portability, cache-loss recovery, and logout's best-effort
  boundary. This documentation-only remediation did not require rerunning the
  journey.
- Tenant and platform OAuth availability: the distinct router owners and
  endpoint return sites establish the documented plane split and wire shapes.
- Workload federation and boot: boot seeding, admin creation, sealing, and
  rewrap owners establish the documented fail-closed and restart behavior.

The supplied focused verification passed with exit 0:

- `mise run docs:check`
- `mise run codegen:check`
- `mise run fmt`
- `mise run lints`
- `git diff --check`

Those lanes establish documentation rendering, generated-output parity,
formatting, lint cleanliness, and whitespace integrity. They do not by
themselves prove outage semantics; the source and caller traces above provide
that proof. Per the task and standing direction, no full journey suite or broad
aggregate was run or required for this documentation/rustdoc-only write set.

## Material proposed findings

None. The prior system-resilience finding `SYS-001`, retained as
`FIND-TASK-005-6`, is closed by source-backed documentation. No new behavioral,
availability, recovery, security, tenancy, durability, or public-contract
defect was found. Placement, wording, and structure preferences were not
treated as findings.

## Overall result

**PASS**
