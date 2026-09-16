# Wave 1 domain review: Oracle remote forwarding

Result: **PASS**. No material findings in this boundary.

## Immutable subject and authority

- Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; candidate `b000af7ab704f077a8a4ba3e29c2b968d47d344d` (tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`). The source tested at `f8e887b3038651d2ba82091d642915d6b856d4d6` is unchanged in the final evidence-only commit for the files in this boundary.
- Approved spec revision 9: `REQ-015` (one selected attempt, no successor), `AC-011` (one original deadline, cancellation, terminal behavior), and the public deadline range. R3 `FIND-TASK-003-R2-3` requires the selected remote delivery to settle at the captured deadline. Governing authority: `AGENTS.md` §§5–6, 9, 11, 16; `architecture/agent-rules.md`; `architecture/bifrost-design.md` Query/Distributed Analytical Execution/Read Audit and Terminal Contract; `architecture/references/domain/olap-serving.md`; original TASK-003 and R3 remediation task.
- `.codegraph/` is absent. The unrelated dirty `verified-change-contract` files were not inspected or changed.

## Boundary and source coverage

`POST /v1/query` reaches `query::routes::sync_query` → `query::service::stream_query` (capability admission, then edge-timer handoff) → Bifrost Gate's `ReadyOracleForwarder::forward`. The forwarder captures one monotonic and wall deadline, checks the local-ready branch, and otherwise sends remote candidates through `route_remote_once`. I read the full bodies of `forward`, `accept`, `connect`, `forward_remote`, `connect_before_delivery`, `route_remote_once`, the signed `ForwardingAttempt::into_claims`, private `OraclePeerGrpc::forward_query`, and the route/edge service. I also traced the test-only `SilentForwardPeer` accessor from boot composition through `BifrostState` into the real peer handler and the role-separated HTTP journey.

The R3 change is at the existing forwarding owner: after one connection and one signed envelope, `route_remote_once` computes remaining time from the original `Instant` and wraps *only* the selected `deliver` future in `tokio::time::timeout`. `deliver` is `forward_remote`, whose awaited work includes credential acquisition and the initial gRPC `forward_query` response. Timeout maps to `BifrostError::QueryTimeout` and drops that future; no loop follows delivery. Existing peer-status mapping and returned stream handling remain unchanged. The remote peer continues to use the signed wall deadline when it accepts the envelope. The local-ready path was not changed.

## Requirement checks

| Boundary obligation | Implementation and proof | Result |
|---|---|---|
| A connected selected peer cannot hold the pre-header request past the original query deadline | `forwarding.rs::route_remote_once` uses the same `deadline` passed to candidate selection; a pending delivery returns `QueryTimeout`. Focused paused-time test `silent_selected_delivery_times_out_once_and_is_cancelled` passed locally. | PASS |
| Expiry cancels the pending delivery and cannot start another selected attempt | Dropping the `timeout` future drops `forward_remote`; the unit drop probe asserts cancellation, one delivery, and one connection. The real HTTP journey observes one parked envelope across Oracle replicas and the peer handler's abandonment. The prior retry-boundary test also passed locally. | PASS |
| Typed HTTP result and usable ingress after cancellation | `query::service::stream_query` passes the forwarder error back before headers; `query_error_response` emits the Wyrd problem. The recorded role-separated journey uses a Scribe-only ingress and expects `WYRD_VALA_504_QUERY_TIMEOUT`, then a successful forwarded query through the same ingress. | PASS |
| Preserve edge admission and terminal body behavior | `EdgeTimeout` still bounds body/auth/capability work until handoff. `forward_remote` still returns the original forwarded stream with its peer deadline and frame validation after response metadata; no generic edge timer or new public deadline was added. | PASS |
| Production/test isolation | `SilentForwardPeer`, its state field, accessors, and handler call are all gated by the existing `test-support` feature; the feature is enabled by `wyrd-testing`. No production peer parking path exists. | PASS |

## Verification limits

- I reran exact `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::forwarding::tests::silent_selected_delivery_times_out_once_and_is_cancelled) | test(=oracle::forwarding::tests::ready_oracle_forwarding_retry_boundary_is_causal_and_one_cut)'`: 2 passed. I did not rerun the Postgres-backed cluster journey or broad aggregates; the R3 task records 2/2 focused HTTP journeys and 12/12 server journeys on the same source tree, plus a negative control in which removing the timeout causes the new journey to fail.
- The test cluster does not expose a per-node concurrency-limit knob, so the follow-up query is indirect permit-release evidence. Source inspection closes the gap: the protected edge's `ConcurrencyLimitLayer` wraps the request future, and the timed-out pre-header call returns an HTTP response, releasing that future's permit. This does **not** assert that all remote Oracle work has completed at that instant; the separate abandoned-handler observation verifies cancellation of the parked peer handler.
- This review covers remote forwarding only, not the repository-wide acceptance matrix or other changed domains.
