# TASK-001 review subject

- Repository: /home/thorrester/Documents/GitHub/wyrd
- Branch: task/authorization-model/TASK-001
- Base: eeb3bbd4f891ae88b00ce1979810eb2a11d04871
- Candidate: c78ad7b5241fbbfb54e85e2390c6a694787debfc
- Implementation: 8e33913ed^..c78ad7b52, 38 commits; inspected with git diff base...candidate.
- Worktree HEAD: 3139c08cb0b7c0194583a66af3ab31bef28dc14f
- Approved contract: SPEC-authorization-model revision 2, changes/active/authorization-model/spec.md, committed at 38ac3ca8a0c365825124aa3e99a36ba19888e26f.
- Original task and evidence: changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md.

The later 38ac3ca8a..3139c08cb authority, design and specification commits are outside the implementation review. The worktree's crates, SDKs, scripts, examples, mise.toml and documentation source match the candidate. The later approved authority is used as the contract supplied for this review. Source and commit identities must remain unchanged throughout review. This directory contains reports and remediation only.

## Navigation map

| Boundary | Owners | Consumers and proof |
| --- | --- | --- |
| Operations, scopes, grants and ceilings | wyrd-spec auth/access.rs, auth/permission.rs, auth/permission_scope.rs; wyrd-runtime permission.rs, builtin_roles.rs | PermissionCheck, Principal, request context; issuer/verifier, public access contracts |
| Current authority and lifecycle | wyrd-auth authority.rs, issuance.rs; wyrd-sql auth/authority.rs, role_bindings.rs and credential writers | Server auth ingress, production worker composition, query stream revalidation; pg_principal_roles, authority unit/PG tests, SDK principal_roles |
| Trusted retained-data parent | Server verification admission, runner, inspection, evidence and approvals; GitHub PR owners; Scribe writers | Execution/review reads, storage downloads, approval decisions; server and SDK journey assertions |
| Managed SQL visibility | vala-bifrost-redux Oracle planning/masking and codec; server query and peer forwarding | Memtable/published/distributed reads, expression safety, stream lifecycle; Oracle tests and public SDK query journeys |
| Administration and projections | Server principals/access.rs, auth local device/invitation owners, Rust Principals | HTTP, CLI, MCP, Python wrappers/exports/stubs, TS native/wrappers/types; per-surface integration tests |
| Persistence | wyrd-sql scoped authorization migration and tenant-bound queries | Account, role, membership, revision, anchor and invitation consumers; SQL integration and tenant-boundary rules |

## Review constraints

The maintainer explicitly excludes TDD compliance reporting. That instruction governs this review. Other behavioral proof and hot-path wait checks remain in scope. TASK-002 owns new UI pages; TASK-001 must independently deliver the programmatic authorization boundary.

No implementation, commits, pushes, Git configuration changes, links or attribution are authorized as part of these reports.
