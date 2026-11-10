# TASK-004 unfinished implementation and task-scope review

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Base: `dfd19aa129830696d68cf8d4a09c144805847345` (restored pre-GitHub source).
Committed candidate: `1beb1aa1980cb03495dfb0778a353604670ba935`.
User-authorized WIP candidate tree: `226fdafc56bacc707b9167d1887be9b72f5e6d53`.
External library: clean pinned `e52c9fee50c94063fc32b8cd9b9d1234e107ae20`, descendant of generic `9fdf6dc`.

The user explicitly requests review of an incomplete working tree. This overrides
the skill's committed-only subject mechanics: a temporary index produced the
immutable tree above without committing, updating a ref, or changing the user's
index/source. It includes HEAD, 14 modified/new source paths and 19 current
packet authority documents. Pre-existing skill edits and unrelated software-factory
work are excluded. Read `git diff <base> <tree>` and `git show <tree>:<path>`;
`git diff <HEAD> <tree>` isolates WIP. Do not demand completion before reviewing.
No acceptance is carried forward from the rejected per-tenant-App implementation
or its abandoned audit. This is the first review of the replacement task.

## User direction and authority

During review the user explicitly rejected **all** new GitHub tables and
required the existing queue table. This overrides task-prescribed new storage.
Do not assume that connection/setup/repository metadata is exempt, or hide
permanent configuration inside disposable queue rows. Independently establish
the existing owner or the concrete gap; inability to justify the current work
can support a start-over recommendation, not invented replacement machinery.
Treat this as a human-directed constraint, not a retroactive FIND ID.

The user subsequently required explicit agreement **before any remediation**.
This turn remains read-only evidence and recommendations. Do not create a
remediation task, revise implementation tasks, edit source or reset/revert Git.
Even a validated FIX_REQUIRED verdict does not authorize remediation authoring.
The captured tree remains the immutable subject; all keep/rework/discard
recommendations require source-backed independent validation and are not
acceptance or instructions to proceed.

Audit implementation and task scope together. The user rejects checklist-driven
overkill and requests a remediation task containing code corrections plus a
recommendation for a smaller task centered on actual team journeys. Do not remove
security/tenant/durable guarantees; distinguish onboarding from the larger
repository-verification capability bundled into TASK-004. Do not manufacture
44 separate defects from the 44-row implementation ledger. Group unfinished
required paths by user outcome and defects by root cause. Recommendations that
redistribute approved work must preserve its later owner/proof; product changes
need explicit spec revision. Do not implement or edit the original task/spec.

Read `.agents/skills/wyrd-task-review/SKILL.md`, root/nested AGENTS, workflow,
maintainer style and applicable architecture/testing guides. Approved context:
`spec.md` revision24's scoped GitHub amendment and existing approved baseline,
`public-contracts.md`, original `tasks/TASK-004-installable-github-app.md`, related
TASK005/006 and execution-state supplement. All are captured in the candidate
tree. Spec aggregate draft status concerns separate waits; do not broaden into
TASK007's unrelated policy. Task evidence and recorded checks are claims, not
an implementation summary to trust. No intended verdict is supplied.

## Shared starting navigation map

| Path | Owners/callers/consumers and relevant proof |
| --- | --- |
| Connection | server `components/github/{mod,routes,service}.rs`, server config/boot/auth/browser middleware, SQL github connections/setup/selection; shared client github; Rust `github_app_connection` journey and operator/team docs |
| Repository targets | spec repository/execution/state arms; VerificationControl execute/enqueue, cache/runner/results/SubmissionSink and queue subject SQL; Rust `repository_verification` journey/pg queue tests |
| Trusted loading/declaration | config pull-request schema; shared loader SourceTree/path/parse/error/declaration stamping; WIP automation and storage routes; existing full loader/canonical reference visitor and cards saga/service/completion owners; loader tests |
| Branch and durable PR work | committed github_bindings query/migration and PG test; WIP automation/worker, github_deliveries/github_checks queries/migrations; shared ClaimLoop, Operator bounded attempts, queue/lease/cancellation; runtime wiring |
| Public contract/proof | shared Rust client, HTTP routes, Python verify stub/native changes, TS generated wire/error types, SDK/server/SQL integration targets and docs; compare implemented calls to declared journeys, not just file presence |
| Generic external boundary | library GitHubApp/GitHubClient transport/OAuth/user installation/repository/Check/webhook APIs at e52c9fe; Wyrd owns configuration/signing/listeners/tenant/grants/work scheduling |

This is a starting map, not coverage or a conclusion. Expand source/callers.
Use CodeGraph first for code navigation. Current on-disk source may be read after
checking against captured blobs. Source must remain unchanged. No reviewer may
read another's discovery report. No Cargo/build/lint/codegen without orchestration;
orchestrator serializes requested checks. Write only the assigned report below.

## Specialist applicability and ownership

| Role/report | Boundary and source-backed reason for selection |
| --- | --- |
| behavior-rev / task-review-behavior.md | First-review acceptance: two real journeys, connect/manage and PR-to-Check; reconcile all ledger rows compactly, group unfinished outcomes, challenge task scope against original approved behavior |
| invariant-rev / task-review-invariants.md | Material shared lifecycle: subject extension crosses queue/runner/Bifrost; WIP worker adds claims/admission/Check association and branch reconciliation |
| repo-rev / standards-review.md | Broad tier rules: Rust/spec/client/server/SQL, Python/TS projections, documentation and fixture/test boundaries; changed-item docs and SQL capability/composite keys |
| maintainer-rev / maintainer-review.md | Major ownership changes: SourceTree/lazy acquisition/declaration staging and WIP automation; compare whole existing workflows, identify concrete excess and recommend a focused task rewrite |
| system-rev / system-review.md | New deployed worker and durability/recovery: runtime wiring, interruption, lease expiry, provider errors, store uncertainty and sibling service availability |
| domain-rev-security / domain-review-security.md | Sensitive browser/OAuth/installation-to-tenant boundary, credentials, current repository/principal authority, webhook routing and source imports |
| domain-rev-data / domain-review-data.md | Sensitive persistent/concurrent state: five new migration families, tenant parentage, active claim ownership, branch revisions and execution/check identity |

All applicable roles selected. No UI specialist: current code has no UI
implementation changes; behavior/repo cover missing browser journey/projection.
No separate ML specialist: existing ordinary Verifier engine is reused, not new
judging behavior. A fresh independent ponytail-rev validates reports, applicability,
materiality, minimal correction and scope recommendations after discovery.

Keep reports concise and bounded. Required behavior that is plainly not yet
built is a grouped missing journey, not an invitation to implement or invent more
machinery. A task rewrite recommendation is explicitly requested; it is not a
new implemented acceptance contract until routed through wyrd-plan as appropriate.
