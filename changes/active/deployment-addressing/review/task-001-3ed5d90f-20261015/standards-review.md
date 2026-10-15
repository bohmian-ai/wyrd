# Repository standards review

Result: **PASS**. No material repository-standards finding proposed.

## Subject and method

Reviewed immutable source `/tmp/wyrd-task-review-3ed5d90f`, original base
`1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` through candidate
`3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`. The complete cumulative changed-path
inventory and diff were the scope; `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`
to candidate located the R2 additions. Reviewed current owning modules,
declarations, query signatures, representative callers and test helpers rather
than accepting a gate as proof of ownership. The archive has no `.codegraph/`;
CodeGraph therefore does not apply inside it. The orchestrator supplied the
common navigation map; it was a starting point, not a finding ledger.

This is a standards audit, not the implementation-acceptance or independent
Ponytail pass. Adjacent changes are approved by the user. No source was edited,
no older checkout was built, and no Cargo, Python, TypeScript, or docs check was
run by this reviewer.

## Authority and changed-surface coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, the router at
`architecture/references/README.md`, `architecture/wyrd-design.md`, and
`architecture/wyrd-doctrine.mdx`.

| Changed surface | Additional applicable authority | Source examined and rule result |
|---|---|---|
| Rust contracts, Card kinds/references/graphs, Agent route, Prompt serialization/hash, gateway deployment and permission scopes | References `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md` | `wyrd-spec` Card/Prompt/Agent/gateway/auth/error modules and schema example; Skald `authoring.rs`, `prompt.rs`, `request.rs`, `media.rs`; shared loader and runtime permission modules: PASS for pure declarations, existing typed identities, shared validation, derive-backed public errors and source-owned generated types. Workflow removal is explicit approved scope. |
| New R2 JSON Schema dependency and all existing test consumers | Foundation/dependency rules in AGENTS §§2–4, reference patterns and Rust core; R2 approved correction boundary | Workspace `Cargo.toml:119`, `Cargo.lock:4104–4128,6095–6109`, `skald-spec/Cargo.toml`, `wyrd-spec/Cargo.toml`, workspace-hack, MCP principals validator and server OpenAPI validator: PASS. See dependency assessment below. |
| Runtime Agent, Prompt builder/media/setter, provider projection, shared client invocation | Rust core, patterns, errors, testing references; design Prompt/Agent and client model | `skald-agent` Agent/gateway/loop/Python modules; `skald-prompt` builder/prompt/media and response projection; provider endpoint/native request handling; shared `gateway_caller.rs`, `observe/invoke.rs`, state and hydration: PASS for retained-state owners, pure compiler/binder, IO at runtime boundaries, shared transport, no new durable SDK routing authority. |
| Server gateway configuration, ingress, invocation, engine, capture, Batches and SQL | `wyrd-security-posture.md`; `v1/00-foundations/permission-model.md`; Bifrost design table identity/durability/audit; references telemetry observations, errors, patterns and testing | Gateway adapter/engine/routing/snapshot/credential, server administration/invocation/batches/caller/capture/ingress/routes/ledger, SQL migration/query/row types: PASS for actual SQL capability types, transaction ownership, tenant-bound state, secret isolation, provider network-policy reuse, existing Scribe outbox and explicit publisher/Card attribution. |
| Auth issuer/exchange, principal Card scope and HTTP error mapping | Security posture authorization/delegation/audit; permission-model; errors and agent-harness references | `wyrd-runtime::Permission{,Set}`, `wyrd-auth` issuance/exchange/Card-scope, server JWT/error mapping, error derive reverse projection: PASS. Exact-intersection refusal uses the catalog and existing issuer/audit boundary, not a second grant service. |
| Python SDK wrappers, authored stubs, exports, runtime/integration/unit/type fixtures | References `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, errors and testing | SDK observe/state/module aggregation, retained owner-crate optional Python projections, public `wyrd.agent`, `prompt`, `gateway`, `cards`, `state` and observe stubs, shared conversion helper, Python installed-CLI and gateway framework tests: PASS for Rust-owned behavior, explicit conversion, shared runtime bridge, matching exports/stubs and public import paths. |
| TypeScript native/wire/ergonomic SDK and tests | `languages/typescript-guide.md`, errors and testing | Native cards/Run projection and `hydrated_prompt`; generated native declarations; generated Card/error-code types; public index PromptCard/AgentCard/state/invoke and integration/type tests: PASS. R2 derived Prompt metadata is read from shared Rust, not separately derived or durably stored; nullable schema/name and readonly arrays have explicit public types. |
| CLI, MCP, real-server and first-class-language journeys | `languages/agent-harness.md`, testing, spec-driven-development and implementation-execution references; `TESTING.md` and mise commands | CLI gateway/card commands and lifecycle/gateway tests; MCP gateway/principals/verification consumers; server registration/OpenAPI tests; installed-CLI Python driver plus Rust child; Rust/Python/TS register/hydrate/route journeys: PASS for typed inputs, public clients/server boundary, owning-language runtimes, Postgres wrapper and explicit focused selectors. New child test earns an external target by loading a real downloaded graph, using client/server IO and being driven by the installed CLI journey. |
| Changed Rust documentation, including helpers, serde/schema implementations and tests | AGENTS §16; agent-rules rustdoc; Rust core and maintainer-style references | Cumulative added/modified item inventory checked against current modules, including R1 authoring/Prompt/ProviderName docs and R2 `check_output_schema`, `Prompt::new`, native caller, `hydrated_prompt`, validator helpers, `GATEWAY_AGENTS`, `Retained` fields and `cli_gateway_agents`: PASS for substantive intent/invariants and fallible `# Errors`/test `# Panics` coverage; no new missing-rustdoc finding. Unchanged private helpers are not swept into a new style round. |
| Docsite, machine-readable docs, examples, YAML fixtures, UI Workflow removal | Doctrine/public contract, approved developer-docsite-rebuild structure, testing/generation rules; current UI projection boundary | Connect pages, setup capture-policy prerequisite and framework snippets, quickstart/identity/card prose, llms indexes, API doc/type generator, gateway-setup examples and fixtures; UI kind registry/mock projections and removed Workflow workspace: PASS for canonical Card/route/deployment vocabulary, source-generated wire declarations, six-section Connect placement and removal of retired UI surfaces. No new client-only durable facts. |
| CI, packaging, release metadata, feature and dependency checks | AGENTS verification/feature/gate rules; operations deployment-and-release and reliability-and-recovery | Workspace manifests/lock/hack; nextest/mise tasks; release workflow retry/swap fixes; telemetry package/release entry; dependency script and change-selector test adjustments: PASS for canonical command ownership, no new production testing feature, no boundary glob widened to hide the change, no generated source written by a build script. Release fix effectiveness remains the recorded qualification limit, not newly executed proof. |

## Material rule assessments

### Pure validation and dependency direction — PASS

`skald-spec` remains the owner of synchronous Prompt validation; R2 replaces
the hand-written subset checker there with the existing workspace JSON Schema
dependency. The workspace disables its default features. The locked normal
dependencies of `jsonschema` and `referencing` contain no HTTP client or Tokio;
`workspace-hack` is only a dev dependency of `skald-spec` and `wyrd-spec`.
`check_output_schema` refuses unknown/custom meta-schema drafts, calls bundled
meta-validation, and does not resolve user reference targets. Neither parsing
nor schema validation became async, and no PyO3/server/SQL type entered the
foundation. The existing server/MCP test consumers use the updated API and
retain their format-validation assertions. This audit does not infer a
runtime proof from a manifest alone: the actual shared function and its
construction/registration callers were inspected.

### SQL capability and lifecycle — PASS

Actual production gateway query signatures (`wyrd-sql/src/queries/gateway.rs`,
including `gateway_batches:1004–1020`) accept `&mut TenantConn<'_>`, not a raw
pool, naked connection or externally supplied sqlx transaction. Query helpers
do not commit/rollback the borrowed capability. Server service/invocation and
batch owners acquire the transaction and commit their own connection. Batches
pruning adds authorization eligibility to the bounded SQL query; it does not
add a caller-controlled tenant predicate or change the RLS boundary. The
changed gateway migration keeps tenant-qualified keys and foreign keys.

### Audit, credentials and provider transport — PASS

Permission decisions remain before protected work and are staged through the
existing process outbox; no audit failure becomes a public operation failure.
Provider adapter construction uses only the admitted deployment's resolved
credential. Existing Skald endpoint-policy transport performs the effective
destination screening; no new unscreened schema retrieval or provider HTTP
path was introduced. Gateway capture retains server-owned publisher identities
and the authorized requested Card/Run correlation rather than trusting row
fields. The capture-policy guide now explicitly enables the existing metadata
policy, preserving the server's default-disabled policy and secret redaction.

### SDK projection and generated declaration parity — PASS

Native TypeScript `hydrated_prompt` (`native/src/cards.rs:645–663`) reads
`Prompt.variables`, `media_variables` and `response_type` while returning the
stored Card separately. The ergonomic `PromptCard` (`src/index.ts:2359–2397`)
projects these values with precise readonly/null types; standalone and
Agent-navigated state paths use the same projection. Route parsing remains in
Rust and both language SDKs call the shared invocation owner. Authored Python
stubs are kept distinct from generated native/TS declarations. No SDK copies
durable gateway validation, transport or registry logic.

### Test integrity and rustdoc — PASS, recorded verification limits

The R2 installed-CLI child uses actual `Agent::from_card`, downloaded pinned
Prompt resolution and the normal public gateway caller. Its IO warrants async;
the explicitly gated external test is called by the owning real-server
journey. The one bounded eventual-evidence poll observes Bifrost publication;
it is not a new retry around failed inference. Its constants, row type,
fields, test and panic conditions are documented.

The reported first merged-tree P9 error is explained as a stale installed
Python native extension; the evidence records a traced rerun, the absent
rebuild behavior of `uv run`, `py:setup`, and subsequent P9/P10 green runs.
No assertion was changed to clear that 502. The separate narrowing from
whole-Agent-spec equality is disclosed and remains for the implementation
reviewers to judge against original identity/save-load obligations; this
standards pass does not waive a behavioral obligation or manufacture a
retrospective RED result.

Reported focused tests, codegen, native TS build, language typing/lint,
dependency and docs checks have precise commands/evidence in the committed R2
packet. They were not independently rerun here. The original task's aggregate
change gate remains a later integrated-change obligation; it is not claimed
complete by this task report.

## Non-blocking note

`skald-spec/src/prompt.rs:268–271` puts two `use` statements inside the new
`output_schema_field` helper. `architecture/agent-rules.md` requires imports
at module scope. Moving them to the existing import block would follow that
rule, but this has no behavioral/public-contract consequence and does not
justify `FIX_REQUIRED` under the active task-review threshold. It is not a
material finding or a separate documentation round.

## Proposed material findings

None. Standards result: **PASS**. Independent acceptance, resilience and domain
reviewers must still determine task completion and the Agent save/hash scope.
