# TASK-009 round-five findings validation

## Immutable subject and validation coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, and TASK-009-R4E; R4E supersedes TASK-009-R4

The candidate matched `HEAD` before and after validation. The repository has no
`.codegraph/` directory, so direct source and caller inspection was used. All
required round-five discovery reports and the focused follow-up were present
and read. This validation inspected the complete cumulative diff, the prior
validated ledgers and remediation tasks, the applicable repository and Run
authorities, every proposed finding, its cited source, its callers and sibling
consumers, and the recorded verification evidence. The independently reported
focused Python result was 37 passing tests; the cumulative `git diff --check`
was clean.

## Producer-to-consumer trace

Shared Rust remains the single Run identity producer. `WyrdState::run` and
`run_for_card` select the root or hydrated alias before `Run::new` mints one
invocation ID. `Run::for_card` creates immutable sibling views that retain that
ID and replace only the exact subject. Rust, Python, and TypeScript project
those owners without another lookup, transport, lifecycle, or durable Run.

Python `PyRun.__enter__` and `__exit__` pass the view's exact CardRef and Run ID
to `wyrd.otel`. The OTel module stores only the tuple stack in one import-time
context key. Its processor reads the innermost pair from the span's parent
context, and the existing authenticated journey carries those record-level
attributes through Gate, Scribe, and Bifrost while server authority supplies
tenant, publisher, and Card UID. Explicit Drift, Eval, and dataset observations
remain sibling consumers of the native Run and state-owned Bifrost facade; they
do not depend on ambient processor success.

Provider registration has two distinct producer defects. First,
`weakref.WeakKeyDictionary` uses referent hash/equality, so two live provider
objects that compare equal share one cached outcome even though the public hook
accepts them as distinct provider instances. The second object can therefore
skip registration and emit uncorrelated spans. A direct standard-library check
confirmed that two distinct equal objects address one `WeakKeyDictionary`
entry. Second, an accept-then-raise provider retains the shared processor while
the cache remains `False`. That processor has no registration-specific state
and later stamps spans normally, contradicting the approved failed-provider
outcome even though retry prevention itself is correct.

The latter mismatch does not require a new product or architecture decision.
Revision 46 is explicit that a provider raising after acceptance gets no
enrichment. The spec outranks remediation mechanics. R4E says one shared
stateless processor is *acceptable*, not required; that allowance cannot
override the approved observable result. The smallest correction stays in the
existing Python owner: hand each attempted provider one initially inactive
processor, activate it only after `add_span_processor` returns normally, and
keep failed retained processors inert. This changes no public API, provider
lifecycle, cross-service contract, persistence, or concurrency semantics.

## Proposed-finding decisions

### `SYS-R5-001` — CONFIRMED as `FIND-TASK-009-9`

- **Reachability:** `install_run_correlation(provider)` is the public
  private-provider escape hatch and accepts any weak-referenceable duck-typed
  provider. No approved contract restricts provider equality semantics.
- **Source proof:** `_outcomes.get(provider)` and assignment at
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:202,257-266` use
  `WeakKeyDictionary`, which aliases distinct live referents when their hashes
  and equality compare alike. Current tests use identity-equal stock or local
  providers and do not exercise this path.
- **Sibling impact:** one provider's success or failure suppresses the second
  provider's registration, while explicit observations remain available. The
  affected provider's framework spans omit the pair required for persisted
  trace/custom/Eval joins.
- **Ponytail decision:** retain. Reuse the existing lock and outcome owner, but
  make its weak bookkeeping identity-based. A short weak-reference list scanned
  with `ref() is provider` is sufficient for the naturally tiny provider set
  and avoids a new abstraction, dependency, provider wrapper, or equality call.

### `TEL-R5-001` — REVISED as `FIND-TASK-009-10`

- **Reachability:** the focused `Retaining` provider at
  `test_observe_surface.py:419-436` stores the supplied processor and then
  raises. Production code leaves its terminal outcome `False`, but the retained
  shared processor at `otel.py:206-236` remains fully active.
- **Source proof:** `install_run_correlation` pre-marks `False`, passes
  `_PROCESSOR`, and never changes the outcome after the exception
  (`otel.py:236-269`). `_RunCorrelationProcessor.on_start` has no provider or
  registration outcome and unconditionally stamps any current Run pair. The
  current test proves at-most-once handoff but never invokes the retained
  processor inside a scope.
- **Authority resolution:** reject the follow-up's `SPEC_REVISION_REQUIRED`
  disposition. Revision-46 REQ-151 at `spec.md:335-342` and the changed Run
  contract at `run_api.md:64-67,153-175` already choose the observable result:
  terminal `False` means no enrichment, including accept-then-raise. R4E's
  lower-priority statement that one shared stateless processor is acceptable
  is an implementation allowance that fails that result, not an unresolved
  product choice.
- **Ponytail decision:** retain as a bounded implementation correction. Create
  one registration-local processor in the existing owner, initially inert;
  enable it only after the foreign registration call returns normally. Preserve
  the same weak terminal outcome, lock, one attempt per provider identity,
  fail-open containment, optional dependency, and caller-owned provider
  lifecycle. Delete the shared `_PROCESSOR`; add no provider wrapper, retry,
  introspection, second registry, dependency, or public surface.

### `MAINT-009-1` — CONFIRMED as `FIND-TASK-009-11`

`changes/active/verified-change-contract/architecture/logic/run_api.md:3`
claims approval only through specification revision 34, while the candidate
changed that same authority to define revision-46 token-free scope and provider
registration behavior. The status contradiction makes the owning document's
authority ambiguous. Change only the status revision to 46; no substantive
Run contract change belongs in this finding.

### `REPO-R5-001` — CONFIRMED as `FIND-TASK-009-12`

`TASK-009-R4-stabilize-provider-registration-idempotency.md:4` remains
machine-readable as `ready`, while its line 13 and R4E's `supersedes` metadata
say revision 46 replaced it. The task lifecycle expressly provides
`superseded` for this case. Change only R4's frontmatter status and retain its
historical body and replacement link.

### `REPO-R5-002` — CONFIRMED as `FIND-TASK-009-13`

TASK-009 names two shared Rust tests and the Rust SDK journey, but its
implementation evidence at lines 261 and 269-272 records a regex-selected bare
Cargo command for the shared tests and only describes the journey as running
under a wrapper. `AGENTS.md` section 11 and the spec-driven test-command rule
require one exact `mise exec -- cargo nextest run` command per named Rust test,
with explicit package, target, exact `test(=...)`, and the complete owning
environment wrapper when needed. Broader green lanes do not supply that durable
proof. No production or test-source change is required.

## Prior-finding closure

| Prior finding | Independent closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Missing/API-only OTel, registration, attach, enrichment, and exit-update failures are contained while representative explicit observation remains strict at its ordinary boundary. | CLOSED |
| `FIND-TASK-009-2` | Native Run subject documentation covers root, initially selected, and sibling-selected views. | CLOSED |
| `FIND-TASK-009-3` | Revision 46 supersedes token/detach recovery with the approved context-value stack and matching attach-only pop. | SUPERSEDED AND CLOSED |
| `FIND-TASK-009-4` | The private OTel key is created once at module import. | CLOSED |
| `FIND-TASK-009-5` | PyO3 parameter names/defaults, owning and generated stubs, and public context-manager calls agree. | CLOSED |
| `FIND-TASK-009-6` | Both shared Rust selection tests contain their required panic documentation. | CLOSED |
| `FIND-TASK-009-7` | Two coordinated tasks enter the identical immutable Run and prove independent correlation and exit. | CLOSED |
| `FIND-TASK-009-8` | Pre-marked terminal outcomes prevent a second processor handoff to the same accept-then-raise provider. `FIND-TASK-009-10` is the separate behavior of the one processor already retained. | CLOSED |

## Final deduplicated finding ledger

| ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Evidence and observable consequence | Decision-complete correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|
| `FIND-TASK-009-9` | `SYS-R5-001`; focused follow-up resolution 1 | CONFIRMED | INCORRECT | REQ-151 requires idempotent registration at most once **per provider** and AC-032 requires exact correlation on supported private providers. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:200-203,239-269`; provider tests at `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:392-456` | `WeakKeyDictionary` keys by referent hash/equality, so a distinct equal provider inherits another object's terminal result and is not registered. Its framework spans lack Card/Run attributes and cannot satisfy the required persisted joins. | In the existing registration owner and under its existing lock, replace equality-keyed outcomes with weak identity bookkeeping. Use weak references and `is` comparison, discard dead entries during lookup, pre-mark the exact object `False`, and update only that identity to `True` after normal return. Preserve weak lifetime, one attempt, fail-open behavior, and no strong provider retention. A linear weak-reference list is the smallest sufficient mechanism; add no registry abstraction or dependency. | Add one focused case with two distinct equal, weak-referenceable duck-typed providers. Prove each receives exactly one processor and returns its own outcome; specifically prove a failed first provider does not suppress a healthy equal peer. Start a scoped span through the healthy peer and assert the exact pair. Run that test and the complete focused Python file. |
| `FIND-TASK-009-10` | `TEL-R5-001`; focused follow-up resolution 2 | REVISED | INCORRECT | Revision-46 REQ-151 and the Run API say a provider whose registration fails, including accept-then-raise, is terminal `False` and gets no enrichment. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:206-220,236-269`; incomplete case at `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:414-436` | An accept-then-raise provider retains the shared stateless processor while its cached result remains `False`. The retained processor cannot see that failed outcome and still stamps later in-scope spans, making the Boolean and no-enrichment contract false. | In the same owner, instantiate one initially inactive processor for each provider registration attempt and activate it only after `add_span_processor` returns normally. A retained processor from a failed call therefore remains inert. Remove the shared processor instance. Preserve the terminal weak outcome, existing lock, at-most-once identity semantics from `FIND-TASK-009-9`, fail-open exception handling, and healthy-provider enrichment; add no wrapper, retry, introspection, second registry, dependency, or lifecycle ownership. Update the changed Run authority and remediation evidence only as needed to remove the now-invalid shared-stateless allowance while preserving revision-46 behavior. | Extend the existing accept-then-raise test to invoke its retained processor inside an active Run scope and assert neither Wyrd attribute is written. Keep one retained processor, repeated `False`, and no second provider call. Existing healthy global/private cases must still return `True` and stamp the exact pair. Run both focused cases and the complete focused Python file. |
| `FIND-TASK-009-11` | `MAINT-009-1`; focused follow-up resolution 3 | CONFIRMED | VIOLATION | The changed owning architecture must identify the approved revision whose current contract it records. | `changes/active/verified-change-contract/architecture/logic/run_api.md:3` | The header says revision 34 while the same changed document records revision-46 behavior, leaving maintainers unable to distinguish approved authority from drift. | Change only the status line from revision 34 to revision 46. Preserve the substantive Run API text except for the separately required correction under `FIND-TASK-009-10`. | A static comparison shows `spec.md` revision 46 and `run_api.md` status revision 46; inspect the diff to ensure no unrelated status or content churn. |
| `FIND-TASK-009-12` | `REPO-R5-001`; focused follow-up resolution 4 | CONFIRMED | VIOLATION | The spec-driven task lifecycle requires `superseded` when an approved replacement invalidates a task. | `changes/active/verified-change-contract/review/TASK-009-r4/TASK-009-R4-stabilize-provider-registration-idempotency.md:4,13`; R4E lines 10-18 | Tooling can still select the invalid revision-45 task because machine-readable `ready` contradicts both its prose and R4E's replacement metadata. | Change only R4's frontmatter status to `superseded`; retain its historical body and R4E link. | Inspect both task frontmatters: R4 is `superseded`, R4E remains the revision-46 replacement, and their cross-reference is unchanged. |
| `FIND-TASK-009-13` | `REPO-R5-002`; focused follow-up resolution 5 | CONFIRMED | VIOLATION | `AGENTS.md` section 11 and spec-driven test-command precision require exact repository-native commands for every named Rust test in task evidence. | `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md:211-214,257-272` | The record substitutes one regex-selected bare Cargo command for two named shared tests and omits the complete repository wrapper plus `mise exec --` for the named SDK journey. The durable proof is not exact or replayable under the required toolchain/environment owner. | Run and record each shared test separately through `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::<exact_name>)'`. Run and record the SDK journey with its complete repository-managed Postgres wrapper and exact `mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run ... -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'`. Preserve broader lane evidence; change no implementation or tests. | The task evidence contains all three complete commands and their zero-exit results. Each selector is exact, each command uses `mise exec --`, and the journey record includes the full Postgres setup/migration wrapper. |

## Validation result

The proposed empty-ledger conclusions are rejected by reachable provider and
packet-state paths. The five retained findings are independent: provider
identity aliasing, failed-provider processor activation, architecture revision
status, superseded-task lifecycle state, and durable Rust command evidence.
They do not reopen prior findings 1-8.

The accept-then-raise correction is fully determined by approved revision 46;
it does **not** require specification revision. Its implementation is private,
local to the existing Python OTel owner, and can satisfy both the no-enrichment
result and at-most-once handoff without changing any public or cross-service
contract.

**Validated ledger result: five retained findings,
`FIND-TASK-009-9` through `FIND-TASK-009-13`.**
