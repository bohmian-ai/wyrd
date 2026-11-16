# Independent findings validation — TASK-001 second remediation

Recommendation: **PASS**. FIND-TASK-001-1 is closed. The independently validated current retained ledger is **empty**; findings 2–5 remain closed.

## Subject, authority and scope

Reviewed immutable delta `96fea41e147c799354f797469260391d2fc237ff` → `625b7b84ccd7cfa8e7dc04da96d0a5c73910a785`, with original base `5234961d51798581a026834cfffd5efaadc226b4`, approved spec revision 2, original TASK-001 and both preserved remediation rounds as context. HEAD matches the candidate. The delta changes only `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs`; no reviewed-source modifications are present. Existing benchmark-scaling deletions and the untracked packet are outside that committed subject.

Read the review skill and scope first, then spec, original task, R1/R2 verdicts and validated ledgers, R2 remediation/evidence, relevant preserved behavior/invariant/maintainer/standards/security/system coverage and scalar follow-up. Applied root AGENTS.md, change workflow, Rust/maintainer guidance and the Bifrost resource/failure authority. No CodeGraph index or applicable nested AGENTS.md exists. SHA-256 matches scope: spec `6709bdcd455c6b669ebd335af83412d5065622825795ac229dd2e2b6a01dfa9f`; task `dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`; R2/evidence `8ff9c7eb48622c0012453d46cdf6a7ebc6761cf476cfc0cda7cb5c91516b99d1`.

This is bounded residual-finding closure, not another full-task discovery. I inspected full changed bodies, the affected callers and sibling path, the existing allocation-size helper, reservation/root interactions and tests. Only this report was written; no source, authority, Git state or prior report was changed.

## Coverage and specialist applicability

| Boundary | Independently checked coverage and selection |
|---|---|
| Held physical/reconstructed workspace, refusal, release and handoff | Fresh validator covers the full local delta and affected paths. `ReadableSource::project`, `AdaptedRequirements::evaluate`, `LiveCapture::read`, the test-only `collect_readable_batches` caller, logical conversion helpers and `unshared_arrow_bytes` were inspected. No new resource authority or ownership protocol appears. |
| Production core linkage and original/source/stream lifetime | Carry R2 `task-review-invariants.md` and `findings-validation.md`: actual server follower → session pool → capture → LiveReadOwners → source rewrites/open stream, with generation charges separately retained. Independently checked existing `ScribeResources::follower_execution`, root/view growth and pool delegation against the new callback/wrapper. Those previous traces remain valid. Another invariant specialist is unnecessary for completing this existing local reservation. |
| Requirement authority, tenancy, permissions, schema and file siblings | Carry R1 `domain-review-security.md`, R2 validated FIND-3 closure, and original behavior/system coverage. The delta receives already validated requirements and does not change assignment, transport, source selection, lease, footer or authentication owners. No new sensitive-boundary question requires a domain specialist. |
| Existing-owner reuse and repository standards | Carry R1 maintainer/standards surface coverage and R2 corrected API/rustdoc closure. Fresh inspection covers changed methods, test and every PeakPool item/field: documented workflow, error propagation and panic invariants remain present. The callback extends the existing owner; no public surface, dependency, generated contract or alternate workflow is added. No broad maintainer/repository specialist is required. |
| Failure, concurrency and persistent behavior | Carry prior system/invariant coverage of startup failure, cancellation, source retirement, staged leases and remote errors. New refusal unwinds the same synchronous read before a provider is returned. No lock, scheduler, retry, durable state or recovery transition changes; no additional system/persistence/concurrency specialist is needed. |
| Scalar outputs and other unchanged acceptance | Carry R2 findings-validation and followup-review, including withdrawal of the earlier unsupported public scalar-regression rationale. No new planner or scalar behavior appears. |

The selected fresh validator and preserved specialist reports cover the applicable obligations. No missing specialist, material conflict, new blocker or untraced common cause calls for a follow-up review. No additional discovery agent was needed.

## FIND-TASK-001-1 closure

**CLOSED.** The prior finding concerned a Wyrd-held copied physical/filter RecordBatch that could disappear before a zero-column output received its final charge. The correction completes the existing workspace reservation rather than adding an independent accounting source.

At `scribe/memtable.rs:2331–2347`, `project` creates `workspace` with `query.new_empty()` and builds `hold` over that reservation. It resizes to existing `unshared_arrow_bytes(held, &[captured])`, excluding captured allocations and deduplicating shared buffer allocations across the reported batches. `scribe/memory.rs:57–68,96–133` remains the shared sizing owner; no second byte walker or production ledger was introduced.

At `AdaptedRequirements::evaluate:2659–2683`, fallback reconstruction reports both physical and logical batches before evaluating the fallback predicate. After exact filtering it reports filtered physical and restored logical batches **together**, so neither held representation is omitted and shared allocations are excluded together. Output fallback at `:2713–2719` reports the same pair when logical rows first become necessary. Adaptation, exact masks, expression order/types, surviving row count and statistics behavior otherwise remain unchanged. The no-requirements pruning sibling reports its copied `kept` batch at `project:2388`; its existing zero-row early return retains no copied data batch.

Budget refusal is propagated through the existing internal Scribe error path. `LiveCapture::read:1996–2037` returns a CapturedRead only after all sources succeed. A failed hold, fallback, batch assembly or final output growth unwinds the local workspace, previous output reservation and partially constructed batches without publishing a partial provider. Zero survivors return `None`; the workspace is freed at the next evaluated batch or on method exit. It cannot escape the call. Successful output construction drops evaluation-local physical/restored owners before returning to `project`; `workspace.free()` at `:2399` precedes final `query.try_grow`. Output-retained allocations consequently move to the existing retained owner without simultaneous workspace/output charging. The original source charge and output-owner lifetime are untouched.

Existing production pool connectivity is preserved: `resources.rs:1339–1349` issues the follower execution from the shared memory root. `GovernedMemoryView:3706–3760` delegates growth/shrink to that root; `GovernedMemoryRoot::try_grow:3535–3571` checks the view ceiling, reserves governor capacity and grows the shared pool with rollback on refusal. `new_empty` remains on the supplied query pool. This is the previously validated core-backed mechanism, not another memory budget.

### Focused proof assessment

The extended `computed_live_outputs_respect_query_memory_budget` builds three real 1024-row Int64 batches and a successfully adapted `value % 2 = 0` filter. Each has exactly 512 survivors. With empty outputs it asserts three zero-column, 512-row batches, a positive observed pool peak of at least one survivor allocation, no retained output bytes and root release. With bare `value` output it measures retained bytes, checks their nonzero copy-size lower bound, exact pool/root attribution, exact peak equality and release on drop. A query ceiling of half the raw survivor bytes refuses the empty-output read and leaves both pool and root without residue. Existing computed multi-batch refusal and captured-buffer sharing assertions remain intact.

`PeakPool:3744–3854` is inside the existing `#[cfg(test)]` module. Registration, unregistration, infallible/fallible growth, shrink, reserved bytes and limit all delegate to the supplied real pool. `record` observes `inner.reserved()` only after successful growth; its AtomicUsize is a test maximum, not a capacity ledger or budget. The fitting count proves positive accounting through this real view. Although its transient core snapshot is not sampled directly, source delegation establishes that the observed growth reaches the governor/root; retained-output assertions also independently inspect root attribution. Final zero alone is not being used as peak proof.

The 12696-versus-12288 assertion correction preserves the property. The rejected expectation was raw `3 × 512 × 8`; the established helper also counts ArrayData metadata (the recorded 408-byte difference is 136 per batch). Measuring retained allocations with that existing owner matches the property's definition. The separate lower bound, exact root/pool equality, exact peak equality and refusal checks remain; changing the expectation did not weaken handoff or introduce a new accounting definition. The appended diagnosis is independently supported by the source. The implementation evidence also reports that moving free after grow makes the peak assertion fail; this mutation result was not rerun here.

### Operation-scratch scope

The appended phrase “as the task allows” should be read against the actual bounded remediation: R2 expressly excludes arbitrary Arrow scratch instrumentation and a general allocation framework. It does not grant a blanket exemption for Wyrd-held copied batches. This candidate accounts the diagnosed physical/restored batches at their ownership boundaries and refuses uncovered capacity. Boolean masks and temporary arrays inside expression/kernel execution still are not comprehensively intercepted; growth is checked after materialization, not by a preallocating Arrow allocator. Neither the authority nor this review claims a process-wide allocator bound or complete dependency-scratch accounting. Extending this closure into such a project would exceed the selected correction and is unnecessary to close the source-proven finding. Existing retained-output charging remains in place.

## Prior finding disposition and current ledger

| Stable ID | Disposition and evidence |
|---|---|
| FIND-TASK-001-1 | CLOSED by current callback, shared sizing, refusal/release/handoff source and focused positive-peak/refusal tests. Previously closed production-root, source-charge and output-lifetime portions remain closed. |
| FIND-TASK-001-2 | CLOSED, carried from R2 validation/follow-up. Per-execution snapshot invariant preserved; public SQL fix-dependent reachability remains unproven and is not claimed. |
| FIND-TASK-001-3 | CLOSED, carried from R2 recursive signed-closure/type guard and recording-tail proof. No change to this boundary. |
| FIND-TASK-001-4 | CLOSED, carried from R2 visibility/caller/test-support proof. PeakPool is test-private and adds no public execution API. |
| FIND-TASK-001-5 | CLOSED, carried from R2 corrected docs plus fresh changed-item documentation inspection. |

**Final deduplicated retained ledger: empty.** No new FIND ID, correction or remediation task is recommended. The smallest safe correction is the existing reservation completion already present; no additional machinery is justified.

## Reconciled acceptance matrix

| Obligation | Implementation / carried-forward evidence | Verification evidence | Result |
|---|---|---|---|
| Baseline cause; existing source/adapter/workflow ownership | R1 behavior/maintainer traces, carried by R2 validation; existing owners unchanged | Original V2 RED/GREEN and V3 observations | PASS |
| Ordered local/remote requirements, aliases/schema and exact-versus-pruning meaning | R2 accepted pipeline/snapshot/closure matrix; new hold calls change no expression meaning | Prior V1/V3/V4 and P2–P5 | PASS |
| Learned scalar/nested/list/null/IN, full-root/residual/fallback and zero-column cardinality | Existing adapter/conversion/evaluator source; current callbacks preserve logical results and explicit row count | Prior V2/V3/P6; current covered-path and workspace tests | PASS |
| Current-batch physical/reconstructed workspace uses core-backed capacity and releases | Fresh source trace above, same query reservation and shared sizing | Positive peak, tight refusal, exact zero-column rows and root/pool release | PASS |
| Single core accounting; retained outputs, captured exclusions and no-double-charge handoff | Existing root/view, LiveReadOwners and source charges; free before final output growth | Exact retained root/pool/peak equality, existing sharing and multi-batch refusal assertions | PASS |
| Tenant/table/version, signed closure, permission/deadline and schema authority | R1 security/system plus R2 FIND-3 closure source evidence; no affected producer/validator | Prior V1/V3 negatives and P3 | PASS |
| Fixed source cohort, original generation/staged leases and clone/stream/cancellation lifetime | R1 invariant/system and R2 fresh invariant/validation traces; same owners | Prior V3/V5/P6 and owner-family evidence; new refusal/drop checks | PASS |
| Conservative statistics, once-only rows and unknown/zero versus compressed IO meanings | R2 accepted source/statistics/measurement matrix; no producer/aggregation change | Prior V6 and existing source tests | PASS |
| Staged/published sibling reader/cache preservation | Prior maintainer/security/system source evidence; no file path changed | Preserved task-local evidence; full file acceptance stays TASK-002 | PASS for TASK-001 scope |
| Private API, mandatory changed-item docs, existing dependencies and no extra resident representation | Current immutable one-file delta and documented private test wrapper, prior visibility closure | Source review; reported format/lints/default clippy | PASS |
| Assertion diagnosis and proof integrity | Source-supported metadata-size diagnosis; lower bound, root/pool and peak properties retained | Current focused tests; supplied RED and mutation evidence distinguished below | PASS |
| Performance and integrated qualification assigned to their owners | No latency/CPU claim from unit tests | TASK-003/change review retain L1 and integrated gates | PASS for task boundary |

## Verification and limits

The orchestrator independently ran the exact two focused tests in one nextest selection: `scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget` and `scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction`. Result: **2/2 PASS**, exit 0, 920 unselected, run `6f1f9576-a6dc-4ed4-a9b2-7afc7a566595`. This validator performed static source inspection and did not run Cargo.

Appended implementation evidence reports expected pre-fix RED, 46/46 affected owner tests, format/lints/default-feature clippy and diff checks passing. Those results are supplied evidence, not independent reruns by this validator. Forced adapter-error fallback, zero-survivor cleanup and some lifecycle branches have static source/carry-forward proof rather than a newly added dedicated runtime test. No source gap invalidates that proof. Full journeys, file consolidation and performance qualification remain with change review/TASK-002/TASK-003.

This recommendation does not trigger the two-unsuccessful-remediation convergence requirement: the present second remediation review passes. No scope expansion or new design decision is needed.

Skipped: broad builds, journeys, benchmarks and unaffected whole-task rediscovery. Risk/limit: dependency-operation scratch is not comprehensively intercepted; this PASS covers the governed held-batch/retained-output obligation and task scope described above.
