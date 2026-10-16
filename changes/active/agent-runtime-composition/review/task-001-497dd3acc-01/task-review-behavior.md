# TASK-001 behavior review

Overall result: **FAIL**.

Subject: base `06a368906187be20364a2d959da311151964b617` to candidate
`497dd3acc8fd06567a0e0ff01baf599f9af1711b`; specification revision 3 and the
original task at the base commit. This is a static acceptance audit. No build,
Cargo command, or test was run, and no implementation file was modified.

## Proposed findings

### BEH-001 — REGRESSION — Python migrated routing calls lose their Run/Card attribution

- **Obligation:** REQ-016 and AC-009 require the migrated gateway-inference
  workflow to preserve Run/Card correlation and server subject authorization.
- **Location:** `sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py:49-55`
  (`answer`), consumed at lines 207-223.
- **Producer-to-result evidence:** Before this change, the same loop created
  `state.run(alias)` and invoked that Run. The removed
  `wyrd-client/src/observe/invoke.rs::Run::invoke_on` constructed
  `PublicWyrdGatewayCaller::with_subject(Some(self.run_id()),
  Some(self.subject_uid()))`. The replacement `answer` neither creates a Run
  nor sets `extra_headers` on its OpenAI request. `openai_client` at lines
  42-46 supplies only the caller's access token and endpoint. There is no
  ambient Run scope in this test or another request hook that supplies these
  missing fields. The unchanged server
  `components/gateway/ingress.rs::requested_subject` (lines 346-382) returns
  `None` when both headers are missing. `GatewayInvocation` at
  `components/gateway/invocation.rs:487-493` invokes subject authorization
  only for a supplied subject and otherwise retains `None`.
- **Reachable consequence:** All four successful migrated calls in
  `test_agent_prompt_routes` are now uncorrelated gateway calls. Their Run and
  Agent Card evidence cannot be joined to this workflow, and this path no
  longer exercises authorization of its claimed Card. The test still passes
  because its Bifrost projection at lines 216-220 checks only deployment.
  The separate lone-header test proves the server can retain explicitly
  supplied headers; it cannot restore headers omitted by this producer.
- **Sibling callers and existing owner:** Python support-desk `answer`,
  Python local-development `call_agent`, Rust `Inference::ask_agent`, and TS
  `askAgent` already supply the two existing subject headers. The defect is
  local to this Python migration helper, not the gateway, Run identity, or
  authentication machinery.
- **Smallest testable correction:** Keep application-owned inference and
  the existing caller. Create/pass the intended Run at this call site and
  attach its ID plus the alias's registered Card UID using the existing
  `wyrd-run-id` / `wyrd-card-uid` headers. Extend this existing journey's
  retained-call projection to assert the exact Run, Card and principal for
  both aliases/deployments; preserve the existing denied-deployment assertion.
  Do not restore Run invocation, add an SDK runner, or change server
  attribution rules. Run the task's exact Python `test_agent_prompt_routes`
  command under its managed Postgres wrapper.

### BEH-002 — MISSING — TypeScript's migrated Prompt journey bypasses both stored Prompt bodies

- **Obligation:** REQ-013/016 and the original task's consumer-closure rule
  require migrating actual runtime consumers while retaining meaningful
  surviving behavior and assertions; AC-009 requires the resulting
  application inference journey.
- **Location:** `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:401-403`
  (`askAgent`), used for both aliases at lines 421-425.
- **Producer-to-result evidence:** The original journey invoked a hydrated
  Run for `agent` and `body-agent`, causing the referenced Prompt to be
  rendered and dispatched. These aliases intentionally point at different
  supported authored forms: `fixtures/cards/gateway_inference/ask-prompt.yaml`
  has portable `messages` with `{{question}}`, while
  `ask-body-prompt.yaml` has a native Chat Completions `body.messages` with
  `${question}`. The replacement reads only `prompt().spec.model`; every call
  sends the same literal `[{ role: "user", content: "hi" }]`. Neither
  stored message body is read or bound. Thus the native-body iteration is now
  the same request as the portable iteration regardless of their contents.
- **Reachable consequence:** This required migration journey succeeds even
  when neither hydrated Prompt contributes its request. It no longer proves
  the native-body application path it previously exercised; changes or
  breakage to that content have no effect on the call. Existing model,
  deployment and permission assertions do not detect the substitution.
  This is a lost task-required consumer regression proof, not a proposal to
  add a TypeScript Prompt execution API.
- **Sibling callers and existing owner:** Rust's migrated
  `Inference::ask_agent` and Python's migrated `answer` consume rendered
  Prompt requests. The checked-in TS support-desk application already
  consumes its portable Prompt messages locally. No SDK compiler or new
  runtime is needed for this test's two known authored forms.
- **Smallest testable correction:** Build this application request from the
  actual hydrated portable messages or native `body.messages`, binding the
  existing question variable in the corresponding authored form. Keep it
  local to the existing test/application caller and retain correlation and
  caller authentication. Make the two Prompt inputs observably distinct and
  assert the upstream receives each Prompt's bound content, so a hard-coded
  common message cannot satisfy the test. Run the existing exact TS
  `agent prompt routes` selector under the task's managed Postgres wrapper.

## Acceptance matrix

Verification names below refer to source-inspected tests and the candidate
task's Implementation Evidence. Reported green outcomes were **not rerun or
independently observed** during this audit.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012: no public executable SDK Agent, including native escape paths | Python root/package/stub deletions and `src/lib.rs` aggregation removal; Skald Python module/features removed; Rust/TS expose declarative Agents only | Reported import probe, Python unit/lint/type/wheel checks, TS package/native declaration checks; static source/export sweep | PASS |
| REQ-013: runtime-exclusive companion removal and consumer closure | Runtime Agent/tool/session/callback/mock glue deleted; public error classes and Prompt package retained; application examples migrated | Deleted tests inspected for runtime ownership; surviving Prompt parity tests remain; TS migrated native/portable Prompt journey bypasses content (BEH-002) | FAIL |
| REQ-014: declarative Agent Cards/registration/hydration/Prompt navigation and internal execution preserved | `WyrdState::agent`, Cards and Prompt owners unchanged; Skald edits remove Python-only cfg fields/glue, retaining Rust runtime and Vala/server callers | Reported P-R/P-P/P-T (3 each), `test:skald`, `test:shared`; preservation journey source includes real server hydration and verification | PASS |
| REQ-015: Run invocation and all projections removed without substitute runtime | Shared invoke module and caller deleted; Python pymethod/authored stub, TS native method/public method/declarations deleted | Static consumer/export sweep; reported SDK owner and package checks | PASS |
| REQ-016: same caller/deployment/model, Run/Card, observations, verification and gateway/subject refusals | Support-desk examples send application calls with headers; local-development Python sends native requests with headers; Rust gateway helper supplies headers; Python gateway routing helper omits them | Support-desk retained evidence assertions survive; local-development readback and foreign-Card refusal survive; Python routing Bifrost projection misses Run/Card (BEH-001); TS Prompt bypass (BEH-002) | FAIL |
| REQ-017: external framework/gateway/MCP/telemetry integrations survive | Framework adapters, trace processors, MCP server/auth, observation scopes and native gateway owners unchanged; runtime-only dependencies removed from clients | Existing framework/stock-client tests remain; candidate cites unchanged owner coverage; gateway negative/capture tests remain | PASS |
| REQ-018: accurate surviving product boundary in docs/examples/generated surface | Run observations docs call application helpers; gateway setup shows native HTTP plus auth/subject headers; SDK/Rust example docs updated; declarations remove native invoke | Reported docs/example/dependency/format/lint/type/package checks; one-time source sweep found no executable Wyrd Agent consumer in published docs | PASS |
| INV-001: durable gateway authorization/capture/accounting/tools/audit stay server-owned | Server gateway and audit owners unchanged; application requests use existing ingresses | Existing server/gateway tests remain; static owner/dependency inspection | PASS |
| INV-002: caller Wyrd token, server-owned provider credentials | Python `http_auth`, TS `httpAuth`, Rust `access_token` authenticate migrated calls; provider deployment/credential owners unchanged | Support-desk/gateway/local-dev upstream key/leak assertions preserved or extended | PASS |
| INV-003: gateway access/Agent declarations do not grant tools | No tool permission or catalog/server dispatch edits; removed Python local-tool runtime has no replacement | Existing server-hosted and MCP permission tests unchanged | PASS |
| INV-004: delegated hosted tool and direct MCP domain checks, non-blocking audit | HostedTools/server verifier/MCP/audit owners unchanged; no alternate server write path | Existing Verifier and hosted-tool journeys retained; reported preservation results | PASS |
| INV-005: server execution refuses native routes | Only Python-only code removed from Skald; server binder and Vala execution owners retained | Existing hosted/server route tests retained; P-R/P-P/P-T reported green | PASS |
| INV-006: Skald stays independent from Wyrd client/server mechanics | Shared client drops Skald agent/runtime/provider/tool dependencies; Skald owns native runtime and removes Python-only optional dependencies | Reported `check:deps`, `test:skald`; manifests/lock diff inspected | PASS |
| INV-007: portable macOS/Linux x86_64/arm64 behavior | No platform-specific execution, resource or process mechanism added | Static diff; no cross-platform runtime checks run | PASS |
| INV-008: declarative/persisted/evidence/server verification contracts retained | No wyrd-spec schema/Card-kind/server persistence change; shared errors survive; Run identity/observation code remains | Existing hydration and verification journeys; reported P-R/P-P/P-T | PASS |
| AC-007: public/native/typing/package/docs absence | Removal reaches Python package/native registration/root stubs and TS source/native/declarations; no compatibility shim | Static absence inspection and reported import/package checks | PASS |
| AC-008: existing Card and internal verification journeys across SDKs | Surviving hydration/navigation and Verifier code unchanged except Python-only Skald glue removal | Reported 9 selected preservation journeys and owner tests | PASS |
| AC-009: all invocation projections absent and correlated migrated three-language journeys | API deletion complete; migration has the Python attribution and TS Prompt proof gaps described above | Reported green J-R/J-P/J-T do not falsify either gap because assertions omit the affected values | FAIL |
| AC-010: external integrations, subject authorization/capture/header stripping/secrets intact | Server validation/authz/stripping owners remain; application helpers generally supply correct subject headers | Lone-header, foreign-Card, viewer/deployment refusal and support-desk evidence tests retained; missing Python producer headers tracked under AC-009 | PASS |
| AC-011: accurate docs/examples/export artifacts and appropriate checks | Owning docs and examples updated; no new SDK execution abstraction | Candidate reports docs/example/format/lint/dependency/type/package checks; no raw logs inspected | PASS |
| Non-goal: no revision-2 lifecycle/tool binder/workspace/limit/budget/cleanup/Jev refactor | Skald modifications limited to Python boundaries and Python-only result fields; server/Vala unchanged | Complete changed-owner and consumer inspection | PASS |
| Non-goal: no Agent/Runner/Prompt execution replacement or compatibility alias | New `answer`/`askAgent` functions are application/example/test code; none are a language SDK execution export | SDK exports and manifests/declarations inspected | PASS |
| Non-goal: no client MCP toolset/transport relocation/new execution endpoint/hosted binder move | No affected MCP/server runtime file or new endpoint in diff | Static diff | PASS |
| Non-goal: do not remove Prompt authoring/loading, Agent Cards, native protocols, external integrations, MCP or observational sessions | Those owners/exports remain; deleted `SessionMemory`/`SessionTurn` belong to the retired runtime, not observed run sessions | Surviving package, observation and hydration source/tests | PASS |
| Non-goal: no persisted/wire/permission/audit/provider credential/shared-error contract change | No schema/migration/server behavior diff; Wyrd error classes and server-emitted codes remain | Static diff and existing server owner tests | PASS |
| Non-goal: no unrelated package cleanup or permanent name-ban framework | Dependency cleanup follows removed runtime consumers; no permanent sweep checker added | Manifests/lock and file-list inspection | PASS |

## Caller and owner audit

Navigation began with CodeGraph; bounded source/diff searches expanded when its
results did not cover a requested path. Reviewed the cumulative change by
owner: Python public/native/stub/package runtime deletion; shared-client Run
execution and native-post deletion; Skald Python feature/glue/result-field
removal; Rust and TS public/native projections; native declaration removal;
support-desk examples and assertions in all languages; docs-only support-desk
Rust app; gateway inference migrations; CLI/native local-development migration;
example manifests/tasks/docs and dependency/lock closure; removed runtime,
callback/session/local-tool, smoke and gateway-caller tests.

Traced application paths through caller auth, deployment/model selection,
request construction, subject header producer, gateway ingress subject decode,
authorization and evidence readback. Checked siblings before locating the
Python defect at its producer. The existing server remains the owner of
attribution validation; changing it would hide the producer error and expand
scope. No new graph store, identity resolver, validator, transport service,
cache or runtime owner was introduced that needs replacing with another
repository owner. Existing auth integrations and native ingress surfaces are
the appropriate migration mechanisms.

## Verification notes and limits

- Candidate Implementation Evidence claims J-R 6/6, J-P 4, J-T 4; P-R/P-P/P-T
  three each; shared/Skald/Python unit, typing, dependency, package, lint, docs
  and example checks. Those are recorded claims, not captured run output.
- The explicit task asks for `ts:test:unit`; its execution is not explicitly
  recorded in the candidate evidence. No behavioral RED outcome/diagnosis is
  recorded there for the executable migration scenario. These are evidence
  limits for the orchestrator to reconcile, separate from the source-proven
  findings above.
- The candidate acknowledges `examples/support-desk-app/rust/lib.rs` is a
  docs-only file not compiled by a lane. Its source was inspected, but this
  review adds no compile proof for it.
- No live-provider, cross-platform or restart experiment was performed. The
  source preserves the internal execution/server ownership boundary; reported
  green checks cannot compensate for omitted application request values or a
  test that bypasses its stored Prompt.

No optional improvements or unrelated refactors are proposed. The material
remaining risk is acceptance of migration journeys that pass while omitting
required provenance or their actual Prompt input.
