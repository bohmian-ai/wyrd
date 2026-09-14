# TASK-002-R1 Findings Validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Review worktree: `/tmp/wyrd-task-002-r1-review-4d9d74b34`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior review and remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/`
- Wave 1 inputs: `task-review.md`, `standards-review.md`, `domain-review-public-sdk-contracts.md`, `domain-review-security-rbac-tenancy.md`, and `domain-review-stream-lifecycle-durability.md` in this review directory.

The candidate worktree was clean and remained at `4d9d74b34803b3f27d55f740a5b40ffbe968b306` throughout validation. The reviewed subject is the complete cumulative base-to-candidate range, not only the remediation commits. No `.codegraph/` directory exists in the review worktree, so source and caller inspection used repository search and direct file reads.

## Wave 1 disposition

| Source ID | Validation | Final finding | Reason |
|---|---|---|---|
| `SDK-CONTRACT-R1-01` | REVISED | `FIND-TASK-002-1` | The shared conversion now preserves catalog identity but still discards the safe variant metadata the same public error contract requires. This is incomplete closure of the existing finding, not a new error authority. |
| `SDK-CONTRACT-R1-02` | REVISED | `FIND-TASK-002-3` | The seven responses now name `WyrdProblem`, but their generated media type still disagrees with the single runtime problem mapper. This is incomplete closure at the same OpenAPI boundary. |
| `SDK-CONTRACT-R1-03` | CONFIRMED | `FIND-TASK-002-14` | The canonical reading guide is reachable from the public Bifrost index and names two SDK classes removed by this cumulative change. It is distinct from the five broken file embeds closed under prior finding 4. |
| `REPO-R1-001` | CONFIRMED | `FIND-TASK-002-7` | The prior remediation required documentation for every added or materially relocated Rust item; live trait implementations, a public fallible method, private conversions, test modules, helpers, and tests remain undocumented. |
| `REPO-R1-002` | CONFIRMED | `FIND-TASK-002-8` | Newly owned signatures and one enum field still use qualified type paths despite the explicit top-level-import rule. |

The task implementation reviewer, security/RBAC/tenancy reviewer, and stream lifecycle/durability reviewer proposed no additional findings. Their empty ledgers were checked against the relevant source paths and do not contradict the five retained findings: none of these findings changes authorization, tenant selection, persistent state, producer admission, query settlement, or stream ownership.

## Validated finding ledger

### FIND-TASK-002-1 — REVISED — INCORRECT: the shared client-error projection drops safe structured metadata

- **Wave 1 source:** `SDK-CONTRACT-R1-01`.
- **Violated obligation:** REQ-023, REQ-024, REQ-024A, REQ-025, AC-002, `architecture/references/languages/errors.md`, and the specification acceptance rule that Rust, Python, TypeScript, HTTP, gRPC, MCP, and CLI preserve the same stable identity and structured metadata.
- **Exact location:** `crates/shared/wyrd-client/src/error.rs:20-53,89-110`; catalog remediation at `crates/wyrd-spec/src/error.rs:3182-3194`.
- **Caller and reachability trace:** the complete conversion body creates one empty `details` object for all variants. `RegistryEngineError::Client` calls it at `crates/shared/wyrd-client/src/cards/error.rs:31-50`; `BifrostClientError::Client` calls it at `crates/shared/wyrd-client/src/bifrost/query.rs:120-146`; `ClientScope::from_config` calls it at `crates/shared/wyrd-client/src/bifrost/scope.rs:35-53`; and HTTP token-exchange failures call it at `crates/shared/wyrd-client/src/transport/http.rs:735-744`. Those owners feed the public Rust facades and their Python and TypeScript projections. The variant definitions explicitly say `Config.field`, `Config.reason`, and `TransportDown.transport` are echoed into the catalog payload, but the conversion ignores them. `NoCredentials` has no structured variant field to preserve.
- **Observable consequence:** `WYRD_CLIENT_400_CONFIG_INVALID` tells callers to correct the field named in `details`, yet supplies `{}`; every projected language must parse display text to recover the documented field and reason. Transport failures likewise lose their safe transport discriminator.
- **Decision-complete minimum correction:** keep this single conversion owner and populate `details` directly while matching the existing enum: `{"field": field, "reason": reason}` for `Config`, `{"transport": transport}` for `TransportDown`, and `{}` for `NoCredentials`. Do not add another projector, change messages, include raw transport failure text in `details`, or alter server-originated errors.
- **Focused closure proof:** extend the existing shared conversion/Cards test to assert the exact configuration and transport detail keys, then assert one existing public Python or TypeScript projection receives those same keys without parsing `message`. Run the narrow owner and language test commands plus format/lints.

### FIND-TASK-002-3 — REVISED — INCORRECT: Bifrost OpenAPI refusal media types disagree with runtime

- **Wave 1 source:** `SDK-CONTRACT-R1-02`.
- **Violated obligation:** REQ-023, REQ-059, AC-002, and `architecture/references/languages/errors.md` require generated HTTP authority and runtime structured errors to agree.
- **Exact location:** `crates/wyrd/wyrd-server/src/bifrost/routes.rs:20-104`; `crates/wyrd/wyrd-server/src/query/routes.rs:90-231`; `crates/wyrd/wyrd-server/src/http/openapi.rs:74-116`; generated Bifrost operations in `openapi.yaml:12-171,722-896`; runtime mapper at `crates/wyrd/wyrd-server/src/http/error.rs:57-66,133-171`.
- **Caller and reachability trace:** the three table handlers and four query/lifecycle handlers all return or directly call the shared `WyrdErrorResponse`/`wyrd_error_response` path. That full mapper serializes `WyrdError::as_problem_json()` and unconditionally sets `Content-Type: application/problem+json`. Every new Utoipa refusal annotation supplies `body = WyrdProblem` without a response `content_type`, so generated OpenAPI publishes `application/json`; the focused OpenAPI test explicitly indexes that wrong key for all seven operations. These are public authenticated, authorization, validation, conflict, not-found, and availability paths, not dormant responses.
- **Observable consequence:** the generated contract advertises a different representation from the live server, so a strict generated client or agent can treat a catalog-backed refusal as an unexpected content type instead of its typed `WyrdProblem`.
- **Decision-complete minimum correction:** retain the existing response statuses and `WyrdProblem` schema, and declare `content_type = "application/problem+json"` on every non-2xx and default response for the seven Bifrost operations. Change the existing focused test to require that media-type key and the same schema reference. Do not add a second response wrapper, enumerate the full catalog per route, or change query terminal-frame behavior.
- **Focused closure proof:** run the exact OpenAPI unit test after making it iterate all seven operations under `application/problem+json`, regenerate the source-owned artifact, and run `mise run codegen:check` plus the server's narrow format/lint checks.

### FIND-TASK-002-7 — CONFIRMED — VIOLATION: the cumulative relocation still contains undocumented Rust items

- **Wave 1 source:** `REPO-R1-001`.
- **Violated obligation:** AGENTS.md section 16 and `architecture/agent-rules.md` require substantive rustdoc for every new or materially modified Rust item, regardless of visibility, including implementation methods, test helpers, and tests; every fallible operation requires `# Errors`. Missing documentation is explicitly a hard blocker.
- **Exact location:** direct remaining examples include `crates/shared/wyrd-client/src/storage/upload/mod.rs:49-64,90-115`; `crates/shared/wyrd-client/src/storage/upload/tests.rs:16-107`; `crates/shared/wyrd-client/src/cards/saga/hash_artifacts.rs:114-219`; `crates/shared/wyrd-client/src/cards/saga/upload.rs:160-200`; `crates/shared/wyrd-client/src/cards/saga/complete.rs:132-234`; `crates/shared/wyrd-client/src/cards/mod.rs:35-56`; `crates/shared/wyrd-client/src/storage/error.rs:116-139`; and `sdks/wyrd-sdk-python/src/state/mod.rs:2977-2983`.
- **Caller and reachability trace:** `ArtifactSource::size_hint` and `into_stream` are called by the complete `dispatch` body at `storage/upload/mod.rs:178-206` for the live `Vec<u8>`, `Bytes`, and `FileSource` implementations; `FileSource::open` is called by the registration upload workflow at `cards/saga/upload.rs:120`; `UploadOutcome::into_server_complete` is called by `WyrdStorageClient` at `storage/mod.rs:124`; `from_authenticated` is the shared storage boundary conversion; and `loader_manifest_error` is called by the live Python state loader at `sdks/wyrd-sdk-python/src/state/mod.rs:2502`. The cited test modules, helpers, and tests are compiled test items and are expressly covered by the rule. Git rename analysis shows the storage upload, Cards saga, Cards module, and Python state code was materially relocated into the new owners in this cumulative task, so unchanged bodies do not exempt them.
- **Observable consequence:** the candidate remains in direct violation of a repository-defined hard completion rule. The implementation evidence's added-line diagnostic is not a closure proof because inherited trait docs and private/test items are not all rejected by ordinary `missing_docs` diagnostics.
- **Decision-complete minimum correction:** finish the documentation-only remediation already prescribed by prior finding 7: add substantive rustdoc to every still-undocumented added or materially relocated Rust item in the cumulative touched modules, including the cited implementation methods, test modules/helpers/tests, and fallible `FileSource::open`; add `# Errors`, `# Panics`, and cancellation/partial-progress sections only where the actual body warrants them. Do not move code, extract helpers, add a lint suppression, or create another permanent check.
- **Focused closure proof:** enumerate the added/materially relocated Rust items from the base-to-candidate diff and review each item rather than only added lines; then run `mise run fmt`, `mise run lints`, and the narrow Rustdoc/build checks covering `wyrd-client` and `wyrd-sdk-python`.

### FIND-TASK-002-8 — CONFIRMED — VIOLATION: newly owned signatures still use qualified type paths

- **Wave 1 source:** `REPO-R1-002`.
- **Violated obligation:** `architecture/agent-rules.md` requires dependencies in the module-top `use` block and bare names in fields, parameters, return types, bounds, and `where` clauses.
- **Exact location:** `crates/shared/wyrd-client/src/cards/handle.rs:215-218,242-245,262-266,288-291`; `crates/shared/wyrd-client/src/storage/upload/mod.rs:141-153`.
- **Caller and reachability trace:** the full `Cards::register_from_path`, `register`, and `register_with_progress` bodies route public Card registration into the one shared saga, while `get_response` routes public selection into the shared read owner; their return types use `crate::cards::RegistrationReceipt` or `wyrd_spec::registry::GetCardResponse`. `UploadOutcome::NeedsServerComplete` is constructed by provider upload paths, consumed by the complete `into_server_complete` match, and then used by `WyrdStorageClient`; both the field and return type spell `wyrd_spec::storage::UploadCompleteRequest`. These are live newly owned public/internal signatures, not qualified paths inside expressions.
- **Observable consequence:** the candidate fails the explicit dependency-manifest style rule at the precise signature/field sites it governs, despite formatter and Clippy success.
- **Decision-complete minimum correction:** add `RegistrationReceipt` and `GetCardResponse` to the existing `wyrd_spec::registry` import in `cards/handle.rs`, add `UploadCompleteRequest` to the existing storage import in `storage/upload/mod.rs`, and replace only the cited field/return paths with bare names. No aliases or new module are needed because no collision exists.
- **Focused closure proof:** source-search the cumulative touched Rust signatures and fields for remaining qualified paths, then run `mise run fmt` and `mise run lints`.

### FIND-TASK-002-14 — CONFIRMED — REGRESSION: the canonical reading guide names deleted sibling clients

- **Wave 1 source:** `SDK-CONTRACT-R1-03`.
- **Violated obligation:** REQ-017, REQ-020, REQ-021, INV-005, INV-010, AC-002, AC-004, and TASK-002's explicit prohibition on stale parallel-client authority require public language guidance to use the single `Bifrost` facade.
- **Exact location:** `docs/src/content/docs/bifrost/reading-data.svx:80-133`; linked from the public Bifrost index at `docs/src/content/docs/bifrost/index.svx:14-17,124-127` and from `docs/src/content/docs/bifrost/forge.svx:395`.
- **Caller and reachability trace:** the page's complete Python example imports and constructs `BifrostQueryClient` and calls `.query`; the complete TypeScript example does the same. Candidate exports instead provide Python `AsyncBifrost(...).stream(...)` at `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py:578-696` and TypeScript `Bifrost.connect(...).stream(...)` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:654-835`. Repository search finds no SDK export named `BifrostQueryClient`. Although the page text existed at the base, the cumulative task removed the sibling clients, explicitly assumed responsibility for converged SDK documentation, and edited the linked Bifrost docs around it; leaving the now-invalid canonical example is a task-caused semantic regression. Prior finding 4 concerned five `CodeFromFile` paths on two different how-to pages and is closed; this is a distinct defect.
- **Observable consequence:** copying the canonical Python snippet fails at import, copying the TypeScript snippet fails export/type checking, and both examples teach the prohibited sibling-client model.
- **Decision-complete minimum correction:** rewrite only these two snippets using the existing public facades and signatures: Python uses `AsyncBifrost(server_url=..., credential=...)` and `await client.stream(query, visibility=..., freshness=...)`; TypeScript uses `await Bifrost.connect({ serverUrl, credential })` and `await client.stream({ sql, visibility, freshness })`. Preserve the existing terminal-validation example and add no compatibility alias or documentation-only wrapper.
- **Focused closure proof:** source-search public docs for `BifrostQueryClient`, run `mise run docs:check`, and compile/type-check the two snippet call shapes through the existing Python and TypeScript SDK typing/test mechanisms; the current language Oracle journeys remain the runtime proof for those same calls.

## Prior-finding closure

| Stable finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-002-1` | REOPENED | Catalog identity is centralized, but safe structured `Config` and `TransportDown` metadata is discarded by that owner. |
| `FIND-TASK-002-2` | CLOSED | Native TypeScript connect/describe results feed the existing runtime `WyrdError` projector; no Wave 1 finding disputes it. |
| `FIND-TASK-002-3` | REOPENED | All seven operations name `WyrdProblem`, but generated media types disagree with their runtime representation. |
| `FIND-TASK-002-4` | CLOSED | The five relocated example embeds and shared `sdks` resolver path are corrected; finding 14 concerns another public guide and another defect. |
| `FIND-TASK-002-5` | CLOSED | `wyrd.errors.WyrdError` is the root class without obsolete aliases. |
| `FIND-TASK-002-6` | CLOSED | The optional Python feature gates PyO3 and retained owner Python features. |
| `FIND-TASK-002-7` | REOPENED | Required documentation remains absent from added/materially relocated items that the previous closure scan did not cover. |
| `FIND-TASK-002-8` | REOPENED | The previously cited paths were corrected, but other newly owned field/signature sites in the same cumulative range still violate the same rule. |
| `FIND-TASK-002-9` | CLOSED | `Bifrost` owns public query/lifecycle methods and sibling query types/accessors are private. |
| `FIND-TASK-002-10` | CLOSED | Shutdown and producer admission share the producer-map synchronization boundary. |
| `FIND-TASK-002-11` | CLOSED | Failed terminals use the same bounded clean-EOF validation as other terminals. |
| `FIND-TASK-002-12` | CLOSED | Pending stream state retains encoded frames and decodes one delivery at a time. |
| `FIND-TASK-002-13` | CLOSED | Shared validation and generated projections enforce `1..=u32::MAX`. |

## Verification limits

- Source, caller, authority, generated-artifact, and cumulative-diff inspection independently established all five retained findings. No implementation or generated file was changed.
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..4d9d74b34803b3f27d55f740a5b40ffbe968b306` passed.
- Cargo, Python, TypeScript, codegen, and docs lanes were not rerun in Wave 2. Their recorded success cannot close source-proven metadata/media-type/documentation/signature gaps, and `docs:check` does not compile fenced SDK snippets.
- The recorded one-time Python multipart-completion failure followed by a 55/55 rerun remains a verification limit, not a retained finding: the affected persistent-storage path is outside the remediation delta and no reproducible candidate cause was established.
- Per TASK-002, the prohibited Bifrost aggregate was not run and is not required to close these bounded findings.

## Recommendation

**FIX_REQUIRED**

The deduplicated ledger contains five bounded implementation findings: reopened `FIND-TASK-002-1`, `FIND-TASK-002-3`, `FIND-TASK-002-7`, and `FIND-TASK-002-8`, plus new `FIND-TASK-002-14`. Every correction is fixed by approved authority and an existing owner, projector, annotation mechanism, import block, or SDK facade. None requires a new product, public API, architecture, security, compatibility, cross-service, concurrency, resource-ownership, or persistent-data decision.
