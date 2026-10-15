# Repository standards review

Result: **FAIL**. One material proposed finding, `REP-R2-001`, continues the
shared schema-validation problem tracked as `FIND-001-5`. This report does not
give the task an acceptance verdict.

## Subject and method

- Immutable source: `/tmp/wyrd-task-review-9516afcd`.
- Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
- Candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.
- Previous candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
- Inputs: shared subject/navigation map, complete cumulative diff, remediation
  diff, approved r6 specification, original task, original documentation
  inventory, and appended R1 execution evidence. The remediation diff was a
  locator; it did not replace cumulative standards coverage.
- No discovery reviewer report from this round was read. No source was edited,
  no build/test was run, and no live-branch change was included. The archive has
  no `.codegraph` directory; CodeGraph was therefore inapplicable there.
- Adjacent changes have explicit human approval and were not classified as
  scope drift merely because they are outside deployment addressing.

## Authority and surface coverage

The router is `architecture/references/README.md`. All rows also apply
`AGENTS.md` and `architecture/agent-rules.md`. Architecture references are
subordinate to their owning Wyrd authority. Inspection included surrounding
source and consumers, not only added lines or a green-check claim.

| Changed surface | Applicable authority | Source and consumer coverage | Result |
|---|---|---|---|
| Card/Spec/kind contracts, execution routes, references, generated schemas | `wyrd-design.md` doctrine, Client model, Prompt, Agent, registry/spec-file authoring; `wyrd-doctrine.mdx`; references `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/errors.md`, `languages/agent-harness.md` | `wyrd-spec` Card/Agent/Prompt/deployment/access/error/graph/reference changes; schema example; loader and Card-holder consumers; server registration and OpenAPI test; generated TS wire projections | PASS except output-schema validation below |
| Permission sets, typed gateway grants, token issuance/delegation | `wyrd-security-posture.md` authorization/delegation/audit; `v1/00-foundations/permission-model.md`; references `rust-core.md`, `errors.md`, architecture patterns | `wyrd-runtime::permission` normalization/subsumption/intersection; `wyrd-auth` issuance/exchange/card scope; stable unrepresentable-delegation error and HTTP projection; gateway request permission consumers | PASS for inspected standards: typed ownership, common issuer, no parallel authority store, nonblocking audit seam |
| Portable/native Prompt construction and binding | `wyrd-design.md` Prompt/Agent; AGENTS §§3–6,9–10,15–16; references Rust core, patterns, errors, testing | `skald-spec::{authoring,prompt,media,request}`; `skald-prompt::{builder,coerce,prompt,media,error}`; `wyrd-spec::card::prompt`; Card wrappers and Python authoring tests | FAIL: REP-R2-001 |
| Agent runtime, caller and pre-run projection | Same protocol/ownership authority; references Rust core, patterns, telemetry observations, testing | Existing `Agent` owner/from_card/to_spec/loop; gateway registry/caller seam; shared client native ingress producer and `Run::{invoke,invoke_on}`; SDK boundary conversions; runtime/tool-loop/SDK journeys | PASS for standards: runtime dependencies stay outside declarative Card/Spec; invocation-specific state is not a persisted SDK contract |
| LLM judge integration | `wyrd-design.md` Verifier/Agent; references `domain/evaluation.md`, `domain/telemetry-observations.md`, patterns/errors/testing | Vala judge preparation, Agent resolver/cache, server gateway caller, realtime verification consumers and judge fixtures/tests | PASS for standards: Vala consumes Skald, server supplies the serving/auth context, no Skald-to-Vala dependency introduced |
| Gateway runtime/adapters/credentials/endpoint configuration | Security posture external URL/secret controls; operations `deployment-and-release.md`, `reliability-and-recovery.md`; references patterns/Rust/errors/telemetry | `wyrd-gateway` adapter, endpoint policy, credential resolver, routing/engine/snapshot; server config/boot; invocation/ingress/routes/service; request projection tests | PASS for inspected standards: existing endpoint screening/transport, no raw-key forwarding or second retry/transport owner added |
| Gateway batch SQL and accounting | Agent rules SQL capabilities/transaction ownership; security posture tenant isolation; operations recovery; references patterns/Rust core | Actual signatures/fields in SQL gateway queries, row types, server batches/ledger/service/invocation, auth/registration/boot consumers; migration foreign keys/RLS; batch tests | PASS: production query APIs use `&mut TenantConn`; cross-tenant boot uses `OperatorPool`; no new raw-pool signature or callee-owned commit on borrowed TenantConn |
| Gateway capture and analytical table mappings | `bifrost-design.md` system boundary, table/row identity, internal writes; security/audit authority; telemetry-observations reference | `GatewayCapture`/attempt span projection, existing Scribe outbox handoff, gateway call table definition and schema mapping, capture tests | PASS for standards: one outbox path, typed/tenant-qualified identity, fields mapped by name, admitted adapter provider kept separate from deployment |
| Shared client, hydrator and loader | Client model; registry/spec-file authoring; references patterns/Rust/errors | `Cards`, existing graph traversal/hydrator, state and loader parse/path/resolve changes; Workflow-only traversal removal; runtime/offline callers; Rust/Python/TS register/hydrate tests | PASS for ownership/layering; no new durable graph/registry implementation in a language SDK |
| Python native boundary/package/stubs | AGENTS §§7–8; references `pyo3-boundaries.md`, `python-api-and-stubs.md`, errors/testing | Native registration, retained optional owner-crate Python features, shared route conversion, SDK observe wrapper; public package exports and authored stubs; public-import authoring/runtime/journey tests | PASS for inspected boundary/export alignment; retained owner wrappers remain approved migration state; pure contracts remain PyO3-free |
| TypeScript native/wire/ergonomic layers | Client model; references `typescript-guide.md`, errors/testing | Native cards/Run invocation parser; generated declarations; wire/error-code generation; public index route options; integration/type tests; Workflow projection removal | PASS: typed route passed to shared Rust invocation, generated wire fields retain their Rust spelling, no parallel routing engine |
| UI removal consumers | Doctrine/kind catalog; AGENTS UI/server boundaries | Removal of Workflow workspace and kind entries; Card feature types/tests; mock fixtures; CSS cleanup | PASS: narrow removal of the retired presentation, no new UI-only contract or permission path |
| CLI/installed journey/examples | Client model, security/credential and operations authority; references agent harness, testing workflows, spec-driven development, implementation execution | Gateway one/list document decoder and ordered put output; Card commands; CLI tests; Python installed-server driver, fresh SDK subprocesses, four-provider fixtures; Rust/TS/Python example projections | PASS for tier/runtime placement and existing-owner reuse; historical RED limitations are preserved as limitations, not represented as behavioral RED evidence |
| Docsite guides and machine-readable indexes | Approved `developer-docsite-rebuild` r2 and Connect task structure; protocol/Client model/error authority; AGENTS docs gates | Published Connect setup/framework pages, frontmatter/order/links, generated `llms` indexes; stock-client/framework inference tests; docs command source | Source structure PASS; human narrow-width visual verification remains unperformed in the evidence record and is not silently marked verified here |
| Build/release/check tooling | AGENTS feature/dependency/generated-output rules; operations deployment/release; testing workflows | Workspace/manifests/lock changes, nextest and mise changes, dependency check, TS/schema generator, release retry/swap and telemetry publication metadata | PASS for inspected standards: Workflow-only lanes removed, installed journey extends existing lane, production test-support exclusion retained; swap success remains a release-runtime hypothesis, not locally proven |
| Planning/evidence artifacts | References spec-driven development, implementation execution | Approved revision and remediation provenance; focused command table; user-directed finding 14 kept distinct from independently discovered findings | PASS for provenance/reporting; this is not a task-acceptance judgment |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Declarative foundational contracts remain IO/async/PyO3/SQL-free | `wyrd-spec/Cargo.toml`; AgentSpec/ExecutionRoute and PromptSpec; shared pure Skald authoring compiler | PASS |
| Durable behavior stays in existing Rust server/service owners; SDKs project shared client | Registration service/resolve, GatewayInvocation/GatewayBatches, shared client caller/invoke, SDK wrappers and manifests | PASS |
| Actual SQL capability boundary and caller-owned transaction composition | `wyrd-sql/src/queries/gateway.rs` signatures throughout; gateway service/ledger/batches and auth issuance; `TenantConn` consumers don't receive naked transactions/pools | PASS |
| Tenant values come from verified caller, not request-selected tenant | Gateway invocation/admin/batches take `Caller`; SQL binds through tenant connection; capture tenant context derives from admitted call | PASS |
| Secrets redacted and egress screened using existing owner | `CredentialResolver`/`ProviderSecret`; managed secret bindings; `EndpointPolicy` resolver uses screened answers and disables proxy/redirects; gateway config rejects production builtin overrides | PASS |
| Public errors derive from common catalog and remain structured across SDK/native boundaries | unrepresentable delegation variant; shared `code_to_wyrd_error`; derive-backed fieldless reconstruction; provider-envelope conversion; Python/TS boundary converters | PASS |
| Validation must apply at shared trust boundary before protected persistence/provider work | `AuthoredPrompt::compile` and `Prompt::new` share `check_output_schema`, but its keyword fallthrough permits invalid known JSON Schema keywords | FAIL, REP-R2-001 |
| Existing retained ownership, synchronous pure validation, bounded IO | Agent retains route/caller; invocation owns call context; pure compiler/model/media/schema checks synchronous; existing transport/deadline/cancellation owners retained | PASS |
| Python runtime lifetime belongs to Python; Node lifetime belongs to TS tests | Installed CLI journey's Python subprocess and TS fresh-client tests; no Rust test embeds a foreign interpreter to prove the new behavior | PASS |
| Public Python exports/stubs and TS generated declarations project native surface | Python agent/observe/gateway/Prompt package stubs and registration; TS native cards/Run and index/declarations; generated source edits accompany codegen evidence | PASS for source parity inspected |
| Generated artifacts are source-driven | Rust schema roots, error derive, `gen_ts_types.py`, docs generator, corresponding generated wire/error/index outputs | PASS; regeneration is reported by implementer, not rerun here |
| Rustdoc hard gate includes changed private/test/helper items | Original 103-function inventory plus associated type/modules/constants and remedial additions inspected as described below | PASS for audited inventory and additional changed items |
| Gates cannot be weakened to hide implementation failures | `mise.toml`/nextest/deps changes remove retired Workflow paths and add the real installed journey; TS placeholders retain literal text without suppression | PASS for inspected gate mutations |
| Documentation proof must state its real scope | R1 evidence explicitly admits missing behavior RED for some fixes and missing narrow-width visual inspection | PASS for candid recording; neither omitted proof is counted as performed |

## Proposed material finding

### REP-R2-001 — Important — VIOLATION — shared schema validation accepts invalid standard keyword values

**Stable prior finding:** `FIND-001-5` (same source and consequence, not a new
finding per provider or SDK).

**Governing rule:** AGENTS §§9–10 and15 require shared Rust-owned validation at
the contract/trust boundary; `wyrd-design.md` declares `response_format` a JSON
Schema, not an arbitrary unchecked map. The approved R1 correction already
requires invalid schemas to be refused before persistence or upstream IO. The
standard itself requires `minimum` to be numeric: [JSON Schema Validation
2020-12, §6.2.4](https://json-schema.org/draft/2020-12/json-schema-validation#section-6.2.4).

**Exact location:** `crates/skald/skald-spec/src/prompt.rs:255–310`, particularly
the unrecognized-keyword fallthrough at307 and success at310.

**Producer → consumers:** Portable `response_format` enters
`AuthoredPrompt::portable_request` (`authoring.rs:361–368`) and
`check_output_schema`. Native bodies decode into the existing provider request,
and `Prompt::new` (`prompt.rs:398–407`) obtains their output schema and calls
the same check. `PromptSpec::deserialize` (`card/prompt/spec.rs:65–72`) compiles
and validates this Prompt for registry decoding; `PromptSpec::new` recompiles
the authored form. The shared native builders use Prompt construction too.
Neither the Card validator nor subsequent typed provider bodies enforce the
types of the schema keywords stored inside the JSON value.

**Reachable counterexample:** A portable Anthropic Prompt with ordinary
messages, `max_output_tokens:64`, and this `response_format` passes every branch
of the new check:

```json
{"type":"object","properties":{"answer":{"type":"number","minimum":"bad"}}}
```

The root and nested `type` values are accepted; `properties` recurses; `minimum`
falls through unchanged. A numeric `$ref` similarly falls through. The existing
native JSON-schema options and authoring wrappers reach the same path. This is
not a claim that every unknown provider extension must be rejected: `minimum`
is a known standard keyword with a fixed value type.

**Observable consequence:** A caller can register/store a malformed output
schema successfully, then have a model call fail upstream instead of receiving
the promised local/shared authoring refusal. It also contradicts the shared
validator's documentation claiming structurally valid JSON Schema. Existing
tests exercise invalid `type`/`required` and selected structural cases, so their
reported GREEN does not exercise this accepted malformed value.

**Required testable correction:** Complete schema-validity checking in the
existing pure `check_output_schema` boundary, preserving the separate
destination-compatibility check, authored bytes/unknown native option
preservation, and no reference/network resolution. Reuse the repository's
already-declared schema-validation facilities if they can run at this pure
boundary; do not add SDK-specific validators or a second registration-only
guard. The implementer must not silently redefine supported JSON Schema as the
few currently checked keywords. A focused shared-compiler negative case for a
malformed standard numeric/string keyword, in portable and native forms, plus
one extension of the existing authenticated registration refusal test proves
the missing boundary without a new harness. This report leaves final
decision-complete correction selection to independent validation.

## Rust documentation disposition

The original FIND-001-12 inventory is no longer an empty-doc inventory:
loader/test helpers; changed Agent construction/loop helpers and tests; Prompt
builders/Python conversion/errors; compiler fallible stages; Prompt
serde/schema methods; ProviderName associated `Err` and serde/schema methods;
judge preparation; Card/graph/ref/hash fixtures; CLI helpers; gateway dispatch;
registration/auth/error fixtures now have in-place documentation. The listed
test modules and function-local backup/external/model constants are documented.
`# Errors` was added to the previously identified fallible compiler/builder/
wrapper methods, and explicit assertion/panic paths in new compiler and
ScriptedGateway tests have `# Panics`.

This pass also inspected remedial route accessors/registry, shared route
conversion, error reconstruction, grant-intersection error/operations/test,
model/schema/media validators and tests, provider-matrix restriction, batch
visibility helper/query changes, capture projection/test, exchange/registration
and routing race tests, and authored-roundtrip helpers. They have substantive
intent/error/invariant documentation as applicable. No new rustdoc blocker is
proposed. Documentation of inherited fixture invariants on their immediate
helper is not expanded into speculative claims that every transitive fixture
assertion requires duplicated panic prose.

The invalid-schema claim in validator rustdoc is part of REP-R2-001's behavioral
defect, not an additional wording-only finding. Function-local imports and
temporary test-scenario labels that remain are not treated as behavioral
blockers or a reason to create a separate style-only round.

## Verification and limits

The appended evidence reports successful format/lints/codegen/dependency,
Python lint/type, TS lint, docs, OpenAPI integration, and focused tests. Source
inspection checked the relevant selectors and assertions as corroboration;
this reviewer did not rerun them, install packages, or start services. The
reported historical RED gaps cannot be repaired by pretending a compile error
was the expected behavioral RED. They do not alone demonstrate that the final
behavior is wrong.

The new schema counterexample is source-derived, not an executed failure in
this pass. No production credential, live provider, release ARM runner, manual
docsite viewport or aggregate gate was exercised. The spec's broader task
acceptance, prior-finding closure outside the standards lens, and final
severity/remediation ledger belong to the orchestrator and independent
validation.
