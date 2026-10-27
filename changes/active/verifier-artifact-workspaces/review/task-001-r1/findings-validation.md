# Independent findings validation

Subject: base `1c52b78bb`, candidate `da7622b52994946e03b9ba241032b99d096063cf`. First review; no prior findings or closure evidence. HEAD remained the candidate. The dirty Python building-block test and unrelated service-identity task are excluded; Python source below means `git show da7622b52:<path>`, not the working copy.

Read the complete review skill, scope and all five selected discovery reports. Independently inspected the proposed paths, their producers/consumers, the cumulative diff and the applicable spec/task, AGENTS, testing, Rust/error/maintainer and architecture authorities. CodeGraph was used before textual source navigation. No reviewer agreement was treated as proof. No Cargo, server or SDK tests were run. Reports only were changed.

## Proposal disposition

| Discovery proposal | Disposition | Deduplicated issue |
|---|---|---|
| B-001 | CONFIRMED | FIND-TASK-001-1 |
| MNT-001 | CONFIRMED (classification normalized from GAP to MISSING) | FIND-TASK-001-2 |
| RSTD-001 | CONFIRMED; consolidate with the same Python input boundary | FIND-TASK-001-2 |
| B-002 | CONFIRMED as two explicit proof omissions, not an established production defect | FIND-TASK-001-3 |
| RSTD-002 | REVISED: require exact codes for the existing raised Wyrd failures and existing structured tool failures; no new error schema or public failure behavior | FIND-TASK-001-4 |
| RSTD-003 | CONFIRMED, bounded to new Card setup/fixed identities; generated uploaded bytes and fresh markers remain legitimate | FIND-TASK-001-5 |
| RSTD-004 | CONFIRMED, three concrete documentation omissions batched with functional correction | FIND-TASK-001-6 |
| Standards observation: missing public upload/inventory CLI/SDK guidance | CONFIRMED against explicit spec/task authority; this is a discovery observation, not a newly invented validator finding | FIND-TASK-001-7 |
| Standards observations: absent lowered-limit unselected-file, audit and cancellation demonstrations | REJECTED as additional remediation obligations on this evidence; retain the verification limits | No finding |
| Historical RED-first absence | Confirmed process/evidence limitation; no retrospective fabricated RED or production correction | No finding |

No proposed material finding was discarded through reviewer voting. No additional blocker was discovered or self-validated. The invariant/security PASS conclusions cover their scoped boundaries and do not contradict missing public projection or explicit acceptance proof.

## Validated ledger

### FIND-TASK-001-1 — CONFIRMED / INCORRECT — wide matches erase discoverable search hits

Discovery: B-001. Obligation: approved REQ-013 and AC-011; architecture `wyrd-design.md` Agents, tools, and workspaces promises bounded per-line hits, with full context obtained by read.

Exact source: `crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:551–628`, especially 620–624, and `snippet` at 935–938. Read the full search/snippet/Collected bodies and their text/file input owners. `Invocation::call` dispatches every declared and authorized `fs.search` to this Workspace method. Both indexed input files and readable scratch files reach the same loop; no sibling implementation repairs its projection.

Cause and consequence: the regex producer returns a byte span; search passes its entire length to `snippet`, whose upper bound is `80 + len`. A UTF-8 line beginning `REFUND` and containing about 70 KiB is admitted under ordinary file bounds. Regex `REFUND.*` consumes the line, and its hit exceeds the default 64 KiB tool-result budget (`hosted/mod.rs:142`). `Collected::push` rejects that first hit, and search returns an empty matches array with truncation. The same file with a short literal query yields a located hit. The loss is of the discoverable path/line, not merely optional surrounding context. This is a supported regex over serialized records/log lines and violates the explicit bounded-snippet obligation.

Smallest correction: bound the snippet independently of match length in the existing shared projection, retaining UTF-8 boundaries, real locator/source identity and the existing whole-result collector. Do not enlarge budgets, add options/cursors/dependencies, duplicate search, or attempt to return the full regex span. A consumer guard in an SDK or provider is too late; all use this producer. Burden: one local projection correction and its focused regression.

Closure: extend `verification::hosted::workspace::tests::workspace_search_matches_lines` with a long spanning regex and multi-byte text; assert a located hit, bounded snippet and serialized response under the configured result limit. Keep existing literal and many-hit truncation coverage. Run the exact nextest selector in `wyrd-server` plus Rust format/lints for the changed write set. No new host journey is necessary for this pure projection bug.

### FIND-TASK-001-2 — CONFIRMED / MISSING and INCORRECT — Python upload narrows authored values and misclassifies invalid names

Discovery: MNT-001 and RSTD-001, one correction boundary. Obligation: REQ-001 explicitly requires existing Metadata and ArtifactSpec authoring values in every SDK; REQ-010 preserves ordinary caller metadata and refuses an authored extraction report when preparation is enabled. Python AGENTS Errors and `agent_docs/errors.md` require repairable structured validation failures.

Exact source: `sdks/wyrd-sdk-python/src/state/mod.rs:2015–2043`, authored stub `python/wyrd/cards/__init__.pyi:521–556`; shared `Cards::upload_artifact` at `crates/shared/wyrd-client/src/cards/handle.rs:297–310`; `ArtifactUpload::prepare` at `wyrd-loader/src/upload.rs:62–188`. Read these full bodies, the existing Python conversion/selector validators, Metadata and ArtifactSpec definitions, and both other SDK projections.

Producer-to-consumer proof: Python accepts only name/version/bump/kind/extract, constructs Metadata from name, and constructs an otherwise default ArtifactSpec. There is no input through which labels/annotations, spec metadata/content type/reference fields can reach the shared owner. These are not merely dropped optional Python annotations: they are the very existing authoring values REQ-001 requires. Rust already takes full typed values, and TS forwards full metadata/spec. The shared preparation preserves those values and adds only its extraction report, so extending that shared owner is unnecessary.

The same Python construction uses bare `?` for serde conversion. `Metadata.name` is CardName (`envelope.rs:163`), whose Deserialize calls its validated constructor (`ids.rs:56–64`). An empty/invalid name fails before source preparation or connection. Its serde error selects `WyrdPyError::from(serde_json::Error)` (`wyrd-utils/src/py.rs:85–95`), which deliberately produces Internal. Existing Python `parse_name` at state/mod.rs:3535 instead maps the same constructor refusal to Validation. Thus a routine invalid name is publicly misclassified, independently of the projection omission.

Smallest correction: expose full caller Metadata and ArtifactSpec mappings at this Python boundary using the existing Python-object-to-JSON conversion and serde domain deserialization, with explicit Validation conversion carrying the offending field for caller mistakes. Existing convenience arguments may continue to lower to those same domain values; do not create an Artifact holder, second validation/parser workflow, compatibility shim or new lifecycle. Keep the shared preparation/declaration untouched and the global serde-to-Internal conversion untouched. Align the native signature, authored stub and affected Python callers. This is an additive projection of the approved contract, not a new product decision. The exact local parameter layout is reversible implementation detail; acceptance requires complete values to reach the existing owner without ambiguous conflicting values.

Closure: extend `test_artifact_upload_extracts_before_registration` to supply labels/annotations and ordinary spec metadata, upload with extraction, and read back the exact returned Card to prove preservation plus the generated report. Supply a typed NonSecretValue `spec.metadata.extraction` and assert the existing exact Validation code/rule on refusal. Add a focused public Python invalid-name assertion for `WYRD_SPEC_400_VALIDATION` and its field before any connection/registration. Run that exact upload selector and the new narrow boundary test, `py:typecheck`, `py:lints`/format, and Rust format/lints for the binding. No Rust lifecycle change or cross-SDK rerun is required solely for Python projection.

### FIND-TASK-001-3 — CONFIRMED / MISSING — two specified real-host observations are absent

Discovery: B-002. Obligations are explicit AC-006 and AC-011, repeated by task scenarios 4 and 5. This issue concerns acceptance evidence; inspected production dispatch supports the paths but cannot substitute for these expressly required real observations.

Exact candidate source: `sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py:1151–1247` and `:1250–1339`. Read complete selection/raw/search tests, Probe setup/play and upstream `probe_turn`, plus `Invocation::call/run` at hosted/tools.rs:192–320.

Opaque bytes are uploaded/mounted/listed, then refused by text read or skipped by search; these tests declare no code.run. Search declares only search/read/extract, and its asserted terminal sequence is extract/search/read. Its script contains no Bifrost query. The task evidence acknowledges both omissions. Older independent sandbox/query tests cannot demonstrate these selected Artifact and mixed-tool paths. Producer → consumer would be upload/selection → raw mount → existing sandbox, and real search response → read → authorized BoundedQuery → final Agent turn. Current tests stop short of those consumers.

Smallest correction: extend the existing raw-file and search journeys. One declared/granted code.run call reads the mounted opaque file and returns a deterministic byte-derived result checked against its uploaded bytes. The search story performs search → read → real bifrost.query → final, with exact tool grants and ordinary query-domain authority; captured responses/order and response-sensitive completion prove each real operation. Reuse existing sandbox, query fixture and upstream mock; do not invent a new runner, parser fallback, fixture framework or mock storage/query. Keep the current semantic cases. Burden: two bounded additions in existing proof owners, integrated with the fixture correction below; production changes only if the proof exposes a real defect.

Closure: run exactly `test_agent_file_tools_read_without_extraction` and `test_agent_file_search_supports_regex_across_files` through the repository Postgres/WyrdTestServer lane documented in the original task. Inspect the byte result, captured order and actual query response before final completion. The working-tree additions are not candidate evidence and provide no closure in this round.

### FIND-TASK-001-4 — REVISED / VIOLATION — new Python refusal proof accepts unrelated failures

Discovery: RSTD-002. Mandatory authority: Python AGENTS Tests and TESTING.md Client-facing tests require one exact catalog code on raised WyrdError, prohibit message matching and any-of-code assertions.

Exact committed source: test_verifier_building_blocks.py:900, 916, 956/962, 1431, 1441/1443 and 1451. Full relevant tests were inspected. Version-conflict production yields a WyrdError consumed only by a `"409"` substring assertion; upload negative loops accept any WyrdError; selected-workspace refusals accept either of two codes; foreign declaration/inventory accept any WyrdError; missing-prefix assertion matches prose. These checks can pass when the actual boundary returns Internal or a refusal from the wrong stage. The invalid-name path above establishes why classification is meaningful, but this is independently an explicit test gate.

Smallest correction: bind each existing raised-error case to its one expected stable catalog code, with field/rule where required to distinguish validation. Parameterize the case's expected code rather than retaining the REFUSED union. Use existing structured tool-result codes where exposed. Retain content-nondisclosure, no-Card and before-model assertions. No change to public errors or a new tool-result schema is authorized merely to strengthen these tests: plain tool failure representation is not a newly found runtime defect. Do not demand every refusal become a new public error variant.

Closure: rerun exactly the existing publication, upload-extraction and workspace-refusal selectors after their assertions change; if additional existing raw/search/extract negative assertions are changed, their exact selectors suffice. These are the same journeys already required by the other corrections, so do not duplicate execution. Python format/lints/typecheck cover the write set.

### FIND-TASK-001-5 — CONFIRMED / VIOLATION — new client journeys generate Card documents and identities

Discovery: RSTD-003. TESTING.md Client-facing tests explicitly requires checked-in fixture YAML, fixed meaningful names and fixture setup returning domain objects.

Exact committed source: test_verifier_building_blocks.py:855–857 (`fresh`), 1026–1086 (`write_probe`), 1090–1122 (Probe constructor), plus its positive/refusal callers at 1151–1456. `write_probe` produces Agent, Verifier and Service documents from Python dictionaries, supplies random names, and writes JSON to YAML-named files. Probe registers those generated Cards; positive selection/read/search/extract and malformed-workspace stories all consume this same authority. Random names also reach ordinary upload tests. These are new public journey artifacts with a concrete mandated source/readability boundary, rather than an optional dislike of helpers.

Smallest correction: remove the Card document builder. Author the positive and deliberately malformed Card variants in existing `fixtures/cards/<story>/` with distinct fixed story names and existing exact references, and use ordinary declaration/pull fixtures returning the registered Card/runtime objects. Use fixed distinct upload identities matching those fixtures; do not rewrite checked-in Cards at runtime or add a template/substitution system. Keep generated source files and the required fresh marker as data, and preserve real registration/storage/tools/authorization and all refusal cases. Scripted mock turns may remain as the model upstream. Fixture organization/private setup method layout is implementation-owned, not a new review requirement.

Burden: replace one shared generator/setup boundary and its callers, not redesign the suite. Deletion of the negative cases or replacement with mocks would not preserve the task. Closure: statically verify the new stories read checked-in Cards and have fixed names; run the exact affected selection/raw/search/extract/refusal journeys (and the publication/extraction selectors for renamed upload identities). Existing fixtures may be declared again under their fixed identities without random suffixes. Python formatting/lints/typecheck suffice beyond those focused journeys.

### FIND-TASK-001-6 — CONFIRMED / VIOLATION — changed-item documentation gate remains incomplete

Discovery: RSTD-004. Root AGENTS and agent_docs/rust.md:235–240 explicitly cover fields and test helpers, possible panics, and async/durable cancellation/partial-progress/idempotency/retry. The review skill forbids waiving this gate.

Exact source and proof:

- `wyrd-loader/src/extract.rs:68`: new public `ExtractError(pub String)` has type documentation but none for its public payload field.
- `wyrd-loader/src/upload/tests.rs:29–35`: `paths` indexes the publicly mutable submissions vector. Its body and all callers assume one prepared submission, but its docs omit that invariant and possible panic. The invariant explains the index; tests do not make indexing intrinsically non-panicking for the signature.
- `wyrd-client/src/cards/handle.rs:278–310`: public async upload delegates blocking preparation and durable declare without documenting interruption/retry. The preparation task can outlive the waiter; after submission the existing saga may leave partial/pending or completed Cards. `cards/saga/mod.rs:20–73` documents that ordinary non-atomic lifecycle and mints a new saga idempotency key per call. A retry must not be described as unconditional same-key atomic replay.

Smallest correction: document the payload meaning, helper invariant/# Panics, and actual upload interruption/partial-progress/retry semantics at those owners, referring to existing declaration reconciliation. Do not change cancellation, invent rollback, add a cleanup service or promise stronger retry semantics. Burden: bounded rustdoc only; batch with functional remediation. Closure: source-check those three docs against the unchanged owners and run formatting/lints already required for functional changes. No runtime test solely for documentation and no separate style-only round.

### FIND-TASK-001-7 — CONFIRMED / MISSING — public upload/inventory workflow guidance is absent

Discovery: standards acceptance observation. Approved spec's final authority paragraph explicitly requires affected public tool/CLI/SDK docs; task Implementation Reference specifically names public CLI/SDK guides. This is a required completion item, not an optional documentation proposal.

Source proof: cumulative public docs changes contain only agent-tools-and-authorization, extraction-output and verifier-card references plus the generated machine copy. These describe file tools/selections/extraction output but not how to publish through `artifact upload`/language helpers and inspect stored files. Searching the candidate public content for `artifact upload`, `artifact files`, `upload_artifact`, `uploadArtifact`, `list_artifacts`, and `listArtifacts` finds no such workflow. Architecture Artifact section documents the behavior, Python stubs describe their method, and native/public Rust/TS methods exist; none supplies the requested public CLI/SDK workflow guidance. A user encountering selected prepared files cannot follow the new independent publication/inventory operation from the public docs.

Smallest correction: extend the appropriate existing public CLI/SDK documentation with upload, optional extraction and authoritative inventory discovery, exact Active identity/version behavior, and links into the existing workspace/tool references. Use current signatures after the Python correction and established docs components; preserve connection/output conventions. Do not create a new documentation system or demand an expansive guide. Reuse runnable journey-backed source for copyable examples as docs/AGENTS requires; avoid duplicate handwritten snippets. Regenerate the machine-readable copy through its owner. Burden is bounded documentation/examples, not a new capability.

Closure: inspect that the public docs let a reader publish and discover paths using shipped CLI and SDK surfaces and describe plain versus opt-in preparation accurately. Run `mise run docs:check` and the owning machine-copy generation/check. If a new runnable example is used, run its focused existing journey; broad cross-SDK aggregates remain change-review work.

## Acceptance and mandatory-gate reconciliation

| Obligation group | Reconciled assessment |
|---|---|
| REQ-001–003, AC-001, AC-007 | Shared lifecycle/inventory/version behavior is supported by source and recorded publication proof; Python full-value projection fails (-2), its error boundary fails (-2), and public usage completion fails (-7). Generated TS/metadata inventory wiring is supported; no additional lifecycle owner is required. |
| REQ-004–005, INV-001, AC-002 | Source and engine/selection proof support exact pinning, ordinary durable visitation, file/directory/collection semantics and selected-relative placement. `attach` enumerates selected files before admission/materialization. No extra lowered-limit journey is imposed just because a reviewer observed its absence. |
| REQ-006, AC-003–005, INV-002 | Shared Agent/tool invocation retains declared-tool resolution, exact invoke authorization, accepted principal and downstream query/Card authority. Handbook Service proof is content-sensitive and uses actual selected registered bytes/citations. Mixed-tool acceptance proof remains missing (-3); scoped security PASS does not fill it. |
| REQ-007–008, AC-006, AC-010 | Raw text/line/pagination and reachable original/prepared/scratch citation identities are supported by read/text/file and evidence owners plus existing tests. Real opaque sandbox observation is missing (-3). No implicit parser restoration or scratch citation identity is permitted. |
| REQ-009–012, AC-008–009 | Shared parser/preparation/report/manifest flow, pre-declare failures, private scratch staging and explicit extraction/no-overwrite paths are supported. Shared parser move is required, not duplicated machinery. Python cannot yet preserve/report-conflict-test full authored metadata (-2). |
| REQ-013, AC-011 | Existing search handles required case/path/glob/hidden/binary/line behavior, but wide-match hit projection fails (-1), and the exact combined sequence lacks proof (-3). |
| INV-003 and failure/recovery | Selection is upstream of evidence admission; selected files share per-file/combined limits and stored length/digest verification. Workspace TempDir/permit/Arc owners remain per invocation. Cancellation drops the whole Agent/session, so release of the async writer guard does not prove a surviving same-workspace writer race; blocking closure retains workspace lifetime. No independent new cancellation/rollback mechanism is warranted. Audit remains at existing nonblocking authorization owners. Existing tests/source suffice for unchanged boundaries, with limits below. |
| INV-004 and non-goals | Existing Cards saga, loader parser, reference visitor, evidence/workspace and Agent loop are extended. No source-backed extra owner/DSL/cache/endpoint/parser/shell/result schema was established. No optional refactor is retained. |
| Mandatory standards | Changed-item rustdoc fails (-6); exact negative-error tests fail (-4); checked-in Card fixture/fixed-identity gate fails (-5). New helper/type/export wiring otherwise has selected coverage. Public docs completion fails (-7). |
| Verification discipline | Recorded green format/lint/codegen/type/journey evidence was read, not reproduced. Test adjustments have concrete recorded permission/raw-UTF8/lint diagnoses; no supported product failure was hidden by a timeout/ignore/allow adjustment. GREEN-first history remains disclosed and cannot be retroactively repaired by inventing RED. |

## Closure scope and limits

Retained issues require no new product, public wire, persistent-data, security, concurrency or resource-ownership decision. Full Metadata/ArtifactSpec Python projection is already approved. All correction recommendations reuse their current owner and preserve adjacent lifecycle/authorization/cleanup behavior. The smallest credible proof is the existing exact Python selectors plus the one exact Workspace unit selector and static/type/lint/docs gates for changed files. Shared selectors should run once after combined changes; no full journey suite or broad `gate` is required for this remediation.

Use the original task's `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::<exact selector>'` recipe. The seven affected Python selectors are publication, upload extraction, workspace selection, raw file tools, grep search, explicit extraction and workspace refusals, as named above. Keep the unchanged handbook/cross-SDK evidence; rerun a selector only if the final correction changes its dependencies/fixture behavior. The search unit command is `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::workspace_search_matches_lines)'`.

Skipped executable verification and platform/load/crash experiments. Recorded tests do not newly assert audit persistence, extraction cancellation, or lowered-limit exclusion of oversized unselected files; source tracing and existing owner tests support those unchanged invariants, with these limits preserved. This ledger does not certify uncommitted work or assign an overall task verdict.
