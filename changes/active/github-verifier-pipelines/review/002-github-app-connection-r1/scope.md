# Task 002 review scope

- Base `395274d58bf76a2075f245ca3a5c1bd044925027` (parent of the task’s first obligations commit).
- Candidate `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`.
- Compare `git diff 395274d58...3dd1555a0`. The revision 4 approval at `46af20f00` lies within the range and supplies authority. Its approved packet updates are evaluated as such, separately from implementation drift.
- First acceptance review. No prior task 002 verdict or remediation task was found in the branch history or change worktree. No acceptance rows carry forward. The candidate task file supplies the obligations and recorded verification commands.
- Authority is the approved revision 4 spec, contracts and App design supplement, with root and touched SQL, docs, Python and TypeScript contributor rules, spec-driven development, maintainer style, architecture ownership, and testing guidance. There is no CodeGraph index in this worktree.

| Boundary and navigation | Selected discovery |
|---|---|
| All task behavior, including CLI/HTTP/MCP/SDK callers via `wyrd-client/src/github.rs`, service, typed spec and guides; real journeys in each shipped surface and SQL/server tests | Behavior, comprehensive |
| `GithubConnections::{begin,setup,callback,disconnect,webhook,installation_token}` and `auth_login_state` share browser transaction and installation lifecycle with OIDC, verified webhooks and eventual queue consumers | Invariants, transaction flow and fencing |
| Pure spec, SQL, server/auth, CLI, MCP, SDKs, generated contracts, guides and test task edits span independently governed tiers | Repository standards, all touched surfaces |
| `GithubApp` and `GithubConnections` introduce provider and application owners beside `ScreenedHttp`, auth browser/login and existing identity/permission owners; client, SDK and fixture ownership also changes | Maintainer, substantial new workflow ownership and reuse |
| Boot/config, provider request failures, callbacks across replicas, webhook retry, token issuance and SQL availability change deployed failure propagation | System resilience, runtime and recovery |
| Browser cookies/session, PKCE, OAuth code, installation proof, outbound secrets and origins, webhook signature, audit and scoped reads/writes are authentication and security boundaries | Security domain |
| Dedicated installation table and altered login-state SQL, tenant capabilities, forced RLS, SECURITY DEFINER routing, uniqueness and lock/revision handoffs change persistent and concurrent tenant state | PostgreSQL domain, including tenancy and concurrency |

No specialist is assigned to Bifrost storage, model execution or UI because this task adds no such implementation. The security and behavior reviews still trace permission audit delivery and affected queue handoffs to their existing owners where required. Each reviewer starts from this map and expands coverage using source.
