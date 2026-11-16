# TASK-002 verdict: FIX_REQUIRED

One independently confirmed public-documentation finding remains:
**FIND-TASK-002-1**. No runtime or SDK implementation correction is required.

## Immutable subject and review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
Base: `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e`.
Candidate: `ff78b51b61fea8f215a2139b17840044f65b17e4`.
First TASK-002 review of the complete `git diff base...candidate` (17 files).
Approved spec: revision 2 of `changes/active/verifier-code-mode/spec.md` at
candidate. Original task: `tasks/TASK-002-public-code-mode-workflow.md` at base;
its original obligations remain unchanged at candidate. There is no prior
TASK-002 acceptance/closure evidence or remediation round to carry forward.

The working spec revision 3, TASK-001 review directories, spec-revisions and
TASK-003 are excluded. TASK-001 runtime dependencies were traced only as
needed to establish TASK-002's public obligations, without restarting its
fault matrices. The candidate remained pinned through final review; no
tracked implementation file was modified. Reports are the only new review
artifacts. No commit, merge, push or deployment occurred.

## Independent review and validated coverage

[scope.md](scope.md) records the authority, navigation map and pre-discovery
contract/role applicability decisions. The independent validator checked all
selected reports and those decisions in
[findings-validation.md](findings-validation.md).

| Role / report | Independent result | Coverage |
| --- | --- | --- |
| behavior-rev — [task-review-behavior.md](task-review-behavior.md) | FAIL; BEH-002-1 proposed | Complete task acceptance, public workflows, docs, reuse, non-goals and credible failure paths |
| invariant-rev — [task-review-invariants.md](task-review-invariants.md) | PASS; no proposals | Shared records/defaults, typed transformations, queued and retained producer-to-consumer paths |
| repo-rev — [standards-review.md](standards-review.md) | PASS; no material proposals | All 17 changed paths, applicable nested/language/docs rules, Rust helper/test rustdoc, generated parity |
| domain-rev-security — [domain-review-security.md](domain-review-security.md) | PASS; no proposals | Accepted caller, exact tool/downstream permission, denial/recovery, scoped MCP discovery/execution and result visibility |
| fresh ponytail-rev — [findings-validation.md](findings-validation.md) | FIX_REQUIRED recommended | Independent source/materiality/correction validation, empty-ledger coverage and specialist applicability |

Separate maintainer-rev was correctly omitted: no production ownership or
abstraction change; behavior review covered local test/fixture reuse.
Separate system-rev was correctly omitted: no runtime executable, deployment
topology, worker lifecycle or failure-propagation change. No additional
persistent-data/concurrency domain review was required: no SQL, transaction,
coordination or concurrency implementation changed; invariant/security
reviewers covered the required retained result and authority boundaries.
All omissions are independently validated, not inferred from an empty ledger.

[claim-comparison.md](claim-comparison.md) groups the single proposal by cause.
The other reports' general documentation assessments do not contradict its
specific last-expression trace. No unresolved conflict or unexplored common
cause required followup-rev; a unique finding alone did not trigger another
discovery pass. The validator found no new blocker needing separate source
validation. The standards report's non-material local import placement
observation is omitted from remediation.

## Reconciled acceptance matrix

The detailed source locations and proof are preserved in the discovery
matrices and validator's reconciled acceptance table.

| Obligation | Implementation / source evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Same nested preparation/check program; REQ-001–004,014; AC-001–002,016 | Shared checked-in programs, common Card declarations, existing runtime/catalog and public Python/TS/MCP journeys | Focused journeys inspect outer model and nested code records and provider requests; supplied passing runs | PASS |
| Protected denial, safe catalog code and authorized recovery; REQ-006–008, INV-004; AC-004–005,017 | Fixed accepted caller and common tool/downstream authorization; existing sandbox error resumption | All three public surfaces exercise missing/granted query permission, zero-byte/no-digest denied call and recovery | PASS |
| Complete honest records, custom-data preservation and all public read paths; REQ-015–017, INV-001,006; AC-012–013 | Shared serde defaults/record owner, generic Python mapping, TS typedResult spread, retained producer/reader and MCP schemas | Immediate/typed/queued/status/retained assertions; type fixtures, served schema and supplied passing focused runs | PASS |
| Original supported pinned citations; REQ-018; AC-001,011,016 | Same pinned notes evidence and original /n07 citation, no promoted scratch/live input | Every public journey checks original evidence and custom hits | PASS |
| HTTP/MCP/stub/generated contract and catalog parity; AC-013 | Existing Rust-owned wire/catalog, authored Python fields and unchanged generated TS owner | Served OpenAPI exact fields/optional compatibility/recipient enum; MCP actual schema/output; supplied generation/native checks and independent typechecks | PASS |
| Public guidance on limits, authority/audit, worker occupancy, failures, citations, upgrade and savings limits | Existing reference/operate/inspect pages compared with governing source and authority | Supplied docs check plus independent source review | PASS |
| Accurate public outer-output visibility guidance; REQ-001,014 and task docs acceptance | Reference and embedded example claim print-only visibility; existing runtime returns last-expression repr too | Producer and model-delivery trace independently confirmed; current print-ending journeys cannot prove the stronger claim | FAIL — FIND-TASK-002-1 |
| Ownership, repository rules, non-goals and existing consumer preservation | Complete range adds tests/types/docs/fixtures, no runtime/client engine, new endpoint/store/dependency/platform change | Selected independent standards/security/behavior coverage and supplied gates | PASS |

## Validated finding ledger

### FIND-TASK-002-1 — CONFIRMED / INCORRECT

Discovery source: BEH-002-1. Violated obligation: TASK-002 documentation
acceptance and approved revision-2 REQ-001/REQ-014.

Changed locations:
`docs/src/content/docs/reference/agent-tools-and-authorization.md:34`,
`fixtures/cards/verifier_building_blocks/programs/nested-investigation.py:14`,
and generated `docs/public/llms-full.txt` prose/embedded copies.

The new reference says the model sees only printed text. In contrast,
`hosted/tools.rs::Invocation::code` returns bounded `value` (last-expression
repr), `value_truncated` and printed `output`; `Dispatch::call` bounds that
JSON and `RecordedTool::invoke` delivers it to the model. An authorized
program ending in an intermediate Card value sends its repr without printing.
A reader relying on the published selection rule can therefore unintentionally
include intermediate content in model context. This is a material false
statement about the core supported workflow, not an authorization bypass.

The existing runtime contract already satisfies the specification. The minimal
correction is to explain both outer output channels in the source reference,
reconcile the shared program's general comment, and regenerate machine-readable
docs through their existing owner. No runtime guard, client filter, new
abstraction or API decision is warranted. A few prose/comment edits and normal
docs generation/checks are sufficient. The complete independently validated
trace, burden assessment and focused closure proof are in
[findings-validation.md](findings-validation.md).

Remediation: [TASK-002-R1-correct-output-visibility.md](TASK-002-R1-correct-output-visibility.md),
ready for `$wyrd-implement`. Preserve the original task and this candidate for
closure review. No prior findings exist; convergence assessment is not yet
applicable. No specification revision is required.

## Verification limits

[verification.md](verification.md) distinguishes independent rechecks from
supplied execution results. Python and TypeScript typechecks (including TS
native builds and consumer checks) and cumulative diff whitespace validation
passed again. Candidate task evidence records passing focused Rust SDK,
Python, TS, MCP and served-schema tests plus format/lint/type/native/package,
codegen and docs checks; the reviewers assessed their actual assertions and
source rather than claiming those journeys were rerun.

Full journey suites and broad gates were not repeated, as the task reserves
them for integrated change review. TASK-001's full runtime/security/fault
matrices were not restarted. The remaining risk is incorrect published
output-selection guidance until FIND-TASK-002-1 is corrected.
