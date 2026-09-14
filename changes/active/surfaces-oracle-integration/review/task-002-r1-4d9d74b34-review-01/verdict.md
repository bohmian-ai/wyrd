# TASK-002-R1 Review Verdict

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Review worktree: `/tmp/wyrd-task-002-r1-review-4d9d74b34`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior review and remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/`

The complete cumulative base-to-candidate range was reviewed. The detached
candidate remained clean and fixed at the candidate commit throughout both
review waves. Later branch commits and uncommitted Forge changes were excluded.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Cards, `WyrdState`, and first-class Rust/Python/TypeScript SDK convergence under the shared `wyrd-client` owner | Cumulative source, generated surfaces, and recorded Card/state/language journeys | PASS |
| `Bifrost` is the sole public query/write/lifecycle facade | Public exports and production callers use `Bifrost`; sibling query types remain internal; focused compile-fail and lifecycle evidence | PASS |
| Query stream framing, exactly-one terminal, bounded Arrow ownership, settlement, and shutdown/drain behavior | Stream/durability trace plus focused failed-terminal, coalesced-chunk, and shutdown-race evidence | PASS |
| Shared deadline domain is `1..=u32::MAX` across Rust, HTTP/OpenAPI, gRPC, MCP, Python, and TypeScript | Shared validator/schema and recorded cross-surface boundary tests | PASS |
| Authentication, authorization, tenancy, credentials, audit, and refusal boundaries remain preserved | Security/RBAC/tenancy trace and recorded negative journeys | PASS |
| Stable client errors preserve common identity and structured metadata | Identity is centralized, but configuration and transport details are discarded | FAIL — `FIND-TASK-002-1` |
| Seven Bifrost OpenAPI operations agree with runtime typed problem responses | Status/schema coverage exists, but generated media types are `application/json` while runtime emits `application/problem+json` | FAIL — `FIND-TASK-002-3` |
| Every added or materially relocated Rust item satisfies repository rustdoc rules | Live implementation, boundary, helper, and test items remain undocumented | FAIL — `FIND-TASK-002-7` |
| Changed Rust signatures and fields use module-top imports and bare names | Live Cards and storage sites retain qualified type paths | FAIL — `FIND-TASK-002-8` |
| Public documentation teaches only current first-class SDK facades | The canonical reading guide names removed Python and TypeScript `BifrostQueryClient` APIs | FAIL — `FIND-TASK-002-14` |
| Non-goals remain excluded | No new client owner, transport, queue, decoder, exception hierarchy, feature hierarchy, dependency, or compatibility alias entered the candidate | PASS |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | FAIL | `REPO-R1-001`, `REPO-R1-002` |
| Public SDK/contracts domain | FAIL | `SDK-CONTRACT-R1-01`, `SDK-CONTRACT-R1-02`, `SDK-CONTRACT-R1-03` |
| Security/RBAC/tenancy domain | PASS | None |
| Stream lifecycle/durability domain | PASS | None |
| Structured Ponytail validation | FIX_REQUIRED | Five retained findings below |

## Validated finding ledger

| Stable ID | Status | Classification | Source IDs | Required correction boundary |
|---|---|---|---|---|
| `FIND-TASK-002-1` | REVISED / REOPENED | INCORRECT | `SDK-CONTRACT-R1-01` | Populate safe `Config` and `TransportDown` details in the existing canonical `WyrdClientError` projection and prove one foreign-runtime projection preserves them. |
| `FIND-TASK-002-3` | REVISED / REOPENED | INCORRECT | `SDK-CONTRACT-R1-02` | Declare `application/problem+json` for every non-success/default response on the seven existing Bifrost operations and regenerate/verify OpenAPI. |
| `FIND-TASK-002-7` | CONFIRMED / REOPENED | VIOLATION | `REPO-R1-001` | Finish substantive rustdoc across every added or materially relocated Rust item, including private/test items and required fallibility/cancellation notes. |
| `FIND-TASK-002-8` | CONFIRMED / REOPENED | VIOLATION | `REPO-R1-002` | Replace the validated qualified field/signature paths with imports in the existing module-top blocks and bare names. |
| `FIND-TASK-002-14` | CONFIRMED | REGRESSION | `SDK-CONTRACT-R1-03` | Rewrite the two canonical reading-guide snippets against the existing Python and TypeScript `Bifrost` facades; add no alias. |

Exact locations, caller traces, consequences, and closure proofs are preserved
in `findings-validation.md`. No proposed finding was softened into optional
advice, and none requires a specification revision.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1`, `FIND-TASK-002-3`, `FIND-TASK-002-7`, `FIND-TASK-002-8` | REOPENED |
| `FIND-TASK-002-2`, `FIND-TASK-002-4`, `FIND-TASK-002-5`, `FIND-TASK-002-6`, `FIND-TASK-002-9`, `FIND-TASK-002-10`, `FIND-TASK-002-11`, `FIND-TASK-002-12`, `FIND-TASK-002-13` | CLOSED |
| `FIND-TASK-002-14` | NEW |

## Verification limits

- Wave 2 independently established the five findings from source, callers,
  authorities, generated artifacts, and the cumulative diff; it did not rerun
  Cargo, Python, TypeScript, codegen, or docs lanes.
- The recorded green lanes cannot close source-proven metadata, media-type,
  rustdoc, qualified-signature, or stale-example gaps. `docs:check` does not
  compile fenced SDK snippets.
- One Python multipart-completion integration test failed once and passed on
  immediate full and focused reruns. The untouched storage path has no
  reproducible candidate cause, so this remains a limit rather than a finding.
- The task-prohibited Bifrost aggregate was not run and is not required for the
  bounded remediation.
- Cumulative `git diff --check` passed.

## Verdict

**FIX_REQUIRED**

The five validated findings are bounded corrections within approved behavior.
They require no new product, public API, architecture, security, compatibility,
cross-service, concurrency, resource-ownership, or persistent-data decision.

