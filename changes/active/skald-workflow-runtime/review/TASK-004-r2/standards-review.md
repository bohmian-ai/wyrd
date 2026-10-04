# TASK-004 r2 repository standards review

Immutable subject: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56..e86831e5ac784028f8022cc3faeee1c22b12c665`

Approved authority: `SPEC-skald-workflow-runtime`, Revision 13.

## Material findings

### Important

#### STD-004-R2-001 — VIOLATION: new Rust interfaces use qualified type paths instead of imported bare types

- **Violated rule:** `architecture/agent-rules.md:9` requires types to be imported at module scope and used by bare name in struct fields, parameters, return types, trait bounds, and `where` clauses. The rule explicitly prohibits fully qualified paths in those positions.
- **Locations and evidence:** the cumulative candidate introduces this pattern throughout the changed Workflow and query surfaces, including:
  - `crates/wyrd/wyrd-server/src/components/workflow/host.rs:409` (`wyrd_spec::ids::CredentialBindingName` in a return type);
  - `crates/wyrd/wyrd-server/src/components/workflow/runs.rs:70`, `:72`, `:77`, `:88`, `:272`, `:404`, `:422`, `:521`, `:523`, `:525`, `:540`, `:542`, `:559`, and `:647` (`blake3::Hash`, `watch::Sender`/`Receiver`, `tokio::time::Instant`, `std::sync::atomic::AtomicBool`, and `tokio::sync::Notify` in fields or signatures);
  - `crates/wyrd/wyrd-server/src/query/collect.rs:332`, `:359-361`, `:371`, `:594`, and `:627` (`crate::state::QueryStreamStall`, `crate::oracle::RunningQueryControls`, `wyrd_spec::DataTenantId`, `wyrd_spec::request_id::RequestId`, `serde::Serialize`, and `arrow::datatypes::Schema` in fields, bounds, or signatures);
  - `crates/wyrd/wyrd-server/src/state.rs:2168` and `:2387` (`crate::components::workflow::WorkflowRuns` and `crate::config::ServerWorkflowConfig`);
  - `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:95`, `:97`, `:309`, and `:386` (`watch::Sender`, `axum::http::Uri`, and `reqwest::Client` in the new journey fixture);
  - `crates/wyrd/wyrd-testing/src/server.rs:486` and `:3853`, `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:145`, `:217`, and `:889`, `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:107` and `:153`, and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:301`, `:317`, `:451`, `:462`, `:473`, `:496`, and `:508` repeat the same violation in changed test-harness fields and signatures.
- **Consequence:** the changed code does not satisfy the repository's mandatory Rust source-shape rule, and dependency ownership remains obscured outside the module import blocks. This is a source compliance failure even though the recorded lint lane passed.
- **Required testable correction:** add module-level imports for each cited type (using aliases where two same-named types would collide) and use only the imported bare names in the cited fields, parameters, return types, and bounds. Preserve behavior. Re-run the existing formatting and lint lanes plus the affected recorded test lanes; do not add a new check or allow attribute.

#### STD-004-R2-002 — VIOLATION: TASK-004's active task authority still identifies Revision 12 after Revision 13 was assigned to it

- **Violated rule:** `architecture/references/languages/spec-driven-development.md:134-156` requires every task to identify the approved spec ID and revision from which it is derived. `spec.md:3` is approved Revision 13, and `spec.md:40-42` plus `spec.md:2717-2721` explicitly assign the provider-tagged `ProviderRequest` contract and its Vertex proof to TASK-004.
- **Locations and evidence:** `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:6` still declares `spec_revision: 12`. `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md:6` also declares Revision 12 and `:17` names Revision 12 as its approved input, while the same remediation record says at `:106` that Revision 13 was added to it and records Revision 13 implementation/proof at `:127-128` and `:159-168`.
- **Consequence:** the active task packet names two different governing revisions for the same cumulative implementation. A later implementer or reviewer following the front matter can apply Revision 12 and omit or reject the provider-tagged request contract that Revision 13 expressly assigns to this task.
- **Required testable correction:** make the active TASK-004 packet identify Revision 13 consistently. Update the task front matter and the remediation's current governing-revision metadata/prose to Revision 13 while retaining a clearly labeled historical note that the r1 verdict originally reviewed Revision 12; alternatively, record a distinct Revision 13 successor task and stop claiming that the Revision 13 implementation belongs to the Revision 12 remediation. Verify by source inspection that every active TASK-004 authority reference resolves unambiguously to Revision 13. No repository check, compatibility mechanism, or new setting is required.

No Critical findings.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active spec, TASK-004, r1 remediation, recorded evidence | `AGENTS.md` §§1, 2, 11, 12; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `maintainer-style.md` | **FAIL** — `STD-004-R2-002` |
| `ProviderRequest`, provider clients, prompt/cache/workflow consumers | `AGENTS.md` §§2-6, 10; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-design.md`; `architecture/references/languages/rust-core.md`; `architecture-constraints.md`; `patterns.md` | PASS |
| Workflow HTTP routes, accepted-run host/table/config/state, gateway and Cards composition | `AGENTS.md` §§2, 3, 5, 6, 9; `architecture/wyrd-design.md` Workflow section; `architecture/wyrd-security-posture.md`; `errors.md`; `agent-harness.md`; `patterns.md` | **FAIL** — `STD-004-R2-001`; ownership, security, and contract rules otherwise pass |
| Bounded query collector and built-in tool projection | `AGENTS.md` §§3, 5, 6, 9-11; `architecture/bifrost-design.md`; `olap-serving.md`; `datafusion.md`; `analytical-operations-reliability.md`; `errors.md` | **FAIL** — `STD-004-R2-001`; query ownership and terminal/error shape otherwise pass |
| Oracle analytical lifecycle, stream settlement, resources, and follower cleanup tests | `AGENTS.md` §§5, 6, 10, 11; `architecture/bifrost-design.md`; `vala-architecture.md`; `datafusion.md`; `analytical-operations-reliability.md` | PASS; the human-approved stream-close release model is preserved |
| Generated JSON schemas, YAML fixtures, Rust/Python/TypeScript contract consumers | `AGENTS.md` §§8, 9, 11, 12; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-design.md`; `testing-workflows.md`; `agent-harness.md` | PASS based on source/diff and recorded codegen/language evidence |
| Rust server and Bifrost journey tests and test-support harness | `AGENTS.md` §11; `testing-workflows.md`; `agent-harness.md`; `rust-core.md` | **FAIL** — qualified types in changed fixtures under `STD-004-R2-001`; test-tier placement and production-shaped coverage otherwise pass |
| `Cargo.toml`, lockfile, and `.config/nextest.toml` | `AGENTS.md` §§4, 11, 12; `rust-core.md`; `testing-workflows.md` | PASS — existing workspace dependencies and native nextest test groups are used; no bespoke check or third-party mechanism was introduced |
| Architecture updates for accepted authority and Oracle lifecycle | `AGENTS.md` §§1-2; `wyrd-design.md`; `wyrd-security-posture.md`; `bifrost-design.md` | PASS; no restoration of the rejected graph-drain polling or supervisor idle refusal was required |

## Applicable rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Approved task metadata names the governing spec revision | Task `:6`, remediation `:6`, `:17`, `:106`, spec `:3`, `:40-42`, `:2717-2721` | **FAIL** — `STD-004-R2-002` |
| Import types at module top and use bare names in interfaces | Representative and complete changed-interface groups listed in `STD-004-R2-001` | **FAIL** |
| Stateful workflows and dependencies have cohesive concrete owners | `WorkflowRunHost` at `components/workflow/host.rs:48-57`; `WorkflowRuns` at `components/workflow/runs.rs:206-223`; `BoundedQuery`/`ResultCollector` in `query/collect.rs` | PASS |
| Async is confined to IO or intentional orchestration | Workflow handlers await authorization, registry/provider/query IO; pure validation/hash/projection helpers remain synchronous (`host.rs:60-123`, `query/collect.rs:621-639`) | PASS |
| Server owns identity, authorization, tenancy, audit, and durable side effects | `components/workflow/routes.rs:76-95`, `:120-128`, `:153-161`; canonical authorization/audit before admission at `host.rs:60-100`; run keys bind tenant and principal at `host.rs:93-99` | PASS |
| Public handlers use typed bodies/errors and trace instrumentation | `routes.rs:41-82`, `:102-125`, `:135-158`; stable `WyrdError` responses and OpenAPI declarations are present | PASS |
| Accepted runs capture bounded authority without retaining bearer material | `architecture/wyrd-security-posture.md` accepted-run exception; implementation composes a caller snapshot into tracked preparation and continues per-call owner admission (`host.rs`, `gateway/workflow.rs`, `query/collect.rs`) | PASS |
| Provider details stay provider-owned and the public request is explicitly tagged | `skald-spec/src/request.rs:20-59`; provider-specific send conversions remain in `skald-providers`; generated schema requires `provider` and `body` | PASS |
| Generated artifacts are regenerated from source, not maintained as a parallel contract | Source derive at `request.rs:24-27`; corresponding schema/fixture diffs; remediation recorded `codegen:regen`/`codegen:check` and no hand edits | PASS |
| User-facing behavior has primary user-journey coverage; negative paths are not unit-only substitutes | `wyrd-server/tests/pg_workflow_runs.rs`; `wyrd-testing/tests/bifrost/oracle/workflow.rs`; recorded S1-S7, forwarded Oracle journey, Rust/Python/TypeScript lanes | PASS |
| Tests that require Postgres/server/runtime live in gated integration or journey lanes | `pg_workflow_runs` is an external server test with `test-support`; Oracle Workflow coverage is in the journey profile; nextest native grouping bounds shared Postgres use | PASS |
| New checks/settings/dependencies must be established, standard mechanisms | No new repository check; `.config/nextest.toml` uses nextest's existing test-group mechanism; `jsonschema` was already a workspace dependency and is dev-only | PASS |
| No gate circumvention or unsupported compatibility layer | No new `allow`, ignored test, compatibility reader, alias, migration, secondary audit path, query engine, or lifecycle owner in the cumulative diff | PASS |
| Required Rust documentation and fallible-operation `# Errors` sections | Changed public/private Workflow, query, config, and fixture items carry substantive rustdoc; recorded lint lane passed | PASS |
| Tenant isolation and secret handling remain server-owned and fail closed | Caller-derived tenant/principal keys, canonical per-call authorization, secret-provider binding, cross-tenant not-found behavior, and no bearer/secret retention are present; the approved foreign-tenant harness limit is accurately recorded | PASS |
| Oracle follower release follows the approved ownership model | Grant-stream close is the release signal; tests await follower cleanup. Deleted graph-drain polling and supervisor idle refusal remain deleted | PASS |

## Verification notes

- Per the review constraint, this reviewer ran no builds, tests, Cargo commands, or mise commands. Review evidence is the immutable diff, candidate source, applicable authorities, and the recorded evidence in the r1 remediation.
- Recorded evidence reports PASS for formatting, lints, code generation, client-tier/PyO3/tenant/unwrap checks, Skald/shared/Wyrd/principals/gateway lanes, Python and TypeScript unit/integration lanes, the Rust workflow-loading journey, Oracle journeys, and all Bifrost lanes.
- The passing recorded lint lane does not waive the source-explicit bare-type rule in `architecture/agent-rules.md`.
- No verification limit is being converted into a pass. The candidate commit remained `e86831e5ac784028f8022cc3faeee1c22b12c665` during this review.

## Overall result

**FAIL**

The candidate otherwise follows the applicable ownership, security, async, contract, test-taxonomy, generated-artifact, and Oracle lifecycle authorities, but the two explicit repository-rule violations above must be corrected. Neither correction requires bespoke machinery, a new check, or restoration of the human-rejected Oracle mechanisms.
