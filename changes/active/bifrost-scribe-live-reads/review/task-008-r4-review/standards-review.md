# Independent repository standards review

Overall result: **FAIL**. The four prior corrections are present and the two prior standards findings are closed. Two bounded repository-rule proposals remain in new R4 documentation and recorded verification. Neither alleges a runtime defect.

## Subject and limits

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`; correction parent `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD remained the candidate when rechecked.

Inputs include the approved revision-20 spec, original TASK-007/008, prior R3 standards/verdict and TASK-008-R4, shared navigation, cumulative and correction diffs, governing repository rules and routed references. No sibling current discovery report was read. There is no CodeGraph index. This is static review only: no cargo, nextest, mise, build, test, benchmark, source edit or commit was performed. The only fresh executable checks were the allowed explicit-base whitespace checks.

Current user decisions govern: FIND-007-3 is unchanged; Postgres `data_tenant_id` columns are excluded; `WYRD_VALA_500_QUERY_TENANT_INVARIANT` stands; no live-read cap is required. Neither proposal challenges those decisions.

## Authority and changed-surface coverage

The router is `architecture/references/README.md`. AGENTS.md, agent-rules and spec-driven-development apply throughout. Focused references yield to user decisions and owning architecture; stale secondary row-tenant or audit-WAL prose does not restore a superseded contract.

| Changed surface / owner | Governing authorities and routed references | Source coverage |
|---|---|---|
| Pure Scribe cut, assignment digest, envelope and audit enum (`wyrd-spec`) | AGENTS §§2–6, 9, 11–12, 16; wyrd-design client/Bifrost contracts; bifrost-design tenant and signed-stage authority; architecture/patterns, doctrine/architecture-constraints, languages/rust-core and errors | `vala/api.rs::ScribeProviderCut`, `assignment_authority.rs` v6 encoding/vector, managed column exports, `audit_detail.rs` and generated schema copies |
| Private protobuf / conversions (`wyrd-tonic`) | Same contract authorities; agent-rules generated-artifact/import rules; testing-workflows contract drift | Cut tags/names 7/8 reserved; `private_conversion.rs` conversion and roundtrip; descriptor is supplied generated evidence |
| Scribe memory, stage, writer and schema paths (`vala-bifrost-redux`) | bifrost-design append/seal/live-tail boundaries; domain/olap-serving, arrow-analytical-interop, analytical-operations-reliability and iceberg; rust-core; AGENTS §§3–6, 10–12, 16 | Memtable seal collector, shards/tail request, writer/claim assembly/persistence footer identity, schema/table envelope removals, test-support producer counter |
| Oracle provider, shared scan, follower, transport and error/audit paths (`vala-bifrost-redux`) | bifrost-design planning/terminal/resource authority; security-posture tenant/peer boundaries; domain/datafusion, olap-serving, iceberg and analytical-operations-reliability; languages/errors | Published footer loader/cache, hot/staged scan proof, projection closure, native completion, codec/tripwire deletion, follower live leaf/leases, live decoder, dispatcher status conversion and leader audit |
| Forge rewrite footer (`vala-bifrost-redux`) | bifrost-design Forge publication; domain/iceberg/datafusion/analytical-operations-reliability; AGENTS §§3–6, 9–12, 16 | Managed policy/executor receive table-binding tenant and pass it to existing rewrite writer properties; promotion remains unchanged-object publication |
| Data root, server lifecycle and private peer IO (`wyrd-server`, `wyrd-tonic`) | AGENTS §§3–6, 9–12, 16; bifrost-design ownership/shutdown; security-posture peer identity; operations/reliability-and-recovery; architecture/patterns and rust-core | `BifrostDataRoot::prepare`, root consumers, `BoundServer::run`, peer/public listener calls, `StoppingIo`, `TransportPlane::admits`, `OraclePeerGrpc::execute_fragment` |
| Shared client default / generated configuration (`wyrd-client`) | wyrd-design client model; AGENTS §§2–4, 9–12; architecture/patterns, doctrine/architecture-constraints, testing-workflows | HTTP 8080 default, config docs/tests, identical generated schema copies and client configuration docs; no server-tier dependency introduced |
| Real-server journeys / retained testing topology (`wyrd-testing`, nextest) | AGENTS §11; TESTING.md; languages/testing-workflows, implementation-execution, spec-driven-development; operations/reliability-and-recovery | PeerCluster stop/lifecycle, distributed footer injection, blocked-window public-client journey, former process-harness deletion/registration and existing real-server ownership. Unrelated TASK-006 benchmark completion excluded |
| Architecture, user docs and active evidence | AGENTS §§1–2, 12, 14, 16; wyrd-design/doctrine; bifrost-design; spec-driven-development/implementation-execution | Footer tenant / envelope docs; new cap-free live-tail paragraph; TASK-006 whitespace-only repair; R4 recorded verification |

No Python, TypeScript, PyO3, N-API or UI source/declaration is materially changed in this subject. Those language-specific coding rules are not triggered by the private native correction. Public client contracts continue through the shared client rather than gaining a second implementation.

## Applicable-rule results

| Rule | Evidence / assessment | Result |
|---|---|---|
| Ownership / narrow dependency cone (AGENTS §§2–3, agent-rules) | Pure cut/digest stay synchronous in `wyrd-spec`; Vala owns computation, server owns network serving; IO wrapper remains in existing server-feature tonic module and adds no dependency or feature | PASS |
| Cohesive concrete owner and narrow async (AGENTS §§5–6) | `StoppingIo` owns the accepted socket and cancellation future; poll methods use that state. Existing server listener boundary composes incoming IO into tonic; new peer listener function awaits actual serving IO | PASS |
| Domain identities and truthful error conversion (AGENTS §§4, 9–10; errors) | Footer uses `DataTenantId`; shared stream classifier now retains Aborted as existing TenantInvariant; live consumer maps that to existing typed public error, retaining leader audit owner | PASS |
| Tenant-before-decode, no compatibility fallback (bifrost-design tenant invariant) | Mandatory published loader and hot/staged metadata helper prove footer before decoding; memtable retains seal-key binding; no row filter/tripwire compatibility owner remains | PASS statically |
| mTLS connect info / private versus public plane (security posture) | `StoppingIo::ConnectInfo` remains `TcpConnectInfo`, forwarded from socket; `TransportPlane::admits` consumes `TlsConnectInfo<TcpConnectInfo>`. Peer task uses new helper at app/server.rs:678; public task keeps existing helper at :662 | PASS statically |
| Cancellation / durable ownership (bifrost-design live-tail and failure invariants) | Poll methods register cancellation waker and fail read/write/flush after shutdown; write-half shutdown still delegates. Existing role drain/WAL/stage settlement remains outside connection wrapper, and stream drop releases retained source owners | PASS statically; supplied journey proof |
| Contract parity / generated artifacts (agent-rules, testing-workflows) | Removed cap fields agree across domain/conversion/proto reservations; v6 domain/vector reflects changed bytes; audit/config schema pairs match source changes. Prior codegen evidence is supplied, not freshly regenerated | PASS within static limits |
| Current architecture matches cap deletion (AGENTS §§1–2, 14) | bifrost-design.md:310–318 now states shallow source references, no byte/batch cap, execution-pool governance, writer epoch/deadline/cancellation and stream lifetime | PASS; FIND-007-8 CLOSED |
| Required whitespace gate (AGENTS §12; implementation-execution) | `git diff --check a7582db58 HEAD` and `git diff --check HEAD~1 HEAD` both exit zero; TASK-006 content is preserved | PASS; FIND-007-7 CLOSED |
| Every new Rust item documented; every fallible method has Errors (AGENTS §16; agent-rules; rust-core Documentation) | New IO trait implementations have no Errors sections; ConnectInfo associated type has no rustdoc | FAIL; RSTD-R4-1 |
| Real-server negative / cancellation journey; no weakening (AGENTS §11) | New distributed test rewrites a staged footer and requires zero rows, Failed/QueryTenantInvariant, one leader security signal. Blocked-window journey uses ordinary ingest/public client, graceful Scribe stop, producer and Oracle release; old public helper unchanged | PASS statically; supplied Oracle 42/42 and peer 11/11 |
| Every named test has exact recorded focused command (AGENTS §11; spec-driven-development Test command precision) | R4 names classifier and remote staged-footer tests, but its commands only give an exact expression for window-stop and drop tests | FAIL; RSTD-R4-2 |
| Relevant format/lint/docs verification (AGENTS §§11–12) | Supplied R4 evidence reports fmt, lints and docs:check exit zero; no test/build run during review | PASS as supplied evidence |

## Proposed findings

### RSTD-R4-1 — new peer IO items omit mandatory rustdoc details

- Classification: VIOLATION.
- Governing rule: AGENTS §16 explicitly includes associated types among documented items and requires `# Errors` for every fallible method regardless of visibility. Agent-rules marks missing/incomplete rustdoc BLOCK_BEFORE_MERGE; rust-core Documentation repeats the requirement.
- Exact locations: `crates/wyrd/wyrd-tonic/src/server/mod.rs:321` (`poll_read`), :334 (`poll_write`), :345 (`poll_write_vectored`), :361 (`poll_flush`), :368 (`poll_shutdown`), and :374 (`ConnectInfo`).
- Evidence: the first four methods can return the newly introduced ConnectionAborted refusal or forwarded socket errors; `poll_shutdown` forwards a fallible socket operation. Their single-line rustdoc has no Errors section. The associated type has no rustdoc at all. The owning `StoppingIo` docs and `poll_running` Errors section explain cancellation, but do not satisfy the explicit per-item rule; no trait-implementation exception is present in the governing rules.
- Consequence: the new transport boundary fails the repository's explicit documentation completion gate. A maintainer cannot find each operation's error/cancellation distinction in its required method contract. This is a documentation finding, not a demonstrated IO defect.
- Smallest correction: add the method Errors sections describing cancellation refusal versus forwarded socket errors; document that shutdown still delegates after cancellation, and describe the associated TCP connect-info type that tonic wraps with TLS peer certificates. Preserve all runtime code and public/private listener behavior.
- Closure proof: source inspection of those six items against AGENTS §16; ordinary affected formatter/doc checks when execution is authorized. No new test, checker, abstraction or runtime change is needed.

### RSTD-R4-2 — two named R4 tests lack their required exact command record

- Classification: VIOLATION (verification evidence).
- Governing rule: AGENTS §11 requires every specifically named test in a task artifact or implementation report to include and run its exact focused command. Spec-driven-development's Test command precision requires explicit package/target/feature/selector and repository-managed environment wrapper.
- Exact location: `changes/active/bifrost-scribe-live-reads/review/task-008-r3-review/TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md`, Implementation evidence and Commands blocks (lines 92–104).
- Evidence: the table names unit `open_stream_status_preserves_tenant_refusal` and journey `distributed::remote_staged_footer_refusal_fails_closed`, both with RED→GREEN claims. The recorded focused expression selects only `remote_live_window_blocked_scribe_stops_cleanly` and `remote_live_scribe_drop_releases_query`. The broader Oracle suite command is useful but does not replace the mandated named-test command record.
- Consequence: the supplied evidence cannot reconstruct the exact focused classifier and streamed-footer closure runs as required. This does **not** establish that either test was unrun, nor contradict the supplied green broader suite.
- Smallest correction: append the actual exact focused commands and results for those two named tests, with correct target/features and Postgres wrapper for the journey, preserving attribution. If no such focused run exists, perform it only after execution is separately authorized and then record it. Do not rerun a benchmark, broaden gates or alter tests.
- Closure proof: source/test-name/target inspection plus the recorded exact command and selected-test result. This static reviewer must not execute the missing proof.

## Prior correction closure and disposition

FIND-007-7 and FIND-007-8 are closed by fresh whitespace checks and corrected authoritative prose. FIND-007-9's correction is at the shared remote-stream conversion and preserves genuine Unavailable behavior. FIND-007-10's correction reaches accepted peer sockets, preserves `TcpConnectInfo` and keeps public listener wiring unchanged; the supplied new journey directly exercises flow-control blocking and clean stop. Runtime acceptance and domain correctness remain the independent specialists' responsibility.

No new runtime repository-rule regression was established. **Overall FAIL** for the two bounded standards proposals above, subject to independent findings validation.
