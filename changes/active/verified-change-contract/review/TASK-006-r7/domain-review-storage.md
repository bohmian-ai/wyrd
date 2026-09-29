# TASK-006 R7 — storage and durability domain review

**Subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543` on `vcc/task-006`. I reviewed the cumulative candidate for storage effects and the focused `f0f6e6363` and `f3c65147e` changes. **Result: PASS.**

## Boundary and authority

| Boundary | Source and authority checked | Result |
|---|---|---|
| Storage construction and callers | `wyrd_storage::factory::build_operator`/`finish_op`, `StorageHandle::from_settings` and `new`, server boot and Eval's direct operator construction; AGENTS.md §§3–6, 11–12 and `architecture/agent-rules.md` | PASS |
| Bifrost read/write durability | `BifrostStorage`'s retrying `run_read` and single-attempt `run_once`, `write_once`, writer operations and delete; `architecture/bifrost-design.md` and the approved TASK-006/spec's durability and startup constraints | PASS |
| OpenDAL transport | Pinned OpenDAL 0.57 `HttpClientLayer` replaces the operator's client and passes operations through; workspace `reqwest` and `opendal` features; changed factory and its test | PASS |

`build_operator` has two production callers: `StorageHandle::from_settings` (server storage and Bifrost/Iceberg users) and Eval's direct operator construction. The test/local `StorageHandle::new` also calls it. No new RetryLayer is installed. OpenDAL's `HttpClientLayer` changes only the HTTP client stored in the accessor; it does not replay a write. Bifrost's write and delete operations still use `run_once`, while its bounded retry remains confined to idempotent reads. The separate signer SDK clients used for upload signing remain outside this fix.

## Focused assessment

- The reproduced rustfs path is reachable: the server's storage readiness check issues object-store operations through the `StorageHandle` operator, and the configured S3 endpoint takes the 2-second idle pool. The recorded 8/40 before and 40/40 after probes and `idle_connection_is_not_reused_after_the_pool_timeout` exercise that path. The test's second request would hit the mock's connection close if the first connection were reused; it checks a new connection was accepted.
- The 15-second S3 and 60-second managed GCS/Azure limits are estimates in source comments, not established provider guarantees. The test `pool_idle_timeout_matches_the_storage_server` checks the selected constants, not real provider idle behavior. This is a **verification limit**, not a task finding: those limits are shorter than the prior 90-second default, no live-cloud behavior is claimed as proven, and the approved R6 task does not require a measured cloud keep-alive guarantee. A middlebox or provider with a shorter idle close can still cause a request failure; writes remain single-attempt by design.
- The new per-operator client reuses an existing dependency and OpenDAL's native layer. Local filesystem operators acquire an unused client, but that has no observable storage effect. No changed path introduces hidden write retry, tenant widening, or a new persistent state owner.

## Verification limits and proposed findings

The recorded storage evidence covers the focused regression, `wyrd-storage` library tests, rustfs and emulator handle CRUD, formatting and lints. I did not rerun those lanes or test a real cloud bucket. The final commit's changed cloud constants and selector were not part of the earlier official-image startup/kind runs; image-proof closure belongs to the task reviewer.

**Material proposed findings: none.**
