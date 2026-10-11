# TASK-001 Repository Standards Review — r7

## Review Findings

### Critical

None.

### Important

- **STD-R7-001 — The active Bifrost authority claims linear raw validation, but the implementation sorts and binary-searches every object's offsets.** `architecture/bifrost-design.md:169` says raw-byte validation is linear in input size, and the R6 evidence repeats that as the closure of `FIND-TASK-001-4` at `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md:307`; however, `crates/shared/wyrd-queue/src/variant.rs:1050-1073` collects all offsets, calls `sort_unstable`, and performs `partition_point` for every field, making an object with *f* fields `O(f log f)` rather than linear. The accepted implementation remains size-bounded and prevents amplification, so the smallest correction is documentation-only: replace the linearity claim with the actual approved invariant—iterative, size-bounded, and non-amplifying validation—and make the R6 evidence describe that same property; do not add a new algorithm solely to preserve an unrequired complexity claim.

- **STD-R7-002 — The R6 implementation record names changed Rust tests without recording their mandatory exact focused commands.** `AGENTS.md` §11 and `architecture/references/languages/spec-driven-development.md` “Test command precision” require every specifically named Rust test in a task artifact or implementation report to be run with an explicit package, target, and exact `test(=...)` expression, while the R6 acceptance table names the new `variant::tests::{raw_shared_field_values_are_refused,renderers_refuse_hostile_stored_variants,json_depth_is_decided_by_wyrd_at_any_depth,raw_numbers_outside_the_json_domain_are_refused,json_size_outranks_numeric_and_depth,raw_repeated_field_names_are_refused}` tests at lines 309-314 but the command table records only the whole `wyrd-queue --lib` lane at line 323. The same record names the modified `maximal_log_projection_preserves_body_context_and_presence` test in its diagnosis without an exact command, and line 327 uses a regex selector for the gateway unit rather than the required exact selector. Run each named new or modified Rust test through the pinned toolchain with its exact final name (and the repository Postgres wrapper where needed), record the commands/counts/exits in the R6 evidence, and retain the already-green whole-crate and journey lanes as broader proof.

- **STD-R7-003 — The immutable cumulative candidate fails the required whitespace gate.** `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..6147cc617d81f2c03464043be698ab2565e9d745` reports `changes/active/bifrost-variant/review/TASK-001-r6/standards-review.md:88: new blank line at EOF`, while the R6 evidence checked only its remediation range (`b4ea01848..HEAD`) and therefore missed the tracked cumulative defect. Remove the extra terminal blank line from that review artifact and rerun `git diff --check` over the complete task base-to-candidate range.

### Suggestions

None.

## Open Questions

None. All three corrections are bounded documentation, evidence, or whitespace changes and require no product, public API, architecture, security, compatibility, concurrency, or persistent-data decision.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The commit and tree matched the requested immutable subject before this report was written. CodeGraph was used to trace the R6 Variant owner and callers; the complete cumulative changed-path map, applicable authorities, R6 implementation diff/evidence, manifests, generated surfaces, and prior standards findings were inspected independently.

## Authority Coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Active spec/task and r1-r6 review/remediation records | `AGENTS.md` §§11-16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows,maintainer-style}.md` | **FAIL** — `STD-R7-002`, `STD-R7-003`; task and remediation lifecycle states otherwise pass |
| Workspace manifests, lockfile, Arrow/Parquet/DataFusion alignment, and pinned Iceberg forks | `AGENTS.md` §§1, 3-6, 15; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/{iceberg,datafusion,analytical-operations-reliability}.md` | PASS |
| `wyrd-spec` Variant contracts/errors, schemas, protobuf, and generated artifacts | `AGENTS.md` §§2-4, 8-9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/{architecture/patterns,languages/errors,languages/agent-harness}.md` | PASS |
| `wyrd-queue` JSON/raw Variant admission, numeric domain, canonicality, rendering, and batch construction | `AGENTS.md` §§3-6, 15-16; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/arrow-analytical-interop.md` | **FAIL** — `STD-R7-001`; owner, dependency, sync/async, rustdoc, and trust-boundary shape otherwise pass |
| Vala tables/Scribe/Oracle/Parquet/Forge/Iceberg and nullable built-in Structs | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/domain/{vala-architecture,telemetry-observations,olap-serving,datafusion,iceberg,arrow-analytical-interop,analytical-operations-reliability}.md` | PASS |
| Server gateway/query/verification, audit, CLI, MCP, Rust client, Python/PyO3, TypeScript/N-API | `AGENTS.md` §§2-10; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,typescript-guide,agent-harness,errors}.md` | PASS |
| Rust unit/integration/journey, Python, TypeScript, MCP, CLI, OTLP, distributed Oracle, and Forge proof | `AGENTS.md` §§11-12, 16; `architecture/agent-rules.md`; `TESTING.md`; `mise.toml`; `architecture/references/languages/testing-workflows.md` | **FAIL** — `STD-R7-002`; behavior coverage exists, but the required exact focused command record is incomplete |
| Active Bifrost authority and public schema documentation | `AGENTS.md` §§1-2, 12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `docs/src/content/docs/bifrost/schema.svx` | **FAIL** — `STD-R7-001`; revision-12/13 behavioral rules are otherwise synchronized |

## Applicable Rule Results

| Repository rule | Evidence | Result |
|---|---|---|
| Durable behavior stays in the narrow owning Rust layer and language SDKs remain projections over `wyrd-client`. | Contracts remain in `wyrd-spec`, shared validated values/rendering in `wyrd-queue`, and durable admission/query/storage in Vala/server; Python and TypeScript add no parallel validator. | PASS |
| Cargo features and dependencies are earned and version-aligned. | The cumulative manifests add the approved Arrow 59.3 Variant crates to their narrow owners, retain immutable Iceberg/compaction revisions, and keep analytical dependencies out of client tiers. R6 adds no dependency after establishing that `serde_json::RawValue` already performs the needed iterative syntax scan. | PASS |
| Material Rust follows struct-centered ownership and synchronous-by-default rules. | `EncodedVariant` owns the construction/admission invariant; its new helpers are private deterministic algorithms. Gateway and query journey methods await real IO. No new stateful free-function workflow or speculative abstraction was added. | PASS |
| Every touched Rust item has substantive rustdoc with required error/panic information. | The R6 owner methods, raw scanner, offset/depth helpers, test constants/helpers/tests, Oracle test, OTLP test, and gateway journey helper all document intent and applicable errors/panics. | PASS |
| Stable public errors use the derive-backed catalog and preserve structured identity. | Raw/JSON Variant failures continue through `BifrostError`; the new numeric cases reuse `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`, and the gateway proof asserts the existing schema error rather than adding a parallel carrier. | PASS |
| Trust-boundary work is bounded and validated before ACK/durable work. | Every raw constructor/renderer calls `EncodedVariant::validate`; Scribe reaches the shared table validator before ACK/WAL; shared/overlapping object regions and unsupported numbers are refused. The work is size-bounded and non-amplifying, but the authority's stronger linearity statement is false. | **FAIL — `STD-R7-001`** |
| Tenant isolation, authorization ordering, audit ownership, and sensitive-root behavior remain unchanged. | No new pool, tenant selector, authorization bypass, audit path, sensitive-root exception, or secret/log exposure entered R6. The peer journey uses the cluster mTLS authority and a generated tenant-bound test fixture. | PASS |
| Generated artifacts and permanent source contain no task/agent implementation references. | Generated contract changes accompany source owners and recorded `codegen:check` is green. No `TASK-*`, finding, plan, or implementation-agent reference was found in changed production/source documentation outside the active review packet. | PASS |
| Every named Rust test has an exact focused command and broader proof uses repository-managed setup. | New and modified tests are named in R6 evidence, but several have only a whole-library command, one gateway selector is regex-based, and one modified fixture test has no focused command. | **FAIL — `STD-R7-002`** |
| No gate is bypassed, and a red gate blocks completion regardless of cause. | No weakened assertion, new ignored production test, lint suppression, boundary exception, or deleted negative case was found; however, the cumulative whitespace gate is red. | **FAIL — `STD-R7-003`** |
| The cumulative diff remains within approved TASK-001/remediation scope. | The previously rejected implementation-skill drift is absent; the task-review skill edits retain the recorded owner exception. R6 changes only Variant behavior/docs/proof and review lifecycle evidence. | PASS |

## Verification Notes

The implementation record reports 68 `wyrd-queue` tests, 847 Postgres-backed Vala tests, focused Oracle/log/gateway tests, three server journeys, and green format, lint, codegen, skill-sync, docs, and remediation-range diff checks. This review did not rerun Cargo, Postgres, Python, TypeScript, code-generation, or documentation commands in the shared checkout.

Reviewer-run static checks:

- `git rev-parse HEAD^{commit} HEAD^{tree}` — matched the immutable candidate and tree.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..6147cc617d81f2c03464043be698ab2565e9d745` — **FAIL**, one extra blank line at `TASK-001-r6/standards-review.md:88`.
- Base-to-candidate permanent-source search for task/finding/agent-plan references — PASS.
- CodeGraph caller/source trace for the R6 Variant gate and its consumers — complete.

## Overall Result

**FAIL**

Finding IDs: `STD-R7-001`, `STD-R7-002`, `STD-R7-003`.
