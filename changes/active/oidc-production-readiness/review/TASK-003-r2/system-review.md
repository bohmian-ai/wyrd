# TASK-003 r2 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved authority: `SPEC-oidc-production-readiness`, revision 5, from commit `d9a098b5`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human direction: `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`; this replaces the prior `FIND-TASK-003-1` diagnosis and `R1-AC-01`
- Cumulative range reviewed: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49..05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Remediation locator inspected: `4e0ca8d2e..05ff68fb47572e7d8e5fa34037042559bcfeac83`

The candidate object remained `05ff68fb47572e7d8e5fa34037042559bcfeac83` throughout this review.

## Deployed-path coverage

| Runtime path | Deployment and lifecycle ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Tenant login start and callback | Browser -> either SvelteKit BFF replica -> Wyrd login route -> tenant connection in Postgres -> IdP authorization/token endpoints -> common Wyrd callback -> fixed BFF completion route | `server-sessions.ts:105-169`; `callback.rs:104-178`; the login state is committed consumed before discovery or token IO. Provider outage, timeout, issuer refusal, or invalid token spends that flow and requires a fresh login, but creates no completion or browser session. The human-directed conditional RFC 9207 rule runs before token IO. | Fail-closed at the login request boundary. An IdP outage stops new SSO login; it does not affect existing browser-session renewal, OIDC-off API-key exchange, SDK traffic, or machine authentication. |
| Session completion and ordinary use | Either BFF -> authenticated private `/internal/bff/v1/*` route -> `BrowserSessions` -> tenant RLS transaction -> Postgres; authority then returns only to the BFF for one normal Wyrd API call | `bff.rs:47-91,188-337`; `browser_sessions.rs:203-426,449-579`; `server-sessions.ts:76-340`. A BFF has no process-local production authority. A BFF or Wyrd restart loses no committed session. A completion crash before commit leaves the completion redeemable; an uncertain response after commit can orphan a bounded session but cannot replay or disclose it. Postgres/Wyrd outage produces an upstream refusal and the BFF does not fall back to mock or another tenant. | Cross-replica state and recovery are sound. The BFF keeps the cookie on non-`401` upstream failures, so a later request can recover after the dependency returns. |
| Concurrent refresh, logout, and connection lifecycle | `BrowserSessions::current` locks the session row; renewal then uses the existing refresh-family/API-key issuance and connection-slot owners; logout locks the same row; activation/deactivation serializes on the connection slot | `browser_sessions.rs:402-579`; `browser_sessions.rs` SQL `FOR UPDATE` and rotate/revoke statements; existing refresh and issuance owners. The combined credential rotation and browser-row update remain in the caller-owned tenant transaction. Cancellation or process loss before commit rolls back both; a competing replica re-reads the winner after the lock. A refused renewal revokes the session in the same transaction. | No split refresh/session state or alternate-provider fallback was found. Already minted access authority remains bounded by the approved five-minute snapshot rule. |
| Canonical sealing-key rollout | Every replica boots `SealedSecretRewrap` before readiness; the operator pass walks provider secrets and every non-null sealed field of live browser sessions through the cross-tenant operator pool | `sealing.rs:1-174`; `human_connections.rs:381-543`; `boot/mod.rs:1507-1550`; `identity_e2e.rs:2994-3277`. Each field swap compares the exact old bytes. Renewal, logout, or a second replica that wins leaves `remaining > 0`; a later pass converges. A post-writer pass reaching zero makes K2-only restart safe. Keyless boot refuses while a live session envelope remains. | The prior system outage finding is closed. The journey creates both session modes under K1, exercises a lost CAS race, reaches zero on a later pass, starts a K2-only replica, renews/acts/logs out, and verifies the mode-specific durable effects. |
| BFF service-key rollout | Each BFF presents one raw deployment key; Wyrd admits one or two configured hashes before any session-store read | `bff.rs:35-91`; `config.rs:2176-2186,3505-3526`. The source supports the packet's safe order: add both hashes on Wyrd, roll BFFs to the new raw key, then remove the old hash. A wrong ordering temporarily removes UI access but does not broaden authority or affect public SDK/machine paths. | The mechanism is bounded and fail-closed. The recorded tests cover parsing and wrong-key refusal, but do not exercise mixed old/new BFF replicas during a live rollout; this remains a verification limit, not a demonstrated source defect. |
| BFF-to-Wyrd transport | Every session operation, including the raw BFF key and returned Wyrd authority, uses the shared `serverUrl()` origin and native `fetch` with bounded timeouts | `upstream.ts:3-36`; `server-sessions.ts:76-94,105-125,321-340`. Non-loopback plaintext is rejected before fetch. Native HTTPS supplies server authentication and confidentiality when correctly deployed. | The implementation boundary is correct, but the required production-shaped TLS proof is absent; see `SYSTEM-R2-001`. |

## Failure and recovery assessment

- **One BFF crash or rolling replacement:** another replica resolves the same opaque cookie against Postgres-backed Wyrd state. The real HTTP journey proves callback completion and subsequent requests across two production Node processes.
- **Wyrd crash/cancellation:** PostgreSQL rolls back the open tenant transaction and releases session, refresh-family, and connection locks. A committed session survives restart when signing and sealing keys remain shared. There is no unbounded retry loop.
- **Postgres outage:** login completion, session reads, renewal, logout, and settings mutation refuse at their request boundaries. Existing browser state is not replaced by a local identity authority and becomes usable again after Postgres/Wyrd recovery. No direct outage/recovery journey is recorded.
- **IdP outage:** new login fails after consuming its one-use state; the user starts again. Existing SSO session renewal rotates Wyrd refresh authority without calling the IdP, so established UI sessions remain available unless Wyrd/Postgres is also unavailable.
- **Concurrent renewal/logout:** the common browser-session row lock prevents both from committing. Logout wipes sealed fields; an interrupted renewal cannot leave a half-rotated browser row.
- **Connection replacement/deactivation:** the connection-slot lock orders lifecycle change against token issuance. Once cutoff commits, later renewal ends the old-connection browser session; previously issued access authority keeps only its approved bounded lifetime.
- **Sealing-key rolling replacement:** K1 remains usable during the roll. Exact-byte CAS prevents the boot pass from overwriting renewal/logout, and the post-writer zero report is the retirement gate. The two-minute unrewrapped login-completion window remains explicitly retained in the runbook.
- **BFF service-key rolling replacement:** two accepted hashes make an overlap possible, but no deployed mixed-key recovery proof was supplied.
- **TLS/certificate failure:** native `fetch` will refuse an untrusted or unavailable HTTPS endpoint, taking UI session operations offline while other Wyrd surfaces remain available. The candidate proves URL policy only, not successful production-shaped TLS establishment and recovery.

## Affected capabilities and blast radius

The browser path affects every production UI request because the SvelteKit hook resolves its server-owned session before tenant page work. Failure of a BFF replica is masked by another replica; failure of the shared Wyrd/Postgres authority removes UI session use but does not authorize fallback identity and does not disable independently authenticated SDK, CLI, MCP, or machine traffic. IdP failure affects new SSO login only. Connection cutoff affects sessions from that connection at their next renewal. Sealing-key retirement can affect every tenant's live UI sessions, but the remediated global inventory and post-writer zero gate now protect that blast radius.

## Material proposed finding

### SYSTEM-R2-001 — The required production-shaped TLS channel has only parser/mock evidence

- **Classification:** `MISSING`
- **Violated obligation:** The TASK-003 packet requires the deployment BFF service key to travel over TLS on the private server route. Remediation `R1-AC-05` additionally requires that HTTPS and loopback HTTP policy be enforced **and** that “the real production-shaped channel has TLS proof.”
- **Locations:**
  - `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md:227-234`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:4-64`
  - `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:91-121,257-267`
  - implementation boundary: `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:3-24`
- **Evidence:** `upstream.test.ts` accepts an `https://` string but replaces the fetcher with `recording()`, which never opens a socket, negotiates TLS, validates a certificate, or sends the BFF key. The real two-BFF identity host obtains `srv.base_url()` from the plain bound test server and permits it only because it is loopback HTTP. Repository search found no TLS-backed BFF-to-Wyrd journey. The implementation evidence lists only the three URL-policy Vitest selectors for `R1-AC-05`, so its claim of production-shaped TLS proof is unsupported.
- **Observable system consequence:** URL validation prevents cleartext to a non-loopback host, but the accepted production topology has not demonstrated that a built Node BFF can trust the deployment CA, negotiate the private HTTPS hop, authenticate with the BFF key, complete/read/renew a session, and recover after endpoint replacement. A CA-mount, hostname, or runtime trust-store mismatch would make every production UI request fail while all loopback evidence stays green.
- **Smallest testable correction:** Extend the existing identity UI host, rather than adding another harness, so its current Wyrd endpoint is reached by the built Node BFF through a test TLS terminator using a repository-managed CA and hostname. Configure the BFF runtime to trust that CA, keep both BFF replicas and the existing real session flow, and prove at least completion on one replica plus read/authority on the other over the HTTPS origin. Also prove an untrusted certificate is refused before any authenticated session operation succeeds. Retain the existing loopback HTTP journey for local topology and the non-loopback plaintext refusal unit test.

## Verification assessment

The task packet records all broader commands as green after the candidate's last code commit: UI unit tests and type check, the full identity journey (including four real UI scenarios and thirty Rust identity scenarios), `test:wyrd`, `test:sql`, code generation, tenant-isolation, docs, format, lints, and `git diff --check`. This review did not rerun Cargo or mise lanes.

Static source inspection confirms credible real-system proof for two BFF processes, shared Postgres session authority, concurrent renewal, connection cutoff, logout, OIDC-off renewal, two active provider tenants, provider replacement, conditional callback issuer binding, sealing-key CAS races, K2-only restart, and keyless boot refusal. The production TLS hop is the one explicit remediation proof that the recorded commands do not exercise. Additional residual gaps are Postgres outage/recovery and a mixed old/new BFF service-key rollout; source shows bounded fail-closed behavior for both, so they are not proposed findings.

## Overall result

**FAIL**

The candidate closes the prior sealing-rotation outage and has sound source-level recovery behavior for process loss, dependency outage, concurrent session lifecycle, provider cutoff, and K1-to-K2 restart. It does not satisfy the remediation task's explicit requirement for a real production-shaped TLS proof of the secret-bearing BFF channel.
