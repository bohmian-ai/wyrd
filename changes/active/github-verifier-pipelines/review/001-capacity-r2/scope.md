# Task 001 remediation review (first remediation round)

Repository: /home/thorrester/Documents/GitHub/wyrd-factory-worktrees/code-review-workflow-github-verifier-pipelines
Original base: 395274d58bf76a2075f245ca3a5c1bd044925027
Prior candidate: 34fb45bb387582d2aedfee77fb4e09fa2753b80d
New candidate: ea4460c3c97adb55395acd50f062658bc8d0d4cd
Incremental review: git diff 34fb45bb3 ea4460c3c
Cumulative context: git diff 395274d58...ea4460c3c
Ancestry is linear; no rebase or correspondence ambiguity. Source starts clean and must remain immutable. Only reports in this new directory may be written; no diff/patch/archive files. Cargo checks are sequential and owned by orchestrator. Reviewers request checks, do not run Cargo or mutate source.

## Acceptance context and authority

- Original task: changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md.
- Approved spec: changes/active/github-verifier-pipelines/spec.md revision 3 (REQ-015/016, INV-001/005/007, AC-010); design/approved-runtime-and-github.md capacity/eligibility.
- Prior verdict and validated ledger: review/001-capacity-r1/verdict.md and findings-validation.md; retain all prior reports and source-backed evidence.
- Remediation: review/001-capacity-r1/001-R1-close-capacity-review.md, including both explicitly human-approved revision blocks. The current user approvals supersede the earlier preserved defaults/config restrictions. Human additions H1–H9 are not reviewer findings and receive no FIND IDs.
- Root AGENTS, docs/AGENTS for changed docs; no nested rules under touched server/testing/Rust SDK/fixtures paths. Read spec-driven-development, maintainer-style and applicable agent_docs/architecture/testing slices. CodeGraph exists: use before code discovery.

## Scope and carried-forward evidence

Review FIND-001-1 through FIND-001-5 closure; full incremental production/test/docs diff; R1–R13 and H1–H9 assertions; callers and consumers affected by fixes/default/config changes. No comprehensive first-round restart.

Carry prior validation of single-engine ownership, direct/queued exhaustive routing and reclaim selection, accepted Caller/evidence/subject authority, persisted budgets/deadline, eligibility and excluded future features from r1 findings-validation.md and invariant/concurrency/maintainer reports. These owners are unchanged; verify changed config projections and timeout cleanup do not invalidate them. Existing semaphore ownership, claim/lease/checkpoint state, Sandbox/Workspaces implementations and execution-mode producers have no incremental functional changes.

Refresh O11 config/refusal, O12 defaults (now intentionally superseded by human revisions), O15 docs, O18 call wait/cleanup where changed, O1–O10/O14 proof assertions because journey relocated, and O19 recovery proof because deterministic ordering changed. Preserve unrelated accepted rows and recorded historical RED exceptions. Do not demand new benchmark guarantees or tests that merely mirror a one-line call unless an explicit obligation/material proof gap requires them.

## Common navigation map (starting points, not a coverage conclusion)

- config.rs: VerificationConfig/VerificationLaneConfig file fields -> resolve_limits (formerly apply_limits_env) -> set_file and existing env overrides/consistency -> WyrdServerConfig::apply_env_overrides/load -> existing boot limits/runtime projections. Normal LimitsConfig/Bifrost config are nearby alternatives. New assertions in verification_capacity_file_values_resolve_under_env; existing verification_limits_env_applies_and_refuses.
- hosted/mod.rs: HostedLimits::lanes_fit checked_add; new approved defaults direct executions256, queued64, model256/256, workers4/12, waiters16/48, workspaces16/48, pool8192MiB, prewarm1 each. Existing lanes_split_the_memory_ceilings now pins exact values and overflow refusal. verification/mod.rs defaults test adds panic contract.
- gateway/caller.rs: ServerWyrdGatewayCaller::call bounds selected-lane wait and passes remaining timeout into existing GatewayInvocation. Same acquired permit lifetime and normal gateway account/cancellation owner. pg_invocation_tests.rs adds bounded-lane call tests/cancelled-wait/recovery capture assertions; existing compatible-adapter test remains relevant.
- SDK verifier_orchestration.rs: recovery test COMPARE held until extra milestones using existing HeldUpstream/reached, preserve takeover/content/publication/count assertions. Directly related disposition test is proof/synchronization precedent.
- wyrd-testing server verification_runtime::isolated_capacity child module: former SDK story relocated with real client/YAML declaration/pull/hydration and mock Gateway/CredentialWriter setup, runtime holds/log observation retained; SDK registration removed. Preserve each old assertion and real wire path. Fixture README updated; no fixture YAML change.
- Operator configure-verification-runtime and verification-runtime-limits pages + llms projections: file keys/defaults, per-process sizing/accounting and unchanged no-borrow/shared downstream contract.
- Remediation packet Evidence section: exact U/G/O/J commands, reported RED/GREEN, cited assertions, gates and exceptions. Reports are evidence, not conclusions; review source and actual assertions independently.

## Specialist selection and applicability

| Contract/invariant, owner and consumers | Selection / carry-forward reason |
|---|---|
| All five closure boundaries and R/H obligations, prior accepted behavior impacted by fixes | Fresh ponytail-rev required, after selected discovery reports. Performs direct independent source validation and reconciliation; must request separate validation for its own new blocker. |
| Newly configurable operator startup contract, file/env merge/validation, raised defaults and runtime projection | repo-rev required for server/config/test/docs cross-tier rules and rustdoc; maintainer-rev required for substantial config authoring/projection additions and relocated fixture workflow. Both review only incremental/affected scope, compare existing owners. |
| Per-call duration and expired/cancelled call cleanup, ordinary gateway settlement, startup rejection | system-rev required: material duration/failure-propagation correction; scope wait/remaining budget -> existing gateway/drop/accounting and affected configuration startup paths. |
| Shared lane ownership and sensitive concurrency | Prior r1 invariant and concurrency reports traced mode/permit producers, sibling consumers, claim/reclaim and existing drop cleanup. No new semaphore, execution routing, shared synchronization or durable state owner in this delta. Changed per-call duration/drop boundary is specifically covered by system-rev plus fresh validator. No separate invariant/concurrency discovery needed absent evidence of a new ownership/semantics issue; validator must independently confirm omission. |
| Security/RBAC/tenancy/persistence | Prior source-backed unchanged accepted-authority/gateway/queue owner proof carries. No changed permission, identity, SQL/schema, evidence or durable handoff contract. No separate domain review selected. |
| Python/TS/UI public surfaces | None changed; no SDK operation or language lifetime addition. No specialist or duplicated language journeys required. |
| Comprehensive first-round behavior discovery | Not selected: remediation default is fresh validator checking closure/affected paths, supported by targeted specialists. Existing whole-task acceptance carries per rows above, not a new full discovery pass. |

## Exact reports

repo-rev -> standards-review.md
maintainer-rev -> maintainer-review.md
system-rev -> system-review.md
ponytail-rev -> findings-validation.md (spawn after discovery; no intended verdict)
Optional focused followup only for material conflict/unresolved path/new validator finding; record rationale if needed.

No selected report or applicable contract may be omitted. Candidate change returns BLOCKED. First remediation review: no two-unsuccessful-round convergence requirement applies yet.
