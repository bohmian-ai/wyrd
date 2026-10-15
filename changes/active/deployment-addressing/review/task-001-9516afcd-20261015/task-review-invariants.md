# Independent invariant review — TASK-001, remediation round

**Result: FAIL.** Source reviewed: `/tmp/wyrd-task-review-9516afcd`, immutable candidate `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`, cumulative base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. The original task, approved spec r6, prior verdict/ledger, R1 packet and appended evidence were inputs. No other new reviewer conclusions were inputs. The source archive has no CodeGraph directory. No builds or tests were run by this reviewer; reported runs below are implementation evidence, checked against their actual test bodies, not fresh execution.

## State and ownership coverage

The review followed the complete task, using the cumulative changed-path inventory and cumulative diff, with the remediation diff locating corrected owners. It expanded the shared starting map through these producer-to-consumer paths:

* Role JSON and typed permission scopes → Permission validation → PermissionSet normalization/intersection → exchange/issuer → effective subject's gateway requested/fallback/listing checks. Explicit scoped grants now survive `All`; conditional open-only intersections are accumulated and refused only if the complete representable meet does not cover them. Empty representable intersections are not prohibited. Wildcard/AnyOf axes remain covered by the same operation matching.
* Authored Prompt → shared compile → native request/derived text and media metadata → PromptSpec decode/register → authored serialization/hash → SDK GET/download/hydration; builders finalize through the same Prompt constructor. Portable media expected-kind and literal model validation are now at their producers. The output-schema walk is still incomplete (INVREV-R2-01).
* AgentSpec → Agent from_card/to_spec/save/load → immutable per-call route registry → existing bounded loop → public/server caller → real ingress → snapshot/admission → attempt/capture. Observed invocation constructs an Agent from hydrated values, whereas the public Python runtime loader/run is a separate path. The new installed-CLI journey exercises the former and bypasses the latter (INVREV-R2-02).
* Gateway batch file/claim → early invoke decision → claimed tracked task → fresh routing snapshot and requested-target decision → dispatch evidence → release/retain/record. Listing's SQL eligibility now prunes All-only restricted deployments before limit while preserving explicit model/deployment grants; ordinary audited decisions remain authoritative.
* Actual attempt adapter → engine provider identity → retained span projection, separate from requested/resolved deployment/model. Existing nonblocking Scribe outbox remains the audit/capture boundary.
* Workflow retirement across Card/Spec/client/runtime/server/SDK/export/fixture catalogs, retaining Eval DAG and observation fields, surviving Agent/provider/judge boundaries. Search hits in testing's Forge workflow-reference helper are unrelated operational vocabulary, not a surviving retired Card API.
* Installed CLI dev/tenant → credential upload → list deployment put → fresh TS requests → registered/downloaded bundles → Python observed invocation → restart/reuse/revocation. The extra stage preserves meaningful upstream/header/evidence checks but does not meet its selected public Agent-loader boundary.
* Connect pages, gateway example README and framework request tests: published setup and first-request paths use deployment/model addresses, ordinary credentials and current stock-client auth. Narrow-width visual proof is explicitly absent from the evidence; no visual pass is claimed here.

The existing alternatives are important: the implementation correctly reuses PermissionSet/issuer, Prompt compile/binder, Agent route registry, public/server gateway callers, snapshot/admission, bounded SQL listing, shared Cards/bundle hydration and ScribeOutbox. No replacement graph, dispatch loop, credential system or persistence workflow is needed. The two remaining corrections belong at the existing shared schema acceptance boundary and existing Agent loader/runtime/host integration; they do not justify a new harness or another downstream routing layer.

## Proposed findings

### INVREV-R2-01 — retain FIND-001-5 — INCORRECT — Important

**Plainly:** invalid output schemas still register because the new validator only recognizes a short list of schema keywords.

**Obligations:** REQ-006/007/008/011, AC-006, R1-AC-05 and its selected correction to validate schema syntax before storage/IO.

**Exact source:** `crates/skald/skald-spec/src/prompt.rs:235–310`, particularly the catch-all `:307`; callers `authoring.rs:363` (portable), `prompt.rs:400–407` (native and builder finalization), `skald-prompt/src/builder.rs:313–322`; `wyrd-spec/src/card/prompt/spec.rs:63–78` uses the same compiled decoder. Existing regression `authoring::tests::output_schemas_are_validated` starts at `authoring.rs:1199`; public registration regression `pg_card_registration_route::invalid_prompt_authoring_is_refused_before_writes` at line 4581.

**Reachable counterexample:** an Anthropic portable Prompt with ordinary messages/settings and `response_format: {"type":"object","properties":{"n":{"type":"number","minimum":"zero"}}}` passes `check_output_schema`: the root object is visited, `properties` recurses, `type:number` passes, and `minimum` falls through unconditionally. The same invalid schema in native `body.output_config.format.schema` reaches `Prompt::new` and passes; builders finalize through that constructor. No subsequent PromptSpec validation repairs it, so standalone and inline registration can persist it and dispatch copies it into the provider output field. The installed workspace jsonschema 0.18.3 meta-schema (`meta_schemas/draft7.json`) explicitly requires `minimum` to be a number. This is schema syntax, independent of any changing provider capability. `if`/`then`/`else` and `dependentSchemas` also receive no nested validation. Existing tests only exercise the implemented subset (type/required/properties/items/anyOf), so their green result does not close this counterexample.

**Consequence:** callers can register an invalid output contract and discover its failure only upstream or during later output validation, violating the promised early shared refusal. The comment describing the validator as structural-only does not waive the approved requirement.

**Smallest correction:** replace the partial schema-language reimplementation at the shared pure acceptance owner with standards-based, offline schema validity validation, reusing the already-installed workspace `jsonschema` mechanism where it fits the narrow owner. Preserve valid authored schema bytes, native names/options, field-specific structured errors, and the separate statically detectable destination constraints; do not fetch refs, add an IO resolver, or add per-SDK validators. Existing native/portable/builder callers must continue through this one boundary. This is completion of FIND-001-5, not a new product decision or new ID.

**Focused proof:** extend the existing authoring and public registration regressions with the numeric-keyword counterexample in portable and native standalone/inline forms and confirm no stored Card/operation/provider IO; extend the existing builder schema test for parity. Include a valid numeric-constraint schema and valid nested schemas to prove preservation. Keep exact focused selectors, no full suite.

### INVREV-R2-02 — retain owner-requested FIND-001-14 — MISSING — Important

**Provenance:** FIND-001-14 is an explicitly human-directed test addition; this report does not recast its origin as independent discovery. The mismatch between its required boundary and the candidate proof is independently source-checked here.

**Plainly:** the new bundle test runs a replacement Agent created by `Run.invoke`, so it still never proves that the public Agent loader can load and run the downloaded Agent.

**Obligations:** user-directed FIND-001-14 steps 3–5, R1-AC-14; REQ-008–010/013 and AC-005/011/012 at that selected boundary.

**Exact source:** `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:231–268`: line 239 loads a WyrdState, line 245 inspects its hydrated Agent's Prompt, lines 252–253 execute `state.run(alias).invoke(...)`. `crates/shared/wyrd-client/src/observe/invoke.rs:86–101` builds a replacement `Agent::new` with a copied route and a gateway caller, then `:103–108` calls `run_prompt`. This bypasses `skald-agent/src/python.rs:316–325` (`Agent.from_yaml`) and `:353–359` (JSON Agent loader) and their `Agent::from_card` construction. Its real Python `Agent.run`, `python.rs:378–405`, goes through `run_with`, not the observed-invocation route.

**Why the distinction matters:** the R1 packet explicitly selected public Python Agent loading/runtime, prohibited building a replacement Agent, and allowed only normal host gateway dependencies. The candidate Python Agent constructor/load/run exposes no host gateway caller/client binding; its authored stub at `python/wyrd/agent/__init__.pyi:399–401` explicitly says a local Agent cannot serve a gateway route. Thus `Agent.from_yaml(...).run(...)` cannot succeed for any downloaded gateway Agent through that public API. In addition, Jev's new single-call support is only in `Agent::run_prompt` (`loop_runtime.rs:239–243`); ordinary `run_with` still rejects Jev via `request_builder::validate_prompt_loop_request`, before the same provider loop can execute. The new test's use of Run.invoke avoids both paths and would pass if the actual Agent loader again lost the route. Separate route-retention unit tests cannot substitute for this specifically requested combined journey.

**Consequence:** the claimed register → download → load Agent → execute Agent → restart proof has an untested handoff and is currently unavailable on its selected public runtime boundary; the test therefore cannot establish the user journey it is labeled as proving.

**Smallest correction:** complete the existing public Agent loading/runtime integration over the downloaded bundle and normal WyrdClient host gateway dependency, reusing shared Cards/hydration/reference resolution, `Agent::from_card`, `with_gateway_caller` and PublicWyrdGatewayCaller rather than adding a new graph store/registry/transport or fabricating an inline Agent. Project only the host integration needed by that already-approved Python user path. Ensure the same existing single-call Jev semantics are reachable by the selected Agent runtime without enabling general tools or streaming. Extend the current T13 stage to inspect and execute those loaded runtime Agent instances and re-execute the same downloaded artifacts after restart, retaining its current upstream credentials, no-native trap and run-correlated retained-evidence checks. Preserve the separate existing observed Run.invoke path and ordinary tool/session/callback behavior. This correction overlaps existing FIND-001-2/3 owner paths, but should remain one combined closure requirement under human-directed FIND-001-14, not new duplicate findings.

**Focused proof:** the existing exact installed-CLI `test_installed_cli_gateway_setup` selector must now call the public Agent loader/runtime for all four providers; missing/lost route must fail the inspected loaded object, and the run must use that object. Keep current controlled upstream HTTP, exact deployment/model/identity assertions, denial/no-upstream check, zero-native trap, saved auth and restart rerun. If a narrow runtime regression is needed for the integration projection/Jev branch, select that exact owner test too; no replacement harness or full journey aggregate.

## Acceptance matrix

PASS below means inspected source and focused recorded assertions support the obligation, not fresh reviewer execution. Related test names refer to actual existing tests in the snapshot and the appended R1 table. An acceptance row can fail because its explicit combined proof remains missing even when its underlying serialization works.

| Obligation | Implementation evidence | Verification evidence / limit | Result |
|---|---|---|---|
| REQ-001 | ProviderDeployment validation, ModelRef split-first, deterministic CallPlan; native caller preserves full target; capture stores provider separately | deployment/routing tests; `native_ingress_preserves_named_deployments`, `agent_routes_reach_named_native_ingress` | PASS |
| REQ-002 | PermissionSet insert/subsumes/intersection, issuer error before signing, invoke decide per snapshot, SQL pre-limit visibility | `gateway_grants_survive_merge_and_intersect_exactly`, real exchange regression; batch pagination/interleaving regressions | PASS |
| REQ-003 | existing CredentialWriter/lifecycle, deployment-file full decode/validate then ordered authorized PUT with explicit progress | CLI gateway tests and installed-CLI malformed/duplicate/missing-credential/replay/header assertions | PASS |
| REQ-004 | external adapter retains nonreserved credential identity, screened transport and unchanged model suffix | `external_deployment_preserves_model_and_credentials`; stock clients and revocation | PASS |
| REQ-005 | check_model runs before request construction/variable derivation; provider once, route separate | `model_is_literal_configuration`, builders model regression and registration negative | PASS |
| REQ-006 | portable mapping/settings/media kind/source improvements present; invalid schema still accepted | source counterexample INVREV-R2-01 absent from current regressions | FAIL |
| REQ-007 | native pair/format/body.model/mixed-field checks preserve native options; schema defect remains | native extension/format tests and partial schema tests | FAIL |
| REQ-008 | authored codec/canonical hash/four-form materialization supported; early schema acceptance still wrong | Rust/Python/TS roundtrip tests now cover JSON/hash; current TS lacks derived accessor and R1 forbids adding one | FAIL |
| REQ-009 | durable required route retained from/to Agent, route registry reused; selected public loaded-runtime path not furnished | direct route preservation/refusal tests pass; installed journey avoids public Agent runtime | FAIL |
| REQ-010 | public/server native target projection and built-in compatibility preserved, no hidden native fallback; selected Python loaded Agent path unavailable | existing routed/tool-loop/override tests, source INVREV-R2-02 | FAIL |
| REQ-011 | typed shared contracts/errors/projections, fieldless error reconstruction; invalid schema remains durable and selected runtime gap remains | OpenAPI and codegen evidence plus source counterexamples | FAIL |
| REQ-012 | retired Card/Spec/runtime/client/routes/exports removed; Eval DAG and judges remain | retired-kind/runtime contract tests, source catalog search and preserved judge source | PASS |
| REQ-013 | real single-command dev/tenant and credential-first four-provider list setup with saved auth/restart | T13 assertions and recorded pass; combined owner extension separately fails FIND14 | PASS |
| INV-001 | TenantConn/RLS, caller-tenant snapshots/credentials/registry retained | SQL-scoped code and tenant journey assertions; no fresh multi-tenant execution | PASS |
| INV-002 | bound managed credentials/redacted views, native trap and upstream-only headers, no secret resolution on registration | upstream header/redaction/revocation assertions, credential owners | PASS |
| INV-003 | no old weight/provider scope/Workflow compatibility decoder in reachable revised surfaces | removed-field and retired-kind tests, source contracts/catalog search | PASS |
| INV-004 | authored hash stable; Agent from_card/to_spec now retains route; immutable invocation route and actual attempt target captured | roundtrip/override/isolation and capture regressions; selected journey proof gap under AC011 | PASS |
| INV-005 | reusable Skald owners, Rust server decisions and shared wyrd-client; no SDK durable business duplicate | dependency check evidence and ownership source inspection | PASS |
| INV-006 | audit/capture continue staging through existing process Scribe outbox; batch retry/settlement not new audit durability machinery | source path, not an injected audit-failure test run | PASS |
| INV-007 | no hidden native fallback; immutable gateway registry, initial-target constraint, per-attempt authorization, literal model and media kinds | focused runtime/ingress/admission regressions; no additional validated escalation found | PASS |
| AC-001 | defaults/restrictions/same-adapter independent named deployments and capture owners present | existing three-SDK gateway admin/inference and recorded task selectors | PASS |
| AC-002 | exact common authority/refusal, subject/actor separation, pre-limit batch pruning and actual-snapshot authorization | runtime permission and real exchange tests plus batch pagination/race cases | PASS |
| AC-003 | external unchanged model/header/authorization and revocation owners preserved | external journey and destination/credential tests | PASS |
| AC-004 | both forms standalone/inline exact authored JSON/hash GET/hydrate covered | Rust/Python derived introspection; TS JSON/hash via existing public surface (no accessor invented) | PASS |
| AC-005 | Rust runtime and observed routes/overrides work; owner-selected actual loaded Python Agent gateway run absent | INVREV-R2-02; current T13 proves only rebuilt observed Agent | FAIL |
| AC-006 | settings/media/model cases improved; invalid schema survives shared validation | INVREV-R2-01 | FAIL |
| AC-007 | judge resolve/cache uses declared route, refuses native under server gateway context, structured result and subject/SYSTEM owner preserved | current judge implementation and existing judge-route scenarios | PASS |
| AC-008 | public native requests keep target, built-in initial compatibility and explicit local Vertex refusal | native ingress/client tests; general local Python Agent Jev path gap belongs FIND14 | PASS |
| AC-009 | contracts/exports/docs mostly cohesive; specified public runtime path and schema behavior incomplete | static/check/served OpenAPI evidence cannot close these producer paths | FAIL |
| AC-010 | all retired public capability shapes removed while independent Eval task semantics retained | removed-kind/served contract assertions and source enumerations | PASS |
| AC-011 | single dev command/four deployments/fresh TS direct requests/evidence exist; selected added Agent-loader stage bypassed | installed T13 only proves observed invocation, INVREV-R2-02 | FAIL |
| AC-012 | complete-file validation and truthful progress, idempotence, saved auth/sealed credentials restart maintained | CLI and installed local-dev source/recorded evidence | PASS |

## Prior finding closure and docs criteria

| Prior finding | Source and proof assessment | Closure |
|---|---|---|
| FIND-001-1 | Separate subsumption, exact meet, conditional representability and issuer refusal; real exchange tests include both directions and mixed representable cases | CLOSED |
| FIND-001-2 | Agent retains declared route from/to card and uses own gateway registry before IO; missing dependency refuses; public Python host integration remains combined FIND14 scope | CLOSED at original route-loss cause |
| FIND-001-3 | Public Anthropic/Google/Jev request producer now preserves deployment, Jev run_prompt call_once supported, local Vertex explicit refusal; public ordinary Python runtime branch is FIND14 scope | CLOSED at original observed invocation producer |
| FIND-001-4 | compile carries expected media kinds/conflict check, binder rejects mismatch before mutation; native placeholders retain untyped semantics | CLOSED |
| FIND-001-5 | partial schema walker misses invalid standard keywords; same producer still accepts invalid schemas | OPEN: INVREV-R2-01 |
| FIND-001-6 | Agent provider carried through public header/server request to initial CallPlan check; direct clients/fallback retain translation | CLOSED for shipping built-in provider matrix |
| FIND-001-7 | All/open/granted pruning is in bounded SQL before LIMIT with explicit scopes retained | CLOSED |
| FIND-001-8 | shared check_model before derivation/new/builder finalization; normal slash IDs retained | CLOSED |
| FIND-001-9 | admitted adapter provider is carried separately into retained attempt spans | CLOSED |
| FIND-001-10 | actual admission always takes requested decision against actual routing snapshot; preclaim decision and fresh/uncertain claim behavior retained | CLOSED |
| FIND-001-11 | shared binder validates base64/20MiB budget, kind/provider MIME and Google File URI before replacement, literal compilation reuses it | CLOSED |
| FIND-001-12 | substantive docs added across prior changed item inventory; no codechecker or runtime fix substituted | No independent residual proposed; standards reviewer owns full rustdoc inventory reconciliation |
| FIND-001-13 | four forms and authored hash across three SDKs; available derived accessors checked Rust/Python | CLOSED, TS accessor limit honestly recorded and consistent with R1 no-new-accessor instruction |
| FIND-001-14 | real register/download/restart checks exist, but executed Agent is rebuilt by Run.invoke, public Agent loader/run bypassed | OPEN: INVREV-R2-02, human-directed provenance retained |
| R1-DOC-AC-01 | published setup page, credential-first one/list deployment commands, four providers, refusal and recovery | Supported by source and recorded gateway checks; no new finding |
| R1-DOC-AC-02 | complete stock clients and OpenAI Agents/Pydantic AI first calls with controlled provider tests | Supported by source and recorded framework tests; TS connect is synchronous in actual index.ts |
| R1-DOC-AC-03 | published Connect order and regenerated indexes, recorded docs:check light/dark | Narrow-width manual layout inspection explicitly missing; no visual pass claimed, route to docs/standards verification reconciliation |

## Limits and non-goals

No runtime, browser, provider, build, test, package installation, broad aggregate, live credential, crash/multi-pod or visual layout execution was performed. Missing historical behavior-level RED for FIND1/2/3 was not used as a defect by itself: source and current assertions were assessed. Approved adjacent changes were not treated as unauthorized drift. No new model mapping, portable audio/video, local Vertex gateway integration, hosted Agent job API, secret upload system, compatibility decoder or Workflow substitute was found necessary. The only retained proposals above complete approved behavior at existing owners.
