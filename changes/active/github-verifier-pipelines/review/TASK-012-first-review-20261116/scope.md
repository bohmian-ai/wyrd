# TASK-012 review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Branch: task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Base: c24e1db8f6641cca9912376a4587e63e2fe74dd1 (ee068f5a2^).
Candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46.
First review: full git diff c24e1db8...901639300. No prior TASK-012 acceptance is carried forward. Imported reviews are historical context only.

Authorities: root and applicable nested AGENTS.md; changes/spec-driven-development.md; agent_docs/index.md and topic guides; github-verifier-pipelines/spec.md revision 15; original tasks/012-integrate-code-mode.md including Maintainer scope direction; inherited verifier-code-mode/spec.md as amended by pipeline revisions 12–15; owning architecture sections and TESTING.md.

Fixed directions: no benchmarks (TASK-013/014 own qualification on one combined server); PostgreSQL is coordination clock; server-clock gateway in-memory admission accepted; no architecture/wyrd-design.md edits. Struck proof cases are excluded. Local modified tasks/README.md and untracked tasks/011-code-review-pipeline-tutorial.md remain untouched. No source changes, commits, pushes, git config, stash, links or attribution in written artifacts. Commands use mise; Postgres tests use scripts/postgres/with-test-postgres.sh.

## Navigation and specialist applicability

All reviewers expand this starting map using CodeGraph, actual diff, callers and assertions. The map is not acceptance evidence.

| Contract/invariant | Owners, producers, consumers and proof | Role and selection reason |
|---|---|---|
| Complete port; dynamic pipeline behavior and direct/queued/publication distinctions | vala-verify blocks/{engine,agent,tools}; server hosted/{blocks,tools,evidence}; SDK verifier_building_blocks and hosted_agent_tools; port coverage and inherited packet | behavior-rev: comprehensive task acceptance, OBL-1–13, AC-014 |
| Settlement before retry/release; spend; charged custody and admission | ToolCalls, Session, Dispatch; hosted/{admission,sandbox,workspace,evidence}; verification/state; runtime/{resources,outbox}; Bifrost resources/caches | invariant-rev: complex shared lifecycle; domain-concurrency: resource and spend concurrency boundary |
| Cross-tier rules, generated parity, portable runtime | manifests, Vala/runtime/server, SDK native/testing/stubs, docs/scripts/mise and all nested rules | repo-rev: broad language/rule coverage including changed-item rustdoc and test-repair diagnoses |
| Existing owners versus new workflow/abstractions | ProcessResources/Outbox; hosted owners; ToolRegistry/Dispatch; test-server controls; diagnostic benchmark drivers | maintainer-rev: substantial ownership/abstraction change, complete-workflow reuse |
| Failure propagation, recovery, role topology, drain | boot/config/app/http/grpc; sandbox/workspace/worker; checkpoint/outbox; Scribe/Oracle/Forge | system-rev: material deployment/recovery effects |
| Per-call RBAC, tenant authority, audits, authored-code isolation, provider publication identities | hosted/{tools,workspace,evidence}; vala Dispatch/ToolSession; verification routes/service; security authority | domain-security: sensitive security and tenancy boundary |
| Persistent attempt spend, ACK ownership, state formats, migrations | verification/{state,hosted/evidence,telemetry}; scribe_outbox; runtime/outbox; SQL claim/checkpoint consumers; AC-014 migration comparison against main | domain-records: persistent integrity/durability boundary |
| Independent source/materiality/coverage/correction validation | All selected reports and affected source, acceptance rows and omitted-role rationale | fresh ponytail-rev after discovery, always required |

No additional specialist: SDK/performance tooling correctness falls to behavior, standards and maintainer; benchmark qualification is excluded. No UI contract changes. Follow-up selected only for a material unresolved conflict/path.

Verification inputs: candidate task Evidence table supplies recorded commands/results, which must be checked against assertions and source. Each reviewer records executed checks and limits. Avoid duplicated broad suites and all benchmarks. Reports only in this directory: no diff, patch or source archive.

## Shared memory reconciliation

Shared-memory lookup returned older maintainer defaults concerning hot-path waits, warm-cache SQL, benchmark qualification, and architecture. Current explicit instructions take priority: no benchmark here, struck warm-cache proof remains excluded, and architecture/wyrd-design.md is not edited. The system review inventories newly added or moved hot-path waits and tests them against sanctioned work. The architecture/source review remains an acceptance audit, not an authority rewrite.
