# Immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `63ee31a0205c9b59a11877d5cf571d7bd0632cd6` (`214cb9870^`).
- Candidate: `dfb3599d5fbfb3d863ecb68382257cb30abfd79b`; HEAD at entry.
- Actual diff: `git diff 63ee31a02...dfb3599d5`; 35 commits, 154 files. Exclude unrelated `05fc05138` and its verification-workflow-ui planning documents.
- Authority: approved authorization-model spec revision 11; original `tasks/TASK-003-carryover.md`, cycles 12–16 only. This is bounded new-scope acceptance, not acceptance of the entire parent. Cycles 17–18 deferred; 19–20 transferred to TASK-004.
- Entry worktree: two unrelated verification-workflow-ui task documents modified; existing untracked prior R2 review directory. Leave untouched.

## Navigation and assignments

CodeGraph explored before source navigation. Contacts/discovery/reset: SocialSignIn → contacts/discovery/password owners → tenant contacts/platform projection/shared admission SQL → ApplicationMail → HTTP/UI; assertions in platform_admin_e2e contacts/resets and SQL pg_auth_contacts. Platform console: PlatformLogin/PlatformSessions → platform SQL → PlatformCaller/console/management endpoints → Node platform-sessions/console routes; assertions in platform_admin_e2e console and UI journeys. Onboarding: browser/social wire contracts → onboarding helpers/Svelte routes → production BFF/server journeys. Deployment: nginx → Node hooks/upstream/Rust router; startup scripts and official-host assertions.

Fresh discovery: behavior_rev owns the requirement/scenario matrix, UI/mock/contracts, TDD and unassigned repository rules; domain_rev owns recovery/discovery/reset security, tenancy and persistence; invariant_rev owns platform session lifecycle/isolation/destructive reauthentication; system_rev owns official-host routing/deployment failure and proof. Separate ponytail_rev validates coverage and every proposed finding. Root orchestrates and executes checks only. No additional repo/maintainer reviewer: those checks remain with primary and boundary specialists. Reports do not share discovery conclusions.

## Evidence to verify

User reports platform_admin_e2e 97/97, console journeys 6/6, onboarding journeys 2/2, ui:check/clippy/codegen/tenancy clean. User identifies startup OOM/nginx unexecuted, missing relay-password documentation line/local docs-build failure, absent pre-proof-expiry test, no real-widget Turnstile E2E, and production preceding tests for cycle12 scenarios5–7 and cycle16 partC scenario3. Actual assertions and task records must support claims; none is an automatic finding or acceptance. Prior reviews accept earlier bounded work only; preserve IDs1–11 and earlier TDD history.

Shared memory searched for authorization-model review context; results supply existing preferences and general workflow notes, not acceptance evidence. Applicable AGENTS, workflow, maintainer style, TESTING and narrow architecture owners govern. Source remains immutable; only new review reports may be written.
