# TASK-003 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
Immutable candidate `39c15176955edca785cc578168c67e1efcb80ea5`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of rebased `ff4145ac6`).
Authority: approved `changes/active/wyrd-ui-main-integration/spec.md` revision 13 and original `tasks/TASK-003-shared-ui-foundation.md`.

[subject.md](subject.md) records navigation, range and attribution. The audit retains the original task, all prior FIND-003-1..12/remediation history, latest R4 changes (`18e62740f`, `791122315`) and cumulative candidate. Earlier pre-rebase objects remain reachable; the preceding packet already confirmed reviewed patch preservation. Unrelated PR #205/main and TASK-027 commits, including narrow WyrdView ordering, are excluded from findings. Shared foundation seams remain reviewed.

## Independent reports and reconciliation

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL: B-NUMERIC |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL: INV-003-11 |
| repo-rev | [standards-review.md](standards-review.md) | FAIL: STD-003-1 |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL: MAINT-1 |
| system-rev | [system-review.md](system-review.md) | PASS; no proposal |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | FAIL: SEC-1 |
| fresh structured ponytail-rev | [findings-validation.md](findings-validation.md) | One independently CONFIRMED finding |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups the five proposals into the same original-value admission invariant. A focused follow-up was unnecessary: reviewers traced its common source and sibling paths; the system PASS explicitly concerns effective Rust integer budgets and does not claim original JavaScript Number fidelity. There is no unresolved conflict or uncovered path requiring another discovery pass. The fresh validator independently checked source, locked binding conversion and primary Node conversion evidence, resolved an imprecise MCP byte-deserialization statement, and confirmed one bounded correction. Agreement alone was not treated as proof. No orchestrator-originated finding is included.

## Reconciled acceptance and prior-finding closure

Both implementation reports retain full acceptance matrices. The following reconciliation incorporates independent validation and recorded proof limits.

| Obligation | Current source and verification evidence | Result |
|---|---|---|
| Approved portable exports, browser-native separation, catalog/schema authority and non-Svelte mounting | Package entries, type-only imports, schema generator and public renderer; supplied check/build/codegen/import assertions | PASS |
| Range precedence and duplicate-column binding | Shared loadChart input-first range and first-match columnIndex used by validation/plots; retained fixed-clock/brush/duplicate assertions | PASS; FIND-003-1/-2 CLOSED |
| Signed plots, twenty distinct series, authored/aggregate identity | Signed geometry, marker/dash combinations, typed tuple key and collision-free final captions feed plot/legend/tooltip; retained mounted/regrouping assertions | PASS; FIND-003-3/-4/-9/-10 CLOSED |
| Exact flat Card reads, bounded version paging, canonical copied identity and actions | Schema/loadCardHeader reuse exact get/list; shared formatter emits Kind/name@version[#uid]; version refs omit stale UID; canonical copy/chip/version-state tests | PASS; FIND-003-5/-12 CLOSED |
| Six authorized reads and positional, terminal-safe query projection | Existing SDK/MCP owners, shared JsonResultBuilder and existing framing/settlement owners; supplied real-server tests and queue/server proof | PASS except original numeric admission below |
| Original requested row/byte budgets refused consistently | Native Option<u32> extraction changes some original JS numbers before shared admission; current budget tests omit wrap/fraction cases | FAIL; FIND-003-11 ACTIVE |
| Acquisition/active/late cancellation, stopped dependencies, isolated failures and stale-load fencing | Node SDK return/native close notification, loader abort checks and renderer controllers; focused retained negative assertions | PASS; FIND-003-6 CLOSED |
| Canonical errors, browser/Node imports and truthful denial/error | Generated problems, complete HTTP payloads, SDK/MCP projection and standard JSON import attribute; existing import smoke/problem tests | PASS; FIND-003-7/-8 CLOSED |
| Credential/tenant isolation and durable ownership | Selected private Rust session transport, existing auth/read owners and query-only Bifrost shape; supplied real-session negatives | PASS |
| Complete Chart/CardHeader/primitives, gallery, custom direct/view app and AppBridge artifact | Current component/host source, supplied build/integration, recorded individual human acceptance | PASS within recorded proof limits |
| Production SDK packaging and single build owner | Existing bundle path builds/packs native SDK/runtime dependencies and import smoke; historical Darwin proof | PASS source/recorded scope; Linux/images not newly verified |
| Repository rules, generated parity and non-goals | Independent standards/maintainer coverage except numeric boundary; supplied fmt/lints/codegen/napi checks; no new durable owner, Ask/saved views/A2UI/production MCP registration/npm publishing | PASS except FIND-003-11 |

Current-source closure for every earlier ID is detailed in [findings-validation.md](findings-validation.md). FIND-003-11 is retained under its original ID: R4 fixes ordinary-u32 admission and effective caps, but original-value admission remains incomplete. FIND-003-12 is closed by the shared formatter and consumers; no new identity lookup/parser/alias or synthesized UID is required.

## Active validated ledger

| ID | Status / classification | One-line summary |
|---|---|---|
| FIND-003-11 | CONFIRMED / INCORRECT | Native N-API uint32 conversion can wrap/truncate forbidden JavaScript row/byte budgets into allowed integers before shared admission, yielding Node success where MCP refuses. |

Location: `sdks/wyrd-sdk-ts/native/src/lib.rs:1382–1403`, especially arguments at 1384–1385; SDK wrapper and Node executor forward original numbers unchanged. For example, 4294977296 rows becomes 10000 and 4299161600 bytes becomes 4194304; fractional budgets can also become allowed integers. The effective output maxima remain enforced: this is an invalid-input/refusal and parity defect, not a demonstrated memory-cap bypass or tenant escape.

The complete ledger records source-local proposal IDs, producer/sibling traces, precise evidence and closure proof. The selected correction validates the original Number losslessly at native collection, then reuses shared integer admission while preserving stable errors, taken-stream release, cancellation and terminal behavior. No new public API, architecture, persistent-data or security decision is required.

## Verification limits, disposition and workspace

[verification.md](verification.md) separates supplied green candidate fmt/lints/codegen/N-API/TS-unit/queue results, UI check (package/app zero errors), test 40/40, build and real-server integration 2/2 from fresh read-only source/dependency/Git evidence. Normal test/build execution was not repeated under the packet-only write constraint. The confirmed numeric examples are source-established rather than new live reproductions. Current green assertions use ordinary representable u32 budgets and do not exercise this conversion gap. Historical human/Darwin proof remains historical; Linux/images, renewed deployed browser/human inspection and live degraded parity are not inferred.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R5-lossless-budget-admission.md](TASK-003-R5-lossless-budget-admission.md) directly to `$wyrd-implement`; subsequent review reassesses the cumulative original task. No other active findings or remediation tasks are introduced by this round.

Final Git checks confirm HEAD remains `39c15176955edca785cc578168c67e1efcb80ea5`, no tracked source/test or index changes, and clean tracked diff whitespace. This review writes only its requested packet. A concurrent untracked `review/task-027-39c151769-1/` packet was left untouched. No commit or push occurred.
