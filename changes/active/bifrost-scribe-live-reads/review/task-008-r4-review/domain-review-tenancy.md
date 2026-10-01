# Independent tenancy and security review — TASK-008-R4

**Result: PASS. Proposed findings: none. FIND-007-9 is CLOSED.**

## Immutable subject and limits

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`; correction parent `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was checked and remained the candidate. Reviewed cumulative tenant/trust-boundary source and the R4 correction, original TASK-007/TASK-008 obligations, and R3 verdict/remediation. Current sibling discovery reports were not read.

Static review only: no builds, tests, Cargo, nextest, mise, commits, or source edits. Supplied fmt/lints/docs checks, Oracle journeys 42/42, peer checks 11/11, focused RED/GREEN evidence, and whitespace checks are attributed implementation evidence, not independently executed proof. Test source was inspected. Fixed maintainer decisions remain excluded from challenge: FIND-007-3, Postgres tenant columns, the existing error code, and absence of a live-read cap.

## Authority and boundary coverage

| Boundary | Governing authority | Source inspected |
|---|---|---|
| Tenant identity and durable server ownership | AGENTS.md §§2–3, 9–11; agent-rules; wyrd-design doctrine; wyrd-doctrine | Server peer composition and Scribe executor; typed bindings, tail snapshots, follower preflight |
| Peer mTLS and receiver-owned context | wyrd-security-posture, Peer identity and distributed Oracle; bifrost-design immutable planning/source authority | wyrd-tonic server TLS and StoppingIo; server grpc TransportPlane/build_peer_grpc; peer_service claims, ticket and digest checks; pinned tonic 0.14.6 connection/TLS source |
| File proof before rows, no compatibility fallback | Spec revision 20 REQ-015/INV-002/INV-006/AC-017; TASK-008; bifrost-design tenant identity; datafusion/olap-serving references | parquet/footer.rs; writer_properties; Scribe writer/assembly; Forge rewrite policy; exec PublishedFooterLoader and HotParquetExec; catalog/follower tenant-qualified sources/cache |
| Remote refusal, terminal and one leader audit | Spec REQ-004/REQ-015; bifrost-design read audit/terminal contract; canonical audit rules | peer_service scribe_stream_error/dispatch_status; dispatcher execute_candidate/stream_status_error; live is_availability_loss/live_error; Oracle audit_tenant_refusal; query_stream terminal mapping; server query_audit |
| Cancellation without widened tenant authority | Spec INV-004/INV-006; R4 preservation constraints; security peer/public separation | app/server listener tasks; StoppingIo; execute_fragment stream; follower live_leaf; staged lease captured by hot scan stream; new remote shutdown journey |
| Review and test evidence | spec-driven-development; maintainer-style; AGENTS.md test runtime/tier rules | R3 verdict/R4 task evidence; staged-footer refusal and blocked-window shutdown journeys; relevant existing tenant/lease tests |

## Findings and source assessment

### Remote staged refusal is preserved end to end

A Scribe staged source receives the authenticated assignment tenant in `follower.rs:816`; its HotParquetExec reader checks retained metadata through `tenant_proven_reader_metadata` (`exec.rs:2961`) before constructing the Arrow reader or decoding rows. Missing, duplicated or foreign `wyrd.bifrost.tenant` yields the typed QueryTenantInvariant from the common proof (`exec.rs:1115`, `footer.rs:50`). Cache hits still run this comparison; published loading also performs it before returning metadata (`exec.rs:1040`). No tenant row/filter fallback was restored.

The lazy failure travels through `peer_service.rs:scribe_stream_error` and `dispatch_status` as gRPC Aborted (`:658`). The schema frame can already have been delivered. The shared `execute_candidate` response loop calls `stream_status_error` (`dispatcher.rs:1501`), whose new Aborted arm is TenantInvariant (`:1815`). `live.rs:618–647` excludes it from availability losses and returns the typed QueryTenantInvariant. It therefore cannot become a pre-row live omission or successful degraded result. The same shared classifier preserves the class for other fragment consumers, while existing unauthenticated/permission/protocol classifications remain terminal.

Both Interactive and Analytical retained-root paths install `audit_tenant_refusal` (`oracle/mod.rs:2421,2459`). Its per-query `Option::take` emits at most one tenant_file event and forwards the failure unchanged (`:2484–2520`). The verified leader query context selects the audit tenant, not the foreign footer or unverified peer bytes. `OracleQueryAudit::append_security_violation` uses that context and `stage` commits through TenantConn and canonical `audit::append_on` (`query_audit.rs:81–107,148–167`). The canonical non-blocking audit failure behavior is preserved. The new `remote_staged_footer_refusal_fails_closed` exercises a sole remote staged source and requires zero rows, Failed/QueryTenantInvariant, and one security event. Its metric assertion proves emission cardinality; canonical durable append ownership follows the inspected existing writer, rather than claiming the test independently reads committed audit rows.

**FIND-007-9 closure: PASS.**

### mTLS identity remains intact through the IO wrapper

`StoppingIo::Connected` uses the exact TcpConnectInfo type and forwards `TcpStream::connect_info` (`wyrd-tonic/server/mod.rs:373–379`). The router is still created through `mutual_tls_server`, with mandatory client CA verification (`:85–116`), before the incoming IO is supplied. Pinned tonic 0.14.6 `io_stream.rs` passes the custom IO through TLS acceptance; its `conn.rs` implements Connected for TlsStream<T> as TlsConnectInfo<T::ConnectInfo> and records the authenticated certificate chain. The resulting extension therefore remains `TlsConnectInfo<TcpConnectInfo>`, matching the existing exact lookup in `grpc/mod.rs:171`. The server feature still enables tonic/tls-connect-info. The same-CA wrong-DNS leaf check and refusal-before-body behavior remain effective; absence of cert information remains denial.

The wrapper adds no principal, tenant, fence, digest or credential authority. Scribe ticket/claims validation, signed assignment digest recomputation, receiver-local fences, and follower preflight still precede provider/tail IO (`peer_service.rs:283–346`; `follower.rs:1560–1620`). Seal-qualified memory snapshots and staged leases retain the same tenant/table identity. The mTLS shared leaf remains cluster-process identity only.

**mTLS and receiver-owned tenant authority: PASS.**

### Listener separation and cancellation preserve security boundaries

Only the private peer task calls `serve_peer_grpc_with_listener` (`app/server.rs:678`); public gRPC retains `serve_grpc_with_listener` (`:662`). Peer services remain mounted through `new_peer` on the mutual-TLS router, while public request authentication remains its separate existing composition. No public shutdown semantics or public TLS configuration were changed.

StoppingIo polls the server cancellation token before reads/writes/flush, registering the connection driver's waker even while the response body cannot advance. Cancellation yields ConnectionAborted, which ends the connection rather than relying on another body poll. The ownership boundary is the stopping private listener's connections, not unrelated listeners or other processes. Its body drop releases captured fragment batches, query execution and scan ownership: `follower.rs:788–845` moves staged protection into the scan; `exec.rs:2895–2905` retains it in the scan stream. Before cancellation socket operations forward unchanged. `poll_shutdown` remains available for socket teardown.

The new blocked-window journey opens a remote live read without draining its public client, requires an open stalled producer, gracefully stops only the Scribe node, requires zero producers, a failed read and released Oracle admissions. This directly addresses the transport-backpressure path absent from the earlier proof. Runtime success is supplied evidence; static inspection supports the selected private-listener correction and reveals no tenant/auth regression.

**Security portions of FIND-007-10 preservation: PASS.** Complete concurrency/drain qualification remains the separate lifecycle reviewer's scope.

## Overall result

PASS for the reviewed cumulative tenancy/security boundary and R4 regressions. No material proposed finding. FIND-007-9 is closed; mTLS connect information, mandatory peer authentication, verified tenant attribution, public listener separation, and hold ownership remain intact. No optional hardening or excluded maintainer decisions are raised.
