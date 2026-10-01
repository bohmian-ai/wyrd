# TASK-004 domain review — security, peer trust boundary, tenancy

Reviewer role: domain-rev (security / peer trust boundary / tenancy), fresh and independent.
Subject: base `a56ab7569` .. immutable candidate `990803fc0`. All source was read at the candidate with `git show 990803fc0:<path>`.
Method: static review only. No builds, mise lanes, Postgres wrappers, or benchmarks were run.

## Reviewed boundary

- **Private peer plane (replayed from vcc/task-006 `0e9c6e98c`).** Covered:
  - `wyrd-server/src/app/server.rs::load_peer_tls`
  - `wyrd-tonic` `MutualTlsServerConfig` (required client CA)
  - `wyrd-server/src/grpc/mod.rs`: `TransportPlane::admits` (leaf must carry the `wyrd-peer` DNS name) and `build_peer_grpc` (which services are mounted only on the mTLS router)
  - `oracle/peer_service.rs`: `reserve_slots`, `release_slots`, `execute_fragment`, `ScribeFragmentExecutor::execute`, `forward_query`
  - `oracle/peer_authority.rs`: `verify_before_decode`, `verify_reservation`, `verify_forward_query`, stage authority, expiry window
  - `oracle/forwarding.rs::accept`, `oracle/lifecycle_service.rs`, `grpc/scribe_tail.rs`
  - `vala-bifrost-redux/src/oracle/peer.rs`, plus `follower.rs::preflight` tenant/binding checks
- **Deletion of the old auth scheme.** Deleted files: `grpc/peer_auth.rs`, `oracle/peer_keyring.rs`, `oracle/peer_credentials.rs`, `wyrd-testing` `peer_keyring.rs`, and `bifrost_peer_test_node`.
  - A tree-wide search at the candidate (excluding `changes/`) for `PeerWorkload*`, `PeerKeyring`, `peer_keyring`, `peer_credentials`, `PEER_(API_KEY|TOKEN|TICKET|KEYRING)`, `denial_audit_concurrency`, and HMAC/nonce replay state found no live caller and no second authentication path.
  - Residue is reported in F-SEC-1.
- **Tenant isolation for remote live Scribe fragments and Oracle reservations.**
  - Fragments: claims tenant is checked against every assignment's tenant, plus the assignment-authority digest and the `authenticated_preflight` binding equality, all before plan decode and IO.
  - Reservations: fence liveness, then typed context binding, then capacity charge.
  - Lifecycle and tail: every request is keyed by the carried typed tenant/binding.
- **Eval observation enqueue, run, and result authority.**
  - `verification/observations.rs`: tenant comes from the authenticated `AuthContext.tenant`, the run uses an RLS `tenant_conn`, and there is a bounded tracked task.
  - `verification/eval.rs`: System-principal read authority, tenant-scoped `TenantMedia`, tenant registry reads. The replay diff against `0e9c6e98c` only changes the query call shape.
- **D13 non-blocking read audit.**
  - `oracle/query_audit.rs`: one bounded queue with a tracked writer, and drops are counted.
  - `vala-sql audit_staging.rs`: `append_audit` now delegates to `append_audit_batch`, so one chaining/hash path remains.
  - Redux `oracle/mod.rs`: `audit_read_decision` stays fail-closed on detail build; `audit_tenant_refusal` stages and passes `QueryTenantInvariant` through; `AuthorizedQueryContext::try_new` still refuses on mismatch.
- **D19 TenantConn begin.**
  - `wyrd-sql/src/tenant_conn.rs`, `scripts/check_tenant_isolation.py` exemption, and `architecture/v1/00-foundations/sql-foundation.md`.
  - Migration `20260601000000_platform.sql`: identical to the pinned TASK-006 source and governed by verified-change-contract REQ-158.

## Authority and source coverage

- AGENTS.md §2: audit write path and publisher, non-blocking Oracle read decisions, tripwire.
- AGENTS.md §9, §11 (journey and negative-flow coverage), §12 (gate circumvention), §16 (rustdoc correctness).
- `architecture/bifrost-design.md` (live tail and distributed analytical execution sections).
- Bifrost spec rev 24: REQ-003, REQ-004, REQ-005.
- verified-change-contract spec rev 44: REQ-159 to REQ-164.
- TASK-004:
  - Owners/Prohibited Changes and the replay map (peer row)
  - Scenario 2
  - Proof matrix row at line 566
  - Material Stop Conditions (lines 652–666)
  - D13 (906–925) and D19 (1088–1103)

## Verification limits

- Static review only. I did not execute any journey, so I cannot confirm pass/fail of:
  - `peer_network::security::peer_context_refusals`
  - `peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete`
  - `tenant_conn::tests::failed_bind_rolls_back_and_the_connection_stays_usable`
- That last test returns early when `WYRD_DATABASE_URL` is unset. Its evidence depends on the `test:sql` run recorded in D19, which I could not observe.
- The owner approval cited in D19 for the one-round-trip raw begin and the check exemption is recorded only in the task. I took it as given.
- Postgres semantics for `SELECT set_config(...,true); BEGIN` are judged from the protocol rules. Statements before `BEGIN` in one simple Query message are absorbed into the explicit block, and a failure before `BEGIN` rolls back the implicit block. I did not run them.

## Confirmed sound (no finding)

- **mTLS.**
  - The listener requires a client certificate chained to the dedicated CA.
  - `TransportPlane::admits` refuses (Unauthenticated) any connection whose leaf lacks the fixed `wyrd-peer` name, and does so before polling the body.
  - Peer services are mounted only on the mTLS router.
  - The listener journey exercises misnamed same-CA, anonymous, foreign CA, expired, and wrong-server-name identities, and asserts zero body polls for the misnamed leaf.
- **Receiver validation before work.**
  - Reserve checks leader fence liveness, then `verify_reservation` (audience, fences, query, operation, body digest, expiry), and only then charges capacity.
  - Release is authorized on the same terms.
  - The Scribe fragment checks the target role, node, and fence first, then verifies context (audience, fence, expiry), then the claims/assignment tenant, binding, and digest. Plan decode (`authenticated_preflight`) and provider IO come after.
  - Forward query runs `verify_forward_query` (audience, fence, expiry) before execution.
  - Stale fence is exercised in `peer_context_refusals` (follower and leader fence) and in unit tests.
- **One scheme.** The old workload-token layer (`PeerWorkloadAuthLayer`), keyring, and signed tickets are gone. REQ-160/164 is satisfied in code; no shim or dual protocol remains.
- **D13** matches AGENTS §2: Oracle read decisions go from one tracked non-blocking task through the canonical append into `vala.audit_staging`. The tripwire still refuses with `QueryTenantInvariant`, and building the decision detail is still fail-closed.
- **D19.**
  - Injection: the inlined value is a typed `DataTenantId` rendered through `Uuid` Display, which cannot contain a quote.
  - Scope: the binding is `is_local=true` and stays transaction-local.
  - Failure handling: `Database` errors map to `TxFailed`.
  - RLS: helper and policies are unchanged.
  - The check exemption names one file and one label. The protected property, that query modules issue no transaction control, still holds and is restated at its authority (`sql-foundation.md`).
- **Eval** enqueue, read, and media authority are tenant-bound through authenticated tenant + RLS + `TenantMedia`. They are unchanged from the approved TASK-006 source apart from the query call shape.

## Findings

### F-SEC-1 — DRIFT — Retired signed-ticket / workload-auth model is still stated as the live peer security model

- **Violated obligation.**
  - TASK-004 Expected Write Set (task line 534–536): "Update architecture/bifrost-design.md and operator docs to replace old role-floor, Forge-estimate, and **ticket descriptions**".
  - Scenario 2 REFACTOR: "Remove superseded startup, routing, ticket, and replay-state paths."
  - verified-change-contract REQ-160: "signed purpose tickets, ticket keyrings, and nonce replay state MUST be retired".
  - AGENTS.md §16: rustdoc must state invariants correctly; documentation is part of correctness.
- **Evidence at `990803fc0`.**
  - `architecture/bifrost-design.md:363-372` still says: "Every stage assignment is versioned, **signed, replay protected**, and binds: … reservation identity, request digest, **nonce** … Peer TLS **and workload authentication** complete before the bounded first frame".
  - `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs` (file materially modified by this range):
    - `:459-462` "Every field here is also covered by the **ticket signature** … Tampering with any of them therefore produces a binding refusal"
    - `:119` "single-use nonce"
    - `:686` "signed stage ticket"
    - `:883` "the signature"
    - `:1319` "a replayed nonce"
    - `:2126` "workload authentication"
  - `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:21` ("Wyrd-signed ticket"), `:1638` ("peer mTLS and workload authentication already did"), `:3737`.
  - `crates/wyrd/wyrd-tonic/src/server/mod.rs:129-134`: `GrpcError::MissingPeerIdentity` is documented as the "resolved peer Service identity" boot check. Its only callers were deleted (base `grpc/mod.rs:409-410`), so it is now dead residue of the retired scheme.
  - The actual model (`peer.rs:1-5`, `peer_authority.rs:1-12`) is mTLS cluster identity plus **unsigned** typed context. The digests prove consistency, not origin. There is no nonce replay state.
- **Observable consequence.**
  - The design authority, and the code docs at the stage-authorization seam, claim tamper and replay protection the system no longer provides.
  - An operator or maintainer reasoning from them would assume a peer holding the shared certificate cannot forge stage claims or replay a stage. REQ-161 explicitly disclaims that protection.
  - The authority and the implementation now disagree on the trust model.
- **Required testable correction.**
  - Rewrite `bifrost-design.md` §"Distributed analytical execution" to state:
    - mTLS cluster identity with the fixed `wyrd-peer` leaf is the only peer authentication;
    - stage context is unsigned typed context checked against receiver state;
    - expiry is a bounded admission window, not replay state;
    - REQ-161's compromised-peer non-goal.
  - Correct the listed rustdoc in `analytical_transport.rs` and `analytical.rs`.
  - Delete the unused `GrpcError::MissingPeerIdentity` variant.
  - Check: a tree grep at the remediation commit for `signed|signature|nonce|workload auth` in those files and in `bifrost-design.md` returns only accurate statements, and `mise run docs:check`, `mise run lints` and `cargo doc` stay green.

### F-SEC-2 — MISSING — "wrong tenant" peer-context refusal is not exercised by the selector the task names, and the Scribe fragment receiver's tenant cross-check has no test at all

- **Violated obligation.**
  - TASK-004 proof matrix line 566: `peer_network::security::peer_context_refusals` must prove "bad peer identity and **wrong tenant**/context/fence fail without result".
  - Scenario 2 Behavior (line 397–398): "Wrong credentials, stale fences, and **cross-tenant contexts** fail without partial results".
  - Material Stop Condition (line 657): do not weaken "receiver context checks, tenant isolation".
  - AGENTS.md §11: a negative flow is pushed down a tier only with a recorded reason.
- **Evidence at `990803fc0`.**
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/security.rs:38-58,196-273`: `peer_context_refusals` drives only the *reservation* plane. The deviations are:
    - destination node and fence
    - source node and fence
    - query id
    - operation
    - expiry
    - protocol version
    - body substitution
  - A reservation context carries no tenant, so no wrong-tenant case exists there or anywhere in `peer_network/`. The only tenant check in `analytical.rs:936` is a *public* foreign-tenant API key, which never forges a peer context.
  - `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:254-270` (`ScribeFragmentExecutor::execute`) refuses when `assignment.binding.tenant_id != tenant_id` (claims tenant) or when the binding name or assignment-authority digest differs. A search of the candidate shows no unit or journey test that reaches this branch:
    - `distributed.rs:1807` uses the injected `RejectTicket` fault, which returns before verification.
    - `distributed.rs:1860` is the TASK-008 data-footer tenant proof, not a context check.
  - The only wrong-tenant context tests are units:
    - `peer.rs:942` flips the stage-claims tenant;
    - the `peer_authority.rs` tests cover an *undecodable* tenant, not a mismatched one.
  - The task records no reason for leaving the remote fragment and stage tenant mismatch unit-only or untested.
- **Observable consequence.**
  - The remote live Scribe fragment's tenant mismatch, receiver-side, before decode, is unproven. The fragment receiver is the one receiver that touches tenant hot-tail data on another pod.
  - A regression that dropped or reordered the claims/assignment tenant comparison would pass the named lane and `test:server:peer`.
  - The acceptance claim "wrong tenant … fail without result" currently has no supporting evidence.
- **Required testable correction.**
  - Add to `peer_context_refusals`, or another `peer_network::` selector run by `test:server:peer`, a real-listener case. It sends an otherwise valid Scribe `ExecuteFragment`, and/or Analytical stage, context whose tenant differs from the assignment's tenant. Variants to cover:
    - claims tenant ≠ assignment tenant, with the digest recomputed for the forged assignment;
    - binding name substitution.
  - Assert a refusal status (`PermissionDenied` or `Aborted`, per `dispatch_status`), zero rows, and no provider or tail IO (e.g., `record_fragment_execution` not advanced).
  - If driving it end-to-end is materially harder, add a focused `ScribeFragmentExecutor` unit test for each branch and record the reason in the task, per AGENTS §11.
  - Run the exact `mise exec -- cargo nextest run … -E 'test(=…)'` selector.

## Overall

FAIL — two findings: F-SEC-1 (DRIFT) and F-SEC-2 (MISSING). The peer code paths, the removal of the old auth scheme, D13 audit semantics, and D19 TenantConn are correct as implemented. Neither finding is a Material Stop Condition trigger in code behavior. Both are required remediations of the task's documentation and proof obligations.
