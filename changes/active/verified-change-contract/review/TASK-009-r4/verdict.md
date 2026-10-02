# TASK-009 round-four verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediations: TASK-009-R1, TASK-009-R2, and TASK-009-R3 in their respective review directories
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
| Nested, awaited, copied-task, sibling-task, and identical-Run concurrent scopes restore and isolate correlation | Execution-local token/prior stack; deterministic focused async and restoration cases | PASS |
| Missing/API-only/registration/attach/enrichment/detach failures remain fail-open without weakening explicit observations or user exceptions | Optional boundary containment and representative Drift/user-exception checks | PASS |
| Global and explicit private providers receive at most one Wyrd processor under every supported registration outcome | A duck-typed provider may retain the processor and then raise; the candidate clears its mark and hands over another processor on retry | **FAIL — `FIND-TASK-009-8`** |
| Runtime `Run.__exit__` names/defaults agree with owning and generated stubs and never suppress exceptions | PyO3 signature, stubs, public signature test | PASS |
| Authenticated stock OTLP/HTTP persists trace/custom/Eval joins with server-derived identity | Existing extended Python journey and independently rerun Postgres-backed proof | PASS |
| Context exit owns no flush, shutdown, exporter lifecycle, network IO, or durability acknowledgement | Exit source and separately ordered journey barriers | PASS |
| No server Run, second pipeline, wrapper span, process-global Card scope, required OTel dependency, client-managed identity, or log/metric promise entered the diff | Cumulative source and dependency inspection | PASS |
| Prior findings `FIND-TASK-009-1` through `FIND-TASK-009-7` are closed | Independent validation of source and focused proof | PASS |

## Independent results and follow-up

| Required role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | PASS |
| Invariants | `task-review-invariants.md` | FAIL — `INV-REV-R4-001` |
| Repository standards | `standards-review.md` | PASS |
| Maintainer | `maintainer-review.md` | PASS |
| System resilience | `system-review.md` | PASS |
| Telemetry domain | `domain-review-telemetry.md` | PASS |
| Concurrency domain | `domain-review-concurrency.md` | PASS |
| Focused follow-up | `followup-review.md` | RESOLVED — retain the ambiguous-registration proposal for validation |
| Fresh Ponytail validation | `findings-validation.md` | one revised, retained finding |

The follow-up was required because the invariant reviewer found a reachable
provider-registration transition that the other reviewers' stock-success and
raises-before-retention paths did not cover. It established that the approved
duck-typed private-provider hook admits a wrapper that retains a processor and
then fails, while TASK-009 requires at most one processor and does not require
registration retries. Independent validation retained and narrowed the issue.

## Validated finding ledger

| Stable ID | Discovery sources | Status / classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-009-8` | `INV-REV-R4-001`; focused follow-up | REVISED / INCORRECT | Preserve one weak, locked per-provider terminal outcome so success remains idempotently `True` and a failed or ambiguous attempt remains fail-open `False` without another processor handoff. |

The exact producer-to-consumer trace, reachability, correction boundary, and
focused proof are preserved in `findings-validation.md`.

## Prior-finding closure

| Prior ID | Result |
|---|---|
| `FIND-TASK-009-1` | CLOSED — every named optional failure reaches unchanged explicit-observation behavior. |
| `FIND-TASK-009-2` | CLOSED — Run subject documentation covers root, initial, and sibling views. |
| `FIND-TASK-009-3` | CLOSED — failed or swallowed detach restores the recorded prior value. |
| `FIND-TASK-009-4` | CLOSED — concurrent first entries publish one private OTel key. |
| `FIND-TASK-009-5` | CLOSED — runtime and stub exit signatures agree. |
| `FIND-TASK-009-6` | CLOSED — new Rust tests document panic conditions. |
| `FIND-TASK-009-7` | CLOSED — coordinated tasks entering the identical Run exit independently. |

## Verification and limits

Round-four reviewers independently reran the complete focused Python surface
(`35 passed`), the exact shared Rust selection tests (`2 passed`), and the real
authenticated Postgres/OTLP journey (`1 passed`, with migration check passing).
The orchestrator also reran the exact R3 test and full focused Python file and
confirmed cumulative `git diff --check`. The immutable implementation records
provide the broader Rust/Python/TypeScript, typing, generation, boundary,
format, and lint results; those broad lanes were inspected rather than all
rerun in this round.

Those limits do not excuse the retained defect: a direct source-free
reproduction against the supported duck-typed boundary returned `False` twice
while the same provider retained two Wyrd processors.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-009-8` is a bounded correction inside the existing Python OTel
owner and approved behavior. It requires no new product, public API,
architecture, security, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision.

Remediation: `TASK-009-R4-stabilize-provider-registration-idempotency.md`.
