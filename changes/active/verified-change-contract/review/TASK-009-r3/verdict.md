# TASK-009 round-three verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediations: `TASK-009-R1-restore-otel-correlation.md` and
  `TASK-009-R2-close-proof-and-boundary-parity.md`
- Scope: the complete cumulative base-to-candidate range.

The candidate remained `HEAD` throughout discovery, follow-up, validation, and
verdict preparation. The reviewed source was not modified. No `.codegraph/`
directory exists, so repository search and direct source inspection were used.
All required independent reports are present; no unavailable reviewer or
missing report was converted into a verification limit.

## Reconciled acceptance matrix

| Obligation | Reconciled source and proof | Result |
|---|---|---|
| Root-default and local initial Card selection use the shared Rust owner and mint one UUIDv7 invocation | `WyrdState::run`, `run_for_card`, `Run::new`; shared and language-surface tests | PASS |
| Immutable sibling views preserve the invocation while selecting exact hydrated Cards | `Run::for_card`; Rust, Python, and TypeScript selection assertions | PASS |
| Unknown aliases fail locally before Run construction or network IO | shared alias lookup and SDK negative cases | PASS |
| Rust, Python, and TypeScript expose the approved Run APIs with declaration parity | shared owner, thin SDK projections, generated Python/TypeScript declarations | PASS |
| Python entry returns the Run, attaches exact CardRef/Run ID, stamps an active span, and enriches child spans | `PyRun.__enter__`, `_enter_run`, `_RunCorrelationProcessor`; focused active/child proof | PASS |
| Nested scopes, `await`, task-context copying, sibling-view task isolation, and concurrent first key creation behave correctly | execution-local stack and locked key creation; focused async, nesting, and thread cases | PASS |
| Concurrent asyncio tasks entering the identical immutable `PyRun` cannot share one attach token | current async case enters two distinct sibling objects; no other test exercises same-object concurrent entry and interleaved exit | FAIL — `FIND-TASK-009-7` |
| Missing/API-only/registration/attach/enrichment/detach failures remain fail-open without blocking explicit observations | optional telemetry containment and focused representative Drift checks | PASS |
| Raising or swallowed detach restores the prior correlation | exact token/prior stack and same-context recovery proof | PASS |
| Global and private providers receive at most one processor; OpenTelemetry remains optional | locked weak registry, optional dependency, provider-focused tests | PASS |
| Runtime `Run.__exit__` names/defaults agree with owning and generated stubs and never suppress user exceptions | PyO3 signature, stubs, public signature test | PASS |
| Authenticated stock OTLP/HTTP journey persists trace/custom/Eval joins with server-derived identity | existing extended Python journey and recorded Postgres-backed proof | PASS |
| Context exit owns no flush, shutdown, exporter lifecycle, network IO, or durability acknowledgement | exit source and separately ordered journey barriers | PASS |
| No server Run, second pipeline, wrapper span, process-global Card scope, required OTel dependency, or client-managed identity entered the diff | cumulative source and dependency inspection | PASS |
| R1/R2 findings 1–6 are closed | independently validated source and focused proof | PASS |

## Independent results and follow-up

| Required role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | PASS |
| Invariants | `task-review-invariants.md` | PASS |
| Repository standards | `standards-review.md` | PASS |
| Maintainer | `maintainer-review.md` | PASS |
| System resilience | `system-review.md` | PASS |
| Telemetry domain | `domain-review-telemetry.md` | FAIL — `TEL-R3-001` |
| Concurrency domain | `domain-review-concurrency.md` | PASS |
| Focused follow-up | `followup-review.md` | RESOLVED — retain `TEL-R3-001` |
| Fresh Ponytail validation | `findings-validation.md` | one confirmed finding |

Follow-up was required because the telemetry and concurrency reports disagreed
about whether shared invocation identity was enough to prove AC-032's “same
immutable Run” case. Authority and source resolve the conflict: REQ-151 forbids
one attach token on the immutable Run, and AC-032 explicitly requires concurrent
tasks using that same Run. Distinct sibling `PyRun` objects cannot detect the
prohibited per-object design.

## Validated finding ledger

| Stable ID | Discovery source | Status / classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-009-7` | `TEL-R3-001` and focused follow-up | CONFIRMED / MISSING | Add focused proof that two coordinated asyncio tasks can enter the identical `PyRun`, interleave exits, preserve correlation only in the still-active task, and leave no correlation after both exit. |

Exact reachability, source locations, sibling-test tracing, consequence, and the
smallest safe correction are preserved in `findings-validation.md`.

## Prior-finding closure

| Prior ID | Result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — actual raising registration and every other named optional failure reach explicit Drift behavior. |
| `FIND-TASK-009-2` | CLOSED — Run subject documentation covers root, initial, and sibling views. |
| `FIND-TASK-009-3` | CLOSED — raising and swallowed detach restore the recorded prior value. |
| `FIND-TASK-009-4` | CLOSED — concurrent first entries publish one private OTel key. |
| `FIND-TASK-009-5` | CLOSED — runtime and stub exit signatures agree. |
| `FIND-TASK-009-6` | CLOSED — both new Rust tests document panic conditions. |

## Verification and limits

Independent discovery reran the complete focused Python surface with **34
passed**. The concurrency reviewer reran five targeted cases; the follow-up and
validator reran the current async case; cumulative `git diff --check` passed.
The immutable task and remediation evidence records the authenticated
Postgres-backed journey, broad Rust/Python/TypeScript suites, typing, codegen,
boundary, format, and lint lanes as passing. Those broad lanes were inspected
but not all independently rerun in this round.

This limit does not excuse the missing proof: none of the recorded green lanes
concurrently enters the identical Python Run object. The required correction is
test-only and needs no product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision.

## Verdict

**FIX_REQUIRED**

Remediation: `TASK-009-R3-prove-same-run-async-isolation.md` in this directory,
routed directly to `$wyrd-implement`.
