# TASK-001 second remediation verdict

**PASS.** FIND-TASK-001-1 is closed; findings 2–5 remain closed. The independently
validated retained finding ledger is empty. This is TASK-001's completion gate,
not completion of the full change or permission to merge, push or deploy.

## Immutable subject and acceptance context

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Initial candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Prior candidate: `96fea41e147c799354f797469260391d2fc237ff`.
- Reviewed candidate: `625b7b84ccd7cfa8e7dc04da96d0a5c73910a785`.
- Primary delta: `git diff <prior-candidate> <reviewed-candidate>`, one descendant
  commit, only `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs`; cumulative
  original-base context remains available. No ancestry correction was needed.
- [Approved spec revision 2](../../spec.md),
  [original TASK-001](../../tasks/TASK-001-memory-variant-execution.md),
  [implemented R2/evidence](../TASK-001-r2-2026-11-06/TASK-001-R2-filtering-workspace-accounting.md).
- [Prior verdict](../TASK-001-r2-2026-11-06/verdict.md),
  [prior validated ledger](../TASK-001-r2-2026-11-06/findings-validation.md),
  [round scope and hashes](scope.md).

Candidate, implementation worktree and authority/evidence hashes remained
unchanged. Existing benchmark-scaling deletions and the untracked change packet
are outside the committed implementation subject. Review writes are reports only.

## Scope, coverage and reviewer selection

This is focused closure of residual FIND-TASK-001-1 and its affected paths,
carrying forward the prior source-backed acceptance and four closed findings.
No unaffected full-task discovery was repeated. [Scope](scope.md) records
contracts, navigation, selection and omissions before reviewer work.

One fresh independent `ponytail-rev` inspected the entire one-file delta, full
changed methods, affected callers/siblings, source sizing and core reservation
interactions, tests, new test helper and mandatory Rust documentation. Its
[findings-validation.md](findings-validation.md) independently validates both
closure and specialist coverage and recommends PASS with an empty ledger.

Prior fresh invariant and validation reports establish actual server follower
pool/core-root connectivity, original generation charges and source/stream
reservation lifetime. This delta completes the same local workspace mechanism
without a new resource authority or ownership protocol. Prior behavior,
maintainer, standards, security and system reports supply unaffected contract
coverage; their relevance was checked against the actual changed paths.

No additional invariant, repository, maintainer, system or sensitive-domain
specialist was required: no new budget, tracker, lifecycle protocol, trust
boundary, public/SDK/generated interface, dependency, writer, lock, deployment
or recovery behavior appears. The test-only pool wrapper delegates all capacity
and mutation to the real pool. No material conflict, missing coverage or new
validator-discovered blocker required a focused follow-up. The corrected scalar
reachability assessment from R2 carries forward unchanged.

## Finding closure and current ledger

| Stable ID | Reconciled disposition |
|---|---|
| FIND-TASK-001-1 | CLOSED. Physical/filter and reconstructed batches are reported together to the existing core-backed workspace reservation; refusal unwinds before publishing a read, and temporary ownership releases before output accounting to avoid double charging. Positive-peak, refusal and handoff proof now cover the diagnosed branch. |
| FIND-TASK-001-2 | CLOSED, carried from R2. Execution-time scalar snapshots remain correct; the previously claimed public SQL regression remains unproven and is not asserted here. |
| FIND-TASK-001-3 | CLOSED, carried from R2. Recursive closure/type validation remains before tail access. |
| FIND-TASK-001-4 | CLOSED, carried from R2. Private payload operations remain internal and observation remains behind existing test-support. |
| FIND-TASK-001-5 | CLOSED, carried from R2 and current changed-item inspection. Documentation reflects actual invariants and ownership. |

**Current independently validated ledger: empty.** No new FIND ID, remediation
task, specification revision or additional corrective machinery is required.

The key closure is at `ReadableSource::project` and
`AdaptedRequirements::evaluate`: the callback sizes the batches held by the
evaluator using the existing alias-aware helper, excluding original captured
buffers. Logical fallback and filtered physical/restored batches use that same
reservation. The no-requirements filtered sibling also reports its held batch.
Errors and zero-survivor paths release through existing local ownership; returned
arrays retain the existing query owner through the previously audited source and
stream lifetime. No new accounting source is introduced.

The new assertion's byte-count correction is independently accepted: raw value
bytes omitted ArrayData metadata counted by the existing helper. Measuring
returned allocations retains independent nonzero/lower-bound, root/pool equality,
peak handoff and capacity-refusal assertions. The validator's source diagnosis
confirms the recorded explanation; the property was not weakened to clear a
failure. PeakPool is test-only observation and has no budget authority.

## Reconciled acceptance matrix

Detailed paths and carried-forward proof are in the
[independent matrix](findings-validation.md#reconciled-acceptance-matrix).

| Obligation | Source/proof disposition | Result |
|---|---|---|
| Baseline cause and existing source/adapter reuse | Prior behavior/maintainer traces and original RED/work observations remain valid; delta extends the same owner. | PASS |
| Ordered requirements, aliases/schema and exact-versus-pruning | Prior accepted pipeline/snapshot/closure validation; callback changes accounting only. | PASS |
| Learned scalar/nested/list/null/IN, residual/full-root/fallback and zero-column rows | Existing evaluator/conversion semantics preserved; focused leaf/result test rerun passes. | PASS |
| Held physical/reconstructed workspace governed by one core budget | Existing core-backed reservation now receives the diagnosed batches together; actual partial-filter/empty-output proof observes a positive peak and capacity refusal. | PASS |
| Retained-output/source sharing, no-double-charge handoff and release | Shared sizing/captured exclusion and free-before-output charge; exact retained root/pool/peak equality plus refusal/drop assertions. | PASS |
| Tenant/table/version/assignment/schema/permission/deadline authority | Prior security and R2 guard proof carried forward; no changed boundary. | PASS |
| Fixed source cohort, generation/staged leases, source rewrites/stream/cancellation lifetime | Prior source-backed invariant/system traces and P6; existing retained owners unchanged, local error unwind inspected. | PASS |
| Statistics, once-only rows, unknown/zero and non-file IO meaning | Prior validated producer/statistics/measurement proof; no affected logic. | PASS |
| Staged/published reader/cache preservation | Prior task-local source/reader evidence carried forward; no file path changed. Full file acceptance stays TASK-002. | PASS for TASK-001 |
| Private API, changed-item docs and excluded dependency/persistence/planner changes | Actual one-file delta and test-private wrapper inspected; existing owners reused. | PASS |
| Proof integrity and original diagnosis | New byte-size correction source-validated; existing literal/result assertions and corrected scalar rationale retained. | PASS |
| Performance and integrated qualification scope | No performance claim from owner tests; L1 remains TASK-003 and broader closeout remains change review. | PASS for task boundary |

## Verification and limits

The orchestrator independently ran:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget) or test(=scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction)'
```

**2/2 PASS**, exit 0, 920 unselected, default profile, run ID
`6f1f9576-a6dc-4ed4-a9b2-7afc7a566595`. The validator inspected source independently
and reconciled this result with actual assertions. Appended implementation
evidence separately reports pre-fix RED, 46/46 owner tests, format/lints,
default-feature clippy and diff checks passing; those broader results were not
rerun during review. Recorded mutation proof is source-coherent but was not
independently repeated.

This PASS covers Wyrd-held physical/reconstructed batches and retained output at
the defined accounting boundaries. It does not claim comprehensive interception
of every temporary dependency-kernel allocation or preallocation through an Arrow
allocator; R2 explicitly excludes that broader project. This boundary is not an
exemption for the diagnosed copied RecordBatch, which is now accounted. Full
journeys, file consolidation and L1 performance remain assigned to their existing
tasks/change review.

No convergence assessment is triggered: there was one unsuccessful remediation
review and this second remediation review passes. No further remediation packet
is prescribed. Review does not edit task source, commit, merge, push or deploy.
