# Verdict: FIX_REQUIRED

The original `SPEC_REVISION_REQUIRED` conclusion is unsupported. Missing or
invalid performance evidence does not establish that a compliant fix requires
changing the approved design or limits. Two independently validated findings
remain: **FIND-006R1B-1** and **FIND-006R1B-5**.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Original base: `55f87cef1d74886a73cedab8d5c0960e915717a9`.
- Original candidate: `c9794a0b0c2182e93dcb8d936d253cb465620faa`.
- Current candidate: `cc0d9c86c049ee93cd3ee3f9012fbdd85c5b1b0a`.
- Authority: approved spec revision 14, original TASK-006-R1 and its final
  approved batch-once revision, applicable repository and architecture rules.
- Request: review the findings in
  `../TASK-006-R1-batch-once-c9794a0b0/`, including their source basis,
  materiality, corrections and verdict; check claimed closure in
  `c9794a0b0..cc0d9c86c`. This is not a replacement comprehensive implementation
  acceptance review.

The subject remained unchanged throughout review. Existing source-backed
observations inform this bounded review; implementation-wide PASS claims from
the original reports are not newly certified. No rebase correspondence was
needed for the closure range. The original scope's earlier patch correspondence
is retained as context, not new runtime evidence.

## Independent review and reconciled matrix

Fresh `behavior-rev` inspected the reported claims and affected source paths;
a separate fresh `ponytail-rev` independently validated every proposed finding,
challenged dismissal and correction. Their reports are
[task-review-behavior.md](task-review-behavior.md) and
[findings-validation.md](findings-validation.md). Selection and navigation are
recorded in [scope.md](scope.md). No additional specialist was selected for this
bounded findings review. No unresolved material conflict or validator-originated
new blocker required a follow-up.

| Obligation or claim | Source and verification evidence | Result |
|---|---|---|
| Required per-intervention diagnostics | Task Steps 1–5/P5 require allocation, copy, geometry and separate local/peer retained-memory observations; execution evidence still supplies tests and integrated timing instead | FAIL — FIND-006R1B-1 remains open |
| Charge retained allocated memory | `EncodedVariant` retains growing Vecs; staged cells charge `encoded_bytes()` lengths; outbox admits the moved item using that estimate | FAIL — FIND-006R1B-5, revising original invariant O-1 |
| Mixed fallback as material drift | Original reviewers established no supported mixed destination; closure removes fallback without affecting homogeneous projection | Rejected as blocker; cleanup closed |
| Required panic documentation | Current source adds the two missing `# Panics` sections | PASS — FIND-006R1B-3 closed statically |
| Evidence accuracy and replacement map | Current task corrects missing audit mapping, capacity wording, commit identity and P3 selector | PASS — FIND-006R1B-4 closed statically |
| P5 normal performance acceptance | Historical five-experiment records are INVALID; no fresh benchmarks were run by either review | FAIL — acceptance remains outstanding |
| Specification revision is necessary | No unavoidable conflict with approved behavior or limits was demonstrated; REQ-021 requires optimization until unchanged limits pass | Original escalation rationale rejected |

The ledger retains FIND-006R1B-1 and assigns the next unused ID,
FIND-006R1B-5, to the independently confirmed capacity defect. FIND-006R1B-2
does not create further remediation. The full source evidence, consequence,
materiality, correction burden and closure proof are in findings-validation.md.

This is the first independent review of these reported claims, not a third
unsuccessful remediation review. No convergence assessment is triggered.

## Verification limits and next action

**The original review did not run benchmarks. Neither did this review.**
The cited records identify `f50fdd9fb`; only task Markdown changed between
that commit and `c9794a0b0`, so their runtime-source attribution carries across
that interval. They remain historical INVALID records, not current performance
acceptance. Verification stopped at the invalid smoke prepass: smoke latency
cannot establish the full three-repetition median outcome. Historical Gateway
and Bifrost failures remain obligations; their age does not waive them.

No builds or tests were rerun here. Original focused-test results remain
attributed evidence only. The capacity correction requires new focused proof;
final task acceptance requires clean, normal full P5 results on the final
runtime source. This review changes no criterion, budget, topology or approval.

Route the bounded [remediation task](TASK-006-R1-R1-owned-memory-and-diagnostic-evidence.md)
to `$wyrd-implement`. Closing it does not automatically PASS the original task:
P5 and the independent final acceptance gate remain outstanding.
