# TASK-001 acceptance verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt`.
- Original cumulative base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`, parent
  of the first implementation commit `c90f4c7d4`.
- Candidate: `02c06e21bd5fd613341935a843fabe5350ea0a9d` on `change/agent-prompt`.
- Approved [spec revision 2](../../spec.md) and original
  [TASK-001](../../tasks/TASK-001-agent-prompt.md).
- Reviewed all seven implementation commits and all 37 changed paths, not only
  the final evidence commit. The spec is unchanged across this implementation
  range; the task adds its recorded evidence without redefining acceptance.
- HEAD remained the candidate, and tracked source remained clean throughout
  review. Only the new review packet was written. No source was repaired,
  committed, merged, pushed, deployed or marked complete.

## Independent review results and completeness

| Fresh role | Preserved report | Result |
|---|---|---|
| behavior-rev | [Behavior](task-review-behavior.md) | FAIL; public Python result union export |
| invariant-rev | [Invariants](task-review-invariants.md) | FAIL; exact-pin reapply and public export |
| repo-rev | [Repository standards](standards-review.md) | FAIL; explicit Rust documentation gate |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL; public export and documentation |
| system-rev | [System resilience](system-review.md) | PASS within inspected failure/recovery scope |
| domain-rev: registry data | [Data integrity](domain-review-registry-data.md) | FAIL; lifecycle lock and caller-local composite selection |
| domain-rev: security/tenancy | [Security and tenancy](domain-review-security-tenancy.md) | PASS within inspected security/tenant scope |
| followup-rev | [Focused registration follow-up](followup-review.md) | RESOLVED; corrected seed example and traced three paths |
| ponytail-rev | [Independent validation](findings-validation.md) | Five retained findings, all proposals dispositioned |

All required roles were separate fresh subagents and expanded the shared
[navigation map](subject.md). All required reports are present with governing
source/authority coverage, evidence limits and results. Discovery implementation
reviewers each supplied an obligation matrix; standards supplied authority/rule
coverage; maintainer/system/domain reports supplied their required scoped
coverage. The final validator independently reconciled claims against source.
The orchestrator routes only independently confirmed or revised findings.

## Follow-up decision

[Discovery comparison](discovery-comparison.md) grouped claims by violated
boundary. A fresh follow-up was required because broad positive registry reuse
assessments did not establish the narrower reapply, same-transaction selection
and delete-versus-reference paths raised by independent reviewers. It resolved
all three. The original authored Prompt@1.0.0 empty-line collision example was
incorrect and rejected; the supported seed is 0.1.0. No extra pass was run merely
to seek agreement. Final validation independently retained the corrected case.

## Validated finding ledger

| Finding | Status / discovery sources | Required correction and observable consequence |
|---|---|---|
| FIND-TASK-001-1 | CONFIRMED; DATA-001 + follow-up | At `service.rs:1130-1134`, reused generated Prompt lacks existing exact Active/FOR SHARE protection. A concurrent deletion can leave an active Agent targeting a deleted Prompt. Reuse existing write-time dependency protection while preserving current-transaction Pending children. |
| FIND-TASK-001-2 | CONFIRMED; INV-REV-001 + follow-up | At `service.rs:1130-1140`, unchanged old exact Agent reapply selects a new child after its line advances, falsely reporting spec drift. Validate against the existing exact parent's pinned generated Prompt and preserve its reference, while retaining real conflict detection and normal selection for fresh versions. |
| FIND-TASK-001-3 | REVISED; DATA-002 + corrected follow-up | At `service.rs:1130-1133`, the Active-only resolver misses this operation's already selected Pending child. A valid composite can allocate the same version twice and roll back with registry unavailable. Include caller-local selections in existing identity/hash/version authority; never broaden global Pending visibility. |
| FIND-TASK-001-4 | CONFIRMED; BEH-001, INV-REV-002, MNT-001 | Public `state/__init__.pyi:79` advertises HydratedCard without runtime export. Ordinary public import was reproduced to fail. Define once in ordinary typed Python and re-export via runtime package, __all__ and authored stub; preserve exact union/narrowing. |
| FIND-TASK-001-5 | CONFIRMED; STD-001, MNT-002 | Changed `PyAgentCard.prompt` field and six new HTTP test items lack mandatory lifecycle/panic rustdoc. Complete those docs in the same remediation without changing assertions or behavior. |

Exact source locations, complete producer/consumer and sibling traces,
source-ID dispositions, minimal safe correction boundaries and closure proof
are in [findings-validation.md](findings-validation.md). Those recommendations
preserve normal server lifecycle/version/security semantics and require no new
material specification decision. Rejected claims are not remediation requests.

## Reconciled acceptance matrix

The matrix below is the independent validator's reconciliation, not a vote
among discovery reports. PASS rows mean bounded source/proof acceptance with
the execution limits below; they do not imply the complete task passes.

| Obligation | Source/proof assessment | Result |
|---|---|---|
| REQ-001 Agent-owned synchronous PromptCard/ref; invocation relationship | Shared HydratedAgent, Python holder attachment, TS thin projection; R/P/T/I source and recorded green proof | PASS |
| REQ-002 all authoring forms, server materialization, same transaction/lifecycle/outcomes | Ordinary paths implemented, generated outcomes included in replay/publication; valid multi-selection composite fails (FIND-3) | FAIL |
| REQ-003 derived name/space/tenant, independent line, immutable old relationships, no cross-Agent sharing | Naming/tenant/version isolation and old-row reads pass; fresh exact-pin reapply and caller-local unchanged reuse fail (FIND-2/3) | FAIL |
| REQ-004 strict typed/generic alias coherence, explicit Prompt alias, no generated Service component | Existing alias index reused; native and language dispatch/narrowing covered | PASS |
| REQ-005 local reads, stable errors and malformed-state refusals | Accessors have no IO; invalid-bundle/unknown/kind cases covered; existing authorization/audit/tenancy retained | PASS for these sub-obligations |
| REQ-005 preserve integrity/conflict/idempotent registration | Delete/reference race and false reapply conflict (FIND-1/2) | FAIL |
| REQ-006 public declarations/exports and holder capability closure | Other projections/callers match; public Python union runtime missing (FIND-4) | FAIL |
| INV-001/002 server-owned durable logic, pure spec boundary | No client durable behavior, hidden writes or PyO3 in spec | PASS |
| INV-003 reuse existing owners | Existing owners reused structurally; incomplete state/lock integration requires extension, not replacement | PASS for architectural boundary; behavior defects above remain |
| INV-004 excluded inline kinds/Workflow/migrations/features/cascade/UI/Bifrost/external repos | Complete changed-path diff remains scoped | PASS |
| AC-001/002 HTTP/SDK registration/identity/edge proof | Ordinary tests and reported green results credible; missing focused regression cases expose FIND-1/2/3 | FAIL for uncovered required behavior |
| AC-003/004 navigation, offline and negative proof | Rust actual shutdown; Python/TS retained-state accessors plus unreachable URL tests; malformed local fixtures and invocation regression | PASS at bounded task scope, with execution limits |
| AC-005 checks/typing/public import closure | Recorded relevant green checks do not prove runtime alias; independent import fails; hard Rustdoc gate fails | FAIL (FIND-4/5) |

## Verification limits and prior-finding closure

- First review: no prior FIND IDs or remediation closure to reassess.
- Original task records GREEN A/B/R/P/T/I and relevant owner/format/lint/typing,
  generated/boundary/docs results. Reviewers inspected actual proof source and
  connected production paths. Raw original command transcripts were not supplied;
  these results were not independently rerun or treated as proof of uncovered
  paths. Recorded GREEN-only TDD history remains a process limitation, not an
  invented historical RED or a separate behavior finding.
- Independent final validation executed the source-immutable public Python import
  using the existing environment with `uv run --no-sync`: HydratedCard was absent
  and the ordinary import raised ImportError. Its exact command/result is retained
  in the validation report.
- No Cargo build, broad suite, live Postgres race reproduction, source mutation,
  historical checkout or baseline-blame experiment was performed during review.
  The three registry defects are supported by full source and transaction/lock
  traces; remediation must supply focused runtime closure proof.
- Rust's navigation proof actually shuts its server down. Python/TypeScript
  tests switch ambient URLs while their server fixtures remain live, so those
  are not described as physical shutdown proof. Their inspected getters retain
  local values and have no hidden IO; no separate finding was retained.
- Full affected journeys and cross-boundary integration remain the later
  integrated change-review obligation. They are not replaced by this audit and
  are not imposed as broad repeated remediation gates.

## Remediation handoff

One self-contained task packages all five validated findings:
[TASK-001-R1 — registration and public type closure](TASK-001-R1-registration-and-public-type-closure.md).

Route directly to `$wyrd-implement`. It includes diagnosis, source-owned minimal
corrections, preserved behavior/non-goals, focused regression selectors and
write-set verification. Documentation closure is batched with behavior fixes.
No spec revision or additional design plan is required. Task completion remains
blocked until all five are closed and the full cumulative candidate passes a
fresh task review.


## Subsequent owner-directed remediation scope

After this immutable-candidate review, the owner explicitly requested removing
FIND-TASK-001-1's deletion-race correction in favor of a simpler solution.
FIND-TASK-001-1 is now an **owner-accepted risk** for the remediation handoff;
it is not disproved or independently marked fixed. Its extra dependency lock/
recheck and coordinated concurrency test are no longer required. Existing
protections must remain intact. The known residual risk is an Agent referencing
a concurrently deleted reused Prompt and subsequently failing hydration.

This instruction supersedes the earlier handoff statement that all five findings
must be fixed. FIX_REQUIRED remains for FIND-TASK-001-2, -3, -4 and -5, packaged
in the same [single remediation task](TASK-001-R1-registration-and-public-type-closure.md).
A later review must retain this explicit risk acceptance instead of reintroducing
the waived correction. Original discovery reports and independent validation
remain unchanged; this addendum records human direction, not a revised finding.
