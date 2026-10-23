# TASK-001 / TASK-001-R1 re-review verdict

**FIX_REQUIRED** — FIND-TASK-001-1, FIND-TASK-001-5,
FIND-TASK-001-7, FIND-TASK-001-8 remain after independent validation.
Prior FIND-TASK-001-2, -3, -4 and -6 are closed.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14` (PR 205 head).
- Candidate: `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`.
- Complete cumulative range: `git diff fb1fd9234...d2ffc5987`.
- Remediation locator: `4b723032f..d2ffc5987`; acceptance includes all earlier
  implementation and rebased commits `031a892be`, `ebd078712`, `4b723032f`.
- Approved [spec revision 1](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-verdict-formats-and-custom-data.md),
  [attempt-02 verdict](../task-001-20261022-02/verdict.md),
  [prior validated ledger](../task-001-20261022-02/findings-validation.md), and
  [R1 task/evidence](../task-001-20261022-02/TASK-001-R1-preserve-schema-and-safe-data.md).

HEAD remained the candidate. The three pre-existing uncommitted workflow edits
listed in [subject.md](subject.md) are excluded and preserved; only this new
review directory was added. The committed workflow was used as subject
context. No implementation, merge, commit, push, deployment or active-packet
completion was performed.

## Independent results and reconciliation

| Role | Report | Discovery / validation result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: BEH-1 resource semantics and BEH-2 Agent boundary proof |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: INV-1 missing plain-code undeclared-data proof |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: REP-1 mandatory rustdoc omissions |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: MNT-001 Python returned-check declaration parity |
| System resilience | [system-review.md](system-review.md) | PASS in scope; no SYS finding |
| Security/privacy | [domain-review-security.md](domain-review-security.md) | PASS in scope; no SEC finding |
| Persistent data | [domain-review-persistent-data.md](domain-review-persistent-data.md) | FAIL only for DATA-1 Agent boundary proof; durability/isolation source-pass |
| Fresh focused follow-up | [followup-review.md](followup-review.md) | RESOLVED: supported-draft/ID resource-closure discrepancy |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; four deduplicated retained findings |

The [claim comparison](claim-comparison.md) groups causes rather than caller
locations. A fresh follow-up was required because behavior's actual Rust
reproduction contradicted broad resource-closure claims. It independently
confirmed draft-04 failure and expanded the same cause to ineffective authored
identifiers. Final validation reproduced the defect again, independently of
the earlier emitted schema file, and reconciled other reviewers' closure
statements to the draft/identity cases they actually covered.

Every discovery proposal, including unique type and rustdoc claims, received
independent source validation. BEH-2/DATA-1 merge into one Agent proof gap;
INV-1 adds another missing case in the same prior proof inventory. Scoped
system/security PASS results remain credible for their availability and access
traces; they cannot establish correct semantics for every accepted schema.

## Reconciled acceptance

Source locations, full caller traces, and recorded versus freshly reproduced
proof are detailed in the [validated matrix](findings-validation.md#reconciled-acceptance-matrix).

| Obligation | Evidence and result |
|---|---|
| REQ-001 / AC-001: generated scalar/object plain formats, native/portable, inline/referenced, authored form/hash | PASS: existing compiler/host resolver/Prompt owners and plain SDK fixtures; unresolved format refusal retained. |
| REQ-001/002 / AC-003: exact supported schema; invalid declarations refused before save | FAIL FIND-1: emitted identity can leave data refs bound to envelope. Other name/schema/native/handwritten refusals supported. |
| REQ-002: check-only formats, required object/non-null data, step/whole support; preparation actions unchanged | PASS source: CodeCheck/CodeCall split, canonical shapes and bounded fits. FIND-5 leaves one plain-code negative proof gap. |
| REQ-007 / AC-004: exact server validation, inconclusive invalid output, rejected data absent | FAIL FIND-1 exact meaning and FIND-5 proof. Shared executor failure/projection owner otherwise preserves required behavior. |
| REQ-007 / AC-005: inclusive compact UTF-8/escapes/wide/depth limits in both kinds | FAIL proof FIND-5: Agent values are 65,535/65,549; adjacent boundaries and required variants missing. Hosted code bounds/readback now present. No production bound arithmetic defect asserted. |
| Preserve citation/pass policy, aggregation, dispatch/status, actual/detail | PASS: unchanged owners and executor continuation, existing regressions. |
| REQ-011 / AC-002: data in whole/step ordinary direct/queued/status/HTTP/MCP/published reads; old/plain absent | PASS for correctly validated data: complete outcome serialization and existing readers/journeys. Subject to FIND-1 at producer. |
| REQ-011 / AC-007: tenant/query-read, retention/durability, no lower-privilege projection | PASS within changed boundary: existing result/outbox/access owners, foreign/scoped proofs and safe diagnostics. |
| REQ-012 / AC-006: actual types on every operation/deadline, failed/mixed/absent data, ordinary transport/lifecycle | PASS: shared local conversions and typed/source/type fixtures; no write/rerun on mismatch. |
| REQ-012: normal exact SDK error, whole failure and safe path | PASS: arbitrary TS throws normalized; all generic decoders discard payload diagnostics and report root. |
| Python positional-only arguments, data_type slot/group/input forms | PASS: native signature and delegation plus Python fixtures. |
| Model/schema authoring, public exports/types, docs/generation | FAIL FIND-7 returned Python CodeCheck parity. Existing authoring/runtime/generator/docs surfaces otherwise supported by source and recorded checks. |
| Schemars migration consumers and tier direction | PASS except FIND-1: installed active version 1.2.1; existing generator/OpenAPI/MCP/extract readers migrated. No new dependency/feature owner. |
| Mandatory changed-item rustdoc | FAIL FIND-8: new exact_data panic contract and PATH const docs. Prior Judge/score omission closed. |
| Scope/non-goals and owner reuse | PASS: no new store, result column, permission, transport, compiler, polling loop, configurable bound, decoder registry or lifecycle owner; schema migration and consumer adaptations are R1 scope. |

## Validated ledger

| Stable ID / status | Discovery IDs | Diagnosis and selected correction | Focused closure |
|---|---|---|---|
| **FIND-TASK-001-1 / REVISED, open** | BEH-1, follow-up | INCORRECT; `execution.rs:603–625` always inserts `$id`, ineffective for draft-04; authored same-base IDs and classic root-ref suppression can also defeat scope. Both runtime consumers validate altered meaning and may retain invalid data. Fix standard effective/distinct resource construction at the shared resolver using installed draft/URI behavior. Preserve usable authored IDs/dialects; refuse unpreservable compositions through existing pre-save compiler errors, with no blacklist or new walker/validator. | Existing resource table plus draft-04/usable-ID good/bad cases, ineffective-ID/root-ref declaration refusal and absence; existing Agent/code proof retains good and omits bad. |
| **FIND-TASK-001-5 / REVISED, partly closed** | BEH-2, DATA-1, INV-1 | MISSING verification; `blocks_agent.rs:604–642` rounds byte budget down and rejects far above one-over; fixture `scored-review.yaml:95–143` EXTRA-DATA is a property inside declared data. Extend existing Agent exact/over UTF-8/escape/wide table with byte assertions and existing checked-in plain-code fixture/SDK negative tables. No runtime defect or new production guard follows from this gap. | Exact existing Agent selector and three SDK invalid-output selectors; repeat accepted readback only when its fixture changes. |
| **FIND-TASK-001-7 / CONFIRMED, new** | MNT-001 | MISSING public declaration; Python `verify/__init__.pyi:218–226` CodeCheck omits returned optional response_format. Installed ty rejects valid narrowed access. Add wire member using existing schema/value types and extend existing typing fixture. | Existing Python type/stub lanes accept scalar/expanded formats, absence and narrowed optional access. |
| **FIND-TASK-001-8 / CONFIRMED, new** | REP-1 | VIOLATION of explicit rustdoc gate; new SDK `exact_data:1455` has assertions without panic docs; gateway `PATH:549` lacks rustdoc. Add concise docs preserving behavior/assertions. Distinct from closed FIND-6. | Source inspection and applicable format/lint only; no behavior test. |

Only independently confirmed/revised findings appear here. The complete
[validated ledger](findings-validation.md#validated-deduplicated-ledger)
contains exact evidence, consequences, correction boundaries and focused lanes.

## Prior-finding closure

| Prior finding | Disposition |
|---|---|
| FIND-1 | OPEN, revised: hoisting/dialect-erasure/collisions fixed; effective resource identity still fails accepted draft/ID cases. |
| FIND-2 | CLOSED: shared TS catch never reads thrown values; malformed/nullish/keyed cases map to exact WyrdError. |
| FIND-3 | CLOSED: shallow interpreter deleted; whole canonical shapes use bounded fits; composition/ref acceptance and optional refusal/no-save covered. |
| FIND-4 | CLOSED: server uses schema-owned location; Rust/Python/TS discard decoder diagnostics and use safe root; raw read/rerun semantics preserved. |
| FIND-5 | PARTIAL: hosted malformed/exact/depth and retained-readback added; Agent adjacent-byte variants and plain-code undeclared-envelope-data proof remain. |
| FIND-6 | CLOSED: Judge and score rustdoc now present. New items in FIND-8 are separately identified. |

## Verification limits and non-blocking process record

[verification.md](verification.md) records fresh cumulative whitespace and
locked/offline dependency-tree checks. Discovery/final validation independently
reproduced schema inversion with the installed Rust validator linked from
existing rlibs, Python narrowed-access rejection with installed ty, and Agent
compact-byte arithmetic. These are concrete focused reproductions, not fresh
full executor journeys.

The original/R1 focused journey, unit, OpenAPI/MCP, generation, lint/type and
docs successes were inspected against actual cases and assertions but were not
all rerun. Attempted TS unit reruns stopped before tests (pnpm database error;
missing local Vitest fallback), so no passing test result is claimed for them.
No broad gate, live strict-provider schema call, crash/load injection, or full
SDK suite was repeated. Existing-rllib probes do not rebuild the candidate or
qualify vendor schema acceptance.

Historical RED for original scenarios 1–3 and retained independent diagnostician
provenance remain unavailable as disclosed. The standards report records the
process violation, but independent validation found no weakened member-set
assertion or demonstrated behavioral consequence from it alone. It creates no
additional FIND or requirement to fabricate historical evidence. Other
placement/snippet/style deviations were rejected as material blockers by the
validator; they are not remediation work. The explicit rustdoc exception remains
FIND-8.

## Remediation handoff

Route [TASK-001-R2-preserve-resource-identity-and-close-proof.md](TASK-001-R2-preserve-resource-identity-and-close-proof.md)
directly to `$wyrd-implement`. It packages the four validated findings under
approved revision 1, with root-cause diagnoses, selected existing-owner
corrections and focused proof. No specification revision is required. This
review does not begin implementation. Task completion requires a later PASS on
the cumulative remediated candidate.

The required fresh [remediation comprehension pass](remediation-comprehension.md)
read only the approved spec/new task and committed workflow. It found no
missing explanation, contradiction or unresolved material decision. It did not
review implementation correctness or add another approval gate.
