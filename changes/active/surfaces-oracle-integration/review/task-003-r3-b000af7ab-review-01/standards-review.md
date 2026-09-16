# Repository standards review — TASK-003-R3 cumulative candidate

## Immutable subject and scope

- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Candidate: `b000af7ab704f077a8a4ba3e29c2b968d47d344d`, tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`.
- Tested source: `f8e887b3038651d2ba82091d642915d6b856d4d6`, tree `eb98062dc854277a248f37d86afe55beb9c3db26`. The later commit adds review/evidence Markdown only, not source.
- Reviewed the complete base-to-candidate change surfaces, with direct source inspection of the R3 delta, manifests, test placement, workflow and verification map. This is a repository-rules audit, not task acceptance. No `.codegraph/` index exists. The unrelated dirty `verified-change-contract` worktree files were not touched. `git diff --check` on the immutable range passed.

## Authority coverage

| Cumulative changed surface | Applicable authority | Inspected evidence |
|---|---|---|
| Shared Rust client; Rust, Python/PyO3 and TypeScript SDK projections; typed contracts and generated declarations | `AGENTS.md` §§2–9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide,errors,testing-workflows}.md` | Cumulative path inventory, client/SDK ownership and generated-artifact locations; recorded codegen, typing, language-journey and boundary-check results. |
| Server HTTP/gRPC/MCP, remote Oracle deadline, identity/security, tenancy, audit and error mapping | Common authorities above; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/references/domain/{vala-architecture,olap-serving,analytical-operations-reliability}.md` | `oracle/{forwarding,peer_service}.rs`, `state.rs`, `http/middleware/edge_timeout.rs`, server journey, `wyrd-server` and `wyrd-testing` manifests. |
| Bifrost Scribe/Forge/catalog, SQL migrations, storage and deployment | Common/security/Bifrost authorities; `architecture/references/domain/iceberg.md`; `architecture/operations/README.md` | Cumulative owner and test locations; R3 did not alter these production surfaces; recorded Forge, Postgres, storage emulator, real-cloud and deployment checks. |
| CI/build/test tooling, docs and approved change evidence | `AGENTS.md` §§11–16; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md` | `mise.toml` gate/Bifrost dependency graph, `scripts/run-bifrost-tests.sh`, cloud Actions, storage docs, R3 same-tree evidence matrix and candidate-only review files. |

## Applicable rule results

| Rule | Result | Evidence and limit |
|---|---|---|
| Server/Vala own durable query and storage behavior; `wyrd-client` is the one SDK-facing implementation; language bindings remain projections; client-tier dependencies stay narrow | PASS | Cumulative owner paths and manifests remain separated; R3 timeout is in `ReadyOracleForwarder`, not duplicated in SDKs. Recorded boundary checks and language journeys cover the cumulative change. |
| Public contracts, stable error projection, generated artifacts and public Python/TypeScript surfaces stay aligned | PASS with verification limit | R3 adds no public schema or SDK API. Existing generated files are in their owning paths; `codegen:check`, Python typing and TypeScript declaration checks are recorded as passing on the tested source. No raw runner logs were provided, and I did not regenerate. |
| Server authentication, peer trust, tenant isolation, audit, deadline/cancellation and protected-edge ownership are retained | PASS for repository boundaries | R3 uses the already captured query deadline and `BifrostError::QueryTimeout` in the server owner. The `SilentForwardPeer` switch and handler call are behind the pre-existing `test-support` feature; `wyrd-testing` opts into that feature while the default server build does not. The hook is consulted only when armed in tests. The prior `EdgeTimeout` associated types and fallible methods now have rustdoc and `# Errors`. Functional task acceptance belongs to task/domain review. |
| New tests belong to their owning runtime/tier, do not weaken gates, and use repository-managed setup | PASS with verification limit | The new paused-time unit test is inline; the real-server role-separated journey is in `wyrd-testing/tests/bifrost/server/query.rs`, `#[ignore]`-gated like that journey lane, and recorded as run with a nonzero exact selector under the Postgres wrapper. No added `#[allow]` or disabled existing test in the R3 source delta. `gate` and `verify:bifrost` did not themselves finish because of host memory pressure; the R3 evidence maps their declared children to same-source-tree passing runs. I verified the declared task/runner structure but did not rerun the heavy or credentialed lanes. |
| Cloud proof and Actions scheduling follow approved revision 9 | PASS with verification limit | `.github/workflows/storage-integration-cloud.yml` triggers on pushes to `main`, with three provider jobs and no weekly schedule. Storage docs and `mise.toml` use `WYRD_STORAGE_URL`; R3 evidence records two passing real-cloud tests per local `mise.local.toml` provider task. Credentialed results were not independently rerun. |
| Rust item documentation, including private fields, test helpers and panic conditions (`AGENTS.md` §16; `architecture/agent-rules.md`; `rust-core.md` Documentation) | **FAIL** | Two new tuple fields and one new assertion-bearing test have incomplete rustdoc; `STD-R3-1`. |
| Top-level `use` imports and bare names in struct fields, signatures, return types and `where` clauses (`architecture/agent-rules.md`) | **FAIL** | Several new R3 field/signature positions use fully qualified type paths; `STD-R3-2`. |
| No new dependency/feature, production test hook, broad compatibility alias, or change to default production behavior | PASS | `test-support` already existed; the new switch, export, state accessor and peer-handler await are all `#[cfg(feature = "test-support")]`. No manifest change in R3. The ordinary remote delivery correction uses existing Tokio timeout and existing typed error. |

## Material findings

### `STD-R3-1` — incomplete rustdoc on new test helpers

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc on every new Rust field and test function, and `# Panics` when a panic remains possible. This includes private, local and test-only items.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/oracle/forwarding.rs:118` defines `Abandon<'a>(...)` with documentation on the struct but none on its tuple field; `:1043` does the same for `DropProbe(Arc<...>)`. The new `silent_selected_delivery_times_out_once_and_is_cancelled` test at `:1052` uses `expect` and assertions but has no `# Panics` section. The new journey test does document both its errors and panics, and the `SilentForwardPeer` cancellation/count comments accurately describe the armed-handler path.
- **Consequence:** New test-support and unit-test code fails the repository's explicit documentation completion gate even though the tests compile and pass.
- **Testable correction:** Add concise intent/ownership rustdoc to both positional fields, and a `# Panics` section stating the test fails when cancellation, one-delivery or one-connect assertions fail. Source inspection plus `mise run fmt:check` and `mise run lints` closes this documentation-only gap.

### `STD-R3-2` — new field and signature types bypass module import style

- **Rule:** `architecture/agent-rules.md` requires types in struct fields, signatures, return types and `where` clauses to enter through top-of-module `use` statements and appear by bare name at the use site.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/oracle/forwarding.rs:71,73` uses qualified `AtomicBool` and `watch::Sender` for new `SilentForwardPeer` fields; `:104` uses qualified `watch::error::RecvError` in a new method return; `:118,1043` uses qualified types in new tuple fields; and the materially changed `route_remote_once` bound at `:598` retains `wyrd_spec::vala::api::VisibilityMode`. The new `Bifrost::silent_forward_peer_for_test` signature at `crates/wyrd/wyrd-server/src/state.rs:1735` uses `crate::oracle::SilentForwardPeer` instead of an imported bare name.
- **Consequence:** New and modified Rust items violate the file-level dependency-manifest convention; unlike body-local constructor paths, these qualified type positions are explicitly prohibited by the repository rule.
- **Testable correction:** Import the named types at the top of their modules (feature-gated where only test-support uses them) and use bare names in the listed field/signature/where-clause positions. Do not change the timeout, test hook, public behavior or feature graph. Confirm by source inspection, `mise run fmt:check`, and `mise run lints`.

## Result

**FAIL** — `STD-R3-1` and `STD-R3-2` are bounded repository-rule findings. This does not imply the remote-query timeout behavior or the recorded tests failed. Broad/credentialed verification was not rerun because this review has an immutable subject and the shared host has limited free memory/disk.
