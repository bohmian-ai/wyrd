# TASK-004 repository-standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Candidate identity was rechecked immediately before this report and remained unchanged.
- Scope: the complete cumulative base-to-candidate diff, including both remediation tasks and the owner-directed deletion in `29721b7e3` of the uncalled `VerifierRunQueue::new` and `ResultPublisher::endpoint` methods.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Active specification, task, remediation, and review artifacts | `AGENTS.md` §§11–12, 14 and 16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Reviewed the cumulative artifacts and their recorded focused/broad evidence. The candidate fails the required clean-diff proof because an earlier review artifact ends with an added blank line. **FAIL**. |
| Verification contracts, IDs, schemas, and public errors in `wyrd-spec` | `AGENTS.md` §§2, 3, 4, 8, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; approved specification revision 33 | Contracts remain typed and foundational; generated schemas and error catalog remain in the owning crate. Fresh `codegen:check` passed. **PASS**. |
| SYSTEM principal, JWT issuance/verification, public lifecycle refusal, tenant identity, and audit | `AGENTS.md` §§2, 3, 4, 9; `architecture/agent-rules.md` audit/tenancy rules; `architecture/wyrd-security-posture.md`; `architecture/wyrd-design.md` runtime identity/authz/audit; approved specification revision 33 | Existing identity/JWT owners are reused; the principal is internal and tenant-bound; authorization audit stays on the canonical path. Fresh tenancy/unwrap boundary checks passed, and the cumulative candidate contains direct unit, Postgres, HTTP, Gate, and gRPC coverage. **PASS**. |
| `wyrd-sql` verifier-run and dispatch control persistence and migrations | `AGENTS.md` §§3–6, 9, 11, 16; `architecture/agent-rules.md` `TenantConn`, `OperatorPool`, transaction, import, and test-location rules; `architecture/references/languages/rust-core.md`; approved specification revision 33 | Tenant operations use caller-owned `TenantConn`; cross-tenant discovery uses `OperatorPool`; lifecycle state is durable and struct-owned. Fresh `check:from-pools-allowlist` and `check:tenant-isolation` passed. **PASS**. |
| Supervised runtime, scheduler, runner, permits, publication, results, health, metrics, shutdown, and server composition | `AGENTS.md` §§5, 6, 9–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/evaluation.md`; `architecture/references/domain/analytical-operations-reliability.md`; approved specification revision 33 | Stateful workflows have concrete owners, concurrency is bounded, scheduler commit ordering has an explicit linearization point, and result settlement remains ACK-gated. Recorded R1/R2 focused Postgres and role-separated proofs cover the corrected races and publication seams. **PASS**. |
| Bifrost Gate/Scribe result admission, exact Verifier scope, result schemas, audit projection, and Oracle journey | `AGENTS.md` §§2, 3, 9–11; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; approved specification revision 33 | Reserved tables are admitted only for the tenant SYSTEM writer with exact signed Verifier scope; named-field payload assembly rejects schema mismatch; publication uses `wyrd_client::Bifrost` and Scribe ACKs. Recorded Gate/gRPC/role-separated journey evidence directly covers these boundaries. **PASS**. |
| HTTP/OpenAPI, shared Rust client, Rust SDK, MCP, Python/PyO3, and TypeScript/N-API projections | `AGENTS.md` §§2, 3, 7–11, 16; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/errors.md` | Durable behavior stays server-owned; language surfaces project the shared client; write surfaces retain permissions; generated Python stubs are owned by the SDK. Fresh `check:client-tier`, `check:pyo3-scope`, and `codegen:check` passed. Recorded Rust/Python/TypeScript/MCP journeys cover the public path. **PASS**. |
| Deployment shutdown budgets, self-hosting docs, `mise` lanes, and fixtures | `AGENTS.md` §§11–12; `architecture/references/languages/testing-workflows.md`; `architecture/references/domain/analytical-operations-reliability.md`; task verification contract | Deployment grace exceeds the runtime's 30-second drain; docs expose the changed configuration; real-server/Postgres fixtures stay in integration/journey lanes. The candidate-wide whitespace gate fails as described below. **FAIL** only for `REPO-1`. |
| Commit `29721b7e3` dead-code deletion | `AGENTS.md` §5 abstraction rules and §16 documentation; Ponytail/YAGNI requirement; public-surface ownership rules | Repository-wide caller search finds no call to either removed method. `VerifierRunQueue` is constructed through its existing `Default` implementation, while `ResultPublisher` still owns and uses its private endpoint during publication. Removing the unused public methods reduces unsupported surface without changing behavior or weakening rustdoc on retained items. **PASS**. |

## Applicable-rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable server behavior and typed wire contracts remain in their owning Rust crates; SDKs are projections over `wyrd-client`. | `crates/wyrd-spec/src/verification.rs`, `crates/wyrd/wyrd-server/src/components/verification/`, `crates/shared/wyrd-client/src/verification.rs`, and the three SDK projections. Fresh client-tier and PyO3-scope checks passed. | PASS |
| `wyrd-spec` remains IO-, async-, and PyO3-free; generated artifacts must regenerate cleanly. | No runtime dependency was introduced into `wyrd-spec`; fresh `mise run codegen:check` passed. | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; callers own tenant transaction lifecycle. | Runtime owners hold `WyrdPostgres`/`OperatorPool`, open tenant transactions at the orchestration boundary, and queue methods do not commit caller-owned connections. Fresh pool-allowlist and tenant-isolation checks passed. | PASS |
| Authorization decisions use the canonical audit path and engine mechanics do not add auth audit rows. | Gate combines result-table authorization and exact `card_ref` scope into one decision/audit append; SYSTEM minting and run mechanics do not emit authorization audit. Recorded cardinality tests cover allow and deny paths. | PASS |
| Stateful Rust workflows are struct-centered and async is limited to IO/composition. | `VerificationRuntime`, `VerificationScheduler`, `VerifierRunner`, `ResultPublisher`, `VerifierRunQueue`, `VerificationService`, and language handles own their dependencies and workflows. Pure payload construction and contract validation remain synchronous. | PASS |
| Public/fallible Rust items have intent and error/cancellation documentation; top-level imports and bare signature types are required. | R1/R2 corrected the cumulative cited source shape; inspection of the final cumulative diff found no remaining material violation. The two methods removed in `29721b7e3` had no callers, so their removal does not create a documentation gap. | PASS |
| Bifrost result publication is bounded, tenant-scoped, exact-scope authorized, name-mapped, and ACK-gated. | Gate, result builder, publisher, runner, and recorded direct/role-separated tests cover refusal, correlation identity, ambiguous replay, crash/reclaim, and detail-before-summary settlement. | PASS |
| Every user/agent-facing surface has real client-to-server journey coverage. | Rust, Python, TypeScript, MCP, HTTP/Postgres, and role-separated Bifrost journey tests are present and recorded green in the cumulative task/remediation evidence. | PASS |
| Public Python and TypeScript boundaries remain thin, registered, exported, typed, and generated consistently. | SDK-owned PyO3/N-API modules call the shared Verification handle; fresh PyO3/client boundary and codegen checks passed; recorded typecheck/integration lanes are green. | PASS |
| Verification must include format/lints, targeted lanes, boundary checks, codegen, and `git diff --check`; failures cannot be waived as pre-existing. | Targeted and broad lanes are recorded green and fresh static/codegen checks passed, but `git diff --check <base> <candidate>` reports an added blank line at EOF. | **FAIL** |
| No unrelated or speculative API remains solely for future use. | Caller tracing confirms the final commit deletes two uncalled methods and preserves the fields/behavior actually used. No replacement abstraction or compatibility path was added. | PASS |

## Material repository-rule findings

### `REPO-1` — candidate-wide clean-diff gate fails

- **Violated rule:** `AGENTS.md` §11 and §12 require `git diff --check` and all targeted gates to pass before completion; pre-existing failure inside the reviewed range is not a waiver.
- **Location:** `changes/active/verified-change-contract/review/TASK-004-r2/findings-validation.md:160`
- **Evidence:** `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8 29721b7e33854633b025b25948fd5d2eaebe7bfd` reports `new blank line at EOF` at line 160.
- **Consequence:** the cumulative candidate does not satisfy its mandatory repository verification contract and cannot receive a repository-standards pass even though the executable and boundary evidence is otherwise credible.
- **Testable correction:** delete only the final blank line from that review artifact and rerun the exact base-to-candidate `git diff --check`; it must exit zero with no output. No production code, API, test, or new check is required.

## Verification notes and limits

Freshly executed against the immutable candidate, all successful unless noted:

- `mise run check:from-pools-allowlist`
- `mise run check:tenant-isolation`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`
- `mise run codegen:check`
- candidate-wide `git diff --check` (**failed only with `REPO-1`**)
- repository-wide caller searches for both owner-directed deleted methods (no callers)

The review did not rerun the long Postgres, Bifrost, SDK, MCP, full Clippy, or full journey lanes. It inspected their tests and relied on the command-level green evidence retained in the original task and both remediation artifacts. That is a verification limit, not a second finding: the candidate's last source commit removes only two uncalled accessors and does not alter executable paths, while the fresh boundary/codegen checks remain green.

## Overall result

**FAIL**

The cumulative implementation conforms to the applicable architecture, ownership, security, tenancy, audit, runtime, client, SDK, MCP, generated-artifact, and dead-code standards inspected here. One bounded repository-rule defect remains: `REPO-1` prevents the mandatory candidate-wide `git diff --check` proof from passing.
