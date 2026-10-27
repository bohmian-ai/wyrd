# Maintainer review

Result: **FAIL** — one incomplete Python projection. No duplicate lifecycle,
parser implementation, or workspace owner was established in the reviewed scope.

## Subject and limits

- Base: `1c52b78bb`.
- Candidate: `da7622b52994946e03b9ba241032b99d096063cf`; HEAD was checked against this identity.
- First review of the complete base-to-candidate range for the assigned ownership
  and abstraction surfaces; no carried-forward evidence.
- Uncommitted Python journey edits and the unrelated service-identity task edit
  are excluded. Committed Python upload implementation and publication proof
  were rechecked with `git show da7622b52:<path>` after the dirty-tree notice.
- Authority: approved specification revision 3, original TASK-001, root and SDK
  AGENTS rules, `changes/spec-driven-development.md`,
  `agent_docs/{index,rust,maintainer-style}.md`, architecture authority map, and
  the Artifact and hosted Agent/workspace sections of `wyrd-design.md`.
- CodeGraph was consulted before textual code discovery. This is a static
  source/diff review. No Cargo processes or tests were run, and reported green
  verification was not used as a substitute for source inspection.

## Changed-surface coverage and existing-owner comparison

| Surface | Source and callers inspected | Existing alternative, missing behavior, and assessment |
|---|---|---|
| Manifest and upload preparation | `wyrd-loader/src/{manifest,upload,lib}.rs`, `upload/tests.rs`; client `Cards::upload_artifact`; CLI upload; Python and N-API projections | Existing `build_artifact_manifest` recursively enumerates files but excludes the authored envelope. New `manifest_of` factors its hashing/path construction, and `collect_files` is reused. Preparing a supplied single file without scanning its parent, and preserving envelope-like ordinary filenames, requires the new input selection. `ArtifactUpload` retains its temporary output lifetime and produces existing `DeclarationInput`; it does not declare, version, transfer, activate, or retain durable files. |
| Declaration/transfer lifecycle | `wyrd-client/src/cards/{handle,engine}.rs`, `saga/hash_artifacts.rs`, existing storage upload dispatcher | `Cards::upload_artifact` calls existing `declare`; pre-submission source rehashing and manifest stamping remain in the saga. No competing activation/version/hash protocol was added. CLI uses the same prepared input plus `declare` because human output needs preparation's generated/skipped lists; this does not reproduce the declaration saga. |
| Shared extraction | Rename-aware diff of server `hosted/extract.rs` into loader `extract.rs`; loader exports/dependencies; server `HostedLimits::extract`; `TenantEvidence::{located,execute}`; `Workspace::extract`; upload preparation/tests | The existing parser implementation is moved, with runtime `BlockError`/`HostedLimits` coupling replaced by client-compatible `ExtractError`/`ExtractLimits`. Server consumers adapt errors and configured limits at their own boundary. All three execution paths call `Extraction::run`; citations retain shared `units`/`locates`. New `lines` is the actual-text locator projection shared by read/search/citation verification, not another document parser. No second language parser, output format, cache, or extraction lifecycle was found. |
| Public handle and SDK/CLI projection | `Cards::{upload_artifact,list_artifacts}`; CLI `dispatch_artifact`; Python `PyCards::{upload_artifact,list_artifacts}` and authored stubs; N-API equivalents; TS public wrappers | Inventory reuses the existing get/UID and reads inventory route. TS and N-API forward full metadata/spec into the shared owner. Python forwards shared behavior but narrows authoring values to a fixed subset; see MNT-001. Neither binding owns storage/auth/versioning. |
| Fixed attachment orchestration | Engine `run`, `attach`, `fixed_selections`, `mount`, `prepare`, `check_bindings`, `Bindings::sources`; `EvidenceReader::select`; engine attachment test; durable-reference visitation diff | Existing engine still plans/drives the run. The added attachment table supplies selected-relative paths and synthetic evidence slots; scope bindings still use `Scope::resolve_all`. Existing evidence admission and provenance/citation reachability consume those slots. Inventory discovery cannot be done by the pure engine, so extending its existing evidence-reader seam is appropriate. `visit_agent_call` extends the existing durable reference walk instead of adding another dependency resolver. |
| Hosted evidence and materialization | `TenantEvidence::{inventory,artifact_file,artifact,stored,select,admit,cited,located}`; `Workspaces::open`; `Workspace::{materialize,place,unzip,untar,file,text,read,search,extract,mounts,work_room}` | New `inventory` factors the prior registry-read/inventory lookup from `artifact_file` and is reused for selection. Singleton omission remains in `artifact_file`; collection/directory selection is a distinct workspace operation. Staging remains in the existing admitted, per-invocation temporary workspace. Raw Artifact staging bypasses archive expansion by removing its archive format hint. Existing non-Artifact archive staging remains with its prior owner. No new filesystem owner or durable mount service was introduced. |
| Hosted tools and judges | `HostedTools::open`, `Invocation::{call,run}`, argument/descriptor changes; `AgentExecutor::{invoke,execute,check}`; migrated refund-reader fixture and support-desk reader diff | Steps and checks share the existing `AgentExecutor::invoke` loop; deleting the check-specific allowlist avoids parallel judging orchestration. `fs.extract` extends existing built-in dispatch and the existing writer/scratch owner. `fs.search` remains a Workspace operation using installed Rust regex and glob parsing. No shell/subprocess search or second tool registry was introduced. |
| Focused proof shape | Loader upload tests; engine `fixed_attachments_mount_selected_files_as_pinned_slots`; workspace read/extract/search tests; Python publication/extraction and Service-judge journeys; Rust/TS Service-judge upload call sites | Tests address caller-visible selection, exact paths, unchanged originals, generated outputs, inventory, and existing declaration behavior. The Python publication tests exercise only the same reduced argument subset as the new wrapper; they do not cover authored Metadata/ArtifactSpec preservation. Runtime execution and the broader acceptance matrix remain outside this static specialist verdict. |

The examined supporting types and helpers are cohesive with their owners.
There is no source-backed reason to replace the attachment table with a new
service, split the Workspace owner, introduce a parser abstraction, or add an
upload lifecycle. Such proposals would be optional redesigns and are omitted.

## Material finding

### MNT-001 — Python upload cannot publish caller-authored Metadata or ArtifactSpec

Classification: **GAP**.

Changed location: `sdks/wyrd-sdk-python/src/state/mod.rs:2015–2041`, especially
the default construction at 2025 and 2034–2036; matching authored declaration at
`python/wyrd/cards/__init__.pyi:521`.

Obligation: approved specification REQ-001 explicitly says the Rust, Python,
and TypeScript helpers accept existing Metadata and ArtifactSpec authoring
values (`spec.md:82–85`). REQ-010 also preserves caller metadata and refuses a
caller extraction report when extraction is requested. The maintainer guide
requires public types to carry their contract; thin projections must not narrow
away an approved capability that the shared owner already implements.

Source-proven scenario: a Python publisher needs an Artifact with a label and
ordinary `spec.metadata` such as its document revision, with optional extraction.
The new method accepts only `name`, `version`, `version_bump`, `artifact_kind`,
and `extract`. Passing `metadata` or `spec` is rejected by the PyO3 signature;
omitting them creates Metadata from only `name` and a default ArtifactSpec with
only `artifact_kind` set. There is no parameter through which labels,
annotations, Artifact metadata, content type, or existing reference-bearing
spec fields can reach the shared upload owner. The Rust method accepts both
full values (`wyrd-client/src/cards/handle.rs:299`), and TS forwards them
(`wyrd-sdk-ts/wyrd/src/index.ts:1824`), so this is a Python projection gap.

Consequence/materiality: a required publication operation cannot be expressed
on one first-class SDK. A caller must build another Card document and use the
general declaration flow to author these fields, losing the requested helper's
combined authoring/extraction operation. This is an explicit supported contract
omission, not a preference for option objects or matching signatures across
languages. It also means Python cannot exercise the shared caller-report
conflict rule or preserve unrelated metadata through opt-in preparation.

Smallest correction: expose caller-authored Metadata and ArtifactSpec values
through the Python boundary and deserialize/convert them with the existing
Python/serde boundary machinery before calling the unchanged shared
`Cards::upload_artifact`. Preserve the shared validation and extraction-report
owner; do not add Python metadata validation, a second preparation workflow,
new registry behavior, or a new Artifact holder merely for this helper. Align
the authored stub and focused Python tests. A dictionary projection is
sufficient if that is the existing Python authoring representation.

Focused closure proof: a Python upload supplies a label plus ordinary Artifact
metadata, enables extraction, and reads the exact returned Card through
`cards.get` to confirm both authored values and the generated extraction report
survive. Supplying `metadata.extraction` with extraction enabled must reach and
receive the shared refusal. This needs one extension of the existing upload
journey, with no new server harness or Rust lifecycle changes.

## Verification limits

No executable checks were repeated. This report does not certify concurrency,
filesystem confinement, tenant/RBAC enforcement, or the full journey matrix;
the selected behavior, invariant, security, and repository reviewers own those
independent checks. No source was edited. Only this report was written.
