# Repository standards review — TASK-011 r2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree at review start and before report write:
  `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`,
  revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Lead direction:
  `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`
- Scope: the complete 60-file cumulative base-to-candidate diff, including
  the r1 review packet and remediation evidence. The remediation delta was
  also used to locate the changed owner and confirm prior-finding closure.
- CodeGraph: not used because the repository has no `.codegraph/` directory.

The lead reversal is applied as authority: browser logout always clears local
cookie/cache state, and RFC 7009 revocation is best-effort. The original
`FIND-TASK-011-2` retry requirement is not reopened. The approved SSO/recovery
page and API-key token-tenant decisions are also treated as settled.

## Authority coverage

| Changed surface | Files and symbols covered | Applicable authority read and applied | Coverage |
|---|---|---|---|
| Approved task and remediation record | `spec.md`; original TASK-011; r1 verdict, findings validation, remediation task, lead reversal; revised implementation evidence | Current human direction; `AGENTS.md` §§1–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | Complete |
| BFF OAuth and browser-session owner | `browser-sessions.ts`: `BrowserSession`; `BrowserSessions.begin`, `complete`, `establish`, `access`, `read`, `logout`, `switch`, `metadata`; deleted `server-sessions.ts`; `session.ts`; `hooks.server.ts`; `app.d.ts` | Approved spec REQ-006/009/010/015/016/021 and INV-001/005; `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `AGENTS.md` client/server and security boundaries; TypeScript guide; Wyrd UI skill | Complete |
| Login, callback, recovery, logout, tenant switch, and authenticated settings/actions | `/login/callback`; tenant login and API-key recovery routes; root actions; tenant layout; settings route; change actions; upstream/problem projections | Same identity/security authority; approved lead decisions; SvelteKit server-boundary and native CSRF guidance in the Wyrd UI skill; canonical error and tenant-authority rules | Complete |
| UI components and safe page projection | `Shell`, `TenantChooser`, login/recovery pages, settings and change feature components, layouts and views | Wyrd UI skill; SvelteKit/Svelte patterns; `AGENTS.md` public-surface and accessibility expectations; maintainer style | Complete |
| TypeScript unit, routing, component, and real-wire journey tests | new `browser-sessions.test.ts`; `production-auth.integration.test.ts`; session, routing, Shell, and Changes tests | `AGENTS.md` §11; agent-rules test-placement/gate-integrity rules; testing workflows; task's narrow verification direction | Complete |
| Package metadata and lockfile | UI `package.json`; `pnpm-lock.yaml` | Task's exact `openid-client` 6.8.8 choice; existing-dependency/minimum-mechanism rule; TypeScript and UI package rules | Complete |
| Rust real-server journey host | `identity_ui_e2e.rs`, including two-server/two-BFF topology and changed helpers/test | `AGENTS.md` Rust, async, test taxonomy, external-test, and rustdoc rules; `architecture/agent-rules.md`; testing workflows; maintainer style | Complete |
| Shared Rust test harness | `WyrdTestServer::start_bound_replica` and its reuse of `start_replica`/`bind` | Struct-centered Rust ownership; async-at-IO rule; substantive rustdoc and `# Errors`; testing-owner rules | Complete |
| Identity lane wiring | `mise.toml` filtered UI journey selection | Exact test-selection and repository-managed environment rules; narrowest-task-lane direction | Complete |
| SQL, Python, PyO3, SDK declarations, generated schemas, MCP, and Bifrost production owners | No such implementation surface changed | Ownership/boundary router and `architecture/agent-rules.md` | Not applicable |

Every changed implementation path is represented above. The remaining changed
paths are the r1 review/remediation records, covered by the first row.

## Applicable-rule audit

| Rule | Source evidence | Result |
|---|---|---|
| Durable identity, grants, tenancy, and token issuance remain server-owned | The BFF uses server-issued access tokens for `/v1` calls. `BrowserSession.context` is a UI projection only; the server remains the authorization authority. No principal store, role mapper, grant writer, or durable session store was added. | PASS |
| Effective tenant authority comes from the verified credential, not browser route state | The encrypted cookie binds a route session, but upstream authority is the exchanged access token. The approved recovery-page case keeps the API key's token tenant authoritative; no route-to-tenant claim or mapping endpoint was added. | PASS |
| Approved standards libraries own OAuth and cookie cryptography | `openid-client` 6.8.8 performs discovery, authorization URL construction, code + S256 PKCE redemption, refresh, RFC 8693 exchange, and RFC 7009 revocation. `jose` 6.2.12 performs JWE sealing. Manifest, lockfile, and installed dependency graph agree. No hand-written OAuth call entered the diff. | PASS |
| Refresh credentials remain client-opaque | `BrowserSessions.establish` no longer decodes the refresh credential and applies the existing 12-hour application-session bound. `BrowserSessions.access` forwards the unchanged string to `refreshTokenGrant`. The focused test uses a non-JWT refresh token and proves forwarding. This closes `FIND-TASK-011-1`. | PASS |
| Logout follows the approved conventional local-first boundary | `BrowserSessions.logout` deletes the selected cookie and cache entry before best-effort `tokenRevocation`; failures resolve logout and log only tenant plus error class. API-key logout remains local-only. This matches the lead reversal and spec revision-8 precedent; no retry/store/setting/endpoint was added. | PASS |
| Browser credentials do not enter page data, URLs, or JavaScript | Session/access credentials remain in the encrypted HttpOnly cookie or private `BrowserSession.#token`; `metadata` emits only subject, expiry, and tenant keys. The real-wire journey scans page/data/redirect surfaces and checks JWE cookies. | PASS |
| Native CSRF boundary replaces the deleted custom mechanism | Custom CSRF fields and checks are removed. SvelteKit's default `csrf.checkOrigin` is not disabled, mutating browser paths remain form POSTs, and the built-server journey checks cross-origin refusal. | PASS |
| SvelteKit server/client separation is preserved | Discovery, exchange, refresh, revocation, encryption, and authenticated Wyrd calls remain in server-only modules/routes. Components receive typed safe projections and own no token or authorization behavior. | PASS |
| TypeScript contracts are typed and maintainable | New exported owners and operations have explicit parameter/return types; request payloads use `unknown`; untrusted form and cookie inputs are narrowed at boundaries; no `any`, wrapper-object types, or parallel public error type was added. | PASS |
| Secret and error handling remain redacted | `WYRD_UI_CLIENT_SECRET` is read only from private environment state. Logout warning fields exclude token values; canonical safe problem projections prevent upstream bodies or credentials from reaching the browser. | PASS |
| Changed Rust follows the owning-struct, async, and documentation rules | `start_bound_replica` is an inherent method on `WyrdTestServer`, reuses the existing replica owner, and awaits real start/bind IO. It has substantive workflow rustdoc and `# Errors`. Changed journey helpers/test retain intent and panic documentation. | PASS |
| External Rust test placement is earned | `identity_ui_e2e.rs` drives compiled BFF processes, two real Wyrd replicas, repository Postgres, TLS, and real providers. It satisfies the external integration-test exception. | PASS |
| Test taxonomy and exact selection are preserved | The public BFF capability has a real client/server journey. The identity lane verifies the four required Vitest names and runs the exact ignored Rust host expression; focused Vitest selectors cover remediation behavior. | PASS |
| Gate integrity is preserved | No `#[allow]`, check weakening, test deletion, or new ignored bypass was added. The existing ignored environment-owning journey remains explicitly run with `--run-ignored=all`. | PASS |
| Verification scope is the narrowest complete task scope | The task records exact focused remediation tests, UI `check`, full UI tests, filtered identity host, Rust format/lints, and `git diff --check`. Per current direction, unfiltered/full journeys are deferred to change review rather than required here. | PASS |
| No custom protocol or compatibility surface remains | `server-sessions.ts` and `/login/complete` are deleted; current implementation paths contain no `WYRD_BFF_SERVICE_KEY`, `internal/bff`, `x-wyrd-bff-key`, `wyrd_flow`, or custom CSRF field. | PASS |
| Permanent code does not name ephemeral task artifacts | `src/routes/t/[tenantKey]/login/api-key/+page.server.ts:15` includes `(REQ-010)` in JSDoc. This is a literal wording-rule miss, but it has no behavioral, security, tenancy, durability, public-contract, or rustdoc-completeness consequence. Under the explicit review direction, wording findings cannot block. | FAIL — non-blocking wording note |

## Material repository-rule findings

None.

## Non-blocking notes

### `NB-REPO-011-R2-1` — requirement identifier in permanent JSDoc

- Location:
  `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/login/api-key/+page.server.ts:15`
- Rule: `architecture/agent-rules.md` says permanent code must not mention task
  or plan identifiers.
- Evidence: the JSDoc says `(REQ-010)`.
- Assessment: wording-only. The surrounding prose independently documents the
  recovery and token-tenant boundary, so removing the identifier would not
  change behavior or maintainability. It is not a material finding and cannot
  produce a failing verdict under the supplied direction.

## Verification assessment

Independently rerun during this standards pass:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — 0 errors,
  0 warnings.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts`
  — 1 file, 2 tests passed.
- `git diff --check 7c48ac7c99f018d3993922e63875839f3695c503..4d468b33e49de4dd9df30c5dd046a334569465bf`
  — passed.
- Installed dependency inspection — direct `openid-client@6.8.8` and
  `jose@6.2.12`; `openid-client` resolves the same `jose@6.2.12`.

The task additionally records green full UI tests (33 files, 179 tests), both
exact remediation tests, the filtered production UI identity journey (four UI
journeys plus its Rust host), Rust formatting/lints, and diff checking. These
are appropriately narrow for task review. Full unfiltered journeys remain the
change-review gate by explicit direction, not a standards verification gap.

## Overall result

**PASS**

The cumulative candidate satisfies the material repository standards for the
BFF OAuth boundary, browser session security, tenant projection, SvelteKit UI,
Rust test harness, manifests, test taxonomy, and narrow verification. The only
literal standards miss is wording-only and therefore non-blocking.
