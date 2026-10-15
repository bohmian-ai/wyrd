# Structured Ponytail validation — TASK-001 r2

Result: **PASS for bounded task review. Independently validated open finding ledger: empty.** FIND-TASK-001-1 and FIND-TASK-001-2 are closed. This does not certify the final whole-change lanes or a candidate-only build of the external example.

## Immutable subject and completeness

- Wyrd original base: `5bcd9e4872f1376f1722922fa63b67384243be04`; candidate: `0cfbf6422709cc9d3de680788c616f82f29255da`.
- External `/home/thorrester/Documents/GitHub/wyrd-example-ts` base: `bef9fc05ec1725070588bcd49efdc937b02b0131`; candidate: `000ed1f2f587a6629a471c51aadff5e05f0c820b`.
- Prior reviewed candidates: Wyrd `824f0c491ea2686977c9da9fce03ac4ef65c01b1`, external `b283666df93a32cb73aa8c07f881d9271797dff0`.
- Authority: approved ambient-auth r3, original TASK-001, R1 remediation, AGENTS.md, agent rules, active client/runtime-identity design, runtime/public-surface doctrine, security posture, reference router, spec-driven development, maintainer style and relevant Rust/PyO3/Python/TypeScript/error/testing guidance.

All six required discovery reports exist and are complete: task-review-behavior.md and task-review-invariants.md contain cumulative acceptance matrices; standards-review.md contains authority/rule coverage; maintainer-review.md contains changed-owner/caller coverage; system-review.md covers deployed and failure paths; domain-review-security.md covers the authentication/tenant boundary. All return PASS and propose no material finding. Read all six plus claim-comparison.md, subject.md, prior verdict/validated ledger and remediation/evidence. No follow-up report was required: no material contradiction or new untraced reachable path emerged from comparison or this validation.

Used CodeGraph before code navigation. Independently inspected the cumulative production, declaration, tests, dependency and documentation changes and the complete external diff; used the remediation diff only to locate prior corrections. Prior review artifacts are historical evidence, not new runtime behavior or authority. Traced credential production through native/shared owners, principal setup through its typed owner, all tracked external application consumers and ignored package provenance. Agreement was not treated as proof. No source, package, build, config, link, credential or server mutation was performed. Only this assigned report is written.

## Dispositions and final ledger

| Claim or prior finding | Disposition | Source-backed resolution |
|---|---|---|
| Six discovery reports' empty proposal union | VALIDATED EMPTY | No reachable material omission, regression, trust-boundary defect, missing touched Rust rustdoc or unearned replacement workflow remains established in the cumulative source. Independent acceptance and prior closure are detailed below. |
| FIND-TASK-001-1 (prior CONFIRMED; BEH-001/INV-001/SEC-001 and narrowed MAINT-001) | CLOSED | The actual runnable MCP consumer imports the existing exported application client and uses its httpAuth. No removed helper consumer or second explicit application client remains. Composed proof is qualified below. |
| FIND-TASK-001-2 (prior CONFIRMED; MAINT-002) | CLOSED | One-use raw create/revoke setup is deleted; the existing typed Principals owner performs the same lifecycle, preserving the actual negative scenario/assertions. |
| Prior STD-001 / stale ignored package regression | REJECTED, remains rejected | The committed external manifest selects an ignored local directory. Neither immutable subject commits the SDK bytes. Current supply is explicitly a composition; source does not substantiate a committed stale-package regression. |
| Reinterpret the example's separate trace-processor dependency as a new ambient-auth defect | REJECTED | Its import/setup exists at the external original base. This task changes provider/MCP transport wiring, and that wiring is correct in committed source. Composition is a proof limit, not evidence of a new transport failure. Absorbing tracing or removing it would change unrelated behavior. |
| Treat absent final aggregates as a new bounded source defect | REJECTED | Testing authority distinguishes bounded task proof from final change verification. Focused journeys/static checks exist and the remaining final lanes are expressly disclosed. AC-007 is not certified complete below. |

**Open ledger: `[]`. No CONFIRMED or REVISED finding remains to remediate.** No FIND-TASK-001-3 is allocated. There is no human-directed addition recast as a reviewer finding and no specification revision is required.

## Independent closure of FIND-TASK-001-1

The original failure was at the application producer/consumer seam: the created client was private while the tracked investigator retained a removed helper. In the external committed candidate, `src/agent.ts:19–20` now exports its sole `WyrdClient.connect()` and supplies its `httpAuth` and `serverUrl` to OpenAI. `scripts/investigate.ts:11,21–26` imports exactly that instance and gives its `httpAuth` and URL to MCPServerStreamableHttp. The script remains reachable through package.json's `investigate` command and tsconfig includes src, scripts and test.

Read the complete agent and investigator bodies. The agent's gateway.withOptions Run/Card headers, tracing registration, prompt/model settings, investigator model, read-only toolFilter, and existing connect/run/close ownership are preserved. Tracked-tree search across every src/scripts/test consumer finds one explicit WyrdClient construction and no wyrdFetch/accessTokenSource reference. Other Bifrost/state users need no transport migration. Corrected task evidence now names the actual MCP consumer.

The minimal selected correction has therefore landed at the shared application instance, without a new wrapper, cache, client, alias or framework adapter. Deleting the required investigator is unnecessary; reusing the existing client solves its actual gap. No downstream guard or production SDK correction is warranted.

Proof attribution is deliberately narrower than candidate-only qualification. Both external vendor/wyrd-ai and node_modules/wyrd-ai resolve to the ignored scratch compose package under `/tmp/claude-1000/.../scratchpad/compose/sdks/wyrd-sdk-ts/wyrd`. Read-only inspection finds scratch HEAD `531eb168940da76f494beba2a49857e1ce7561e2`, with uncommitted trace-processor changes and an unresolved `UU` Python uv.lock entry. This is not the pinned candidate and not a clean integrated SDK commit. The remediation's prose says that conflict was resolved; current status does not support that incidental statement. It does not change which emitted TypeScript package was supplied or establish a transport defect.

The supplied package exports executable dist/index.js and matching dist/index.d.ts. Its getter at JS line 924 has the candidate's exact header-removal/token/fetch behavior; its declaration at line 866 exposes `get httpAuth(): typeof fetch`. It additionally supplies WyrdAgentsTraceProcessor. `git show bef9fc05:src/agent.ts` independently confirms that trace-processor import and registration predate this task. Candidate-only typecheck is recorded as failing TS2305 for that separate export; no candidate-only success is inferred from the composed run.

Recorded external typecheck and investigator success, and the behavior reviewer's fresh successful typecheck, establish the migrated wiring in that supplied composition. Candidate-owned official MCP/Agents MCP and provider journeys separately exercise the new transport. These are sufficient focused evidence for this consumer correction; they do not prove a standalone candidate package supplies all existing example features, the tracing change itself, a reproducible release, or the full example ask/evidence journey. Do not reopen FIND-TASK-001-1 merely to absorb that unrelated integration dependency.

## Independent closure of FIND-TASK-001-2

The remediation diff deletes `revokedApiKey` from tests/support/server.ts, its raw routes/JSON/status/error handling and unused WyrdClient value import. Residual SDK/docs search finds no reference to it or removed auth helpers. The sole negative-test consumer at `gateway-inference.test.ts:226–249` now calls `Principals.connect()`, creates an empty-role Service named revoked-gateway-caller, and revokes that same id with service kind and the same audit reason before installing its returned credential.

CodeGraph and source trace the full existing equivalent: TypeScript Principals.connect/createServicePrincipal/revokePrincipal (`index.ts:2258,2269,2313`) → native NativePrincipals create/revoke → shared Principals handle (`principals/handle.rs:82–89,158–170`) → authenticated typed POST create/revoke routes. This owner retains validation, decoding, catalog errors, server authorization/audit and the existing principal lifecycle. CLI/Python projections and principal-role journeys also use that shared owner. No production principal capability needs changing.

The test fixture resets an isolated ambient administrator/config per test. Setup resolves its administrator before `WYRD_API_KEY` is replaced; only afterward is a fresh WyrdClient created for OpenAI. No token has been minted for that newly revoked credential. httpAuth awaits accessToken before fetch, so refusal prevents gateway dispatch. Placeholder key, WyrdError cause, unpinned status and unchanged-upstream assertions are all preserved. The recorded exact selector selected **1 passed, 11 skipped**, and owning TS format/lint/typecheck passed. There is no replacement helper or duplicate workflow. This closes the deletion/reuse cause without weakening AC-004.

The focused closure command already recorded is:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t "^revoked api key is refused by the token exchange, before the gateway$"'
```

No additional correction or proof harness is required for this finding.

## Reconciled cumulative acceptance

PASS below means independently inspected implementation plus attributed available proof at bounded task scope. It does not mean this validator executed the historical checks.

| Obligation | Independent implementation evidence | Available proof and limit | Assessment |
|---|---|---|---|
| D1 / INV-003 shared token/cache/renewal | TS accessToken → native accessor → shared client.rs:438 → AuthMiddleware::bearer at auth.rs:840; Python uses the same owner. Full bearer match retains raw, workload, API-key, renewable and delegated variants. | Per-request adapter tests; unchanged shared owner. No independent SDK cache/exchange. | PASS |
| D2 / REQ-001 TS transport | httpAuth closes over exact client; copies effective init/Request headers; case-insensitively deletes three carriers; overwrites Wyrd carrier; awaits token then forwards input/options to fetch. | Gateway unit checks both request forms, unrelated header retention and two minted tokens. R2 invariant reviewer freshly selected 1 pass. | PASS |
| D2 / REQ-001 Python transport | Entire HttpAuth retains exact client; both HTTPX families' sync/async protocols; common header transformation; async token call through asyncio.to_thread. Native getter constructs adapter around bound client; token accessor detaches interpreter. | Three adapter units, freshly passed by r2 invariant reviewer; source proves off-event-loop acquisition. No separate cache/lifetime owner. | PASS |
| REQ-002 / AC-008 removals and administration continuity | TS standalone gatewayFetch removed; Python GatewayAuth implementation/lazy export/declarations removed; required older names absent; native/public Gateway and administration types/operations retained. Source/output client stubs match getter and assembler projection. | Recorded gateway public-export/surface/type checks and 10 administration journeys. No missing export or admin deletion found. | PASS |
| REQ-003 credential sources and stable failures | Full existing resolver retains explicit/env chain, saved-login selection and file floor; existing bearer cases retained. Adapters do not catch/reclassify errors. Delegated clients retain their actual owner. | Recorded ambient API-key journey and raw-bearer unit. Workload/login/delegation continuity is composition over unchanged source, not fresh all-source expiry testing. | PASS |
| REQ-004 / D6 / AC-005 CLI | AuthCommand dispatch calls existing from_global assembly and shared access_token; prints only exposed token/newline after success. Changed Rust items include substantive rustdoc/errors/panics as applicable. | Journey asserts one nonempty non-key token line and gateway models acceptance. Recorded pass; historical raw cargo command lacks canonical exact mise/setup detail. Full CLI lane remains outstanding. | PASS bounded; evidence limit retained |
| AC-001 TS stock OpenAI/Anthropic/official MCP/Agents MCP | Real serverTest deployments; placeholder stock provider clients; both MCP clients list/call actual read tools; transport is httpAuth. | Recorded gateway file 12 and MCP file 2 passing journeys. No mock token substitute in journey. | PASS |
| AC-002 Python OpenAI/AsyncOpenAI/Anthropic/HTTP MCP | Five new tests use real gateway_server/native public property. Supported httpx2 drives MCP; shared dual protocol also has direct httpx coverage. | Recorded five passing journeys; no claim every third-party snippet or every HTTPX-family async permutation was run. | PASS |
| AC-003 placeholder success | Both adapters remove all three names and replace Wyrd token; stock journeys keep placeholder keys; provider receives server-owned credentials. | Explicit adapter assertions plus recorded successful server journeys. | PASS |
| AC-004 revoked key | Fresh-client ordering and typed principal lifecycle traced above; token acquisition precedes outbound dispatch. | Exact negative journey recorded 1 pass; rejection cause and untouched upstream retained. | PASS |
| AC-006 external one-client OpenAI/MCP | All tracked consumers traced above; existing instance supplies both transports. | Recorded composed runtime/typecheck plus fresh r2 typecheck; candidate-only example proof expressly excluded. | PASS for changed wiring; separate integration dependency retained |
| AC-007 checks/owning lanes | Relevant source, manifests/lock, test registrations and mise lane definitions inspected. Anthropic is dev-only, earned by required journey. | Recorded format/lint/codegen/N-API/TS/Python typing/unit/docs and focused journey results. Full TS/Python integration, CLI journey and installed-CLI local-dev lanes were not run. | Bounded proof supported; full AC-007 UNPROVEN until change review |
| INV-001/002 tenant/secret boundaries | Exact bound client's token; no tenant selector, token claim, SQL, server permission or production secret log addition. Existing gateway extracts exactly one carrier and verifies Wyrd audience; MCP retains authenticated principal boundary. CLI stdout is intentional contract. | Source trace and recorded real-server authorization paths. No fresh tenant/log qualification claimed. | PASS |
| D3 docs and non-goals | Guide covers required providers/framework/MCP/key-slot/CLI. Generated llms changes synchronize existing Responses schema forms; no runtime schema delta. No fallback, aliases, per-framework helper, server change or new production dependency. | Recorded docs/codegen checks; full diff/source inspection. | PASS |

The file-scoped TS fixture's denied-agent role rename preserves its assertion and has a recorded duplicate-principal diagnosis; it does not disable or relax a check. Runtime failure remains request-local: acquisition errors precede dispatch, optional Python imports remain lazy, adapters add no replay loop, persistent state, shutdown owner or replica affinity. Existing shared cache/renewal and server permission/audit ownership remain intact. No new outage, revocation-timing, cancellation or rolling-restart guarantee is inferred.

## Proof, final identity and recommendation

Fresh validator operations were source/provenance inspection and both cumulative `git diff --check` commands, which passed. A read-only provenance command initially hit an unavailable `python` shim; rerunning with `/usr/bin/python3` completed. This was not a test/build failure or a reason to change the environment. Existing r2 focused adapter/typecheck results were read; repeating those checks was unnecessary. No Cargo, full suites, live calls, credentials access, package install, generation or cleanup ran.

Both HEADs were checked at entry and immediately before this report's completion and still match the pinned candidates. External tracked status is clean; Wyrd only has the new untracked r2 report directory. Required reports are present and no candidate source mutation was observed.

Recommendation: accept TASK-001 at its bounded review gate, with the explicitly empty validated ledger and both stable prior findings closed. No remediation task is warranted. Preserve AC-007's remaining final-lane obligation and the composed-package qualification in the final verdict; neither constitutes proof of a new bounded source defect or authorization to absorb tracing.

Skipped fresh builds/full suites/live execution. Remaining risks are unrun final owning lanes and the external example's separately supplied trace-processor integration dependency.
