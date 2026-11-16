---
id: TASK-012-R3
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 15
parent_task: TASK-012
remediates: [FIND-012-2]
requirements: [REQ-017]
depends_on: []
---

# Promise the result representation before building it

A queued verification must account for its independently materialized result before building it, while keeping the decision's memory custody independent. Complete that single remaining boundary using the existing result builder, process ledger and charged outbox handoff.

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Approved spec: changes/active/github-verifier-pipelines/spec.md revision 15, REQ-017; original TASK-012 OBL-7.
Original task: changes/active/github-verifier-pipelines/tasks/012-integrate-code-mode.md.
Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1.
Earlier candidates: 9016393004dc0eb319075245c5f6a1d7a6c2cd46 and f6fac677cd71208871fa5c21e4035085026a99c4.
Reviewed candidate and implementation input: 0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd on task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Prior contracts: review/TASK-012-first-review-20261116/TASK-012-R1-preserve-settlement-and-resource-custody.md and review/TASK-012-R1-review-f6fac677c-20261116/TASK-012-R2-complete-settlement-and-copy-custody.md in this packet.
Current validated diagnosis: findings-validation.md and verdict.md beside this task. Prior accepted rows and all other finding closures remain acceptance context.

## Diagnosis: FIND-012-2 remains at result production

R2 correctly charges initial and recovered terminal copies before cloning, retains the returned decision across persistence, and transfers the finished result's charge through the reservation wait into the outbox. Preserve those fixes.

The remaining defect is verification/runner.rs:1627–1636: conclude promises the selected decision's compact JSON retained size plus its existing fixed allowance, calls result(), and only afterward fits the charge to actual ScribeSink weight. That decision representation is not an upper bound on the result representation. ResultPayloadBuilder::build_as produces a Variant-backed output and two StagedRows; weight includes actual Variant buffers, cells and correlation text. StoredOutput includes captured step output and excludes input evidence. Both initial selection and recovered delivery reach this same producer.

A supported captured step output containing 100,000 integer zeros is 200,001 compact JSON bytes. The pinned production Variant encoder stores each zero as a header plus Int8 and uses list count/offset storage, producing 500,008 value bytes for that array alone. Outer metadata/rows add more. The independent result can therefore hold hundreds of kilobytes beyond its approximately 200 KiB precharge while encoding, before fit repairs the ledger. Other runtime threads can admit sibling hosted/Bifrost work using those unpromised bytes even though this encoding has no await. This violates the explicit R2-2/OBL-7 before-allocation requirement; documenting the limit does not close it.

Evidence owners: runner.rs above; state.rs:356–359 retained-size calculation; results.rs:185–190,317–341,618–623; scribe_outbox.rs:2075–2082; Wyrd queue Variant serializer and pinned parquet-variant revision 394d4b0757488d70df9a78c0c271ab3c8fbc2928. findings-validation.md records the complete format calculation, producer/consumer trace and supported size limit. A 10,000-zero array also demonstrates expansion, so proof does not require extreme cardinality.

The current terminal-copy test uses large input evidence, which is excluded from StoredOutput. It proves the now-correct decision lifetime but cannot detect this result-sizing defect. No other state-copy, media, settlement, gateway, workspace or documentation finding remains open.

## Selected correction

Keep the existing common queued conclude/result handoff. Compute a conservative result-representation bound from its borrowed selected/frozen inputs before construction, with representation sizing owned by the existing result builder. Replace the decision-size proxy with that bound for the separate same-ledger infallible promise. Preserve mandatory-growth behavior: this correction introduces no new refusal/admission policy.

Use the existing nonretaining serde counting approach to account for serialized scalar/string content and container entries, applying conservative pinned-Variant bounds for primitive headers/data, container counts/offsets and object-key metadata. Include both result schemas' cell storage and actual dynamic text/correlation/trigger demand from the same inputs the builder uses. Count repeated metadata conservatively; no deduplication cache is needed. Derive the estimate from the representation rather than an arbitrary multiplier of decision JSON. A second fully materialized result built merely to discover its size is too late and doubles work.

Take this additional promise before build_as creates the result, retaining the decision's independent charge until that owner ends. After successful construction, reconcile to actual ScribeSink weight and keep the already correct reservation/stage_charged takeover. The estimate must cover supported actual result demand; postconstruction growth must not be how required representation bytes first enter the ledger.

This is one bounded sizing integration at the existing producer. Deleting the promise, reusing live/frozen custody for an independent copy, inflating Held permanently, or charging only at reservation all miss the same boundary. A generic allocator, global Variant accounting framework, new ledger/dependency, exact RSS accounting, new refusal contract, schema, SQL wait or changes to unrelated direct/scheduled/group builder callers are unnecessary. Local sizing and its queued consumer suffice; preserve all public/durable behavior.

## Acceptance scenarios and proof

Use these entries as the implementation evidence record; update them in place under the current workflow. Do not append another obligations ledger or duplicate evidence table.

1. At the existing result owner, construct a supported successful outcome with captured numeric-array step output, empty evidence and otherwise ordinary metadata. The actual built result/ScribeSink weight must exceed the old compact decision promise, while the new preconstruction bound covers it. Before production changes, the focused test must fail for that missing bound. Assert against the actual representation, rather than merely restating the sizing formula or putting the large value only in excluded evidence.
   - **Proof:** `verification::results::tests::a_captured_numeric_output_is_bounded_before_its_result_is_built` (results.rs). It uses one succeeded step whose captured output is 100,000 integer zeros, with empty evidence and the ordinary `TestRun` metadata (trigger, owner, binding, Run). It asserts that the built rows' summed `ResultRow::owned_bytes` (exactly what `ScribeSink::weight` sums for a result) exceeds the old promise, `encoded_len(TerminalSelection) + 512`, and that `bound` covers that weight.
   - **RED:** run first against a stub `bound` that kept the old decision-size proxy: `the bound 200650 falls short of the built 502290`. The test's first assertion (built > old promise) already held.
   - **GREEN:** PASS. **Result: PASS.**
2. Cover the estimator's distinct scalar/container/key/text and dynamic correlation/trigger branches where they independently affect its bound. Reuse the same result builder/test owner and real encoded output. The estimate may overcount; every required result byte must be covered. Add only cases needed to establish the chosen bound, without a new property-test dependency or arbitrary input-hardening project.
   - **Proof:** `verification::results::tests::the_bound_covers_every_representation_branch` builds real payloads and checks the bound against them:
     - narrow scalars, `u64::MAX` (decimal16), doubles, short/long/escaped strings, 1,000 long strings, 60-deep lists, empty containers, 2,000 long distinct keys, 1,000 repeated-key objects, and the drift/assert requirement fixture;
     - a run with 4 KiB deployment environment, version and trigger text, a group, and no owner or Run (the dynamic summary and correlation text);
     - 1,000 `i64` map keys checked directly with `variant_bound` against `EncodedVariant::from_serialize`, which stores them as key text.
   - **RED:** against the same stub: `narrow scalars: bound 682 < built 2285`.
   - **GREEN:** PASS.
   - **Mutation (assertion strength):** each mutation removes one estimator term; each makes the named case fail, and the source was restored after every run.

     | Term removed | Failing case |
     |---|---|
     | key text | keys: bound 28346 < built 153191 |
     | numeric-key text | numeric keys: bound 13018 < built 27012 |
     | decimal width counted as 9 bytes | wide numbers: bound 15387 < built 22278 |
     | string length header | long strings: bound 71388 < built 74290 |
     | list offsets | wide numbers: bound 20392 < built 22278 |
     | correlation text counted once instead of per row | dynamic text: bound 14469 < built 18474 |
     | cell arrays | narrow scalars: bound 801 < built 2285 |

   - **Result: PASS.**
3. Source inspection must show the separate promise is acquired before result construction while the decision remains independently charged. Keep existing postconstruction exact fitting, reservation custody, single takeover and final release assertions. Initial and recovered decisions continue through this one producer; first-decision identity, persistence/lease/ACK behavior and memory-first mutation remain unchanged. No noisy whole-server ledger assertion or new allocation/test-control harness is required to prove this order.
   - **Proof (source):** `runner.rs` `conclude`, in the `Decision::Selected` arm, runs in this order:
     1. builds the `ResultPayloadBuilder`;
     2. takes `charge.ledger().charge_infallible(builder.bound(&selected.outcome))` while `selected` and its own `charge` are still alive;
     3. calls `result(&builder, …)`, which alone calls `build_as`;
     4. then `drop((selected, charge))`, `fit` to `ScribeSink::weight`, and `deliver` → `stage_charged`, all unchanged.

     Recovered and initial decisions both reach this arm through `Offer::Accepted`/`state.select`. No state, persistence, lease or ACK code changed. Reuse: the existing builder owns the sizing (`bound`, `variant_bound`), and `variant_bound` reuses the `serde_json` streaming walk through a counting `Formatter` into `io::sink()`, the same nonretaining approach as `encoded_len`. It adds no dependency, ledger or refusal.
   - **Exact preserved regressions:** PASS (commands below). **Result: PASS.**

Test owner: inline verification/results.rs or its existing owning test module; package wyrd-server, unit tier. Record exact new selectors before their genuine missing-behavior RED, then GREEN and any necessary refactor. Do not manufacture historical RED for R2: its recorded fix-reverted checks remain mutation proof and its test-first chronology remains unproven. Original task's disclosed production-first exceptions remain visible.

Preserve these exact focused regressions:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::terminal_decisions_hold_their_copy_charge)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::held_changes_coalesce_into_the_next_revision)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::charged_items_are_taken_over_without_a_second_charge)'
```

Run each new named test exactly, then only the existing result module's narrow lane needed for the final write set, mise run fmt:check, git diff --check and these scoped owner checks:

```bash
mise exec -- cargo clippy --locked -p wyrd-server --all-features --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

Recorded run (sequential, all PASS):
- the two new exact selectors above;
- the three preserved regressions above;
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::results::/)'` (6/6);
- `mise run fmt:check`;
- both scoped clippy commands;
- `git diff --check`.

TDD compliance for R3 is PASS: both new selectors failed against the stub before the production bound or the runner change existed.

Limits:
- The bound covers the staged weight the ledger later reconciles to. It does not cover the encoder's transient builder capacity, because exact RSS accounting is excluded.
- Map keys that are numbers or booleans are charged a 40-byte text width.
- Raw JSON fragments are charged 18 bytes per character. `StoredOutput` contains none.

Write set: `verification/results.rs` and `verification/runner.rs` only. All non-goals stayed excluded.

All commands run through mise, with Cargo invocations sequential. If a genuinely necessary focused test uses Postgres, use scripts/postgres/with-test-postgres.sh; do not invent a database dependency for the pure sizing property. No full journeys, gate, benchmarks, synthetic host load or unrelated package lanes.

## Preserved scope and handoff

FIND-012-1/3/4/5/6/7/8 are closed. Preserve settlement ownership, accepted spend/deadlines/identity, withheld discarded output, workspace encoding/scratch/cleanup custody, conservative media admission, gateway read-through/participant retirement, docs, one ledger, background group commit, ACK-before-retirement and claim fencing. The only reopened boundary is this result estimate, its construction/charge handoff and directly affected tests/items. Apply mandatory substantive rustdoc to every changed declaration.

Do not edit architecture/wyrd-design.md or pre-existing local planning files. No migration, public API, dependency, testing control, compatibility path, scheduler, provider workflow or resource-policy decision. TASK-013/014 retain combined-server qualification. The separately approved workflow commit is not remediation work. No commit/push/Git configuration/stash is authorized by this review task.

Route directly to wyrd-implement. The next review compares this immutable candidate with its successor, checks FIND-012-2 closure and affected result seams, and carries all closed findings and unaffected acceptance forward. It must not restart the complete integration review.
