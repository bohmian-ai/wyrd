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
  reusable sessions and process ledgers, with revision 12's queued runtime,
  pool and CPU/memory/scratch share in place of A's single pool, and B's
  lanes reduced to model-call permits.
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
| A's one shared pool with no fixed hosted share | Revision 12, REQ-015: separate queued runtime, pool and reservation. |
| A's `apply_limits_env` entry point | B's `resolve_limits` owns the same resolution and now also serves the capacity benchmark. |
| A's 200-tenant thresholds | Replaced by 100 tenants; see the amendment below. |

`monty-type-checking`, a local path stand-in, is not from A. It entered at
`0e886ceec` before the merge base and is unchanged by this port.

## Not implemented

Direct borrowing of idle queued capacity, which REQ-015 permits ("may"), is
not built: direct runs within its own remainder of the ledgers. Queued work
never borrows direct capacity.

## Inherited packet

A's tracked packet at `changes/active/verifier-code-mode/` is carried
unchanged from A's tree, except for the amendment notes below. No review
verdict is added or changed. TASK-001 and TASK-002 keep their recorded review
history. TASK-003-R2 stays `superseded` and TASK-003-R3 stays open.

### 100-tenant amendment

Revision 12 and the maintainer directions in TASK-013 and TASK-014 replace
the sustained ">=200 tenants" of REQ-033, AC-034 and TASK-003-R3 V6 with 100
tenants. The two-replica proof moves to TASK-014. Notes at those rows record
this; their older threshold no longer applies.

### TASK-003-R3 obligations

| Row | Disposition | Owner now |
|---|---|---|
| R3-PLAN | carried: process plan unchanged by the port | TASK-012 evidence |
| R3-MEM | changed: one ledger now also holds the queued reservation, counted once | TASK-012 OBL-7/OBL-9 |
| R3-CACHE | carried | unverified on the port; TASK-012 lanes |
| R3-SPILL | carried | unverified on the port; TASK-012 lanes |
| R3-REPLICA | pending | TASK-014 two-replica record |
| R3-CPU | changed: queued work also has a bounded CPU share on its own runtime | TASK-012 OBL-9; TASK-014 |
| R3-ADMIT | changed: tenant-fair waits kept; queued has a fixed reservation; no direct borrowing yet | TASK-012 OBL-9; TASK-014 |
| R3-WORKER | carried, per lane | TASK-012 OBL-6 |
| R3-DEADLINE | changed: arrival deadline also lowered by the Verifier's `limits.timeout_seconds` | TASK-012 OBL-8 |
| R3-EDGE | carried | TASK-012 OBL-8 |
| R3-TEL | carried | unverified on the port |
| R3-BENCH | carried | TASK-012 OBL-14 |
| R3-QUAL | pending; its recorded FAIL stands | TASK-013 baseline, TASK-014 qualification |
| R3-SUSTAIN | pending, amended to 100 tenants | TASK-014 |
| R3-DIAG | pending | TASK-014 artifact analysis |
| R3-LEDGER | pending | TASK-014 |
| R3-DOCS | changed: reconciled to revision 12 in `bb78916eb` | TASK-012 OBL-2 |
