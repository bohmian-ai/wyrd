# TASK-003 r1 verdict

**Verdict: FIX_REQUIRED.** SPEC-1 was settled by the user: Wyrd-specific
logic stays in Wyrd, and only logic that needs DataFusion, arrow-rs or
`datafusion-distributed` internals goes to the forks. Revision 26 was
reworded to match, and the write-side `parquet-variant*` use stays.

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003`,
  branch `wyrd/bifrost-variant/TASK-003`.
- Base `b145767dd`, candidate `861a0342f`. The candidate did not change
  during review.
- Fork pins (bohmian-ai):
  - DataFusion `a4ff14550`
  - arrow-rs `0d0eb24`
  - datafusion-distributed `0b81b12`
- Authority:
  - working-tree `changes/active/bifrost-variant/spec.md` and
    `spec-revision-26-datafusion-variant.md`;
  - the governing plan
    `tasks/TASK-003-native-datafusion-read-path-plan.md`;
  - `AGENTS.md`, `architecture/agent-rules.md`,
    `architecture/bifrost-design.md`.

## Independent review results

| Role | Report | Result |
|---|---|---|
| behavior-rev | `task-review-behavior.md` | FAIL (BEH-1..9) |
| invariant-rev | `task-review-invariants.md` | FAIL (INV-1..6) |
| repo-rev | `standards-review.md` | FAIL (STD-1..10) |
| maintainer-rev | `maintainer-review.md` | FAIL (MNT-1..9) |
| system-rev | `system-review.md` | FAIL (SYS-1..2) |
| reuse-rev | `reuse-review.md` | FAIL (REU-1..5) |
| domain-rev security/tenancy | `domain-review-security-tenancy.md` | FAIL (SEC-1..2) |
| domain-rev resource lifecycle | `domain-review-resource-lifecycle.md` | FAIL (RES-1..2) |
| domain-rev storage/durability | `domain-review-storage-durability.md` | FAIL (STO-1..2) |
| followup-rev | `followup-review.md` | RESOLVED (FUP-1..5) |
| ponytail-rev | `findings-validation.md` | 20 findings, 1 SPEC_REVISION_REQUIRED |

## Follow-up decision

A follow-up was needed, and it resolved three uncertainties:

1. INV-4 against SEC and STO on the tenant refusal: `spec.md:338-340`
   requires a refusal before execution, and the candidate has none.
2. BEH-4 disputed the recorded Group A diagnosis. The fixture fix is valid;
   only the stated mechanism is wrong.
3. INV-2's Analytical scan-count path, which no other reviewer traced, is
   confirmed as a double count.

This is the first TASK-003 review, so no root-cause pass was required.

## Reconciled acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| Analytical rebuilt on native `datafusion-distributed`; no Wyrd-invented DataFusion logic; deleted code not reintroduced | PASS except FIND-10, FIND-20 | behavior, invariants, reuse |
| Workers read only through the tenant footer proof | PASS | security, storage |
| Pre-execution tenant-prefix refusal (`spec.md:338-340`) | FAIL | FIND-12 |
| Peer contract binds the fork revision (rev 26, plan:165) | FAIL | FIND-1 |
| Live fragments carry pushed predicates under the signed digest | PASS | security; `leaf_predicates_round_trip_and_execute` |
| Live route event-time pruning | PASS | `event_time_predicate_selects_only_overlapping_routes`, journeys |
| Leader cleanup releases every owner on every terminal | PASS | resource lifecycle; Oracle journeys 53/53 |
| Scan statistics are accurate for Analytical queries | FAIL | FIND-11 |
| Planning respects the query deadline | FAIL | FIND-13 |
| AC-007 nested-leaf proof (List element, `unnest`, distributed) | FAIL | FIND-2 |
| REQ-019 ¶2 late-failure proof on all surfaces | FAIL | FIND-3 |
| REQ-034: a published key is never uploaded again | FAIL | FIND-14 |
| Latency gates (non-Variant/Struct unchanged; whole-Variant ≤ 2x; key reads faster) | PASS | Step 5 evidence |
| Generated artifacts and workspace checks green | FAIL | FIND-5, FIND-6 |
| Rustdoc and repository style rules | FAIL | FIND-7, FIND-15..18 |
| No dead or test-only production surface | FAIL | FIND-8, FIND-9, FIND-10 |
| No `parquet-variant*` dependency in `vala-bifrost-redux` (rev 26) | PASS after the revision 26 rewording | SPEC-1 (resolved) |
| Bench task matches its description | FAIL | FIND-19 |
| Recorded failure diagnoses are accurate | FAIL (docs only) | FIND-4 |

## Validated finding ledger

The full entries are in `findings-validation.md`:

- FIND-003-1 through FIND-003-20;
- SPEC-1 (resolved by rewording revision 26).

## Verification limits

- The validator ran `check:docs`, `check:examples`, `check:test-coverage`
  (red), `check:rustls-provider` (red, rejected as outside every gate) and a
  hakari diff (non-empty).
- `repo-rev` ran `check:proto-drift` (red) and the boundary checks, which
  pass.
- Candidate evidence:
  - `test:bifrost` green;
  - Oracle journeys 53/53;
  - `fmt` and `lints` clean;
  - fork `verify6.log` `ALL-OK`;
  - LTO benchmarks recorded.

## Prior-finding closure

None; this is the first review.

## Remediation

`TASK-003-R1-review-fixes.md` in this directory, routed to
`$wyrd-implement`.
