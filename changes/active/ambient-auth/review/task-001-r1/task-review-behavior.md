# TASK-001 behavior review

Overall result: **FAIL**.

## Immutable subject and scope

- Repository base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Repository candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External example base: `bef9fc0`; candidate: `b283666df93a32cb73aa8c07f881d9271797dff0`.
- Approved authority: `changes/active/ambient-auth/spec.md`, revision 3; original task: `changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md`.
- Reviewed the complete 27-file cumulative Wyrd diff and the external example diff. No other discovery report was read. No implementation or generated artifacts were changed.
- Used CodeGraph first for client/native/authentication and CLI navigation, then expanded through source, consumers, tests, declarations, manifests, and verification task definitions.

Applicable authority inspected: root `AGENTS.md`, `architecture/agent-rules.md`, Wyrd design's principal/credential, client-model and journey boundaries, `architecture/wyrd-doctrine.mdx`, the reference router, spec-driven development, maintainer style, testing workflows, Python/stub, PyO3 and TypeScript guidance, and the security posture's credential and trust-boundary rules. The approved transport explicitly removes provider placeholders; it does not modify server authorization or the server's one-credential rule.

## Caller-to-result coverage

| Changed surface | Traced production path and regression boundary |
|---|---|
| TypeScript `WyrdClient.httpAuth` | Stock SDK/official MCP/Agents MCP receives a bound arrow-function fetch, copies effective request headers, deletes all three provider credential names case-insensitively, awaits the same client's `accessToken`, sets the Wyrd header, and calls global fetch with other request options preserved. `Request` input and `init.headers` use ordinary fetch override semantics. |
| Shared token authority | `accessToken` calls native `NativeWyrdClient::access_token`, then `WyrdClient::access_token`, then `AuthMiddleware::bearer`. The latter owns the existing bearer, API-key, workload, renewable-login, and delegated branches and their cache. The task adds no alternate credential resolver, cache, exchange, or renewal workflow. |
| Python property and transport | Native `PyWyrdClient::http_auth` imports the helper lazily and passes the same Python client instance. Sync and async flows mutate the request's header container and ask that client for each token. The async path awaits `asyncio.to_thread`; the Rust access-token method detaches the interpreter while using the shared runtime. A delegated client has the same getter through its native class. Missing optional HTTP dependencies retain the requested extra-install error. |
| Python administration | `wyrd.gateway` keeps its native `Gateway` import, administration TypedDicts, exported names, stub source and assembled stub. Only the standalone auth class and its lazy export are removed. Existing typed-contract fixtures, public-export tests, administration setup and OpenAI gateway journeys remain. |
| CLI | Clap routes `AuthCommand::AccessToken` to the new dispatch; `crate::client::from_global` retains endpoint/config/credential resolution, then the existing access-token method exchanges or returns the bearer. The command prints one line; the process logging subscriber writes diagnostics to stderr. |
| Tests and support | Inspected changed TS unit, gateway inference, MCP, local-development and server support paths; Python helper units, stock-client journeys and local-development substitution; CLI journey and surrounding fixture pattern. The changed duplicate role name is local to test setup and leaves authorization assertions intact. |
| Documentation and generated text | Framework guide covers OpenAI, Anthropic, Gemini, Pydantic AI, Python/TS MCP, callable-key alternative and CLI; generated LLM text includes that guide. Other generated-text changes synchronize existing Responses schema text rather than alter runtime behavior. |
| External example | Verified `src/agent.ts`, `scripts/investigate.ts`, package manifest, and ignore rules directly with `git show b283666:<path>`. The tracked MCP investigation command exposes the incomplete migration below. Also inspected the locally installed SDK projection, which is an ignored symlink and therefore an environment limitation, not immutable candidate source. |

The existing alternative to both adapters is precisely the existing client token API. The new wrappers earn only the foreign SDK header hook: they retain the client and delegate token authority to it. No additional framework-specific adapter, dependency-owning token service, transport cache or compatibility alias is introduced. The Anthropic dependency is development-only and used by its required stock-client journey.

## Acceptance matrix

Verification evidence below distinguishes tests present in source, task-recorded executions, and this review's actual command. Reported passes are scoped claims, not independently rerun suites.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| D1: retain existing token primitive and owner | TS `index.ts:1550`; Python `src/client.rs:118`; shared client `client.rs:438`, `auth.rs:840` | Existing native paths unchanged; focused stock-client journeys exercise API-key exchange | PASS |
| D2 / REQ-001: TS transport is `WyrdClient.httpAuth`, callable as fetch | `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1569-1575` | Changed gateway unit checks both URL/init and Request inputs; recorded TS unit pass | PASS |
| REQ-001: remove Authorization, x-api-key, x-goog-api-key and set only current Wyrd credential | TS `index.ts:1571-1574`; Python `_http_auth.py:49-52` | TS unit checks all three placeholders; Python units do so through both HTTP libraries; stock SDK journeys succeed | PASS |
| REQ-001: token is fetched for every request; cache and renewal remain shared | Getter closure calls `this.accessToken()` inside every invocation; Python flows call the held client for every yielded request | TS mocked minting and Python Minting units require distinct per-request values; shared Rust bearer branches preserve renewal | PASS |
| REQ-001 / D2: Python sync/async httpx and httpx2 auth | `_http_auth.py:25-46`; native getter `src/client.rs:136-139` | Python units drive httpx sync and httpx2 sync/async; OpenAI/AsyncOpenAI/Anthropic journeys recorded passing | PASS |
| REQ-001: async renewal does not block event loop | `_http_auth.py:45`; Rust `src/client.rs:119` | Source proves off-thread token resolution and interpreter release; no separate event-loop timing proof was recorded | PASS |
| REQ-002: remove standalone helpers with no aliases | TS removes actual base helper `gatewayFetch`; Python removes `_auth.py`, GatewayAuth lazy export and stub declarations. Base does not contain `accessTokenSource` or `wyrdFetch` | Residual search of main-repository `sdks/` and `docs/` finds none of GatewayAuth/gatewayFetch/accessTokenSource/wyrdFetch | PASS |
| REQ-002 / r3 / AC-008: retain gateway administration | Python runtime/stub changes remove only auth helper declarations; Gateway and administration types/operations remain | Existing public export and surface tests, gateway type fixture, and 10 existing gateway journeys recorded passing | PASS |
| REQ-003: access token, workload identity, API key and saved login use supported chain | Both language wrappers reuse existing constructed client and `AuthMiddleware::bearer` branches; CLI uses existing `from_global` | New environment/API-key journey plus existing client-source architecture; all-source renewal journeys were not rerun in this audit | PASS |
| REQ-003: unresolvable credentials retain catalog error | Native construction and access-token projection remain unchanged; wrapper does not catch/replace errors | Native error-preserving source; no new missing-credential transport journey in this diff | PASS |
| REQ-004 / D6 / AC-005: CLI prints only usable token plus newline | `auth/access_token.rs:28-32`, `auth/mod.rs` routing | New `auth_access_token_cli_journey` asserts one nonempty token line, distinct from API key, and accepted gateway `/v1/models`; focused run recorded passing | PASS |
| D3: one documented transport for providers and MCP; one-line callable-key alternative | New stock-client guide and generated LLM text | `docs:check` recorded passing; docs include all specifically requested frameworks | PASS |
| D4: Wyrd header and server exactly-one rule unchanged | Adapters set `X-Wyrd-Access-Token`; no server source changed | Provider/MCP journeys would reject extra placeholder credential headers; units check their removal | PASS |
| INV-001: tenant isolation | No new tenant selector or server behavior; verified client bearer remains sole identity authority | Existing server auth/tenant enforcement preserved, real-server journeys use token identity | PASS |
| INV-002: secrets never logged | Adapters set request headers without logging; CLI deliberately writes requested token only to stdout, diagnostics stay stderr | Source inspection; no new secret-bearing debug/error payload introduced | PASS |
| INV-003: no SDK durable behavior | Token calls end at shared Rust middleware; adapters only own per-request header projection and Python off-loop dispatch | Cumulative production diff contains no duplicate exchange/cache/renewal state | PASS |
| AC-001: four TS stock-client journeys through WyrdTestServer using httpAuth | Gateway inference has stock OpenAI and Anthropic; MCP reference file has official SDK and Agents `MCPServerStreamableHttp`; fixture deploys ambient credentials | Task records gateway 12 and MCP 2 passing, plus local-development 1 | PASS |
| AC-002: Python OpenAI, AsyncOpenAI, Anthropic and HTTP MCP journeys use http_auth | `tests/integration/gateway/test_client_http_auth.py` drives all four; MCP uses supported httpx2, with httpx's same adapter protocol covered in unit | Task records new file's 5 tests passing | PASS |
| AC-003: placeholder provider key succeeds through transport | SDK constructors keep `wyrd` placeholder; MCP test supplies Authorization placeholder; transport removes them | Stock-client journeys and explicit header assertions in units | PASS |
| AC-004: revoked API key refused by token exchange before gateway | `revokedApiKey` creates/revokes fresh principal; inference test builds fresh client only afterward; httpAuth awaits accessToken before fetch | Test asserts provider SDK's cause is WyrdError and upstream unchanged, without pinning status; recorded passing | PASS |
| AC-006: external example uses one WyrdClient and httpAuth for OpenAI and MCP | Only agent's OpenAI construction migrated; tracked runnable MCP consumer still calls wyrdFetch | Immutable `git show b283666:scripts/investigate.ts` retains removed-helper import/call; local typecheck also fails against an ignored stale SDK, an environment limit | FAIL |
| AC-007: format, lint, codegen and owning verification | Task records formatting/lints/codegen/native declaration/typecheck/docs passes and focused affected tests; owning mise task definitions inspected | Credible bounded-task evidence recorded. Full integration/CLI/local-development aggregates are explicitly deferred to change review and are not claimed independently run here | PASS (bounded task scope only) |
| Non-goals: no API-key fallback, framework-specific helpers, or server changes | Complete runtime diff is SDK header adapters plus CLI; no server implementation or wire contract changed | Cumulative diff inspection | PASS |

## Proposed finding

### BEH-001 — MISSING: the external example's runnable MCP connection still uses the removed standalone helper

- **Violated obligation:** AC-006 requires the external example's OpenAI client and MCP connection to use the client-owned transport. REQ-002 removes the standalone helpers without aliases.
- **Exact immutable locations:** external commit `b283666df93a32cb73aa8c07f881d9271797dff0`, `src/agent.ts:19-20` (changed OpenAI consumer), `scripts/investigate.ts:9,21-23` (remaining MCP consumer), and `package.json:12` (runnable command). Confirmed using `git show` of each path at the pinned commit, not only working-tree reads.
- **Producer-to-consumer evidence:** the changed agent constructs a WyrdClient and uses its httpAuth. The committed investigation command imports that agent's gateway but separately imports `wyrdFetch` from wyrd-ai and gives `wyrdFetch()` to `MCPServerStreamableHttp`. It is reachable through `npm run investigate`, not an unused hypothetical consumer. The task evidence's claim that the example has no MCP connection is contradicted by this committed source. The reviewed Wyrd public SDK has no wyrdFetch export, as required by the approved contract.
- **Observable consequence:** when the example consumes the reviewed SDK, its real MCP investigation command imports a nonexistent helper and cannot load/typecheck. Leaving an older SDK in place does not satisfy the changed OpenAI consumer either: the old SDK lacks httpAuth. The missing MCP migration is proven from the pinned commits independently of local dependency state.
- **Sibling consumers/writers checked:** external committed `src`, `scripts`, `test`, package manifest and lock, plus the reviewed Wyrd SDK public source. `src/agent.ts` is the OpenAI owner; `scripts/investigate.ts` is the runnable MCP consumer. The existing WyrdClient is the common owner to reuse. Local vendored SDK tests are package copies, not additional application consumers to migrate.
- **Smallest testable correction:** finish the external application migration by sharing the existing client used by `src/agent.ts` with the investigation command, and give the MCP transport that client's httpAuth instead of wyrdFetch. Do not add a local adapter or compatibility alias. Preserve agent tracing, model setup, tool filter and investigation flow. Validate against the reviewed SDK using the example's existing package mechanism; do not treat an ignored stale symlink as part of the immutable reviewed commit.
- **Focused closure proof:** verify the resulting committed source has no removed-helper imports and both OpenAI and MCP constructors receive the shared client's transport. Run `mise exec -- npm run typecheck` from the external repository with its actual SDK dependency pointing to the reviewed candidate, then its existing focused application tests. No new full framework harness is needed.

### Local dependency evidence, distinguished from the immutable finding

The external candidate's committed `.gitignore` excludes `vendor/wyrd-ai`; the current path is an ignored symlink into a separate checkout. `node_modules/wyrd-ai` links to that vendor. This local SDK lacks httpAuth. Consequently `mise exec -- npm run typecheck` exited 2 with `src/agent.ts(20,98): error TS2339: Property 'httpAuth' does not exist on type 'WyrdClient'.` This demonstrates why no successful local build can be claimed, but it is not asserted as a committed dependency regression and is not a separate finding. No package installation or symlink mutation was performed during review.

## Verification limits and final subject check

This is a static cumulative acceptance audit plus one read-only external typecheck. No packages were installed, no full suites or Cargo jobs were launched, and none of the task's recorded passing lanes were independently rerun. Full owning aggregate execution remains change-review work. No prior findings or remediation were supplied for this first task-review round.

Before writing this report, Wyrd HEAD remained `824f0c491ea2686977c9da9fce03ac4ef65c01b1`; external HEAD remained `b283666df93a32cb73aa8c07f881d9271797dff0`. Only review artifacts were untracked in the Wyrd worktree.

The missing committed MCP example migration is an observable application/public-API failure, not a style objection. It prevents acceptance even though the main repository transport implementation follows the required behavior.
