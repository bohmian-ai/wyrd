# TASK-001 remediation review verdict

**FIX_REQUIRED** — all seven original findings are closed; two explicitly
human-directed error corrections remain.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/wyrd-verifier-upload-artifact`.
- Original base: `1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44`.
- Prior candidate: `da7622b52994946e03b9ba241032b99d096063cf`.
- Candidate: `0d30e262aafa07766ca73cd0613c71c861b9ddd0`.
- Reviewed remediation range: `git diff da7622b52 0d30e262a`.
- Cumulative context: `git diff 1c52b78bb...0d30e262a`.
- [Approved specification, revision 3](../../spec.md), [original task](../../tasks/TASK-001-reusable-artifact-workspaces.md), [prior verdict](../task-001-r1/verdict.md), [prior validated ledger](../task-001-r1/findings-validation.md), [remediation and evidence](../task-001-r1/TASK-001-R1-complete-artifact-workspaces.md).

Ancestry is continuous; no patch correspondence adjustment is needed. Prior
review reports committed in the range are acceptance context. The candidate
remained fixed throughout review. The pre-existing dirty service-identity
TASK-007 is excluded and untouched. Only new reports were written.

## Scope, selection and expansion

This is focused remediation review: prior-finding closure, the actual fix diff,
affected callers/tests/gates and carried-forward acceptance. Unchanged
references, selected inventory integrity, tenancy/RBAC, citations, admission,
cleanup, shared parser and declaration lifecycle retain their source evidence
from the first review. No comprehensive rediscovery was performed.

During review the user explicitly instructed: **“I want the review to address
the 1st 2 risks”**, meaning indistinguishable upload refusals and semantic tool
refusals reported as 500 execution errors. That instruction reopens only those
producer/projection paths. R1 expressly excluded new error variants or schema
work solely for stronger tests. The current direction expands the error review;
it does not retroactively make R1's exact-code assertion correction incomplete.

| Fresh independent reviewer | Selection and coverage | Report/result |
|---|---|---|
| Focused behavior reviewer | Two new human-directed error paths; producer-to-consumer/sibling tracing, existing catalog alternatives and minimal correction | [error-boundary-review.md](error-boundary-review.md), FAIL |
| ponytail-rev | All seven prior closures, affected remediation paths/gates, carried evidence, independent source/materiality/correction validation of both error proposals | [findings-validation.md](findings-validation.md), seven CLOSED; two human-directed items CONFIRMED |

The [scope map](scope.md) was supplied to both reviewers. No broad specialist
team was recreated: the remediation does not materially alter security, durable
state, concurrency or process owners. No follow-up was required: the validator
resolved the model-envelope uncertainty directly from the open JSON contract
and provider projections. No new independent blocker was discovered.

## Prior-finding closure and reconciled acceptance

| Finding | Final closure and evidence |
|---|---|
| FIND-TASK-001-1 | CLOSED: UTF-8-safe snippet length no longer depends on regex span; long multibyte regression retains a located hit within result bounds. |
| FIND-TASK-001-2 | CLOSED: Python forwards full Metadata/ArtifactSpec, matches its stub/callers, and maps invalid inputs to field-bearing Validation before connection; persisted values/report conflicts are covered. |
| FIND-TASK-001-3 | CLOSED: an Agent invokes real code.run over mounted opaque bytes; the search journey completes real search/read/query turns and asserts responses/order. |
| FIND-TASK-001-4 | CLOSED: existing structured refusals assert exact present codes and retain nondisclosure/no-Card/before-model proof. The later error-semantic expansion is separate. |
| FIND-TASK-001-5 | CLOSED: checked-in Card fixtures, fixed identities and existing declaration/pull setup replace the generator/random Card names; fresh uploaded markers remain data. |
| FIND-TASK-001-6 | CLOSED: the error payload, helper panic invariant and actual non-atomic upload cancellation/retry behavior are documented. |
| FIND-TASK-001-7 | CLOSED: public recipes cover upload, optional extraction, version behavior and authoritative inventory; final SDK signatures and generated machine copies agree. |

Exact source/proof references for each closure are in the validator's closure
table. Those references, rather than implementation summaries alone, support
acceptance.

| Obligation group | Reconciled result |
|---|---|
| REQ-001–003; AC-001/007: publication, lifecycle, typed authoring and inventory | PASS for original scope; shared lifecycle proof carries forward, Python/docs omissions closed. Upload repair semantics remain open under HUMAN-ERROR-1. |
| REQ-004–005; INV-001/003; AC-002: exact identity, selections and verified bounded bytes | PASS, carrying forward invariant/security source tables and existing owner tests; unchanged production owners. |
| REQ-006–008; AC-003–006/010: Service-bound real tools, opaque sandbox, text and citations | PASS for original scope, with missing real observations now present. Tool repair semantics remain open under HUMAN-ERROR-2. |
| REQ-009–012; AC-008–009: shared preparation/parser and explicit extraction | PASS for original scope; no parser/lifecycle replacement. Both new error requests affect refusals, not successful publication/extraction behavior. |
| REQ-013; AC-011: bounded search and repeated mixed-tool loop | PASS for original scope; regex projection and combined proof closed. Semantic search refusals are included in HUMAN-ERROR-2. |
| INV-002/004, failure/recovery and non-goals | PASS within carried execution limits: ordinary caller grants/audit, existing owners, no new endpoint/cache/parser/shell/result schema. |
| Mandatory fixtures, rustdoc, exact error assertions and public docs | PASS for the submitted remediation. |

## Validated remaining ledger

These are explicitly human-directed additions, kept separate from independently
discovered FIND IDs. They do not rename, reopen or extend FIND-TASK-001-1..7.

| ID | Status/classification | Source-proven consequence and correction |
|---|---|---|
| HUMAN-ERROR-1 | CONFIRMED / INCORRECT | Upload/nonregular traversal producers discard refusal categories into invalid-envelope diagnostics with null context. Expected source/document/collision repairs can only be distinguished through prose, and the envelope remediation is inapplicable. Reuse existing Validation with structured rules at the source and preserve it through upload; retain ordinary declaration/IO conventions. |
| HUMAN-ERROR-2 | CONFIRMED / INCORRECT | Workspace semantic refusals become Terminal strings, then generic tool invocation 500s; model and record consumers lose repair meaning. Classify tool-facing validation at its producer, reuse structured projection and forward existing agent-safe details in the open model failure envelope; preserve real execution failures and Busy. |

Full exact locations, reachability, violated authority, materiality, sibling
consumers and correction burden are in [findings-validation.md](findings-validation.md).
Both corrections reuse existing catalog types and metadata. The model's
ConversationTurn tool content is unrestricted JSON and all provider projections
forward it; no closed error envelope prevents safe-details projection. No new
catalog variant, result schema or expensive public/security/ownership decision
is required, so SPEC_REVISION_REQUIRED is unwarranted.

## Verification, convergence and handoff

The implementation task reports eight focused Python journeys, the exact search
unit test, workspace tests, formatting/lints, Python type checks and docs checks
passing. Review inspected their source/evidence but did not rerun them. The
orchestrator checked ancestry, candidate identity and `git diff --check` for the
committed remediation range. No platform, load or crash experiment was run.
Historical GREEN-first limits, and prior audit/cancellation/lowered-limit
execution limits, remain disclosed; they create no invented retrospective proof.

This is the first remediation review: every original issue closes. Remaining
work comes from explicit human scope expansion, not incomplete fixes,
fix-induced regressions or failed convergence. The next review should check
HUMAN-ERROR-1/2 closure and affected error consumers only, carrying forward these
seven closures unless changed dependencies or new material evidence invalidate
them. Do not restart discovery or require broad journey aggregates.

Route [TASK-001-R2](TASK-001-R2-preserve-repairable-errors.md) directly to
`$wyrd-implement`. Review does not implement, commit, merge, push or deploy.
