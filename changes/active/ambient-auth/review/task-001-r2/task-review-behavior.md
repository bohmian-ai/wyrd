# TASK-001 behavior re-review

Overall result: **PASS**, for the bounded task review. No material finding is proposed. Full change-closeout verification remains outstanding as recorded below.

## Immutable subject and method

- Wyrd original base: `5bcd9e4872f1376f1722922fa63b67384243be04`; reviewed candidate: `0cfbf6422709cc9d3de680788c616f82f29255da`.
- Previous reviewed candidate: `824f0c491ea2686977c9da9fce03ac4ef65c01b1`; remediation comparison includes that candidate through the current candidate, rather than replacing the original cumulative scope.
- External example base: `bef9fc05ec1725070588bcd49efdc937b02b0131`; reviewed candidate: `000ed1f2f587a6629a471c51aadff5e05f0c820b`.
- Authority: approved SPEC-ambient-auth r3, original TASK-001, TASK-001-R1, its prior verdict and independently validated findings; AGENTS.md, agent rules, reference router, applicable client/runtime-identity design, doctrine/security boundaries, specification/maintainer and Rust/PyO3/Python/TypeScript/testing/error references.

Used CodeGraph before navigating code. Inspected the cumulative production, test, declaration, dependency and documentation changes, expanded callers through the native/shared credential owner and the external application's tracked `src`, `scripts`, and `test` inventory, and inspected the previous review evidence as historical evidence. No other r2 reviewer report was read. No Cargo jobs, provider calls, credential-store access, installation, generation or reviewed-source mutation was performed. An early authority-reading command inadvertently wrote a concatenated authority copy to `/tmp/ambient-review-authority-behavior.txt`; it did not alter either subject or a build/configuration input, and no cleanup was attempted.

## Caller-to-result audit

The TS getter at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1569` binds its owning client in an arrow-function fetch. Each invocation copies the effective Request/init headers, removes the three case-insensitive provider carriers, overwrites the Wyrd carrier, awaits that client's `accessToken`, and forwards the original request plus other init values to global fetch. It follows ordinary fetch header-override semantics and does not mutate the caller's header collection. Failure obtaining a bearer prevents the outbound call. No alternate credential resolver or cache is introduced.

TS `accessToken` at `index.ts:1550` reaches `NativeWyrdClient::access_token` at `native/src/client.rs:103`, then the shared `WyrdClient::access_token` at `crates/shared/wyrd-client/src/client.rs:438`. `AuthMiddleware::bearer` at `auth.rs:840` retains the existing raw bearer, API-key, workload, renewable-login and delegated branches, with their shared cache/renewal. Construction resolves the existing chain through `ClientConfig::resolve_credential` at `config.rs:229`. The wrappers neither infer tenant authority nor swallow catalog errors.

Python's native getter at `sdks/wyrd-sdk-python/src/client.rs:136` lazily imports the adapter and passes the exact client object. `_http_auth.py` implements both HTTPX families' flow protocol; both paths acquire the token before yielding the request, and the async path dispatches the existing GIL-releasing Rust token accessor through `asyncio.to_thread`. A client returned by `on_behalf_of` has the same getter and retains its delegated identity. Optional HTTP imports remain lazy relative to normal client/gateway administration imports. The source stub and generated client stub agree on the property; gateway runtime/types/stubs remove only the standalone auth helper.

CLI dispatch at `crates/wyrd/wyrd-cli/src/auth/access_token.rs:28` uses the same `crate::client::from_global` owner as other commands, obtains the current bearer, and prints one newline-terminated token. It introduces no credential argument, exchange logic, cache or log statement. The new journey asserts one nonempty token line distinct from the API key and spends that token against the gateway's models route. Changed Rust items have substantive documentation, including fallible-operation errors and the test's panic conditions.

The new TS/Python provider and MCP tests drive real Wyrd test servers and local provider upstreams, retaining placeholder provider keys. Existing gateway administration exports, typed fixtures and journeys remain. The changed duplicate test role uses a distinct role/principal name on the file-scoped server; its recorded duplicate-identity diagnosis explains that correction, and the authorization assertion was not weakened.

## Acceptance matrix

Recorded executions below are evidence from the task/remediation files, not fresh executions by this reviewer. Source inspection establishes the implementation independently of those claimed successes.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| D1 / INV-003: existing token primitive, cache and renewal remain shared Rust behavior | TS/native/shared chain above; Python getter retains the native client; no shared client implementation changed | Existing shared-owner source; adapter minting units assert per-request calls | PASS |
| D2 / REQ-001: client-owned TS fetch | `index.ts:1569–1575` copies headers and delegates global fetch | Changed gateway unit covers URL/init and Request input; original task records TS unit lane passing | PASS |
| REQ-001 / D4: remove Authorization, x-api-key and x-goog-api-key; set Wyrd header | TS getter and Python `_authenticate` remove all three and overwrite Wyrd bearer | TS and Python units assert all three disappear; placeholder-bearing real-server journeys succeed in recorded evidence | PASS |
| REQ-001: acquire token on every request | Token accessor is invoked inside each TS call and Python flow, without adapter state/cache | TS unit and Python Minting tests require distinct request tokens | PASS |
| D2 / REQ-001: dual HTTPX sync/async Python transport; renewal off event loop | `HttpAuth(httpx.Auth, httpx2.Auth)` and `asyncio.to_thread` in async flow; native accessor detaches interpreter | Recorded 3 Python adapter units plus OpenAI/AsyncOpenAI/Anthropic journeys; source proves off-thread dispatch | PASS |
| REQ-002: remove standalone helpers without aliases | Actual base TS helper `gatewayFetch` removed; GatewayAuth implementation/lazy export/stubs removed; obsolete accessTokenSource/wyrdFetch absent in SDK and migrated external callers | Residual search across SDK/docs and tracked external application inventory is clean | PASS |
| REQ-002 / AC-008: retain wyrd.gateway administration | Existing Gateway import, administration types, exports and both gateway stub files remain | Existing public-export, gateway surface/type and 10 administration/provider journeys are recorded passing | PASS |
| REQ-003: every existing credential source and stable no-credential failure | Existing chain and bearer branches reused unchanged; no adapter catch/reclassification | Recorded environment/API-key journey; workload/saved-login continuity established by shared-owner composition, not a fresh all-source journey run | PASS |
| REQ-004 / D6 / AC-005: CLI prints usable token plus newline | New auth dispatch routes through existing ambient client | Recorded focused CLI journey checks stdout and gateway acceptance; no fresh binary execution | PASS |
| AC-001: stock OpenAI, Anthropic, official MCP and Agents MCP TS journeys | `gateway-inference.test.ts` and `mcp-reference-client.test.ts` use only httpAuth for these cases; serverTest deploys isolated ambient credentials | Original task records 12 gateway and 2 MCP tests passing; source preserves list/call and provider-result assertions | PASS |
| AC-002: Python OpenAI, AsyncOpenAI, Anthropic and HTTP MCP | Five-test `test_client_http_auth.py` uses the public native property, real gateway server and placeholder keys | Recorded five passes; MCP uses supported httpx2, while httpx uses the same adapter and has direct unit coverage | PASS |
| AC-003: placeholder-bearing request succeeds | New provider/MCP constructors retain placeholder keys; adapter strips them before send | Recorded journeys and explicit header-removal unit assertions | PASS |
| AC-004: revoked key rejected at exchange before gateway, status unpinned | Typed Principals creates/revokes first; fresh client constructed only after environment replacement; accessToken is awaited before fetch | Remediation records exact revoked-key selector: 1 passed, 11 skipped; WyrdError cause and unchanged upstream assertions remain | PASS |
| AC-006: one application client supplies OpenAI and MCP | External `src/agent.ts:19–20` exports its existing client; `scripts/investigate.ts:11,22–23` reuses its serverUrl/httpAuth | Fresh external typecheck exits 0 against inspected composed package; recorded investigator success is composition-scoped, as detailed below | PASS within explicit integration limit |
| D3 / documentation: required stock clients, MCP and callable-key alternative | New guide covers OpenAI, Anthropic, Gemini, Pydantic AI, both MCP languages, key-slot alternative and CLI | Recorded docs check; snippets inspected; not every framework snippet was executed | PASS |
| AC-007: appropriate format/lint/codegen and owning proof | Recorded format/lint/codegen/native/Python/TS/docs checks; remediation's TS format/lint/type checks recorded passing | Bounded task proof is present. Full TS/Python integration, CLI and local-dev aggregate lanes are explicitly deferred to change review | PASS for bounded task; full closeout remains unproven |
| INV-001 / INV-002: tenancy and secret handling | Exact client bearer remains the identity source; no server/SQL/tenant-selector change or new production secret logging; CLI stdout is the intentional requested export | Source/caller tracing; no fresh tenant-isolation/log suite claimed | PASS |
| Non-goals and reuse-first constraints | No fallback API-key transport, compatibility aliases, per-framework helper, server change, new token owner or new Cargo feature; raw principal setup deleted | Cumulative source and dependency diff; Anthropic added only as a required journey dev dependency | PASS |

The regenerated `docs/public/llms-full.txt` additions synchronize Responses-schema definitions already present in the owning checked-in schema source. They introduce no new runtime contract in this task. Prior review artifacts and task/spec updates are historical/workflow evidence rather than SDK/server behavior.

## Prior-finding closure

**FIND-TASK-001-1: closed.** The tracked runnable investigator no longer imports or calls wyrdFetch. It imports the client exported by the existing agent module and gives that client's httpAuth to MCP, alongside the OpenAI gateway already using the same client. Tracked src/scripts/test search finds one application WyrdClient construction. OpenAI `withOptions` Run/Card headers, trace setup, model configuration, read-only tool filter and MCP connect/close ownership remain unchanged. The task evidence now names the actual MCP consumer and dependency limitation.

The external package supply is not an immutable SDK artifact. `node_modules/wyrd-ai` resolves through the ignored vendor link to a scratch composition rooted at Wyrd `531eb1689`, with additional trace-processor changes. Its status includes a Python lockfile merge conflict. The composition's emitted `dist/index.js:924–931` contains the reviewed httpAuth logic, and its accessToken delegates to the same native owner. Its package exports executable JS plus matching declarations. The candidate's own TS package does not export the example's pre-existing WyrdAgentsTraceProcessor import; the external original base already imports that class, so this task did not create that separate integration dependency. Consequently the recorded live MCP/gateway success and this review's successful typecheck establish the migrated transport wiring in that composition, not a clean candidate-only example build or acceptance of the adjacent trace-processor change. No ignored package bytes are attributed to the external commit. This does not establish a remaining defect in the bounded transport migration.

**FIND-TASK-001-2: closed.** The raw revokedApiKey helper is absent from `tests/support/server.ts`, including its now-unneeded value import. The sole negative-test caller uses existing `Principals.connect().createServicePrincipal` and `revokePrincipal`. These project through N-API to shared `Principals::create_service_principal` / `revoke_principal`, which already own the same typed routes and errors. Creation still uses empty roles; revocation still names service kind and an audit reason. The fresh revoked-credential client, placeholder key, WyrdError cause, absence of status pinning, and untouched-upstream assertion are preserved. No replacement setup abstraction was introduced.

## Verification and limits

Fresh commands performed by this reviewer:

```sh
mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run typecheck
# exit 0; supplied SDK is the inspected ignored composition, not candidate-only proof
git diff 5bcd9e487..HEAD --check
git -C /home/thorrester/Documents/GitHub/wyrd-example-ts diff bef9fc05..HEAD --check
# both exit 0
```

No full suites were rerun. Original recorded focused journey evidence and remediation evidence are credible for bounded review but do not establish the explicitly unrun final owning aggregate lanes. The original CLI test record also lacks the canonical mise/environment wrapper; source establishes its assertions, but this report does not invent a corrected execution history.

Final identity check: Wyrd HEAD remains `0cfbf6422709cc9d3de680788c616f82f29255da`; external HEAD remains `000ed1f2f587a6629a471c51aadff5e05f0c820b`. The external tracked worktree is clean; Wyrd only shows the new untracked r2 review directory. No source-local proposal IDs are allocated because no material defect remains established by this behavior audit.

Skipped full suites, Cargo jobs, live calls and clean candidate-only example execution. Remaining proof limits are the final aggregate lanes and the example's separately supplied trace-processor dependency.
