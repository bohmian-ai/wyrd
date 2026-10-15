# Repository Standards Review — TASK-001

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate tree: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`
- Review scope: the complete cumulative base-to-candidate diff (126 files). The worktree HEAD had the same tree as the candidate during this review.

## Authority coverage

| Changed surface | Governing authority reviewed | Coverage and result |
|---|---|---|
| Shared telemetry sampler, dual processors, request context, tenant routing | `AGENTS.md` §§2–6, 9–10, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Trace context; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/telemetry-observations.md` | Owner remains `wyrd-telemetry`; one process provider and one live-route owner are cohesive; operator/tenant paths use the OTel SDK contracts; identifiers and tenant route values are typed; the deliberate span-data clone is localized and documented. PASS except the documentation finding below is in another owner. |
| Authenticated tenant/principal binding at HTTP and gRPC edges | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` SQL/tenant rules; `architecture/wyrd-security-posture.md` Security principles and Tenant and data isolation; `architecture/references/doctrine/architecture-constraints.md` | Tenant and principal come from verified runtime principals, not caller fields. No new raw SQL pool capability or manual tenant filter entered production signatures. PASS. |
| Process Scribe outbox and bounded tenant-span staging | `AGENTS.md` §§2, 6, 9, 15; `architecture/wyrd-design.md` audit/outbox and Trace context; `architecture/bifrost-design.md` Resource and failure invariants and Telemetry; `architecture/wyrd-security-posture.md` Audit integrity and privacy; `architecture/references/domain/analytical-operations-reliability.md` | Reuses the single Scribe outbox; the 2,048 permits apply only to tenant span copies and travel with the staged item; permission audit, gateway capture, and result semantics are not widened. PASS. |
| Queued producer context, sealed retries, gRPC request shape, Gate links | `AGENTS.md` §§2–6, 9–10, 15–16; `architecture/agent-rules.md` feature/dependency rules; `architecture/bifrost-design.md` Table and row identity, Resource and failure invariants, Telemetry; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md` | Existing `wyrd-queue` sealing/retry ownership is extended; context count is fixed at 32; the sealed vector is stable across retry; Gate validates typed IDs and ignores malformed/excess link metadata without changing durable row admission. The `wyrd-tonic` telemetry edge is limited to its existing `server` feature, and the recorded `check:deps` result is green. PASS. |
| Deferred verifier cause persistence and SQL | `AGENTS.md` §§4, 9, 15–16; `architecture/agent-rules.md` `TenantConn` rules; `architecture/wyrd-security-posture.md` Tenant and data isolation; `architecture/references/languages/rust-core.md` | The existing unshipped migration is edited rather than adding migration/compatibility machinery. Query methods retain `&mut TenantConn<'_>` and caller-owned transaction lifecycle; no raw pool or manual tenant predicate is introduced. FAIL only for `REPO-001` documentation placement. |
| Shared Rust Bifrost timeline and row projection | `AGENTS.md` §§2–6, 9–10, 15–16; `architecture/wyrd-design.md` Client model and Trace context; `architecture/bifrost-design.md` Public surface; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | `wyrd_client::Bifrost` remains the one client owner; SQL uses typed bind parameters; decoding reuses canonical `SpanRecord`; failures map through the existing client error. No server owner leaks into client tiers. PASS. |
| Python/PyO3 projection and generated Python types/stubs | `AGENTS.md` §§7–8, 11, 16; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/testing-workflows.md` | New PyO3 code is inside `wyrd-sdk-python`; Python objects are converted before the GIL is released; no `Bound` crosses an await; sync and async public exports use generated `SpanRecord`; tests are top-level functions. Generator source changed with generated outputs, and recorded `codegen:check`, Python format/lint/typecheck lanes are green. PASS. |
| TypeScript/N-API projection, declarations, optional OTel integration | `AGENTS.md` §§2, 8–9, 11, 16; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md` | Durable work remains in `wyrd-client`; the N-API code only bridges Node context and promises; `SpanRecord` is generated and exported consistently; `.d.ts`/`.d.cts` and package sources agree. Recorded build, N-API, format and typecheck lanes are green. PASS. |
| Rust first-class SDK export | `AGENTS.md` §§2–4, 9, 11, 16; `architecture/wyrd-design.md` Client model; `architecture/references/languages/rust-core.md` | Thin re-export of `TraceId` and `SpanRecord`; no duplicate implementation. PASS. |
| MCP and first-class user journeys | `AGENTS.md` §11; `architecture/agent-rules.md` test placement and journey rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md` | New external tests earn their placement by running real SDK/server/Postgres or MCP surfaces. Rust, Python, TypeScript, MCP, server, gateway, drift, SQL, shared, and Vala lanes are recorded green. Added `#[ignore]` markers are the repository's gated journey mechanism and are run with `--run-ignored=all`; they do not disable proof. PASS. |
| Schemas, generated declarations and docs | `AGENTS.md` §§8, 11, 16; `architecture/agent-rules.md` generated-artifact rule; `architecture/references/languages/agent-harness.md` | `scripts/gen_card_types.py` and schema generation source changed alongside outputs; recorded `codegen:check` is green. Architecture authorities were updated with the approved linked-producer rule. Rust item documentation has one hard defect: `REPO-001`. |

## Applicable rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable/server behavior stays Rust-owned; SDKs project `wyrd-client` | Timeline implementation is in `crates/shared/wyrd-client/src/bifrost`; Python and TypeScript wrap it; server ingest stays in Gate/Scribe owners. | PASS |
| `wyrd-spec` remains foundational, IO-/async-/PyO3-free | The trace context contract change remains pure serialization/validation; no runtime dependency entered `wyrd-spec`. | PASS |
| Client tiers do not gain SQL/cloud/DataFusion dependencies | Manifest diff and recorded `check:deps` evidence show no prohibited client dependency. | PASS |
| Cargo features are earned and narrow | `wyrd-tonic` adds `wyrd-telemetry` only to the existing `server` feature that owns the listener request span. | PASS |
| Rust uses cohesive concrete owners and avoids speculative abstractions | `ServerSampler`, `TenantRoutes`, `TenantExport`, `ServerSpanSink`, existing queue owners, and `Bifrost` each retain the state/invariant they own; no single-implementation trait was added except the intentional sink boundary between shared telemetry and server outbox. | PASS |
| Async is limited to IO/composition; foreign runtimes use shared runtime boundaries | Timeline awaits query IO; Gate/server functions await IO; pure parsing/filtering remains synchronous; Python uses the shared runtime with the GIL detached. | PASS |
| Tenant identity comes only from verified authority; SQL uses `TenantConn`/`OperatorPool` | Route binding uses verified principals. Changed verifier SQL retains `TenantConn`; no production raw `PgPool` signature was introduced. | PASS |
| One Scribe write path; audit remains non-blocking and unchanged | Tenant spans become another typed `ScribeWrite` on the existing process outbox; audit/capture/result paths are preserved. | PASS |
| Bounded resources and retry identity | Tenant spans have 2,048 permits; queued producer contexts have a 32-entry cap sealed before retry; existing batch identity is reused. | PASS |
| Typed public contracts and stable errors | Timeline takes `TraceId`, returns canonical `SpanRecord`, binds SQL parameters, and uses existing `BifrostClientError`/`WyrdError` projections. | PASS |
| PyO3 code stays in Python SDK and does not hold Python borrows across awaits | `sdks/wyrd-sdk-python/src/trace.rs` reads Python context before `detach`; no `Bound` is retained across the Rust future. | PASS |
| Generated artifacts are produced from source and remain in parity | Generator and schema source changed; Python/TypeScript artifacts align; recorded `codegen:check` passed. | PASS |
| New public capability has real first-class journeys | Rust, Python, TypeScript, and MCP paths have gated real-server journeys; owner integration/unit tests cover lower seams. | PASS |
| No gate circumvention | No new `#[allow]`; journey `#[ignore]` attributes are paired with explicit `--run-ignored=all` lanes. The TypeScript gateway assertion now checks the established full ancestry rather than an incorrect direct-parent assumption, matching Rust/Python. | PASS |
| Every new or materially modified Rust item has intent/invariant rustdoc and fallible functions have `# Errors` | `stored` lost its rustdoc when `fixed_width` was inserted; see `REPO-001`. | **FAIL** |
| Imports live in the module import block | Two newly added test functions contain local trait imports; see Non-blocking notes. | FAIL (non-blocking placement only) |

## Material repository-rule findings

### REPO-001 — BLOCK_BEFORE_MERGE — modified decoder lost its rustdoc

- Governing rule: `AGENTS.md` §16 and `architecture/agent-rules.md`: every new or materially modified Rust item, including private helpers, requires rustdoc; every fallible function requires `# Errors`; missing rustdoc is a hard blocker.
- Location: `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:2143-2158`.
- Evidence: the pre-existing `stored` documentation (`Decode a stored value...` and its `# Errors`) now immediately precedes the new `fixed_width` helper, so rustdoc attaches both descriptions to `fixed_width`. `pub(crate) fn stored` begins at line 2158 with no rustdoc. This is a candidate-introduced documentation regression in the changed verifier-run decode path.
- Consequence: maintainers no longer have the required error contract on the shared stored-value decoder, while `fixed_width` presents two unrelated contracts as if both describe it. Repository authority explicitly makes this merge-blocking even though runtime behavior and tests are unaffected.
- Smallest testable correction: leave `fixed_width` and its implementation unchanged; keep only the fixed-width description and its `# Errors` immediately above `fixed_width`, then restore the existing stored-value description and `# Errors` immediately above `stored`. No new helper, behavior, or test is needed. Confirm by source inspection and the existing format/lint lane; `check:docs` does not cover private `wyrd-sql` items, so it is not sufficient proof by itself.

## Non-blocking notes

- `crates/shared/wyrd-client/src/transport/trace.rs:187-188` and `crates/shared/wyrd-telemetry/src/export.rs:668` add trait imports inside non-generic test functions. `architecture/agent-rules.md` requires imports at the module top except the documented generic-function exception. Move these three `Trait as _` imports into their respective test modules' existing import blocks. This has no behavioral, security, tenancy, durability, or public-contract consequence, so it is not a blocking finding under the task-review rules.
- The extra `SpanData` read/clone at `crates/shared/wyrd-telemetry/src/export.rs:328-330` is a localized, documented ownership cost used to inspect links at span start. It introduces no new abstraction or configuration, and the approved rule requires links to influence tenant eligibility. No standards finding.

## Verification assessment

Recorded evidence in the task covers telemetry unit tests, server Bifrost integration, Rust/Python/TypeScript journeys, MCP/drift/gateway/shared/Vala/SQL owner lanes, format/lints, code generation, dependency and tenant-isolation checks, Python/TypeScript format/lint/typecheck, N-API checking, and `git diff --check`. The commands and results are credible for the touched surfaces. Per orchestration instructions, this reviewer did not rerun Cargo, mise, codegen, or database lanes while parallel discovery was active.

The one blocking defect is documentation-only and is not detectable by the recorded public-doc lane because `stored` is private to `wyrd-sql`; closure requires inspecting the corrected source placement. No additional broad verification command is needed for that correction.

## Overall result

**FAIL**

`REPO-001` is the only material repository-standards finding. All reviewed architecture, ownership, tenancy, SQL capability, async/runtime, generated-artifact, SDK parity, resource-bound, and test-integrity rules otherwise pass. The two import-placement violations are non-blocking.
