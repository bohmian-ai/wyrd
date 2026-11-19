# Round 2 subject and coverage

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004

Original candidate: 4d001e6d9. Authority/remediation baseline: 16abc095e.
Immutable candidate: ec5471ca2421e9ba0049658546554f1b3c6dac8a.
Branch: task/github-verifier-pipelines/TASK-004-pinned-pr-preparation.
Review actual changes using git diff 4d001e6d9 ec5471ca2; implementation-only range 16abc095e ec5471ca2.
Original task: changes/active/github-verifier-pipelines/tasks/009-reports-and-github-delivery.md.
Approved authority: changes/active/github-verifier-pipelines/spec.md revision 11; amended owning architecture.
Prior evidence: review/009-reports-github-delivery-r1-4d001e6d9, including verdict.md, findings-validation.md, specialist reports and remediation.md.

Scope: closure of FIND-009-1, -3, -4, -5, -7, -8, -10, fix diff and affected callers only. FIND-009-2, -6, -9 are accepted limits under revision 11 and excluded. Ignore the working-tree tasks/README.md edit and untracked tasks/011-code-review-pipeline-tutorial.md. Carry forward round-1 acceptance evidence for schema, output selection, SDK authoring, access/tenancy, lookup-before-create, verdict ownership, HTTP/MCP authorization and excluded machinery unless a scoped fix invalidates it. No broad journey suites.

## Navigation and specialist applicability

| Contract/invariant | Owners, producers, consumers and source-backed selection |
| --- | --- |
| Optional extraction, retained anchor sides/context, report review existence | presentation.rs Completed, Source, Retained, sides, render; delivery.rs render supplies retained selection; extraction/output and serialized ReviewContext supply source. Fresh remediation ponytail discovery covers FIND-009-1/3/4, affected rendering tests and mandatory rustdoc including report.rs and helpers. |
| Cooldown and completed Check monotonicity | app.rs cooldown classifies provider headers; delivery.rs Write::send and retry loop consume timing and Check body. Closure discovery plus system specialist cover FIND-009-5/7 and affected provider paths. |
| Cancellation delivery identity and inspection | inspection.rs cancel commits SQL transition then spawns existing GithubDelivery; delivery.rs event/key/recorded and recovery; inputs.rs delivery and service.rs inspection consume Bifrost event. Material lifecycle change selects invariant specialist; durable event identity and sibling cancellation writers select persistent-data domain specialist. |
| Restart/retry and provider failure | GithubDelivery deliver/recover, Write::send, app classifier; existing server recovery and public cancellation handoff. Material cancellation/recovery effect selects system specialist. Deferred shutdown and post-lookup races remain excluded. |
| Standards and ownership | Narrow extensions to existing server owners, pure-contract docs and user guide, no SDK/public schema/SQL migration changes. No broad cross-tier standards specialist; closure reviewer covers changed-item rustdoc, nested docs rules and generated guide parity. No substantial owner replacement or abstraction change requiring separate maintainer specialist; closure reviewer compares existing source/context owners. |
| Security, tenancy, UI and performance | No new permission decision, credential authority, tenant key, UI or engine hot-path wait. Round-1 security/data evidence carries unchanged authorization boundaries. Separate security, UI and benchmark specialists omitted; invariant/data reviewers check changed cancellation handoff preserves those boundaries. |

All reviewers receive this map as a starting point, must verify it from immutable source, and may expand only for fix-induced regressions or specific evidence gaps. Reports contain no attribution or links. Independent validation follows discovery in a fresh context, validates applicability/coverage and all proposals, and cannot validate its own new blocker. Focused follow-up only for a material unresolved path or conflict.
