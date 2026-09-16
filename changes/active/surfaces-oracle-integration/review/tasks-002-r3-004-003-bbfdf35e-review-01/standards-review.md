# Repository Standards Review

Overall result: **FAIL**

Immutable subject:

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Range: 483 files changed, 11,895 insertions, 2,614 deletions

This review covers repository-rule compliance only. It does not decide task
acceptance and does not perform the structured Ponytail audit.

## Review Findings

### Critical

- None.

### Important

- **REPO-001 — VIOLATION — contradictory audit authorities remain after the approved audit model changed.** `architecture/wyrd-security-posture.md:239-242`, `architecture/operations/README.md:25,50`, `architecture/operations/reliability-and-recovery.md:148-150`, `architecture/operations/runbooks.md:135-139`, `architecture/references/architecture/patterns.md:259-262`, `architecture/references/doctrine/architecture-constraints.md:116-118`, `architecture/references/domain/vala-architecture.md:77-80`, `architecture/references/languages/agent-harness.md:93-96`, and `architecture/references/languages/implementation-execution.md:225` still require Oracle's removed local audit-acceptance WAL and bounded relay. That directly contradicts `AGENTS.md:118-122`, approved spec revision 8 (`spec.md:203-207,423-438,872-876,929-936`), `architecture/bifrost-design.md:435-443`, and the implemented tracked outbox writer at `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:1-15,63-105`. The routed repository authority therefore tells implementers and operators to restore a prohibited second WAL/relay and declares readiness dependencies that no longer exist. Update every listed security, operations, and focused-reference passage to the approved tracked non-blocking `vala.audit_staging` commit model: reads do not wait; failures are logged/counted; shutdown tracks pending commits; there is no audit WAL or relay. Closure is testable with an exact repository search showing no Oracle audit-WAL/relay requirement plus `mise run docs:check`.

- **REPO-002 — VIOLATION — the Python SDK adds a forbidden and ineffective per-crate Cargo profile.** `sdks/wyrd-sdk-python/Cargo.toml:67-71` defines `[profile.release]`, contrary to `AGENTS.md` section 4's rule not to add per-crate profile blocks. Cargo confirms the defect on every metadata/build-backed check: `profiles for the non root package will be ignored`, naming this manifest and the workspace root. The intended LTO, codegen-unit, stripping, and debug settings therefore do not affect the Python wheel while adding persistent warning noise to all workspace commands. Delete the member-local profile. If those settings are truly required for every applicable workspace release, that is a separate root-workspace profile decision; the current block cannot provide a Python-only profile. Closure is `rg '^\[profile\.' --glob Cargo.toml` showing only sanctioned workspace-root profiles and a Python wheel build with no ignored-profile warning.

- **REPO-003 — VIOLATION — newly added and materially changed Rust signatures and fields use fully qualified type paths instead of module-top imports.** Concrete sites are `crates/shared/wyrd-queue/src/producer.rs:1861`, `crates/vala/vala-sql/tests/pg_forge_tasks.rs:3139`, `crates/wyrd/wyrd-server/src/boot/mod.rs:1542`, `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:713,2570`, `crates/wyrd/wyrd-testing/src/server.rs:245,489,494,3433,3442`, and `crates/wyrd/wyrd-testing/tests/bifrost/scribe/round_robin.rs:304-307`. These use paths such as `std::path::PathBuf`, `tempfile::TempDir`, `serde_json::Value`, `wyrd_spec::ids::DataTenantId`, and `vala_bifrost_redux::catalog::TableRef` in governed fields or signatures. `architecture/agent-rules.md` explicitly requires imports at module top and bare type names in fields, parameters, returns, trait bounds, and `where` clauses, including tests. Import the named types at each module's top-level dependency block and use the bare names at these sites. Closure is a cumulative added-line scan for qualified types in fields/signatures plus `mise run fmt:check` and `mise run lints`.

### Suggestions

- None. Optional improvements and unrelated pre-existing debt are excluded.

## Authority Coverage

| Changed surface | Applicable authority reviewed | Coverage/result |
|---|---|---|
| Review subject, active packet, commit provenance | `AGENTS.md` sections 13-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; approved spec revision 8 | Covered. Candidate/tree are stable. The owner's explicit 2026-09-14 acceptance of the 22 earlier AI trailers is recorded in candidate `bbfdf35e`; no history rewrite is required by this review. |
| Workspace membership, dependency graph, Cargo features, Hakari workspace-hack, lockfile | `AGENTS.md` sections 1, 4, 11-12, 15-16; `agent-rules.md` Cargo/build rules; `rust-core.md`; `testing-workflows.md`; root/member manifests, `mise.toml`, `mise.lock`, `Cargo.lock` | **FAIL — REPO-002.** Workspace-hack ownership and boundary checks otherwise pass. |
| Shared Rust client consolidation: Cards, storage, `WyrdState`, Bifrost, transport, queue | `AGENTS.md` sections 2-6, 9, 16; `wyrd-design.md` client model; `wyrd-doctrine.mdx`; `architecture-constraints.md`; `patterns.md`; `rust-core.md`; `errors.md`; `bifrost-design.md`; `arrow-analytical-interop.md` | Covered. The one `wyrd_client::Bifrost`/Cards owner and structured error projection conform. **Rust type-import style fails at the REPO-003 sites.** |
| Python SDK relocation, PyO3 aggregation, public package, stubs, tests | `AGENTS.md` sections 2-8, 11-12, 16; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `errors.md`; `testing-workflows.md`; Python manifest and `pyproject.toml` | Covered. Placement, optional feature boundary, public imports, generated stubs, and production-wheel testing exclusion pass. **Manifest profile fails REPO-002.** |
| TypeScript SDK relocation, napi projection, declarations, unit/journey tests | `AGENTS.md` sections 2-3, 8-12, 16; `typescript-guide.md`; `errors.md`; `testing-workflows.md`; TS manifests and declarations | Covered. Native binding remains a thin `wyrd-client` projection; generated declaration and journey evidence is present. |
| Server auth, query, MCP/HTTP/CLI, config, readiness, unified data root | `AGENTS.md` sections 2, 6, 9, 11, 15-16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `bifrost-design.md`; `wyrd-security-posture.md`; `operations/README.md` and its deployment/reliability/runbook authorities; `patterns.md`; `agent-harness.md`; `errors.md` | Covered. Typed server ownership, one serving surface, root lock-before-activation, and public problem responses pass. **Derived security/operations authority fails REPO-001; Rust type-import style fails REPO-003.** |
| Vala/Bifrost Scribe, Oracle, Forge, catalog, audit publication and SQL migrations | `agent-rules.md` SQL/tenancy/audit rules; `bifrost-design.md`; `wyrd-security-posture.md`; `vala-architecture.md`; `olap-serving.md`; `iceberg.md`; `datafusion.md`; `arrow-analytical-interop.md`; `analytical-operations-reliability.md`; manifests and migrations | Covered. Tenant-bound owners, canonical staging/publisher path, Forge/Scribe lineage split, and single data-root composition conform in source and targeted checks. **Authority prose fails REPO-001; qualified signatures fail REPO-003.** |
| Wyrd contracts, schemas, OpenAPI, error derive/catalog | `AGENTS.md` sections 2-4, 8-9, 12; `wyrd-design.md`; `architecture-constraints.md`; `agent-harness.md`; `errors.md`; `wyrd-spec` manifest/generators | Covered. `wyrd-spec` remains IO-/async-/PyO3-free; codegen and protocol drift checks pass. |
| Test taxonomy, fixture/runtime ownership, journeys, Postgres harness | `AGENTS.md` sections 11-12, 16; `agent-rules.md`; `TESTING.md`; `testing-workflows.md`; `implementation-execution.md`; test manifests and `mise.toml` | Covered. Every crate is assigned once, Bifrost and language journeys have owning lanes, and the checked deployment contract passes. **Test/helper signature style fails REPO-003.** |
| CI selection/aggregation, nightly/release/performance split, scripts/checks | `AGENTS.md` sections 11-12; `TESTING.md`; `testing-workflows.md`; workflow files, `.github/scripts`, `scripts/checks`, `mise.toml` | Covered. Change classification defaults conservatively, required-job aggregation handles failure/cancellation/missing results, and script self-tests pass. |
| Kubernetes deployment, operations, docs, theme/skill copies | `AGENTS.md` sections 2, 11-12, 14-15; `wyrd-doctrine.mdx`; `bifrost-design.md`; `wyrd-security-posture.md`; all `architecture/operations/` authorities; `agent-harness.md`; docs and deployment files | Covered. Data-root manifests, docs build, accessibility, tokens, and skill synchronization pass. **Audit readiness/runbook language fails REPO-001.** |

## Applicable Rule Results

| Rule | Evidence | Result |
|---|---|---|
| Immutable review subject | `HEAD` remained `bbfdf35e26212b2a831bda5e31e1ef4433e41900`; `HEAD^{tree}` remained `58f5c2acfe8da01f3e4b58363f38ce761626df35` before report creation. | PASS |
| Active architecture and all routed authorities must agree; missing/contradictory authority update blocks completion | Canonical current rule is `AGENTS.md:118-122`; Bifrost authority is `bifrost-design.md:435-443`; nine routed security/operations/reference passages still require the removed WAL/relay. | **FAIL — REPO-001** |
| Server owns durable behavior; SDKs project `wyrd-client` | `wyrd-client/src/bifrost/facade.rs:1-52`; Python aggregation at `sdks/wyrd-sdk-python/src/lib.rs:1-125`; napi projection at `sdks/wyrd-sdk-ts/native/src/lib.rs:1-31`; boundary checks pass. | PASS |
| Client tier excludes server/SQL/cloud/DataFusion/Iceberg ownership | `mise run check:client-tier`, `check:sdk-client-tier`, and `check:workspace-hack` pass. | PASS |
| `wyrd-spec` remains foundational and PyO3-free; PyO3 is confined to the Python SDK boundary | `mise run check:pyo3-scope` and `check:sdk-pyo3-scope` pass; Python aggregation is feature-gated. | PASS |
| Production Python wheel excludes testing behavior | `mise run check:py-wheel-no-testing` passes. | PASS |
| Do not add per-crate Cargo profiles | Cargo ignores `sdks/wyrd-sdk-python/Cargo.toml:67-71` and warns on every Cargo-backed check. | **FAIL — REPO-002** |
| Imports live at module top; governed Rust type positions use bare names | Cumulative added-line inspection finds the exact qualified field/signature sites listed in REPO-003. | **FAIL — REPO-003** |
| New/materially modified Rust follows struct-centered ownership and documented item rules | Source inspection of `Bifrost`, `Cards`, `OracleQueryAudit`, `BifrostDataRoot`, queue owner, server composition, and Forge scheduler shows cohesive owners and substantive rustdoc; `check:unwrap-audit` and `check:clippy-allow-audit` pass. | PASS |
| Async exists only at IO/composition boundaries; no ad-hoc runtime | Modified async paths await transport, SQL, filesystem task boundaries, queue ownership, or composed execution; static boundary checks found no new runtime owner. | PASS |
| Public failures use the derive-backed Wyrd catalog and consistent HTTP/Python/TS projections | `codegen:check`, `check:proto-drift`, source error projection, and language-boundary tests/evidence pass. | PASS |
| Generated stubs/OpenAPI/schemas are reproducible, not hand-maintained drift | `mise run codegen:check` passes. | PASS |
| Tenant SQL uses the sanctioned owners; audit has one staging path and publisher | `query_audit.rs:63-105`, `peer_audit.rs:21-117`, Vala SQL/import inspection, and recorded SQL/Bifrost journeys support the current implementation. No added raw pool signature or second publisher was found. | PASS |
| Bifrost managed local paths derive from one exclusively locked root; Forge owns no root | `wyrd-server/src/boot/data_root.rs:1-128`; deployment contract task passes 2/2; resource-governance check passes. | PASS |
| Journey-first testing and lane ownership | `check:test-coverage` assigns all 48 crates exactly once; CI-selection self-tests pass 31/31 and required-job tests 7/7; task evidence records Rust/Python/TS/server/MCP/CLI journeys. | PASS |
| Formatting, generated contracts, docs, skills, CI scripts, and boundary checks remain clean | Locally rerun checks listed below pass; `git diff --check` is empty. | PASS, subject to verification limits |
| No AI co-author trailers | Twenty-two earlier trailers exist, but candidate `bbfdf35e` records explicit owner acceptance on 2026-09-14 without rewriting history. Current-user/explicitly-approved outcomes outrank the repository default under `spec-driven-development.md`. | PASS by explicit owner acceptance |

## Open Questions

- None. The three failures have repository-defined corrections and require no new product or architecture decision.

## Verification Notes

Locally executed against the candidate and passed:

- `mise run fmt:check`
- `mise run check:client-tier`
- `mise run check:sdk-client-tier`
- `mise run check:pyo3-scope`
- `mise run check:sdk-pyo3-scope`
- `mise run check:py-wheel-no-testing`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:skills-sync`
- `mise run check:ci-selection` (31 classification cases and 7 aggregation cases)
- `mise run check:workspace-hack`
- `mise run check:bifrost-oracle-deploy` (2/2)
- `mise run check:bifrost-resource-governance`
- `mise run codegen:check`
- `mise run docs:check`
- `mise run check:proto-drift`
- `mise run check:test-coverage`
- `mise run check:no-legacy-server-vocab`
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf bbfdf35e26212b2a831bda5e31e1ef4433e41900`

Verification recorded in the immutable active packet was also inspected for
the Rust, Python, TypeScript, SQL/Postgres, storage-emulator, Bifrost capability,
identity, CLI, Cards, `WyrdState`, MCP, server, docs, codegen, and CI-selection
lanes. Those records are supporting evidence, not a substitute for the source
inspection above.

Verification limits:

- This reviewer did not rerun the complete `mise run gate`, workspace-wide
  `mise run lints`, every long Bifrost journey, live-cloud storage jobs, or the
  weekly performance qualification. The active packet records the aggregate's
  dependencies and affected journeys passing across the immutable candidate's
  constituent commits; hosted workflow execution still requires a push.
- `docs:check` and `codegen:check` prove mechanical generation/build integrity;
  neither detects REPO-001's semantic contradiction.
- Cargo's ignored-profile warning is repeatable in every Cargo-backed command
  above and is direct evidence for REPO-002 even though the boundary commands
  themselves exit zero.

## Candidate Immutability Check

- Initial `HEAD`: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Initial candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Pre-report `HEAD`: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Pre-report candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Tracked candidate source remained unchanged. Concurrent reviewers created
  only untracked review artifacts under the assigned review directory and an
  unrelated untracked verifier directory; neither changes the candidate tree.
