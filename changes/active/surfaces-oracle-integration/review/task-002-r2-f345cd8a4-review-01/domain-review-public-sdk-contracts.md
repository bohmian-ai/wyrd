# Public SDK and Contract Domain Review

## Reviewed boundary

- Immutable worktree: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior review and remediation: both TASK-002 review directories through `TASK-002-R2-close-r1-review-findings.md`
- Scope: closure of `FIND-TASK-002-1`, `FIND-TASK-002-3`, and `FIND-TASK-002-14` across the shared Rust error owner, Python and TypeScript projections, the seven Bifrost HTTP operations, runtime problem mapper, generated OpenAPI, and canonical reading documentation.

The immutable subject remained at the named candidate throughout this review. No `.codegraph/` index exists in the immutable worktree, so inspection used repository search and direct source reads.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Shared client error identity and metadata | `AGENTS.md` §§2–9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md`; specification REQ-023–REQ-025, AC-002; TASK-002-R2 `FIND-TASK-002-1` | `crates/shared/wyrd-client/src/error.rs`; complete `RegistryEngineError`, `BifrostClientError`, `ClientScope`, and HTTP auth conversion call paths; `wyrd-utils` Python exception projection; Python Cards test; TypeScript native and public projections plus connection-error test | PASS |
| Bifrost refusal wire contract | `AGENTS.md` §9; `architecture/bifrost-design.md`; `architecture/references/languages/errors.md`; specification REQ-059 and AC-002; TASK-002-R2 `FIND-TASK-002-3` | Three table operations in `bifrost/routes.rs`; four query/lifecycle operations in `query/routes.rs`; shared runtime mapper in `http/error.rs`; seven-operation OpenAPI assertion; generated `openapi.yaml` | PASS |
| First-class Python and TypeScript reading examples | `AGENTS.md` §§2, 8, 11–12; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/typescript-guide.md`; specification REQ-017, REQ-020, REQ-021, INV-005, INV-010, AC-002, AC-004; TASK-002-R2 `FIND-TASK-002-14` | `docs/src/content/docs/bifrost/reading-data.svx`; Python `AsyncBifrost` implementation/stubs; TypeScript `Bifrost.connect`, `Bifrost.stream`, and stream terminal surface; public-doc identifier search | PASS |

## Prior-finding closure

### `FIND-TASK-002-1` — CLOSED

`From<&WyrdClientError> for WyrdError` at `crates/shared/wyrd-client/src/error.rs:89-117` is still the single client-local projection. Its exhaustive variant match now emits exactly `{"field", "reason"}` for `Config`, `{"transport"}` for `TransportDown`, and `{}` for `NoCredentials`. `RegistryEngineError::Client` (`cards/error.rs:31-58`), `BifrostClientError::Client` (`bifrost/query.rs:104-146`), `ClientScope::from_config` (`bifrost/scope.rs:35-53`), and HTTP token-exchange conversion (`transport/http.rs:735-744`) all route through that owner; no parallel metadata mapper was added.

The Python exception boundary reads the derive-backed problem `details`, and `test_cards_with_an_empty_server_url_raise_config_invalid_details` asserts the public `wyrd.WyrdError` receives the exact configuration object. The TypeScript native boundary serializes the same problem `details`, `projectedError` parses it into the public `WyrdError`, and `connect-errors.test.ts` asserts `{"transport":"http"}` on a reachable transport failure. The shared Rust focused test covers all three variant shapes. Raw transport text was not added to structured details.

### `FIND-TASK-002-3` — CLOSED

All non-success and default `WyrdProblem` annotations on the three table operations and four query/lifecycle operations explicitly declare `content_type = "application/problem+json"` (`bifrost/routes.rs:25-98`, `query/routes.rs:92-226`). The shared runtime mapper still serializes `WyrdError::as_problem_json()` and sets the identical media type (`http/error.rs:57-66,72-120,164-171`). The focused OpenAPI test enumerates all seven operations, requires every common and operation-specific refusal under `application/problem+json`, requires the shared `WyrdProblem` schema reference, and rejects an `application/json` sibling (`http/openapi.rs:74-120`). The generated `openapi.yaml` agrees with those annotations.

### `FIND-TASK-002-14` — CLOSED

The canonical guide now imports Python `AsyncBifrost`, constructs it with the real `server_url` and `credential` parameters, awaits its real `stream(query, *, visibility, freshness)` method, and reads terminal metadata after iteration (`reading-data.svx:85-108`). The TypeScript example uses the real `await Bifrost.connect({ serverUrl, credential })`, calls `stream` with the public request shape, and reads the stream terminal after iteration (`reading-data.svx:110-133`). These shapes match the public implementations and declarations. `BifrostQueryClient` is absent from the public documentation tree; no compatibility alias was introduced.

## Reported TypeScript validation limit

`client_from_options` writes an explicit `server_url` into `ClientConfig.http.base_url` and delegates to `WyrdClient::with_config`, which does not call `HttpConfig::validate` (`bifrost/facade.rs:789-805`, `client.rs:69-88`). Consequently an empty `serverUrl` on TypeScript `Bifrost.connect` or `TableConfig.describe` reaches the request path and is classified as `WYRD_CLIENT_503_TRANSPORT_DOWN`, whereas Python Cards has its own existing empty-URL guard and produces `WYRD_CLIENT_400_CONFIG_INVALID`.

This does not reopen `FIND-TASK-002-1`: that finding and its decision-complete R2 correction govern preservation of metadata for client errors that are produced, not a new cross-facade rule assigning one catalog variant to empty endpoint input. The TypeScript path still returns the required catalog-backed runtime `WyrdError` with the safe transport discriminator. Requiring `client_from_options` or `WyrdClient::with_config` to invoke transport validation would change shared construction behavior beyond this bounded remediation and is not fixed by an explicit approved TASK-002 obligation. It remains a disclosed test-coverage/behavior limit, not a material finding.

## Material proposed findings

None.

## Verification limits

- This reviewer inspected the complete cumulative diff and current source but did not rerun Cargo, Python, TypeScript, codegen, or docs lanes. The implementation record reports both named Rust tests and all requested language, codegen, docs, lint, and boundary lanes green.
- The foreign-runtime proof is split along reachable public paths: Python proves configuration details through Cards, while TypeScript proves transport details through Bifrost table description. The canonical Rust owner test directly proves both detail shapes.
- `docs:check` alone does not compile fenced snippets. The implementation record says both call shapes were separately type-checked, and direct comparison against the public Python and TypeScript implementations/declarations found no signature mismatch.
- A cumulative `git diff --check 861f8d86..f345cd8a4` rerun reported blank-line-at-EOF warnings in two prior review Markdown files. That contradicts the unqualified recorded `git diff --check` pass but is outside this public SDK/contracts boundary; the orchestrator or repository-standards reviewer should disposition it.
- Per the original task, no Bifrost aggregate was required or reviewed for this bounded remediation.

## Overall result

**PASS**

The candidate closes all three assigned public SDK/contract findings without adding a projector, response wrapper, client alias, compatibility path, or parallel authority. No material public SDK/contracts finding remains.
