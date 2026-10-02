# TASK-002 cumulative review verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Original task base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Cumulative candidate: `e7d16b5bd622b9a564a49edb18239df7f209ca92`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Active replacement task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Original superseded task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Prior verdict, ledger and remediation: `review/TASK-002-r1/{verdict.md,findings-validation.md,TASK-002-R1-close-validated-graph-gaps.md}` in the same change packet.

This review covers the complete original-base-to-candidate range, including the original implementation, prior corrections and approved Revision 12 replacement. Revision 12 supersedes the original loader/keyed-normalization mechanics. Its accompanying architecture, skill and packet revisions are approved authority updates, not unrelated product changes. Navigation and subject provenance are recorded in `subject.md`; the diff is reproducible with `git diff BASE CANDIDATE`.

HEAD remained the candidate and tracked source remained unchanged through discovery, follow-up, independent validation and final artifact creation. Only this new review directory was written. No implementation, merge, push or deployment occurred.

## Independent review results

| Required review | Result | Report |
|---|---|---|
| Behavior implementation | FAIL | [task-review-behavior.md](task-review-behavior.md) |
| Invariant implementation | FAIL | [task-review-invariants.md](task-review-invariants.md) |
| Repository standards | FAIL | [standards-review.md](standards-review.md) |
| Maintainer | FAIL | [maintainer-review.md](maintainer-review.md) |
| System resilience | PASS | [system-review.md](system-review.md) |
| Tenancy/security domain | PASS | [domain-review-tenancy-security.md](domain-review-tenancy-security.md) |
| Registry durability/concurrency domain | FAIL | [domain-review-registry-durability.md](domain-review-registry-durability.md) |
| Focused follow-up | RESOLVED | [followup-review.md](followup-review.md) |
| Independent structured Ponytail validation | COMPLETE; six retained findings | [findings-validation.md](findings-validation.md) |

Each discovery role used a separate fresh agent. A separate fresh follow-up traced the newly exposed root-version path. Another fresh agent independently validated every proposal against source and callers; the orchestrator did not validate its own findings. All required reports are present with coverage and evidence limits.

## Reconciled acceptance matrix

The detailed independent matrices remain in the two implementation reports. This matrix reconciles their acceptance claims with the final validated ledger and stronger repository authority. Later task obligations remain excluded.

| Obligation | Cumulative implementation and credible proof | Result |
|---|---|---|
| REQ-001/002: native Workflow Card and inline/path/exact Agent references | Existing loader/reference visitor and Skald resolver lower the actual bundle; focused loader/client proof passes. | PASS |
| REQ-003/040, INV-014: versioned native Prompts and existing binder | Seven-card example retains native request bodies/variables; shared/server execution inspects downstream bound reviewer content. | PASS |
| REQ-013/013A: earliest pure and resolved validation | Loader and Skald existing rules validate graph, bindings, outputs and dialect; focused negative tests pass. | PASS |
| REQ-014: no unvalidated graph acceptance or dispatch | EffectiveSpecs validates sibling/external bodies before persistence; declaration-only tools preserved. | PASS for semantic validation; owner shape fails below |
| REQ-024, INV-007, AC-001/003/029: complete Native local/registered SDK paths within cleanup | Loading exists, but Rust executes only injected gateway; Python never executes; TS proves binding refusal only. | FAIL — FIND-TASK-002-7 |
| REQ-025/055, AC-030: automatic lazy refs and each-language authored journey | Shared source supports lazy reads; Rust omits public mixed authored/env path and required language proof remains incomplete. | FAIL in proof — FIND-TASK-002-7 |
| REQ-028, AC-002: preserve registration intents and exact relationships | Seven exact outcomes and Agent/Prompt relationships pass. Omitted/scoped Workflow root metadata fails preflight before existing durable version resolution. | FAIL — FIND-TASK-002-10 |
| REQ-029, INV-005: registered graph remains exact, Active and UID-bound | Registered traversal and exact assertions preserved; stale preflight replacement is refused. | PASS source; complete TS after-v2 execution proof remains FIND-TASK-002-7 |
| REQ-052, INV-008: declarative tool names and existing runtime ownership | Validation drops tool binding only on temporary values; persisted declarations and explicit caller dependencies remain. | PASS |
| REQ-054: exact three-SDK public contracts | Loading/selectors project shared owner. TS run accepts unknown input and erases canonical step/error result fields. | FAIL — FIND-TASK-002-8 |
| REQ-056: source provenance, exact reads, no hidden writes/secrets/privileges | Separate authored/registry stores, Active checks and tenant-authorized reads retained; collision and UID negative tests pass. | PASS source; distinct-body each-language execution proof remains FIND-TASK-002-7 |
| REQ-057/059, AC-031: existing owners, obsolete machinery removed, no new parser/cache/executor | Obsolete loading/normalization removed; existing loader, graph, runtime and EffectiveSpecs reused; Bundle hydration preserved. Changed free IO workflows violate mandatory owner shape. | FAIL — FIND-TASK-002-9 |
| INV-002/003: pure foundational contracts and declarative Cards | No IO/async/PyO3 added to spec contracts; live dependencies remain client/server/runtime-owned. | PASS |
| AC-006/013: negative semantic validation before side effects | Shared/PG invalid graph/binding/dialect/collision tests retain no-dispatch/no-write assertions. Non-pin root regression needs added proof. | PASS retained negatives; FIND-TASK-002-10 closure required |
| No Workflow principal, Service-root WyrdState, no privilege transfer | Existing provisioning excludes Workflow; shared loading uses requesting context; WyrdState unchanged. | PASS |
| No artifacts/disk publication in Workflow load; cancellation returns no partial Workflow | Runtime graph scope omits artifact work and owns call-local values; existing Service Bundle scope retained. | PASS source; boundary documentation fails below |
| Required struct-centered Rust | Changed graph/preflight/write IO operations remain free functions despite hard owner rule. | FAIL — FIND-TASK-002-9 |
| Required changed-item rustdoc | New loading boundaries omit required error/outcome/partial-loading contracts. | FAIL — FIND-TASK-002-5 |
| Module-top imports and bare signature names | New native signatures retain qualified std::result::Result. | FAIL — FIND-TASK-002-6 |
| Applicable focused checks and docs/skill synchronization | Independent focused loader/client/server/Rust SDK, formatting, docs and skill-sync checks pass; broader evidence has stated limits. | PASS for checks executed; green checks do not waive missing assertions |
| Shared selected gateway configuration, server jobs, CLI/all-route closure | Reserved to TASK-003/004/005; none demanded by this remediation. | PASS — excluded |

## Follow-up decision

[claim-comparison.md](claim-comparison.md) groups proposals by obligation. A follow-up was required because the durability reviewer exposed a fresh public registration path absent from the pinned-root proof used by the other reviewers. It traced omitted/scoped root metadata through graph-only projection, original preflight input, typed Workflow envelope conversion and the existing locked server version allocator. It resolved the uncertainty: legal authored root intents are refused before that allocator. Exact registered selectors and dependency pins do not justify a pinned-only registration contract.

No extra discovery pass was needed for corroborating Native/type claims. Every unique standards proposal still received independent validation. Broad owner refactors and hypothetical runtime/configuration changes were rejected as remedies; only the bounded existing-owner correction survives.

## Validated finding ledger

Only independently CONFIRMED or REVISED findings are included. Full source paths, producer-to-consumer evidence, consequences and decision-complete corrections are in [findings-validation.md](findings-validation.md).

| Stable finding ID | Status | Classification | Required correction |
|---|---|---|---|
| FIND-TASK-002-5 | REVISED | VIOLATION | Complete changed loading-boundary Rust error/outcome and partial-loading documentation; regenerate declarations. |
| FIND-TASK-002-6 | CONFIRMED | VIOLATION | Reuse module-top StdResult import convention in new native signatures. |
| FIND-TASK-002-7 | REVISED | MISSING | Complete public Native execution and authored/registered/pinning/negative journey proof in all three owning languages. |
| FIND-TASK-002-8 | CONFIRMED | INCORRECT | Project JSON input and complete canonical WorkflowRun step/error fields in TypeScript. |
| FIND-TASK-002-9 | REVISED | VIOLATION | Place changed graph/preflight/write workflows on meaningful owners while preserving all existing lifecycle and transaction boundaries. |
| FIND-TASK-002-10 | CONFIRMED | REGRESSION | Preserve omitted/scoped Workflow root registration intents using transient graph-only preflight metadata. |

## Prior-finding closure

| Prior stable finding | Disposition |
|---|---|
| FIND-TASK-002-1 | Invalid source production repaired at WorkflowBodies/EffectiveSpecs. Negative collision proof retained; remaining positive per-language distinct-body proof is explicitly covered by FIND-TASK-002-7. |
| FIND-TASK-002-2 | Closed: three exact-version Prompt Cards and both relationship layers in the actual seven-card example. |
| FIND-TASK-002-3 | Superseded and closed under Revision 12: keyed normalization and sole-use metadata deleted; canonical untagged forms retained. Do not rebuild the old remedy. |
| FIND-TASK-002-4 | Closed: expected UID reaches Active exact-UID lock/recheck and refusal precedes persistence; focused replacement test passes. |
| FIND-TASK-002-5 | Historical locations corrected/deleted, same mandatory documentation obligation remains at new loading boundaries; stable ID retained. |
| FIND-TASK-002-6 | Historical locations corrected/deleted, same import/signature violation remains in new native module; stable ID retained. |

## Verification evidence and limits

[verification.md](verification.md) records exact commands and logs. Independently passed:

- retained loader example test: 1 selected / 1 passed;
- shared-client loading test: 1 / 1;
- the three exact server registration/loading/UID-replacement selectors: 3 / 3;
- ignored Rust SDK loading journey explicitly selected: 1 / 1;
- managed Postgres migration setup;
- read-only `mise run fmt:check`, `git diff --check`;
- `mise run docs:check` including generation drift, commands, links, build and 63-page a11y;
- `mise run check:skills-sync`.

Python/TS and broader lane PASS results are recorded implementation evidence, not independent reruns in this review. Their test source was inspected. Native execution assertions are absent from the current journeys, so replaying those green tests cannot close FIND-TASK-002-7. The version-intent regression follows a deterministic resolved_pin branch; this review does not claim a runtime reproduction for it. Static hard-rule findings are not disproved by compiling/linting. No crash/outage qualification beyond the reports' explicit source/ownership assessments is claimed.

## Verdict and handoff

**FIX_REQUIRED**. Six bounded findings remain under approved Revision 12. No specification revision or new product, public API, security, concurrency or persistent-data decision is required.

Remediation: [TASK-002-R2-close-cleanup-review-gaps.md](TASK-002-R2-close-cleanup-review-gaps.md), ready for `$wyrd-implement`. Review does not execute that remediation. Re-review the original cumulative base-to-new-candidate range against the approved specification, replacement task, both prior ledgers and this remediation after implementation.
