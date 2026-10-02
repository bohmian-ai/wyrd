# TASK-009 round-five verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, and TASK-009-R4E; R4E supersedes TASK-009-R4
- Scope: complete cumulative base-to-candidate range

The candidate remained `HEAD` throughout discovery, focused follow-up,
validation, and verdict preparation. The reviewed source was not modified. No
`.codegraph/` directory exists, so repository search and direct source
inspection were used. Every required independent report is present; no missing
reviewer or report was converted into a verification limit.

## Reconciled acceptance matrix

| Obligation | Reconciled source and proof | Result |
|---|---|---|
| Root-default and local initial Card selection use the shared Rust owner and mint one UUIDv7 invocation | `WyrdState::run`, `run_for_card`, `Run::new`; shared and language-surface tests | PASS |
| Immutable sibling views preserve the invocation while selecting exact hydrated Cards; unknown aliases fail locally | `Run::for_card`, shared alias lookup, Rust/Python/TypeScript assertions | PASS |
| Rust, Python, and TypeScript expose the approved Run APIs with declaration parity | Shared owner, thin projections, generated declarations, typing and journey evidence | PASS |
| Python entry returns the Run and applies exact CardRef/Run ID to active and child spans | PyO3 boundary, `_enter_run`, `_RunCorrelationProcessor`; focused and persisted proof | PASS |
| Nested, awaited, copied-task, sibling-task, and identical-Run concurrent scopes restore and isolate correlation | Import-time OTel key and tuple stack; deterministic focused async and restoration cases | PASS |
| Missing/API-only/registration/attach/enrichment/context-update failures remain fail-open without weakening explicit observations or user exceptions | Optional-boundary containment and representative Drift/user-exception checks | PASS |
| Registration is attempted at most once and cached independently for each provider identity | Equality-keyed weak outcomes alias distinct live providers that compare equal | **FAIL — `FIND-TASK-009-9`** |
| A provider whose registration raises, including after retaining the processor, receives no enrichment | The retained shared processor remains active after an accept-then-raise result of `False` | **FAIL — `FIND-TASK-009-10`** |
| Runtime `Run.__exit__` names/defaults agree with owning and generated stubs and never suppress exceptions | PyO3 signature, stubs, public signature test | PASS |
| Authenticated stock OTLP/HTTP persists trace/custom/Eval joins with server-derived identity | Existing extended Python journey and immutable implementation evidence | PASS |
| Context exit owns no flush, shutdown, exporter lifecycle, network IO, or durability acknowledgement | Exit source and separately ordered journey barriers | PASS |
| Changed architecture and task artifacts identify their current approved revision and lifecycle state | `run_api.md` still claims revision 34; superseded TASK-009-R4 remains `ready` | **FAIL — `FIND-TASK-009-11`, `FIND-TASK-009-12`** |
| Every named Rust test has exact, repository-native, replayable command evidence | The task records a regex-selected bare Cargo command and omits the full exact journey command | **FAIL — `FIND-TASK-009-13`** |
| No server Run, second pipeline, wrapper span, process-global Card scope, required OTel dependency, client-managed identity, or log/metric promise entered the diff | Cumulative source and dependency inspection | PASS |
| Prior findings `FIND-TASK-009-1` through `FIND-TASK-009-8` are closed | Independent validation of source and focused proof | PASS |

## Independent results and follow-up

| Required role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | PASS |
| Invariants | `task-review-invariants.md` | PASS |
| Repository standards | `standards-review.md` | FAIL — `REPO-R5-001`, `REPO-R5-002` |
| Maintainer | `maintainer-review.md` | FAIL — `MAINT-009-1` |
| System resilience | `system-review.md` | FAIL — `SYS-R5-001` |
| Telemetry domain | `domain-review-telemetry.md` | FAIL — `TEL-R5-001` |
| Concurrency domain | `domain-review-concurrency.md` | PASS |
| Focused follow-up | `followup-review.md` | RESOLVED |
| Fresh Ponytail validation | `findings-validation.md` | five retained findings |

The focused follow-up was required because the discovery reports disagreed on
provider identity semantics, accept-then-raise enrichment, and whether the
artifact-state issues were material. It confirmed all five reachable paths.
Independent validation resolved the only authority disagreement: revision 46
already requires no enrichment after failed registration, so the telemetry
defect is a bounded implementation correction rather than a specification
revision.

## Validated finding ledger

| Stable ID | Discovery sources | Status / classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-009-9` | `SYS-R5-001`; focused follow-up | CONFIRMED / INCORRECT | Cache registration outcomes by weak provider identity, not referent equality, so distinct equal providers are each attempted once. |
| `FIND-TASK-009-10` | `TEL-R5-001`; focused follow-up | REVISED / INCORRECT | Keep a processor retained by a failed registration inert; activate only the processor belonging to a normally completed registration. |
| `FIND-TASK-009-11` | `MAINT-009-1`; focused follow-up | CONFIRMED / VIOLATION | Identify revision 46 as the approval level of the changed Run authority. |
| `FIND-TASK-009-12` | `REPO-R5-001`; focused follow-up | CONFIRMED / VIOLATION | Mark replaced TASK-009-R4 as `superseded`. |
| `FIND-TASK-009-13` | `REPO-R5-002`; focused follow-up | CONFIRMED / VIOLATION | Record exact `mise exec -- cargo nextest` proof for each named Rust test, including the journey's complete environment wrapper. |

The exact producer-to-consumer trace, reachability, correction boundary, and
focused closure proof are preserved in `findings-validation.md`.

## Prior-finding closure

| Prior ID | Result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — optional OTel failures remain contained while explicit observations keep their ordinary strict behavior. |
| `FIND-TASK-009-2` | CLOSED — Run subject documentation covers root, initial, and sibling views. |
| `FIND-TASK-009-3` | SUPERSEDED AND CLOSED — revision 46 replaced token/detach recovery with the approved tuple stack. |
| `FIND-TASK-009-4` | CLOSED — the private OTel key is created once at import. |
| `FIND-TASK-009-5` | CLOSED — runtime and generated stub exit signatures agree. |
| `FIND-TASK-009-6` | CLOSED — both shared Rust selection tests document their panic conditions. |
| `FIND-TASK-009-7` | CLOSED — coordinated tasks entering the identical Run isolate and exit independently. |
| `FIND-TASK-009-8` | CLOSED — each provider receives at most one processor handoff; finding 10 concerns the behavior of the already-retained failed processor. |

## Verification and limits

The complete focused Python surface was independently rerun and passed (`37
passed`). The cumulative `git diff --check` was clean. The immutable task and
remediation records contain broader Rust, Python, TypeScript, journey, typing,
generation, boundary, format, and lint results; those broad lanes were
inspected rather than rerun in this review.

The provider failures are established directly from reachable source paths and
standard-library weak-key behavior. The command-evidence violation is itself a
limit on replayable Rust proof and is retained as a finding rather than used to
excuse the candidate.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-009-9` through `FIND-TASK-009-13` are bounded corrections within
the approved revision-46 behavior and its review packet. They require no new
product, public API, architecture, security, compatibility, cross-service,
concurrency-semantics, resource-ownership, or persistent-data decision.

Remediation: `TASK-009-R5-provider-identity-and-proof-closure.md`.
