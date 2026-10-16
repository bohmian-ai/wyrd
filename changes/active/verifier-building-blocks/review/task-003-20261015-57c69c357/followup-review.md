# Focused follow-up: TASK-003 proof ownership and worker identity

**RESOLVED.** Both uncertainties can be resolved from the approved revision-6 authority and pinned candidate `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`. These are discovery conclusions for independent validation, not a task verdict. Later commits are excluded. Only this report was written.

## Inspected paths and limits

Read `subject.md`, `spec-snapshot.md`, `task-snapshot.md`, the pertinent portions of `cumulative.diff`, `task-review-behavior.md` (BEH-08) and `domain-review-security.md` (SEC-003-01). Read the task-review skill, repository agent rules, spec-driven-development and maintainer references, Wyrd design/doctrine and security posture. Used CodeGraph before source navigation; candidate source below was then inspected with `git show 57c69c357:<path>`.

Inspected candidate owners/tests:

- `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs` and `tools/tests.rs`;
- `crates/vala/vala-eval/tests/integration/blocks_agent.rs`;
- `crates/wyrd/wyrd-monty-worker/src/main.rs`;
- `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs`;
- `crates/wyrd/wyrd-server/tests/integration/sandbox_worker.rs`;
- `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs` (existing authority, retry and reclamation regression seams);
- installed `monty-pool-1.1.0/src/worker.rs::Worker::subprocess`.

Also inspected `tasks/TASK-005-server-integration.md` for the cross-task allocation and the supporting proposal's worker-threat discussion; neither implementation claims nor the proposal override the approved snapshots. No tests, privileged launch experiments or additional credential observations were run in this follow-up. The security review's `/proc` observation is corroborating reported evidence; the source and Linux credential semantics independently establish its interpretation.

## 1. BEH-08: author now, compose/execute later — RESOLVED

The task expressly separates authoring from final integrated execution:

- `task-snapshot.md:27–31` assigns TASK-005 accepted authority, pinned evidence and composition, forbidding TASK-003 production endpoints/queue writes/schedules/Operators.
- Approach item 5 (`:58–60`) requires integrated regressions without running full service suites in TASK-003.
- Scenario 6 (`:179–189`) requires focused injected-owner cases now, followed explicitly by: “Author real protected-owner and queued retry/restart regressions for the final merged pass”.
- Verification (`:261–264`) says invoker integration regressions and hosted cross-boundary proofs are updated now and run with the fully merged candidate.
- TASK-005 owns actual acceptance/ACK/queue persistence/claim wiring, its further regression completion, and final execution (`TASK-005` scope, scenarios 1/2 and acceptance). This does not cancel TASK-003's explicit authoring obligation.

Therefore BEH-08 is correct **on the disputed authoring boundary**. A source-level authority gap or missing queue composition implementation is not established by the missing tests, and TASK-003 must not be made to implement TASK-005 to close this proof gap.

The candidate's authored cases do not supply that proof. `blocks_agent.rs:32–58` deliberately refuses registry lookup and `:62–113` supplies only `FixedRead`; it does not provide the registered Agent/Bifrost/code composition regression. `hosted/tools/tests.rs:163–166` explicitly says Oracle's real table decision is for a merged-pass regression, but the ensuing test only invokes tools without underlying grants and query without a tool grant. No matching real-Oracle or queue-to-hosted-authority regression is authored in this range. Existing SQL tests already assert persisted manual authority on claim (`pg_verifier_runs.rs:583`) and cover expired-lease reclamation/retries (`:1443`, `:1552`); these are useful reusable seams, not proof of the new hosted tool boundary after recovery.

**Smallest feasible correction:** add the missing focused invoker/hosted seam cases, plus actual authored protected-owner and recovered-authority regression cases in existing gated test homes. Use existing accepted-authority/queue claim and protected capability owners, then supply the recovered caller through the already available `HostedTools::new(..., caller, ...)` boundary. A test may persist/reclaim accepted authority through the existing queue owner and reconstruct the hosted session without implementing new production enqueue/ACK wiring. Assert a granted tool cannot widen restricted table/Card/delegated access after recovery and cannot reuse another invocation's authority/workspace. Keep new-contract ACK-to-queue composition and its complete restart journey with TASK-005.

Run focused component/seam cases that exist now. Record actual gated selectors and prerequisites for the final merged execution. A prose promise to add tests later, a disabled placeholder, or a test requiring invented future production APIs is not authored regression proof. No new queue, harness, public route or graph engine is needed. BEH-08's unrelated catalog/citation/cancellation subclaims remain for the independent validator; this follow-up does not validate them wholesale.

## 2. SEC-003-01: a namespace does not change the inherited host user — RESOLVED

Approved authority requires a separate unprivileged worker identity (`spec-snapshot.md:483–485`, repeated `:876–883`). It does not mandate a named account, specific numeric IDs, a container, or distinct identities for every worker. The supporting proposal explains the purpose as residual native-interpreter escape containment under a separate unprivileged user; it does not introduce the requirement.

At the candidate:

1. `Worker::subprocess` starts the worker with an empty environment and framed pipes; it performs no credential transition.
2. Worker `main.rs:87–88` creates new user/network/IPC namespaces. No UID/GID mapping or credential-changing call exists before the protocol loop.
3. `confine` applies rlimits, `no_new_privs` and Landlock. These restrictions are real independent controls, but none changes the inherited host UID.
4. `sandbox_worker.rs:93–134` proves namespace inequality and `no_new_privs`, not a separate host account. The worker's opening documentation incorrectly describes overflow display as running under `nobody`.

Linux makes unmapped IDs appear as the overflow ID inside a namespace; file permission checks use IDs mapped into the initial namespace. Creating the user namespace grants capabilities there and removes capabilities in the parent namespace, but does not convert inherited kernel credentials into the overflow host account. See the Linux man-pages project's [user_namespaces(7)](https://man7.org/linux/man-pages/man7/user_namespaces.7.html), sections “Capabilities”, “User and group ID mappings”, “Interaction with system calls that change process UIDs or GIDs”, and “Unmapped user and group IDs”. This explains both the namespace test passing and the security review observing host UID 1000 after confinement; those observations are compatible.

**Discovery conclusion:** retain SEC-003-01's missing separate-user boundary for independent validation. This is not evidence of an actual Python escape, and it does not invalidate the existing filesystem/network controls. Merely asserting a different user-namespace inode or displaying overflow UID does not close it.

**Correction boundary and decision scope:** satisfy the already approved identity requirement at the existing worker launch/confinement boundary, before serving, and fail closed when the necessary host credential transition cannot be established. Conventional OS credential dropping can meet that outcome when the launcher has the required credentials/capabilities; a rootless same-UID process cannot manufacture a distinct host UID just by unsharing. Provisioning necessary launch privilege is an implementation/deployment prerequisite, not inherently a new product/security decision. Preserve the existing parent-serviced mounts, empty environment, Landlock/network restrictions and fresh-process lifecycle. Do not prescribe running the credential-bearing server as root, a new privileged daemon, a setuid helper, containers or a weakened requirement as the correction. Those would need separate assessment if proposed; the approved outcome does not require them.

Revise SEC-003-01's proposed closure proof where it requires **both** host UID and GID to differ: the approved text requires a separate unprivileged identity, not numerically different primary groups. Prove the worker has a distinct unprivileged host user credential, no retained privileged/server-access group authority, and refusal when that separation cannot be established. A shared harmless group is not automatically a violation. No per-worker unique-account criterion should be added.

There is no authority conflict requiring `SPEC_REVISION_REQUIRED` merely to fix this implementation. If the chosen deployment truly cannot furnish the required host identity separation, apply TASK-003's explicit unsupported-isolation stop condition rather than shipping it or converting namespace isolation into a waiver. This follow-up does not select an unapproved deployment architecture to avoid that stop condition.

No additional finding IDs are introduced. Both scoped issues go to the required fresh Ponytail validator with the corrections/limits above.
