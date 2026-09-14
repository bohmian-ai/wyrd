---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-017, REQ-020, REQ-021, REQ-023, REQ-024, REQ-024A, REQ-025, REQ-059, INV-005, INV-010, AC-002, AC-004]
depends_on: [TASK-002-R1]
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-3, FIND-TASK-002-7, FIND-TASK-002-8, FIND-TASK-002-14]
---

# Close TASK-002-R1 review findings

## Authority and candidate

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/TASK-002-R1-close-review-findings.md`
- Review: `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Implementation route: `$wyrd-implement`

## Outcome

Complete the existing convergence: preserve actionable shared-client error
metadata, make generated Bifrost refusal media types match runtime, finish the
repository-required documentation/import cleanup, and make the canonical
reading guide use the public Python and TypeScript `Bifrost` facades. Do not
change the approved architecture or add compatibility surfaces.

## Issue diagnoses and required corrections

### `FIND-TASK-002-1` — shared client errors lose structured details

`crates/shared/wyrd-client/src/error.rs:89-110` now owns the canonical
`WyrdClientError` projection, but it constructs `{}` for every variant even
though `Config` carries safe `field` and `reason` values and `TransportDown`
carries a safe `transport` discriminator. Cards, Bifrost scope/query, HTTP token
exchange, Python, and TypeScript all reuse this owner. As a result,
`WYRD_CLIENT_400_CONFIG_INVALID` tells callers to use a field in `details` that
is absent, forcing display-text parsing.

Keep the existing conversion owner and populate only the existing safe variant
metadata: `field` and `reason` for configuration, `transport` for transport
unavailability, and an empty object for missing credentials. Preserve messages,
codes, statuses, server-originated errors, and redaction; do not expose raw
transport failure text or add another projector.

### `FIND-TASK-002-3` — generated refusal media types disagree with runtime

The seven Bifrost table/query/lifecycle route annotations identify
`WyrdProblem`, but omission of a response content type causes `openapi.yaml` to
publish `application/json`. The one runtime mapper in
`crates/wyrd/wyrd-server/src/http/error.rs` always emits
`application/problem+json`. Strict generated clients can therefore reject a
live typed refusal as an unexpected representation.

Keep the existing statuses, default responses, schemas, and runtime mapper.
Declare `application/problem+json` on every non-2xx/default response for the
seven operations, update the existing OpenAPI proof to require that media type
and schema reference, and regenerate through the repository generator. Do not
add a response wrapper, enumerate the full catalog per route, or change query
terminal frames.

### `FIND-TASK-002-7` — materially relocated Rust items remain undocumented

The R1 added-line scan missed live trait implementations, private conversions,
test modules/helpers/tests, and fallible methods in materially relocated code.
Validated examples remain in:

- `crates/shared/wyrd-client/src/storage/upload/{mod.rs,tests.rs}`;
- `crates/shared/wyrd-client/src/cards/saga/{hash_artifacts.rs,upload.rs,complete.rs}`;
- `crates/shared/wyrd-client/src/cards/mod.rs`;
- `crates/shared/wyrd-client/src/storage/error.rs`; and
- `sdks/wyrd-sdk-python/src/state/mod.rs`.

This violates the hard documentation rule even though compiler and lint lanes
pass. Finish the documentation-only correction across every undocumented added
or materially relocated Rust item in the cumulative touched modules. Document
intent, workflow role, invariants, and side effects; include `# Errors`,
`# Panics`, and cancellation/partial-progress behavior only where the actual
operation warrants them. Do not move code, extract helpers, suppress a lint, or
add a permanent check.

### `FIND-TASK-002-8` — qualified paths remain in fields and signatures

The newly owned Cards methods at
`crates/shared/wyrd-client/src/cards/handle.rs:215-291` still return qualified
`RegistrationReceipt` and `GetCardResponse` paths. The upload outcome field and
conversion at `crates/shared/wyrd-client/src/storage/upload/mod.rs:141-153`
still use a qualified `UploadCompleteRequest`. These live sites violate the
module-top import rule despite format and lint success.

Add those three types to the existing module-top imports and use bare names at
the validated field/signature sites. There is no collision, so add no alias,
helper, or module.

### `FIND-TASK-002-14` — canonical reading examples use removed clients

`docs/src/content/docs/bifrost/reading-data.svx:80-133`, linked from the public
Bifrost index and Forge guide, imports and constructs `BifrostQueryClient` in
Python and TypeScript. Neither SDK exports that class after convergence, so the
examples fail while teaching the prohibited sibling-client model. This is
separate from the five file-embed paths corrected by R1.

Rewrite only those two snippets against the existing public APIs: Python uses
`AsyncBifrost(server_url=..., credential=...)` and awaits `.stream(...)`;
TypeScript uses `await Bifrost.connect({ serverUrl, credential })` and then
`.stream(...)`. Preserve the existing terminal-validation explanation. Add no
alias, wrapper, client type, or second documentation loader.

## Constraints and preserved behavior

- Keep `wyrd-client` as the sole SDK-facing Rust owner and `Bifrost` as the
  sole public query/write/lifecycle facade.
- Preserve server-owned durable behavior, tenant isolation, authorization and
  audit ordering, credentials/redaction, transport, query framing, settlement,
  bounded Arrow ownership, backpressure, and shutdown/drain behavior.
- Keep one derive-backed catalog and one runtime `WyrdError` projection per
  language.
- Keep PyO3 optional and owned by `wyrd-sdk-python`; keep generated artifacts
  source-owned.
- Do not weaken, ignore, allowlist around, or delete a gate or test.
- Do not hand-edit generated OpenAPI, schemas, stubs, or declarations.

## Non-goals

- No new abstraction, error projector, response wrapper, client alias,
  compatibility path, docs loader, feature, dependency, or permanent check.
- No changes to query terminal frames, stream ownership, lifecycle concurrency,
  persistent state, server authz/audit behavior, or SDK architecture.
- No cleanup outside the validated cumulative touched modules and two stale
  documentation snippets.
- No implementation, merge, push, release, deployment, or Bifrost aggregate.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| Configuration and transport client errors preserve the exact safe detail keys through the canonical Rust owner and one existing public foreign-runtime projection. | `FIND-TASK-002-1` |
| All seven generated Bifrost operations publish every non-success/default `WyrdProblem` response as `application/problem+json`, matching runtime. | `FIND-TASK-002-3` |
| Every added or materially relocated Rust item in the cumulative touched modules satisfies the repository rustdoc rule, including fallibility and async/durable behavior where relevant. | `FIND-TASK-002-7` |
| The validated Cards and upload field/signature sites use module-top imports and bare type names, with no remaining cumulative touched-site violation. | `FIND-TASK-002-8` |
| The canonical reading guide contains compiling Python and TypeScript examples using only the existing public `Bifrost` facades, and public docs contain no `BifrostQueryClient`. | `FIND-TASK-002-14` |

## Required proof

Extend and run the existing focused shared-client error test:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=cards::error::tests::cards_client_failures_keep_client_catalog_identity)'
```

Assert the same safe detail projection through the nearest existing public
Python or TypeScript error test. If using the existing TypeScript connect-error
target, run:

```bash
cd sdks/wyrd-sdk-ts/wyrd
pnpm exec vitest run tests/unit/connect-errors.test.ts
```

Update and run the existing seven-operation OpenAPI proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=http::openapi::tests::bifrost_operations_publish_typed_problem_refusals)'
```

Directly enumerate all added or materially relocated Rust items in the
cumulative base-to-new-candidate range and verify their rustdoc; source-search
the same touched fields/signatures for qualified type paths. Source-search
public docs for `BifrostQueryClient`, run the documentation build, and exercise
the two documented call shapes through the existing Python and TypeScript
typing/test mechanisms.

Run the narrow broader lanes covering the touched surfaces:

- `mise run fmt`
- `mise run lints`
- `mise run py:format`
- `mise run py:lints`
- `mise run py:test:unit`
- `mise run py:typecheck`
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run ts:test:unit`
- `mise run codegen:check`
- `mise run docs:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `git diff --check`

Re-run the nearest existing shared-client and language query/error journeys
that exercise the corrected public surfaces. Do not run the prohibited Bifrost
aggregate, and do not substitute aggregate success for either named focused
proof.

## Implementation evidence

Commits on `change/surfaces-oracle-integration` (cumulative candidate is the
last one):

- `305762044` fix(client): keep safe detail keys on client-local errors and use module-top imports
- `c73bb414e` fix(server): publish Bifrost refusals as problem+json and document the public Bifrost facades
- `c0a12085c` docs(client): document relocated shared-client items and test helpers
- `2dc538f81` test(sdk): assert client error detail keys through the Python and TypeScript facades

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Config/transport client errors preserve safe detail keys through the canonical Rust owner and one public foreign-runtime projection | `crates/shared/wyrd-client/src/error.rs` `From<&WyrdClientError> for WyrdError` emits `{field, reason}`, `{transport}`, `{}` | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=cards::error::tests::cards_client_failures_keep_client_catalog_identity)'` (1 passed); `pnpm exec vitest run tests/unit/connect-errors.test.ts` (4 passed, transport detail asserted); `uv run python -m pytest tests/unit/cards/test_registry_surface.py -k "client_catalog_error or config_invalid"` (2 passed, config + empty details asserted) | PASS |
| All seven Bifrost operations publish every non-success/default `WyrdProblem` response as `application/problem+json` | `crates/wyrd/wyrd-server/src/bifrost/routes.rs`, `crates/wyrd/wyrd-server/src/query/routes.rs` (`content_type = "application/problem+json"` on 40 responses); `openapi.yaml` regenerated via `mise run codegen:openapi` | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::openapi::tests::bifrost_operations_publish_typed_problem_refusals)'` (1 passed; asserts problem+json ref and absence of `application/json`); `mise run codegen:check` pass | PASS |
| Every added or materially relocated Rust item in the cumulative touched modules has rustdoc | Docs added across `crates/shared/wyrd-client` (src + tests), `vala-bifrost-redux/src/catalog/tenant_table.rs`, `wyrd-cli/src/card.rs`, `wyrd-sdk-python/src/state/mod.rs`, `wyrd-sdk-rust/src/lib.rs` | Scripted scan of every added line in `861f8d86..HEAD` for undocumented `fn/struct/enum/trait/mod/const/type/static` items: 0 remaining (one false positive on a multi-line-attribute struct that is documented); `mise run lints` pass | PASS |
| Cards and upload field/signature sites use module-top imports and bare type names | `cards/handle.rs` imports `RegistrationReceipt`, `GetCardResponse`; `storage/upload/mod.rs` imports `UploadCompleteRequest` | `grep` for `crate::cards::RegistrationReceipt`, `wyrd_spec::registry::GetCardResponse`, `wyrd_spec::storage::UploadCompleteRequest` in those files: 0 hits; `mise run lints` pass | PASS |
| Canonical reading guide uses only the public `Bifrost` facades; public docs contain no `BifrostQueryClient` | `docs/src/content/docs/bifrost/reading-data.svx` Python uses `AsyncBifrost(server_url=, credential=)` + `await .stream(...)`; TypeScript uses `await Bifrost.connect({ serverUrl, credential })` + `.stream(...)` | `grep -rn BifrostQueryClient docs/src` → 0; `mise run docs:check` pass; both call shapes type-checked as scratch files via `uv run ty check` and `pnpm exec tsc --noEmit` (no errors, files removed) | PASS |

Broader lanes: `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run py:test:unit` (464 passed), `mise run py:typecheck`,
`mise run ts:build`, `mise run ts:typecheck`, `mise run ts:test:unit`,
`mise run codegen:check`, `mise run docs:check`, `mise run check:client-tier`,
`mise run check:pyo3-scope`, `git diff --check`, plus
`cargo nextest run -p wyrd-client --lib --test cards_transport --test storage_dispatch`
(195 passed) — all pass.

Notes and limits:

- The TypeScript `Bifrost.connect`/`TableConfig.describe` paths build through
  `client_from_options`, which does not run `HttpConfig::validate`, so an empty
  `serverUrl` there surfaces as transport-down, not config-invalid. The
  config-invalid detail projection is therefore proven through the Python
  `Cards(server_url="")` path (which validates) and the transport detail through
  TypeScript. Changing that validation is outside this remediation.
- Non-goals held: no new abstraction, projector, wrapper, alias, loader, feature,
  dependency, or check; no query-frame, lifecycle, authz, or persistence change.
- Pre-existing uncommitted forge/vala-sql working-tree edits in this worktree
  were left untouched and are not part of the candidate.
