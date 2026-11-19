# TASK-001 second remediation review scope

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Initial candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Prior candidate: `96fea41e147c799354f797469260391d2fc237ff`.
- Current candidate: `625b7b84ccd7cfa8e7dc04da96d0a5c73910a785`.
- Review delta: `git diff <prior-candidate> <current-candidate>`, one descendant
  commit changing only `scribe/memtable.rs`. Full cumulative context is available;
  no rebase or patch correspondence issue appears.
- [Approved revision-2 spec](../../spec.md) and
  [original task](../../tasks/TASK-001-memory-variant-execution.md).
- [Prior verdict](../TASK-001-r2-2026-11-06/verdict.md) and
  [validated ledger](../TASK-001-r2-2026-11-06/findings-validation.md).
- [R2 task and implementation evidence](../TASK-001-r2-2026-11-06/TASK-001-R2-filtering-workspace-accounting.md),
  with original R1/prior reports available through the preserved review packet.
- SHA-256: spec `6709bdcd455c6b669ebd335af83412d5065622825795ac229dd2e2b6a01dfa9f`;
  original task `dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`;
  R2/evidence `8ff9c7eb48622c0012453d46cdf6a7ebc6761cf476cfc0cda7cb5c91516b99d1`.
- Source worktree has no tracked implementation modifications. Existing
  benchmark-scaling deletions and the untracked packet are outside the committed
  subject. No CodeGraph index exists. Reports only may be written here; no source,
  authority edits, diffs, patches or archives.

## Review scope and carry-forward

Check residual FIND-TASK-001-1: held filtering/reconstruction workspace, core-backed
reservation, refusal/release and one-charge handoff to retained output. Check
changed/affected paths and new peak-observation tests, not the full task afresh.

Carry forward FIND-2–5 closure and accepted unchanged matrix rows from the prior
independent ledger and its source-backed reports: actual follower/core-root
connection; original generation ownership; returned owner lifetime through capture,
source rewrites and streams; exact Variant/fallback/schema/cardinality semantics;
closure/tenant/assignment authority; test-support visibility/rustdoc; scalar
snapshot closure and corrected public-reachability rationale. Do not reopen an
accepted property without an affected path or new material evidence.

The maintainer requires one authoritative core Bifrost accounting source per
process. Query limits and attribution remain backed views, not new trackers or
independent budgets. R2 explicitly excludes a generic allocator/Arrow scratch
instrumentation project; distinguish held physical/reconstructed workspace and
retained output from dependency-operation scratch without using that distinction
to waive the actual diagnosed obligation. Source-check the limits claimed in the
appended evidence against the original task, remediation and resource authority.

This is the second remediation review. Only one remediation review has so far
been unsuccessful; if this review is also FIX_REQUIRED, include the required
convergence assessment before another packet. Do not expand expectations to
defend a prior finding or require a fake RED.

## Starting navigation map

The reviewer must independently verify and expand this map where needed.

- `ReadableSource::project`: existing query/workspace reservation; new `hold`
  callback resizes from existing `unshared_arrow_bytes`, excluding captured buffers;
  no-requirements filtered batch also reports workspace; free-before-output charge
  handoff. Trace success, zero survivors, failure and fallback paths.
- `AdaptedRequirements::evaluate`: new workspace callback after logical row
  construction and physical/restored filtering. Verify all held batches at these
  boundaries are represented together, sharing excluded, exact outputs preserved.
- `computed_live_outputs_respect_query_memory_budget`: new partial-filter/empty-
  output, retained-output handoff and tight-ceiling cases on existing resources.
  Validate actual input/selectivity/cardinality, root connection, peak measurement
  and failure cleanup rather than trusting final zero counters.
- Test-only `PeakPool`: pass-through to the real pool with peak observation;
  inspect all delegated memory operations, gating and mandatory rustdoc. An
  observational peak is not a second production tracker or budget if it does not
  own capacity or replace underlying accounting.
- Existing `unshared_arrow_bytes`, core pool/view/governor and source owner/drop
  paths are dependencies covered by the previous resource/validation reports;
  inspect affected interactions without re-inventorying unrelated owners.
- New assertion byte-count correction: recorded 136-byte-per-batch difference
  led to measuring retained outputs with the existing helper. Assess whether this
  preserves the intended real-accounting/handoff proof; do not treat assertions
  as automatically sound or require guesses to pass.

## Specialist applicability before review

| Contract or invariant | Selection and justification |
|---|---|
| Residual shared workspace ownership and handoff, exact filtering, core accounting/refusal | One fresh `ponytail-rev` checks closure and all changed/affected paths. The delta completes a local reservation callback under the already-audited root/owners, adding no new resource authority, lifetime protocol or representation. |
| Complex capture/source/generation lifetime and production core linkage | Carry prior fresh invariant report and independent validated matrix: they trace actual follower execution/root, generation charges and source/stream ownership. These production owners and transfer paths do not change; fresh validator checks how the local workspace interacts with them. No new invariant specialist selected absent a new material shared-state protocol/question. |
| Repository/maintainer standards | Carry prior source-backed authority/surface coverage; fresh validator checks changed functions/test helper and rustdoc. One runtime module changes, no new public surface, dependency, language, generated contract or replacement workflow requiring another broad specialist. |
| Security, system failure/recovery, concurrency, persistence, SDKs | Prior detailed reports/ledger cover unchanged identity/IO boundary, refusal propagation, topology, leases and corrected scalar behavior. No such contract changes in this local delta. Fresh validator must challenge this carry-forward and request targeted specialist review if actual behavior invalidates it. |

The selected fresh reviewer writes `findings-validation.md`. No discovery team or
follow-up is automatically repeated. A newly discovered blocker needs separate
focused source validation before retention; the validator must notify the
orchestrator rather than validate its own new finding. A new material unresolved
boundary similarly requires appropriate specialist coverage.

## Verification and handoff discipline

The appended R2 evidence supplies focused RED/GREEN, result/leaf regression,
46 owner tests and static gates; these are claims to source-check. No intended
verdict or implementation-agent summary is supplied to the reviewer. Test fixes,
scope limits and source behavior must be reconciled with stronger authority.
Review is static unless the orchestrator coordinates a needed focused runtime
check. No broad journey/gate/L1 runs belong to this bounded remediation. Verify
candidate, authority and evidence identity before final verdict. No source edits,
commits, pushes or implementation actions occur during review.

## Independent focused runtime evidence

The orchestrator coordinated and executed this sole Cargo command, with the
validator performing source inspection only and no concurrent builds:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget) or test(=scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction)'
```

Result: exit 0, 2/2 PASS, 920 tests unselected, default nextest profile; run ID
`6f1f9576-a6dc-4ed4-a9b2-7afc7a566595`. This independently reproduces the new
workspace-budget proof and affected leaf/result/cardinality regression. It does
not constitute broad journey, gate or performance evidence. The result was sent
to the validator for reconciliation with its independent source inspection.
