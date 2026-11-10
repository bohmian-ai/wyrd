# Repository standards review

Result: **FAIL**. Proposed findings: `REPO-R3-001` (the mandatory rustdoc gate, continuing FIND-TASK-001-3) and `REPO-R3-002` (the unreadable-source negative test does not pin the canonical error code).

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`. Immediate task base: `a387f2a47a145a867c8a4948966fcbd136af9961`. Candidate: `37b3c2f96b1bd425bf1f0923a1950d0363c40907`. Original cumulative base: `5234961d51798581a026834cfffd5efaadc226b4`. Reviewed the complete immediate task diff and the complete resulting changed modules, with original/prior candidate context where needed for documentation closure. HEAD still equals the candidate. No implementation, authority, task or other report was edited.

Authority: approved revision-4 `spec.md`, `TASK-001-R3-canonical-authoring-grammar.md`, root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md`, all four applicable topic guides (`rust`, `maintainer-style`, `errors`, `agent-surfaces`), `TESTING.md`, and the architecture sections mapped below. Historical standards/verdict evidence was used only to identify prior rule coverage and FIND-3's residual panic contracts; current source establishes closure. This specialist audits repository standards, not task acceptance or alternative abstraction design.

There is no `.codegraph/` directory. Enumeration found no nested `AGENTS.md` or `CLONES.md` in the changed client, loader, Rust SDK, examples or architecture directories. No Python, TypeScript, SQL, docs-site or UI implementation is changed, so their nested implementation rules do not apply to the write set.

## Authority coverage

| Changed surface | Governing authority | Source and consumer coverage |
|---|---|---|
| `authoring/mod.rs`: Segment/Location/field_path, Captures, FieldValue and conversions, Body, fields macro | Root Code and client/server/Card decisions; Rust Structure, Types, Imports, Ownership, Errors, Rustdoc; maintainer Layout/Methods/Documentation; design Client model, Kind catalog, In-memory authoring, Reference forms/inventory | Complete module and macro expansion templates; nested location/capture transfer; canonical JSON at the authoring boundary; every From method and field; use by kinds/verifier and local tests |
| `authoring/mod.rs`: Card/CardDefinition, Graph, Declaration/LocalArtifact, CardRef/selection, CodeSource/ArtifactFile and common Card macros | Same contributor authorities; design per-kind Artifact, Registry lifecycle/Heavy artifacts/Idempotency, Spec-file authoring/reference rules; errors guide Validation failures | Complete definitions, decoding, graph collection, equality/conflict and snapshot methods; canonical loader entry points and existing publication consumers; every private/test-facing item, fallible method and tuple field |
| New `authoring/kinds.rs` | Root thin-client, owner, Card and rustdoc rules; all named per-kind design sections (Data through Operator); Rust and maintainer guides | All named-kind macro bodies and invocations, constructors, field/add declarations, conversions, Artifact descriptor/local conversion, Component and Binding; service/agent canonical consumers and SDK inventory |
| `authoring/verifier.rs` | Root Code; Rust/maintainer/errors guides; design Verifier, Agents/tools/workspaces, Drift checks, Trigger/Operator binding rules | Complete Verifier/plan, slots, Step, Requirement, Action/Check constructors and field/add macros; canonical `VerifierSpec` decode/plan path; all conversion/field docs and planner invariant/panic documentation |
| `cards/handle.rs`, `cards/mod.rs` | Root one-client/owner, async, errors, dependencies and rustdoc; Rust Structure/Imports/Async/Rustdoc; design Artifact, Client model, Registry lifecycle, In-memory authoring; operations Reliability and recovery: Dependency and regional failure | Complete changed declare/submit/upload call path, declare_from_path, Declarable variants and every From adapter, export; existing saga entry, upload preparation and source lifetime; native/path/authored branches remain on the same Cards engine |
| `state.rs` | Root owner/async/error/rustdoc rules; design Client model/In-memory authoring; security Permission check and Tenant and data isolation | DeclareOptions docs, declare/declare_with and complete pull_declared tail; same explicit/ambient client reaches Cards and hydration; no principal or tenant selector introduced |
| `wyrd-loader/src/lib.rs`, `parse.rs` | Root shared/spec/client boundaries; Rust pure-function, Errors and Rustdoc; errors guide; design Spec-file authoring and single reference-slot inventory | Both added parse entry points; complete existing parse_single_envelope and declaration_input_from_cards, plus load/build-declaration boundary; new input delegates to existing canonical decode/validation/order owners |
| Client `authoring/tests.rs` | Root Testing and rustdoc; Rust Tests/Rustdoc; TESTING.md tier placement, fixtures, exact selectors, stable error assertions | Complete six-test module and every helper; pure offline conversion/diagnostic checks, checked-in story parity, code-file timing, duplicate/reset/capture regressions; Errors/Panics sections inspected |
| Rust SDK `tests/integration/authoring.rs` | Root Testing/rustdoc and first-class SDK boundary; Rust Tests/Rustdoc; TESTING.md SDK tests, deployment fixtures, journey lanes | Complete module: Story and its fields/methods, all helpers and conversions, four real-server journeys plus public inventory, existing integration registration/support; current negative flows assert exact registry/permission/verification codes; ignored journey registration retained rather than introduced to bypass a failure |
| `examples/finance-release/finance_release.rs` | Root Code/Testing/rustdoc; Rust Async/Errors/Rustdoc; maintainer examples/readability; design Service/Agent/Verifier/Artifact/In-memory authoring | Entire binary: main, release_check, policy_judge, prompt, print_result; existing `examples/tests/finance_release.rs` real-binary reader journey and exact output expectations |
| `architecture/wyrd-design.md` In-memory authoring | `architecture/README.md` conflict resolution and normative-writing rules; approved revision 4; adjoining Client model, Kind catalog, Registry lifecycle and Spec-file authoring | Complete replaced section and adjacent reference/predeclaration doctrine; checked against deferred fields, loader, local Artifact publication and unified declaration source |

Security authority was consulted for the actual unchanged authenticated boundary: `wyrd-security-posture.md` Authorization and policy, Permission check/Auth crate boundaries, and Tenant and data isolation. New declaration dispatch retains the owning engine/client and existing server permission/tenant owners; it adds no permission decision, credential, route or SQL capability. No `PgPool`, `OperatorPool`, `TenantConn`, migration or server SQL signature/field is introduced. Bifrost internals, deployment configuration and generated contracts are unchanged, so their implementation authorities have no new write surface here; this does not substitute for the independently assigned identity/publication and resilience reviews.

## Rule results

| Applicable rule | Result and evidence |
|---|---|
| Durable behavior stays server-owned, common semantics in the one client, thin Rust SDK export | PASS for repository boundary. SDK `src/lib.rs:128–132` still reexports client authoring. Changed definitions produce canonical loader Cards/submissions and use existing saga and pull owners. No server workflow, endpoint, wire schema or client scheduler is added. |
| Canonical Card vocabulary, reference forms, local-only source conveniences and server-derived envelope data | PASS for shape. Card.envelope builds kind/metadata/spec; reference conversion distinguishes sibling definitions and external exact refs; local Artifact/code-file descriptors stay in captures/declaration preparation. No authored relationships/status/UID setter or compatibility authoring alias is added. |
| Struct-centered retained state; synchronous local transformations; async only for IO composition | PASS. Body/Card/Graph own their transformations; Cards owns declaration/upload, WyrdState declaration/hydration. Loader parsing stays synchronous and all new async production paths await existing IO. No new utility struct, single-implementation trait or ad hoc runtime. |
| Existing parsing, reference inventory, validation, publication and hydration owners remain authoritative | PASS for ownership. parse_card_value → parse_value → parse_single_envelope → Spec::from_kind_and_value; Graph → declaration_input_from_cards; Cards → upload_artifact/submit → existing saga; WyrdState → existing pull_declared. |
| Spec/client dependency boundaries, feature discipline, cross-platform behavior, generated-artifact discipline | PASS by diff. No manifest/feature, spec implementation, PyO3/N-API, cloud/SQL/OLAP dependency, generated artifact, forbidden OS mechanism or platform gate changes. Codegen outputs have no changed source contract requiring regeneration. |
| Typed public error owners and validation diagnostics | PASS for production rule coverage. Existing WyrdError variants and LoadError/Diagnostic conversions are reused; no manual error catalog or problem mapper. Snapshot refusal retains YAML paths/source labels. Negative test code assertion has the distinct failure below. |
| Safe external-input/error handling and synthetic fixtures | PASS for changed production source. No new IO unwrap, exposed secret, credential owner or live provider test. Source reads return diagnostics; the planner unreachable branch names its fixed-kind invariant and is documented. |
| Every changed item has meaningful rustdoc, appropriate Errors/Panics and durable cancellation/retry semantics | **FAIL, REPO-R3-001.** Most new/private/test/macro items are documented correctly; the finite remaining locations below fail the explicit gate. |
| Tests use appropriate tiers, deployment-shaped journeys, native lifetime ownership and scoped proof | PASS for structure. Offline tests stay in the client module; existing SDK/example binaries drive actual WyrdTestServer/Postgres and reader behavior. No replacement binary, Python/Node lifetime emulation, new sleep/polling or weakened existing ignore/gate is introduced. New negative proof must use the exact error catalog code: **FAIL, REPO-R3-002**. |
| Architecture owns current normative behavior without plans/status/history/secrets | PASS. The changed section states the approved current grammar, source/validation/declaration contracts and retained publication behavior; no delivery record, unsupported performance claim or parallel authority is added. |
| Import spelling guideline | NON-BLOCKING deviation. Rust Imports asks for imported bare names in signatures; some new signatures retain qualified forms, e.g. Declarable::Path/From PathBuf and loader serde_json::Value. No material caller or maintenance failure is established, and this is not an explicit mandatory gate. No remediation finding is proposed for spelling alone. |

## Prior FIND-TASK-001-3 closure

The five previously retained SDK wrapper panic omissions have been resolved by current source, not merely by adding headings:

- `release_access` and `summarized_access` (`authoring.rs:284,290`) now call infallible deferred construction. `access_verifier` and `fact_step` record CodeSource descriptors; they do not parse versions or read files. Their prior inherited panic is gone.
- `baseline` (`:389`) now builds the deferred graph without reading fixture source. Its former inherited source-read panic is gone.
- `outcome` (`:485`) documents the existing requirement lookup's missing-ID panic.
- `policy_published` (`:753`) documents the existing card_ref parser's invalid-version panic before IO.

The rest of the item pass covers fields, From methods, macro-generated setters/constructors, private helpers, tests and fallible sections; no additional inherited-construction panic omission was found. The prior source locations themselves are closed. The mandatory documentation invariant is still open at the newly changed locations below, so retain FIND-TASK-001-3 rather than inventing a new documentation finding ID.

## Proposed findings

### REPO-R3-001 — Finish the finite mandatory documentation remainder

Classification: **VIOLATION**. Stable context: **FIND-TASK-001-3**.

Rule: root AGENTS Code requires rustdoc for every new/changed Rust item and says “Missing rustdoc blocks merge.” `agent_docs/rust.md` Rustdoc explicitly includes all visibility, fields, private/test items, fallible Errors, applicable Panics, cancellation/partial-progress/idempotency/retry for async or durable operations, and says “Placeholder docs that restate the name are a blocker.”

Exact changed locations and evidence:

1. `examples/finance-release/finance_release.rs:23–24`: materially rewritten `main` has no item rustdoc at all. Module `//!` prose documents the binary module, not this fallible async item. Its body performs WyrdState declaration, upload/hydration and two verifications through `?`. It needs a short workflow doc and Errors contract; its durable/cancellation behavior can refer to the existing WyrdState owner rather than repeat it.
2. `crates/shared/wyrd-client/src/authoring/kinds.rs:126,139,154`: public macro-generated Model, Data and Experiment type docs are respectively only “A Model Card.”, “A Data Card.” and “An Experiment Card.” Those are the exact name-only placeholders prohibited by the guide. Their roles are already specified in the owning design sections (model interface/signature, dataset interface/schema, experiment run grouping), so no design or new API explanation is needed.
3. `crates/shared/wyrd-client/src/cards/handle.rs:315–320`: newly introduced private `submit` awaits the durable declaration/upload/completion saga. Its docs name the operation and Errors but omit applicable cancellation, partial progress and fresh-call retry semantics. Public declare/upload docs immediately above/below already describe the underlying owner semantics; a short accurate reference suffices. The all-visibility gate applies to the extracted helper too.

Consequence/materiality: the explicitly mandatory documentation merge gate remains unsatisfied. The public kind docs give a reader no domain role, and the private durable helper does not document how interrupted work persists. No runtime defect is alleged.

Smallest testable correction: add only these short item/type/section docs, reusing existing design and declaration-owner contracts. Preserve every body/signature/assertion. This is a bounded prose correction, not a request for new checks, helpers, tests, wrappers or API changes. Independent source inspection of these exact locations plus applicable formatting/docs checks closes it; do not rerun journeys solely for prose. Batch with other validated remediation and do not create a separate style-only round.

### REPO-R3-002 — Pin the unreadable code source's actual catalog code

Classification: **VIOLATION** of required negative-proof standards.

Rule: `agent_docs/rust.md` Tests requires “Assert Wyrd error codes, not messages”; the approved validation contract requires existing stable native error families. `TESTING.md` likewise requires one exact catalog code for SDK-facing error assertions rather than any-of-code matching.

Location: `crates/shared/wyrd-client/src/authoring/tests.rs:573–578`, in the newly added field-edit/source-read test.

Producer-to-consumer evidence: missing CodeSource file → Card::envelope's read_to_string error → Diagnostic::io (`wyrd-loader/src/error.rs:132–140`) → existing WyrdError::LoaderIo, catalog code **WYRD_LOADER_400_IO** (`wyrd-spec/src/error.rs:320–323`) → LoaderInvalidEnvelope diagnostic array → the test's diagnostics helper. The test instead accepts `code == "WYRD_LOADER_500_IO" || code.contains("IO")`. The exact alternative names the wrong status, and the fallback also matches unrelated codes containing `IO`, including `VERIFICATION` and `PERMISSION` families. A wrong-family diagnostic can therefore satisfy the test without proving this source-read boundary's required refusal.

Consequence/materiality: this is the task's focused negative proof for deferred source IO; its assertion cannot detect an error-family regression even though catalog stability is an explicit acceptance boundary. This is a proof defect, not evidence that the current production IO mapping is wrong. The production owner already returns the correct code.

Smallest testable correction: replace the permissive alternatives with equality to the existing **WYRD_LOADER_400_IO** diagnostic code. Retain the surrounding source-read timing scenario and assertions. No production change, new harness or extra test is needed. Rerun only the existing exact `authoring::tests::field_edits_defer_validation_and_preserve_snapshots` test through the approved mise/nextest command, and inspect that the assertion pins the canonical code.

## Verification limits

Static standards review only. No Cargo, codegen or runtime command was run; the orchestrator owns sequential verification and should attach its independent command results. Historical task evidence was available as claims, not assumed execution. `git diff --check a387f2a47a145a867c8a4948966fcbd136af9961 37b3c2f96b1bd425bf1f0923a1950d0363c40907` passed. HEAD stayed at the immutable candidate; only this report was written.

Runtime acceptance, sensitive identity/publication behavior, resilience and abstraction economics remain with their separately selected reviewers. No broad gate or unrelated server/domain rediscovery was performed. The remaining risk in this report is the two explicit standards/proof gaps above; current source does not establish a production IO error-family defect.
