# TASK-002 Review Verdict

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Review attempt: `task-002-f66a33769-review-01`

The candidate commit remained unchanged throughout both review waves. Review artifacts are untracked additions and are not part of the reviewed candidate.

## Acceptance matrix

| Obligation group | Result | Evidence |
|---|---|---|
| One `wyrd-client` implementation and thin Rust, Python, and TypeScript SDK roots | PASS | Deleted sibling client crates, SDK manifests and exports, and passing client-tier/PyO3 ownership checks establish the topology. |
| Bifrost facade is the sole public owner of table, query, stream, and lifecycle operations | FAIL | `FIND-TASK-002-9`: public `QueryClient`/`RawQueryStream` and `query_client()` remain the production path for CLI and both foreign SDKs. |
| Oracle bounded queue ownership, shutdown drain, and terminal-safe streaming survive convergence | FAIL | `FIND-TASK-002-10`, `FIND-TASK-002-11`, and `FIND-TASK-002-12` identify reachable admission/drain, terminal, and decoded-batch ownership violations. |
| Cards, registration, loading, and `WyrdState` remain coherent across public surfaces | FAIL | Core journeys pass, but `FIND-TASK-002-1` misclassifies shared client failures in Cards. |
| Python exposes one catalog-backed eight-field `WyrdError` and the authority-defined module layout | FAIL | The exception projection itself passes; `FIND-TASK-002-5` records the missing `wyrd.errors` module and `FIND-TASK-002-1` records incorrect Cards metadata. |
| TypeScript exposes runtime structured failures through `@wyrd/sdk` | FAIL | `FIND-TASK-002-2` shows connect and describe paths still throw display-only napi errors. |
| Public Bifrost HTTP/OpenAPI/generated contracts are complete and consistent | FAIL | `FIND-TASK-002-3` omits typed refusal bodies and `FIND-TASK-002-13` leaves deadline domains inconsistent. |
| PyO3 remains behind the Python SDK boundary feature | FAIL | `FIND-TASK-002-6`: PyO3 and owner-crate Python features are unconditional. |
| Materially relocated Rust follows repository documentation and signature rules | FAIL | `FIND-TASK-002-7` and `FIND-TASK-002-8`. |
| MCP discovers exactly three Bifrost tools and CLI query behavior remains intact | PASS | Recorded exact MCP discovery and CLI journey evidence passed; no validated finding challenges those behaviors. |
| Five Card/CLI/WyrdState Postgres lanes own the canonical isolated lifecycle | PASS | All five recorded lanes and `test:postgres:inventory` passed. |
| SDK relocation leaves documentation and examples usable | FAIL | `FIND-TASK-002-4` causes deterministic prerender failures on two how-to routes. |
| Non-goals remain excluded | PASS | No legacy typed reads, audit verification, bootstrap path, compatibility client, UI query workspace, merge, push, deploy, or Bifrost aggregate entered the subject. |

## Wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TASK-REV-001` through `TASK-REV-004` |
| Repository standards | FAIL | `STD-001` through `STD-007` |
| Security/RBAC/tenancy | FAIL | `SEC-001` through `SEC-003` |
| Stream lifecycle/durability | FAIL | `STREAM-001` through `STREAM-004` |
| Public SDK/contracts | FAIL | `SDK-CONTRACT-01` through `SDK-CONTRACT-03` |
| Structured Ponytail validation | FIX_REQUIRED | Thirteen retained findings; `STD-005`, `SDK-CONTRACT-03`, and `STD-007` rejected |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-002-1` | CONFIRMED | INCORRECT | Canonically preserve `WYRD_CLIENT_*` identity for shared Cards/client failures. |
| `FIND-TASK-002-2` | CONFIRMED | INCORRECT | Project TypeScript connect/describe failures as runtime `WyrdError`. |
| `FIND-TASK-002-3` | REVISED | MISSING | Publish typed `WyrdProblem` refusal responses for all seven Bifrost OpenAPI operations. |
| `FIND-TASK-002-4` | REVISED | REGRESSION | Make relocated Python examples resolvable and restore both broken docs routes. |
| `FIND-TASK-002-5` | CONFIRMED | VIOLATION | Restore only the canonical `wyrd.errors.WyrdError` projection. |
| `FIND-TASK-002-6` | CONFIRMED | VIOLATION | Put relocated PyO3 and owner Python features behind one optional `python` feature. |
| `FIND-TASK-002-7` | CONFIRMED | VIOLATION | Complete required rustdoc on added/materially relocated Rust items. |
| `FIND-TASK-002-8` | CONFIRMED | VIOLATION | Use module-top imports and bare names in relocated signatures. |
| `FIND-TASK-002-9` | CONFIRMED | VIOLATION | Make `Bifrost` the only public query/lifecycle facade. |
| `FIND-TASK-002-10` | CONFIRMED | INCORRECT | Serialize close with producer lookup/creation so shutdown drains or refuses every racing insert. |
| `FIND-TASK-002-11` | CONFIRMED | INCORRECT | Require clean EOF after failed terminals and reject all post-terminal frames. |
| `FIND-TASK-002-12` | REVISED | VIOLATION | Decode at most one pending Arrow batch regardless of HTTP chunk coalescing. |
| `FIND-TASK-002-13` | REVISED | INCORRECT | Enforce the same `1..=u32::MAX` deadline domain across all public surfaces and generated schema. |

The complete evidence, caller traces, corrections, and closure proofs are in `findings-validation.md`.

## Verification limits

- The review relied on the implementation evidence recorded in the original task and independently inspected the immutable source and diff.
- Wave 2 additionally passed `mise run py:format:check`, `mise run py:lints`, and `git diff --check 861f8d86c..f66a33769`.
- The task intentionally prohibited running the Bifrost aggregate lane; it was not run.
- Passing codegen established artifact consistency, not completeness of omitted error responses; passing journeys did not cover the confirmed race and malformed-stream cases.
- No live cloud-storage qualification was required or run.

## Prior-finding closure

This is the first review of TASK-002. There are no prior `FIND-TASK-002-*` findings to close.

## Verdict

**FIX_REQUIRED**

All retained findings are bounded by approved authority and existing owners. None requires a specification revision. Remediation is defined in `TASK-002-R1-close-review-findings.md`.
