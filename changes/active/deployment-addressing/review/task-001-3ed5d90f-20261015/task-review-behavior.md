# Independent behavior review — TASK-001, remediation round 3

## Subject and method

Result: **FAIL**. This is an independent caller-to-result acceptance audit of base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` → candidate `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`, read from `/tmp/wyrd-task-review-3ed5d90f`. The supplied cumulative and R2 fix diffs are the reviewed range and navigation aid respectively. No live source mutation, build, baseline execution, or test rerun was performed. Paths below are relative to that immutable source.

Authority: repository AGENTS and agent rules; approved deployment-addressing spec r6; original TASK-001 including the human-directed provider bundle stage; original review ledger/R1 and second review ledger/R2; spec-driven development and maintainer guidance; applicable architecture route/identity/gateway/outbox ownership. Prior conclusions were hypotheses checked against current producers and consumers, not proof by themselves. Other fresh reviewer conclusions and the owner's completion summary were not inputs.

Expanded navigation: named deployment and ModelRef contracts → CLI DeploymentFile and Gateway administration → immutable tenant snapshot and GatewayInvocation::decide/admit_call → CallPlan and admitted AttemptRecord → capture; PermissionSet normalization/intersection → token issuer; AuthoredPrompt portable/native lowering → Prompt constructor/binder/render/derived metadata → PromptSpec hash/register/hydrate → Rust/Python/TS projections; Agent from_card/from_yaml and route_registry → loop run_with/run_prompt → PublicWyrdGatewayCaller/native ingress and separate server judge caller; existing Cards graph hydrator → installed CLI's fresh Python/Rust children → retained Bifrost rows before/after restart; published Connect setup → existing framework request and retained-row tests. Existing owners continue to cover these workflows; no replacement graph/transport/runtime is necessary for either correction below.

## Acceptance matrix

PASS means source plus supplied focused proof supports the obligation within task-review scope; it does not claim this reviewer executed tests or the final change-review aggregate.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 | `wyrd-spec/src/gateway/{mod,deployment}.rs`: first-slash ModelRef, arbitrary named adapter/credential target, optional nonempty allowlists/capabilities; gateway CallPlan deterministic target | Rust/SDK gateway admin/inference named/slash/default/restriction tests; original T01/T03–T06 evidence | PASS |
| REQ-002 | `wyrd-runtime/src/permission.rs:305–313,882–1000` preserves explicit grants and exact fallible intersections; `gateway/invocation.rs:706–760` uses effective principal, Granted object scopes and stages each decision; batch list scopes before SQL bound | Existing permission merge/intersection and real exchange tests; gateway subject/actor/visibility/fallback tests; R1 evidence | PASS |
| REQ-003 | CLI `gateway.rs:356–477` validates all list entries before writes, uses ordinary ordered PUT and truthful outcomes; credential/admin existing owner retained | Installed CLI malformed/duplicate/missing credential/reapply/single/list test and CRUD SDK journeys | PASS |
| REQ-004 | Existing compatible adapter, credential resolver and HTTP dispatch retain separate credential identity, unchanged model suffix, pinning and header isolation | External deployment and revocation/header journeys; installed CLI upstream assertions | PASS |
| REQ-005 | Authored top-level provider/model; `Prompt::new:336–352` refuses typed body mismatch before deriving variables; authoring refuses body.model; `check_model` literal/slash checks | R2 P7 direct typed constructor matrix; original authored-negative journeys | PASS |
| REQ-006 | Shared portable settings/content/media/schema lowerers exist, but synthetic literal-media identifiers collide with accepted authored bindings, silently replacing caller-supplied content | Existing portable multimodal test uses ordinary `photo`, not colliding supported name; B-02 below | FAIL |
| REQ-007 | Provider/format closed matrix and typed native bodies; authored body.model/mixed fields refused, extension fields retained; output schema uses offline standard metadata validation | R2 P2–P7 and original generation-pair/options/negative tests | PASS |
| REQ-008 | Authored spec JSON/hash persistence and shared compiler remain intact; derived media variables can be erased by compiler collision; TS nonempty media projection lacks requested journey proof | Four-form Rust/Python/TS register/GET/hydrate tests; B-01/B-02 | FAIL |
| REQ-009 | `Agent::from_card:674–699`, `to_spec:784–793`, per-invocation route registry and existing loops preserve declared route and isolated effective route | Agent saved YAML/tool-loop isolation; SDK default/authorized-denied override tests; R2 actual downloaded loader child | PASS |
| REQ-010 | Public caller `gateway_caller.rs:102–114` refuses actual Vertex provider before projection/IO, typed provider header reaches initial-adapter constraint; native ingress retains address, server caller separate | R2 P1; named native ingress and Jev single-call tests; override/tool/cancellation regressions | PASS |
| REQ-011 | Thin SDK projections/errors/catalogs/generated contracts and docs corrected; required TS media metadata proof remains incomplete | Existing Python/TS type/export/codegen/served OpenAPI evidence; B-01 | FAIL |
| REQ-012 | CardKind/Spec and live client/server/CLI/SDK retirement remove Workflow product; reusable routing/loops/judges survive under Agent owners | Removed-kind journeys, catalog/OpenAPI tests and retained independent Eval paths | PASS |
| REQ-013 | Existing dev bootstrap, credential command, one deployment list and installed setup/restart stage; four actual downloaded Agent instances now loaded and run | R2 P9 real installed CLI → Python driver → Rust loader child, same bundle after restart, retained correlated rows | PASS |
| INV-001 | TenantConn/RLS snapshot/admin/registry/capture ownership retained; shared client uses authenticated tenant | Existing tenant-separated gateway/model/batch tests and tenant checks | PASS |
| INV-002 | Existing credential sources/keyring/provider binding and endpoint screening; no secrets on Cards; upstream headers isolated | Credential redaction/revocation/destination tests and installed journey header checks | PASS |
| INV-003 | Removed legacy surfaces have no compatibility decoder/alias/model mapping/weighted replacement | Contract negative/codegen/catalog/retired Workflow proof | PASS |
| INV-004 | Prompt authored identity unchanged by Agent route override; route stored on exact Agent, invocation effective route local, actual attempts recorded | Concurrent default/override SDK and tool-loop tests; retained evidence | PASS |
| INV-005 | Pure Prompt/Spec owners remain synchronous; server durable auth/admission/storage and shared client/SDK projections retain boundaries | Dependency/codegen/type lanes and source owner tracing | PASS |
| INV-006 | `GatewayInvocation::decide` stages on existing outbox; no new synchronous audit dependency or writer | Source plus original outbox failure tests; audit writer unchanged | PASS |
| INV-007 | Gateway without caller fails before native credentials, route registry bounds all model iterations/callbacks, overrides authorized at actual target | Native trap/tool-loop/missing caller and denied override journeys | PASS |
| AC-001 | Three-language deployment/gateway owner paths support unrestricted/restricted/same-adapter targets and admitted evidence | Existing real SDK fixtures, original T03–T06/T14–T15 evidence | PASS |
| AC-002 | Exact shared authority and Open/Granted decisions retained, subject/actor separation, no requested denial rescue | Permission/exchange and real gateway authority/fallback/visibility tests | PASS |
| AC-003 | Compatible deployment and credential/header/screening owners unchanged by R2 | External credential/model/revocation journeys | PASS |
| AC-004 | All four forms preserve server/hydrated authored JSON/hash, TS text/schema metadata now projected, but nonempty media metadata not exercised | TS `register-and-hydrate.test.ts:202–223` only checks `mediaVariables == []`; B-01 | FAIL |
| AC-005 | Required route and invocation overrides implemented through existing runtime/shared client; same Prompt native/named/external supported | Rust/Python/TS route journeys, agent loop/timeouts/denial, R2 P9 | PASS |
| AC-006 | Standard schema and literal typed model corrected; portable collision can silently use wrong media instead of caller input | Existing multimodal/binder negatives miss colliding legal binding; B-02 | FAIL |
| AC-007 | `vala-eval/src/orchestrator/judge.rs` resolves same Agent/Prompt route and refuses server-native route/missing gateway; shared server caller attribution preserved | Original realtime judge/structured output/dependent assertion journeys | PASS |
| AC-008 | Public/native named Anthropic/Google/Jev protocol handling and explicit local Vertex refusal now cover actual public caller | R2 P1 and original native ingress/Agent route tests, server/native Vertex retained | PASS |
| AC-009 | SDK metadata projection implemented; docs/capture prerequisite/public examples corrected; remaining TS media proof | R2 P8/P10 and codegen/docs/OpenAPI evidence; B-01 | FAIL |
| AC-010 | Workflow product removal remains across registrable kinds/routes/SDK/CLI/UI while Eval DAG remains unrelated | Original removed-kind/headless evidence and cumulative source | PASS |
| AC-011 | One dev start, tenant, saved auth, four credentials/deployments/direct and actual loaded Agent runs/evidence | Installed CLI P9, before/after restart | PASS |
| AC-012 | CLI list structural rejection/partial failure/convergence, same sealing config and tenant reuse survive restart | Original CLI file tests and installed P9/revocation checks | PASS |
| Human FIND-14 / R2-AC-05 | Actual `Agent::from_card` receives downloaded Card from validated bundle, pinned downloaded Prompt resolver; same Agent inspected then `run_prompt` with only host caller attached | New `local_development::cli_gateway_agents`, parent runs it both lifecycles; deliberate route-drop experiment recorded honestly | PASS |
| R1-DOC-AC-01 | Connect setup enables metadata capture using existing capture-policy command before first request | Current page step5 plus P9 same procedure | PASS |
| R1-DOC-AC-02 | Existing stock/framework requests use saved Wyrd auth; both framework tests now read their own fresh server's retained rows | R2 P10 and capture-removal mutation evidence | PASS |
| R1-DOC-AC-03 | Existing Connect navigation/index structure retained; requested desktop/narrow/theme observation recorded with iframe/theme limitations | docs:check + recorded Chrome production preview observation, no inferred visual defect | PASS |
| Non-goal: no hosted Agent job/runtime replacement | Existing local bounded Agent/runtime and server judge boundary retained | No new job API/runtime in cumulative owner paths | PASS |
| Non-goal: no model alias/remapping/discovery | ModelRef removes only deployment prefix; no catalog/model rewrite owner introduced | Slash/native/external forwarding fixtures | PASS |
| Non-goal: no universal external auth/protocol/cloud provisioning | Existing compatible adapter/credential source scope retained | Existing adapter/credential owners and tests | PASS |
| Non-goal: no Prompt embedding/predict/raw formats | Closed public authoring formats, non-Prompt request support retained internally | Pair/format rejection tests | PASS |
| Non-goal: no portable audio/video/local Vertex gateway | Portable types are text/image/document; local actual Vertex rejected; native/server capabilities remain | R2 P1 and original portable/native tests | PASS |
| Non-goal: no new secret/media fetch/queue/harness | Existing credential resolver, media binder, Cards hydrator, outbox, installed test process seams reused | Source owner comparison | PASS |

## Material proposed findings

### B-01 — retain FIND-001-13 — MISSING verification

**Obligation:** REQ-008/011, AC-004/009 and the explicit R2 FIND-13 focused closure require derived media introspection, including portable media names, through the real hydrated TypeScript object.

**Location:** `sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts:202–223`; R2 evidence row explicitly states no fixture has `${media:...}` and only an empty list is checked.

**Trace:** Rust Prompt derives separate text/media names → N-API `cards.rs::hydrated_prompt` serializes `prompt.media_variables` → TS `promptCard` maps `media_variables` to `mediaVariables` → standalone and Agent-navigation callers. That implementation looks correct and is the proper thin projection; the retained issue is proof, not a missing API or data corruption claim. Every current portable/native case expects `[]`, so an implementation which always returned `[]` would pass the new test while violating a required public field. Existing pure Rust media/binder tests cannot cover this N-API/TS handoff.

**Minimal correction:** extend the same real register/GET/hydrate TypeScript journey with a portable Prompt containing actual named image/document bindings and assert their exact nonempty derived names through standalone and Agent navigation, while retaining original authored spec/hash and text/schema assertions. Reuse shared Rust derivation and current SDK projection; add no parser, endpoint, harness, stored metadata or new production accessor. Preserve plain-text null schema and existing empty-media behavior.

**Focused proof:** same R2 P8 selector, `mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'`, with existing normal native prerequisites. Test body must expect actual nonempty image/document names. Only fixture/test write-set type/lint checks are needed if production code remains unchanged.

### B-02 — new finding candidate (next stable ID FIND-001-16) — INCORRECT portable media compilation

**Plain behavior:** A valid caller-supplied media binding can silently become a literal attachment already in the Prompt because the compiler uses the same name for its own internal placeholder.

**Obligations:** REQ-006 requires ordered content and typed media bindings, REQ-008 requires correctly derived runtime media variables, AC-006 forbids silent conversion/content loss.

**Exact producer/consumer source:** `crates/skald/skald-spec/src/authoring.rs:215–234` and `449–487`; `crates/skald/skald-spec/src/prompt.rs:513–560,988–1017`.

**Reachable counterexample (ordinary portable authoring):**

```yaml
provider: openai
model: gpt-4o
messages:
  - role: user
    content:
      - type: image
        source: '${media:__wyrd_media_0}'
      - type: image
        source: {type: url, url: 'https://example.com/fixed.png'}
```

The public media-name regex accepts the authored `__wyrd_media_0`. `PortableMessage::lower` records its image expectation and emits its token. The next literal source independently allocates `format!("__wyrd_media_{}", literals.len())`, which is also `__wyrd_media_0` for the first literal, and emits the identical token. `AuthoredPrompt::compile` calls `bind_media_mut` for literals before assigning `prompt.media_kinds`. The existing OpenAI binder visits every matching token and replaces both with the fixed URL. It removes that name from derived `media_variables`, so rendering succeeds without the caller supplying the requested image. The authored JSON/hash still shows the unbound source while effective execution uses the fixed attachment twice. A document binding plus literal image additionally defeats the declared kind because expectations are attached only after literal binding. Anthropic and Google sibling binders also replace every matching token, so this is one compiler-produced identity collision, not a separate provider bug.

**Why existing owners matter:** the ordinary binder intentionally replaces repeated occurrences of one named user binding; changing that to first-match would regress legitimate repeated bindings. The invalid shared identity originates in portable lowering. Prompt compiler/binder and all register/inline/load/SDK consumers use this shared path; no downstream registration, SDK, or provider guard is appropriate.

**Decision-complete minimal correction:** keep internal literal-media names disjoint from all authored placeholders before sending the lowered request through the existing binder, so literal substitution cannot consume caller-controlled text/media bindings. Preserve currently accepted binding names (including leading underscores), repeated user binding semantics, content order, shared MIME/kind validation, authored JSON/hash, and native placeholder behavior. Do not reserve a new public prefix, add another compiler/binder, fetch media, or fix each provider separately.

**Focused closure proof:** extend existing `authoring::tests::portable_content_settings_and_schema` with literal media mixed with a valid authored binding using the currently generated first literal name (including the case the authored binding appears later in another turn). After compilation, the literal is bound once, the authored image/document name remains in `media_variables`, and render without that required input refuses. Binding the caller input places the distinct supplied source at the authored position and preserves the literal source. Check built-in OpenAI/Anthropic/Google mappings through this same shared owner. Use `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_content_settings_and_schema)'`. Source trace above establishes reachability; no execution or fabricated RED is claimed by this reviewer.

## Prior-finding closure

| Prior ID | Current source/evidence assessment |
|---|---|
| 1 | CLOSED: `Permission::subsumes` preserves explicit grants beside All; complete fallible intersection and issuer error prevent unrepresentable delegated authority; actual merge/exchange tests cover representable mixed/empty results. No new empty-intersection policy. |
| 2 | CLOSED: actual Agent loader copies execution_route; to_spec retains it; route_registry and both public run entry owners dispatch through bound caller or fail closed. Actual loader now exercised by installed child. |
| 3 | CLOSED: body/path ingress retains deployment; public caller additionally refuses actual Vertex provider before IO, including override. Server/native Vertex separate and unchanged. |
| 4 | CLOSED at original caller kind mismatch: derived media_kinds retained and bind checks kind before mutation; conflicting explicit user declarations refused. New synthetic name collision B-02 is a different producer defect. |
| 5 | CLOSED: hand structural walk removed; bundled standard JSON Schema meta-validation selects supported declared draft, refuses unknown meta-schema before resolution, preserves strict OpenAI root check and never fetches user refs; builder and registration reuse owner. |
| 6 | CLOSED: typed Prompt provider reaches initial admission compatibility without changing ordinary direct-client translations or explicit fallback conversion. |
| 7 | CLOSED: batch list computes Granted exclusions and exact explicit scopes from snapshot before bounded SQL LIMIT; ordinary audited decide afterward uses same snapshot. |
| 8 | CLOSED: Prompt::new checks all body-model native variants against literal canonical model before derivation; GenerateContent body-less target retained. Public authored body.model still refused even equal. |
| 9 | CLOSED: AttemptRecord stores actual admitted adapter provider separately from deployment; live and retained provider mapping share that fact rather than mutable deployment spelling. |
| 10 | CLOSED: actual admit_call owns snapshot and always evaluates requested authority there; early batch claim check does not bypass dispatch admission; fresh no-dispatch release and uncertain fences/replay retained. |
| 11 | CLOSED at raw source boundary: shared check_media_source bounds/decodes base64, validates MIME/kind/provider support and Google file URI; literals/runtime binding converge before replacement. B-02 does not justify reopening these source-validation checks. |
| 12 | CLOSED: prior complete rustdoc correction inventory is represented in cumulative source and supplied checks; no new missing-doc finding from this behavior review. Standards reviewer owns full new-item gate. |
| 13 | PARTIAL: both N-API paths now expose actual shared derived metadata and four authored forms preserve JSON/hash; B-01 retains only explicit nonempty media projection proof gap. |
| 14 | CLOSED, human-directed provenance retained: actual public Rust Agent::from_card loads downloaded card with downloaded pinned Prompt; no rebuilt Agent/route setter/provider override. Same object executes run_prompt before/after server restart; separate Python child still covers Card-scope refusal. Its 403 is honestly Card-scope, not role denial. |
| 15 | CLOSED: canonical setup enables metadata capture before request; framework pages inherit it; function-scoped fresh server tests read normal retained rows belonging to their requests. Reported rendered check states iframe/theme limits honestly. |

## Owner-raised Agent save/hash question

`Agent::from_card` resolves `AgentRunConfigSpec` through runtime defaults (`agent.rs:844–858`), and `to_spec` always projects effective defaults (`833–840`), so an omitted tool_concurrency_cap becomes Some(8); omitted max_iterations similarly becomes Some(default). This is real lossy omission preservation, and generic verified_by is also not retained by this runtime projection. It should be tracked as a separate Agent authoring/round-trip issue rather than silently described as lossless serialization.

I do **not** propose it as a task blocker: original REQ-008/AC-004/Scenario7's authored-form/hash promise is specifically Prompt forms; Agent requirements preserve declared route, pinned Prompt identity, invocation override isolation and actual target evidence. R2's selected loader proof requires those fields and actual execution, which the candidate now performs. The `agent_run_config` conversions were not changed to introduce this normalization in the cumulative executable diff; this audit does not widen task scope to unrelated general Agent projection debt. Narrowing the new test's initial whole-spec assertion to the approved loader obligations was recorded with the actual mismatch and did not weaken a previously passing required contract. No specification revision or new behavior decision is necessary to complete the retained task findings.

## Verification limits

Only static source/call-path and committed test/evidence review was performed. Supplied R2 focused green proof, the public caller behavioral RED, typed-model/schema counterexamples, route-drop and capture-removal mutations, native extension rebuild diagnosis and rendered observations are assessed as reported evidence; no passing run is newly asserted here. The stale Python extension diagnosis is consistent with an unchanged test and rebuilt binding producing the current known-code behavior; `uv run` is not itself a native rebuild. Missing historical runtime RED does not independently negate otherwise credible current source proof. The final integrated `mise run gate` remains change review's responsibility, not a new remediation aggregate. No production code, test assertion, check, deployment, merge, push or secret was changed during this audit.
