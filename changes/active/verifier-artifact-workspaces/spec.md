---
id: SPEC-verifier-artifact-workspaces
revision: 3
status: approved
---

# Reusable Artifacts, explicit extraction, and verifier workspaces

## Problem and outcome

Users publish files independently of verification, then select which registered
files a Verifier's Agent can access and where they appear. The same Artifact
version can serve multiple Verifiers. An Agent receives application output at
execution time and uses explicitly declared tools to inspect its attachments.

Publication accepts arbitrary bytes. Optional client-side extraction prepares
readable files once before upload. Those outputs are ordinary versioned Artifact
files, not a cache. Mounting does not extract them again. Ordinary filesystem
tools read, search, and list; a separate extraction tool performs conversion.

For example:

```sh
wyrd artifact upload ./references --name shared-references --extract
```

The Artifact preserves the supplied paths and adds generated files:

```text
rules.xlsx
notes.md
extracted/rules.xlsx.json
extracted/notes.md.json
```

A user discovers these paths from upload output or Artifact inventory, then
selects a stored directory with the existing Artifact selector shape:

```yaml
workspace:
  /inputs/policy:
    artifact:
      card:
        kind: Artifact
        name: shared-references
        version: 0.1.0
      path: extracted
  /inputs/application-output: inputs.output
```

Here `0.1.0` stands for the exact version returned by publication. The Agent sees
`/inputs/policy/rules.xlsx.json` and can read it directly. It needs no knowledge
of the Artifact's stored `extracted/` convention. The originals are not mounted
unless selected separately. Existing registered and inline Agent forms remain.

## Rationale, cost, and limits

Keep declaration, versioning, storage, dependency resolution, and hosted
workspaces in their existing owners. Preparing extraction before registration
lets originals and outputs enter one initial manifest; the server does not need
to append files to an immutable activated Artifact. Existing storage, hashes,
download, and deletion cover all declared files equally.

Reuse the existing Rust parser implementation in a client-compatible shared
owner for upload preparation, explicit server extraction actions, citation
validation, and the new Agent extraction tool. Do not create separate Python or
JavaScript parser stacks. This adds parser code to native SDK packages; no new
Cargo feature, parser implementation, or extraction cache is required.

The deliberate public changes are optional upload extraction, inspectable output
mapping, Artifact file/directory workspace selection, ordinary filesystem-tool
semantics, and an explicit `fs.extract` tool available to steps and checks.
Existing callers relying on implicit document parsing must migrate within this
change. This work does not add formats, rendering/OCR, provider-native upload,
server-certified client transformations, or original-document citation mapping
for uploaded prepared JSON.

## Requirements

### Publish and inspect independently

- **REQ-001 — Upload convenience.** CLI `wyrd artifact upload SOURCE --name NAME`
  and language-idiomatic `Cards.upload_artifact` helpers in Rust, Python, and
  TypeScript publish one regular file or directory independently of consumers.
  Helpers accept existing Metadata and ArtifactSpec authoring values. CLI keeps
  existing connection/output and version/bump conventions, with optional
  `--artifact-kind` defaulting to `file`. Success returns the activated exact
  Artifact CardRef including UID. A file retains its basename; a directory
  retains all regular files at their relative paths. Do not scan a file's parent
  or exclude an ordinary entry because it resembles a Card document filename.
- **REQ-002 — Existing lifecycle.** Use the existing declaration and transfer
  workflow, path validation, nonregular-file/symlink refusals, version resolution,
  unchanged-content deduplication, exact-pin drift conflict, and activation
  rules. Interrupted transfers keep existing pending/reconciliation behavior;
  do not promise atomic rollback. Preserve ordinary get/list/pull/delete and
  inbound-reference deletion protection. Adding generated files after activation
  requires ordinary new-version publication, not inventory mutation.
- **REQ-003 — Artifact contract.** Reject empty/whitespace-only `artifact_kind`
  while preserving its open string vocabulary. Keep file paths, lengths and
  digests authoritative in the existing manifest/inventory. Descriptive URI or
  aggregate spec fields do not replace that inventory. Provide consistent typed
  SDK projections, including generated TypeScript Artifact types.

### Select attachments with existing references

- **REQ-004 — Artifact workspace selector.** Extend `AgentCall.workspace` using
  the existing `artifact: {card: Ref, path?: ...}` selector shape beside existing
  execution-scope reference strings. The `card` field retains existing exact
  CardRef, declaration-sibling, and authored Card-document reference behavior.
  Workspace strings remain scope expressions, not local uploaded-file paths.
  Visit these durable references through existing graph resolution and UID
  pinning so dependencies and deletion protection see them. Reject wrong Card
  kinds and unresolved references. Heavy Artifact bodies remain non-inlineable.
- **REQ-005 — User-selected files and directories.** In workspace selectors,
  `path` selects an exact inventory file or a directory prefix matched on path
  boundaries. Omission explicitly selects the whole collection. An exact file
  appears at the chosen destination; a directory/collection retains paths
  relative to its selected root beneath the destination. Reject missing,
  traversal and conflicting destination paths, including file/directory conflicts.
  The same source may be mounted at distinct nonconflicting destinations.
  Do not silently select originals, generated files, or both. Existing singleton
  path-omission rules on other Artifact consumers do not change. Callers cannot
  replace fixed attachments via execution input or route overrides. Stage only
  the selected inventory files, unchanged; do not infer parsing or archive
  expansion from their filename or MIME. Do not preload extracted text at mount.
- **REQ-006 — Judge parity.** Agent steps and checks use the same existing
  hosted workspace and explicitly declared tool machinery. Tool availability,
  Agent declarations and the executing principal's ordinary tool/resource grants
  govern use; remove the separate check-position tool allowlist. Reuse the
  existing repeated model/tool loop, including search, read and query turns
  before a final result, under existing iteration/deadline bounds. Existing
  `code.run` remains sandboxed Python; this change adds no shell runtime.
  Preserve read-only `/inputs`, bounded writable
  `/work`, per-invocation lifetime, and cancellation cleanup. Do not add Artifact
  attachments to AgentSpec, an Artifact scope root, or another workspace owner.

### Ordinary file operations and explicit extraction

- **REQ-007 — Filesystem semantics.** `fs.list` lists workspace paths/metadata
  without parsing contents. `fs.read(path, start?)` opens the selected file and
  returns bounded UTF-8 text with existing resumable pagination; it does not
  dispatch a document parser or read directories recursively. Invalid/binary
  text contents and directory reads fail explicitly. `fs.search` searches visible
  text files with the grep-style behavior in REQ-013 and no binary-document
  extraction. It does not expose unselected files.
  Original line positions describe the actual text file read, including JSON
  source text. Update descriptors, schemas, prompts and callers to these rules.
- **REQ-008 — Evidence and citations.** Include fixed attachments in existing
  evidence/source reachability. Uploaded generated JSON is its own registered
  evidence; citations identify that file, not the document it was derived from.
  Ordinary read/search line locators are verified against actual registered text
  bytes. Existing extraction-specific original locations remain valid under the
  original extractor's rules. `/work` files are scratch, never independent
  citation sources; scratch line numbers/JSON pointers must not masquerade as
  original-file locations. Invalid citations retain existing handling. Keep
  verification/requirement result schemas unchanged.
- **REQ-011 — Explicit Agent extraction.** Add `fs.extract` to the existing
  built-in tool catalog, permissions and generic provider-name projection.
  Required arguments are `path` and `output_path`. It reads one file from the
  existing workspace, runs the shared extractor, and writes its existing
  `ExtractOutput` JSON under `/work`. The bounded response contains
  `output_path`, original `sources` using existing source IDs (empty for scratch
  input), and `extractor_version`. The Agent can then read/search the output as
  ordinary text. Reject directories, unsupported/malformed input, invalid or
  existing output destinations, and exceeded limits. Failed extraction must not
  leave a usable partial destination. There is no recursive extraction or
  implicit overwrite. Retain the existing declarative extract action. The tool
  reports origin and version, but does not invent cross-file citation translation
  or add fields to verification results.

### Grep-style search within the existing Agent harness

- **REQ-013 — Multi-file content search.** Extend the existing `fs.search` tool;
  do not introduce a shell, subprocess, second search tool or filesystem owner.
  Its proposed arguments are `query` (required), `regex` (default false),
  `case_sensitive` (default false), optional `path`, and optional `glob`.
  Existing `{query}` calls retain case-insensitive literal matching. Regex mode
  uses the existing Rust regex syntax, with explicit diagnostics for invalid or
  unsupported patterns; do not promise PCRE look-around/backreferences.
  Search is line-oriented, one result per matching line, across all selected
  `/inputs` files and readable `/work` text when no path is supplied. A supplied
  path scopes to one workspace file or recursively to a directory; a missing or
  invalid scope fails explicitly. `glob` filters filenames, not content: `*`
  matches within a path component, `**` spans directories, and `?`/character
  classes use ordinary glob syntax. A glob without `/` matches the basename;
  otherwise it matches paths relative to the scoped directory (the file's parent
  for a file scope), or workspace root when path is omitted. Reject absolute,
  traversal and invalid glob patterns. Matching never escapes mounted inputs
  or scratch. Do not silently omit explicitly mounted hidden files or apply
  repository ignore files; the workspace selection defines available evidence.
  Skip binary/non-UTF-8 contents without extraction. Return the existing
  `matches`/`truncated` shape with file paths, actual text line locators,
  bounded snippets and source IDs; multiple matches on a line produce one hit.
  Keep deterministic path/line order and existing host output/traversal/deadline
  bounds. A truncated search must say so; an empty match set is a valid result,
  distinct from an invalid pattern. Larger context is obtained through fs.read.
  Update tool schemas, descriptors, SDK contracts and examples consistently.

For example, after mounting selected extracted files, an Agent can call:

```json
{"query":"refund|reimbursement","regex":true,"path":"/inputs/policy","glob":"*.json"}
```

It then reads a matching file, optionally queries evidence, and returns its
existing structured result through the same Agent loop. Regex mode adds content
matching, not shell evaluation or document parsing.

### Optional preparation before upload

- **REQ-009 — Client-side opt-in.** CLI `--extract` and SDK named `extract`
  argument default to false. When enabled, run the shared extractor locally
  before declaring the Artifact. Prepare supported files in temporary storage,
  leaving the source tree untouched. Preserve unsupported files as originals
  and report them as unextracted. Corrupt supported files, extraction limit
  failures, and output conflicts fail preparation before registration. Do not
  publish a helper-generated output paired with different original bytes if a
  source changes during preparation/transfer. Apply the shared bounded defaults
  through existing hashing/transfer checks. Submit originals and outputs together;
  do not add a server post-upload transformation or finalization lifecycle.
- **REQ-010 — Output naming and discovery.** Source `P` produces
  `extracted/P.json`, retaining its original extension and relative directories.
  Extract only the supplied source snapshot, not outputs the helper generates.
  Reject generated-path/ancestor collisions rather than rename or overwrite.
  The helper records its report in existing typed `ArtifactSpec.metadata` under
  `extraction`: `version` is the extractor version; `files` lists `{source,
  output}` path pairs; `skipped` lists unsupported source paths. Represent these
  using existing NonSecretValue object/list/string values; do not duplicate
  sizes/digests there. With extraction enabled, refuse caller-supplied
  `metadata.extraction` rather than overwrite it. These remain client-submitted
  claims, not server attestations. CLI human output displays generated/skipped
  paths. CLI JSON and SDK success keep the exact CardRef return; public
  `Cards.list_artifacts` and `wyrd artifact files` expose the existing inventory
  using existing selection/response conventions. Inventory plus Card metadata
  make extraction outputs discoverable later, with no new endpoint or receipt
  wrapper. Without extraction, ordinary submitted files/metadata remain ordinary
  Artifact content and receive no claimed transformation certification.
- **REQ-012 — One parser implementation.** Share the existing Rust Extraction
  implementation and ExtractOutput with upload authoring and server consumers.
  Remove its server-runtime coupling at the shared boundary; libraries at the
  client tier must not import the server or Vala block executor. Reuse the
  existing parser dependency stack and extractor version. Correct current
  mount-time/read-time implicit parsing instead of preserving it under a new
  name. Update all affected SDK fixtures, prompts, examples and tests; preserve
  existing explicit-extract behavior and its original-locator validation.

## Invariants and boundaries

- **INV-001 — Immutable identity.** Bind exact versions with UID consistency;
  never follow latest/ranges or retarget an existing binding after publication.
  Original and generated entries share ordinary Artifact version/deletion rules.
- **INV-002 — Caller authority.** Use the accepted caller's tenant and existing
  `cards:read`/`cards:write` grants, plus each tool's `tool:invoke` grant.
  An application authenticated as a Service principal uses that principal;
  selecting a Service Card alone does not switch the accepted identity.
  Declaration/mounting confers no publisher/system authority. Preserve ordinary
  nonblocking permission audit behavior.
- **INV-003 — Verified, bounded bytes.** Resolve object locations from
  tenant-scoped inventory and verify lengths/digests. Account only selected
  files, but count them together with dynamic inputs under existing file,
  workspace, output, concurrency and deadline bounds. Explicit parsing retains
  archive/page/text protections; writing outputs retains scratch limits.
  Unsupported extraction does not prevent plain upload, byte mounting or use
  by another suitable tool. Preserve cleanup on success, failure and cancellation.
- **INV-004 — Existing owners.** Keep contracts in wyrd-spec, durable storage in
  existing server owners, upload orchestration in Cards, shared parsing at a
  client-compatible owner, and thin SDK projections. No new binding DSL, scope
  root, cache, storage lifecycle, endpoint, result schema, parser implementation,
  provider upload path or unrelated capability. Reuse installed dependencies and
  existing owners first; additional dependencies, Cargo features or migrations
  need source-backed justification, not an automatic spec stop. Changes to
  approved public contracts, durable semantics or security boundaries still need
  spec review. Existing MCP Card discovery stays read-only; no MCP upload tool.

Code action/check data contracts, standalone Agent hosting, extraction
attestation, original-document mapping for prepared JSON, new file formats and
SDK size optimization and shell execution are outside scope.

## Acceptance and credible evidence

| ID | Situation and expected observation |
|---|---|
| AC-001 | Publish a file and nested collection before any consumer. Returned identity is Active; inventory/pull preserve selected source bytes and names. Unchanged, changed and exact-pin submissions follow existing version rules. |
| AC-002 | Two Verifiers, using registered/inline Agent forms, select files/subdirectories from the same exact Artifact. Only requested contents appear at chosen paths. Dependency identity persists after a new version; omitted workspace path explicitly selects the collection, while other consumer singleton rules regress unchanged. |
| AC-003 | Python uploads with extraction, discovers generated paths, creates a Service with a bound Verifier, then verifies through that Service. Its Agent check reads only selected prepared files with an explicitly declared tool and returns a valid citation to that registered file. |
| AC-004 | The mock gateway requests the mounted path and accepts judgment only after the real tool response carries a fresh uploaded marker under the matching call ID. The marker is absent from prompts/input and fixed responder answers. Bad paths, denied tools and read errors cannot produce the successful transcript. |
| AC-005 | Cross-tenant/denied access, wrong kind/UID, inactive/missing references, corrupt bytes, path conflicts, replacement attempts and limit failures are refused without protected content reaching tools/model. Audit and cleanup persist. |
| AC-006 | Opaque bytes mount/list and reach existing sandbox processing without parsing. Ordinary text/JSON read/search returns actual text lines, with bounded pagination. No directory read or mount invokes extraction; unselected originals are neither staged nor parsed. |
| AC-007 | Rust/TypeScript and CLI ship equivalent upload, optional preparation, inventory, selection, tool and Service-bound behavior. Types/exports/stubs and generated contracts agree. Existing explicit-extract and building-block journeys adapt without losing required behavior. |
| AC-008 | Opt-in upload generates deterministic `extracted/P.json` files plus typed source/output/version/skipped metadata before registration. Pull preserves originals and outputs. Unsupported inputs are reported; malformed supported inputs, collisions and source mutation cannot yield a misleading successful publication. Repeated runs read stored outputs without re-extraction. |
| AC-009 | An Agent explicitly invokes fs.extract, reads its written JSON, and receives source/version information. Single-file, no-overwrite, confinement, malformed/unsupported input, output limits and partial-file cleanup are enforced by the real host. |
| AC-010 | Ordinary text citations use actual registered line locations; prepared JSON citations identify that JSON. Original extraction citations still validate with existing locators; scratch-file locations and fabricated derivation claims cannot impersonate original evidence. |
| AC-011 | Search nested selected inputs and scratch across many files using literal/regex, path and glob filters, both case modes, and actual line locations. Invalid patterns differ from no matches; binary files are skipped, hidden selected files remain searchable, limits report truncation, and unselected files remain invisible. A real search → read → query → final sequence proves reuse of the bounded Agent loop and declared/principal-authorized tools. |

Only the model upstream is mocked in journeys. Registration, storage, Service
binding, workspace, tools, extraction, authorization and citation validation stay
real. The mock proves data access/orchestration, not model judgment quality.

## Authority, approval, and revision history

Revision 2 remains the record of the approved artifact/extraction direction.
Revision 3 was explicitly approved by the user on 2026-10-26, including the
regex/grep expansion and public search arguments/defaults specified above.
There are no open material decisions. Approval does not start implementation.

The approved direction carried forward is:

- independent reusable Artifact upload through existing Card lifecycle/identity;
- optional client-side extraction, stored as ordinary files under `extracted/`;
- user-selected original/generated files or directories at chosen mount paths;
- ordinary read/search/list and separate explicit extraction using existing tools;
- existing references, result contracts, authorization and repeated Agent loop;
- the Python Service-bound journey with real tools and a mocked model upstream.

Subsequent user clarification removes the check-specific tool allowlist: available
and declared tools run under the executing principal's permissions. The user also
accepted allowing repeated sources at nonconflicting destinations and replacing
absolute implementation bans with justified choices. These are distinct from the
new proposed search signature. This record does not claim an unlisted set of
“five recommendations” was approved. Private implementation choices remain owned
by implementation; new public-contract choices require approval.

Implementation must reconcile the Artifact, references, Verifier, filesystem
operations and Agents/tools/workspaces sections of
[Wyrd design](../../../architecture/wyrd-design.md), the repeated judge rules in
[evaluation guidance](../../../.agents/skills/wyrd-advise/references/evaluation.md),
and affected public tool/CLI/SDK docs. The approval intentionally changes the
old check-tool prohibition and implicit document-read behavior. Other authority
remains [security posture](../../../architecture/wyrd-security-posture.md),
[repository rules](../../../AGENTS.md) and
[spec-driven development](../../spec-driven-development.md).

- Revision 1 — approved independent upload, Artifact workspace references,
  existing reader access for judges, and the mocked-gateway Python Service proof.
- Revision 2 — approved explicit extraction, client-prepared ordinary Artifact
  outputs, inspectable paths, selective mounts and representation-correct tools
  and citations; retains one specification and one implementation task.
- Revision 3 — approved grep-style multi-file regex search; records the subsequent
  approved authorization/destination/implementation corrections and explicitly
  keeps shell execution outside this change.
