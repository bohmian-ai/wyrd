# Independent findings validation — TASK-001

**Verdict recommendation: SPEC_REVISION_REQUIRED.** FIND-001-1 exposes an unresolved permission contract, in addition to bounded implementation defects FIND-001-2–13. A security-safe patch is possible, but cannot honestly be called preservation of the approved exact permission intersection without deciding the mixed-scope outcome. No source correction, test, build, migration, or external operation was performed.

Subject: immutable archive `/tmp/wyrd-task-review-ef2fe8bd`; base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`; candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`; approved `changes/active/deployment-addressing/spec.md` r5 and original `tasks/TASK-001-deployment-addressing.md`. References below are snapshot-relative candidate lines. Live concurrent tests and subsequent commits are excluded. No CodeGraph index exists in the archive. The cumulative diff, changed-path inventory and 46-commit history establish scope; claims were checked against source, not reviewer agreement. Read all nine discovery reports, focused follow-up and claim comparison. The latter is orchestration, not proof.

## Disposition crosswalk

| Discovery source ID | Disposition | Retained cause |
|---|---|---|
| INVREV-001, SEC-001 | REVISED | FIND-001-1: exploit/lost grant confirmed; exact-meet correction cannot be prescribed within current contract |
| FOLLOW-001 | CONFIRMED | FIND-001-1; narrows the correction, not another exploit |
| BEH-001, INVREV-002, MAINT-003, PROMPT-001 | REVISED | FIND-001-2; preserve route at Agent owner and existing caller boundaries, without replacing the loop |
| BEH-002, INVREV-003, MAINT-002, SYS-001, PROMPT-002 | REVISED | FIND-001-3; include the earlier Jev loop rejection, not just the unreachable public projection arm |
| BEH-003, INVREV-005, PROMPT-003 | CONFIRMED | FIND-001-4 |
| BEH-004, INVREV-006, PROMPT-004 | CONFIRMED | FIND-001-5 |
| BEH-005 | CONFIRMED | FIND-001-6; Agent origin compatibility only, preserve direct-client conversion |
| BEH-006, DATA-001 | CONFIRMED | FIND-001-7 |
| BEH-007, INVREV-004, PROMPT-005 | REVISED | FIND-001-8; shared literal model validation, no provider model catalog or slash prohibition |
| DATA-002 | CONFIRMED | FIND-001-9 |
| CONC-001 | CONFIRMED | FIND-001-10; snapshot-bound authorization, not immediate revocation of already admitted calls |
| PROMPT-006 | REVISED | FIND-001-11; validate at shared media boundary, reuse existing budget and source restrictions |
| REP-001, MAINT-001 | REVISED | FIND-001-12; one changed-item rustdoc correction, including changed test bodies |
| FOLLOW-002 | REVISED | FIND-001-13; bounded proof gap, no demonstrated hash/storage corruption or new API requirement |

No proposed source ID is silently dropped. No rejected proposal is retained as optional advice. The revisions above narrow correction/proof claims; they do not soften defects into preferences.

## Validated ledger

### FIND-001-1 — Critical — REVISED — INCORRECT / VIOLATION: permission algebra loses explicit authority and manufactures delegated Granted authority

**Obligations:** REQ-002; INV-007; AC-002; design lines 216–221, security posture 274–278, permission-model 184–191. **Locations:** `crates/shared/wyrd-runtime/src/permission.rs:866–871,886–890,917–952`; `crates/wyrd-spec/src/auth/permission_scope.rs:151–158`; `crates/wyrd/wyrd-auth/src/issuance.rs:212–230,778–787`; `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:709–738`.

**Independent evidence:** `Permission::covers` uses scope coverage and All covers every scope. `insert` removes a matching Gateway deployment/model grant beside All, in either order. Stored-role decoding merges through this function before authority issuance. Consequently built-in All plus a dedicated explicit grant becomes All-only and cannot invoke a Granted deployment. Separately `intersection` meets All × Gateway(D) to an actual Gateway(D) grant. `DelegateToken::execute` verifies both parties, resolves actor authority through the issuer and signs this meet (`exchange_api_key.rs:279–307`). The resulting subject token passes `contains_object_scoped` for Granted D although the All-only party cannot directly access D. Same-tenant provider spend/files/batches are reachable; this is not cross-tenant access.

**Sibling/correction owner:** direct issuance, direct verification and owner recovery all use the issuer's role-resolution owner; delegation is the production PermissionSet intersection consumer. Requested calls, candidate fallbacks, model visibility, batch admission/listing, and file access consume `GatewayInvocation::decide`; changing one handler or actor attribution cannot recover discarded grants. Bifrost/Verifier and wildcard/AnyOf operation semantics share PermissionSet and must survive.

**Bounded part:** preserve explicit Gateway invoke grants during normalization, including beside broad wildcard/AnyOf All grants. Distinguish grant subsumption from matching an Open target requirement: globally making All fail Gateway requirements breaks Open access. Keep existing PermissionSet, issuer, signed authority and gateway decision owners; no second grant store or role lookup.

**Material decision requiring specification revision:** All-only means every Open deployment; explicit D means D in either mode. Their common authority is D **only while Open**. Existing atoms are All, Deployment(D), Model(D,M), combined by OR. All overgrants other Open targets; either matching explicit atom overgrants Granted D; empty loses shared Open-D authority. Enumerating today's Open targets as explicit grants overgrants after an access-mode change. A successful exact meet is therefore not generally representable. All×All, explicit×explicit and some complete mixed sets remain representable; a blanket ban on any mixed pair is excessive.

Three choices have different observable contracts: (1) approve representation/enforcement of the exact conditional intersection; (2) explicitly approve attenuation, losing unrepresentable common Open access; (3) explicitly approve refusing otherwise valid exchanges whose exact result is unrepresentable. Options 2/3 avoid a new scope but change successful authority/availability. Existing identity, depth, size and activity refusals do not authorize labeling this valid case malformed or silently attenuating it. `into_access_grant` is infallible and no current issuer/delegate error declares this refusal. Do not prescribe one choice or claim complete closure from a no-escalation guard. The minimal safe policy can be chosen only after that clarification; nothing here requires a wire extension in preference to approved refusal.

**Focused closure:** existing permission unit owner plus real delegation/gateway owners: both insertion orders; wildcard/AnyOf axes; All×All, matching/disjoint explicit, both mixed directions; complete mixed sets already representing the exact result; Open/Granted transition; explicit-role merge; no upstream IO on denial; unchanged Bifrost scope tests. Run P1/P2 below after the approved decision and record the exact new regression selector. No broad suite is needed to prove this local correction.

### FIND-001-2 — Important — REVISED — INCORRECT / REGRESSION: runtime Agent drops its declared route and can execute natively

**Obligations:** REQ-009/010/011; INV-004/007; AC-005/009. **Locations:** `crates/skald/skald-agent/src/agent.rs:173–205,601–625,668–716`; `loop_runtime.rs:67,196`; Python `src/python.rs:355`; `crates/wyrd/wyrd-cards/src/agent.rs:103–136`.

`from_card` retains Prompt/tools/config/metadata but never `execution_route`; Agent has no such state; `to_spec` emits Native unconditionally. YAML/path/wire and Python loaders converge here; Card/save/model-dump projections converge on `to_spec`. `run`, `run_with`, `run_prompt`, and DelegateAgentTool's `run_with` select the retained/default provider registry. A loaded gateway Agent can therefore use host native credentials and changes route/hash when saved. Python authoring also synthesizes Native without accepting the required route. The shared observed `Run::invoke_on` and judge `agent_for` manually apply routes but do not repair this owner; Run additionally refuses tools. T09 manually binds registries to newly constructed Agents and bypasses the failing durable path.

**Correction:** retain the declared route in Agent across all Card/load/save projections and resolve the invocation-local effective route through its existing registry/caller seam at actual execution entry points. Host dependencies remain host-owned; absent gateway dependencies must refuse before native IO. Require declared route at durable authoring projections and project the whole-route override through existing supported public runtime surfaces, with Python wrapper changes at the approved SDK boundary. Preserve tools, sessions, journal, callbacks, server-native refusal and sub-agents' own routes. No replacement execution loop or independent SDK routing.

**Closure:** extend existing Agent construction/journey owners starting from a gateway Card, roundtrip it, drive a tool iteration and default/override isolation, assert a native fixture receives zero calls, and test missing gateway caller. Public Python YAML/JSON load/run coverage is required for its runtime lifetime. P3 and exact added selectors.

### FIND-001-3 — Important — REVISED — INCORRECT / MISSING: non-OpenAI Agent paths do not reach named public ingress

**Obligations:** REQ-001/010/011; AC-005/008. **Locations:** `crates/shared/wyrd-client/src/gateway_caller.rs:140–168,270–294`; `crates/skald/skald-agent/src/request_builder.rs:37–66`; consumers `server/.../gateway/ingress.rs:390–398,451–511,519,565–575`.

Run → registry correctly produces ModelRef(deployment, Prompt model) → public caller drops deployment in Anthropic body and Google resource. Actual ingress requires the full address. Bare models fail; slash model prefixes can be interpreted as another deployment. A Google deployment named `vertex` is falsely refused by its spelling. Actual Vertex refusal already exists at `observe/invoke.rs:94`. Jev fails even earlier: run_prompt extracts messages through loop validation, which rejects JevSystemOne. If reached, public projection/response decoding still lacks Jev. The server caller has a Jev projection and retains ModelRef separately; raw HTTP T10 manually supplies correct addresses, proving another path.

**Correction:** repair this existing public projection and its stale contract mocks: full addressed Anthropic/Jev body model; deployment plus correctly encoded complete Google suffix; Jev typed response/error handling. Complete the supported single-call Jev Agent path at the current Agent owner without adding general Jev tools/streaming or another loop. Remove name-based provider inference and retain actual local Vertex refusal. Preserve transport/auth/correlation/cancellation/redaction and server caller behavior. **Closure:** P4 and P5 extended to real named Anthropic/Google/Jev Agent calls, slash suffix and valid Google deployment named vertex, plus actual Vertex refusal before IO. Supporting mocks alone cannot close ingress parity.

### FIND-001-4 — Important — CONFIRMED — INCORRECT: portable bindings erase their required media kind

**Obligations:** REQ-006/008; AC-006. **Locations:** `crates/skald/skald-spec/src/authoring.rs:386–436`, especially 413–426; `src/prompt.rs:346–373`.

Lowering knows image/document but preserves only a placeholder string for bindings; Prompt retains names, not required kinds. The shared binder chooses the supplied MediaRef kind, so image→document and document→image silently change request meaning. Literal compilation, Python Prompt binding and judge record binding converge here (judge.rs:129); native placeholders have no portable declaration.

**Correction:** preserve the portable expected-kind invariant as derived runtime information on the existing Prompt/compiler/binder owner and check it before request mutation; refuse conflicting declarations for one name. Keep authored JSON unchanged and native untyped placeholder behavior working. No separate resolver or per-language validation. **Closure:** P6 extended with both mismatch directions, matching ordered content and repeated/conflicting names across supported portable providers; add exact public Python binder regression with no provider IO.

### FIND-001-5 — Important — CONFIRMED — MISSING: output schemas are copied without schema validation

**Obligations:** REQ-006/007/008/011; AC-006. **Locations:** `authoring.rs:292–322,360,518–526,608–612,667–668`; `skald-spec/src/prompt.rs:209–222`; `wyrd-spec/src/card/prompt/validate.rs:232`; `skald-prompt/src/builder.rs:278–287` and `coerce.rs:8`.

Portable response_format and native schema fields decode only to JSON containers. A schema with `type: not-a-json-schema-type` is copied into protocol output fields. Prompt::new derives metadata; PromptSpec decoding/validation checks variable names, not schemas. Server Card validation uses that decoder, so both standalone and inline authored registration accept the invalid output declaration. Builders' schema_object checks object shape and finalize_prompt calls Prompt::new; Agent output parsing occurs after IO and is not an authoring validator.

**Correction:** validate schema syntax and statically detectable destination incompatibility in the existing pure shared acceptance owner, reused by native/portable compilation and runtime builder finalization. Preserve valid schema bytes/name and native extension options, refuse with field-specific structured errors, and do no network lookup. **Closure:** P6 plus focused existing public registration owner extended with invalid type/required/properties, supported nested schema, detectable unsupported destination feature and builder parity; refusal before storage/provider IO. Record exact added selector, without another harness.

### FIND-001-6 — Important — CONFIRMED — INCORRECT: initial Agent target accepts direct-client translations prohibited by its compatibility matrix

**Obligations:** REQ-010 initial-target matrix; INV-007; AC-005/008. **Locations:** `crates/skald/skald-agent/src/gateway.rs:35–44,78–94`; client `observe/invoke.rs:93–109`; server `gateway/caller.rs:110–143`; adapter `crates/wyrd/wyrd-gateway/src/adapter/mod.rs:205–224`.

Gateway call carries native request and address but no enforceable Prompt-provider initial-target constraint. Both callers enter ordinary gateway representability, which intentionally allows OpenAI Chat→Anthropic/Gemini/Vertex and Google→Vertex. A routed OpenAI Agent can therefore dispatch to incompatible initial Anthropic/Vertex; checking whether the Prompt itself is Vertex does not prevent that. Server judges share the same seam.

**Correction:** carry the existing typed Agent origin/provider compatibility into the ordinary initial-target admission decision in public and server Agent paths, enforce against the resolved adapter before IO, and leave direct-client translation and explicitly configured fallback conversion intact. No admin-read requirement, global adapter translation ban, new route type, or reserved deployment names. This enforces an already approved matrix; private carriage is implementation-owned, not a new product decision. **Closure:** extend P5 with incompatible initial targets and compatible positives, then prove direct-client conversion and authorized cross-adapter fallback still work in their existing owners. Exact added selectors must be recorded.

### FIND-001-7 — Important — CONFIRMED — REGRESSION: batch visibility filters after LIMIT and can prevent pagination

**Obligations:** REQ-002; AC-002. **Locations:** `server/.../gateway/batches.rs:168–197,575–617`; SQL `crates/wyrd/wyrd-sql/src/queries/gateway.rs:1000–1018`.

All-only returns `eligible=None`, SQL takes limit+1 without access pruning, then list hides Granted rows and derives last_id from the remaining objects. Newer denied batches can produce empty data/null cursor/has_more=true indefinitely while older Open batches exist. Exact-scope SQL pruning and model listing are siblings; neither repairs this All-only list path.

**Correction:** use the existing tenant snapshot and scoped SQL eligibility owner to apply Open/Granted plus effective explicit-grant coverage before LIMIT; retain a bounded read, tenant RLS and authoritative audited decisions. Include All+explicit Granted reach after FIND-001-1 normalization is corrected. Do not unboundedly scan or expose hidden rows/IDs as objects. **Closure:** P7 with denied newer batches, visible older batches, small pages, usable continuation/has_more, exact model/deployment and mixed grants, tenant isolation.

### FIND-001-8 — Important — REVISED — INCORRECT: rendering can substitute the model identity

**Obligations:** REQ-005/008; INV-004/007; AC-006. **Locations:** `skald-spec/src/authoring.rs:214–225`; `prompt.rs:209–222,324–328,382–415`; `skald-prompt/src/coerce.rs:16`.

Only nonempty model is checked. OpenAI/Anthropic/Jev body models get the authored string; Prompt scans all request leaves and bind_mut replaces all string leaves. `{{target}}`/`${target}` therefore become variables and render into another native target while the declared model remains templated. Native execution bypasses gateway ModelId validation. Runtime builders use checked_model with the same emptiness-only rule.

**Correction:** validate model as literal configuration at the shared pure identity/construction boundary before variable derivation; reuse that rule from builders/setters. Reject placeholder syntax and invalid controls, preserve valid slash model identifiers, and keep target identity out of content substitution. No model catalog, aliases, model renaming or restrictive gateway-prefix interpretation. **Closure:** P6 plus exact native render/public authoring negatives for both forms and placeholder grammars; content variables still render and slash literals remain identical.

### FIND-001-9 — Important — CONFIRMED — INCORRECT / REGRESSION: durable trace provider is the arbitrary deployment name

**Obligations:** REQ-001 adapter/deployment distinction; telemetry-observations signal meaning. **Locations:** `server/.../gateway/capture.rs:921`; producer `wyrd-gateway/src/engine.rs:337–350,494–505`; correct sibling live mapping `engine.rs:673–683`.

Engine AttemptRecord retains deployment/model but loses admitted adapter identity; captured otlp_span uses `fields.model.deployment` for gen_ai.provider.name. Named `openai-primary` thus becomes provider `openai-primary`; live span correctly uses adapter.builtin_provider or openai_compatible. CallCapture::spans_batch→project_spans→ScribeOutbox retains the wrong value in vala.traces.spans, including fallbacks/external names. Provider-grouped verification/query results become false.

**Correction:** retain actual per-attempt provider identity from admitted adapter in the existing engine/capture facts and use the same mapping as live spans. Keep separate exact deployment/model fields and fallback identities; do not re-read mutable configuration after dispatch or infer provider from names. **Closure:** extend existing capture projection unit owner with differently named same-adapter deployments and a fallback adapter, asserting retained provider and deployment separately; P8 is the existing surrounding real capture proof. Record exact added unit selector.

### FIND-001-10 — Important — CONFIRMED — INCORRECT: batch preauthorization crosses a new routing snapshot

**Obligations:** REQ-002/010; INV-007. **Locations:** `server/.../gateway/batches.rs:410–439,486–498,737–789`; `gateway/invocation.rs:481–488`.

Create authorizes snapshot A, waits for SQL claim/commit, then settle calls run with authorized=true. Replay refresh also passes true. admit_call loads snapshot B and skips decide, routing credentials/access from B. A concurrent Open→Granted PUT between claim and dispatch therefore permits an All-only caller to dispatch/refresh using a Granted B target; the only audit records permission against A. Normal calls use one owned snapshot and are unaffected.

**Correction:** bind the authorization verdict to the same admitted snapshot used for routing/credentials, reusing GatewayInvocation admission. The minimal correction can reauthorize at that owner rather than treating an unrelated prior boolean as authority. Retain any early authorization needed to avoid unauthorized claims; preserve fresh-claim release on undispatched refusal, uncertain-dispatch fencing, completed replay and provider-state pinning. Do not serialize all administration or revoke state from calls already admitted consistently.

**Closure:** P9 with deterministic existing claim seam/database barrier: authorization A completes, hold claim, replace access, resume; assert denial/audit before IO and no stranded undispatched claim. Cover completed replay and scoped success. No sleeps, synthetic load, or second lifecycle owner.

### FIND-001-11 — Important — REVISED — MISSING: new portable media accepts invalid payload/MIME/source combinations

**Obligations:** REQ-006; AC-006. **Locations:** `authoring.rs:227–228,428–436`; `skald-spec/src/prompt.rs:940–1130`; existing budget `skald-prompt/src/media.rs:16,27–65`.

Literal blocks become MediaRef and enter binder helpers. Base64 is not decoded/bounded; MIME checks are emptiness-only for OpenAI/Google, absent for Anthropic. An image with application/pdf and `%%%` therefore compiles into an image request. Google File accepts arbitrary uri while URL checks GS/Gemini file API forms. Plain serde MediaRef adds no validation. Path helpers' 20 MiB local-file budget and extension checks do not validate literal/raw base64 entrypoints.

**Correction:** validate source data at the existing shared pure media/binder boundary used by literal compilation and runtime bindings: base64 validity and existing size budget, kind-compatible/provider-supported MIME and accepted Google URI forms. Reuse current media constraints and existing installed base64 facilities; place shared pure facts at the lower owner as necessary, without adding a specialized dependency upward or inventing upload/fetch/OCR behavior. Preserve provider-native file IDs and supported existing combinations. **Closure:** P6 plus focused public binding/authoring negatives for malformed/over-budget data, wrong/empty MIME and unsupported Google File URI; matching image/document sources retain order and dispatch support. This is not a general audit of old media conveniences.

### FIND-001-12 — Important — REVISED — VIOLATION: changed Rust items fail the explicit rustdoc acceptance gate

**Obligations:** AGENTS §16 and agent-rules hard documentation criterion. **Primary locations:** `skald-spec/src/authoring.rs:273,328,386,451,480,561,625,701,717,734,745,764,798`; `skald-spec/src/prompt.rs:60,73,81,90,94`; `request.rs:92,135–160`; `wyrd-spec/src/card/prompt/spec.rs:59`; `vala-eval/src/orchestrator/judge.rs:137`.

Private fallible compiler stages omit Errors; serde/schema methods omit method docs; judge agent_for omits item documentation for its changed resolution/cache/route refusals. These execute through both Card decoding and runtime/judge consumers. Test/helper/associated-item omissions independently violate the same explicit gate; no runtime bug is inferred from missing prose.

**Inventory validation:** Checked REP-001's expanded per-file function/line inventory against candidate definitions and added lines inside each complete function body in cumulative.diff, including unchanged signatures with changed bodies. All listed functions have candidate body changes; signature proximity alone would wrongly exclude loader parse tests, loop helpers, Agent contract tests, adapter dispatch and judge preparation. Retain that expanded inventory as part of this finding, plus its separately identified ProviderName::Err, new compiler/Prompt test modules and function-local BACKUP_KEY/EXTERNAL_KEY/MODEL constants. MAINT-001's Panics omissions apply to new compiler helpers/tests and ScriptedGateway/tool-loop test too. Do not duplicate this into individual findings or treat untouched neighboring helpers as a blanket documentation project.

**Correction:** substantive in-place documentation on that changed item set, error conditions for fallible methods, remaining panics and relevant async cancellation/partial progress/cache behavior. Recheck the actual remediated changed Rust bodies rather than assuming the sample list is exhaustive. Keep existing documentation that already satisfies the rule (e.g. ProviderName::from_str). No new checker, abstraction or runtime change. **Closure:** source audit against §16 and owning formatting/lint commands; behavioral tests unnecessary solely for documentation. Batch with substantive corrections, never a style-only round.

### FIND-001-13 — Important — REVISED — MISSING: AC-004 preservation proof is incomplete

**Obligations:** AC-004, task scenarios 7/12, AGENTS user-journey rule. **Locations:** `sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs:485–557`; Python `test_gateway_inference.py:181–230` and `test_register_and_hydrate.py:163–204`; TS `gateway-inference.test.ts:387–435`, `register-and-hydrate.test.ts:144–183`.

T07 compares authored portable/native standalone and portable-inline GET/hydration JSON, GET variables and replay refs. It omits native-inline and explicit authored hash comparison/introspection after hydration. Python/TS T14/T15 execute two standalone Prompt-backed Agents and compare route; they do not assert authored Prompt JSON/hash/derived metadata preservation. Existing navigation tests cover inline/path/ref identity but not the two-form matrix. Pure hash unit tests and the source's authored codec/hash owner support correctness but cannot prove the language/register/hydrate boundaries. INVREV's AC-004 PASS overstates recorded selectors.

**Correction:** extend the existing register-and-hydrate owners/fixtures in each SDK with both authored forms standalone and inline materialized Prompt Cards; compare GET/hydrated authored JSON and the existing public/retained hash representation, and derived variables/response-schema introspection with no duplicate authored fields. Use the already available public representation or canonical computation in tests; this finding does not require adding a production accessor, hash field or new harness. Add only missing Rust assertions/context. **Closure:** P10 and exact focused Python/TS preservation selectors recorded after extension. No route/override matrix rerun solely for this proof gap. No storage/hash corruption is established.

## Focused closure commands at existing owners

Commands below are recommendations, **not executed results**. Extend the named owner to exercise the described missing case; existing success alone does not close a finding. A new regression in an owner must also record/run its exact selector after discovery. Do not run broad journey aggregates during remediation; the integrated gate belongs to change review.

```sh
# P1: permission normalization/meet supporting cases (FIND-001-1)
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::gateway_permissions_are_typed_and_scope_checked) | test(=permission::tests::intersection_keeps_only_the_narrower_shared_authority)'
# P2: real issuance/delegation owner; extend with gateway authority and record its exact new selector
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::an_exchange_names_subject_and_actor_and_carries_only_the_intersection)'
# P3: reusable bounded Agent, extend to load declared route
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(=agent_journey::execution_route_isolated_through_tool_loop)'
# P4: shared native protocol producer
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=gateway_caller::public_gateway_call_context_and_errors)'
# P5: existing real Agent route journey
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::agent_routes_and_pre_run_overrides)'
# P6: pure authored compiler/media/schema owner
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_content_settings_and_schema)'
# P7: bounded visible page owner
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_batch_listing_is_one_bounded_pruned_read)'
# P8: surrounding capture regression; add a focused capture projection selector
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_observations_are_bounded_correlated_and_secret_free)'
# P9: existing batch claim/uncertain-dispatch regressions; add exact deterministic interleaving selector
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_batches_converge_replays_on_one_upstream_batch) | test(=components::gateway::pg_invocation_tests::gateway_batch_creations_release_claims_only_without_dispatch)'
# P10: authored register/GET/hydrate proof
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=register_and_hydrate::prompt_forms_round_trip_as_authored)'
```

Python runtime negative/preservation proof belongs under existing public Prompt and register-and-hydrate pytest owners through `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest ...`; Node proof belongs in existing register-and-hydrate Vitest owner through `mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run ...`. Select the actual added test names exactly and record them. No invented test names are acceptance evidence. Documentation uses `mise run fmt` and owning lint checks; source/public-contract changes add only their applicable existing type/codegen checks.

## Reconciliation recommendation for all 32 obligations

PASS below means inspected implementation/recorded focused assertions support the bounded obligation, with no retained counterexample; it never means independently executed. FAIL identifies validated implementation or required proof gaps. Do not copy discovery PASS cells blindly.

| Obligation | Implementation/proof recommendation | Reason |
|---|---|---|
| REQ-001 | FAIL | Named targeting core exists; Agent projection and adapter/provider evidence differ, FIND-001-3/9 |
| REQ-002 | FAIL, decision required | FIND-001-1/7/10 |
| REQ-003 | PASS | Existing credential/admin owners; complete-file structural validation and ordered partial PUT, T06/T13 |
| REQ-004 | PASS | Separate credential provider, unchanged external model and screened transport, T05/T13 |
| REQ-005 | FAIL | FIND-001-8 |
| REQ-006 | FAIL | FIND-001-4/5/11 |
| REQ-007 | FAIL | Native shape/extension preservation exists, schema validation missing FIND-001-5 |
| REQ-008 | FAIL | Shared authored codec/hash/materialization exists; early validation/proof FIND-001-4/5/8/11/13 |
| REQ-009 | FAIL | FIND-001-2 |
| REQ-010 | FAIL | FIND-001-2/3/6/10 |
| REQ-011 | FAIL | Shared projections exist; runtime contract/validation parity FIND-001-2/3/5 |
| REQ-012 | PASS | Retired Card/runtime/jobs/routes/exports removed; independent Eval and Agent/judge owners preserved |
| REQ-013 | PASS | Existing installed CLI setup/fresh saved-auth TS/restart owners, T13 |
| INV-001 | PASS | Inspected tenant connection/RLS/snapshot/sealing/registry owners; no retained cross-tenant finding |
| INV-002 | PASS | Credentials remain outside Cards, bound/redacted; no Wyrd credential forwarding |
| INV-003 | PASS | Old shapes/weights/retired Workflow public capability removed without compatibility alias |
| INV-004 | FAIL | FIND-001-2/8 |
| INV-005 | PASS | Rust server decisions, reusable Skald, shared client projections retained |
| INV-006 | PASS | Existing nonblocking process ScribeOutbox remains write owner; no new durability promise |
| INV-007 | FAIL | FIND-001-1/2/4/6/8/10 |
| AC-001 | PASS with proof limits | Existing three-runtime gateway onboarding/addressing owners and recorded evidence; does not imply all Agent protocols work |
| AC-002 | FAIL | FIND-001-1/7/10; isolated grants do not prove mixed/delegated semantics |
| AC-003 | PASS | Resolved external target/credentials/revocation/screening evidence; distinct standard-provider metadata defect retained separately |
| AC-004 | FAIL proof, implementation preservation supported | FIND-001-13 resolves behavior/invariant disagreement |
| AC-005 | FAIL | FIND-001-2/3/6; manual registry T09 and OpenAI-only T08 cannot close full runtime contract |
| AC-006 | FAIL | FIND-001-4/5/8/11 |
| AC-007 | PASS for stated constrained judge paths | Both forms/structured result/subject/SYSTEM/native refusal owners retained; no claim that generic incompatible initial Agent targets are valid |
| AC-008 | FAIL | FIND-001-3/6; correct raw HTTP requests cannot prove the public Agent producer |
| AC-009 | FAIL | Runtime/public declarations disagree and proof incomplete, FIND-001-2/3/12/13 |
| AC-010 | PASS | Workflow removal and independent Eval fields/DAG retained; T12 served-contract owner |
| AC-011 | PASS | Real installed-CLI/fresh TS four-adapter saved-auth and retained evidence path exists, T13 |
| AC-012 | PASS | Single/list/no-write malformed/rerun/partial progress/restart lifecycle remains, T06/T13 |

Non-goals remain: no model renaming/aliases/catalog; no portable audio/video, new local Vertex ingress, hosted general Agent job API, duplicate orchestration runtime/media upload/credential system, deployed-data deletion or synchronous audit durability. Current stronger architecture materializes inline Agent Prompts as independent exact Prompt Cards; do not enforce the superseded private embedded-storage task mechanics. Preserve independent Eval task DAGs/observed-data fields. Approved adjacent release/protoc/swap/telemetry/query changes are not DRIFT merely due to adjacency. No new finding is justified by an unrelated old omission or reviewer preference.

## Authority decision and proof limits

The orchestrator should return SPEC_REVISION_REQUIRED naming FIND-001-1's mixed-intersection choice, while preserving this bounded ledger for subsequent implementation. It must not package attenuation/refusal as already approved exact-intersection remediation. Once authority chooses the outcome, implement one cohesive root-cause correction using current owners; other bounded findings need no new product decision. A new material decision is not inferred merely because implementation needs private carriage, runtime metadata or corrected projections explicitly required by r5.

Recorded T01–T17 and owner-check passes were reviewed as claims against their source assertions; they were not rerun and do not prove absent cases. No independent aggregate, served OpenAPI, MCP runtime, installed package, live provider, crash/restart or multi-pod execution was performed. Snapshot navigation initially encountered untrusted mise shims; read-only source inspection used standard tools afterward, with no trust/source modification. Cumulative scope and changed items were inspected for claim applicability; this is static source validation, not execution certification of every diff line. No absent runtime evidence is invented. The required integrated `mise run gate` remains change-review work, not an extra remediation aggregate.

Skipped tests/builds and all source edits as required; material residual risk is delegated Granted escalation plus unsupported/misrouted Agent execution until corrected under approved authority.
