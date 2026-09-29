# TASK-002 R2 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Branch: `verified-change-contract`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `a000c201ae86f584fd5b80349f375e087902fd78`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior verdict: `changes/active/verified-change-contract/review/TASK-002-r1/verdict.md`
- Prior validation: `changes/active/verified-change-contract/review/TASK-002-r1/findings-validation.md`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md`

The candidate remained `a000c201ae86f584fd5b80349f375e087902fd78`
through both review waves.

## Wave 1 results

| Independent review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None; reported all prior findings closed |
| Repository standards | FAIL | `SR2-001`, `SR2-002` |
| Contracts and SDK projections | FAIL | `DC-R2-001` through `DC-R2-003` |
| Data and durability | PASS | None |
| Security and tenancy | PASS | None |
| Concurrency and lifecycle | PASS | None |

## Acceptance matrix

| Obligation | Result | Evidence or finding |
|---|---|---|
| Configured Rust startup matches the locked API | PASS | `FIND-TASK-002-1` closed |
| Fixed-table compatibility checks exact order, type, count, and nullability | PASS | `FIND-TASK-002-2` closed |
| Python rejects coercible non-string mapping/dataclass keys | PASS | `FIND-TASK-002-3` closed |
| TypeScript refuses every value that its JSON serialization would silently omit or coerce | FAIL | `FIND-TASK-002-4` remains open for own symbol keys |
| Python and Node runtime-local active spans are captured with explicit precedence | PASS | `FIND-TASK-002-5` closed |
| Concurrent dynamic-table first use converges through the existing owner | PASS | `FIND-TASK-002-6` closed |
| Successful shutdown is terminal across startup races | PASS | `FIND-TASK-002-7` closed |
| Ambiguous shutdown retries on the same state and batch | PASS | `FIND-TASK-002-8` closed |
| Audit lock timeout returns an honest transaction error | PASS | `FIND-TASK-002-9` closed |
| Every new/materially modified Rust item has mandatory rustdoc | FAIL | `FIND-TASK-002-10` remains open |
| Imports and signature paths follow repository placement rules | PASS | `FIND-TASK-002-11` closed |
| Unknown and unauthorized dynamic describes fail before admission at a real boundary | PASS | `FIND-TASK-002-12` closed |
| Every SDK Eval journey proves session/media, persisted fixed-width identity, and pre-admission negatives | FAIL | `FIND-TASK-002-13` |
| Required SDK journeys prove fixed-table startup refusal, cached reuse, and stale fingerprint refusal | FAIL | `FIND-TASK-002-14` |
| Five fixed schemas, writer/subject split, fixed-binary decoding, and tall Drift/canonical Eval projection | PASS | Source, focused tests, and recorded capability lanes |
| One state-owned Bifrost, existing bounded queue, immutable views, no synchronous verdict wait | PASS | Cumulative source and lifecycle/queue proof |
| No second queue, cache, transport, schema authority, config type, authorization model, or test harness | PASS | Complete cumulative diff inspection |
| Non-goals remain excluded | PASS | No run registry, new observation type, retention policy, atomic multi-row API, or synchronous scoring path |

## Prior-finding closure

| Prior finding | R2 status |
|---|---|
| `FIND-TASK-002-1` | CLOSED |
| `FIND-TASK-002-2` | CLOSED |
| `FIND-TASK-002-3` | CLOSED |
| `FIND-TASK-002-4` | OPEN |
| `FIND-TASK-002-5` | CLOSED |
| `FIND-TASK-002-6` | CLOSED |
| `FIND-TASK-002-7` | CLOSED |
| `FIND-TASK-002-8` | CLOSED |
| `FIND-TASK-002-9` | CLOSED |
| `FIND-TASK-002-10` | OPEN |
| `FIND-TASK-002-11` | CLOSED |
| `FIND-TASK-002-12` | CLOSED |

The missing AC-026 and AC-025 journey obligations are distinct from the closed
runtime defects and receive the new stable IDs `FIND-TASK-002-13` and
`FIND-TASK-002-14`.

## Validated finding ledger

| ID | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-002-4` | CONFIRMED / OPEN | INCORRECT | Reject own symbol keys in the existing TypeScript strict serializer and cover root/nested cases. |
| `FIND-TASK-002-10` | CONFIRMED / OPEN | VIOLATION | Complete the mandatory rustdoc audit for every added Rust item, including trait and test-local items. |
| `FIND-TASK-002-13` | REVISED | MISSING | Extend the three existing Eval journeys to prove session/media, persisted fixed-width trace identity, and negative refusal. |
| `FIND-TASK-002-14` | REVISED | MISSING | Extend existing real journeys for fixed-table startup refusal, cached reuse, and one shared-owner stale-fingerprint refusal. |

Full independently validated caller tracing, evidence, consequences, and closure
proofs are preserved in `findings-validation.md`.

## Verification limits

Recorded evidence on code-bearing commit `a96820fa` includes all nine
`verify:bifrost` lanes, 689 shared tests, Rust SDK tests, Python and TypeScript
unit/integration/type/binding lanes, code generation, client/PyO3 checks,
formatting, lints, focused Postgres journeys, and exact focused Rust tests. R2
reviewers reran selected lifecycle, schema, cache, Python, TypeScript, boundary,
and formatting checks.

Those lanes do not cover own symbol-keyed TypeScript input, the repository's
stronger every-item rustdoc rule, the complete per-SDK Eval journey matrix, or
the AC-025 real-boundary startup/cache/fingerprint cases. Green aggregate lanes
therefore do not close the four validated gaps.

## Verdict

**FIX_REQUIRED**

All four remaining findings are bounded corrections within approved revision 32
behavior. None requires a specification or architecture decision.

Remediation task:
`changes/active/verified-change-contract/review/TASK-002-r2/TASK-002-R2-close-boundary-and-journey-gaps.md`.
