# Public SDK and Contract Domain Review

## Reviewed boundary

- Repository subject: `/tmp/wyrd-task-002-r1-review-4d9d74b34`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior verdict and remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/{verdict.md,findings-validation.md,TASK-002-R1-close-review-findings.md}`
- Scope: the shared Rust facade; HTTP/OpenAPI/MCP/gRPC query contracts; stable error projection; Rust, Python/PyO3, and TypeScript/N-API SDK surfaces; generated artifacts; public documentation; and their boundary tests.

The candidate remained `4d9d74b34803b3f27d55f740a5b40ffbe968b306` throughout this review. Review artifacts in the main worktree are outside the immutable subject.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer coverage |
|---|---|---|
| Shared client and language projections | `AGENTS.md` §§2–9; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client model and Bifrost public surface; `architecture/wyrd-doctrine.mdx`; specification REQ-016–REQ-021, INV-004–INV-006, INV-013, INV-024 | `crates/shared/wyrd-client/src/{error,bifrost,cards,state,storage}`, the Rust SDK re-export, Python aggregator/package/stubs, TypeScript N-API and public wrapper, CLI and test consumers. `QueryClient` and `RawQueryStream` are private module mechanics and production callers now enter lifecycle operations through `Bifrost`. |
| Stable errors | `AGENTS.md` §§4, 7–9; `architecture/references/languages/{errors,pyo3-boundaries,python-api-and-stubs,typescript-guide}.md`; specification REQ-022–REQ-025, AC-002 | `WyrdClientError` and its canonical `WyrdError` conversion, Cards and Bifrost callers, Python `WyrdError`/`wyrd.errors`, TypeScript `NativeWyrdError` and runtime `WyrdError`, focused error tests. |
| Query wire and agent contracts | `architecture/bifrost-design.md` public surface and terminal contract; `architecture/references/languages/agent-harness.md`; specification REQ-019, REQ-056–REQ-059, AC-002, AC-004, AC-019 | `BifrostQueryRequest`, JSON Schema, OpenAPI, protobuf conversions, server HTTP routes, MCP schema/conversion, Python signed input, TypeScript numeric input, and deadline tests. |
| Generated and documented public surface | `AGENTS.md` §§8–12; `architecture/references/languages/{testing-workflows,python-api-and-stubs,typescript-guide,agent-harness}.md`; specification REQ-017–REQ-025, REQ-059, INV-005, INV-010, AC-002–AC-004 | `openapi.yaml`, generated schemas and declarations, docs Bifrost pages, examples, SDK export inventories, and recorded codegen/docs/language verification. |

## Prior-finding closure

| Prior findings in this domain | Result | Evidence |
|---|---|---|
| `FIND-TASK-002-1` | Partial | Cards and Bifrost now share one `WyrdClientError` to `WyrdError` conversion and preserve code/status, but `SDK-CONTRACT-R1-01` shows that conversion discards the documented structured fields. |
| `FIND-TASK-002-2` | PASS | Cards/Bifrost connect and table description return native closed results that the TypeScript wrapper converts through `projectedError`; focused tests cover missing credentials and transport refusal. |
| `FIND-TASK-002-3` | Partial | All seven operations now reference `WyrdProblem`, but `SDK-CONTRACT-R1-02` shows their generated media type disagrees with the runtime problem response. |
| `FIND-TASK-002-4` | PASS for the named defect | The relocated example resolver and the two previously broken pages were corrected. `SDK-CONTRACT-R1-03` is a separate published Bifrost page that still presents deleted clients. |
| `FIND-TASK-002-5`, `FIND-TASK-002-6` | PASS | `wyrd.errors.WyrdError` is the root class, and PyO3 plus owner Python features are behind the Python SDK's explicit feature; Rust and TypeScript cones remain outside it. |
| `FIND-TASK-002-7`, `FIND-TASK-002-8` | PASS in this boundary | The materially relocated SDK items inspected here have intent/error documentation and module-top imports with bare signature names. |
| `FIND-TASK-002-9` | PASS | Public exports no longer name `QueryClient` or `RawQueryStream`; Rust, CLI, Python, and TypeScript callers use `Bifrost` methods. The test-support-only sealed-batch transport remains the explicit harness seam. |
| `FIND-TASK-002-13` | PASS | The shared request validates `1..=u32::MAX`; schemas publish the bounds; gRPC, MCP, Python, and TypeScript conversions preserve or structurally reject that domain with recorded focused proof. |

## Material proposed findings

### SDK-CONTRACT-R1-01 — INCORRECT: the canonical client-error conversion drops its actionable structured fields

- **Violated obligation:** specification REQ-023, REQ-024, REQ-024A, REQ-025, AC-002, and `architecture/references/languages/errors.md` require safe structured metadata to survive Rust, Python, and TypeScript projection. The `ClientConfigInvalid` catalog remediation explicitly tells callers to use the field named in `details`.
- **Exact location:** `crates/shared/wyrd-client/src/error.rs:89-103`; originating field contracts at `crates/shared/wyrd-client/src/error.rs:25-53`; catalog remediation at `crates/wyrd-spec/src/error.rs:3182-3194`.
- **Evidence:** the new shared conversion computes `let details = serde_json::json!({})` for every variant. It ignores `Config { field, reason }` even though those fields are documented as being echoed into the catalog payload, and ignores the safe `transport` discriminator on `TransportDown`. Cards and Bifrost now correctly reuse this one conversion, so the loss reaches all of their Rust, Python, and TypeScript public error projections. The added Cards and TypeScript tests assert code/status/title/remediation but never the documented details.
- **Observable consequence:** a caller receiving `WYRD_CLIENT_400_CONFIG_INVALID` is instructed to correct the field named in `details`, but receives `{}` and must parse display text. The converged language errors therefore do not retain the same actionable structured contract promised by the catalog.
- **Required testable correction:** keep the new single conversion owner, but project the existing safe variant fields into `details` (`field` and `reason` for configuration; the transport discriminator for transport failure) rather than discarding them. Extend its focused Rust test and one public foreign-runtime projection test to assert the structured fields without parsing `message`.

### SDK-CONTRACT-R1-02 — INCORRECT: OpenAPI advertises JSON while the Bifrost refusal runtime returns problem+json

- **Violated obligation:** specification REQ-023, REQ-059 and AC-002 require the generated HTTP authority and structured errors to agree; `architecture/references/languages/errors.md` defines public HTTP failures as problem payloads; RFC 9457 responses are emitted by the server's single mapper.
- **Exact location:** Bifrost annotations at `crates/wyrd/wyrd-server/src/bifrost/routes.rs:24-104` and `crates/wyrd/wyrd-server/src/query/routes.rs:90-231`; generated examples at `openapi.yaml:31-55`, `openapi.yaml:80-116`, and the remaining Bifrost non-2xx responses; runtime mapper at `crates/wyrd/wyrd-server/src/http/error.rs:57-66,164-170`; focused assertion at `crates/wyrd/wyrd-server/src/http/openapi.rs:74-116`.
- **Evidence:** each new refusal annotation names `body = WyrdProblem` without declaring its response media type, so Utoipa generates `content.application/json`. `WyrdErrorResponse` always emits `Content-Type: application/problem+json`. The new OpenAPI test explicitly indexes `application/json`, pinning the disagreement instead of detecting it.
- **Observable consequence:** clients and agents generated from OpenAPI may select an `application/json` decoder branch while the live server responds with `application/problem+json`, degrading a typed Wyrd refusal into an unexpected-content-type or generic transport error.
- **Required testable correction:** retain the existing `WyrdProblem` response set but declare `application/problem+json` for every Bifrost non-2xx/default response. Regenerate OpenAPI and make the focused test assert the problem media type and schema reference for all seven operations.

### SDK-CONTRACT-R1-03 — REGRESSION: the public reading guide constructs two removed sibling query clients

- **Violated obligation:** specification REQ-017, REQ-020, REQ-021, INV-005, INV-010, AC-002 and AC-004 require one `Bifrost` facade per language and prohibit stale parallel-client authority in documentation.
- **Exact location:** `docs/src/content/docs/bifrost/reading-data.svx:80-133`; public navigation at `docs/src/content/docs/bifrost/index.svx:14-17,124-127`.
- **Evidence:** the Python example imports and constructs `BifrostQueryClient`; the TypeScript example does the same. Neither SDK exports that class: Python exports `Bifrost` and `AsyncBifrost`, and TypeScript exports `Bifrost` with `Bifrost.connect` and `stream`. The page is linked as the canonical reading guide. `docs:check` can prerender fenced snippets without resolving their imports, so its recorded pass does not validate these examples.
- **Observable consequence:** users following the canonical Python example get an import failure and TypeScript users get a missing-export/type error; the docs also reintroduce the exact sibling-client model the task removed.
- **Required testable correction:** rewrite both snippets against the already-installed public `Bifrost` facades and their current connection/stream signatures, without adding an alias. Run `docs:check`, prove the retired identifier is absent from public docs, and rely on the existing Python and TypeScript Oracle journeys for the referenced public calls.

## Verification limits

- Reviewed the complete cumulative base-to-candidate diff, the prior review/remediation packet, the candidate source and generated artifacts, and the recorded focused and lane evidence.
- This reviewer did not rerun Cargo, Python, TypeScript, codegen, or docs commands. The remediation records those lanes as passing; the original task intentionally excludes the Bifrost aggregate.
- Recorded codegen proves source/artifact equality, not semantic agreement with the runtime media type. Recorded docs build proves page rendering, not that fenced SDK imports exist.
- No live-cloud verification is relevant to these three findings.

## Overall result

**FAIL**

The remediation establishes the intended single-facade ownership, Python feature boundary, root exception classes, TypeScript connection projection, and common deadline domain. The three reachable public-contract findings above prevent this domain from satisfying TASK-002 exactly.
