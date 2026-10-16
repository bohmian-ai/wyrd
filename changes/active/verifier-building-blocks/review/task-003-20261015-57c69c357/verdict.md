# TASK-003: FIX_REQUIRED

The independently validated ledger retains fourteen bounded findings. Existing focused tests pass, but the cumulative candidate does not satisfy the original task's required evidence, isolation, resource and proof boundaries. Route [TASK-003-R1-hosted-runtime-closure.md](TASK-003-R1-hosted-runtime-closure.md) directly to `$wyrd-implement`.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-003`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`, parent of the first TASK-003 implementation commit.
- Candidate: `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`.
- Full actual range: [cumulative.diff](cumulative.diff), covering all eight task implementation commits, including Scenario 1.
- Approved authority: specification revision 6, [spec-snapshot.md](spec-snapshot.md); original task, [task-snapshot.md](task-snapshot.md). Original paths are recorded in [subject.md](subject.md).
- This is the first TASK-003 review. There are no prior TASK-003 finding closures. TASK-001 review history concerns the prerequisite, not this task's remediation.

The active packet was untracked user-provided authority at review start; snapshots and SHA256 identities are preserved in subject.md. The user subsequently instructed the review to ignore later branch edits/commits and continue. The reviewed candidate remains fixed; later edits are excluded, using pinned Git source for validation. HEAD still named the candidate when validation completed, while user worktree edits existed. No reviewed implementation, user edits or original authority were modified by the review. There was no merge, push or deployment.

## Independent reviews and follow-up

| Required role | Preserved report | Discovery result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | FAIL |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | FAIL |
| domain-rev security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL |
| domain-rev concurrency/resources | [domain-review-resources.md](domain-review-resources.md) | FAIL |
| followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| ponytail-rev independent validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED recommendation |

All required roles ran as separate fresh agents. Discovery received the same immutable subject/navigation map and no peer conclusions. The root orchestrated and reconciled reports; it did not validate its own findings. [claims-comparison.md](claims-comparison.md) groups proposals by invariant.

Follow-up was necessary to resolve task-authoring versus TASK-005 integration ownership and the separate-worker-identity interpretation. TASK-003 must author gated authority regressions now, while TASK-005 retains production ACK/queue composition and final journey execution. An unmapped namespace does not change inherited host user credentials; correction must not demand a numerically different harmless primary GID. Independent validation then assessed every proposal, including unique claims and these follow-up conclusions.

## Validated finding ledger

Only independently CONFIRMED or REVISED findings appear here. Full source-local proposal dispositions, producer/consumer and sibling tracing, exact evidence, selected correction boundaries and focused closure proof are preserved in [findings-validation.md](findings-validation.md). The remediation task carries each diagnosis and selected correction as executable work.

Paths below abbreviate `H/` = server `src/verification/hosted/`, `V/` = `crates/vala/vala-eval/`, `W/` = `crates/wyrd/wyrd-monty-worker/`.

| Finding | Status / classification | Candidate location and consequence | Selected correction |
|---|---|---|---|
| FIND-TASK-003-1 | CONFIRMED / MISSING | H/evidence.rs:115–126; H/sandbox.rs:349–362. All exact Artifact evidence/code refuses. | Complete authorized exact Artifact resolution in existing evidence/source owner; reuse inventory and bounded storage. |
| FIND-TASK-003-2 | REVISED / VIOLATION, INCORRECT | H/sandbox.rs:80–105,184–190,234–239,283–329. Sampled aggregate limit permits overshoot; cancellation retains stale raw-PID kill authority. | Worker-only kernel memory cgroup; remove superseded sampler/PID registry; preserve pool/process/allocator boundaries. |
| FIND-TASK-003-3 | CONFIRMED / INCORRECT | H/tools.rs:246–283; H/workspace.rs:145–201. Per-feed quotas reset while work persists; total scratch ignores code writes. | Workspace-lifetime native mount/accounting authority, bounded by cumulative writes and remaining scratch before writes. |
| FIND-TASK-003-4 | REVISED / INCORRECT | H/tools.rs:117–124,246–283. Concurrent calls share an unreserved remainder; cancellation/retry restores spent time. | Existing per-execution/action budget with atomic reservation and elapsed settlement across opens/drop. |
| FIND-TASK-003-5 | REVISED / VIOLATION | H/extract.rs:317–331. Small DOCX gridSpan requests unbounded parent structural allocation. | Bound source-to-derived expansion before allocation in existing extractor. |
| FIND-TASK-003-6 | CONFIRMED / INCORRECT | H/sandbox.rs:194–197. Valid multibyte output can panic the parent task. | Existing UTF-8 boundary truncation pattern with actual-worker boundary proof. |
| FIND-TASK-003-7 | REVISED / INCORRECT | H/workspace.rs:421–451; H/tools.rs:383–387. Oversized unit tail is unreachable and apparent EOF misleading; size estimate ignores encoding. | Subdivide display units under existing continuation and original locators; account encoded bytes. |
| FIND-TASK-003-8 | REVISED / MISSING | H/workspace.rs:382–385,391–451,497–503; tools.rs:224–237. Retained original-slot mapping never reaches concrete filesystem output. | Expose retained mapping for original files through existing results/citation vocabulary. |
| FIND-TASK-003-9 | CONFIRMED / INCORRECT | H/extract.rs:535–568; evidence.rs:69–101,206–246. Valid empty/null JSON/canonical locations rejected. | Preserve empty locations in existing projection while retaining metadata/binding exclusions. |
| FIND-TASK-003-10 | REVISED / INCORRECT | H/tools.rs:285; V/src/blocks/tools.rs:172–188; H/workspace.rs:391–412,466–503,542–566. Result cap follows complete repr/collection/serialization allocation. | Bounded producer formatting/traversal and bounded final serde encoding; retain existing protected owners. |
| FIND-TASK-003-11 | REVISED / VIOLATION | W/src/main.rs:9–11,76–115 and parent launch. Required separate host user credential is absent. | Conventional credential transition at existing worker boundary, preserving other protections and process cap. |
| FIND-TASK-003-12 | REVISED / MISSING | V/tests/integration/blocks_agent.rs; hosted tool/workspace tests; sandbox_worker target. Required concrete protected/recovered-authority, citation and cancellation/process-cap proofs unauthored. | Existing focused seams and gated real-owner/recovery regressions; no TASK-005 production wiring. |
| FIND-TASK-003-13 | CONFIRMED / VIOLATION | H/tools.rs:453–454 and tools/tests.rs. Four ordinary-namespace tests start real Postgres. | Sanctioned pg_tests namespace and managed selectors, preserving protected assertions. |
| FIND-TASK-003-14 | CONFIRMED / VIOLATION | Changed production resolvers/descriptor, local constants and test helpers listed exactly in validation. Mandatory rustdoc/Errors/Panics incomplete. | Batch changed-item docs and relevant cancellation contracts; no prose checker or unrelated rewrite. |

The hard-group correction also closes the stale-watchdog lifecycle proposals; it does not add another guard around a superseded owner. The authoring finding does not claim there is no two-call citation test: the existing test uses a double and does not establish concrete hosted propagation. Optional warm-worker replenishment, speculative parent-death machinery, query-cancellation replacement and style-only preferences are excluded, following validation.

Kernel delegation and worker credential transition are prerequisites of approved containment. They do not authorize a privileged server, helper daemon, setuid infrastructure, new topology or weakened fallback. Refuse unsupported Python capability while preserving unrelated server roles. TASK-005 owns provisioning/wiring; inability to satisfy approved prerequisites invokes the original stop condition rather than a hidden architecture decision. With those boundaries the findings require implementation correction, not a change to approved behavior.

## Reconciled acceptance matrix

| Original obligation | Implementation / verification evidence | Result |
|---|---|---|
| Shared action/check seams, Vala/Skald/server ownership, existing complete workflows | Agent::run_prompt, ToolRegistry, protected Card/Bifrost/query and storage owners; cumulative diff | PASS |
| Prompt inputs/schema, exact route, check-position refusal | AgentExecutor and 9 passing Agent cases | PASS implemented seams; registered/composed proof FAIL (12) |
| Python action/check and exact source provenance | Inline execution and worker cases pass | FAIL Artifact (1), isolation/resources/output (2–6,10–11) |
| Named pinned file validation and existing Artifact evidence | Shared TenantEvidence, hosted size/digest/format cases pass | PASS uploaded File; FAIL Artifact (1) |
| Nine locked extraction shapes and canonical locations | Shared extractor and ordinary fixtures pass | FAIL bounded semantic expansion (5), empty location acceptance (9) |
| Read-only inputs, path/archive isolation and workspace lifetime | Existing guards, TempDir/permits and ordinary tests | PASS ordinary isolation; FAIL cumulative scratch (3), cancellation proof (12) |
| Bounded complete reads/search/list and original-slot mapping | Shared units and concrete hosted dispatch | FAIL (7,8,10) |
| Derived cited output to explicit tool-less downstream check | Existing two-call double test; real TenantEvidence/unit validation separately | FAIL concrete propagation (8,12), empty locations (9) |
| Full built-ins, invoke permission before domain checks, accepted authority | Existing protected owners retain Caller and audit; direct grant cases pass | PASS inspected authorization structure; FAIL full real/recovered-boundary proof (12) |
| Prewarming/max workers/waiters, Python-only checkout, end-to-end deadline | Pool/admission/timeout source and actual-worker saturation cases | PASS ordinary policy; independent process/cancellation proof FAIL (12) |
| Hard group memory and separate worker identity | Sampler and inherited host user contradict mandatory limits | FAIL (2,11) |
| Cumulative Python/work/output budgets | Fresh quotas, deferred time charge and late result guard | FAIL (3,4,6,10) |
| Agent/Prompt/route/extractor/code/tool provenance | Existing structs/recording and passing assertions | PASS supported paths; FAIL missing Artifact and bounded producer cases (1,10) |
| Environment-free fast lane and changed Rust documentation | PgFixture reached in ordinary tests; validated item omissions | FAIL (13,14) |
| Non-goals: no new graph, SDK, endpoints, schedules, Operators or queue implementation | Complete cumulative diff | PASS |
| Credible focused verification | Raw logs show existing cases green | PASS those cases; omitted boundaries remain FAIL |

The full task-review and independent-validation matrices retain the detailed requirement/acceptance mapping. No green lower-level case substitutes for the source gaps above. Final service/SDK journeys remain required at merged change review, not this remediation.

## Verification and limits

Root independently reran checks before user edits appeared:

- `mise exec -- cargo nextest run --locked -p vala-eval --test integration -E 'test(/^blocks_agent::/)'`: **9 passed**, [agent-check.log](agent-check.log).
- Managed Postgres wrapper running `mise run db:migrate:all:inner` then `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/verification::hosted::/)'`: **21 passed**, [hosted-check.log](hosted-check.log).
- `mise run test:verification:sandbox`: worker build and **5 passed**, [sandbox-check.log](sandbox-check.log).
- `git diff --check 41b0f01d9 57c69c357`: PASS.

The standards reviewer independently ran standalone-worker formatting check: PASS. The security reviewer independently inspected actual worker credentials; validation corroborated the source/semantic conclusion and retained the report's observation with its limits. Original format/Clippy/dependency/workspace-hack successes are supplied evidence, not root reruns. No destructive memory/PID experiment, synthetic load, live provider/cloud test, full service suite or SDK journey ran here. No failed lane was weakened or bypassed.

All required reports exist and the independently validated ledger is nonempty. Verdict: **FIX_REQUIRED**. Only a later immutable cumulative task review can pass TASK-003 after remediation.
