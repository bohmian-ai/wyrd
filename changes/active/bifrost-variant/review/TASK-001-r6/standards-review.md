# TASK-001 Repository Standards Review — r6

## Overall result

**FAIL**

Finding: `STD-R6-001`.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

CodeGraph was used first. The review then inspected the complete cumulative
base-to-candidate changed map, the r1-r5 review/remediation record, the r5
implementation evidence, the latest two implementation commits, applicable
architecture/reference authority, surrounding owners, manifests, generated
surfaces, and tests. No other r6 review conclusion was used.

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Active spec, original task, r1-r5 remediation/review records, and review-skill changes | `AGENTS.md` §§1, 11, 12 and completion rules; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows,maintainer-style}.md` | **FAIL** — `STD-R6-001`; the remaining packet contents and approved task-review skill exception pass |
| Workspace manifests, lockfile, Arrow/Parquet/DataFusion alignment, pinned Iceberg forks | `AGENTS.md` §§1, 3-6; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/{iceberg,datafusion,analytical-operations-reliability}.md` | PASS |
| `wyrd-spec` Variant wire contracts, errors, schemas, protobuf, and generated artifacts | `AGENTS.md` §§2-4, 8-9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/errors,languages/agent-harness}.md` | PASS |
| `wyrd-queue` JSON/raw Variant validation, exact numbers, schema identity, encoding, rendering, and batch construction | `AGENTS.md` §§3-6 and documentation rules; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/arrow-analytical-interop.md` | PASS |
| Latest raw shared-object defense and its focused/unit journey proof | Trust-boundary validation and error-handling rules in `AGENTS.md` §§4, 6, 10-12; `architecture/bifrost-design.md`; `architecture/references/domain/{arrow-analytical-interop,analytical-operations-reliability}.md` | PASS |
| Vala table declarations/validators, Scribe admission, Parquet, Gate, Oracle SQL/distributed execution, Forge and GC | `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/domain/{vala-architecture,olap-serving,datafusion,iceberg,arrow-analytical-interop,analytical-operations-reliability}.md`; `architecture/references/languages/errors.md` | PASS |
| Verification, evaluation, gateway, audit, agent traces, and OTLP trace/log/metric built-ins | `AGENTS.md` §§2, 9-10; `architecture/bifrost-design.md`; `architecture/references/domain/{telemetry-observations,evaluation,drift-monitoring}.md` | PASS |
| Shared Rust client, HTTP/gRPC, CLI, MCP, Python/PyO3, TypeScript/N-API and declarations | `AGENTS.md` §§2-9; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,typescript-guide,agent-harness,errors}.md` | PASS |
| Rust, server, Oracle, Forge, CLI, MCP, Python, TypeScript, and OTLP tests | `AGENTS.md` §11; `architecture/agent-rules.md`; `TESTING.md`; `mise.toml`; `architecture/references/languages/testing-workflows.md` | PASS |
| Bifrost architecture and public schema documentation | `AGENTS.md` §§1-2, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `docs/src/content/docs/bifrost/schema.svx` | PASS |

## Applicable rule results

| Repository rule | Evidence | Result |
|---|---|---|
| The active packet binds one approved specification revision and advances each task through `proposed`, `ready`, `in_progress`, `review`, and `approved`. | Spec-driven development lines 170-179 define the lifecycle. TASK-001 is correctly `review` at `tasks/TASK-001-variant-storage-and-query.md:1-10`, but the fully implemented r5 remediation remains `status: ready` at `review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:1-10` despite implementation evidence and final verification at lines 288-373. | **FAIL — `STD-R6-001`** |
| Repository-wide workflow changes must be within approved task scope or an explicit exception. | The unapproved `wyrd-implement` edits identified in r5 are absent from the cumulative base-to-candidate diff; both mirrors match the base. The separately approved `wyrd-task-review` edits remain synchronized. | PASS |
| Durable behavior stays in server/Vala owners; SDKs project `wyrd-client`; `wyrd-spec` stays foundational and PyO3-free. | Variant contracts remain in `wyrd-spec`, shared encoding/validation in `wyrd-queue`, durable admission/query/storage in Vala/server, and language wrappers remain thin projections. `check:client-tier` and `check:pyo3-scope` passed. | PASS |
| Cargo features and dependencies must be earned and kept in the narrowest owner. | The cumulative manifest diff adds only the dependencies required by the Variant owners and keeps DataFusion, Parquet, and Iceberg out of client tiers. Revision 13 and the latest shared-object correction add no dependency or feature. | PASS |
| New/materially changed Rust follows struct-centered ownership; free functions are pure helpers; async is limited to IO. | `EncodedVariant` owns the construction invariant; `VariantViolations` and the walkers are private deterministic helpers. Table, Oracle, Scribe, Forge, and client workflows remain on their existing concrete owners. Pure Variant work is synchronous. | PASS |
| Rust imports stay at module/test-module tops and signatures use imported bare names. | Prior cumulative import issues were remediated; the latest changes add no nested production import or fully-qualified signature type. | PASS |
| New/materially changed Rust items have substantive rustdoc, including errors, panics, invariants, and side effects. | `scan_encoded`, `raw_shared_field_values_are_refused`, `AB_METADATA`, and `shared_objects` document their role and relevant panic/error behavior; the previously changed surfaces retain the documented shape accepted in r5. | PASS |
| Public failures use the Wyrd error catalog and preserve structured identity across HTTP/gRPC/SDK/CLI/MCP and distributed query paths. | Variant failures remain `BifrostError` catalog variants. `QueryCatalogError` retains local/remote identity; the task packet now cites that current owner. The shared-object refusal reuses `WYRD_VALA_400_VARIANT_INVALID_JSON` rather than adding a parallel error. | PASS |
| Trust-boundary work is bounded and validated before ACK/durable work; safety and validation may not be simplified away. | `EncodedVariant::from_bytes` size-checks, performs the explicit-stack scan, refuses malformed/numeric/depth violations, and invokes upstream recursive validation only for bounded depth. The latest node budget prevents overlapping offsets from expanding into exponential work and refuses before the Scribe ACK path. | PASS |
| Tenant isolation, authorization-before-IO, audit ownership, and sensitive-root behavior remain intact. | No new raw pool, tenant predicate, permission bypass, audit path, caller-selected tenant, or sensitive-root access path entered the cumulative change. The latest remediation changes only raw Variant validation and fixtures. | PASS |
| Schema fields that can diverge are mapped by name; persisted Variant/Struct shape and generated declarations stay synchronized. | Table validation/projection remains name-based; revision-13 raw numeric rules and nullable Struct rules are reflected in schema/docs and generated surfaces. Recorded `codegen:check` passed. | PASS |
| User/agent-facing behavior has real SDK-to-server journeys in each shipped surface; runtime-specific behavior stays in its runtime. | The task evidence records Rust, Python, TypeScript, CLI, MCP, server, OTLP, Oracle, and Forge journeys. The latest malformed shared-object case is also exercised through the raw-ingest real-server journey, not only a unit test. | PASS |
| External tests must earn their binary and environment-sensitive tests use repository-managed setup. | External additions exercise real server, Postgres, gRPC/HTTP, SDK, and object-store boundaries. Recorded commands use the repository Postgres wrapper and `mise exec`; Python/Node lifetime tests remain in Python/TypeScript. | PASS |
| Generated artifacts are changed through owners/generators, not edited to mask drift. | Contract/protobuf/schema/N-API changes accompany their source owners and recorded `codegen:check` passed. No generated artifact changed in the latest remediation commits. | PASS |
| Gates may not be bypassed with weakened assertions, ignores, sleeps, skips, or unjustified lint allowances. | The cumulative diff contains no new ignored required test or production lint suppression used to clear this task. The recorded lint failure was corrected by splitting one test without weakening behavior. `check:unwrap-audit` and `check:clippy-allow-audit` passed. | PASS |
| The cumulative diff stays within the approved Variant task/remediation scope and preserves explicit non-goals. | The r5 remediation removed the unrelated implementation-skill drift. Remaining code, docs, tests, task evidence, and the explicitly approved task-review skill change concern Variant storage/query, its built-in consumers, cross-surface proof, or review machinery approved by the caller. | PASS |

## Material finding

### STD-R6-001 — Implemented r5 remediation is still marked `ready`

- **Violated rule:** `architecture/references/languages/spec-driven-development.md:170-179` requires task status to advance from `ready` through `in_progress` to `review` after implementation; review is the read-only phase.
- **Location:** `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:1-10`.
- **Evidence:** The same record contains completed implementation evidence for every remediated finding at lines 288-302, successful focused and aggregate commands at lines 304-330, and a final post-addendum proof at lines 340-373. The original TASK-001 correctly says `status: review`, so the parent and implemented remediation disagree about the current lifecycle.
- **Consequence:** Automation or a maintainer reading the active packet sees TASK-001-R5 as not yet started/actionable for implementation even though its candidate is already under r6 review. That makes the packet non-authoritative and can route duplicate implementation work.
- **Testable correction:** Change only TASK-001-R5 front matter from `status: ready` to `status: review`. Confirm the parent task and remediation both identify the current review phase and run `git diff --check`; no new checker or evidence artifact is needed.

## Verification and limits

Source inspection and recorded r5 evidence cover the full cumulative range. The
r5 record reports green focused Variant/table/Oracle/server journeys, Rust,
Python, TypeScript and MCP contract journeys, OTLP and Forge coverage,
`mise run fmt`, `mise run lints`, `mise run codegen:check`,
`mise run check:skills-sync`, `mise run docs:check`, and `git diff --check`.
The final addendum reports the 12-test Variant lane, affected server journeys,
Oracle tests, formatting, lints, and diff check green on the final candidate.

This reviewer ran only non-runtime repository checks before the coordinator's
shared-checkout restriction was received: `git diff --check`, `mise run
fmt:check`, `mise run check:skills-sync`, `mise run check:client-tier`, `mise
run check:pyo3-scope`, `mise run check:unwrap-audit`, and `mise run
check:clippy-allow-audit`; all passed. No Cargo test, code-generation, database,
Python, TypeScript, or documentation lane was rerun in r6. Runtime proof is
therefore the recorded r5 evidence, not an independent r6 execution.
