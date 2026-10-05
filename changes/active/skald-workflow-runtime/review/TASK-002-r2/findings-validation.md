# Independent structured Ponytail validation — TASK-002-r2

Validation complete. Six bounded obligations remain. No specification revision
or unresolved material decision is required. This is a validated finding ledger,
not the orchestrator's task verdict.

## Subject and evidence

Repository `/home/thorrester/Documents/GitHub/wyrd`; base
`0569b79702218600c4f9790f45cc03100d5c6f1c`; candidate
`e7d16b5bd622b9a564a49edb18239df7f209ca92`. HEAD was checked against the
candidate; tracked source remained unchanged. Only this report was written.
Revision 12 and `tasks/TASK-002-cleanup.md` govern, with the original task and
r1 ledger retained as historical closure inputs. The cumulative changes, current
source, discovery reports, focused follow-up and verification were inspected.
No CodeGraph index exists. No build, test, code generation or environment
mutation was performed by this validator.

Applied AGENTS §§2–9,11–12,16, agent-rules, current design/doctrine, the reference
router, spec-driven-development, maintainer-style and the applicable SDK,
reference, tenant/audit and registry boundaries. The task explicitly requires
Native proof now; selected gateway configuration and CLI/all-routes proof remain
TASK-003 and TASK-005. A passing test proves its assertions, not stronger
acceptance claims.

Independent tracing covered public shared/SDK loading → loader/canonical slots →
WorkflowBodies → existing graph traversal/authorized Cards reads → Skald body
lowering/binder/run → portable snapshot → SDK conversion; registration route →
request hash/replay/pure graph → EffectiveSpecs and its binding/baseline sibling
consumers → exact UID recheck → binding/node/version/relationship persistence →
audit/commit/upload handoff. Correction targets' complete bodies and callers
were inspected, including Bundle versus Runtime graph consumers, the two distinct
submission_card helpers, version-resolution branches and the public/native result
conversion. Agreement among reports is not used as evidence.

## Every discovery proposal

| Proposal | Disposition | Independent basis and final ID |
|---|---|---|
| BEH-002-R2-001, INV-R2-001: Native execution and complete SDK journey proof | REVISED | S3–S5/Outcome explicitly require it. Rust only runs a fake gateway through as_skald; Python never runs; TS only asserts unavailable gateway. Native default dispatch already has a deterministic mock seam, so new provider configuration is unnecessary. FIND-TASK-002-7. |
| BEH-002-R2-002: TS non-JSON run input | CONFIRMED | Public Record<string, unknown> reaches JSON.stringify; undefined keys disappear and bigint throws before native validation. Approved JsonValue signature is absent. Combined in FIND-TASK-002-8. |
| INV-R2-002, MAIN-001: TS canonical result erased | CONFIRMED | NativeLifecycleResult serializes the canonical Rust WorkflowRun unchanged, while TS declares steps unknown and omits error details/remediation. Same public projection correction as input: FIND-TASK-002-8. |
| MAIN-002: graph entry operations outside owning struct | CONFIRMED | New resolve_refs and materially rewritten resolve_graph read the hydrator's engine and orchestrate its traversal. All three callers are CardGraphHydrator methods. FIND-TASK-002-9. |
| REPO-R2-1: client/server free orchestration | REVISED | Hard owner rule applies to the changed graph entrypoints, materially extended resolve_card_references and UID-fenced write_registration. Retain the violation but reject a broad Cards-service refactor, a zero-state wrapper, or moving all persistence helpers. FIND-TASK-002-9. |
| REPO-R2-2: missing Rust error/cancellation docs | REVISED | Python from_path lacks mandatory # Errors; the two new napi load operations omit their catalog-result outcome and read-only partial-loading contract. Preserve prior FIND-TASK-002-5. Result-envelope functions need actual outcome documentation, not a fictitious thrown Result. |
| REPO-R2-3: fully qualified signature Result | CONFIRMED | Two new native/workflow.rs signatures use std::result::Result despite module-level napi::Result; native/lib.rs already uses StdResult alias. Preserve FIND-TASK-002-6. |
| REG-DUR-R2-001 and FOLLOWUP-R2-001: None/Scope Workflow root refused | CONFIRMED | Original root metadata enters WorkflowCard::from_envelope before durable resolution; resolved_pin returns None for both legal authored intents. Graph-only projection is already present but only used in pure planning. FIND-TASK-002-10. |

No discovery finding is silently omitted. System and tenancy/security reports
propose none; their bounded PASS assessments do not establish Native execution,
TS type precision or the newly traced root-version path. Optional redesign,
extra runtimes, new transports/configuration and broad unrelated owner cleanup
are rejected parts of proposed remedies, not additional ledger findings.

## Deduplicated stable ledger

### FIND-TASK-002-5 — REVISED — VIOLATION: loading boundary documentation is incomplete

Discovery REPO-R2-2; relates to prior STD-TASK-002-001. AGENTS §16 and
agent-rules require substantive rustdoc, # Errors on fallible operations, and
relevant partial-progress/cancellation behavior.

Locations: `sdks/wyrd-sdk-python/src/workflow.rs:490`,
`sdks/wyrd-sdk-ts/native/src/workflow.rs:105`,
`sdks/wyrd-sdk-ts/native/src/cards.rs:157`.
Python from_path returns WyrdPyResult and documents Python Raises only. Native
loads return NativeWorkflowLoad, mapping shared errors to its error field;
neither documents that outcome or what interrupted loading publishes/writes.
These are newly introduced public loading boundaries, with real filesystem or
registry reads, not dormant helpers. Python registered loading at
`src/state/mod.rs:2586` has # Errors; retain it and align its no-partial-result,
read-only explanation while editing boundary documentation, without changing
its synchronous runtime bridge.

Correction: add the missing Rust # Errors to Python from_path alongside existing
Python Args/Returns/Raises. Document native envelope errors and that loading can
stop after completed reads, yields no partial Workflow and writes no durable
state. Explain the actual channel: native functions return catalog errors in
NativeWorkflowLoad, public TS nativeHandle throws WyrdError; Python converts the
shared error at its boundary. Do not claim a synchronous Python call exposes an
async cancellation API. Reuse shared Workflow::{from_path,load} contracts and
regenerate declarations from source. No implementation change or behavioral
test is needed. Closure: item-local source inspection, codegen:check,
ts:napi:check, format/lints and declaration parity.

### FIND-TASK-002-6 — CONFIRMED — VIOLATION: new native signatures hide imported types

Discovery REPO-R2-3; prior STD-TASK-002-002. Agent-rules require module-top
imports and bare imported signature names. New
`sdks/wyrd-sdk-ts/native/src/workflow.rs:37,75` spells
std::result::Result in from_outcome's input and parse_workflow_selector's return.
Both have live callers: the local and registered native load paths. Native
napi::Result is independently used by run.

Correction: reuse `use std::result::Result as StdResult`, already used in
native/lib.rs, for these two signatures; retain napi::Result for run. No new
alias module, dependency or behavior. Closure: source inspection,
ts:napi:check, format/lints; generated public behavior remains unchanged.

### FIND-TASK-002-7 — REVISED — MISSING: mandatory Native execution and per-language loading proof

Discovery BEH-002-R2-001, INV-R2-001. Violates cleanup Outcome/S3–S5,
REQ-024/054–056, INV-007, AC-029/030 and AGENTS journey ownership.
Locations: `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:17–20,109–126,242–327`,
Python `tests/integration/cards/test_cards_crud.py:485–566`, TS
`wyrd/tests/integration/workflow-loading.test.ts:45–113`.

Proof selection produces the gap. The canonical example and mixed fixtures
explicitly choose wyrd_gateway. Rust's loaded local value is only inspected;
registered execution injects WyrdGatewayCaller through as_skald/run_with_options.
Rust never public-loads mixed authored refs and delegates ambient proof to other
languages. Python inspects local/mixed/collision/registered step IDs or YAML,
never calls run. TS calls run only for WYRD_WORKFLOW_503_BINDING_UNAVAILABLE,
never executes a successful run or introduces v2. Collision fixtures with the
same identity have distinct native bodies, but step IDs alone cannot distinguish
which source supplied a step. All named tests can therefore pass while public
Native execution or interpreter/Node input/result conversion fails. This is a
required proof gap, not proof that the executor itself is broken.

Selected minimal correction: extend these existing three owning-language
journeys. Keep the checked-in gateway artifact/its existing declarative loading
and registration assertions. Use an explicitly authored Native test variant of
the same DAG and exact Agent/Prompt structure, with deterministic provider
configuration already available. Prefer existing `ProviderRegistry::from_env`
→ `MockProvider::echo` (Custom("mock")) and the existing
`ProviderRequest::OpenAiChatCompatible { provider, request }` authoring form:
wrap unchanged OpenAI Chat payloads for explicit mock dispatch; do not infer or
rewrite a gateway route during execution. The existing Python native binding
journey in `test_workflow_parameter_injection.py` demonstrates this default
mock provider. This requires no new configuration API, provider registration
surface, native testing hook, executor or TLS harness. Echoed results can prove
the exact final bound reviewer content; use observably distinct authored payloads
for sibling/external collision and newer dependency, so actual run output proves
body selection. Preserve the original payload/model/variables and existing binder
semantics in the authored variant. If an existing deterministic native upstream
is reused instead, it must use supported configuration without weakening TLS:
OPENAI_BASE_URL from_env explicitly rejects HTTP, so a plain HTTP URL in that
environment variable is not an executable recommendation.

Exercise public from_path/fromPath → successful public run for local and mixed
variants, SDK apply/registration → exact/UID load → successful public run;
assert outputs, step results, final bound values and locked Agent/Prompt
identities. Apply newer bodies and prove original registered output stays pinned.
Complete Rust automatic mixed refs, missing/denied/inactive/mismatched refs and
collision proof with existing subprocess/environment-isolation patterns;
multithreaded unsafe set_var is not required. Keep zero-registry-local-load and
zero-dispatch/partial-write refusal assertions. The positive distinct-body proof
also closes the r1 provenance proof obligation without changing its repaired
source. CLI apply and gateway-selected preparation remain later tasks.

Closure: run the existing exact Rust workflow_loading_journey, Python
 test_workflow_loading_journey and TS workflow loading journey commands from
cleanup S3–S5, selecting successful Native assertions; retain the exact shared
loading and three PG Workflow selectors. Run owning Rust/Python/TS/native and
typecheck lanes from the task. No new per-function suite or harness.

### FIND-TASK-002-8 — CONFIRMED — INCORRECT: TypeScript runnable Workflow does not project the closed JSON/run contract

Discovery BEH-002-R2-002, INV-R2-002, MAIN-001. REQ-054 exact public signature,
INV-007 single snapshot, cleanup S5, typed-contract guidance.
Location `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1255–1273,1321–1324`.

Input producer Record<string, unknown> permits undefined, bigint or functions;
JSON.stringify erases or throws before native Map<String,Value> can validate the
original value. Native run then delegates shared run and serializes the canonical
WorkflowRun through NativeLifecycleResult::outcome; lifecycleValue parses and
casts this same JSON. Rust canonical owner
`wyrd-spec/src/card/workflow.rs:723–733,775–790,811–831` defines closed step
status/text/structured_output/attempts/timestamps/error and error
code/message/details/remediation. TS steps unknown, outputs unknown and the
truncated error prevent typed consumers from using those guaranteed fields.
Python already projects this snapshot; no runtime conversion defect is inferred.

Correction at the public TS projection: reuse a JSON-domain alias if one exists
(no JsonValue alias is present in this module); otherwise add the ordinary
recursive JsonValue alias. Use Record<string, JsonValue> for input/outputs,
closed WorkflowStepStatus/WorkflowStepResult, and full WorkflowRunError including
JsonValue details and remediation, following canonical nullability/wire names.
Keep serialization, native execution, catalog mapping and selector behavior.
Do not implement another input validator or runtime. This merges input/result
issues because one typed projection owns them, rather than adding downstream
consumer casts or a second snapshot.

Closure: owning TypeScript typecheck accepts nested valid JSON and rejects
undefined/bigint/functions; typed code reads step status/attempts/terminal error
remediation/details without casts. Successful Native owning-language journey
asserts canonical result fields; retain pre-dispatch WyrdError behavior. Regenerate
published ergonomic declarations and run TS build/typecheck/codegen lanes.

### FIND-TASK-002-9 — REVISED — VIOLATION: materially changed IO workflows remain outside their owners

Discovery MAIN-002, REPO-R2-1. AGENTS §5 and agent-rules explicitly govern new
and materially modified symbols; functional precedent cannot waive the rule.
Locations: client `cards/hydrate/graph.rs:322,350`; server
`components/cards/resolve.rs:49`; `components/cards/service.rs:1059`.

resolve_graph was rewritten for scoped traversal and resolve_refs is new. Both
extract RegistryEngine through all three CardGraphHydrator callers and orchestrate
reads/traversal. resolve_card_references adds Workflow validation to the existing
external-resolution/binding/baseline orchestration. write_registration materially
changes its validation-to-write fence to exact expected UID; it still threads
AppState/Caller/plan/audit dependencies through the whole transaction. These
operations are reachable and explicitly excluded from stateless free helpers.
The rule is structural and material, not a speculative runtime failure.

Selected correction boundaries:

- Put the two graph entry operations and their dependency-backed root-read/
  selector preparation on existing CardGraphHydrator using its RegistryContext.
  Retain GraphTraversal as mutable traversal owner; pure identity/alias helpers
  may stay free. Disk hydrate, resolve_external and load_workflow call this same
  owner. No second traversal/store/handle.
- Put reference-resolution/preflight orchestration on existing EffectiveSpecs,
  which already owns the provenance stores and resolved UID pairs. Initialize
  its existing state for the request, then resolve/validate through its inherent
  owner operations; retain caller-owned borrowed TenantConn and all three
  validation phases. Its sibling and external stores remain distinct; non-pinned
  roots remain absent as exact sibling targets. Do not move tenant acquisition,
  commit, audit or persistence into EffectiveSpecs.
- There is no existing cohesive write service owner in cards/service.rs:
  RegistrationPlan is a pure domain plan, ExistingNode/CardCompletionState are
  values, BindingProjector owns frozen binding projection only. Add the minimum
  private registration-write owner with real AppState/Caller dependencies and
  place this changed write operation on it. Keep RegistrationPlan a value;
  do not attach IO to it, add a zero-sized facade, expand AppState into a god
  operation owner, or move unrelated Cards listing/read/upload/lifecycle helpers.
  The current register_card entry calls this owner at the same point. This is
  the one narrow owner required by the hard rule, not a new registration system.

Preserve graph scopes, root-response reuse, exact selector/UID/active/alias/cycle
checks, Service disk/inventory/publication behavior, tenant/RLS context, audit
append ordering, original request hash/replay, exact UID row locks, idempotency
race rollback/replay return flag, version-line lock/bind/relationships, atomic
commit and existing post-commit upload initialization. Persist_node and
BindingProjector semantics stay where they are; no broader untouched-symbol
conversion is required. No new security, concurrency or persistence decision.

Closure: source review finds inherent operations on meaningful owners and no
new duplicate workflow; exact client load plus all three pg_workflow_registration
selectors and SQL relationship_recheck_blocks_target_lifecycle_race remain green,
with Service/loader regression lane, formatting and lints. Preserve those tests'
negative/state assertions; do not add a structural source-scanning check.

### FIND-TASK-002-10 — CONFIRMED — REGRESSION: Workflow preflight rejects legal server-owned root version intents

Discovery REG-DUR-R2-001, FOLLOWUP-R2-001. Design `wyrd-design.md:1730–1737`
preserves None/Scope/Pin registration intents, with server resolution;
REQ-028 and cleanup S2 preserve composite registration.
Location `components/cards/resolve.rs:430` (envelope helper at :509–523);
consumer `wyrd-spec/src/card/workflow.rs:1001–1010`.

The public typed registration request accepts omitted/scoped root metadata and
validate_request has no pinned-root condition. Pure planning uses
`graph_ready_submissions` with a graph-only 0.0.0 placeholder, leaving original
submissions intact. resolve_external passes originals into EffectiveSpecs;
validate_workflows converts original root metadata into WorkflowCard, whose
resolved_pin refusal occurs before declarative validation or any write. An
otherwise valid singleton Workflow with inline Agent/Prompt reaches this path
without any sibling-target ambiguity. Fresh omitted root or version "1" therefore
never reaches persist_node's locked resolve_existing → resolve_version
None/Scope branches. Exact dependency pins and registered loading invariants
are separate. Replay can return before preflight; fresh requests remain broken.

Correction: retain EffectiveSpecs preflight, using existing
`graph_ready_submissions` only to project the transient Workflow root holder's
metadata before WorkflowCard::from_envelope. This helper currently has one
preflight caller; the service.rs submission_card namesake is for persistence and
must preserve authored intent. Keep the original request/submissions for hashing,
replay, pure sibling provenance and durable writes. Do not use placeholder
submissions to seed EffectiveSpecs.siblings, allocate durable versions in
preflight, change exact Agent/Prompt identities, or relax public/registered
WorkflowCard::from_envelope. The temporary root pin is discarded after pure/
resolved validation; it selects no dependency and transfers no authority.

Closure: extend registers_only_valid_explicit_workflow_graphs with omitted and
scoped roots, inline or exactly pinned dependencies; assert server-resolved exact
seed/version and reload. Exercise invalid resolved bindings under both intents
and assert no registration operation/Card/relationship persists, retaining
standalone refusal audit and existing provenance/UID/lifecycle negatives. Use
its exact managed-Postgres command from cleanup S2; keep the replacement and
lifecycle-lock focused tests. No new API/allocator/transaction mechanism.

## Prior stable finding closure

| Prior ID | Current closure |
|---|---|
| FIND-TASK-002-1 | Original invalid body production is repaired: WorkflowBodies and EffectiveSpecs keep Ref/Sibling stores distinct and Skald preserves original discriminator. Negative incompatible collision coverage survives. Positive per-language distinct-body execution proof remains in FIND-TASK-002-7, rather than claiming source is still defective. |
| FIND-TASK-002-2 | Closed: actual example has three exact-version Prompt Cards, Agent paths, seven registration outcomes and both relationship layers. |
| FIND-TASK-002-3 | Superseded/closed under Revision 12: duplicate keyed normalization and sole-use metadata are absent; canonical untagged/reference visitor remains. Do not recreate the old normalization remedy. |
| FIND-TASK-002-4 | Closed: preflight UID pairs reach exact-UID Active FOR SHARE recheck, returned UID is the expected one and bindings consume it. Stale replacement test independently passes; lifecycle lock proof remains. |
| FIND-TASK-002-5 | Old removed parser/loading locations no longer apply; same documentation obligation remains at new loading boundaries. Retained above with stable ID. |
| FIND-TASK-002-6 | Historical imports/qualified signatures corrected or removed; same policy violation exists in new native signatures. Retained above with stable ID. |

## Proof and decision limits

Root independently observed client 1/1, PG Workflow 3/3, Rust SDK 1/1 plus
fmt:check, diff check, docs:check and skills synchronization PASS. Their sources
have exactly the proof limits described above. Python/TS and broader lane
results remain recorded implementation evidence, not validator reruns. The
root-version failure follows a deterministic resolved_pin branch; no test
failure diagnosis or missing trace is being invented. No gate suppression or
weakened refusal is proposed.

Retained IDs: FIND-TASK-002-5, -6, -7, -8, -9, -10. IDs 7–10 are the next unused
numbers for new obligations. Validation is decision-complete within approved
behavior; neither BLOCKED nor SPEC_REVISION_REQUIRED applies. The root
orchestrator owns final verdict and remediation artifacts.
