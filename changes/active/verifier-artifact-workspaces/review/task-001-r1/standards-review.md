# Repository standards review

Result: **FAIL**. Source-local proposals: `RSTD-001` through `RSTD-004`.

Subject: `1c52b78bb...da7622b52994946e03b9ba241032b99d096063cf`, all 58 changed files, first review. Approved spec revision 3 and original TASK-001 were read. This report reviews repository standards; task-acceptance observations below are inputs for reconciliation, not an independent acceptance verdict.

The candidate commit remained fixed. An unrelated dirty task and later uncommitted Python journey edits are excluded. Committed Python lines were checked with `git show da7622b52:<path>`. No source was edited, no Cargo command was run, and no implementation completion message was used as proof. CodeGraph was consulted before textual code discovery. Its output served navigation only.

## Authority and surface coverage

| Changed surface | Applicable authorities read | Inspected source and result |
|---|---|---|
| All source, dependencies and review scope | Root `AGENTS.md`; `agent_docs/index.md`, `rust.md`, `errors.md`, `maintainer-style.md`, `agent-surfaces.md`; `changes/spec-driven-development.md`; `architecture/README.md`; approved spec/task | Complete cumulative diff, including rename-aware extractor diff, manifests, lockfile and task verification evidence. No SQL, UI, deployment or migration write set; their nested owners do not apply. |
| Contract types, validation and references | Root rules; Wyrd design Client model, Artifact, Verifier / Agents, tools and workspaces, Reference forms and Reference-slot inventory | `wyrd-spec` artifact validation, Agent tool enum/catalog/name conversion, WorkspaceSource/ArtifactSelection, workspace validation, reference visitation and re-exports. Pure contract tier retained; no IO/async/PyO3 additions. |
| Shared extraction, upload and public Rust client | Root rules; Rust/errors/style guides; Wyrd design Client model and Artifact; spec's one-parser boundary | `wyrd-loader` extractor and moved tests, manifest enumeration, upload preparation and tests, public exports; `wyrd-client` Cards methods/exports; Cargo manifests/lockfile. One relocated parser implementation, existing declaration saga and inventory reader reused. Errors and rustdoc exceptions below. |
| Vala execution and hosted server | Root rules; Rust/errors/style/agent-surfaces; Wyrd design Verifier/workspaces; security posture Authorization and policy / Request principal / Permission check and Tenant and data isolation | `blocks/agent.rs`, engine and engine tests, blocks_agent integration; server card validation, hosted evidence/mod/tools/workspace and workspace tests. No new SQL capability, route, transport, credential or tenant selector. Existing accepted Caller, inventory authorization, recorded tools, resource owners and stable boundary errors remain in use. Detailed security/lifecycle behavior belongs to selected specialists. |
| CLI | Root rules; Rust/errors/agent-surfaces; Wyrd design Artifact | `card.rs` ArtifactCommand/UploadArgs/FilesArgs/dispatch and cli routing. Existing connection and output types, manifest preparation, declaration and inventory owners reused. JSON returns existing CardRef/inventory shapes. No compatibility aliases or new error catalog. |
| Python native/public API and stubs | Python nested `AGENTS.md`, root rules, errors/style guides; Wyrd design Client model | `src/state/mod.rs` PyCards upload/inventory, Cards authored stub, verify BuiltinTool alias; existing Cards registration/export path, `WyrdPyError` conversion and `parse_name` inspected. Methods attach to existing registered class, so no new class registration/export is needed. FsExtract projects the string tool catalog, not a new pyclass. RSTD-001 applies. |
| TypeScript native/public/generated API | TS nested `AGENTS.md`, root rules, agent-surfaces | Native Cards methods, generated `index.d.ts` and `.d.cts`, public wrapper and exports, generator SUPPORTED_KINDS/WIRE_ROOTS and generated card-types. Native/public methods forward to shared Rust; ArtifactSpec, inventory, WorkspaceSource and FsExtract appear in generated contracts. No duplicate durable runtime. |
| Tests and fixture migration | Root testing rules, both SDK nested AGENTS, `TESTING.md`, task proof requirements | Loader upload/extraction tests, engine/workspace tests, blocks_agent integration, Python conftest/journeys, Rust and TS Service journeys; new handbook YAML and migrated refund-reader/support-desk YAML. Real SDK/server journeys exist; a provider mock reads actual tool content to obtain the fresh marker. The existing block-reader mocks now call explicit extraction. RSTD-002/RSTD-003 apply to newly added Python proof. |
| Public docs, examples and advice | Docs nested `AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; Wyrd design owning sections; spec authority reconciliation | Three reference pages, generated llms-full diff, support-desk README/Agent and evaluation guidance. Removed check-only tool restrictions and updated raw line/extraction semantics. No invented performance/adoption claims. Public upload documentation observation below. |

Only nested AGENTS in docs, Python and TS apply: repository inventory contains no additional nested rule file in the touched Rust trees. The SQL nested file is outside the write set.

## Applicable rule results

| Rule | Result and source evidence |
|---|---|
| Domain ownership, struct-centered workflows, no client/server-tier inversion | PASS in inspected diff. ArtifactUpload owns prepared output lifetime, Extraction owns extraction accounting, Cards calls its declaration saga, Workspace owns file operations. Parser dependencies move to loader, not spec or language-specific stacks. No new Cargo feature or server/Vala dependency in loader. |
| Contract and SDK projection / generated ownership | PASS for source-to-projection wiring, subject to the Python input-shape acceptance observation. `gen_ts_types.py` adds Artifact and ArtifactInventoryResponse before generated changes; native methods explain corresponding generated declarations; Python stubs match new native signatures. Actual regeneration was not rerun in this static pass. |
| Public errors use catalog-backed boundary conversion and input failures remain repairable | **FAIL RSTD-001**. Python invalid upload names become Internal through implicit serde error conversion. Other newly inspected boundaries use existing WyrdError or BlockError-to-tool invocation projection; no handwritten public codes were added. |
| Rustdoc on every changed item, including fields/helpers and durable cancellation semantics | **FAIL RSTD-004**. Most added methods, fields, variants, tests and helpers have contextual docs and Errors/Panics sections; concrete omissions below remain. |
| No unsafe platform-specific production behavior, unwraps, ad hoc runtimes, lint suppression or gate bypass | PASS in production changes. No unsafe/cfg(target_os)/runtime construction/allow attribute added. The Rust SDK journey's ignore marker follows TESTING.md's explicit Postgres journey lane with `--run-ignored=all`; it is not a disabled failing proof. The unix symlink fixture runs on all three supported Unix platforms. |
| Tests at correct tiers; public journeys, fixture authoring and exact typed failures | Partial PASS, **FAIL RSTD-002/RSTD-003** for newly added Python proof. Unit/integration placement otherwise follows existing owners. Real storage, tools and Service dispatch are not mocked out. |
| Tenant/principal and audit authority, inventory integrity owner | PASS for structural standards: TenantEvidence retains Caller and calls existing authorized inventory/read owners; tool dispatch retains ordinary invoke authorization. No SQL/RLS or audit ownership changes. Behavioral correctness remains independently reviewed by the domain specialist. |
| Public CLI output/schema and documentation rules | PASS for shape and changed reference-page structure. CLI uses existing selector/connection/output conventions; docs machine copy tracks changed public references. Completeness observations below are for acceptance reconciliation. |
| Verification discipline | Limited. Task records broad command results and focused journey selectors, but acknowledges no genuine RED-first run. This report does not independently certify those commands or treat a claimed PASS as source proof. No Cargo-backed verification was attempted to avoid shared-target interference. |

## Material proposed findings

### RSTD-001 — Invalid Python upload names are projected as internal failures

Classification: INCORRECT / public error contract.

Authority: Python AGENTS Errors requires expected failures to retain structured Wyrd codes; `agent_docs/errors.md` requires caller-repairable validation failures to carry validation meaning. `wyrd-utils/src/py.rs:85` explicitly says caller-supplied JSON 4xx boundaries must map failures themselves.

Location: `sdks/wyrd-sdk-python/src/state/mod.rs:2025`.

Producer-to-consumer path: `cards.upload_artifact(source, name="")` or a name rejected by CardName validation builds JSON and deserializes Metadata. Metadata.name is CardName (`wyrd-spec/src/envelope.rs:163`); its Deserialize calls the validated constructor (`ids.rs` id_type macro). The bare `?` widens serde_json::Error through `WyrdPyError::from`, which invokes boundary_internal (`wyrd-utils/src/py.rs:85-109`), producing Internal, not Validation. This happens before source preparation or network calls. Rust callers provide validated Metadata; TS explicitly maps malformed metadata/spec to Validation in native cards.rs. Existing Python `parse_name` at state/mod.rs:3535 already preserves the field's validation meaning.

Consequence/materiality: a routine repairable authoring mistake appears as a 500 internal failure to Python users/agents, unlike the other first-class clients. This is a supported public input, not malformed server state.

Smallest correction: validate the name using the existing Python boundary validator or explicitly map metadata input conversion to its existing validation error. Keep the shared Internal conversion unchanged: it correctly handles unrelated internal serialization failures. Add a focused public Python assertion for the exact validation code and field, before any registration. No new exception/catalog/helper layer is needed.

### RSTD-002 — New negative journeys accept the wrong failure contract

Classification: VIOLATION / mandatory testing rule.

Authority: Python AGENTS Tests says assert typed Wyrd errors and codes; TESTING.md Client-facing tests explicitly requires one exact catalog code, and prohibits message matching or accepting any of several codes.

Locations in committed `test_verifier_building_blocks.py`: line 900 checks only `"409" in conflict.value.code`; lines 916, 956/962 and 1441/1443 only assert WyrdError; line 1431 accepts either entry of REFUSED for every distinct refusal; line 1451 checks message text for missing selection. Similar new negative branches should be closed in the same bounded correction.

Consequence/materiality: these tests can remain green when an expected input-validation/authorization refusal regresses into an unrelated Wyrd failure. RSTD-001 demonstrates the same class of wrongly classified public failure. This is an explicit repository test gate, not a demand for additional speculative cases.

Smallest correction: bind each existing negative case to its one expected stable code (and field/rule when that distinction matters); keep the real public journey and its existing assertions. For tool results, use their existing structured error field rather than prose where available. No new harness or broad aggregate is necessary; rerun the existing exact affected tests.

### RSTD-003 — New SDK journeys construct their Card fixtures in test code

Classification: VIOLATION / mandatory client-facing test ownership rule.

Authority: TESTING.md Client-facing tests requires checked-in YAML under fixtures/cards, forbids building/editing Card YAML/JSON in test code, requires fixed meaningful Card names, and requires setup fixtures returning domain objects. These are explicit rules rather than preferences about helper size.

Locations: committed Python `test_verifier_building_blocks.py:1026-1086` (`write_probe`) builds Agent/Verifier/Service documents as nested Python dictionaries and serializes them into .yaml files; `fresh` at 855-857 creates random identity suffixes used by that builder and ordinary publication cases. Probe at 1090 then registers those generated documents. Positive workspace/read/search/extract journeys and refusal cases consume the builder.

Consequence/materiality: the newly shipped client examples bypass the repository's authored-Card fixture contract, so readers cannot inspect/reuse the actual Service/Verifier artifacts and proof of public authoring remains inside an imperative generator. This violates a required test boundary even though it exercises real runtime behavior.

Smallest correction: keep the content-sensitive mock and real tool calls; move positive Card definitions and specifically malformed refusal documents into the existing fixture tree, using fixed story identities and ordinary SDK upload/pull setup. Return the resulting domain objects from fixture setup. Do not replace real registration/storage/tools with mocks or add a second fixture framework. The uploaded document bytes and fresh marker remain generated data; the prohibition concerns Card construction and random Card identities, not the required marker.

### RSTD-004 — Changed-item rustdoc gate is incomplete

Classification: VIOLATION / mandatory rustdoc gate.

Authority: root AGENTS says missing changed-item rustdoc blocks merge; `agent_docs/rust.md` includes fields and test helpers and requires async/durable operations to document cancellation, partial progress, idempotency and retry. The review skill expressly preserves this mandatory gate.

Locations/evidence:

- `wyrd-loader/src/extract.rs:68`: the newly public ExtractError tuple's public String field has no field rustdoc; only the outer type is described.
- `wyrd-loader/src/upload/tests.rs:29-35`: new `paths` indexes public mutable `upload.input.submissions[0]`, but its docs omit the invariant / possible panic. The helper's actual caller invariant is one prepared Artifact submission.
- `wyrd-client/src/cards/handle.rs:278-310`: new durable async upload documents happy-path lifecycle delegation but no cancellation/partial-progress/retry contract. Cancellation can occur while blocking preparation continues or after declaration transfer has begun; the approved REQ-002 explicitly preserves pending/reconciliation rather than atomic rollback. Readers should not have to infer this from an undocumented dependency chain.

Smallest correction: document the field, test-helper invariant/panic, and existing upload interruption/retry semantics at these owners. Do not introduce a new cleanup/rollback mechanism or change existing lifecycle behavior. Review the added docs against the existing saga; no runtime test is needed solely for prose. Batch these corrections with functional remediation, not a separate style-only round.

## Acceptance observations for orchestration

- Approved REQ-001 says helpers accept existing Metadata and ArtifactSpec authoring values. Rust accepts both; TS accepts both in options. Python at state/mod.rs:2015-2036 accepts only name/version/version_bump/artifact_kind/extract and constructs a default ArtifactSpec. A Python caller cannot supply spec.metadata (including ordinary non-extraction metadata), schema_ref, framework_adapter or other existing fields through this helper. Reconcile against the approved authoring promise; mapping it to a new Python convenience signature is a product-contract question for acceptance, not a standards preference.
- Public docs edits cover tools/extraction/verifier references, and the new upload CLI appears in architecture. No public upload/files CLI or SDK workflow documentation is added in the reviewed public content diff. The approved spec's final authority paragraph and task Implementation Reference require affected public CLI/SDK guidance. Reconcile that completeness separately; stubs and architecture alone do not establish a public workflow guide.
- The evidence table explicitly acknowledges no code.run opaque-byte proof, no real search → read → query → final sequence, and no unselected oversized-file admission proof. These limitations must remain visible in acceptance reconciliation; this report does not turn them into a new test scope.

## Verification limits

Static source review only. Claimed format/lint/codegen/typecheck/journey results were read in task evidence, not rerun here. A clean generated diff/source relationship is not independent regeneration proof. No deployment, benchmark, broad gate or runtime error reproduction was run. Review artifacts only were written.
