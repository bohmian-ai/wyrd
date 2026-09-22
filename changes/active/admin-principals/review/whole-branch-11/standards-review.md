# Repository Standards Review — cumulative R9 + R10

Immutable subject: `c5c20754a167e8f4d74a555a720bd51df6179a6f..41e60be61958c92f562fceb5b7a03f40f611bbc8`

## Review Findings

### Critical

None.

### Important

- **REPO-R9R10-1 — [`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1469`](../../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs), [`crates/shared/wyrd-auth-issue/src/lib.rs:730`](../../../../../../crates/shared/wyrd-auth-issue/src/lib.rs), [`crates/shared/wyrd-auth-verify/src/lib.rs:1085`](../../../../../../crates/shared/wyrd-auth-verify/src/lib.rs):** New test helpers and test functions that can panic document their scenario but omit the mandatory `# Panics` contract; examples include `seed_actor`, `a_policy_denied_exchange_commits_one_denied_decision`, `issue_access_token_delegated_names_subject_and_outer_actor`, and `into_verified_keeps_subject_and_orders_actors_earliest_first`, all of which call `expect!`, `panic!`, or assertions. This violates `AGENTS.md:678-692`, which explicitly applies complete rustdoc to private items, test helpers, and test functions and makes missing required sections a hard blocker; the recorded `cargo doc -D missing_docs` run cannot detect private/test-item omissions. Complete a diff-based inventory of every new or materially modified Rust item in R9 and R10, add substantive rustdoc plus `# Errors` and `# Panics` where the item can return an error or panic, and rerun both the all-item audit and strict rustdoc.

- **REPO-R9R10-2 — [`sdks/wyrd-sdk-python/tests/unit/client/test_client.py:20`](../../../../../../sdks/wyrd-sdk-python/tests/unit/client/test_client.py), [`sdks/wyrd-sdk-ts/wyrd/tests/unit/wyrd-client.test.ts:7`](../../../../../../sdks/wyrd-sdk-ts/wyrd/tests/unit/wyrd-client.test.ts):** The new public Python `on_behalf_of` and TypeScript `onBehalfOf` capabilities have only unit tests against an unreachable port, so neither runtime proves a successful delegation exchange through its public SDK against a real server. `AGENTS.md:378-420`, `architecture/wyrd-design.md:218-231,247-265`, `python-api-and-stubs.md:83-104`, and `typescript-guide.md:185-194` require one client→server→client user journey for every first-class SDK surface and state that unit tests do not substitute. Add gated Python and TypeScript integration journeys that import the public SDK, use the existing `WyrdTestServer`/repository Postgres harness to seed subject A, actor B, and the invoke policy, successfully call the helper, and prove an observable allowed and refused delegation outcome through that language runtime.

- **REPO-R9R10-3 — [`architecture/references/languages/typescript-guide.md:171`](../../../../../../architecture/references/languages/typescript-guide.md), [`sdks/wyrd-sdk-ts/native/src/client.rs:15`](../../../../../../sdks/wyrd-sdk-ts/native/src/client.rs):** The focused TypeScript authority still says the N-API binding “must not assemble a … raw `WyrdClient`,” while the approved design and implementation now intentionally expose a thin `NativeWyrdClient` over the single shared `wyrd_client::WyrdClient`. Leaving an applicable repository authority contradicted by the accepted higher-level client model makes future conformance checks ambiguous and violates the requirement to keep changed public surfaces and architecture guidance aligned. Update the TypeScript guide narrowly to allow a thin projection of the shared `WyrdClient` for shared auth/delegation while retaining the prohibition on a separate `QueryClient`, duplicated transport, or per-call gRPC transport; then run the documentation checks.

- **REPO-R9R10-4 — [`mise.toml:19`](../../../../../../mise.toml), [`changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md:279`](../whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md):** R10 changes shared repository tooling by pinning Python, setting global `UV_PYTHON`, and rewriting many audit, docs, codegen, and test tasks to execute through `uv run python`, but the final evidence contains only selected component lanes and no `mise run gate`. `AGENTS.md:483-487` requires the aggregate gate for shared CI/build/test infrastructure changes; selected passing leaves do not prove that the changed task graph composes correctly. Run `mise run gate` additionally on the immutable final candidate (not as a substitute for the named focused lanes) and record its final-tree result, or revert the global tooling change if it is not required by the product change.

### Suggestions

None.

## Authority Coverage

CodeGraph was used before text/source inspection. Every selected focused reference was read completely, and its linked governing authority was applied under the hierarchy in `architecture/references/README.md`.

| Changed surface | Applicable authority inspected | Coverage result |
|---|---|---|
| Repository workflow, task evidence, build/test tooling | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md`; `languages/testing-workflows.md` | Complete; final aggregate-gate proof fails (REPO-R9R10-4) |
| Rust auth contracts, issuance, verification, middleware, and client owner | `AGENTS.md` §§4-6, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | Complete; structural ownership passes, item-documentation rule fails (REPO-R9R10-1) |
| Principal, credential, grant, platform, tenant, audit, and SQL paths | `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `doctrine/architecture-constraints.md`; `languages/errors.md` | Complete; tenant capability, typed identity, fail-closed audit, and stable-error shapes conform in inspected paths |
| HTTP/OpenAPI/MCP/CLI public contracts | `AGENTS.md` §§9, 11; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `languages/agent-harness.md`; `languages/errors.md`; `architecture/patterns.md` | Complete; typed contract/error ownership and secret handling conform |
| Bifrost HTTP/gRPC audience admission and delegated query journey | `architecture/bifrost-design.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `domain/vala-architecture.md`; `domain/olap-serving.md`; `doctrine/architecture-constraints.md` | Complete; Bifrost audience binding and existing Rust journey conform |
| Shared Rust client and Rust SDK | `AGENTS.md` §§2-3, 9, 11; `architecture/wyrd-design.md` client model; `architecture/patterns.md`; `languages/rust-core.md`; `languages/testing-workflows.md` | Complete; the shared owner and Rust journey conform |
| PyO3 and public Python SDK | `AGENTS.md` §§7-8, 11; `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `languages/testing-workflows.md`; `languages/errors.md` | Complete; thin binding, export, typing, and error projection pass, but journey coverage fails (REPO-R9R10-2) |
| N-API and public TypeScript SDK | `AGENTS.md` §§2-3, 11; `languages/typescript-guide.md`; `languages/testing-workflows.md`; `languages/errors.md`; `architecture/wyrd-design.md` client model | Complete; thin binding passes under the governing design, but journey coverage and focused-authority synchronization fail (REPO-R9R10-2/3) |
| Architecture, security, generated docs, and docs site | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/references/README.md`; `AGENTS.md` | Complete; main authorities describe the new flow, but the TypeScript focused guide remains stale (REPO-R9R10-3) |

## Applicable Rule Results

| Rule | Repository/source evidence | Result |
|---|---|---|
| Server owns durable auth behavior; SDKs project the shared Rust client | `crates/shared/wyrd-client/src/client.rs:293`; `sdks/wyrd-sdk-python/src/client.rs:58`; `sdks/wyrd-sdk-ts/native/src/client.rs:15-20,78`; no Python/TS exchange implementation | PASS |
| RFC subject/actor claims and audience are typed contracts rather than language-specific shapes | `crates/wyrd-spec/src/auth/token.rs`; `crates/shared/wyrd-auth-issue/src/lib.rs:730-758`; `crates/shared/wyrd-auth-verify/src/lib.rs:1085-1128` | PASS |
| Bifrost-scoped tokens are verified only on Bifrost surfaces | `crates/wyrd/wyrd-server/src/http/router.rs:63`; `crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs:38-43`; `crates/vala/vala-bifrost-redux/src/gate/auth.rs:119` | PASS |
| Authorization/audit attributes use typed credential identity and preserve the single audit path | `crates/wyrd/wyrd-auth/src/issuance.rs:567-609`; `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:304,418`; `crates/wyrd/wyrd-server/src/audit/mod.rs:57` | PASS |
| CLI secrets do not enter argv and secret-bearing values use `SecretString` | `crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:33,136,260-280`; `crates/wyrd/wyrd-cli/src/auth/refresh.rs:35-38` | PASS |
| New stateful workflows have one concrete owner and do not introduce speculative traits | `ExchangeApiKey`, `TokenVerifier`, `IssuingKey`, and `WyrdClient` remain the concrete owners; no new delegation framework/trait was added | PASS |
| Public Python/PyO3 and TypeScript/N-API layers are thin and generated/exported artifacts are checked | Candidate evidence records `py:typecheck`, `ts:build`, `ts:typecheck`, `codegen:check`, and `check:pyo3-scope` passing on the final candidate | PASS |
| Every new or materially modified Rust item has complete rustdoc, including `# Errors`/`# Panics` where applicable | `AGENTS.md:678-692` versus the new panic-capable helpers/tests cited in REPO-R9R10-1 | **FAIL** |
| Every first-class language surface for a user-facing capability has a real user journey | Only Rust has `query::service_b_acts_for_service_a_with_only_a_table_authority`; Python and TypeScript contain only unreachable-port unit tests | **FAIL** |
| Applicable architecture/reference guidance remains synchronized with the changed public surface | Governing `wyrd-design.md:247-265` permits the shared client projection; `typescript-guide.md:182-183` still forbids it | **FAIL** |
| Verification matches scope; shared CI/build/test changes run the aggregate gate | `mise.toml:19-39,821-872,1115-1141` changes global tooling/tasks; R10 evidence at lines 279-315 omits `mise run gate` | **FAIL** |
| Generated artifacts, format, lints, boundary checks, focused auth tests, docs, and whitespace are evidenced on the candidate | R9/R10 evidence tables record the exact focused selectors and passing owning lanes; `git diff --check` and strict public rustdoc are recorded clean | PASS, subject to the four failed rules above |

## Open Questions

None. The failures are resolvable from existing repository authority and do not require a new product or architecture decision.

## Verification Notes

- Reviewed the complete immutable base-to-candidate diff rather than only the final commits or implementation summary.
- R9 and R10 evidence records passing format, lint, Python/TypeScript unit/type/build lanes, boundary checks, principals/auth/Bifrost/SQL tests, code generation, docs, public rustdoc, and whitespace checks.
- The approved five-minute access-token revocation window was excluded from findings as directed.
- The claimed inability of the present Python/TypeScript harnesses to seed two services and policy does not waive the explicit journey requirement; extending the existing integration fixture is part of proving the shipped public surface.
- `cargo doc -D missing_docs` establishes public-item documentation only; it is not evidence for the repository's stricter private/test-item contract.

## Overall Result

**FAIL** — four bounded repository-standard findings remain: `REPO-R9R10-1`, `REPO-R9R10-2`, `REPO-R9R10-3`, and `REPO-R9R10-4`.
