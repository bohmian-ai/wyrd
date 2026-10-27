---
id: TASK-006-R1-R1
kind: remediation
status: ready
spec: SPEC-benchmark-experiments
spec_revision: 14
requirements: [REQ-019, REQ-021, INV-001, INV-002, AC-010, AC-014]
depends_on: []
parent_task: TASK-006-R1
remediates: [FIND-006R1B-1, FIND-006R1B-5]
---

# Charge retained Variant memory and supply the missing diagnostic proof

Implement through `$wyrd-implement`. This bounded task closes two validated
findings; it does not authorize changing benchmark limits or redesigning the
write pipeline.

## Subject and authority

- Approved specification: `changes/active/benchmark-experiments/spec.md`,
  revision 14, especially REQ-019/021 and AC-014.
- Original task:
  `changes/active/benchmark-experiments/tasks/TASK-006-R1-simplify-verification-data-plumbing.md`,
  including its approved “build batches once and remove shared Scribe copies”
  revision and all preceding evidence.
- Original base: `55f87cef1d74886a73cedab8d5c0960e915717a9`.
- Original reviewed candidate: `c9794a0b0c2182e93dcb8d936d253cb465620faa`.
- This review's cumulative candidate: `cc0d9c86c049ee93cd3ee3f9012fbdd85c5b1b0a`.
- Validated diagnosis: [findings-validation.md](findings-validation.md).

## FIND-006R1B-5: retained allocations exceed the staged charge

The approved task requires retained allocated capacity charged while queued,
waiting, retrying and in flight. `EncodedVariant::from_serialize` and its JSON
constructor keep the buffers returned by the installed parquet-variant builder.
Those are growing Vecs returned without shrinking. Ordinary compound values can
therefore retain capacity beyond their encoded lengths.

`tables/staged.rs::Cell::owned_bytes` charges a Variant through
`EncodedVariant::encoded_bytes`, which adds lengths only. `StagedRow` adds inline
cell storage but cannot recover the omitted heap capacity. Verification inputs,
outputs and audit flow through this shared cell owner into `ScribeSink::weight`.
The outbox admits and moves the item without shrinking it or making another
charge for that capacity. Under backlog, live payload memory exceeds the fixed
byte budget's charge. The original review dismissed this as minor; that conflicts
with the explicit allocated-memory requirement.

Correct the estimate at the existing `EncodedVariant` ownership boundary and
consume it at the staged-cell memory boundary. Account for both retained buffer
capacities, with inline object/cell storage remaining charged once by its existing
owner. Preserve `encoded_bytes()` as encoded length: sibling value limits,
column builders, payload estimates and wire/frame sizing depend on that meaning.
Do not globally reinterpret a size accessor as heap memory.

This single owner correction reaches audit and all verification staged Variants,
including existing frame estimates through their delegation. There is no need
to shrink or copy buffers, add per-producer guards, redesign the outbox, add an
allocation registry or change a resource budget. Those alternatives add cost
or duplicate accounting without correcting the shared source more directly.

### Focused behavioral proof

Use a real compound value whose encoded buffers retain spare capacity. At the
Variant owner, independently compare the memory charge with the private buffers'
capacities and separately preserve encoded-length assertions. At staged-row and
server boundaries, prove that the extra retained allocation contributes to the
charge. Comparing two methods using the same estimator is insufficient.

Run RED against the current length-only staging charge, implement the common
owner correction, then GREEN the affected existing fixtures. Extend the existing
tests where that keeps each property clear; any new named test needs an exact
focused selector recorded before execution. Existing suitable extension points:

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::typed_values_encode_as_their_json_text)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::staged::tests::staged_rows_project_into_one_dense_batch)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=scribe_outbox::tests::staged_writes_charge_their_owned_memory)'
```

Keep text/null/Variant projection, existing limits, ordering and retry semantics.
New or changed Rust items require the repository's rustdoc. No public wire or
stored-data migration is involved.

## FIND-006R1B-1: required intervention measurements are missing

The Step 1–4 PASS rows cite correctness selectors. The task separately requires
profile/allocation evidence of eliminated validation copies, producer allocation
and frame geometry, local-versus-peer retained memory, and persistence copy
reduction without increased downstream merge work. None is supplied by those
selectors or the integrated smoke CPU figures. Relabeling the rows incomplete
is honest interim reporting, not closure of the proof obligation.

The evidence says all profiling must wait for a c64 driver-budget decision.
That is unsupported: the existing selection owner forwards `--steps` to the
prepass, and verification declares c1 separately. Reuse existing selected
profiling, for example:

```bash
mise run bench:l1:verification -- --steps direct-verify-c1 --profile
```

This is a diagnostic command under unchanged validity checks, not a guarantee of
validity or full acceptance. It does not alone measure every missing operand.
Use existing projection, local/peer delivery and persistence fixtures for
bounded observations where a CPU sampler cannot reveal allocation/copied or
retained bytes. Add temporary bounded probes only for those operands; remove
them before final acceptance. Add no permanent metrics, new harness or driver
budget workaround.

Append comparable before/after observations identified by source, configuration
and workload at the existing intervention commits and their predecessor/control:
`b47082554` (validation), `c5d0ed5ad` (dense staging and local delivery), and
`f50fdd9fb` (gathered persistence). Steps 2 and 3 share a commit; distinguish their
costs through their measured seams rather than fabricate separate source history.
Record the memory correction separately when it affects observed charges.
Do not reset or modify the working tree to reconstruct history; preserve other
work and the immutable reviewed candidates. Do not commit or push without caller
authorization.

The proof must cover:

| Intervention | Required observation |
|---|---|
| Validation | Discarded owned-buffer construction/copy eliminated; remaining validation distinguished |
| Dense staging | Producer allocations/copied bytes and actual rows/frame and decoded batch geometry |
| Local canonical | Local and peer retained memory separately, plus evidence local native decode is absent |
| Gathered persistence | Payload copy/allocation change and observed downstream merge work |

Retain numeric operands, units, method, source/configuration identity, profile
availability and limitations. Inclusive CPU shares are not additive savings;
sampler absence of a symbol alone does not quantify allocated bytes. Correct the
completion table according to the proof actually obtained. Missing observations
keep the corresponding obligation open.

## Preserved boundaries and non-goals

Preserve three canonical tables, exact values and public reads, full external
validation, canonical eligibility, identity/splitting, tenant attribution, WAL
fences, persistence preflight, key ordering, fail-fast bounded admission and
nonblocking audit. Keep the one-shard default, worker/concurrency bounds, resource
limits, workload and timing/validity criteria unchanged. Add no linger, admission
wait, larger queue, retries or relaxed SLO.

The original `SPEC_REVISION_REQUIRED` verdict is not authority for a budget or
criterion change. Escalate only a concrete necessary conflict with approved
behavior. Historical Gateway/Bifrost failures remain acceptance obligations.
This remediation does not select further performance changes before diagnosis.

## Acceptance and evidence

1. FIND-006R1B-5: real spare-capacity fixtures fail the old estimate and pass the
   corrected common owner; encoded-length semantics and existing staged projection
   remain unchanged. Run the exact focused selectors above and record results.
2. FIND-006R1B-1: the four intervention observations above are recorded with their
   source/configuration and limitations. Correctness test results are not substituted
   for cost measurements. Mark any missing proof incomplete.
3. Remove temporary probes; run `mise run fmt`, `mise run lints` and
   `mise exec -- git diff --check` for the final correction. Add only narrower
   affected-owner regression proof justified by the actual write set; do not run
   a broad journey suite solely for this remediation.

Append evidence to the original task without rewriting historical results. Return
the new immutable candidate and evidence for independent closure review. Both
reviews to date ran no benchmarks. Closing these findings does not establish P5:
the original task still requires its five clean normal full L1 experiments on
the final runtime candidate before acceptance. Partial/profiled/smoke or INVALID
records cannot replace them.
