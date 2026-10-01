# TASK-006 Wave 1 Security, Authorization, and Privacy Review

## Result

**FAIL**

The candidate preserves tenant isolation at the ingest, Postgres, Oracle, object-path, and result-publication boundaries, but the media resolver does not actually enforce its approved byte ceiling at the read boundary and propagates private storage locators into durable/public execution errors. Both paths are reachable from the continuous Eval workflow required by TASK-006.

## Reviewed boundary

This review traced the complete security-sensitive continuous-Eval path for the immutable subject `f8811ac5035c3aa165d34c38992f9889b3c9081f..55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`:

1. authenticated native Eval observation admission and signed per-row Card scope;
2. Scribe acknowledgement to tracked, fail-open post-ACK enqueue;
3. tenant-RLS binding lookup and run creation;
4. tenant-scoped observation and trace reads through Oracle;
5. tenant object-path validation, media byte resolution, and provider-native binding;
6. Eval error persistence/logging and the public run-status projection; and
7. SYSTEM-only result-table admission, exact Verifier Card scope, and tenant result publication.

The explicit candidate commit remained the reviewed source. The working branch advanced during the review to `01146cf87d22147b87d0c9224aa2bdf67decad92`, but that descendant changes only review/implementation skill documents; `git diff --quiet 55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f --` over the reviewed source files remained clean.

## Authority and source coverage

| Boundary | Governing obligation | Source and test coverage | Result |
|---|---|---|---|
| Observation attribution and enqueue tenancy | REQ-077, INV-007, AGENTS.md tenant isolation | `vala-bifrost-redux/src/gate/mod.rs` (`dispatch_native_frame`), `tables/eval/observations.rs` (`acknowledged`), `wyrd-server/src/verification/observations.rs`, `wyrd-sql/src/queries/verifier_runs.rs` (`enqueue_observation`) | PASS: the acknowledged subject is re-derived from the authenticated principal's signed Card scope, then binding lookup and inserts run through `TenantConn` for `auth.tenant`. |
| Observation and trace reads | REQ-083, REQ-131, AC-016, INV-007 | `wyrd-server/src/verification/eval.rs:270-424`, `query/scheduled.rs`, Oracle authorized-query path | PASS: the server-created read context fixes the claimed run tenant; Oracle remains the query authority. Record selection fixes subject UID, record ID, and frozen UTC day; trace lookup remains tenant-confined. |
| Media URI tenancy and provider secrecy | REQ-131, REQ-084, AC-027 | `wyrd-server/src/verification/eval.rs:694-765`; `vala-eval/src/orchestrator/judge.rs:84-111,183-218`; `vala-eval/tests/orchestrator_judge_skald.rs`; real-server journey `eval_verification.rs:661-814` | PARTIAL: backend scheme and tenant prefix are checked and only native bytes reach the provider, but SEC-T006-01 and SEC-T006-02 remain. |
| Context capture/privacy | REQ-083, eval authority "Context capture", REQ-138 | `wyrd-server/src/verification/engines.rs:51-72`; `verification/results.rs`; `vala-eval/src/results.rs` | PASS for completed results: capture is applied before result batch construction and redacted `actual` is covered by the real-server journey. Error privacy fails separately under SEC-T006-02. |
| SYSTEM result writer | REQ-086, INV-007 | `vala-bifrost-redux/src/gate/mod.rs` authorization matrix and scope validation; `wyrd-server/src/verification/publisher.rs`; existing Gate SYSTEM tests | PASS: ordinary principals cannot write reserved result tables, SYSTEM cannot write other tables, and every native result row must resolve within the token's one exact Verifier scope. |
| Cross-tenant isolation | REQ-078, REQ-131, INV-007, AC-027 | signed scope resolution, `TenantConn`, `AuthorizedQueryContext`, `tenant_path::validate`, journey foreign-tenant media case | PASS for the inspected reachable paths. The negative media journey proves foreign-tenant refusal with no result or dispatch; SQL and Oracle paths are independently tenant-bound. |

Applicable authority read: approved specification revision 35, original TASK-006, `architecture/verifier/eval.md`, `architecture/evidence-context.md`, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` security/ingest scope rules, and `architecture/bifrost-design.md` tenant/query boundaries. The complete candidate diff and callers/tests for the listed boundary were inspected.

## Material findings

### SEC-T006-01 — Media size enforcement has a check/use race and can load an object above the approved ceiling

- **Classification:** INCORRECT / security availability boundary
- **Violated obligation:** REQ-131 requires the server to read bounded media bytes; AC-027 requires oversized media to fail as an input/execution error without provider invocation. The task explicitly requires bounded bytes at the tenant-authorized resolution seam.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/eval.rs:732-752`; `crates/wyrd/wyrd-storage/src/handle.rs:281-303`.
- **Evidence:** `TenantMedia::resolve` first calls `object_len` (`stat`) and rejects a size above 20 MiB, then performs a separate unrestricted `get_object`. `get_object` calls `operator.read(&path.full)` and materializes the complete returned object into a `Vec<u8>`. Nothing constrains the second operation to the metadata version or to `MEDIA_LIMIT_BYTES`, and nothing checks the returned length before base64 expansion and provider submission. The focused test only covers an object that is already oversized at `stat`; it cannot detect replacement between the two operations.
- **Reachable exploit scenario:** a semi-untrusted tenant points an Eval record at an authorized object key, lets the metadata check observe a small version, then atomically replaces that key with a much larger version before the body read. Cloud object replacement (or stale metadata followed by a current body read) makes the two operations observe different versions. The server then allocates and base64-expands the larger object and may send it to the configured judge provider. Repetition can exhaust shared pod memory and create unbounded egress/provider cost despite the 20 MiB contract.
- **Observable consequence:** oversized media can cross the exact boundary intended to reject it; the run need not fail before the body is retained and provider work begins.
- **Required testable correction:** make the storage read itself bounded in one shared `StorageHandle` operation, using the already-installed OpenDAL ranged-read mechanism to read at most `limit + 1` bytes, and reject when the returned body exceeds `limit`. `TenantMedia` must use that operation rather than `stat` followed by unrestricted `get_object`. Add a deterministic storage test whose object changes/grows between metadata observation and body retrieval (or a test backend that reports a small stat and returns a larger body), proving at most `MEDIA_LIMIT_BYTES + 1` bytes are retained and the provider is never invoked.

### SEC-T006-02 — Private media locators enter logs and the public run-status error

- **Classification:** INCORRECT / sensitive data exposure
- **Violated obligation:** `VerificationError` is explicitly the "structured, secret-free execution or delivery error" (`crates/wyrd-spec/src/verification.rs:291-299`). REQ-131 treats the URI as a private durable locator rather than provider/public content, and the approved Eval context-capture boundary requires uncaptured evidence not to cross public or persistence boundaries.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/eval.rs:711-745`; propagation through `crates/vala/vala-eval/src/tasks/judge.rs:138-143`, `crates/wyrd/wyrd-server/src/verification/eval.rs:242-245`, and `crates/wyrd/wyrd-server/src/verification/runner.rs:565-570`; public projection at `crates/wyrd/wyrd-server/src/components/verification/service.rs:183-220`.
- **Evidence:** media resolution converts `tenant_path::validate`, `object_len`, and `get_object` failures with `error.to_string()`. Existing errors include the rejected path (`TenantPathError::ShapeMismatch`) and tenant storage key (`StorageError::ObjectNotFound`); malformed Azure handling can format the complete URL. That string is wrapped in `JudgeError`, then `EvalExecError`, copied into `VerificationError.message`, logged by the runner, persisted on the run, and returned by `GET /verification/runs/{run_id}` to any same-tenant caller with the broad `cards:read` permission. No test asserts that error/status/log output omits the URI or object key.
- **Reachable exploit scenario:** an Eval emission carries an inaccessible or malformed private object URI (including an Azure URI with a credential-like query component or a sensitive object-key name). Resolution fails as required, but the URI/path is retained in the run error and structured warning. A different same-tenant principal with `cards:read`, or an operator with log access, can retrieve data the media contract intended to keep as a private locator.
- **Observable consequence:** storage topology, tenant/card identifiers, object names, and potentially URL query secrets escape into durable status and telemetry even though provider payload tests remain green.
- **Required testable correction:** at `TenantMedia`, map URI/path/storage failures to stable safe error classes containing only the media binding ID and non-sensitive category (`invalid`, `unauthorized`, `missing`, `oversized`, `unsupported`, or transient storage failure). Do not propagate `Display` text from rejected URLs, tenant paths, or storage keys into `VerificationError`. Add a focused failure test with a unique sentinel in the URI path and query, carry the error through run settlement/status, and assert the sentinel is absent from the returned `VerificationError` and captured tracing output while the stable failure remains visible.

## Positive controls

- Gate only invokes the observation hook after Scribe returns durable acknowledgement, and the hook is bounded/tracked and cannot roll back ingest.
- The enqueue reconstructs subject UID from the authenticated principal's signed scope rather than trusting a client UID, then uses tenant RLS for binding discovery and insertion.
- Cross-tenant media paths fail before storage IO through `tenant_path::validate`.
- Provider requests receive base64/native media content; the private URI and `${media:id}` placeholder are not rendered as prompt text.
- Result publication retains the closed SYSTEM table matrix and per-row exact Verifier scope validation.
- Execution/input failures remain errors with no fabricated verdict, result, or Operator dispatch.

## Verification performed and limits

- Ran `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::eval::tests::media_resolves_only_authorized_bounded_supported_objects)'`: **PASS** (1 test). This confirms the current static-size, scheme, tenant-prefix, MIME allowlist, missing-object, and native-content behavior; it does not close either finding.
- Inspected the real-server Eval journey covering native media and foreign-tenant refusal. It asserts only that provider request bodies omit `file://`; it does not inspect persisted/public error messages and does not exercise object replacement between stat and read.
- Broad verification results recorded in TASK-006 were not independently rerun in this domain slice.
- No external cloud backend was used. The size-bound finding follows directly from the two separate backend operations and unrestricted body materialization; closure needs a deterministic seam test plus the repository's storage/backend matrix.
- No source under review was modified.
