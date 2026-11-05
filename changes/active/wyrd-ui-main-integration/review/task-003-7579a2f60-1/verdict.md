# TASK-003 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
Immutable candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of rebased `ff4145ac6`).
Authority: approved specification `changes/active/wyrd-ui-main-integration/spec.md` revision 13 and original `tasks/TASK-003-shared-ui-foundation.md`.

[subject.md](subject.md) records the full range, navigation and attribution. This review includes the original task, all prior FIND-003-1..10/remediation rounds, authored-series R3 (`f0fff4b13`) and human-directed flat identity adaptation (`4eca1e2ba`, `bde4396d9`). The human direction is not retroactively assigned a finding. TASK-027 and unrelated PR #205/main content are excluded. Range-diff independently confirms reviewed implementation patches survived the rebase; differing packaging context and packet authority links do not remove reviewed behavior.

## Independent results

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL: two proposals |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL: same two causes |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL: budget admission proposal |
| system-rev | [system-review.md](system-review.md) | FAIL: budget admission proposal |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | PASS |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh independent ponytail-rev | [findings-validation.md](findings-validation.md) | Two confirmed findings |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups duplicates by cause. A focused follow-up was used to reconcile overlapping PASS language: structured exact read identity differs from copied canonical text, and enforcement of a supplied output budget differs from admission of that budget's range. The fresh validator independently traced and confirmed both causes, constrained corrections to existing owners, and preserved native stream ownership and protocol-specific error semantics. There is no unresolved disagreement and no orchestrator-originated finding.

## Reconciled acceptance and prior closure

| Obligation | Current implementation / verification evidence | Result |
|---|---|---|
| Portable package/exports, browser-native separation and generated catalog authority | Package entries, type-only imports, schemas/generator/dispatch; supplied check/build/import test evidence | PASS |
| Exact flat Card wire reads, dependent paging and version refusal | Strict schema, resolved kind/name get/list, current SDK/MCP tools, live read/BFF tests | PASS |
| Canonical copied Card identity | Shared formatter omits kind; same-name/version different kinds copy identically | FAIL: FIND-003-12 |
| Identical authorized six reads and positional/terminal-safe results | Existing SDK/MCP owners and shared JSON builder; supplied live tests and historical Rust proof | PASS except requested-budget admission |
| Existing maximum collected row/byte ceilings on both paths | MCP admits ranges; native collect stores unrestricted supplied budgets | FAIL: FIND-003-11 |
| Explicit/inherited range and duplicate-column binding | Shared range precedence and first-match columnIndex; retained focused assertions | PASS; FIND-003-1, -2 CLOSED |
| Signed chart geometry and twenty distinguishable series | Signed domains/bar endpoints and marker/dash combinations; retained mounted cases | PASS; FIND-003-3, -4 CLOSED |
| CardHeader named action states and authoritative selection | ActionNote and pending/refusal handling; direct/view action assertions | PASS; FIND-003-5 CLOSED |
| Node acquisition/active/late cancellation | Existing stream return/close and abort checks; focused acquisition tests | PASS; FIND-003-6 CLOSED |
| Canonical problems and native public imports | Generated Rust examples, complete endpoint payloads, JSON import attribute and native smoke | PASS; FIND-003-7, -8 CLOSED |
| Synthetic overflow caption versus authored series | Caption selected against emitted authored captions and retained exact totals | PASS; FIND-003-9 CLOSED |
| Typed authored-series identity before aggregation | JSON typed tuple key, collision qualification, shared plot/legend/tooltip consumers; four-tuple/null/rerender assertions | PASS; FIND-003-10 CLOSED |
| Isolated panel failures, supersession, validated host intents | Catalog, controller/generation fencing, Node/MCP/BFF adapters; retained focused tests | PASS |
| Credentials, tenant isolation and shared durable ownership | Rust selected-session/auth/query owners, closed read union, server-only example credentials; supplied real-session negatives | PASS |
| Complete components/primitives and four-host foundation | Current source plus supplied gallery/example/AppBridge/live evidence and recorded 2026-10-20 human acceptance | PASS within recorded proof, subject to canonical copy defect |
| Production SDK packaging closure | Established bundle owner packs SDK/runtime/addon; range-diff preservation and historical Darwin smoke | PASS source/recorded scope; Linux/images UNVERIFIED |
| Non-goals, reference preservation and successor attribution | Cumulative attributed diff; no new durable owner/Ask/saved views/A2UI/production MCP registration/npm publication | PASS |

The full acceptance matrices are preserved in both implementation reports. Prior finding closure is supported by current producer and sibling-consumer source traces in [findings-validation.md](findings-validation.md); green summaries alone are not treated as proof. The two new findings are distinct causes and do not reopen prior chart/action/cancellation corrections.

## Active validated ledger

| ID | Status / class | One-line summary and correction owner |
|---|---|---|
| FIND-003-11 | CONFIRMED / INCORRECT | Native bounded collection admits zero/above-maximum budgets refused by MCP; reuse shared JSON-result admission policy while preserving native release and existing protocol errors. |
| FIND-003-12 | CONFIRMED / INCORRECT | Shared Card formatter drops kind from canonical copied text; correct the existing formatter for flat kind/name/version and supplied optional UID. |

The complete source-local discovery IDs, exact locations, reachable consequences, selected corrections, preserved behavior and closure proofs are in [findings-validation.md](findings-validation.md). No rejected or merely stylistic proposal is included. Neither correction requires a new product, architecture, persistent-data or public-contract decision.

## Verification limits and disposition

[verification.md](verification.md) distinguishes caller-supplied candidate-green check (package/app zero errors), test 40/40, build and Postgres-managed integration 2/2 from fresh read-only source/range-diff checks. Those tests do not assert requested-budget range admission or successful canonical copying. One discovery reviewer also records a read-only existing-output formatter/grouping probe; no reviewer rebuilt sources. Historical Rust/codegen/native/Darwin/browser/human evidence remains historical. Linux/images, renewed human/browser inspection and live degraded-terminal parity were not rerun or inferred. No build/test suite was repeated under the review's only-new-packet write constraint.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R4-bounded-reads-and-card-references.md](TASK-003-R4-bounded-reads-and-card-references.md) directly to `$wyrd-implement`. A subsequent review reassesses the cumulative candidate against the original task. HEAD and tracked source/test files remained unchanged; this review writes only its new packet and does not commit or push. A concurrently appearing TASK-027 review packet was left untouched.
