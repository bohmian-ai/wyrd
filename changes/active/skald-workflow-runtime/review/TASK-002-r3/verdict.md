# TASK-002 cumulative re-review verdict

**FIX_REQUIRED — reviewed candidate `8a8282331042c3cc7610478b3d46583dbbf121f8`.**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Original cumulative base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `8a8282331042c3cc7610478b3d46583dbbf121f8`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original superseded task: `tasks/TASK-002-load-and-register-graphs.md` in that packet
- Active replacement: `tasks/TASK-002-cleanup.md`
- Prior reviews: `review/TASK-002-r1` and `review/TASK-002-r2`, including verdicts, ledgers and remediation tasks
- Remediation reviewed: `review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md`

The complete original-base-to-candidate diff was reviewed through an immutable candidate archive. [Subject](subject.md) records navigation and exclusions. Live HEAD advanced to `375d97e67f3affe0d5c59727ef3135b22a459140` during discovery; the selected candidate object and archive stayed unchanged, and archive contents matched a fresh extraction of the same object. This verdict **does not review or approve the later Workflow error-code commit or its verification**. The user-selected error outcome is authority for the correction, not proof that its later implementation passes. Review changed only this new report directory; no implementation, commit, merge, push or deployment occurred.

## Independent review results

| Review | Result | Report |
|---|---|---|
| Behavior | FAIL | [task-review-behavior.md](task-review-behavior.md) |
| Invariants | FAIL | [task-review-invariants.md](task-review-invariants.md) |
| Repository standards | FAIL | [standards-review.md](standards-review.md) |
| Maintainer | FAIL | [maintainer-review.md](maintainer-review.md) |
| System resilience | FAIL for selector metadata; no additional resilience defect | [system-review.md](system-review.md) |
| Tenancy/security | FAIL for selector metadata; authorization and tenancy source checks pass | [domain-review-tenancy-security.md](domain-review-tenancy-security.md) |
| Registry durability/concurrency | PASS | [domain-review-registry-durability.md](domain-review-registry-durability.md) |
| Focused follow-up | RESOLVED | [followup-review.md](followup-review.md) |
| Structured Ponytail validation | COMPLETE; six retained findings | [findings-validation.md](findings-validation.md) |

Each role used a separate fresh agent. All required reports are complete. The orchestrator routed inputs and packaged the independently validated ledger; it did not validate its own findings. [Claim comparison](claim-comparison.md) groups proposals by obligations. Follow-up was required for the disputed per-language relationship proof and the unique async filesystem proposal. It resolved both, followed by independent validation of the entire proposal union.

## Reconciled acceptance

The full obligation matrices remain in the two implementation reports; this table reconciles them with authoritative scope and validated findings.

| Obligation | Evidence and disposition | Result |
|---|---|---|
| Native Workflow/Agent/Prompt graph, binder and runtime reuse | Existing loader, canonical visitor and Skald lowering retained; public Native execution now succeeds in all three owning runtimes. | PASS |
| Early resolved validation and declaration-only registration | EffectiveSpecs validates before writer; invalid binding/route/provenance graph assertions retain no-write boundaries. | PASS |
| Legal omitted/scoped Workflow root versions | Graph-only holders permit preflight; original submissions still drive hashing/replay/allocation. PG assertions register and reload 0.1.0/2.0.0 and refuse invalid bindings. | PASS |
| Local, mixed and shadowed provenance; lazy caller-authorized registry reads | Separate local/registered bodies and distinct rendered outputs, ambient Rust child and language refusal cases establish intended paths. | PASS |
| Exact Active UID fences, transaction/audit/version/relationship ownership | Expected UID survives preflight to FOR SHARE write recheck; race/lifecycle test source retains rollback and lock assertions. | PASS |
| Full canonical TS input/result typing | JsonValue and complete step/status/error snapshots match native serialization; type tests cover invalid JSON types. | PASS |
| Per-language exact relationship proof, AC-029 | Native/v2 output proof passes; full Workflow→Agent and Agent→Prompt reference/relationship assertions remain incomplete. | FAIL — FIND-TASK-002-7 |
| Consistent malformed Workflow selector error | Candidate emits Data, generic specification or registry codes instead of the user-selected owning Workflow error. | FAIL — FIND-TASK-002-11 |
| Explicit supplemental non-Workflow Python selectors | Shared selector parsers produce DataCard schema metadata for generic identity fields. Older behavior included at user request, not attributed to this diff. | FAIL — FIND-TASK-002-12 |
| Async blocking boundary | Shared async from_path directly performs synchronous filesystem work. No measured starvation or server outage is claimed. | FAIL — FIND-TASK-002-13 |
| Existing cohesive owners, declarations and documentation | Hydrator/EffectiveSpecs/RegistrationWriter, loading docs and module import repairs satisfy prior obligations. | PASS |
| Foundational contracts, no principal/privilege transfer, Service Bundle regression boundary | Pure contracts and declaration-only Workflow retained; WyrdState remains Service-rooted and Bundle hydration keeps its inventory/publication path. | PASS |
| Required green verification and cumulative hygiene | Candidate-bound fmt/skills checks pass; hakari and explicit base-to-candidate whitespace checks fail. | FAIL — FIND-TASK-002-14/-15 |
| Later gateway composition, server hosting, CLI/all-route proof | TASK-003/004/005 remain excluded; no new runtime, transport or public registry API is required here. | PASS — excluded |

## Validated ledger and prior closure

| Stable finding | Status | Required correction |
|---|---|---|
| FIND-TASK-002-7 | REVISED, partially open | Add exact structured references and matching relationship targets for all three Agent/Prompt pairs in each existing journey. Native execution is closed. |
| FIND-TASK-002-11 | REVISED, new | Use the user-selected WorkflowInvalidCardRef contract at shared and foreign selector owners; preserve other loading errors. Later implementation is unreviewed. |
| FIND-TASK-002-12 | CONFIRMED, new supplemental | Correct generic Python registry identity parsing to existing request Validation; preserve genuine Data body errors. |
| FIND-TASK-002-13 | CONFIRMED, new | Isolate existing synchronous bundle loading/canonicalization with the installed Tokio blocking pool at the shared facade. |
| FIND-TASK-002-14 | CONFIRMED, new | Regenerate the stale workspace-hack feature union using sanctioned hakari commands and pass its check. |
| FIND-TASK-002-15 | CONFIRMED, new | Remove only the excess EOF blank in the prior maintainer report and pass the explicit cumulative diff check. |

Prior findings 1, 2, 4, 5, 6, 8, 9 and 10 are source-closed; 3 is superseded/closed under Revision 12; 7 remains only for the narrowed mandatory relationship proof. Rejected proposal portions are omitted: blanket generic Workflow error replacement contrary to the user-selected owner, global Data helper changes, invented Python registry APIs, speculative runtime/availability redesign and later-task scope. Detailed source evidence, producer/caller traces and closure proof are in the independent ledger.

## Verification limits and handoff

[Verification](verification.md) records independent candidate-bound fmt:check and check:skills-sync passes; check:workspace-hack fails with stale OpenTelemetry `spec_unstable_logs_enabled` features; explicit cumulative diff check fails for one new EOF blank. No baseline authorship investigation was performed. These known red checks block completion under AGENTS §12 even if outside the task's reported lane list.

Committed R2 evidence reports green focused journeys/server/SQL commands and broad task lanes. Reviewers inspected actual assertions, but this review did not independently rerun runtime tests or claim newer live results as candidate proof. Source integrity passed. No production chaos/live-provider qualification is claimed.

Route [TASK-002-R3-close-remaining-review-gaps.md](TASK-002-R3-close-remaining-review-gaps.md) directly to `$wyrd-implement`. Work forward from the current branch, retaining and reassessing existing newer work; do not reset to the reviewed candidate or duplicate its already-written follow-up. The next review must select a new immutable cumulative candidate and reassess all retained findings against the original base. This task remains **FIX_REQUIRED** at the reviewed candidate.
