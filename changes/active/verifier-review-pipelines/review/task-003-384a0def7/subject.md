# TASK-003 immutable acceptance review

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
First independent task review; no prior TASK-003 verdict or carried-forward acceptance.
Base (excluded): `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`.
Candidate: `384a0def73ed6eac4041dc1e428dc30e981514fe`.
Range: `git diff e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355...384a0def73ed6eac4041dc1e428dc30e981514fe`.
39 consecutive task commits; user independently confirmed range and no interleaved tasks.
Revision-22 subset `1f92a5800..384a0def7` does not replace full first-review scope.
Scribe shared-owner correction `ceea0a094` is in scope.

## Authority and immutability

Approved baseline revision 18 plus approved amendments, including revision 22;
overall spec remains draft only for unrelated TASK-007 decisions. The user
explicitly approved revision-22 corrections in this session. Task/spec packet
is uncommitted; its exact authority bytes are pinned in `authority-sha256.txt`.
Tracked implementation is clean at candidate; pre-existing dirty skill edits
and untracked software-factory packet are outside implementation scope and must
not be changed. The currently supplied task-review skill governs this review.
Check HEAD, tracked source diff and authority hashes before verdict.
Review reports alone may be added in this directory. No implementation edits,
source/diff archives, commits, merges or test weakening.

## Starting navigation map

This is navigation, not coverage or a conclusion. Every reviewer expands against
source/callers and the complete immutable range. Use CodeGraph before code
location/exploration because `.codegraph/` exists; all commands through `mise`.

| Outcome/boundary | Existing owner and changed paths | Consumers/proof to trace |
|---|---|---|
| Authored controls/references | wyrd-spec card/verifier/{mod,reference,validate}.rs; refs; verification/{state,execution}.rs | declaration/planning, Scope, generated TS card-types, validation tests |
| Readiness/fanout/repeat/speculation | vala-verify blocks/{engine,scope,agent}.rs, walk.rs, blocks.rs | hosted execution, actions/checks, engine/tests.rs, Rust SDK verifier_orchestration.rs |
| Recovery/cumulative budgets | engine checkpoints and controls; server verification/{state,runner,leases}.rs | original deadlines/attempts, repeat/speculative takeover journeys, real Postgres tests |
| Admission/cancellation | Hosted workspace, engine permits, lease renewal, SQL cancel notification migration/queries | shared process/per-execution ceilings, tenant claim rotation, cancellation/capacity release |
| Scribe failure propagation | scribe/{memtable,shards}.rs; server outbox and terminal delivery | retained identity rejection, grouped writes, WAL closeout, deterministic two-writer race and same-lane bystander |
| Read authority/evidence | server service/inspection; Scope collections/publications; hosted workspace | verifier:read, current grants, strict names, excluded content, no cross-tenant data |
| Public contracts/errors | shared client verify/execution; wyrd-spec error catalog; Python/TS projection/docs/generated types | cancellation wait, unknown codes/timeouts; Rust/Python/TS/MCP journeys, HTTP/CLI/OpenAPI |

## Specialist applicability and assignment

| Contract/invariant | Role | Source-backed reason |
|---|---|---|
| All task obligations/non-goals, complete caller-to-result workflows | behavior-rev | First review of new authored controls and orchestration; no prior accepted matrix |
| Accepted controls/identities/dispositions, iteration state and durable recovery | invariant-rev | engine.rs and server state/runner changes materially extend shared lifecycle and checkpoint state |
| Cross-tier/language rules, generated contracts, rustdoc, test integrity | repo-rev | 93 changed files across Rust, SQL migration, Python, TypeScript, MCP, docs and fixtures; no prior coverage |
| Existing walker/engine/Scope ownership and reuse, maintainability | maintainer-rev | substantial scheduling/ownership changes to engine.rs, walk.rs and scope.rs require whole-workflow reuse audit |
| Shared-process failures, Scribe group isolation, cancel/delivery recovery | system-rev | shared Bifrost correction and immediate cancellation affect unrelated tenants/executions and retry/delivery availability |
| Tenant/read/tool authority and exclusion visibility | domain-security | controls introduce new evidence selection and retained-content projections; unchanged authorization dependencies must be traced |
| Persistent control format and accepted checkpoint/WAL integrity | domain-persistence | Controls and format compatibility materially change persisted recovery semantics; separate domain review validates exact accepted-state and retained conflict boundaries |
| Admission/permit/retry concurrency | domain-concurrency | nested fanout/repeat/speculation and Busy drain change resource ownership across active tasks and execution bounds |

Specialist selection was expanded before validation to add domain-persistence:
persisted control records and format compatibility are a material data-integrity
boundary even without a new store. Invariant/system evidence remains independent
corroboration. No separate database performance review is needed: no new
checkpoint store or backlog scan is introduced in this range. Security domain must independently
check actual tenant capability boundaries. No UI changes: UI specialist omitted.
No LLM provider/tool protocol changes: provider-specific specialist omitted;
behavior, security and maintainer reviewers cover existing Agent seam consumers.
Validator must independently assess every selection/omission and obtain missing
specialist evidence if any is required.

## Verification limits and commands

Read task Verification and Evidence / Implementation Evidence for available
recorded runs, exact selectors, RED exceptions and owner-check results. These
are evidence claims to test against source, not an intended verdict. No reviewer
receives the implementation completion summary. Runtime proof is not required
to rediscover already source-proven defects. Do not run Cargo concurrently: only
the orchestrator runs focused tests; reviewers request any needed command.
Comprehensive integration/platform/credentialed suites are not rerun by default.

## Report assignments

- behavior-rev: `task-review-behavior.md`
- invariant-rev: `task-review-invariants.md`
- repo-rev: `standards-review.md`
- maintainer-rev: `maintainer-review.md`
- system-rev: `system-review.md`
- domain-security: `domain-review-security.md`
- domain-concurrency: `domain-review-concurrency.md`
- domain-persistence: `domain-review-persistence.md`
- independent validator (after discovery): `findings-validation.md`
