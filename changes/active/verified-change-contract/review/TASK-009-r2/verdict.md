# TASK-009 review verdict — Round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`
- Approved spec: `changes/active/verified-change-contract/spec.md`, candidate revision 45.
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`, retaining revision-35 mapped obligations.
- Prior verdict/ledger: `changes/active/verified-change-contract/review/TASK-009-r1/{verdict,findings-validation}.md`.
- Remediation reviewed: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`.
- Scope: complete cumulative base-to-candidate diff, including original implementation and remediation.

HEAD remained the candidate throughout review and verdict preparation. The later commit records only this review directory; it does not change the reviewed subject. No CodeGraph index exists. Some discovery reports identify specification revision 35, the task's mapped revision; the current approved candidate is revision 45 and preserves the applicable obligations, as confirmed by standards, follow-up and validation.

The interrupted wave preserved maintainer and system reports. Five missing discovery reports were completed by separate fresh reviewers on resumption. Each retained its own role; returned report content was preserved by the orchestrator where a review-only role prohibited writing. All required reports and independent validation are present. No unavailable reviewer or report is treated as a verification limit.

## Reconciled acceptance matrix

| Obligation | Reconciled source and proof | Result |
|---|---|---|
| Root default, local initial Card selection, UUIDv7 invocation and immutable sibling views across Rust/Python/TS | shared state/Run owner, delegated SDK projections, selection/refusal unit and journey assertions | PASS |
| Python sync entry returns Run; exceptions propagate; exact active/child enrichment | PyRun boundary, OTel entry/processor, focused active/child/user-exception proof | PASS |
| Nesting, await, task-context copying and isolation | execution-local ContextVar/OTel state and focused scenarios | PASS |
| Concurrent first entries share private key | existing lock and double-check; deterministic two-thread check | PASS |
| Failed or swallowed detach restores outer/absent correlation | exact token plus recorded prior; same-context recovery check | PASS |
| Missing/API-only/attach/enrichment/detach do not block explicit observations | representative Drift ordinary-error assertions | PASS |
| Actual processor registration failure has explicit-observation proof | raising provider is installation-only; Run/Drift case uses no-method object, never registration exception | FAIL — FIND-TASK-009-1 |
| Global/private provider idempotency and optional integration | registry/lock, global and private tests, optional production dependency | PASS |
| Real authenticated OTLP persisted trace/custom/Eval joins and managed identity | extended existing journey with explicit publication barriers and exact joins | PASS by source and recorded journey evidence |
| Strict Card/error/auth behavior; reused observation/Bifrost ownership | native explicit paths and unchanged server boundaries | PASS |
| Public Python exit signature agrees with typed contract | underscored optional runtime parameters conflict with conventional required stub parameters; public keyword call raises TypeError | FAIL — FIND-TASK-009-5 |
| Changed Rust subject documentation | root/initial/sibling field rustdoc | PASS |
| New Rust tests meet mandatory documentation rule | two selection tests omit # Panics despite fixture/assertion paths | FAIL — FIND-TASK-009-6 |
| Exit has no lifecycle/durability effect; approved non-goals excluded | cleanup only; no server Run, extra queue/writer/wrapper, global Card, log/metric or client-managed identity changes | PASS |
| No unrelated implementation changes | full cumulative ownership/dependency/source review | PASS |

## Independent results and follow-up

| Required role | Report | Result |
|---|---|---|
| Behavior | task-review-behavior.md | PASS; ID 1 closure claim superseded by validation |
| Invariants | task-review-invariants.md | FAIL; residual prior ID 1 |
| Repository standards | standards-review.md | FAIL; runtime/stub parity and test docs |
| Maintainer | maintainer-review.md | PASS |
| System resilience | system-review.md | PASS; registration proof closure claim superseded by validation |
| Telemetry domain | domain-review-telemetry.md | FAIL; residual prior ID 1 |
| Concurrency/context domain | domain-review-concurrency.md | PASS |
| Focused follow-up | followup-review.md | RESOLVED |
| Fresh Ponytail validation | findings-validation.md | FIX_REQUIRED; three retained findings |

Follow-up was required because behavior/system accepted R1's compressed API-only-or-registration wording while invariant/telemetry required distinct AC-032 cases. Source and approved authority resolve the conflict: AC-032 separately names API-only providers and actual registration failure. Derived task wording cannot waive the latter. No other unresolved reachable path or authority conflict remains.

## Validated ledger and prior closure

| Stable ID | Discovery sources | Status/classification | Required outcome |
|---|---|---|---|
| FIND-TASK-009-1 | INV-REV-R2-001, D-TEL-R2-001, FOLLOWUP-009-1 | REVISED / MISSING | Actual raising registration through Run entry followed by representative explicit Drift proof; establish registration was reached. |
| FIND-TASK-009-5 | REPO-009-1 | CONFIRMED / VIOLATION | Align existing PyO3 and owning stub names/defaults, regenerate, and prove public calls agree. |
| FIND-TASK-009-6 | REPO-009-2 | CONFIRMED / VIOLATION | Document panic conditions on the two new Rust tests without executable changes. |

Full exact locations, producer/caller/sibling tracing, consequences, selected correction boundaries and focused proofs are preserved in `findings-validation.md`.

Prior ID 1 is partially closed: all failure proof except actual registration is present. Prior ID 2 closes with accurate Run subject docs. ID 3 closes with same-context prior restoration after raising/swallowed detach. ID 4 closes with serialized key creation and deterministic first-entry proof. IDs 5 and 6 are new and do not reuse closed IDs.

## Verification and limits

The focused Python surface file passed all 33 tests in orchestrator and multiple independent reviewer reruns, including validator (0.89s). Validator independently reproduced the public conventional-keyword TypeError on a real offline Run and verified no-argument exit returns False. Source/diff inspection confirms required panic sections are absent. Cumulative diff whitespace checks passed.

Broader Rust/Python/TS, type/codegen/boundary/format/lint and real Postgres journey results are supplied in immutable task/remediation evidence, inspected against source and owning recipes; they were not all independently rerun here. Those limits neither excuse the missing required case nor erase the reproduced public mismatch or explicit documentation violation. No missing reviewer, authority, source or report exists.

## Verdict

**FIX_REQUIRED**

All three corrections are bounded within approved behavior and existing owners. No specification, architecture, security, lifecycle, concurrency or persistent-data decision is required.

Remediation: `TASK-009-R2-close-proof-and-boundary-parity.md` in this directory, routed directly to `$wyrd-implement`.
