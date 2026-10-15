# Independent invariant review — TASK-001, R3

## Subject and method

Result: **PASS**. No proposed blocking finding.

Reviewed immutable archive `/tmp/wyrd-task-review-3ed5d90f`, base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` to candidate `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`. Latest remediation diff from `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23` locates changes; full original task remains the acceptance subject. Source was not edited. This report is written outside the archive. Archive has no CodeGraph index; root exploration was recorded in the supplied subject.

Authorities read: AGENTS.md, agent-rules, spec-driven-development, maintainer-style, applicable design/doctrine references and Bifrost outbox passages, approved deployment-addressing spec r6, original task, both prior verdict/validation ledgers, both remediation packets and appended implementation evidence. Review expands the shared map independently. The original task's r5 frontmatter is superseded by approved r6 and R1 delegation amendment, not a request to undo that amendment. Adjacent changes have explicit owner approval.

I followed authored Prompt values through compiler/normalization/binder/serialization/hash/registration/hydration; Agent routes through both runtime entrances, gateway caller, initial admission, attempt/capture; permissions through normalization/intersection/exchange/admission/listing; and downloaded bundle state through the installed driver and actual Rust loader. Verification below refers to committed evidence inspected against test assertions; **I did not run tests/builds**. Full aggregate remains integrated change review work.

## Existing-owner and abstraction comparison

| Changed mechanism | Existing owner / alternatives checked | Assessment |
|---|---|---|
| Portable/native compiler | skald-spec AuthoredPrompt compile, Prompt::new/bind_media/render; skald-prompt builders | Existing owners extended; no SDK assembly tables, second parser or provider runtime introduced. Stored authored form is compared against current compiled state, preserving public mutation behavior. |
| Output schema acceptance | shared check_output_schema; workspace jsonschema | Partial bespoke walker removed; standard offline bundled meta-validation now owns language validity. Destination object-root check remains separate. Dependency belongs to narrow skald-spec owner. |
| Route lifecycle | Agent::from_card/to_spec/route_registry; run_with/run_prompt; shared Run::invoke_on | Existing Agent loop and host gateway seam reused; route is copied once per invocation and runtime caller never serialized. Existing Run facade is retained rather than new execution infrastructure. |
| Gateway target/auth | ModelRef, ProviderDeployment, CallPlan, GatewayInvocation::decide/admit_call | Existing named target, planner and authorization service reused. Prompt provider constraint is attached only to Agent-origin admission, leaving direct-client translation/fallback behavior. |
| Batch listing/admission | GatewayBatches and gateway_batches tenant query | Pre-limit eligibility uses current snapshot and typed grants, then authoritative decisions/audit. Dispatch admission always owns authorization for its actual snapshot; no second grant cache. |
| Registered bundle proof | existing installed CLI, WyrdState, registered Prompt resolver, Agent::from_card, PublicWyrdGatewayCaller | Rust child exercises existing loader with host caller only. It does not create replacement Agent or own a parallel graph loader. |
| TS metadata | Rust Prompt derivation, hydrated_prompt shared N-API projection, PromptCard | Thin projection of existing derived fields; stored Card/spec hash remains separate. No new durable metadata. |
| Recording setup | existing CapturePolicy administration and Scribe outbox | Guide enables existing opt-in metadata capture; no default change/new writer. |

## Prior-finding closure from candidate source

| ID | Source and producer-to-consumer closure | Inspected proof | Result |
|---|---|---|---|
| FIND-001-1 | `wyrd-runtime/src/permission.rs:307–327,889–997`: subsumption retains explicit gateway grants next to All; intersection collects conditional open-only meets and rejects only uncovered ones. Existing auth issuance/exchange maps unrepresentable result before signing. Representable mixed/AnyOf/wildcard inputs retain exact authority. | `gateway_grants_survive_merge_and_intersect_exactly`, exact intersection and exchange tests; recorded focused reruns | CLOSED |
| FIND-001-2 | `skald-agent/src/agent.rs:674–701,784–791` preserves declared route; `loop_runtime.rs` resolves route_registry at both run_with/run_prompt entrances; missing host gateway caller is an error, not native fallback. `observe/invoke.rs:62–118` invocation override builds a private instance. | Existing route-isolation test; actual downloaded loader test now adds combined proof | CLOSED |
| FIND-001-3 | `gateway_caller.rs:108–122` rejects actual typed Vertex provider before projection/transport. NativeCall projects deployment/model into Anthropic/Jev bodies and escaped Google model resource, independent of deployment spelling. Separate server caller retains Vertex. | P1 explicit Vertex and override zero-transport cases plus Google-on-deployment-named-vertex success; P9 all providers | CLOSED |
| FIND-001-4 | `authoring.rs:218–230,422–475` keeps expected media kind and rejects conflicting names; `prompt.rs:521–530` checks supplied kind before replacement. | Existing portable media/settings regression | CLOSED |
| FIND-001-5 | `prompt.rs:232–267` checks schema object, refuses unknown dialect before resolution, invokes `jsonschema::meta::validate`, preserves error path and strict OpenAI root. Native constructor, portable compiler, builders and PromptSpec decode converge here; no user-reference compilation/fetch. | P2–P6 malformed nested keywords/draft-specific values, builder and real no-write registration; valid local refs retained | CLOSED |
| FIND-001-6 | `GatewayCallRequest.prompt_provider` reaches `invocation.rs:557` and `CallPlan::require_compatible_initial`. Initial target checked before retain_representable/direct translation; fallback policy remains separately authorized. | Adapter matrix/native ingress regressions | CLOSED |
| FIND-001-7 | `batches.rs:560–615` derives explicit grants/All and Granted names from one snapshot before bounded SQL query; final per-model decisions audit against same snapshot. | Bounded/pruned batch listing regression | CLOSED |
| FIND-001-8 | `Prompt::new` at `prompt.rs:336–351` rejects mismatched Chat/Responses/Anthropic/Jev body model before variable derivation; AuthoredPrompt rejects authored body.model and inserts canonical model. Model-less Google stays model-less. | P7 covers 12 direct-constructor mismatches, matching slash target and content rendering | CLOSED |
| FIND-001-9 | `wyrd-gateway/src/engine.rs:345,501,682–706` retains actual adapter provider separately; capture.rs:901–939 consumes that provider for gen_ai.provider.name, with deployment/model distinct. | Live/retained provider and fallback assertions | CLOSED |
| FIND-001-10 | `invocation.rs:455–520` snapshot load immediately precedes requested decision; batch dispatch cannot mark stale preflight as authorization. Fresh undispatched claim release and uncertain-dispatch fencing remain with existing batch owner. | Batch snapshot replacement/replay/release regressions | CLOSED |
| FIND-001-11 | `prompt.rs:1314ff` bounds/decode-checks base64, validates provider/kind MIME and file URI before any replacement. Literal portable media goes through same binder. | Source/MIME/base64/limit tests, portable media proof | CLOSED |
| FIND-001-12 | Previously enumerated inventory is documented in current owners; no remaining item identified by this invariant review. Dedicated standards review owns exhaustive rustdoc check. | R1 documentation inventory and R2 merged docs-only correction | CLOSED within this lens |
| FIND-001-13 | `native/src/cards.rs:620–665` projects Rust-derived values through shared hydrated_prompt for standalone and Agent navigation; TS PromptCard and promptCard at `src/index.ts:2358–2395` expose same metadata without spec mutation. | Four-form real register/GET/hydrate test `register-and-hydrate.test.ts:198–223` asserts JSON/hash, text variables, empty media list, schema/name for standalone/inline native/portable. Nonempty media derivation is already shared Rust behavior; direct projection is visible. | CLOSED |
| FIND-001-14 (human-directed) | `tests/integration/local_development.rs:70–137` calls WyrdState, pins downloaded Prompt, Agent::from_card, checks loaded route/ref/id/model, attaches public caller and executes **that** instance. Python installed driver at743–758 executes it before and after restart with same bundle; headers/evidence/native trap retained. | P9 current recorded pass and mutation proof removing from_card route restore; actual retained rows constrained by unique Run IDs | CLOSED |
| FIND-001-15 | Setup guide now applies existing metadata capture before first request. Framework tests at `test_client_http_auth.py:132–189` perform actual inference and query their function-isolated server's one retained call; fixture owns no earlier inference. | P10 passes and no-capture mutation fails; doc checks plus recorded Chrome desktop/narrow iframe/light/dark inspection | CLOSED |

## Cumulative requirement and acceptance matrix

PASS means source and inspected recorded proof establish this obligation; it does not claim a fresh execution by this reviewer.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 named deterministic addressing | ModelRef/deployment contracts, CallPlan lookup, native ingress preserve first-segment deployment/full suffix | Named same-adapter/model/capability/default tests and SDK gateway journeys | PASS |
| REQ-002 authority/attribution/visibility | PermissionSet exact meet, issuer, snapshot-bound decide, scoped discovery and batches | Merge/intersection/exchange, tenant visibility and denied IO regressions | PASS |
| REQ-003 onboarding | Existing credential sealing/auth owner; CLI DeploymentFile validates full list before ordered existing PUTs | Installed malformed/duplicate/reapply/partial failure and credential tests | PASS |
| REQ-004 external gateways | Existing OpenAiCompatible dispatch, separate custom credential provider, screening/pinning | External unchanged model/auth/redaction/revocation journeys | PASS |
| REQ-005 one literal provider/model | AuthoredPrompt check_model/provider validation and Prompt::new matching-model check | P7 plus authoring identity cases | PASS |
| REQ-006 portable content/settings | Existing shared compiler, expected media kind/source validator, eight explicit setting mappings, offline schema acceptance | Compiler settings/media/source/schema tests P2–P4 | PASS |
| REQ-007 native forms | Pair/format mapping, body.model exclusion, extension-preserving typed bodies, native schema check | Native format/unknown/mixed/model negative cases and builders | PASS |
| REQ-008 authored persistence/derivation | Prompt authored form serialization; PromptSpec canonical authored hash; shared Rust-derived TS/Python projections | Four-form three-SDK registration/GET/hydrate, P8 TS introspection | PASS |
| REQ-009 required Agent route/override | AgentSpec required typed route, Agent from_card/to_spec, immutable runtime selection | Native/gateway/external and concurrent default/override proof; P9 actual loader | PASS |
| REQ-010 execution resolution/refusal | Public caller rejects Vertex; actual adapter matrix; gateway admission authorizes every selected target; separate server/judge caller | P1, named ingress, tools/timeout/cancel, requested/fallback denial | PASS |
| REQ-011 typed public errors/consumers | Existing catalog error projection; SDK/native/CLI/MCP contract consumers updated | Served OpenAPI and SDK negative/error tests; codegen | PASS |
| REQ-012 complete Workflow removal | Card/catalog/runtime/routes/reference/SDK/CLI/UI removal, surviving Agent gateway seam and Eval DAG owners | Retired kind/route negative tests, surviving judge/Agent and contract catalogs | PASS |
| REQ-013 simple installed setup | Existing dev launcher/migration/saved auth, four-provider list setup, original and Rust child stages | P9 installed CLI direct requests, real loader/runs, restart/decryption/revocation | PASS |
| INV-001 tenancy | TenantConn/RLS, tenant snapshots, credentials and captured data | Existing isolation and tenant-separated SDK proof | PASS |
| INV-002 secrets | Existing binding/sealing source, caller/provider header separation, registration/loading no secret IO | Installed header/native trap/redaction/restart/revocation assertions | PASS |
| INV-003 no legacy/aliases/weights | Typed revised contracts; removed Workflow and old model/weight/provider-scope surfaces | Removed-field/kind negative, source catalog and codegen | PASS |
| INV-004 hashes/route identity | Authored Prompt hash independent of route; invocation override private state; capture effective/actual target | Stored Card/hash unchanged override tests; P7 target identity | PASS |
| INV-005 Rust/shared boundaries | Skald compiler/loop, server durable decisions, wyrd-client transport, thin SDKs | Source dependencies/owner comparison and recorded check:deps | PASS |
| INV-006 nonblocking outbox | decide stages existing scribe_outbox; capture uses same existing owner, no new durable writer | Source tracing; no new injected audit-failure run claimed | PASS |
| INV-007 no fallback/escalation/loss | Route registry missing-caller refusal; effective target reauthorization; source/kind/schema/matrix refusal | Denied override/no-upstream, local Vertex and unsupported media/settings proof | PASS |
| AC-001 three-SDK named gateway | Shared CRUD/address/admission and independent SDK journeys | Existing Rust/Python/TS onboarding/inference tests | PASS |
| AC-002 exact authorization | Complete grant meet/refusal, snapshot authorization and visibility | Recorded exact tests both directions/mixed grants, no IO and tenant cases | PASS |
| AC-003 external forwarding | Compatible adapter preserves suffix, credential provider, governance/authorization | External/header/revocation/screening proof | PASS |
| AC-004 all forms/SDK derived data | Prompt codec/hash plus thin TS/Rust/Python hydration | Four-form JSON/hash/introspection and P8 | PASS |
| AC-005 Agent routes and isolation | Existing loop with route_registry, shared observed invocation, no server native | Native/gateway/external/override/cancel/tool proof plus extended P9 | PASS |
| AC-006 compiler and negatives | Single compiler/model/schema/media validator; provider setting and native field checks | P2–P7 and existing portable/media/settings/registration negatives | PASS |
| AC-007 judges | Vala SkaldJudgeInvoker preserves route; server gateway context rejects native before IO; structured output same compiler | Existing judge real-time authority/structured-output tests | PASS |
| AC-008 native ingress/Vertex deferral | Actual deployment retained in every supported projection, local Vertex guard, server Vertex separate | P1, P9 four-provider runs, existing ingress/server/native Vertex regressions | PASS |
| AC-009 surfaces/docs/consumers | Typed SDK/CLI/MCP and served OpenAPI, two runnable Connect guides | Codegen, typings, P5/P6/P10, docs checks/render evidence | PASS |
| AC-010 no Workflow product | Removed full owner/runtime/routes/generated/public surfaces; independent Eval retained | Unknown-kind/retired route/catalog and surviving judge/Agent proof | PASS |
| AC-011 installed first tenant/all providers | Real CLI with isolated state and fresh TS saved auth, downloaded Agent child | P9 before/after restart and retained per-run attribution | PASS |
| AC-012 batch/restart | Whole-file structural validation before writes, ordinary per-item PUT, saved tenant/key reuse | P9 and existing CLI partial-progress/replay checks | PASS |
| R1-DOC-AC-01 | Runnable Connect setup includes credential/deployment/auth/recording/recovery commands | P9 and docs check/Chrome evidence | PASS |
| R1-DOC-AC-02 | Complete stock/framework examples and own retained-evidence read | P10 actual Agents SDK/Pydantic AI calls and isolated retained rows | PASS |
| R1-DOC-AC-03 | Existing docsite-rebuild Connect structure/index/theme reused, no new platform | Recorded desktop/narrow390px iframe/light/dark inspection and docs:check; iframe method honestly recorded | PASS |
| Non-goal: no hosting/new runtime | Existing local Agent and observed invocation, no new submission/status/cancel API | Source/catalog removal inspection | PASS |
| Non-goal: no mapping/discovery/protocol expansion | Exact model suffix preserved; explicit deployment; unchanged compatible adapter | Routing/native request source and external tests | PASS |
| Non-goal: no portable audio/video/new local Vertex | Portable enum limited to approved core; typed actual Vertex pre-transport refusal | Compiler negatives and P1 | PASS |
| Constraint: existing owners/no new platform | See owner comparison above; standard schema dependency replaces private walker | Source dependency and transport/loader comparison | PASS |
| Constraint: no production secret/network IO at load | Compile is pure; meta-check bundled with default network features disabled | Source constructors/dependency evidence | PASS |

## Owner-raised Agent save/projection question (not a new finding)

The reported behavior is real: `agent.rs:844–858` starts from RunConfig::default, and `833–840` writes resolved runtime values back to optional AgentRunConfigSpec fields. Thus absent `tool_concurrency_cap` becomes Some(8); absent max_iterations also becomes Some(10). `Agent::to_spec` additionally always sets verified_by empty. Consumers traced: from_card/from_wire and from_yaml_path, to_spec→to_card→to_wire/to_yaml_string, Python from_yaml/from_wire/to_card/to_wire, shared observed Run and Vala judge config converters. Hydrated AgentCard itself retains its exact spec and ordinary registration/download does not pass through runtime Agent::to_spec.

I do **not** propose this as a deployment-addressing finding: REQ-008's exact authored save/load guarantee is explicitly for Prompt forms; REQ-009/INV-004 require the Agent route to survive load/projection and invocation overrides to preserve the *stored* Card/hash, which the current private-run path does. The owner-requested stage requires checking route/provider/model/registered identities and actual loaded execution, not all Agent runtime-to-Card optional-field fidelity. Source comparison shows these run-config conversion/default and verified_by projection behaviors are unchanged portions of the existing lifecycle; the changed route member is now preserved. Expanding this review into full runtime Agent serialization preservation would add a separate lifecycle repair beyond the required route closure. It should be tracked separately rather than blocking this task or changing runtime defaults in this remediation. A future repair must account for explicit mutation and all omitted fields, not merely replace Some(8) with None or weaken canonical hashing. No new product decision is required for the reviewed route work.

The R2 test adjustment from whole-spec equality to the required route/ref/Prompt/provider/model comparisons therefore did not hide a required criterion. Its initial unexpected failure and narrowed scope are recorded rather than represented as behavioral RED for a production route defect; deliberate route-loss mutation separately proves the actual loader check.

## Verification limits

No test/build rerun by this reviewer; no paid provider, broad suite, load experiment or aggregate gate. Historical missing behavioral RED remains evidence limitation, not a fabricated present defect. The stale Python extension explanation is consistent with the recorded exact error mismatch and unchanged source/assertion following py:setup; source retains the remote Wyrd problem code at agent_failure. The final integrated gate and remaining independent reviewer checks are not claimed here.
