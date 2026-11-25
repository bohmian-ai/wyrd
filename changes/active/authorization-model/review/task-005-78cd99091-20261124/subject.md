# TASK-005 review subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`.
- Candidate: `78cd99091c81e5a7d706ac6f54a8222e62705e0a`.
- Base is the merge base; inspect `git diff f7e0d67b2...78cd99091` directly.
- Branch supplied by maintainer: `task/authorization-model/TASK-002`.
- Authority: approved revision 12 of `changes/active/authorization-model/spec.md`, REQ-021 and AC-016; original `tasks/TASK-005-admin-console.md`, finalized `design/admin-console.md`, approved mock and twelve recorded maintainer rulings.
- Initial HEAD equals candidate; initial tracked and untracked working tree clean.
- `.codegraph/` absent; ordinary repository navigation applies.

## Independent assignments and navigation

| Reviewer | Boundary | Report |
|---|---|---|
| behavior-rev | Complete requirement/scenario coverage, Settings and platform workflows, actual assertions and evidence, TDD | `task-review-behavior.md` |
| domain-rev | Provider claims and stored identity; SQL directory/Role projections and tenancy; server and all SDK/MCP/CLI projections; hand-merged platform helper/provider seams | `task-review-domain.md` |
| ui-rev | Approved mock differences, shell and section list, provider marks, responsive/light/dark/accessibility presentation and capture evidence | `task-review-ui.md` |
| repo-rev | Merge provenance, task/dependency boundary, hot-path wait applicability, generated and mandatory verification gates, adopted dev environment and paused-clock fix | `task-review-repo.md` |
| ponytail-rev | Fresh independent requirement coverage and finding validation after discovery reports | `findings-validation.md` |

Principal producers are `wyrd-auth` social/callback owners and `wyrd-auth-oidc` claims; storage owners are `wyrd-sql/src/queries/auth/{users,principal_directory,roles}.rs`; contracts are `wyrd-spec/src/auth/{tenant_principals,access}.rs`; handlers are server `components/principals`. Consumers include `wyrd-client::Principals`, Python/TypeScript projections, MCP, CLI and UI BFF loaders. Production UI owners are `settings/(admin)`, `settings/account`, `platform/ui`, shared `lib/Shell.svelte` and associated components. Tests include server `pg_principal_roles`, `pg_openapi_contract`, `platform_admin_e2e` and its child modules, SQL `pg_admin_principals`, language principal-role journeys, MCP principals/discovery, `ui-pages.integration.test.ts`, and production-auth integration tests.

The cumulative range includes merge `8cc9b03d3` (PR 244). Review includes its effects on TASK-005 and explicitly adopted dev/test fixes; it does not replace the independent review of unrelated Bifrost/benchmark changes. The maintainer supplies post-merge lane results in the request and original task. Reviewers must distinguish recorded execution, inspected assertions and any newly executed checks. TASK-004 paused live-provider smoke remains excluded by the original task.

Review writes reports/remediation only. No source edits, implementation, merge, commit, push, deployment, or benchmark execution are authorized by this review. Cargo-backed runs require coordination to preserve sequential use of the shared target directory.
