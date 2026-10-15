# Structured Ponytail validation — TASK-001

Result: **FIX_REQUIRED**. Two independently validated, bounded findings remain: `FIND-TASK-001-1` and `FIND-TASK-001-2`. No specification revision is needed.

## Immutable subject and method

- Wyrd: `5bcd9e4872f1376f1722922fa63b67384243be04..824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External example: `/home/thorrester/Documents/GitHub/wyrd-example-ts`, `bef9fc0..b283666df93a32cb73aa8c07f881d9271797dff0`.
- Authority: approved `changes/active/ambient-auth/spec.md` r3; original `tasks/TASK-001-client-http-auth.md`; repository AGENTS.md, agent rules, applicable design/client/runtime-identity and security boundaries, doctrine, reference router, spec-driven development, maintainer style, and language/testing references. The task cannot narrow AC-006 to the one external file it happened to list.
- Read all discovery reports, claim comparison, and focused follow-up; inspected the complete 27-file cumulative diff, including generated text and dependency lock changes, and the complete external diff. Used CodeGraph before source traversal, then expanded through source bodies, callers, declarations, package supply, test fixtures and mise definitions.
- This is a fresh validation role. No implementation changes, dependency installation, generation, full suite, or live provider execution. Only this report is authored by this reviewer. No prior `FIND-*` ledger was supplied, so numbering starts at 1.

The comparisons below use immutable external `git show`, `git grep` and `git ls-tree` objects for candidate claims. Current ignored dependency bytes are explicitly identified as environment inputs, not attributed to an external commit.

## Disposition of every discovery finding

| Discovery ID | Decision | Independent resolution |
|---|---|---|
| BEH-001 | CONFIRMED | Tracked runnable MCP consumer retains the removed import and does not use the existing application's client. Consolidated as FIND-TASK-001-1. |
| INV-001 | CONFIRMED | Same AC-006 defect; no additional credential-owner defect. Consolidated as FIND-TASK-001-1. |
| SEC-001 | CONFIRMED | Same missed MCP consumer; consequence is broken integration, not permission bypass. Consolidated as FIND-TASK-001-1. |
| MAINT-001 | REVISED | Retain the committed MCP migration defect under FIND-TASK-001-1. Remove the claim that stale ignored SDK bytes constitute a committed package regression. Supplying the built candidate package remains necessary closure preparation. |
| STD-001 | REJECTED | The independently reproduced TS2339 is real for this environment, but the committed manifest selects a local directory, not immutable stale SDK contents. No `vendor` file or gitlink is in the external candidate. It does not prove the asserted committed dependency regression. The external migration/proof remains incomplete under FIND-TASK-001-1, without a second finding. |
| MAINT-002 | CONFIRMED | Existing Principals owner completely covers this new setup workflow. Delete the added raw HTTP setup and reuse its typed methods. Retained as FIND-TASK-001-2 because it deletes unearned transport/contract machinery, the explicit blocking exception in the review skill. |

The system reviewer proposed no finding. The follow-up proposed no new discovery ID and is independently corroborated below. Non-blocking discovery notes (Python override annotations, exact-command record precision, fixture environment use) are not promoted to findings. No missing Rust documentation was established on the changed items.

## Final deduplicated ledger

### FIND-TASK-001-1 — Complete the external example's existing MCP migration

**Status:** CONFIRMED, with MAINT-001 narrowed. **Classification:** MISSING. **Discovery sources:** BEH-001, INV-001, SEC-001, the MCP portion of MAINT-001. **Obligation:** spec AC-006 and D3; REQ-002 forbids preserving removed helpers as aliases; AGENTS.md §§11–12 require completing affected callers and proving the touched example.

**Exact locations:** external candidate `b283666`, `src/agent.ts:19–20`, `scripts/investigate.ts:9,11,21–27`, `package.json:12`, `tsconfig.json:17`. Wyrd candidate's SDK `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1569` owns the replacement transport. Main-repository `sdks/` and `docs/` contain no removed helper references.

**Producer → consumer and reachability:** the changed `src/agent.ts` constructs one WyrdClient and gives its httpAuth to the exported OpenAI gateway. The full `answer` body continues to derive a model client with `gateway.withOptions`, preserving Run/Card headers. Its sibling investigation script imports that gateway for `OpenAIResponsesModel`, but independently imports `wyrdFetch` from `wyrd-ai` and passes `wyrdFetch()` to `MCPServerStreamableHttp`. The script connects, lists/uses read tools through its agent, and closes the MCP connection. `npm run investigate` executes this tracked script and README advertises that workflow; tsconfig includes scripts. This is an active sibling of the changed gateway consumer. The reviewed SDK exports no `wyrdFetch`, so a correctly supplied current SDK cannot load/typecheck that named import. The task's claim that the example has no MCP connection is false.

**Sibling/owner comparison:** immutable application search across `src`, `scripts`, and `test` finds this one MCP construction and the one explicit WyrdClient in agent.ts. Bifrost callers in query/test use their existing facade and require no migration. Read the complete agent module, complete investigator, package manifest, tsconfig, external tree/diff, and the SDK getter/accessToken body. No alternative token owner is needed: reuse that already-created application client. Existing tracing, gateway model setup, read-only tool filter and MCP connect/close remain caller-owned.

**Dependency discrepancy resolved:** the immutable external tree has 17 tracked files, no vendor content, and no SDK gitlink. Its `.gitignore:2` excludes `vendor/wyrd-ai`. The manifest's `file:vendor/wyrd-ai` and linked lock entry do not pin source bytes or a commit digest. Current `vendor/wyrd-ai` is an ignored symlink to a scratch checkout outside either reviewed tree; node_modules resolves through it. This reviewer reproduced the local typecheck failure: `src/agent.ts(20,98): TS2339: Property 'httpAuth' does not exist on type 'WyrdClient'`. That is missing successful build proof, not evidence that the candidate committed those stale bytes. Neither keeping the stale package nor refreshing it alone closes AC-006: the former lacks httpAuth and the latter exposes the tracked removed MCP import.

**Observable consequence:** the documented investigator cannot consume the reviewed SDK and authenticate its MCP connection through the one application client. No bypass, credential leak, or new server failure is claimed.

**Smallest decision-complete correction:** make the already-created client available to the existing investigator and use that client's httpAuth for its MCP transport, removing the obsolete import/call. Preserve gateway.withOptions, tracing, model configuration, read-only tool filter, and MCP lifecycle. Correct the inaccurate task evidence. Supply a built candidate SDK via the existing local file dependency for proof and record its resolution/identity; the package exports executable `dist/index.js` and declaration `dist/index.d.ts`, so supplying only native declarations is insufficient. Do not commit an ignored SDK snapshot, introduce an adapter, create another client, publish a package, add compatibility exports, or redesign packaging.

**Ponytail ladder:** the existing MCP workflow is required by AC-006 and cannot be deleted. The replacement already exists on the application client. Native fetch is already behind that getter. No new dependency, abstraction, configuration or runtime is necessary. Delete the stale helper consumer; reuse the existing owner.

**Focused closure proof:**

1. Commit-object inspection of all application consumers shows exactly the one constructed application WyrdClient serving OpenAI and MCP, with no removed helper reference. Record external candidate and supplied SDK candidate identities and actual package resolution.
2. With that built SDK supplied, from this Wyrd root run `mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run typecheck`. This checks src/scripts/test together and must pass; the current environment does not.
3. In the example's existing configured deployment, run its existing MCP path with `mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run investigate -- 'Use wyrd.guide to explain the overview.'`. Successful MCP connection and answer establish the migrated consumer executes. This command needs that example's configured server/provider; record an unavailable environment as a proof limit, never a successful run. Do not create a new framework harness or require the full external ask journey suite merely for this wiring correction.

The existing two focused Wyrd MCP journeys already exercise the exact stock transports. They are supporting evidence, not a substitute for correcting and typechecking the actual external consumer. No broad SDK suite is needed in this remediation.

### FIND-TASK-001-2 — Delete duplicate principal administration in revoked-key setup

**Status:** CONFIRMED. **Classification:** DRIFT. **Discovery source:** MAINT-002. **Obligation:** AGENTS.md §15 reuse-first requirements; task-review's complete-workflow reuse and deletion threshold. The required AC-004 negative journey must remain.

**Exact locations:** `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:113–143`, `revokedApiKey`; its sole caller at `tests/integration/gateway-inference.test.ts:225–242`. Existing replacement owner: `wyrd/src/index.ts:2243–2315`, Principals.connect/createServicePrincipal/revokePrincipal; native `native/src/principals.rs`; shared `crates/shared/wyrd-client/src/principals/handle.rs`.

**Producer → consumer:** before mutating the ambient key, the sole test awaits revokedApiKey. The helper constructs an ambient admin transport, manually POSTs creation with `{name, roles: []}`, casts unvalidated response JSON to a fabricated id/credential shape, then manually POSTs principal revocation with service kind/reason. It returns the credential; the test installs it in WYRD_API_KEY, constructs a fresh WyrdClient/httpAuth, and asks stock OpenAI to call the gateway. A WyrdError cause and unchanged provider request count prove rejection before upstream. The setup is reachable and new, not dormant debt.

**Complete equivalent owner and siblings:** read the entire helper, its test, deploy/serverTest (which resets environment per test), the entire Principals class, N-API create/revoke bodies and the complete shared Principals handle. Principals.connect resolves the same ambient client before the key mutation. createServicePrincipal accepts the same name/roles, returns typed principal_id/credential, and calls N-API → shared Principals → authenticated `request_json(POST, /v1/principals, ...)`. revokePrincipal sends the same service-kind/reason to the same revoke route through the same owner. The Rust client owns auth, JSON decoding, catalog errors and replay semantics. Other principal consumers use the typed facade (e.g. principal-roles.test.ts), and CLI/Python projections reuse this shared owner too. No production owner, route, permission, audit or principal lifecycle needs modification.

**Concrete cost/consequence:** new setup duplicates route strings, request serialization, response-shape assertions and status/error handling already owned by the facade. Its cast bypasses that typed contract and makes future principal contract changes require a second update outside the owner. This is not merely naming or placement: approximately two dozen added lines of transport and contract machinery can be deleted without losing any approved behavior. The test-only scope means this is not a production security finding.

**Smallest decision-complete correction:** delete the new one-use revokedApiKey helper and its sole import; perform the required create/revoke setup through the existing Principals facade in the negative journey. Use the same empty-role service principal and revokePrincipal semantics, preserving the test's fresh post-revocation client, unpinned status, WyrdError cause and unchanged-upstream assertions. Remove the now-unneeded support-module WyrdClient import. No new helper, raw HTTP replacement, native method, error type, fixture server or test is required. Typed setup is complete equivalence, including server authorization/audit. Do not change revocation to a different lifecycle operation.

**Ponytail ladder:** deleting AC-004 itself is prohibited. Deleting only its raw setup implementation is possible because the installed repository capability already implements the entire workflow. This does not remove required httpAuth coverage: the OpenAI operation under test still uses it, while successful OpenAI/Anthropic/MCP journeys cover actual outbound transport. Testing httpAuth for principal-administration setup was an adjacent, unrequested purpose and does not justify retaining duplicated principal operations.

**Exact focused closure commands** (run from Wyrd root after the normal existing TS build prerequisites are available):

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t "^revoked api key is refused by the token exchange, before the gateway$"'
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
```

The exact test name and setup wrapper come from current test source and mise.toml. Assert that one test ran; no positional zero-selection proof, full gateway suite, new test harness, or broad aggregate is needed. No codegen lane is needed for this test-only correction because no public declaration changes.

## Validation of acceptance and pass claims

`PASS (source + recorded)` means the implementation obligation is supported by independently inspected source and the task's recorded focused proof; it does not invent a fresh execution. `PASS (fresh)` has an actual command/result below. Full owning aggregates are reserved for change review by the task-review workflow and remain unproven here.

| Obligation | Independent implementation evidence | Verification evidence and practical limit | Result |
|---|---|---|---|
| D1 / INV-003: one token/cache/renewal owner | TS accessToken → NativeWyrdClient.access_token → shared WyrdClient.access_token → AuthMiddleware.bearer; Python access_token reaches that same owner. Entire bearer match retains renewable, raw bearer, workload, API-key and delegated branches. | Adapter tests require fresh per-request asks; source contains no second token cache/exchange. Shared owner is unchanged. | PASS (source) |
| D2 / REQ-001: TS fetch/header behavior | index.ts:1569–1575 copies effective Headers, deletes three case-insensitive names, replaces Wyrd header, awaits accessToken before fetch, forwards input/init. | Fresh exact gateway unit: 1 passed, 3 unselected. It covers URL/init and Request and distinct tokens. | PASS (fresh) |
| D2 / REQ-001: Python sync/async dual-httpx auth | Full HttpAuth body retains same client; both flows authenticate original request; async token call uses asyncio.to_thread. Native getter attaches the actual bound native object; access_token detaches interpreter. | Fresh test_http_auth.py: 3 passed. httpx sync, httpx2 sync/async; source proves off-loop token execution, not a new timer/latency guarantee. | PASS (fresh) |
| REQ-002: removed standalone helpers, no aliases | Complete SDK/docs residual search clean; TS removes base gatewayFetch; Python removes GatewayAuth implementation/export/stubs. accessTokenSource/wyrdFetch already absent in main base. | Diff and search; public-stub source/output pairing inspected. External consumer independently fails AC-006. | PASS for SDK removal |
| REQ-003: every existing credential source / missing-credential catalog | Full resolve_credential and bearer methods preserve existing chain; neither adapter re-resolves identity or catches/replaces its errors. | API-key ambient journey source and recorded pass; fresh unit uses a raw bearer. Workload/saved-login/derived-client continuity is source composition, not newly run journeys. | PASS (source + recorded) |
| REQ-004 / AC-005: CLI exact token output | Full dispatch uses existing from_global, awaits token before printing once. Clap dispatch is wired. Existing CLI assembly preserves config/errors; new test checks exchanged non-key output and spends it at models. | Task records focused CLI success; exact historical command lacks required mise/environment detail. Source proves intended assertion, no fresh binary run. | PASS (source + recorded; command-record limit) |
| AC-001: four TS stock-client journeys | Gateway inference uses OpenAI/Anthropic with placeholder key and httpAuth. Official MCP/Agents MCP list and call real tools using only httpAuth. serverTest starts real Wyrd server and deploys ambient credential, not mock auth. | Task records gateway file 12 and MCP file 2 passes. Native/server fixtures inspected. Not rerun by validator. | PASS (source + recorded) |
| AC-002: Python OpenAI, AsyncOpenAI, Anthropic, HTTP MCP | New five-test file drives real gateway_server deployments and HTTP /mcp with public getter. HTTP MCP uses supported httpx2; httpx uses the same adapter protocol and is directly covered by fresh unit. | Recorded 5 passes; no separate async-httpx1 journey claimed. | PASS (source + recorded) |
| AC-003: placeholders succeed | TS/Python units explicitly assert removal of all three carriers. Stock SDK journeys retain placeholder keys and successful replies; upstream receives server-owned provider credential. | Fresh four adapter units plus recorded real-server journeys. | PASS (fresh unit + recorded journeys) |
| AC-004: revoked API key rejected before gateway | Fresh client created only after revocation; accessToken awaited before fetch; test asserts WyrdError cause and no provider call, status unpinned. | Recorded passing case; source negative flow sound. Duplicate setup is FIND-TASK-001-2 and does not negate the existing rejection behavior. | PASS behavior; DRIFT setup |
| AC-006: external one-client OpenAI and MCP | Immutable agent change is incomplete; existing script retains wyrdFetch and cannot access private client. | Immutable application-source proof plus fresh failing local typecheck; latter's stale dependency is explicitly not committed. | FAIL: FIND-TASK-001-1 |
| AC-007: checks and owning lanes | Source and lock versions agree; new Anthropic dependency is development-only and required by AC-001. Existing mise definitions own each test file. Generated llms Responses additions match already-existing owning schema definitions. | Task records format/lints/codegen/native/TS/Python typechecks/docs and focused tests passing. It explicitly does not run full TS/Python integration, CLI journey, or local-dev lanes. | Bounded task proof supported; full AC-007 remains UNPROVEN until change review, not an unconditional PASS |
| AC-008: gateway administration retained | Runtime/stub diff removes only auth exports/classes; native Gateway and all administration definitions remain. Stub assembler maps hand-authored client/gateway sources to public outputs. | Existing public-export/surface/type fixtures and 10 gateway journeys recorded passing. No missing owner/export detected. | PASS (source + recorded) |
| INV-001 / INV-002: tenancy and secrets | Both adapters bind exact client; no caller tenant selector, token claim, server policy, SQL or production log change. CLI exposes only intentionally requested stdout. | No new server authority; successful journey authorization and unchanged shared owner. No independent tenant or log suite claimed. | PASS (source) |
| D3 docs and non-goals | Complete guide covers required providers, framework, both MCP languages, callable-key alternative and CLI. Production diff has only header adapters and CLI, no server fallback/per-framework helper/cache. | Recorded docs check; SDK source/snippets inspected. Every third-party framework snippet was not independently executed. | PASS (source + recorded) |

No additional runtime failure-containment defect is established: exchange errors refuse the requesting operation before outbound dispatch; adapters add no retry loop, background owner, durable state or server shutdown. Existing shared token state and sibling server availability are unchanged. The duplicate fixture role correction preserves rejection assertions and has a recorded concrete duplicate-principal cause; no timeout/assertion/check was weakened.

## Actual focused executions and limits

Executed through mise without package installation:

```sh
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/gateway.test.ts -t '^httpAuth replaces provider key headers with a fresh access token on every request$'
# exit 0; 1 passed, 3 unselected
mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/client/test_http_auth.py
# exit 0; 3 passed
mise exec -- npm --prefix /home/thorrester/Documents/GitHub/wyrd-example-ts run typecheck
# exit 1; TS2339 at src/agent.ts(20,98), current ignored package lacks httpAuth
```

These fresh checks strengthen adapter behavior proof without running full suites or mutating source. No red runtime test was diagnosed from an untraced panic; the external failure is a deterministic compiler/declaration mismatch, whose package resolution was inspected. No successful external build is claimed. No package-supply workflow was invented: the external README has no canonical vendoring script; the existing local file mechanism can be supplied for proof without a distribution redesign.

Final identity check: Wyrd HEAD remains `824f0c491ea2686977c9da9fce03ac4ef65c01b1`; external HEAD remains `b283666df93a32cb73aa8c07f881d9271797dff0`. Wyrd worktree has only untracked review artifacts. The immutable source candidate has not changed.

Skipped full suites, installs, generation and live provider calls. Remaining risks are the unmigrated external MCP consumer, unearned setup code, and explicitly outstanding final aggregate/example runtime proof.
