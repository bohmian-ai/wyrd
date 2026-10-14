# Maintainer Review

## Subject and authority

- Repository: detached immutable candidate worktree.
- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`.
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`.
- Reviewed range: the complete `base..candidate` diff, 153 changed files.
- Product authority: `changes/active/example-journey-fixes/spec.md` revision 4 and `owner-decisions.md` OD-1 through OD-7.
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, and `architecture/references/languages/maintainer-style.md`.
- Task evidence: `TASK-001` through `TASK-005`, including the OD-5 replacement of TASK-005.

The candidate hash was still `c4ba2b0502f8968a78e5e960bcc9eba43e960643` when this review completed. I did not modify the immutable subject.

## Changed-surface coverage

| Surface | Changed owners and symbols inspected | Callers, consumers, and proof inspected | Maintainer result |
|---|---|---|---|
| Typed Jev contract | `skald-spec` request/response variants and `wire::jev::{JevRequest,JevQuestion,JevAnswer,JevUsage}`; Prompt rendering; generated schema and Python/TypeScript projections | Skald adapter/runtime/provider client, gateway native and unified adapters, Agent refusal, Vala judge, Rust/Python/TypeScript journeys and wire/adapter tests | One hard rustdoc defect; otherwise the dialect remains in the existing provider owners and uses closed typed variants rather than a parallel raw-JSON path. |
| Jev gateway behavior | `wyrd-gateway::adapter::{prepare,complete,jev,http}`, deployment capability selection, server gateway workflow | Native `/v1/systemone`, unified `/v1/chat/completions`, refusal mapping, fallback tests, OpenAPI contract and three SDK journeys | Route-specific preparation is kept beside the adapter; the pass-through does not introduce a second provider abstraction or compatibility alias. |
| Bifrost errors and subqueries | `oracle::{caller_planning_fault,map_sql_planning_error,map_query_planning_error}` and `live::reads_scalar_subquery` | Logical/physical planning callers, authorization-before-planning path, unit errors, SDK/MCP subquery and unauthorized-table journeys | Root-cause edits are local to the existing Oracle and live-leaf owners; no query-shape guard or new execution layer was added. |
| Analytical correctness workload | `SupportWorkload`, `SupportOracle`, `AnalyticalQuery`, seeded row generator and ten shared queries | Rust published/live/mixed journey and remote-Scribe peer journey; hand-counted oracle tests | Large but earned by OD-6: one shared dataset/query/oracle owner replaces language-specific copies and is consumed by all required data-state proofs. |
| MCP compatibility | operator `CreateArgs`, principals descriptor builder, `WyrdMcpHandler::{catalog,list_tools,get_tool}`, full-catalog schema test, TypeScript framework journey and Vitest configuration | Official MCP SDK and OpenAI Agents conversion test, real-server catalog/read call, existing operator validation journey | The union moved under the existing request property and retains the contract type; no validator or schema-normalization layer was invented. The worker cap is confined to the existing Vitest runner. |
| Attribution | gateway ingress headers, `GatewayCallAttribution`, `GatewayCallSubject`, invocation admission/capture, `ServerWyrdGatewayCaller::attributed`, direct/eval verification plumbing, result payload and verifier-run SQL | Four independent subject-header cases, direct/no-Run judge journeys, scheduled/observation SQL tests, result-to-call joins, generated result schema | Requested versus server-resolved provenance earns the enum because it controls the existing card-scope authorization boundary; optional Run/Card values remain one subject value passed through existing owners. |
| Trace propagation and links | client `transport::trace`, `HttpTransport::authenticated`, server `http::middleware::trace_context`, MCP re-scope, gateway span projection, telemetry `link_remote_span`, Python/TypeScript verify bridges | All authenticated HTTP helper callers, malformed header tests, SDK/MCP journeys, captured observation link test | Task-local owners are narrow and represent distinct inbound and outbound runtime boundaries. The shared HTTP helper removes duplicated header injection from callers. |
| Managed-secret sealing | `ManagedSecretBinding::{seal,open}`, `CredentialResolver`, gateway administration/service, boot rewrap and `SealedSecretTable::GatewayProviderCredentials` | RLS query paths, rotation/restart/cross-tenant tests, CLI local-dev journey and docs | OD-5 deletes the rejected tenant-keyring stack and extends the existing deployment keyring/rewrap owner. This is a net simplification with no compatibility branch. |
| Local sealing-key provisioning | `DevServer` retained key path and `local_sealing_key` | Installer root owner, every child-command creation, rerun and secret-leak CLI journey | One focused helper owns the atomic owner-only file creation; no duplicate server config parser or restart state was added. |
| Contracts, SDKs, docs, migrations, configuration | `wyrd-spec` gateway/verification changes, generated JSON/Python/TypeScript declarations, SQL migrations/queries, package manifests, docs and architecture posture | Recorded `codegen:check`, typecheck, docs, dependency and tenancy evidence in task packets; source/export parity across the three SDKs | Generated shapes match their Rust sources in the candidate; removed gateway-keyring configuration has no surviving public alias. |

## Material findings

### MNT-001 — New fallible Jev provider methods do not meet the repository's rustdoc contract

- Changed locations:
  - `crates/skald/skald-providers/src/clients/jev.rs:136-138`
  - `crates/vala/vala-eval/tests/integration/orchestrator_judge_skald.rs:442-457`
- Governing rule: `AGENTS.md` section 16 and `architecture/agent-rules.md` require substantive rustdoc on every new or materially modified Rust item, including test helpers and trait methods; every fallible method requires `# Errors`, and remaining panics require `# Panics`. Missing rustdoc is explicitly a hard blocker.
- Evidence:
  - `JevClient`'s new `ProviderClient::send` returns `ProviderResult<ProviderResponse>` but its one-line rustdoc has no `# Errors` contract. A link to `send_native` does not satisfy the repository's explicit per-method requirement.
  - The new test `JevProvider::send` returns `Result` and can panic on the mutex and typed-answer invariants, but documents neither `# Errors` nor `# Panics`.
  - The new test `JevProvider::stream` always returns a `ProviderError`, but does not document its error behavior.
- Concrete maintenance cost: these are the concrete provider boundary exercised by the typed-Jev judge. A maintainer reading the trait implementation cannot determine locally whether failures are forwarded, impossible, or deliberately synthesized, and the test double's panic invariants are hidden. More importantly, accepting them establishes an exception to the repository's hard documentation gate precisely on the new provider boundary.
- Smallest testable correction: add substantive `# Errors` sections to all three methods, naming the forwarded `send_native` failures, the test sender's absence of an `Err` outcome, and the stream's deliberate timeout refusal; add `# Panics` to the test sender for the poisoned capture lock and invalid static typed answer. Do not change behavior, introduce helpers, or duplicate the provider implementation. `mise run fmt` and the focused Jev provider/Vala judge tests are sufficient proof for this documentation-only correction.

## Non-blocking maintainer calibration

- `crates/shared/wyrd-telemetry/src/lib.rs:275-276`, `crates/wyrd/wyrd-cli/tests/pg_tests.rs:643-644`, and `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:582-589` add function-scoped imports despite `architecture/agent-rules.md` requiring the module import block to expose dependencies. The simpler form is to move these imports to each module's existing top-level import block (with test-only `cfg` where required). This is an explicit standards issue, but by itself it has no behavioral, contract, or safety consequence, so it is not a material maintainer finding.
- The 687-line support workload is not a decomposition finding. Its generator, schemas, query inventory, oracle, and hand-counted checks share one cohesive test contract, and OD-6 explicitly requires that shared multi-state comparison. Splitting it would add navigation without separating an independently owned capability.
- The inbound and outbound trace task-locals are not duplicates: one bridges server request tasks and one bridges foreign-runtime client contexts. Collapsing them would mix process roles and make propagation ownership less clear.

## Verification assessment and limits

I inspected the complete diff and the recorded verification evidence in every task file. The evidence covers format, lints, code generation, Python and TypeScript typing, docs, dependency and tenant-isolation checks, focused owner tests, real-server SDK/MCP/CLI journeys, and the Bifrost data-state matrix. I independently ran `git diff --check`; it passed. I did not rerun the expensive Postgres-backed or full journey lanes in this maintainer pass; their command/result records remain evidence for the orchestrator and validation reviewer to corroborate.

The owner-held pre-release results-schema migration limit, null internal requester in a tenant without SYSTEM, and open observation-judge authority decision were not raised as findings. The candidate does not add a competing decision for them.

## Overall result

**FAIL**

The implementation is otherwise findable, owner-aligned, typed across its public projections, and materially simpler where OD-5 required deletion. `MNT-001` remains a hard repository documentation violation and must be corrected before acceptance.
