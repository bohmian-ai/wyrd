# TASK-001 independent behavior review — BLOCKED

## Immutable subject and interruption

Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. Pinned candidate: `0aae38f9ad3886977d7684fc17d708cbb15d1f17`. Approved authority: `changes/active/deployment-addressing/spec.md` r5; original task: `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md`.

Discovery began with HEAD verified at the pinned candidate. The orchestrator subsequently reported HEAD moving to `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183` and directed discovery to stop. The task-review skill requires BLOCKED when the candidate moves. This report preserves partial independent observations, not an acceptance verdict or validated finding ledger. No builds/tests ran and no implementation files changed. Do not carry source line numbers below into a new review without checking the new immutable candidate.

## Partial proposed findings — require fresh validation

### BEH-001 — INCORRECT: direct Agent reconstruction loses its declared gateway route

Pinned-source locations inspected: `crates/skald/skald-agent/src/agent.rs:611` (`from_card`) and `:708` (`to_spec`). `from_card` reads prompt, tools and run configuration but does not retain `card.spec.execution_route`; the runtime Agent has no route field. `to_spec` writes `ExecutionRoute::Native {}` unconditionally. The public `Agent::run` uses the host registry. The separate `crates/shared/wyrd-client/src/observe/invoke.rs:75` applies a route but expressly rejects tools at `:82`.

Reachable proposed consequence: load a gateway Agent through the existing direct Agent YAML/Card APIs, run it with host provider configuration, or save it again: declared gateway execution is silently native, or the durable route changes on round trip. A missing native provider produces an unrelated configuration failure. This conflicts with REQ-009, REQ-010, INV-004, INV-007 and AC-005. Python direct Agent entry points and Rust from_card/from_wire/from_yaml consumers must be included in validation.

Smallest correction boundary to evaluate: retain and honor route on the existing Agent owner and its bounded loop, with an invocation-local whole-route override; reuse the gateway caller/registry seam rather than invent another loop. Preserve the authored route when projecting a Card. Focused proof should reconstruct a gateway Card through actual direct Agent APIs, execute two tool iterations through its named gateway, save/load unchanged, and show native host credentials are never used. This is a proposal, not a completed full sibling-caller audit.

### BEH-002 — INCORRECT/MISSING: public Agent projection cannot reach named Anthropic/Google/Jev deployments

Pinned-source locations inspected: `crates/shared/wyrd-client/src/gateway_caller.rs:270–289`. Anthropic projection assigns only `model.model` to the model body; Gemini projection puts only `model.model` in the URL. The Gemini guard at `:279` treats the deployment name `vertex` as provider identity. No Jev request branch exists; it falls into the unsupported-dialect branch. Conversely, server ingress parses a full deployment/model address (`crates/wyrd/wyrd-server/src/components/gateway/ingress.rs`, `native_model` and `anthropic_messages`).

Reachable proposed consequence: an observed Agent routed to `claude-prod/claude-sonnet-4-5` or `gemini-team/gemini-2.5-flash` loses the selected deployment and is rejected or interpreted as another address. Jev Agent invocation is refused despite supported direct Jev ingress. A Gemini deployment legitimately named `vertex` is also refused. Violates REQ-001, REQ-009–REQ-010 and AC-005/AC-008. All SDK observed Run surfaces share this Rust producer. The raw HTTP test `native_ingress_preserves_named_deployments` constructs correct addresses manually, so does not exercise the defective Agent projection.

Smallest correction boundary to evaluate: preserve the full ModelRef in public native ingress projection, add the existing supported Jev request/response projection, and refuse local Vertex based on actual Prompt/provider information rather than the arbitrary deployment name. Reuse existing ingress and provider mappings. Proof must call these named targets through the real Agent/Run SDK path, including a slash-containing model and Gemini deployment named `vertex`; retain explicit local Vertex refusal. Full compatibility and sibling server-consumer tracing remained unfinished.

### BEH-003 — INCORRECT: portable media binding discards its declared block kind

Pinned-source location inspected: `crates/skald/skald-spec/src/authoring.rs:423–425`. `PortableMessage::lower` converts image/document bindings into identical plain placeholder strings and explicitly comments that binding MediaRef kind is not checked against the block kind. `Prompt::bind_media_mut` chooses the inserted provider-native content from supplied MediaRef kind; inspected source has no authored block-kind comparison.

Reachable proposed consequence: `${media:photo}` declared in an image block may be bound as a document and dispatched with different semantics. REQ-006 explicitly requires validating MediaRef kind against the block; INV-007 forbids unsupported-option/content loss. A shortcut comment cannot waive this constraint.

Smallest correction boundary to evaluate: preserve the authored binding kind in the existing Prompt/compiler/binder owner and reject mismatches before dispatch; do not add another media subsystem. Prove image/document matching and cross-kind rejection through the public binder and a relevant runtime caller. Shared bindings and literal-media generated-name interactions were not fully traced before interruption.

### BEH-004 — MISSING (investigation incomplete): schema validation appears absent from shared authoring compilation

Inspected `AuthoredPrompt::compile`, native/portable compilation, response-format mapping, `Prompt::new`, and `wyrd-spec` Prompt validation. Portable schema Maps appear copied directly to wire fields; pure Card validation checks placeholder parameter names only. An invalid schema such as `{"type":"not-a-json-schema-type"}` appears admissible to storage/dispatch, contrary to REQ-006–REQ-008/REQ-011 and AC-006. No final finding is asserted: remaining downstream registration and dispatch validation owners were not exhausted before candidate movement.

Validate against all existing schema owners before retaining. If confirmed, reuse the existing shared schema validator at authoring acceptance and refuse unsupported destination schema features rather than silently forwarding them. Add a focused invalid-schema public registration proof, not another schema engine.

## Acceptance matrix — interrupted audit

`FAIL (incomplete)` below means acceptance was not established because the immutable review was interrupted; it is not an independently validated implementation defect. Proposals above identify particular source concerns where present. Recorded T01–T17 results are task evidence claims; this reviewer did not rerun them. No obligation is marked PASS on partial inspection alone.

| Obligation | Implementation evidence reviewed / remaining | Verification evidence reviewed / remaining | Result |
|---|---|---|---|
| REQ-001 | Named deployment contracts, CallPlan, ingress inspected; full consumers incomplete | T03/T10 source sampled; Agent projection concern BEH-002 | FAIL (incomplete) |
| REQ-002 | Invocation requested/fallback decisions, explicit-model listing, granted check inspected | Full scope table, batch and tenant journeys not completed | FAIL (incomplete) |
| REQ-003 | CLI single/list prevalidation and ordered outcomes inspected; credential binding source sampled | T06/T13 source sampled; whole lifecycle proof not reconciled | FAIL (incomplete) |
| REQ-004 | External adapter credential separation and public ingress inspected | External forwarding journey sampled; network/durability proof incomplete | FAIL (incomplete) |
| REQ-005 | Authored provider/model fields and compiler inspected | T02 sampled; literal identity validation not exhausted | FAIL (incomplete) |
| REQ-006 | Portable lowering/settings/schema inspected; BEH-003/004 | T02 sampled; negative binding/schema proof missing from inspected test | FAIL (incomplete) |
| REQ-007 | Native pair table/model insertion/extension flattening inspected | Native compile test sampled; full native-content matrix incomplete | FAIL (incomplete) |
| REQ-008 | Authored Prompt serialization/compiler and Card placeholder validation inspected | Three-language round-trip and main registration seam incomplete | FAIL (incomplete) |
| REQ-009 | AgentSpec/route enum, direct runtime, observed override inspected; BEH-001 | Rust/Python/TS observed route tests sampled; direct Agent proof not established | FAIL (incomplete) |
| REQ-010 | Public/server gateway callers, ingress, planning inspected; BEH-001/002 | Raw native ingress and OpenAI observed route tests sampled; Agent-native families incomplete | FAIL (incomplete) |
| REQ-011 | Typed contract/error projection and SDK route consumers sampled | Generated/catalog/OpenAPI full closure incomplete | FAIL (incomplete) |
| REQ-012 | Full changed-path deletion inventory, surviving Agent/judge owners sampled | Workflow rejection and independent Eval regression closure incomplete | FAIL (incomplete) |
| REQ-013 | Installed CLI four-provider setup test and walkthrough ownership sampled | T13 source covers restart, keys, failures and fresh TS process; full helper/lane closure incomplete | FAIL (incomplete) |
| INV-001 | Snapshot tenant, invocation caller, credential assignment source sampled | Batch/caches/registration isolation not completed | FAIL (incomplete) |
| INV-002 | Credential provider/host binding and public caller header projection sampled | External/header/restart tests sampled; full sealing owner trace incomplete | FAIL (incomplete) |
| INV-003 | New addressing/contracts and Workflow removal inventory sampled | Full compatibility/retired decoder sweep incomplete | FAIL (incomplete) |
| INV-004 | Observed override immutable; direct Agent projection BEH-001 | Stored-card/hash and fallback capture full proof incomplete | FAIL (incomplete) |
| INV-005 | SDKs share Rust Run::invoke_on and compiler; Agent/judge gateway seam inspected | Runtime/dependency boundaries not completely audited | FAIL (incomplete) |
| INV-006 | Invocation decide stages process outbox; no synchronous audit wait in inspected path | Existing outbox failure regression not fully reconciled | FAIL (incomplete) |
| INV-007 | Route consumers/compiler examined; BEH-001–003 | Callback/tool/native/override negative closure incomplete | FAIL (incomplete) |
| AC-001 | Named/optional deployment contracts, planner inspected | Complete three-SDK evidence not reconciled | FAIL (incomplete) |
| AC-002 | Subject decision/fallback/model discovery source inspected | Full authorization matrix, actor and batch evidence incomplete | FAIL (incomplete) |
| AC-003 | External credential source/forwarding and installed setup inspected | Full pricing, SSRF and revocation proof incomplete | FAIL (incomplete) |
| AC-004 | Authored codec and hydration starting map inspected | All three-runtime round trips/introspection incomplete | FAIL (incomplete) |
| AC-005 | BEH-001/002; observed path only tool-free | T08/T14/T15 inspected: OpenAI declared/backup/granted denial only, not complete acceptance matrix | FAIL (incomplete) |
| AC-006 | Compiler settings/media/native branches inspected; BEH-003/004 | T02 sampled; schema and MediaRef kind negatives not established | FAIL (incomplete) |
| AC-007 | SkaldJudgeInvoker route setup and server gateway caller inspected | T11 source and adjacent attribution regression sampled; full subject proof incomplete | FAIL (incomplete) |
| AC-008 | Public raw ingress supports named targets; Agent projection concern BEH-002 | T10 raw HTTP test bypasses Agent caller; Vertex and translation closure incomplete | FAIL (incomplete) |
| AC-009 | Authority, contract, SDK and changed consumers sampled | Full generated/MCP/OpenAPI/docs agreement incomplete | FAIL (incomplete) |
| AC-010 | Workflow deletion inventory sampled | HTTP/CLI/SDK retirement and independent Eval full closure incomplete | FAIL (incomplete) |
| AC-011 | Installed startup/four-provider/fresh TS test source inspected | Installed helper and selected-lane chain incomplete | FAIL (incomplete) |
| AC-012 | CLI list parser/progress and restart test inspected | Complete interruption/partial-failure/convergence proof not reconciled | FAIL (incomplete) |

## Constraints and non-goals

No proposed new server Agent job API, direct external bypass, aliases/model renaming, portable audio/video, replacement Workflow runtime, cloud integration or local Vertex support was requested by this review. Adjacent approved changes were not classified as drift simply for scope. Full cumulative non-goal and adjacent runtime/release/query audit is incomplete due the mandatory stop.

## Verification limits and result

CodeGraph was reached before source navigation. Approved r5, task evidence, skill, agent rules, reference router, maintainer/spec-driven/testing guidance, and relevant design/doctrine sections were read. Review discovery sampled core caller paths independently and did not consume another reviewer's findings. All source proposals remain unvalidated; full authority, cumulative diff, sibling-consumer and 32-obligation closure was interrupted.

**Overall: BLOCKED — candidate changed during immutable review.** Restart with a newly pinned subject and complete independent discovery/validation. Skipped builds/tests as instructed; the partial observations cannot establish merge readiness.
