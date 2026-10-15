---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-deployment-addressing
spec_revision: 6
parent_task: TASK-001
depends_on: []
requirements: [REQ-001, REQ-002, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-013, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-002, AC-004, AC-005, AC-006, AC-008, AC-009, AC-011, AC-012]
remediates: [FIND-001-1, FIND-001-2, FIND-001-3, FIND-001-4, FIND-001-5, FIND-001-6, FIND-001-7, FIND-001-8, FIND-001-9, FIND-001-10, FIND-001-11, FIND-001-12, FIND-001-13, FIND-001-14]
---

# Repair deployment addressing, Prompt validation and Agent execution

## Authority, reviewed candidate and handoff

Implement with `$wyrd-implement`. This is one remediation task for the complete
validated finding set and the owner-requested finding 14, not a new design exercise. No implementation, test pass,
or finding closure is claimed by this packet.

- Approved specification: `changes/active/deployment-addressing/spec.md`, revision 6.
- Original task: `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md` (r5 implementation history remains unchanged).
- Review base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
- Reviewed candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
- Immutable source used for review: `/tmp/wyrd-task-review-ef2fe8bd`.
- Review evidence: `verdict.md`, `findings-validation.md`, `followup-review.md`, all independent reports and `cumulative.diff` beside this task.
- Governing standards: root `AGENTS.md`, `architecture/agent-rules.md`, the reference router and applicable Rust/Python/TypeScript, security, testing and Bifrost authority.

The owner explicitly allows concurrent tests aligning with main. Later commits
are not reviewed here. Before implementation, inspect their effect at these
owners and retain others' edits. If a later test or correction already closes a
finding, prove closure rather than reimplementing it. Do not revert approved
adjacent release, tooling, inline-Prompt materialization, or async-connect work.
Source line references below identify the reviewed candidate, not guaranteed
line numbers in a later checkout.

## Outcome and approved decision

An Agent must keep its declared route, use it for every supported model call,
and preserve it when loaded or saved. Named deployments must reach the actual
gateway ingress across the supported provider families. Prompt authoring must
reject invalid identities, schemas and media before storage or provider IO.
Authorization, batch visibility and retained evidence must match the target
actually used.

The owner settled the only material decision after review: **both actor and
subject must have permission for an operation.** A successful delegated token
contains their exact shared authority. If existing token scopes cannot express
that complete result, **reject the exchange and issue no token**. The explicit
approval was “Reject unrepresentable exchanges (recommended)”. Revision 6 and
its architecture reconciliation record this narrow amendment. There is no
remaining choice between adding scopes, silently reducing permissions or
refusing exchange: implement refusal. Existing representable intersections,
including representable mixed inputs, remain supported. Empty intersection is
representable; this amendment does not add a separate empty-intersection
refusal policy.

## Owners and correction boundaries

| Existing owner | Required extension; consumers to close |
|---|---|
| `wyrd-runtime` Permission/PermissionSet; `wyrd-auth` tenant issuer/delegation | Preserve explicit grant authority and compute exact representability before signing; role issuance, verification/recovery and delegation consume the same owner. |
| `skald-agent::Agent` and current bounded loop/gateway registry | Retain declared route and bind effective route per invocation; construction, load/save, direct execution, delegation tools, observed Run and judges must agree. |
| `wyrd-client::PublicWyrdGatewayCaller` and server GatewayInvocation | Correct native wire projection and initial Agent compatibility without duplicating transport or banning direct-client translations. |
| `skald-spec` Prompt/compiler/media and `skald-prompt` builders | Shared pure model/schema/media acceptance and typed binding; native/portable/Card/builder/Python/judge consumers use the same invariant. |
| Server GatewayBatches and existing tenant SQL list/claim owners | Select visible pages before LIMIT and authorize actual dispatch snapshot while preserving batch lifecycle fences. |
| Gateway engine attempt facts and existing capture projection | Carry admitted provider identity into retained trace attributes, separately from deployment and model. |
| First-class SDK register/hydrate journeys and current fixtures | Add missing authored preservation/hash/introspection proof without new production accessors or another harness. |

The table is an ownership map, not a requirement to edit already-correct files.
Durable decisions remain Rust server-owned; language wrappers stay thin. New or
materially relocated PyO3 belongs in the Python SDK. Generated outputs come
from their source; authored Python stubs change with the native signature.

## Owner-directed addition: initial gateway docsite guides

This section is an explicit user addition after independent review, not a new
reviewer finding; do not assign it a FIND ID. It completes deployment-addressing
REQ-011/AC-009 under the existing approved documentation architecture:
`changes/active/developer-docsite-rebuild/spec.md` revision 2, especially
REQ-004/006/010/011/013/014/016, and its
`tasks/TASK-006-connect-and-automate.md`. This is the bounded gateway contribution
to that task; do not mark the entire docsite rebuild or TASK-006 complete.

Use `$human-tech-docs` for reader-facing prose, including its required voice,
humanizer and page-purpose passes. `docs/` owns content/navigation/indexes;
retain the current SvelteKit/mdsvex/Pagefind/Shiki platform, brand, themes,
existing page components and six-section structure. This addition needs no
product/runtime behavior beyond the validated corrections already in this task.

**Selected publication scope:** publish two complete Connect task guides, using
existing content where correct rather than creating competing instructions:

1. **Set up the LLM gateway.** Start with a short gateway explanation: the user
   connects to Wyrd with Wyrd credentials; a named deployment selects the
   upstream adapter and provider credential; Wyrd injects upstream auth and
   records the request. Then show the shortest complete setup of one provider:
   prerequisites; `wyrd server dev --tenant acme`; an ordinary provider
   credential JSON file and existing credential put command; deployment JSON
   and existing deployment put command; list/get to confirm setup. Explain
   `<deployment>/<model>` using a name different from the provider, show that
   the upstream model remains unchanged, and distinguish provider identity,
   deployment name and Prompt identity without an architecture detour. Include
   a compact four-provider list example using the approved OpenAI, Anthropic,
   Gemini and Jev configuration contract, with links to each adapter's actual
   auth/operation limits. Clearly separate Wyrd caller authentication from the
   tenant's upstream secret; no keys in Cards, source control or sample output.
   Cover missing/revoked credential, missing deployment and permission denial,
   with actionable recovery. Link the first-request guide and exact source-owned
   CLI/API/configuration reference rather than duplicating their catalogs.
2. **Make gateway requests from a framework.** Reuse/update
   `docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md` as the
   starting content, correcting its stale provider-address terminology and
   synchronous TypeScript connect example. Give complete executable first
   requests, not client construction alone: imports, installation/prerequisites,
   current authenticated Wyrd connection, real client/model setup, addressed
   model, invocation and printed response. The initial stock-client coverage is
   OpenAI Python and TypeScript, Anthropic and Google GenAI; include framework
   sections for **OpenAI Agents SDK** and **Pydantic AI**, which reuse those
   existing client/transport hooks. Verify exact installed client/framework
   versions and APIs before writing examples. Each section must actually run
   an inference through a named deployment and show how to confirm response
   and retained gateway evidence through normal supported reads. Show a material
   authorization or invalid-target refusal and its recovery. Reuse existing
   saved-config/Wyrd HTTP auth transports; document supported static Wyrd-key
   inference only where applicable, without suggesting management/MCP/Bifrost
   accepts that key. Respect provider-specific base URLs, request formats and
   limitations; do not imply Jev supports general chat/tools or local Vertex
   gateway support. Keep any language support gaps explicit at point of use.

Each guide follows the rebuild's task-page order: outcome, prerequisites,
complete commands/code, expected result and success check, common failure and
recovery, concise explanation/reference links. Gateway overview belongs in the
setup guide's introductory section; it does not require an extra empty page.
Exact titles/slugs remain editorial implementation choices. Keep one canonical
procedure for provider onboarding and cross-link it from framework examples.
No wholesale restoration of deleted historical articles, separate gateway site,
brand redesign, exhaustive framework catalog or placeholder navigation.

**Publication and evidence:** use current docs navigation/content metadata to
place the two useful published pages in **Connect** with no deeper path than
section → topic → page; update related links, search and machine-readable
indexes from the published corpus. Walk from landing/sidebar to setup to the
first request. Run every published example in its owning Python/Node runtime
against the real Wyrd server with controlled upstream fixtures and ordinary
credentials/auth, reusing existing gateway/client and installed-CLI owners.
Record actual commands, dependency versions, expected response, retained evidence
and material refusal; constructor-only or tracing-only checks do not prove a
framework's inference path. Do not copy draft:true/status:runnable metadata as
proof that existing prose works. The relevant current source owners include
`sdks/wyrd-sdk-python/tests/integration/gateway/test_gateway_openai_client.py`,
`test_gateway_native_clients.py`, the shared HTTP-auth tests, and the existing
local-dev setup journey. Add only focused missing example proof to existing
owners and record exact selectors before execution; do not require entire
framework/journey suites or live provider credentials.

Run `mise run docs:check` for this added docs write set, using the site's
existing link, command, build, search/index and accessibility checks. Inspect
rendered sidebar/search/links, copyable code and headings at desktop/narrow
widths and in both themes. This is documentation/example verification rather
than manufactured behavioral RED; if a necessary example exposes a real
runtime gap, correct it only within this packet's approved behavior and prove
it at its owning boundary. No new docs test framework or permanent check.

**R1-DOC-AC-01:** the two Connect guides publish the gateway overview, one-provider
onboarding, multi-deployment config and complete first requests using the
current named-target/authentication contracts.
**R1-DOC-AC-02:** stock clients plus OpenAI Agents SDK/Pydantic AI examples each
have actual inference, visible response/evidence and material refusal proof;
no unsupported capability is advertised as runnable.
**R1-DOC-AC-03:** placement, links, search/indexes, both themes and narrow layouts
follow the approved rebuild structure and pass `mise run docs:check`; ownership
of the remaining docsite rebuild remains unchanged.

## Issue diagnoses and selected corrections

### FIND-001-1 — Critical — permission normalization and delegation can change authority

**Obligations:** REQ-002, INV-007, AC-002; approved r6 refusal policy.
**Evidence:** `crates/shared/wyrd-runtime/src/permission.rs:866–871,886–890,917–952`,
`crates/wyrd-spec/src/auth/permission_scope.rs:151–158`,
`crates/wyrd/wyrd-auth/src/issuance.rs:212–230,778–787`,
`crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:709–738`.

Role merge currently treats an explicit deployment/model grant as redundant
beside `all`, even though only the explicit grant permits a matching restricted
(`granted`) deployment. The grant is lost in either insertion order. Separately,
delegation intersects `all` with a deployment grant by emitting that deployment
grant. The delegated token can then access a restricted deployment even though
one party could not. `DelegateToken::execute` verifies both tokens, resolves
actor roles, and signs this result; gateway admission trusts the resulting
object-scoped grant. This affects same-tenant inference spend, discovery and
batch/file operations. No cross-tenant exploit is claimed.

**Selected correction:** fix grant normalization and exact intersection in the
existing Permission/PermissionSet owner, then propagate unrepresentability to
the existing issuer/delegation boundary as an explicit refusal **before token
signing**. Preserve explicit invoke grants beside broad grants, including
wildcard/AnyOf resource/action combinations. Keep requirement matching separate
from grant subsumption so `all` still permits open deployments. Evaluate the
complete resulting authority: do not reject every mixed input pair when its
full result is representable. Do not enumerate current open deployments into
explicit grants, add a conditional wire scope, or issue an attenuated token.
Keep other permission families, audiences, subject/actor identity and token
lifecycle behavior intact. No separate grant store, role lookup at inference,
or per-handler patch is needed.

Return a derive-backed public authorization error for an unrepresentable
exchange: HTTP 403, stable catalog code
`WYRD_AUTH_403_UNREPRESENTABLE_DELEGATION`, explaining that the shared
permissions cannot be represented and the exchange issued no token. Remediation
text directs the caller to use grants whose exact shared authority fits the
supported scopes; it must not suggest widening either party's permissions.
Expose no token, key or secret in details. Register metadata once in the
existing error catalog and regenerate consumed errors/types/docs. This public
error is the concrete projection of the approved clear refusal, not a new
permission or route. Preserve existing exchange failure propagation and stage
the denial on the existing nonblocking audit owner. Refuse this request only;
do not crash or disable the server/issuer.

Direct issuance, verification and owner recovery must retain explicit grants;
delegation must not manufacture an explicit grant from an all-only party.
Requested targets, fallbacks, model visibility, batches and files continue to
use the ordinary gateway decision owner. Existing isolated-grant tests never
exercise mixed role normalization or real delegated issuance, so their green
results do not close this gap.

**Focused proof:** extend P1/P2 with both insertion orders, broad axes, both
subject/actor directions, representable mixed results, All×All and explicit
intersections. Prove the error/status and no signed token on unrepresentable
exchange, exact permissions on success, no upstream request on denied use,
explicit-role retention, and unchanged unrelated Bifrost permission behavior.
No-overlap remains an exactly representable empty result under existing
issuance behavior; do not conflate it with unrepresentability.

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

### FIND-001-14 — Owner-requested — MISSING: prove downloaded Agents retain and execute every provider's configuration

**Provenance:** the owner explicitly requested this additional test after the
independent review and asked that it be numbered finding 14. This is an
approved coverage requirement, not a retroactive claim that the reviewers
independently discovered another implementation defect. Findings 2, 3 and 13
explain why the path needs direct end-to-end proof.

**Obligations:** REQ-008–011/013, INV-004/007, AC-004/005/008/011/012.
**Existing owners:** original TASK-001 Scenario 13/T13;
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_installed_cli_gateway_setup`,
its fresh TypeScript saved-config client driver, shared Cards registration and
bundle download/materialization, and the existing public Agent loader/runtime.
Use the real Python Agent runtime for the load/run stage in a fresh client
process under the same installed-CLI journey; the TypeScript setup remains
unchanged. Do not invent a TypeScript Agent runtime API.

**Diagnosis:** direct requests after onboarding prove a deployed provider works,
but bypass Agent registration, downloaded bundle documents, Prompt reference
resolution and runtime Agent construction. Separate register/hydrate tests and
manually attached provider registries do not prove that this complete user path
keeps deployment names, route, provider and model. A regression in any handoff
can pass setup/direct-inference checks and then change or lose the user's Agent
configuration. This finding requires the missing combined proof, without
claiming the path has been newly executed or that a new corruption bug was
independently demonstrated.

**Selected correction:** extend the existing installed-CLI setup journey with
one new provider-loop stage immediately after step 5 (direct response/header/
retained-evidence checks), then continue its existing step 6 (server restart).
Reuse its real server, isolated database/config, tenant, uploaded credentials,
named deployments, saved authentication and controlled provider HTTP fixtures.
The four providers are those already onboarded: OpenAI, Anthropic, Google via
the Gemini adapter, and Jev. This does not introduce local Vertex gateway
support. Prompt provider is `google`, independently of the `gemini` adapter or
deployment spelling. Choose deployment names that differ from provider names
so a provider-name substitution cannot accidentally pass. Use distinct literal
fixture model IDs and cover a slash-containing identifier where supported.
Use a valid provider-specific Prompt form, including native `system_one` for
Jev; keep existing Jev tool/streaming limits.

For **each** of those four providers, perform this sequence:

1. Author a Prompt with that provider and exact model, and an Agent whose
   required `execution_route` selects that provider's named gateway deployment;
   register through the real public Cards/CLI path with saved caller auth.
2. Download the registered Agent bundle through the supported public bundle
   download/materialization path, including its exact referenced Prompt Card;
   use a fresh output directory, and do not manufacture a bundle from the
   original authoring inputs.
3. Load the downloaded Agent through the public **Agent** loader with the
   bundle's normal reference resolution, rather than building a replacement
   Agent or running the pre-download in-memory object.
4. Assert the registered representation, downloaded documents and loaded Agent
   retain the same exact route kind/deployment, Prompt provider/model and exact
   Agent/Prompt identities; confirm the referenced Prompt is the registered
   Prompt actually present in the bundle. Do not reconstruct missing fields.
5. Supply only normal saved Wyrd connection/auth and host gateway dependencies,
   then execute the **loaded Agent** against the controlled provider fixture.
   Mock upstream provider HTTP responses only: Agent execution, rendering,
   route selection, Wyrd auth/transport, gateway admission and retained evidence
   must remain real. Do not monkeypatch Agent.run, return a fabricated model
   answer locally, force native execution, or replace the declared route with a
   manually attached provider registry/override to make the test pass.
6. Assert the expected result, exact selected deployment, unchanged upstream
   model and correct upstream credential header with no forwarded Wyrd key;
   query normal retained evidence correlated to this Agent invocation and
   assert requested/resolved deployment/model, not the earlier direct request.
   A native-provider trap receives zero calls. Include a representative denied
   loaded-Agent request with no upstream IO using existing permission fixtures.

After every provider has completed that stage, **continue step 6**: restart dev
with the same tenant/config/state and rerun those downloaded, loaded Agents
without re-registering or re-downloading them. Assert route/provider/model and
request/evidence remain exact, saved auth still works, tenant/key identities
are reused, and existing upstream credentials still decrypt. Preserve current
restart/revocation checks and normal lifecycle cleanup/timeouts. This validates
that restart does not conceal a prior registration/load defect.

**Acceptance R1-AC-14:** a single readable installed-CLI user journey runs
setup/direct inference → register/download/load/inspect/execute all four
provider Agents → restart/re-execute the downloaded Agents, with exact identity,
route/provider/model, upstream auth and retained evidence assertions. Its
provider fixtures are the only mocked inference boundary; no live keys or paid
calls, parallel launcher, test-only bootstrap auth, or production SSRF exception.
Retain separate focused tests from other findings where they isolate failures;
this combined journey does not replace them or the three-SDK contract proof.

**Focused execution (extend existing T13, not a new harness):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
```

Use its existing installed artifact/build prerequisites and local-dev lane
selection; record nonempty discovery and RED/GREEN for the extended stage.
Run that exact selector after the relevant routing/bundle fixes. This is one
required focused journey, not authorization to repeat every journey suite.
Update the matching gateway setup walkthrough to show the same user sequence.

## Bounded documentation inventory for FIND-001-12

This inventory is part of the task, not a request to document untouched code.
The independently validated additional items are ProviderName's associated Err,
new compiler/Prompt test modules, function-local BACKUP_KEY/EXTERNAL_KEY/MODEL
constants, and the new compiler/ScriptedGateway/tool-loop Panics contracts.
The primary compiler/serde/judge locations are listed in the diagnosis above.


The following cumulative changed functions also require correction under the same finding. `item` means no item rustdoc; `Errors` means rustdoc exists but the required error section is absent. This includes material body edits where the signature itself was unchanged. The preceding finding also identifies affected modules, an associated type, and function-local constants.

| File | Functions and candidate lines |
|---|---|
| `crates/shared/wyrd-loader/src/lib.rs` | `prompt`:217 (item) |
| `crates/shared/wyrd-loader/src/parse.rs` | `parse_rejects_missing_kind`:353 (item); `parse_accepts_multi_doc_yaml`:420 (item); `parse_preserves_source_path`:464 (item); `parse_collects_every_envelope_violation`:490 (item); `parse_rejects_inline_file_symlink_outside_workspace`:527 (item) |
| `crates/shared/wyrd-loader/src/path.rs` | `path_discovery_does_not_follow_symlink_directories`:265 (item) |
| `crates/shared/wyrd-loader/src/resolve.rs` | `resolve_path_rewrites_durable_ref_and_loads_sibling`:253 (item); `resolve_rejects_multi_document_path_targets`:282 (item); `resolve_loads_transitive_path_references_in_discovery_order`:306 (item) |
| `crates/skald/skald-agent/tests/integration/agent_construction.rs` | `openai_prompt`:10 (item); `agent_with_prompt_overrides_prompt_reference`:49 (item) |
| `crates/skald/skald-agent/tests/integration/agent_fields.rs` | `test_prompt`:14 (item) |
| `crates/skald/skald-agent/tests/integration/agent_journey.rs` | `test_prompt`:178 (item) |
| `crates/skald/skald-agent/tests/integration/construct.rs` | `prompt`:37 (item); `unknown_runtime_local_tool_uses_wyrd_code`:305 (item) |
| `crates/skald/skald-agent/tests/integration/delegate.rs` | `test_prompt`:196 (item) |
| `crates/skald/skald-agent/tests/integration/loop_basic.rs` | `build_agent`:15 (item); `run_prompt_sends_rendered_prompt_verbatim_on_first_turn`:188 (item); `run_prompt_provider_mismatch_returns_409_provider_mismatch`:261 (item); `run_prompt_missing_variable_emits_prompt_error`:289 (item) |
| `crates/skald/skald-agent/tests/integration/loop_conversation.rs` | `test_prompt`:386 (item) |
| `crates/skald/skald-agent/tests/integration/loop_gemini.rs` | `agent_run_executes_gemini_loop_against_mock_provider`:61 (item) |
| `crates/skald/skald-agent/tests/integration/session_journal.rs` | `test_prompt`:745 (item) |
| `crates/skald/skald-prompt/src/builder.rs` | `openai_chat`:90 (Errors); `openai_responses`:130 (Errors); `anthropic`:164 (Errors) |
| `crates/skald/skald-prompt/src/coerce.rs` | `provider_name_from_py`:31 (Errors) |
| `crates/skald/skald-prompt/src/error.rs` | `from`:84 (item) |
| `crates/skald/skald-prompt/src/prompt.rs` | `with_system`:89 (Errors); `with_tool_result`:127 (Errors); `with_role_text`:186 (item); `unsupported_role`:277 (item); `wrong_provider`:285 (item); `openai_chat`:445 (Errors); `openai_responses`:477 (Errors); `anthropic`:507 (Errors); `gemini`:537 (Errors); `vertex`:565 (Errors); `image_url`:732 (Errors); `set_request`:925 (Errors); `append_native_json_content`:1513 (item) |
| `crates/skald/skald-spec/src/authoring.rs` | `native_request`:273 (Errors); `portable_request`:328 (Errors); `lower`:386 (Errors); `check_format`:451 (Errors); `openai_chat`:480 (Errors); `anthropic`:561 (Errors); `google`:625 (Errors); `refuse`:701 (Errors); `within`:717 (Errors); `positive`:734 (Errors); `stops`:745 (Errors); `to_body`:764 (Errors); `compile`:798 (Errors) |
| `crates/skald/skald-spec/src/prompt.rs` | `eq`:60 (item); `serialize`:73 (item); `deserialize`:81 (item); `schema_name`:90 (item); `json_schema`:94 (item); `bind_mut`:324 (Errors); `bind_media_mut`:346 (Errors); `render_binds_dollar_brace`:1192 (item); `render_binds_mustache`:1201 (item); `render_binds_mixed_syntax_in_one_pass`:1210 (item); `prompt`:1227 (item); `media_placeholder_in_system_message_is_rejected`:1280 (item); `prompt`:1517 (item) |
| `crates/skald/skald-spec/src/request.rs` | `fmt`:135 (item); `serialize`:141 (item); `deserialize`:147 (item); `schema_name`:156 (item); `json_schema`:160 (item) |
| `crates/vala/vala-eval/src/orchestrator/judge.rs` | `agent_for`:137 (item) |
| `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs` | `resolve`:35 (item); `inline_agent_ref`:81 (item) |
| `crates/wyrd-spec/src/card/agent.rs` | `agent_spec_inline_prompt_roundtrips`:291 (item); `agent_spec_card_prompt_uses_single_version_field`:315 (item); `agent_card_uses_the_shared_envelope_without_client_derived_state`:343 (item) |
| `crates/wyrd-spec/src/card/mod.rs` | `prompt_spec`:2500 (item) |
| `crates/wyrd-spec/src/card/prompt/spec.rs` | `deserialize`:59 (item) |
| `crates/wyrd-spec/src/error.rs` | `from`:4036 (item) |
| `crates/wyrd-spec/src/graph/mod.rs` | `build_ignores_external_refs`:241 (item); `build_derives_sibling_edges_from_nested_refs`:261 (item); `graph_ready_submissions_preserves_authored_version_selection`:305 (item); `duplicate_submission_identity_is_rejected`:323 (item) |
| `crates/wyrd-spec/src/metadata.rs` | `prompt_spec`:386 (item) |
| `crates/wyrd-spec/src/reference.rs` | `unresolved_card_ref_paths_include_nested_slots`:896 (item) |
| `crates/wyrd-spec/src/refs/mod.rs` | `agent`:539 (item) |
| `crates/wyrd/wyrd-auth/src/card_scope.rs` | `resolve_caps_at_max_scope_cards`:599 (item) |
| `crates/wyrd/wyrd-cards/src/agent.rs` | `__new__`:103 (Errors) |
| `crates/wyrd/wyrd-cards/src/prompt.rs` | `converts_holder_metadata_to_prompt_spec`:878 (item) |
| `crates/wyrd/wyrd-cards/src/prompt/io.rs` | `prompt_card`:139 (item); `yaml_native_prompt_card_preserves_extension_options`:203 (item) |
| `crates/wyrd/wyrd-cli/src/card.rs` | `parse_kind`:745 (item) |
| `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs` | `write_prompt`:72 (item); `write_multi_card_service`:88 (item) |
| `crates/wyrd/wyrd-gateway/src/adapter/http.rs` | `dispatch`:641 (item) |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs` | `insert_test_card`:708 (item) |
| `crates/wyrd/wyrd-server/src/components/cards/resolve.rs` | `submission`:724 (item); `sibling_prompt_reference_is_not_external`:779 (item) |
| `crates/wyrd/wyrd-server/src/http/error.rs` | `sample_errors`:595 (item) |
| `crates/wyrd/wyrd/tests/umbrella_integration.rs` | `prompt`:40 (item) |

## Ordered behavioral scenarios and execution discipline

Execute one scenario at a time through RED → minimum GREEN → REFACTOR. Record
the exact nonempty selector, expected behavioral RED (not an environment or
compile failure), and GREEN. Reuse these existing test owners and controlled
provider fixtures. Documentation and generated/static updates use source and
regeneration proof, without artificial behavioral tests.

1. **Grant normalization and delegation (FIND-001-1):** mixed roles retain explicit grants; real token exchange cannot increase either party's authority; exact representable results succeed and unrepresentable results return the approved refusal without a token. Exercise wildcard/AnyOf and both actor/subject directions.
2. **Loaded Agent routing (FIND-001-2):** construct from a gateway Agent Card through Rust and public Python loaders; roundtrip the declaration; execute supported tool turns on the declared gateway. Missing dependencies refuse rather than use native credentials. Concurrent default/override calls preserve each other and stored identity.
3. **Actual public provider path (FIND-001-3):** real Agent/Run calls reach named Anthropic/Google/Jev ingress; preserve slash models; Google deployment spelling `vertex` is valid while actual local Vertex remains unsupported. Jev retains its single-call/no-general-tools/no-streaming constraints.
4. **Initial-target compatibility (FIND-001-6):** incompatible Agent initial adapters fail before IO, including OpenAI/Google targeting Vertex; compatible targets succeed. Existing direct-client translation and explicit cross-adapter fallback still work.
5. **Literal identities (FIND-001-8):** both forms/builders reject target placeholders and controls before registration/render dispatch; real content variables still render and slash model IDs remain exact.
6. **Schema acceptance (FIND-001-5):** shared compiler/builder and public Card registration reject invalid schema syntax and detectable unsupported destination features before persistence/IO; valid schemas preserve exact authored values and output envelope.
7. **Typed media binding (FIND-001-4):** both mismatch directions and conflicting declarations refuse before mutation/IO; matching portable content retains kind/order; native bindings retain their existing semantics; public Python/judge binding uses the shared rule.
8. **Media source validity (FIND-001-11):** malformed/oversized base64, unsupported MIME and Google File URI refuse at the same shared boundary; valid supported sources and provider file identifiers remain usable.
9. **Batch pagination (FIND-001-7):** newer denied restricted batches cannot displace older visible open batches; small pages expose usable continuation and correct has_more under all, explicit and mixed grants, across tenants.
10. **Batch dispatch race (FIND-001-10):** deterministic claim barrier separates early authorization from access replacement; actual create and replay refresh refuse an unauthorized new snapshot before IO. Preserve fresh-claim release, uncertain-dispatch fencing and authorized success.
11. **Provider evidence (FIND-001-9):** retained spans report actual adapter provider separately from arbitrary deployment names, including fallback attempts; align with the existing live-span mapping without mutable post-call lookup.
12. **Three-SDK authored preservation (FIND-001-13):** portable/native × standalone/inline all compare authored JSON, hash identity and derived introspection after GET/hydrate. Inline Prompts materialize as `<agent>-prompt` under current architecture. Add only missing assertions/cases to existing owners.
13. **Installed setup through downloaded Agent execution (FIND-001-14):** after direct-request evidence, register/download/load/check/run all four provider Agents, then restart and rerun those downloaded Agents with normal auth and real gateway plumbing.
14. **Consumer/static completion (FIND-001-12 and all changed contracts):** finish changed Rust rustdoc, Python stubs/exports, TypeScript declarations, error/schema generators and targeted docs. No checker or unrelated documentation campaign.

## Acceptance criteria and finding closure

| Criterion | Findings | Observable acceptance |
|---|---|---|
| R1-AC-01 | FIND-001-1 | Explicit grants survive mixed-role merge; both parties authorize every delegated operation; successful tokens carry exact authority and unrepresentable exchanges return the stable 403 with no token. |
| R1-AC-02 | FIND-001-2 | Loaded/saved Agent keeps declared route; direct/public execution and invocation-local override use the complete existing loop; missing gateway dependency cannot call native. |
| R1-AC-03 | FIND-001-3 | Anthropic/Google/Jev Agent calls traverse real public ingress with exact named target/model; Jev and local Vertex constraints remain explicit. |
| R1-AC-04 | FIND-001-4 | Portable media kind mismatch/conflict is rejected by shared binding before mutation/IO; valid ordered/native bindings survive. |
| R1-AC-05 | FIND-001-5 | Invalid or detectably unsupported output schemas fail in shared authoring before storage/IO; valid native/portable/builder outputs preserve intent. |
| R1-AC-06 | FIND-001-6 | Agent requested target enforces the approved matrix; direct protocol conversion and explicit fallback behavior survive. |
| R1-AC-07 | FIND-001-7 | Authorized batches remain reachable through bounded, correctly filtered pagination under all relevant grant shapes. |
| R1-AC-08 | FIND-001-8 | Model identity is literal and never content-rendered; invalid identities reject early, valid slash IDs survive. |
| R1-AC-09 | FIND-001-9 | Retained provider/deployment/model values describe the actual admitted attempt, matching live telemetry provider semantics. |
| R1-AC-10 | FIND-001-10 | Every dispatched batch call is authorized against its routing snapshot; concurrent replacement cannot reuse stale authorization or strand an undispatched fresh claim. |
| R1-AC-11 | FIND-001-11 | Shared literal/runtime media inputs enforce existing payload limits, encoding, MIME and URI constraints without fetching or dropping content. |
| R1-AC-12 | FIND-001-12 | Changed Rust items satisfy AGENTS §16, including private/test items and applicable Errors/Panics/cancellation contracts. |
| R1-AC-13 | FIND-001-13 | All first-class SDKs prove two-form standalone/inline authored JSON/hash preservation and derived introspection across real register/GET/hydrate. |
| R1-AC-14 | FIND-001-14 | After direct setup proof, register/download/load/inspect/execute each provider Agent, then restart and re-execute the same downloaded Agents with unchanged route/provider/model, normal auth, correct upstream credentials and correlated retained evidence. |

## Focused commands and proof boundaries


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

### Exact first-class language preservation selectors

Extend the existing named preservation/navigation owners; keep these selected
at their existing integration tier. If a focused regression is added instead,
record its actual exact selector before running it and retain the owning lane
selection. The commands below are planned execution, not reported passes.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_hydrated_agent_prompt_card_navigation
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^hydrated agent prompt card navigation$'
```

The direct Python Agent and Prompt runtime regressions belong in their current
public Python unit owners, not a Rust-hosted interpreter. Record/run each new
exact pytest selector. The three-SDK Agent-route additions use existing focused
route owners; Python and TypeScript commands are:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_gateway_inference.py::test_agent_prompt_routes
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t '^agent prompt routes$'
```

Use normal existing Python/Node native setup/build prerequisites, once when
needed. Run Cargo-backed work sequentially; do not compete with another active
build. Before diagnosing a failure rerun with tracing and read the trace. A
missing subscriber is a harness gap. No positional Rust filters or zero-test
passes count as proof. Existing green tests must gain the missing assertions
before they can close a finding. New exact regression names are implementation
choices, not invented execution evidence in this packet.

After focused behavior is green, run only existing static/type/boundary lanes
for the actual write set: `mise run fmt`, `mise run lints`,
`mise run codegen:check`, `mise run check:deps`; add
`mise run check:tenant-isolation` for changed SQL signatures/queries;
`mise run py:format`, `mise run py:lints`, `mise run py:typecheck` for the Python
boundary; `mise run ts:format:check`, `mise run ts:lints`,
`mise run ts:typecheck`, `mise run ts:napi:check` for the TypeScript boundary;
and `mise run docs:check` for the required gateway docsite guides. Regenerate from
owners before checking. Do not run full user-journey suites, family suites or
`mise run gate` as remediation proof. The integrated aggregate runs once at
change review; these focused commands do not claim it passed.

## Preserved behavior and non-goals

Retain tenant RLS/TenantConn, credential sealing/provider/exact-host bindings,
SSRF/DNS pinning, upstream header protections, effective subject authority,
SYSTEM-as-actor-only attribution, nonblocking existing audit/capture outbox,
budget/accounting settlement and provider-held batch pinning. Do not make
permission changes alter Bifrost or unrelated operation families. Do not make
request errors crash shared services. Already admitted consistent snapshots
keep their current lifecycle semantics.

Keep the approved required route and whole-invocation override; both Prompt
forms, all eight portable settings and text/image/document core; exact authored
hashes; normal native/provider behavior; supported direct-client translations
and explicit fallback conversions. Keep the existing credential-first and
single/list deployment CLI/dev setup and saved-key restart behavior. Current
inline Prompt materialization and async client construction remain authoritative.
Workflow stays removed; independent Eval DAGs/fields and surviving judges stay.

Do not add new scopes, model mappings/aliases/catalogs, a direct external route,
local Vertex gateway support, portable audio/video, a hosted Agent job API,
replacement loop/graph/transport, media upload/fetch/OCR subsystem, new audit
queue/WAL, permanent checker, compatibility alias or deployed-data migration.
Approved adjacent changes are preserved. Use `$wyrd-ui` only if a necessary
consumer correction reaches UI; no visual redesign is part of this task.

## Completion record and stop conditions

For each FIND-001-1–14 and R1-DOC-AC-01–03 record the source correction, exact RED/GREEN or justified
static proof, focused regression results and required owner checks. Do not
claim a finding closed because a related selector passed without exercising
the failing producer. Record later-commit reuse/no-op proof where appropriate.
Missing actual selectors must be discovered and recorded rather than guessed.
Keep the original task's historical evidence intact and append remediation
results to this task. No source implementation has been performed by review.

Return the implemented task to `$wyrd-task-review` with the same original base,
a new immutable candidate, original task, this remediation and prior review
ledger. Reassess the cumulative change, not only the fix diff; preserve stable
finding IDs. The approved r6 delegation choice removes the former material
stop. Stop only if a different material behavior/security/ownership/persistence
decision is genuinely required, with concrete evidence; routine helpers,
fixtures, type conversions and execution mechanics are implementation-owned.
No merge, push or deployment is authorized by this packet.
