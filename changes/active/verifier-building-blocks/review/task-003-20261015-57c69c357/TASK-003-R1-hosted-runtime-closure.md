---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6, FIND-TASK-003-7, FIND-TASK-003-8, FIND-TASK-003-9, FIND-TASK-003-10, FIND-TASK-003-11, FIND-TASK-003-12, FIND-TASK-003-13, FIND-TASK-003-14]
---

# Complete the hosted runtime's approved evidence and isolation boundaries

Route directly to `$wyrd-implement`. This task packages the independently validated findings; it authorizes no implementation during the review that produced it.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-003`.
Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 6, preserved as [spec-snapshot.md](spec-snapshot.md).
Original task: `changes/active/verifier-building-blocks/tasks/TASK-003-hosted-check-runtime.md`, preserved as [task-snapshot.md](task-snapshot.md).
Original cumulative base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Reviewed candidate: `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`.
Verdict and diagnosis authority: [verdict.md](verdict.md), [findings-validation.md](findings-validation.md), and [followup-review.md](followup-review.md).

Later user edits/commits are excluded from this review. Before implementation, accommodate them and inspect whether they already close a finding; never revert them to reproduce the reviewed tree. A later review must retain this original cumulative base, original task, this remediation and the new cumulative candidate. Prior findings close only with source and focused proof.

`H/` below means `crates/wyrd/wyrd-server/src/verification/hosted/`; `V/` means `crates/vala/vala-eval/`; `W/` means `crates/wyrd/wyrd-monty-worker/`. Locations refer to the reviewed candidate. AGENTS.md, agent rules and applicable architecture retain precedence.

## Intended outcome and correction boundaries

The concrete hosted components must honor the already approved Artifact, citation, scratch, worker, time and output contracts through their existing owners. Keep existing Skald execution, protected domain authorization, bounded storage and Monty capability operations. The current passing 9 Agent, 21 hosted and 5 real-worker cases do not exercise the failures diagnosed below.

### 1. Resolve exact Artifact evidence and code — FIND-TASK-003-1

**Diagnosis.** `H/evidence.rs:115–126` accepts only File declarations and `H/sandbox.rs:349–362` accepts only inline source. Every valid Artifact therefore fails before native media, extraction, workspace or code execution; citation resolution also discards these sources. This violates REQ-004/010, and TASK-005 construction cannot repair unconditional refusal.

**Selected correction.** Complete the existing tenant evidence/source boundary with exact, authorized Card identity and server-authoritative Artifact inventory/path resolution, then the existing bounded storage read and stored size/digest validation. Reuse `get_card_by_ref_for`, existing Artifact metadata/path logic and `StorageHandle`; retain the existing permission/audit owner. Storage `download_init` assumes route authorization and must not be treated as an authorizer. Do not mint a download URL to fetch local server bytes or build a second registry/transport. Share loaded immutable bytes across existing consumers and preserve exact source/runtime/digest provenance.

**Acceptance/proof.** Valid exact Artifact code and media work through the relevant code action/check, extraction/workspace/native-media/citation seams. Wrong tenant, grant, kind, UID, path, size or digest refuses before use. Exercise the shared loading authority plus consumer-specific wiring without duplicating a loader.

### 2. Enforce hard worker-group containment and remove stale PID authority — FIND-TASK-003-2

**Diagnosis.** `H/sandbox.rs:80–105,184–190,234–239,283–329` uses a 100 ms RSS sampler, after-the-fact killing and an independently retained busy-PID set. It cannot enforce the hard aggregate ceiling. Cancellation bypasses set removal while the pool kills the child, leaving stale PIDs that can later refer to unrelated processes. The one-byte watchdog test proves eventual killing rather than required containment.

**Selected correction.** Use a worker-only Linux memory cgroup with configured `memory.max`, covering warm, starting, replacement and busy workers before activity; keep the parent outside. Delete the superseded sampler, resident-memory reader and busy-PID bookkeeping rather than adding another PID guard. Preserve the pool/checkout/admission/deadline owner and independent process/allocator limits. Do not divide the group ceiling statically between workers, use a server-wide group or shorten the polling interval as a substitute.

Construction requires deployment-supplied cgroup delegation. Refuse unsafe Python capability initialization/work when unavailable while retaining unrelated server capabilities. TASK-005 owns provisioning/wiring. This task does not choose a privileged server, helper daemon, scheduler or new topology. If approved deployment prerequisites cannot supply enforcement, obey the original unsupported-isolation stop condition; do not implement a soft fallback.

**Acceptance/proof.** Inspect actual kernel limit/membership and parent exclusion. Under small valid limits with normal bounded workers, aggregate pressure fails affected operations, preserves parent/sibling availability and permits recovery. Include idle/starting/replacement membership and repeated cancellation with child exit/capacity recovery. Establish deletion of raw-PID kill authority through source inspection; no host load or PID-reuse experiment.

### 3. Retain cumulative writable and total scratch accounting — FIND-TASK-003-3

**Diagnosis.** `H/tools.rs:246–283` recreates MountSpec/accounting for every fresh feed while `/work` persists. Locked Monty creates a new zeroed write counter per feed. `H/workspace.rs:145–201` checks scratch only during input placement. Individually allowed writes and overwrites exceed the cumulative invocation limit and retained total scratch.

**Selected correction.** Keep one capability-confined native Monty mount/accounting authority with the existing workspace across feeds, with serialized mutation at parent OS-call handling. Reuse `monty-fs::MountTable::handle_os_call`, native write accounting and descriptor roots. Limit cumulative writable bytes to both the configured `/work` allowance and scratch remaining after inputs; successful cumulative writes conservatively bound retained growth. Preserve blocking-IO strategy, fresh interpreters, read-only inputs and cleanup. Failure/cancellation after a successful write cannot restore spent allowance.

**Acceptance/proof.** Small-limit actual mounted writes across sequential and concurrent calls reject before crossing combined allowance, preserve earlier files, and cover overwrites, failure/cancellation after write and near-full input scratch. No downstream read check or per-feed reset substitutes for write-boundary accounting.

### 4. Account Python time across concurrency, cancellation and retry — FIND-TASK-003-4

**Diagnosis.** `H/tools.rs:117–124,246–283` initializes full time per attempt, copies the same remainder into concurrent calls and debits only after await. The actual Skald concurrent tool loop reaches this path. Calls overbook time; cancellation loses charges and retry restores the full budget, contrary to Deployment limits.

**Selected correction.** Own one cumulative budget in existing per-execution HostedTools keyed by the existing action identity across session opens. Atomically reserve before checkout/wait, settle actual elapsed waiting/execution on completion or drop, and return only unused reservation. Preserve per-operation/enclosing deadlines and independence between actions/executions. Do not change global Skald concurrency, block non-Python tools/model waits, reserve workers across model calls or introduce persistent queue accounting.

**Acceptance/proof.** Concurrent small-budget calls cannot collectively overspend; unused time remains usable; cancellation/failure charges consumed time. A second session open for the same action retains spent time; another action/execution is independent. The seam can exercise retry without implementing TASK-005.

### 5. Bound DOCX semantic expansion before allocation — FIND-TASK-003-5

**Diagnosis.** `H/extract.rs:317–331` parses caller-controlled gridSpan as arbitrary usize then allocates covered empty cells. Empty strings consume no text budget. A tiny bounded archive can request huge parent allocation through the shared extraction/workspace/citation path, potentially aborting the shared server.

**Selected correction.** Reject unsupported/invalid spans before expansion using supported table geometry and the existing extraction resource boundary; bound derived structure rather than relying on allocation failure. Preserve merged-cell blanks and locked output. Address the immediately related expansion path together, without a general parser project, new service or public limit family. `try_reserve` alone is insufficient.

**Acceptance/proof.** A tiny malicious-span fixture returns a bounded terminal refusal without allocating its requested vector; ordinary merged/empty-cell output remains exact. No OOM reproduction.

### 6. Truncate stdout on UTF-8 boundaries — FIND-TASK-003-6

**Diagnosis.** The parent print callback in `H/sandbox.rs:194–197` slices at arbitrary bytes. A two-byte character with one byte remaining panics the parent task; ASCII flooding does not test this.

**Selected correction.** Use the existing workspace character-boundary truncation pattern or its minimal standard-library equivalent before append. Preserve the byte ceiling and operation behavior; no panic-catching framework.

**Acceptance/proof.** Actual-worker multibyte output crossing and exactly meeting a small cap yields valid bounded UTF-8 without unwind; a subsequent operation succeeds.

### 7. Make oversized reads complete and correctly bounded — FIND-TASK-003-7

**Diagnosis.** `H/workspace.rs:421–451` truncates an oversized first unit and skips its suffix or returns apparent EOF. The tool at `H/tools.rs:383–387` promises following `next`. Text+64 estimates ignore escaping/metadata and can trigger a later blanket size refusal. Valid long lines/pages become unreadable.

**Selected correction.** Deterministically subdivide display units so the existing private start/next unit cursor reaches all text with original locators. Account actual serialized bytes, including metadata/escaping; explicitly refuse if fixed metadata cannot fit. No new public cursor/citation format or relabelling of original locations.

**Acceptance/proof.** Repeated bounded reads reconstruct an oversized single line/page through its relevant tail with every character and original locator retained. Quote/control-heavy Unicode responses obey actual encoded size; ordinary paging remains correct.

### 8. Expose original-slot mapping through filesystem results — FIND-TASK-003-8

**Diagnosis.** `H/workspace.rs:382–385,391–451,497–503` retains sources but exposes them only to a test getter. Hosted dispatch forwards path/locator/text without original slot. A workspace-only path unrelated to its slot name gives the Agent no reliable way to produce a citation that the real validator accepts.

**Selected correction.** Return the existing source-slot mapping through existing filesystem results for original pinned files, using retained Workspace state and existing Citation/Locator vocabulary. Preserve citations carried in ordinary derived data. Do not infer slots from filenames, relabel derived JSON pointers as original evidence, add an evidence wrapper or inject undeclared Prompt context.

**Acceptance/proof.** Use deliberately different slot/path names, obtain original mapping/locator through concrete fs output, carry cited findings through an explicit downstream binding, and validate with real TenantEvidence. Derived data cannot manufacture an unrelated original location.

### 9. Preserve empty/null citation locations — FIND-TASK-003-9

**Diagnosis.** `H/extract.rs:535–568` drops null/empty values and uses the remaining display leaves to decide existence. `H/evidence.rs:69–101,206–246` consequently rejects real JSON/canonical Office pointers and downgrades grounded passes.

**Selected correction.** Extend the existing shared location projection with explicit empty leaf/container locations and empty display text. Preserve canonical metadata exclusions and out-of-binding checks. No second parse or separate citation protocol.

**Acceptance/proof.** Existing null, empty string/array/object/root and applicable canonical empty/error-cell locations pass; missing sibling/index and unreachable-slot locations fail. Keep text display and invalid-cited-pass behavior.

### 10. Bound results at collection and encoding — FIND-TASK-003-10

**Diagnosis.** `H/tools.rs:285` builds unlimited `py_repr`; `V/src/blocks/tools.rs:172–188` serializes the complete object before checking. `H/workspace.rs:391–412,466–503,542–566` eagerly collects path/work-unit sets. A small result cap therefore permits large parent allocations; zero-byte files evade write-byte-based entry bounds.

**Selected correction.** Bound hosted producer collection and final RecordedTool encoding. Use installed Monty's borrowed formatting into a bounded writer (public StringRepr for strings and Display for other values), standard bounded serde writing and bounded/lazy filesystem traversal. Preserve repr semantics and account for traversal work as well as retained bytes; private dependency formatting methods are not an available API. Keep exact successful digest/byte provenance and the final boundary guard, with bounded serialization. Retain existing protected Card/Bifrost/query owners and their caps; no duplicate service/query policy or unbounded serialization merely to measure output.

**Acceptance/proof.** Large Python values and many small/empty files stop at bounded result/refusal without full repr/tree/work-unit collection. Check read/search/list/code siblings with small limits and quote/control-heavy Unicode; establish early stop through tests and source, without host load.

### 11. Establish the approved separate worker user — FIND-TASK-003-11

**Diagnosis.** `W/src/main.rs:9–11,76–115` creates an unmapped user namespace but never changes inherited host credentials; Monty's subprocess launch also makes no transition. Namespace/no-new-privs proof in the worker test is insufficient. Reported parent-namespace observation confirms the same UID. This omits the separate unprivileged identity requirement, without establishing a Python escape or negating existing controls.

**Selected correction.** At the existing launch/confinement boundary, perform conventional OS credential dropping to a distinct unprivileged host user before serving, remove privileged/server-access group/capability authority, and verify/refuse unsupported separation. Preserve empty environment, mount mediation, Landlock/network isolation, session freshness and resource caps. Ensure process-memory enforcement survives credential separation by applying required limits before dropping authority or inside the existing worker.

The transition requires deployment-furnished credentials/ability. No authority here permits making the server privileged, adding a setuid helper/daemon/container topology or weakening isolation. TASK-005 supplies deployment wiring; unavailable prerequisites invoke the original stop condition, not a fallback. A proposed new provisioning architecture requires its own authority.

**Acceptance/proof.** Parent host-namespace observation proves a distinct unprivileged UID and absence of retained privileged/server-access groups/capabilities before service; unsupported transition refuses. A harmless shared primary GID is allowed. Retain independent mount/network/freshness/memory proofs; namespace inode/overflow display alone does not close this finding.

### 12. Author the required concrete boundary regressions — FIND-TASK-003-12

**Diagnosis.** `V/tests/integration/blocks_agent.rs:32–113,427–478`, `H/tools/tests.rs:163–196`, `H/workspace/tests.rs:222–263` and the actual-worker target lack the required concrete hosted/registered/protected/recovered-authority proofs. A two-call citation test exists, but uses a double; tool tests use an idle sandbox and omit actual Oracle table decisions. Dropping a finished workspace is not cancellation proof. Independent process-cap and parent-filesystem cancellation cases are absent. The task explicitly requires authoring protected-owner/retry/restart regressions now and running the full merged pass later.

**Selected correction.** Add focused existing invoker/tool/evidence seam cases and gated real protected-owner/recovered-authority cases using scripted providers and the existing harness. Cover registered Agent query+code composition, complete catalog dispatch/undeclared calls, concrete citation propagation, restricted and multi-table Oracle refusal, narrowed delegation and distinct-tenant/workspace isolation. Reuse accepted-authority claim/reclaim/retry seams in `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs` and reconstruct the accepted Caller through the existing HostedTools constructor; prove tool grants cannot widen recovered Card/table/delegated scope. Add cancellation during materialization/parent-serviced filesystem work and independent process-memory breach/recovery proof. Eventual cleanup after a started blocking task returns is valid; do not require forced thread cancellation.

Do not duplicate finding-specific tests above. Do not implement new acceptance/ACK/queue production wiring, invent future APIs, add a queue/harness/engine, or execute full service/SDK journeys here. TASK-005 retains that composition and execution responsibility.

**Acceptance/proof.** Executable cases cover each omitted original scenario, with actual selectors and prerequisites recorded. Run currently executable focused seams now; author gated protected/recovery cases for merged execution. Promises, disabled placeholders and future-API sketches do not count.

### 13. Put Postgres tests in the gated namespace — FIND-TASK-003-13

**Diagnosis.** `H/tools.rs:453–454` includes ordinary `tests`, but all four cases call shared fixtures that start PgFixture and a real catalog. Non-pg selectors still require Postgres; passing with the managed wrapper does not make the fast lane environment-free.

**Selected correction.** Use the sanctioned `pg_tests` module namespace and update selections/evidence. Preserve real protected-owner assertions and the managed setup; do not replace authoritative state with weaker doubles to avoid gating.

**Acceptance/proof.** Nextest listing shows the four cases under `verification::hosted::tools::pg_tests::`; the non-pg selection excludes them and exact gated cases pass with managed Postgres/migrations.

### 14. Complete changed-item Rust documentation — FIND-TASK-003-14

**Diagnosis.** The validated ledger identifies absent rustdoc/Errors/Panics on changed TenantRegistry resolvers, StaticResolver, new fallible Agent/extractor/workspace/worker helpers, fixture invariants, local MIB/P/NS constants and the catalog descriptor. AGENTS §16 explicitly makes these omissions blocking; Clippy permits some omissions and cannot prove this gate.

**Selected correction.** Batch substantive docs for the complete changed-item set, explaining actual failures/invariants and applicable cancellation/partial progress. Exact examples and locations are preserved in finding 14 of the validation ledger; include the listed production and test items, not unrelated historical code. Update misleading worker identity/quota comments with their behavioral corrections. No documentation checker or prose-only test.

**Acceptance/proof.** Source inspection covers all newly/materially changed items and fallible/panic/async contracts; normal owning formatting/lint checks pass.

## Preserved behavior, ownership and non-goals

Keep existing tool-invoke plus independent domain authorization and nonblocking audit, original Caller/tenant/delegation attribution, exact routes, check-position tool refusal, all locked extraction shapes, fresh interpreter sessions and normal provenance. Keep Vala/Skald/server/storage ownership and narrow dependencies. No public contracts, SDK features, new tools, external writes, endpoints, queues, schedules, Operators, replacement Agent loop, parser service or plugin platform are authorized. Do not introduce steady-state worker replenishment, speculative parent-death machinery, query-cancellation replacements or style-only rewrites rejected by validation.

Kernel delegation/credential prerequisites are requirements of approved containment. If satisfying them requires a new material deployment/security architecture, stop for that authority rather than choosing it inside this task. Do not waive a limit, weaken a test or take unrelated server roles offline to clear a finding.

## Focused execution and verification

Work through the original behavioral scenarios using Red–Green–Refactor; consolidate tests sharing a root cause. For each newly named test record and run its exact nextest selector through mise after its real name is known. Do not invent selectors for unauthored tests. Use the managed Postgres wrapper for protected/queue-dependent cases and the actual compiled worker for containment cases.

The pinned candidate's relevant module/target iteration commands are below; adapt module paths only when the sanctioned pg_tests relocation changes them. These are focused slices, not full service suites:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --test integration \
  -E 'test(/^blocks_agent::/) or test(/^orchestrator_judge_skald::/)'

mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/^verification::hosted::/) and not test(/::pg_tests::/)'

scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::hosted::tools::pg_tests::/)'"

mise run test:verification:sandbox
mise run fmt
mise exec -- cargo fmt --manifest-path crates/wyrd/wyrd-monty-worker/Cargo.toml -- --check
mise exec -- cargo clippy --locked -p wyrd-server -p vala-eval --all-features --lib --tests -- -D warnings
mise exec -- env RUSTUP_TOOLCHAIN=1.99.0 cargo clippy --locked \
  --manifest-path crates/wyrd/wyrd-monty-worker/Cargo.toml \
  --target-dir target/monty-worker -- -D warnings
mise run check:deps
mise run check:workspace-hack
git diff --check
```

Toolchain/worker commands describe the reviewed candidate; accommodate authorized later toolchain/workspace changes rather than reverting them. Run tenant-isolation checks if remediation changes SQL/connection boundaries. No public schema change is selected; a proposed contract change needs authority and its own generation proof. Record exact gated recovery/protected-owner selections using existing setup, but leave broad full journeys/aggregates to the merged change review.

Completion requires source and focused proof for all FIND-TASK-003-1 through FIND-TASK-003-14, with the complete original task re-reviewed cumulatively. Do not mark the task passed from these closure criteria alone.

## Remediation Evidence

Commits: `7e7a1c326`, `85a04881b`, `b41141dbc`, `6d7117ef8`, `fc0f7f60f`,
`6506f02cf`, `645948f3e`, `d0e35c428`, `5b992c907`, `77d998559`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 Artifact evidence and code | `hosted/evidence.rs::TenantEvidence::{artifact, stored}` via `get_card_by_ref_for` + `list_card_artifacts`; `sandbox.rs::CodeExecutor::run` | `pg_hosted_artifacts::{artifact_evidence_and_code_reach_every_consumer, artifact_reads_refuse_before_use}` | PASS |
| 2 Worker-group containment | INVALID as specified: cgroups/namespaces/`prlimit`/`/proc` violate the locked macOS+Linux, x86_64+arm64 rule (AGENTS.md §2). Intent met portably: the RSS watchdog and busy-PID set are deleted; live workers are capped by `HostedLimits::live_workers` (pool ÷ per-process ceiling) plus Monty's allocator ceiling | `hosted::tests::memory_ceilings_cap_live_workers`, `sandbox_worker::{sandbox_contains_runaway_code, sandbox_bounds_workers_and_waiters}` | PASS (portable) |
| 3 Cumulative `/work` and scratch | `Workspace::{writer, mounts}`: one writer at a time, allowance measured from disk against `/work` limit and scratch left after inputs | `sandbox_worker::workspace_writes_share_one_budget` (incl. failed, overwritten, concurrent, cancelled writers, near-full scratch) | PASS |
| 4 Python time | `tools.rs::Reservation`, per-action budget in `HostedTools` | `tools::tests::reservations_never_overbook_the_budget`, `tools::pg_tests::retries_keep_their_actions_spent_time` | PASS |
| 5 DOCX expansion | span bound before allocation in `extract.rs` | `extract::tests::docx_refuses_impossible_spans` | PASS |
| 6 UTF-8 stdout | char-boundary cut in `sandbox.rs` | `sandbox_worker::sandbox_cuts_output_on_character_boundaries` | PASS |
| 7 Oversized reads | `Workspace::read` pieces with `next` | `workspace::tests::workspace_reads_long_units_in_pieces` | PASS |
| 8 Original slots | `sources` on `fs.list/read/search` | `workspace::tests::workspace_tools_name_original_slots` | PASS |
| 9 Empty/null locations | `extract.rs` pointer existence | `extract::tests::empty_values_are_locations` | PASS |
| 10 Bounded results | `Capped` writers in `tools.rs` (repr) and `vala-eval` `RecordedTool`; bounded list/search | `tools::tests::reprs_stop_at_the_limit`, `workspace::tests::workspace_results_stop_at_the_limit` | PASS |
| 11 Separate worker user | INVALID: setuid/credential switching violates the locked portability rule. No host credential transition is used on any platform; the worker reaches only its mounts | `sandbox_worker::sandbox_reaches_only_its_mounts` | PASS (portable) |
| 12 Boundary regressions | gated Agent step/check over `HostedTools` under a recovered, delegated, table-scoped `Caller::accepted`; cancellation tests | `pg_hosted_artifacts::recovered_authority_bounds_every_agent_tool` (query+code+fs recorded; Oracle "query forbidden" for other table and mixed join; fs.read locator kept as citation by real-evidence check, fabricated one dropped; undeclared tool fails the step), `workspace::pg_tests::cancelled_open_returns_its_admission`, `workspace_writes_share_one_budget` (cancelled writer) | PASS |
| 13 Postgres tests gated | `evidence::pg_tests`, `tools::pg_tests`, `workspace::pg_tests` | Postgres-wrapped selector below | PASS |
| 14 Rustdoc | Errors/Panics on every changed item incl. test helpers and constants | source scan of the remediation diff; `mise run lints` | PASS |
| Agent/Prompt Card reads authorized (found during remediation) | `hosted/registry.rs::TenantRegistry` resolves through `get_card_by_ref_for`/`get_card_for` as the accepted caller; `eval.rs` consumes it until TASK-005 deletes `eval.rs` | `pg_hosted_artifacts::card_resolution_is_the_callers_read` | PASS |

Diagnosis — `eval_verification::background_owner_subject_system_actor`
failed after Card reads were authorized:

- **Symptom:** expected owner/SYSTEM invoke decisions `[B denied, A allowed]`
  were missing B's denial.
- **Evidence:** trace `the Card cannot be read: [WYRD_PERMISSION_403_DENIED_RBAC]
  principal ... lacks Cards/Read` for owner B, whose roles the test deletes.
- **Cause:** `TenantRegistry` now authorizes `card:read` as the caller, so a
  principal with no role stops at Card resolution before `gateway.invoke`.
  This is the intended tightening.
- **Fix site:** the test (read-only diagnostician agreed): B gets a role with
  only `Permission::card_read()`, resolves the judge's Cards, and is still
  denied at `gateway.invoke`; the expected decisions are unchanged. Other
  `JUDGE_ROLE` tests keep default roles and passed unchanged.

Commands:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --test integration \
  -E 'test(/^blocks_agent::/) or test(/^orchestrator_judge_skald::/)'      # 21 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/^verification::hosted::/) and not test(/::pg_tests::/)'       # 19 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::hosted::.*pg_tests::/)'"  # 12 passed
mise run test:verification:sandbox                                        # 10 passed (now needs Postgres)
mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all \
  -E 'test(/^(eval_verification|verification_runtime)::/)'               # 22 passed (Postgres wrapper)
mise run fmt && mise run lints && mise run check:deps && mise run check:workspace-hack
git diff --check
```

Material limits:

- The ledger's "retained MountTable" is not used: monty-pool cannot retain a
  mount table across feeds, so cumulative `/work` accounting is a
  per-workspace writer lock plus a disk-measured allowance. Bytes written
  before a failure or cancellation stay spent.
- Default limits give 5 live workers (2 GiB pool ÷ 384 MiB per process); a
  3 GiB pool restores 8.
- `test:verification:sandbox` now runs under the managed Postgres wrapper.
- Registered Agent Cards are resolved by `TenantRegistry`, but production
  composition into `AgentExecutor` is TASK-005; the gated Agent test uses an
  inline Agent.
