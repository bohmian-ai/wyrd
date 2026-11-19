# Code-mode port record (TASK-012)

## Source identities

| Role | Commit |
|---|---|
| A, `change/verifier-code-mode` | `0eb5ccdd9ac0b7b35fa4d4f3c6afd8e091a176f0` |
| B, pipeline line before the port | `fa7c86dba8c3148a9621248dac94a7353ead24c8` |
| Merge base | `11474b4a9aa95499c3fbd3a69393ff7590a5b245` |
| Squash port | `ee068f5a2` (one parent; A is not a merge parent) |
| Contract reconciliation | `bb78916eb` |
| Regenerated projections | `2aecc1fee`, `dfa919edc` |

## Path coverage of A's delta

`git diff --name-status 11474b4a9..0eb5ccdd9` lists 227 paths. 152 are
carried: the port's blob equals A's. 75 are adapted: the port composes A's
change with B's, listed below. No A path is dropped whole; every exclusion is
a hunk inside an adapted path, listed under Excluded. A's untracked
payload-sizing packet and the software-factory work are outside the range and
were not imported. The local task index and the untracked TASK-011 are not
part of any port commit.

| Adapted path | A status |
|---|---|
| `Cargo.lock` | M |
| `Cargo.toml` | M |
| `TESTING.md` | M |
| `architecture/bifrost-design.md` | M |
| `architecture/wyrd-design.md` | M |
| `architecture/wyrd-security-posture.md` | M |
| `crates/shared/wyrd-runtime/src/resources.rs` | A |
| `crates/vala/vala-bifrost-redux/src/scribe/mod.rs` | M |
| `crates/vala/vala-verify/src/blocks.rs` | M |
| `crates/vala/vala-verify/src/blocks/agent.rs` | M |
| `crates/vala/vala-verify/src/blocks/engine.rs` | M |
| `crates/vala/vala-verify/src/blocks/engine/tests.rs` | M |
| `crates/vala/vala-verify/src/blocks/tools.rs` | M |
| `crates/vala/vala-verify/tests/integration/blocks_agent.rs` | M |
| `crates/wyrd-spec/src/error.rs` | M |
| `crates/wyrd-spec/src/verification/execution.rs` | M |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs` | M |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` | M |
| `crates/wyrd/wyrd-server/Cargo.toml` | M |
| `crates/wyrd/wyrd-server/src/boot/mod.rs` | M |
| `crates/wyrd/wyrd-server/src/components/gateway/caller.rs` | M |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs` | M |
| `crates/wyrd/wyrd-server/src/config.rs` | M |
| `crates/wyrd/wyrd-server/src/http/error.rs` | M |
| `crates/wyrd/wyrd-server/src/http/middleware/body_limit.rs` | M |
| `crates/wyrd/wyrd-server/src/http/router.rs` | M |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/engines.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/mod.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/tools/pg_tests.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/hosted/workspace/tests.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/results.rs` | M |
| `crates/wyrd/wyrd-server/src/verification/runner.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_hosted_artifacts.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_hosted_files.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_operator_delivery.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs` | M |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs` | M |
| `crates/wyrd/wyrd-testing/Cargo.toml` | M |
| `crates/wyrd/wyrd-testing/src/bin/capacity/verification/mod.rs` | M |
| `crates/wyrd/wyrd-testing/src/gateway_capture.rs` | M |
| `crates/wyrd/wyrd-testing/src/server.rs` | M |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs` | M |
| `docs/public/llms-full.txt` | M |
| `docs/src/content/docs/api/errors.md` | M |
| `docs/src/content/docs/build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md` | M |
| `docs/src/content/docs/operate/configure-verification-runtime.md` | M |
| `docs/src/content/docs/reference/agent-tools-and-authorization.md` | M |
| `docs/src/content/docs/reference/verification-runtime-limits.md` | M |
| `mise.toml` | M |
| `sdks/wyrd-sdk-python/python/wyrd/testing/__init__.py` | M |
| `sdks/wyrd-sdk-python/python/wyrd/testing/__init__.pyi` | M |
| `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi` | M |
| `sdks/wyrd-sdk-python/src/lib.rs` | M |
| `sdks/wyrd-sdk-python/src/observe/mod.rs` | M |
| `sdks/wyrd-sdk-python/src/testing.rs` | M |
| `sdks/wyrd-sdk-python/tests/integration/conftest.py` | M |
| `sdks/wyrd-sdk-python/tests/typecheck/verify_contracts.py` | M |
| `sdks/wyrd-sdk-rust/src/lib.rs` | M |
| `sdks/wyrd-sdk-rust/tests/integration/main.rs` | M |
| `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs` | M |
| `sdks/wyrd-sdk-ts/native-testing/src/lib.rs` | M |
| `sdks/wyrd-sdk-ts/native/src/lib.rs` | M |
| `sdks/wyrd-sdk-ts/testing/native.cjs` | M |
| `sdks/wyrd-sdk-ts/testing/native.d.cts` | M |
| `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` | M |
| `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | M |
| `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts` | M |

The adapted paths fall into these compositions, each decided by revision 12
or by B's normative contracts:

- **Engine and tools** (`vala-verify` blocks, `wyrd-spec` execution): A's
  tool ownership, settlement and records around B's step/item/iteration/attempt
  identities, producer and `call_id` forwarding, and checkpoints.
- **Hosted capacity** (`hosted/*`, `boot`, `config`, `runner`, `service`,
  gateway caller, `wyrd-testing` server): A's byte-charged workspaces,
  reusable sessions and process ledgers. The ported tree still carries revision
  12's queued runtime, pool and CPU/memory/scratch share; revision 13 replaces
  them with one per-use capacity owner in `combined` and isolation by
  deployment target (`direct`, `worker`), which is pending in TASK-012 OBL-9.
- **Tests**: A's and B's tests both kept; B tests that saturated removed
  counts now hold lane memory, scratch or model calls.
- **Contracts, docs and projections**: reconciled at their owners and
  regenerated.
- **Dependencies** (`Cargo.toml`, `Cargo.lock`, workspace-hack): A's Monty fork
  excluded; every Monty crate resolves from crates.io.

## Excluded from A

| Excluded | Reason |
|---|---|
| `[patch.crates-io]` entries for `monty`, `monty-fs`, `monty-macros`, `monty-pool`, `monty-proto` and `monty-types` (fork revision `e7a848cf0`) | Revision 12: Monty stays on its crates.io release. |
| The fork-only `service_mounts`/`resume_serviced` split in the sandbox | Depends on the excluded fork. The runnable-turn bound runs on the crates.io API. |
| Nested `fs.write` and `publish` through `call_tool` | Revision 12: the nested bridge excludes `code.run`, `fs.write` and `publish`. |
| A's `apply_limits_env` entry point | B's `resolve_limits` owns the same resolution and now also serves the capacity benchmark. |
| A's 200-tenant thresholds | Replaced by 100 tenants; see the amendment below. |

`monty-type-checking`, a local path stand-in, is not from A. It entered at
`0e886ceec` before the merge base and is unchanged by this port.

## Superseded

Revision 12's queued reservation was ported without the direct borrowing it
permitted. The maintainer rejected the reservation, and revision 13 replaces it
([deployment targets](approved-deployment-targets.md)).

## Inherited packet

A's tracked packet at `changes/active/verifier-code-mode/` is carried
unchanged from A's tree, except for the amendment notes below. No review
verdict is added or changed. TASK-001 and TASK-002 keep their recorded review
history. TASK-003-R2 stays `superseded` and TASK-003-R3 stays open.

### Isolation proof and throughput qualification

Maintainer planning correction, 2026-11-16, under approved pipeline revision 15:
TASK-013 owns deterministic 100-tenant fairness, completion, drain and return of
the process memory ledger to its starting charge. Extend the existing held-engine
Postgres integration scenario with one flooding tenant and 99 quiet tenants,
one run each: every quiet run is claimed in the first round, behind at most one
flood run; releasing the engines completes all work and leaves no queued or
leased rows. Saturation comes from configured limits and held engines.

The existing L2 Verification `competing` experiment already overlaps open-loop
direct `observe().verify` and queued `observe().submit` on one `combined` server,
with Scribe evidence writes and capacity → overload → recovery → drain. TASK-013
adds its missing direct-only control at the same direct rate and schedule, in
the existing declaration, record and tables. It delivers a full clean unprofiled
`mise run bench:l2:verification` baseline containing both steps. The existing L1
`direct-verify` record remains the direct-only baseline. L2 is not moved into L1
or given a new command, record format, renderer or pass/fail budget.

TASK-014 owns direct-throughput optimization and the retained six-population
c1/c64 and matched-rate qualification, diagnosis and ledger obligations. It uses
those L1/L2 baselines and re-runs competing with its control after optimization.
Memory levelling off over long runs remains an observation from existing L2
resource windows; fairness, drain and charged-memory return fail deterministically
in TASK-013's integration test instead of being a sustained benchmark qualification.

This supersedes the earlier 100-tenant sustained and two-process assignments in
this port record, TASK-012, and the inherited REQ-033 / AC-034 / TASK-003-R3 V6
amendment notes that refer here. No two-process capacity run is required:
`pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once` already
proves shared-Postgres claiming across two runtimes. It is integration proof,
not a claim of two OS processes. Admission budgets are per process; a second
process does not add proof of the single-process capacity invariant. TASK-013
and TASK-014 carry its exact focused command. Historical verdicts remain intact;
remaining inherited rows close only against their actual evidence.

### TASK-003-R3 obligations

| Row | Disposition | Owner now |
|---|---|---|
| R3-PLAN | carried: process plan unchanged by the port | TASK-012 evidence |
| R3-MEM | changed: one process ledger with no queued reservation (revision 13, `35fd76607`); every hosted charge is held only while in use | TASK-012 OBL-7/OBL-9 |
| R3-CACHE | carried | unverified on the port; TASK-012 lanes |
| R3-SPILL | carried | unverified on the port; TASK-012 lanes |
| R3-REPLICA | proof reassigned by the planning correction above; no two-process capacity experiment | Existing shared-Postgres replica-claim integration test, cited by TASK-013/014 |
| R3-CPU | changed: one runnable-turn bound of the effective CPUs shared by direct and queued work in `combined`; isolation is by `direct` and `worker` pods (revision 13) | TASK-012 OBL-9; TASK-014 |
| R3-ADMIT | changed: tenant-fair waits kept; revision 13 isolates by deployment target | TASK-012 OBL-9; TASK-014 |
| R3-WORKER | carried: one sandbox pool per process | TASK-012 OBL-6 |
| R3-DEADLINE | changed: arrival deadline also lowered by the Verifier's `limits.timeout_seconds` | TASK-012 OBL-8 |
| R3-EDGE | carried | TASK-012 OBL-8 |
| R3-TEL | carried | unverified on the port |
| R3-BENCH | carried | TASK-012 OBL-10 (harness delivered); TASK-013/014 run it |
| R3-QUAL | pending; its recorded FAIL stands | TASK-013 baseline, TASK-014 qualification |
| R3-SUSTAIN | proof corrected above: deterministic 100-tenant fairness/drain/ledger test; long-run memory remains an L2 observation | TASK-013 integration evidence; TASK-014 post-optimization L2 resource windows |
| R3-DIAG | pending | TASK-014 artifact analysis |
| R3-LEDGER | pending | TASK-014 |
| R3-DOCS | changed: reconciled to revision 12 in `bb78916eb`, then to revisions 13–15 in `456546fbb`, `b2afb89a8`, `b78edf183` and `9755b9c2a` | TASK-012 OBL-2 |

### Other inherited rows

| Inherited | Disposition |
|---|---|
| A's spec revision 9 functional and security requirements | carried; amended by pipeline revisions 12–15 for capacity, nested exclusions and tool spend |
| A's REQ-027/029 idle-capacity borrowing | superseded by revision 13: one shared owner per process, isolation by deployment target |
| A's REQ-033 and AC-034 | proof split corrected above: TASK-013 owns the 100-tenant integration invariant and L2 baseline; TASK-014 retains direct qualification and post-optimization L2 observations; replica claiming uses existing integration proof |
| A's TASK-001 and TASK-002 | carried with their recorded review history; no verdict changed |
| A's TASK-003-R2 | superseded, unchanged |
| A's TASK-003-R3 | open; rows above |
| `capacity-diagnosis.md` | carried; it cites source locations and public references only and names no local diagnostic artifact, so none is retained or marked unavailable |
| `verifier-review-pipelines` revision 24 | historical; not reactivated |
| `resource-scaling` revision 5 | one shared packet kept; its measurements are not treated as current |
| `verified-change-contract` REQ-195 | reconciled to the six-control union in `bb78916eb` |
| `software-factory` and A's payload-sizing packet | outside the port |

## Conflict coverage

The merge plan's inventory (37 textual paths, 38 textually clean shared files,
17 semantic conflicts) was resolved on the composed tree; textual cleanliness
was not taken as acceptance. Each semantic conflict's resolution and proof:

| # | Conflict | Resolution | Proof |
|---|---|---|---|
| 1 | capacity guarantees | one shared owner per process; isolation by deployment target (revision 13: `5b358f452`, `35fd76607`, `b2afb89a8`) | TASK-012 OBL-9 |
| 2 | nested bridge broadening | nested calls exclude `code.run`, `fs.write`, `publish` (revision 12) | `blocks_agent::nested_calls_refuse_writable_tools_and_model_calls_keep_their_call_id`, `pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries` |
| 3 | provider `call_id` | forwarded through the one recorder | same tests; `hosted_agent_tools::agent_publish_retries_use_host_identity` |
| 4 | queued state outside the ledger | live state, frozen copies and custody charged to the one ledger | TASK-012 OBL-7 |
| 5 | publication buffers | mounts and snapshots stream through the charged workspace | `pg_durable_verifier_work::publication_mounts_preserve_status_and_lineage`, `pg_hosted_artifacts::workspaces_stream_evidence_to_disk` |
| 6 | `fs.write` accounting | admitted through the scratch ledger (`68b4fb9b3`); replacement credited, refusal leaves content | `workspace::tests::writes_charge_process_scratch`, `hosted_agent_tools::tool_limits_release_capacity_and_preserve_provenance` |
| 7 | borrowed block futures | `walk.rs` kept; host work settles before retry, result or release | TASK-012 OBL-3 |
| 8 | tool-budget recovery | spend recorded as spent and resumed (revisions 14–15) | TASK-012 OBL-5 |
| 9 | direct deadline restart | arrival deadline lowered by the authored timeout | `pg_verification_routes::direct_execution_waits_end_at_the_arrival_deadline`, `pg_verification_runtime::target_loading_spends_the_claimed_deadline` |
| 10 | capture bypass, batching | bound calls never wait for capture; direct results destination-batched; queued ACKs fenced | `gateway::pg_invocation_tests::capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline`, `scribe_outbox::tests::direct_results_deliver_as_destination_batches`, `pg_verification_runtime::queued_origins_require_their_own_input_ack` |
| 11 | direct vs queued inspection | direct keeps no run state; inspection only over HTTP and MCP | `pg_durable_verifier_work::only_queued_verification_keeps_run_state` |
| 12 | mode-aware target cache | direct and queued entries separate, invalidated together | `verification::cache::tests::*` |
| 13 | authorization | RBAC-only verification, `verifier:approve`; nested tools keep their own grants | `pg_verification_routes::direct_execution_is_authorized_by_rbac_alone`, `hosted::tools::pg_tests::tool_and_domain_grants_are_both_required` |
| 14 | dependencies | the plan recommended keeping A's Monty fork; revision 12 overrides it: crates.io Monty, fork patches and fork-only sandbox code excluded | `check:deps`, `Cargo.toml` |
| 15 | generated outputs | regenerated from final sources (`2aecc1fee`, `dfa919edc`) | `codegen:check`, `ts:napi:check` |
| 16 | contributor contracts | Repository is kind 13 everywhere, the contents entry included | `architecture/wyrd-design.md:26` |
| 17 | test-control guards | six-control union in REQ-195, guidance, Rust, Python and TypeScript | `wyrd-testing` `server::tests::public_controls_are_the_six_sanctioned_operations` |

The 37 textual paths follow the plan's resolution column. Rows 1, 11, 12,
14, 15, 24 and 27–29 took the capacity choice of conflict 1; rows 2–6, 16 and
17 conflicts 2, 3, 7 and 8; row 7 conflict 11; rows 9–10 conflicts 9, 10 and
13; rows 13 and 18–20 conflicts 5 and 6; row 21 conflict 4; rows 25, 31 and 34
conflict 17; rows 26, 35 and 36 were regenerated (conflict 15). Rows 8, 22, 23,
30, 32, 33 and 37 keep both sides' tests, exports and options.
