---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verifier-artifact-workspaces
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, INV-001, INV-002, INV-003, INV-004, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
depends_on: []
parent_task:
remediates: []
---

# Publish reusable Artifacts with explicit extraction and selected workspaces

## Outcome and Value

A user publishes arbitrary files independently, optionally prepares extracted
JSON before upload, discovers the resulting paths, and chooses which files or
directories to attach to an Agent call. Multiple Verifiers reuse the same exact
Artifact version. Agents use ordinary read/search/list tools or explicitly ask
for extraction. Mounting and ordinary reads do not silently parse documents.

The required Python journey uploads an Artifact, creates a Service with a bound
Verifier, and proves its Agent judge reads selected stored contents through a
real tool call. A content-sensitive mocked model is the only fake in that path.

Spec revision 3 is approved, including the search contract. This task remains
`proposed`: spec approval is not execution authorization or an independent
readiness verdict. Implementation has not started.

## What Changes and What Must Stay True

Extend shared Cards publication/inventory, typed Artifact validation, existing
reference traversal and selector forms, hosted workspace staging, the built-in
tool catalog and original-evidence citation validation. Share the existing Rust
extractor at a client-compatible owner, rather than importing the server into a
client or adding language-specific parsers. Upload preparation is authoring;
server storage and registry behavior remain authoritative.

Originals and generated outputs enter one initial manifest. Existing version,
activation, pending upload, integrity, download and deletion rules cover both.
The `extracted/` layout and typed metadata report are discoverable authoring
outputs, not runtime rules: users choose source paths and mount destinations.
Reject conflicting destination paths and caller replacement of fixed attachments.
The same source may be attached at distinct nonconflicting destinations.
Stage only selected files, without mount-time document parsing or preloaded
extracted text. Do not infer archive expansion from fixed Artifact filenames.

`fs.read` returns actual text bytes with line locations; `fs.search` searches
visible text without parsing binaries; `fs.list` lists paths. New `fs.extract`
uses the existing parser/output and writes explicitly under bounded `/work`.
Both steps and checks use available, declared and principal-authorized tools,
without a check-specific tool allowlist. Reuse the existing bounded repeated
model/tool loop. Search gains regex and path/glob scope while preserving plain
query defaults. Scratch stays uncitable;
prepared JSON is evidence in its own right. Preserve existing explicit extract
actions and original locator validation while correcting ordinary text locators.

The task includes necessary caller migration: fixtures/prompts that currently
ask fs.read to extract a document must explicitly extract it or attach prepared
text. Do not weaken old journeys or preserve implicit parsing as a compatibility
alias. The parser relocation may move existing dependencies to the shared owner;
no new parser stack, endpoint, cache, storage lifecycle, result schema,
standalone Agent hosting, shell runtime or SDK size optimization is included.
Prefer existing dependencies/owners; any added dependency, feature or migration
needs justification, and semantic/security changes still require spec review.
Existing MCP discovery remains read-only.

## Approach

1. Update owning authority/contracts for upload preparation, report metadata,
   existing selector directory semantics, ordinary file tools and fs.extract.
2. Decouple/reuse the existing extraction implementation at a shared owner and
   extend Cards authoring to prepare originals/outputs before declaration.
3. Expose upload and existing inventory through thin CLI/SDK helpers, retaining
   current Card versioning and lifecycle orchestration.
4. Resolve selected fixed Artifact files through existing caller-authorized
   inventory and stage them beside dynamic workspace bindings without parsing.
5. Correct filesystem tools, add explicit fs.extract to the current catalog,
   enable judge workspaces/tools under existing principal authorization, expand
   grep-style search and align citation validation with each representation.
6. Migrate affected callers/examples, close cross-SDK Service journeys and
   negative flows, regenerate contracts and run focused checks.

## Ordered Implementation Scenarios

### Scenario 1 — Independent byte publication

**Behavior.** Publish one file and a nested directory before creating any
consumer. Only supplied files enter the collection; names and bytes survive
inventory and pull. Success returns an Active exact CardRef. Repeated unchanged,
changed and conflicting pinned versions keep existing semantics. Empty kinds,
invalid paths, symlinks and nonregular sources fail. Default upload never parses.

**RED.** Add the [planned plain-upload test](#plain-upload). Confirm the new
helper/command is absent or cannot supply the required workflow, rather than a
setup or collection failure. Add focused shared-owner validation assertions
where needed and record exact collected selectors before RED.

**GREEN.** Build existing manifests/local-source maps and delegate to shared
Cards declaration. Expose existing inventory without a new endpoint or receipt.
Keep server activation, transfer checks and version decisions in their owners.

**REFACTOR.** Remove duplicate version/upload logic. Reuse existing enumeration
without authored-envelope exclusion accidentally dropping an ordinary file.
Rerun publication and existing declaration regressions before proceeding.

### Scenario 2 — Optional extraction before registration

**Behavior.** With extraction enabled, prepare existing ExtractOutput JSON at
`extracted/P.json`, leaving originals/source directories unchanged. Publish the
complete original/generated manifest once. Typed `metadata.extraction` exposes
version, generated source/output pairs and skipped unsupported paths. Human CLI
output reports these paths; JSON/SDK success remains the exact CardRef. Reject
caller report conflicts, generated path collisions, malformed supported inputs,
limits and source mutation without a misleading successful publication.

**RED.** Add the [planned upload-extraction test](#upload-extraction), asserting
that preparation failures leave no new registered Card for the attempted fresh
identity. Prove raw unsupported bytes still publish/report correctly. Preserve
existing format-specific parser tests when moving their owner; record focused
commands for moved/new tests before RED.

**GREEN.** Share the existing parser and output contract with client authoring,
using existing bounded defaults and typed NonSecretValue metadata. Retain the
source/derived byte association through hash/transfer verification. Feed both
sets of files to the ordinary declaration lifecycle; do not append server files
post-upload or add a cache/provenance store.

**REFACTOR.** Remove server-specific error/runtime coupling from the shared
parser boundary, adapting errors back at existing consumers. Keep one parser
implementation and version; rerun all earlier scenarios and parser regressions.

### Scenario 3 — User-selected file and directory attachments

**Behavior.** Existing Artifact selector payloads select an exact file,
path-boundary directory subtree, or whole collection when workspace path is
omitted. Stage only the selection at the user's destination. Registered and
inline Agents and existing scope-reference bindings work. Two Verifiers share
one pinned Artifact; publishing a new version does not retarget them. Existing
non-workspace singleton omission semantics remain unchanged. Collisions,
missing prefixes, wrong kinds and replacement attempts fail.

**RED.** Add the [planned selection test](#workspace-selection). Include a
supported binary that would fail extraction if parsed, an opaque file, a sibling
prefix that must stay unselected, and an unselected file that would exceed a
lowered test limit. Selection must neither parse nor stage those siblings.
Add focused serde/reference/path tests and record their exact commands before RED.

**GREEN.** Extend current reference visitation and authorized inventory selection;
reuse CardRef/sibling/authored-Card handling inside the existing selector. Extend
workspace staging and collision/accounting checks instead of adding a resolver,
resource scope or materialization service.

**REFACTOR.** Consolidate selection/path checks at existing owners. Keep source
identities and selected-relative paths explicit; rerun publication/extraction
proof and existing dynamic workspace cases.

### Scenario 4 — Ordinary read, search and list

**Behavior.** Listing and mounting do not parse contents. Reading UTF-8 files
returns their actual source lines, including raw JSON text, with resumable bounded
pagination. Directory and invalid/binary text reads fail clearly. Search retains
bounded case-insensitive literal matching over visible text and does not extract
binary contents. Opaque mounted bytes remain usable by existing code.run.

**RED.** Add the [planned ordinary-filesystem test](#ordinary-filesystem). Current
JSON-pointer/extracted-unit behavior must fail actual-line assertions. Add a
focused owner proof that ordinary mount/list/read/search does not call extraction;
record its actual selector and command before RED. Preserve bounds/security
assertions from existing workspace tests as their expected representations change.

**GREEN.** Separate raw file access from optional interpretation within existing
workspace/evidence owners. Use actual text locators for read/search and verify
those against original bytes. Keep original extraction locator paths for explicit
extract actions/citations. Do not keep eager extracted units as a hidden fallback.

**REFACTOR.** Share bounded raw access and text pagination/search code across
inputs and scratch where appropriate. Keep scratch source sets empty and rerun
earlier scenarios plus the migrated file-tool regressions.

### Scenario 5 — Grep-style search across selected files

**Behavior.** fs.search spans selected input files and scratch text, preserving
literal case-insensitive `{query}` calls and adding explicit regex, case,
file/directory scope and filename glob filters. It returns one located hit per
matching text line in deterministic order, with bounded snippets and truncation.
Selected hidden files are searchable; binary files are skipped without parsing.
Unselected files stay invisible. Invalid regex/glob/scope fails explicitly;
no matches succeeds. Regex evaluation does not invoke a shell or native grep.

**RED.** Add the [planned search journey](#grep-style-search). Exercise nested
files, scratch, repeated matches on a line, both case modes, literal regex
metacharacters, file/subtree scope, basename and recursive globs, invalid patterns,
selected hidden files, binary content, no-match and truncated results. Use a
content-sensitive gateway sequence of search → read → query → final to prove the
existing loop can select different declared/authorized tools over multiple turns.
Record focused matcher/schema selectors before RED; missing options/dispatch
must cause failure rather than a setup failure.

**GREEN.** Extend existing search arguments, descriptors and bounded raw-text
search; use the installed Rust regex capability and ordinary glob semantics.
Apply filters only to visible workspace paths. Preserve the response envelope,
registered line provenance, scratch source isolation and existing execution
bounds. Remove the check-specific tool gate, retaining ordinary tool/resource
permissions and Agent declarations; do not introduce a second harness/runner.

**REFACTOR.** Keep one search owner and share raw text/locator handling with read.
Rerun ordinary filesystem, selected-mount and earlier publication/extraction
proof. Update examples/descriptors to distinguish regex from literal mode and
content patterns from filename globs; obtain additional context via fs.read.

### Scenario 6 — Explicit Agent extraction

**Behavior.** An Agent declares fs.extract and passes one input path plus an
explicit `/work` output path. The host writes the existing JSON extraction shape
and returns output path, original source IDs and extractor version. Subsequent
fs.read/search operates on that text. Directories, unsupported/malformed input,
existing/invalid destinations and output limits fail without a usable partial
file. Extraction permission is checked normally. Scratch output is not a new
citation source and cannot supply original-file line/pointer identities.

**RED.** Add the [planned explicit-tool test](#explicit-extraction-tool). The
missing catalog entry/dispatch supplies the initial failure. Assert the real
output, no-overwrite behavior, bounded failures and scratch citation rejection;
record focused host/tool tests before RED rather than mock the reader/parser.

**GREEN.** Add fs.extract to the existing BuiltinTool catalog, schemas, permissions
and generic dotted-name provider projection. Use the shared parser, existing
workspace confinement/write budgets and recorded-tool serializer. Retain source
information and original locator rules without adding verification-result fields
or automatic cross-file citation translation.

**REFACTOR.** Reuse current tool registration/session/recording and extractor
adapters. Avoid another output schema for full extracted contents: it remains
ExtractOutput JSON on disk. Rerun earlier scenarios and explicit-extract actions.

### Scenario 7 — Service-bound judge reads uploaded prepared content

**Behavior.** Python uploads a collection with extraction, inspects generated
paths and creates a Service with a bound Verifier. The Agent check mounts only
selected prepared files at user-chosen paths and declares fs.read. Through that
Service binding, the mock gateway requests the mounted path. The real tool
response contains a fresh source marker and valid locations; only then may the
mock return a structured judgment citing the registered prepared file. A second
Verifier can reuse the same version without runtime re-extraction.

**RED.** Add the [planned Python Service journey](#python-service-journey). Initial
judge restrictions/selection gaps must prevent the flow. Add citation and
no-re-extraction observations as the flow progresses. The responder must fail
on absent marker, wrong call ID/path, tool error or fabricated citation.

**GREEN.** Enable existing hosted Agent/tool sessions for checks, extend all
fixed-source reachability seams and use the existing Service binding entry point.
Keep the marker out of initial prompts/execution input and fixed mock responses.
Registration, storage, workspace, reading and validation remain real.

**REFACTOR.** Reuse scripted upstream/capture fixtures. Keep the Agent prompt
about chosen mount paths, with no hidden extracted-directory convention. Do not
add a fake AgentExecutor, fake extraction/storage, gateway framework or judge
result tool-call fields. Rerun earlier scenarios.

### Scenario 8 — Authority, integrity and failure closure

**Behavior.** Wrong tenant/grants, denied tools, wrong kind/UID, inactive or missing
resources, tampering, bounds, collisions and fixed-mount replacement fail without
protected contents reaching model/tool. Caller authority is never borrowed from
the publisher. Audit and cancellation cleanup remain. Prepared JSON is its own
evidence; scratch and metadata derivation claims cannot impersonate an original.

**RED.** Add the [planned refusal test](#refusals). Reuse already-correct server
negative paths with verification-only evidence rather than manufacture RED.
For missing new-path checks, assert the precise refusal and absence of protected
bytes in captured upstream turns; record focused owner selectors before RED.

**GREEN.** Apply existing admission, inventory, tool grants, digest, source and
cleanup checks to new paths. Count selected fixed inputs with dynamic inputs;
apply extraction/write limits only when explicitly preparing or extracting.

**REFACTOR.** Remove parallel trust paths and keep permission audits at existing
boundaries. Rerun all prior scenarios, including raw-byte and scratch cases.

### Scenario 9 — CLI and SDK consumer closure

**Behavior.** CLI and Rust/TypeScript SDKs deliver the same independent upload,
optional preparation, inventory selection and Service-bound judge flow. All
public/native exports, Python stubs and generated TS types agree. Old document
reader fixtures/prompts explicitly extract or select prepared text; existing
building-block and explicit-extract journeys retain their required behavior.

**RED.** Add the [planned Rust](#rust-service-journey) and
[planned TypeScript](#typescript-service-journey) journeys. Include real CLI
upload/inventory calls in Python proof. Confirm new exports and corrected
filesystem expectations fail for their intended missing behavior, not setup.

**GREEN.** Add thin projections over shared authoring/runtime owners and
regenerate source-owned contracts. Update all affected fixtures, mock tool
sequences, prompts, docs and existing assertions without dropping coverage.

**REFACTOR.** Remove language-specific lifecycle/parser duplication. Keep
content-sensitive mocks and user-chosen paths consistent; run focused journeys
and applicable static closure below.

## Acceptance Criteria

| Spec obligations | Concrete proof and expected observation |
|---|---|
| REQ-001–003; AC-001 | Plain publication and inventory/pull prove independent file/collection upload, activation, path/byte integrity, shared validation and existing version/conflict behavior. |
| REQ-009–010, REQ-012; AC-008 | Client preparation and parser regression proof show deterministic initial-manifest outputs/reporting, failure-before-registration, source consistency and storage reuse without an extraction cache. |
| REQ-004–005; INV-001; AC-002 | Selection/reference proof shows user-controlled file/subdirectory/collection mounts, graph dependencies, exact-version reuse and unchanged non-workspace omission rules. |
| REQ-007–008; AC-006, AC-010 | Raw filesystem and locator proof shows actual text/JSON lines, bounded pagination/search, no mount/read parsing, original locator validation and no scratch/source impersonation. |
| REQ-006, REQ-011; AC-009 | Explicit tool proof shows declared/authorized single-file extraction, real JSON output, origin/version, confinement, no overwrite/partial success and existing extract-action parity. |
| REQ-013; AC-011 | Multi-file search proof covers literal/regex/case, path/glob filters, actual lines, invalid/no-match/binary/hidden cases, truncation and a real repeated search/read/query sequence under existing authorization. |
| AC-003–004 | Service-bound Python transcript proves the mounted uploaded marker reaches the model via a real tool before a valid prepared-file citation. Reuse does not re-extract on the server. |
| INV-002–003; AC-005 | Security/integrity negatives show no unauthorized content, correct combined selected-byte/extraction/output limits, audits and cleanup. |
| INV-004; AC-007 | CLI/SDK journeys and generation/import/type/docs checks prove one lifecycle/parser, thin projections and complete caller migration without new result/transport/cache mechanisms. |

## Implementation Reference

These inspected owners explain reuse and gaps, not a private file/helper allowlist.

- **Upload:** `crates/shared/wyrd-client/src/cards/handle.rs::Cards::declare` and
  `cards/saga/mod.rs` own declaration, transfers and Active verification.
  `cards/saga/build_submission.rs` hashes complete original/generated sources
  before registration. Extend this workflow, not a second uploader. Existing
  directory enumeration in `wyrd-loader/src/manifest.rs::build_artifact_manifest`
  preserves paths/hashes but excludes an authored envelope name; raw uploads must
  not apply that exclusion. Local-source maps already allow prepared temp files.
- **Shared extraction:** `crates/wyrd/wyrd-server/src/verification/hosted/extract.rs::Extraction`
  already implements all current parsers and units/locator projection.
  ExtractOutput is already serializable in `wyrd-spec/src/card/verifier/extract.rs`.
  Current code depends on HostedLimits and Vala BlockError: decouple those boundary
  inputs/errors instead of importing server execution into clients. Existing
  `wyrd-loader` is the offline authoring owner consumed by both client and server;
  compare its whole workflow first when choosing the smallest shared placement.
  Private library organization is implementation-owned, but parser duplication
  or a new service is not justified. Move current parser tests with their owner.
- **Artifact metadata/inventory:** `wyrd-spec/src/card/artifact.rs::ArtifactSpec`
  lacks nonempty-kind validation. Its metadata uses typed NonSecretValue, not
  plain JSON; use the existing object/list/string forms for the report.
  `registry/submission.rs::ArtifactManifestEntry` remains integrity authority.
  The declaration receipt has no inventory: expose the existing server inventory
  read (currently private in `wyrd-client/src/cards/reads/list_artifacts.rs`),
  plus normal Card metadata, without another receipt/route/store.
- **Selectors/references:** `AgentCall.workspace` currently contains only scope
  Reference strings. `CodeSource::Artifact` supplies the existing selector shape;
  `reference.rs::Ref` supplies durable/sibling/authored references. Extend
  `refs/mod.rs::visit_agent_call` so fixed attachments are visited. The current
  evidence reader accepts only an exact file or omitted singleton, so add
  workspace-scoped subtree/collection selection without changing other consumers.
- **Raw bytes/workspaces:** `hosted/evidence.rs::TenantEvidence` verifies identity,
  tenant inventory, length and digest; its current Artifact interpretation path
  requires inventory MIME while manifest generation can omit it. Keep raw
  selection/materialization independent of MIME interpretation. Reuse existing
  format inference for explicit parser consumers and optional preparation.
  `hosted/workspace.rs::Workspace::place` currently eagerly parses. Remove that
  implicit filesystem path, retaining bounded bytes, source identities and
  cleanup. Existing read/search/list/output schemas and pagination remain owners.
- **Tool catalog:** `wyrd-spec/src/card/agent.rs::BuiltinTool` owns enum/ALL/names;
  permission scopes use that vocabulary in `wyrd-runtime/src/permission.rs` and
  `wyrd-spec/src/auth/permission_scope.rs`. `hosted/tools.rs::HostedTools` owns
  descriptors, closed arguments and dispatch. Existing RecordedTool bounds and
  records output; provider names derive generically (`fs.extract` → `fs_extract`).
  New tool-result origin/version fields do not require verification-result fields.
  Current fs.search accepts only query and searches pre-extracted inputs plus
  scratch lines: extend this same schema/dispatch/workspace owner for raw-text
  regex/path/glob search. Workspace regex is already declared and used by Vala;
  use it at the narrow search owner rather than adding a subprocess. A glob
  package in Cargo.lock is not proof of a direct owning-crate dependency.
  Existing search has matches/truncated but no continuation cursor; preserve
  explicit truncation and scope narrowing rather than inventing pagination.
- **Judges/citations:** `vala-verify/src/blocks/agent.rs` and verifier validation
  currently impose a check-specific tool allowlist and reject check workspaces;
  remove that role-specific gate, keeping executing-principal authorization.
  `blocks/engine.rs` passes no check
  workspace. TenantEvidence slots plus engine/Agent citation filters currently
  consider explicit inputs only. Include selected fixed evidence. Current
  `located`/`cited` use extracted units: preserve original extractor locations
  and add verification of ordinary text lines. `/work` deliberately has empty
  sources; preserve that behavior rather than register it as original evidence.
  `skald-agent/src/loop_runtime.rs::run_loop` already repeats model/tool turns
  under iteration/concurrency limits; no new reasoning harness is needed.
  Accepted caller authority flows through HostedBlocks into tools; selecting a
  Service Card does not replace the principal. Existing code.run is Python-only.
- **Affected callers:** workspace/tool owner tests, hosted Artifact integration,
  Vala Agent integration and permission-catalog tests depend on current semantics.
  `fixtures/cards/verifier_building_blocks/refund-reader.yaml`,
  `policy-review.yaml`, `examples/support-desk/service/policy-reader.yaml`, and
  the support-desk README currently assume fs.read extracts a document. Python
  conftest and Rust/TS building-block mocks script the same behavior; migrate
  them to explicit extraction or selected prepared text with actual-byte checks.
- **SDK/schema closure:** Python authored BuiltinTool stubs/public exports/native
  registration must include the new tool/helper shapes. TS Artifact typing is
  deferred by `scripts/gen_ts_types.py::SUPPORTED_KINDS`; fix the generator and
  regenerate card-types/native declarations. Never hand-edit generated artifacts.
- **Authority/docs:** update Wyrd design Artifact/reference/Verifier/Agent-tool
  sections, `.agents/skills/wyrd-advise/references/evaluation.md`, and public
  `docs/src/content/docs/reference/agent-tools-and-authorization.md` plus CLI/SDK
  guides and affected examples. Regenerate the docs machine-readable copy through
  its owner. Follow SDK/docs nested AGENTS; SQL rules apply only if SQL is touched.

## Verification and Evidence

The selectors below are planned in inspected existing integration owners. Confirm
actual collection after adding them and before RED. Record exact focused
commands here for additional named or moved owner tests before running them;
package-wide lanes are not substitutes. Fixture/helper organization remains
implementation-owned.

### Setup

Use `mise run build:server` and `mise run py:setup` before Python proof. For CLI
subprocesses, use the local-development journey build recipe:
`mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server --bins --tests`.
Use `mise run ts:build` and `mise run ts:build:testing` before TS proof. The
commands below retain the existing local Postgres/migration setup; do not replace
real storage, tools, extraction or Service dispatch with mocks.

### Plain upload

Planned Python test for publication/version/inventory/CLI defaults:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_upload_preserves_files_and_versions'
```

### Upload extraction

Planned Python test for optional preparation, report metadata and failure timing:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_upload_extracts_before_registration'
```

### Workspace selection

Planned Python test for file/subdirectory/collection selection and fixed identity:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_workspace_selects_files_and_directories'
```

### Ordinary filesystem

Planned Python test for non-parsing file tools, source lines, bounds and raw bytes:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_agent_file_tools_read_without_extraction'
```

### Grep-style search

Planned Python test for multi-file regex/grep semantics and repeated tool turns:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_agent_file_search_supports_regex_across_files'
```

### Explicit extraction tool

Planned Python test for the real host/parser/output, tool grant and scratch rules:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_agent_extract_tool_writes_explicit_output'
```

### Python Service journey

Planned user's required upload → inventory → Service binding → selected Agent
workspace → real file-tool response → cited judgment, with repeated reuse:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_uploaded_artifacts_are_reused_by_service_judges'
```

### Refusals

Planned Python negative journey for selected mounts/tool authority/integrity:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_workspace_refusals_preserve_authority'
```

### Rust Service journey

Planned matching journey in the inspected wyrd-ai integration target:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::uploaded_artifacts_are_reused_by_service_judges)'"
```

### TypeScript Service journey

Planned matching journey in the existing verifier-building-blocks owner:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^uploaded artifacts are reused by service judges$"'
```

### Static and owner closure

After each focused test and its relevant earlier scenarios, run applicable
focused shared-extractor/contract/workspace/tool/citation/permission regressions
and record their selectors. Preserve existing error/limit and format coverage
when callers/tests move; correct expected representations rather than drop cases.

- `mise run codegen:regen`, then `mise run codegen:check`: source-generated
  contracts/catalogs remain synchronized. No fabricated RED for generation.
- `mise run ts:napi:check`, `mise run ts:typecheck`, `mise run py:typecheck`:
  native/public APIs, tool unions and authored Python stubs agree.
- `mise run fmt:check`, `mise run lints`, `mise run py:format:check`,
  `mise run py:lints`, `mise run ts:format:check`, `mise run ts:lints`:
  changed-language format/lint closure.
- `mise run check:deps`: shared parsing adds no server-tier dependency to clients.
- `mise run docs:check` when docs-site content changes: journey-backed public
  tool/CLI/SDK guidance and machine-readable docs regenerate/build correctly.
- `mise exec -- git diff --check`: final whitespace/diff closure.

Authority/docs changes use static/regression proof. Do not run a full cross-SDK
journey aggregate or `gate` as a replacement for focused task proof; integrated
change review owns those aggregates. Record actual RED causes, GREEN results,
commands and final evidence here when implementation is authorized. Existing size
probes informed placement but are not a hard package-size acceptance threshold.

## Material Stop Conditions

Stop for spec revision if completion needs a new binding DSL/scope root,
verification-result field, parser implementation/format, server-side upload
transformation, cache/derived-storage lifecycle, provider file API, a new
permission model, standalone Agent hosting, shell runtime, extraction attestation,
or original-document citation translation for
uploaded prepared JSON. Reusing/relocating the approved existing parser stack and
adapting its private limits/errors is in scope. Additional dependencies, features
or migrations require justification, not an automatic product-approval stop;
changes to approved contracts, durable semantics or security remain a spec stop.
File/helper layout, focused test
seams, equivalent commands, cleanup repairs and caller migration are execution
choices under the approved outcomes.

Never weaken the fresh-marker tool-response proof, bypass the Service binding,
mock away storage/read/extraction, or restore implicit parsing to keep an old
fixture green. Missing repository-managed local services are setup work, not a
material blocker.

## Authority Links

- [Approved spec, revision 3](../spec.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Repository rules](../../../../AGENTS.md)
