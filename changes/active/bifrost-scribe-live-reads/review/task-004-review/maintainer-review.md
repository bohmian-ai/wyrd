# TASK-004 maintainer review (role: maintainer-rev)

- Subject: base `a56ab7569` .. candidate `990803fc0` (worktree HEAD `78bd1049b` differs only under
  `changes/active/opitimization-and-benchmarks`; source read from the worktree and confirmed with
  `git diff --stat 990803fc0 HEAD`).
- Authorities: `AGENTS.md` §5, §8, §16; `architecture/references/languages/maintainer-style.md`;
  task `TASK-004-integrate-eval-server-and-unify-bifrost-memory.md` (Scenario 2 REFACTOR, line 415;
  "Likely owners" paragraph, line 536; stop condition, line 657); spec rev 24.
- Method: static only. I read each changed symbol in its owning module, traced callers and tests with
  `git grep`, and ran mechanical sweeps (symbols deleted in the range but still referenced, changed items
  without rustdoc, doc identifiers that resolve to nothing, schema mirror `cmp`, SDK declaration diff).
  The one exception to static review was a single read-only `rustfmt --check` on `resources.rs`. I did not
  build anything, run lanes, or edit source.

## Changed-surface coverage

| Surface (TASK-004 attributable) | Read / traced | Result |
|---|---|---|
| `resources.rs` governor, `MemoryCgroup`, cgroup walk, `ScribeResources`/`OracleResources`/`ForgeResources`, test-tier hold/park | module doc, consts, owners, `MemoryCgroup::{detect,from_v2_dirs}`, `cgroup_v2_dirs`, `detect_cgroup_cpu`, `detect_snapshot`; callers in boot/config/scribe/forge | Owner shape and docs are sound. Calibration notes C3, C6 |
| `gate/limits.rs` `BifrostTransportAdmission` body owner | full diff; HTTP `body_limit.rs`, gRPC `grpc/mod.rs` callers; tests | Clean |
| `forge/managed/{executor,policy,queue}.rs`, deleted `memory.rs`; `forge/worker.rs` | full diff, `governed_context_for` test, worker config | **MR-3** |
| `boot/data_root.rs` | full diff and tests | Clean |
| `scribe/admission.rs` | full file at the candidate; boot and embedded constructors | **MR-5** |
| `scribe/{ingress,persistence,parquet_writer,member_stager,shards,staging_runtime,claim_assembly}.rs` | added rustdoc and the referenced symbols | **MR-4** |
| `storage/mod.rs` permit wait (`acquire_request`, `governed_decode`, `run_once`) | full diff and tests; `storage/{error,telemetry}.rs` | Code is clean; stale outcome doc is in **MR-4** |
| `oracle/{dispatcher,mod,analytical,analytical_transport,admission,query_stream,peer}.rs` | deleted-symbol sweep, error mapping, admission snapshots, peer vocabulary | **MR-1**; calibration notes C2, C5 |
| `QueryResourcesExhausted` across spec/server/client/SDKs | `wyrd-spec` catalog, `grpc/query.rs`, `query/routes.rs`, `query/service.rs`, `wyrd-client` mapping, tonic conversion, TS `error-codes.ts`, docs | Parity complete. Calibration note C1 |
| Server `config.rs` memory and peer settings | full diff; stale-setting sweep over code, docs, and architecture | Clean. Calibration note C4 |
| `verification/{eval,observations}.rs`, `gate::ObservationAck` | owners, trait justification, `ReadError`, boot wiring | **MR-6**; calibration note C7 |
| `oracle/{peer_service,peer_authority,lifecycle_service}.rs` | outlines, trait impls, fault hook | **MR-1** |
| `wyrd-sql/tenant_conn.rs` | full diff; `sql-foundation.md`; `forge_operations.rs` caller | Clean. Calibration note C8 |
| Generated declarations (Python `.pyi`, TS `.d.ts`, JSON schemas) | SDK diff; `stubs/client.pyi` against `src/client.rs`; schema mirrors | **MR-2**; the public JSON schema part of **MR-1**. TS and schema mirrors are at parity |

## Findings

### MR-1 — DRIFT — Peer-plane code still names and documents signed tickets that TASK-004 deleted

- **Obligation:** Task Scenario 2 REFACTOR (task line 415: "Remove superseded startup, routing, ticket,
  and replay-state paths"). Task line 536: "replace old ... ticket descriptions". Stop condition, line 657:
  "Do not keep both ticket and mTLS protocols". `AGENTS.md` §16 says rustdoc must be accurate, and
  maintainer-style says docs describing deleted behavior are a concrete cost.
- **Evidence (candidate):**
  - The authority now says contexts are unsigned:
    `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:3-12`: "The typed context each request carries
    is unsigned ... origin is the mTLS cluster identity". The keyring and signing code is deleted
    (`peer_keyring.rs`, `peer_credentials.rs`, `grpc/peer_auth.rs`, wyrd-testing `peer_keyring.rs`).
  - Live docs still describe signing:
    - `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:21`: "every stage operation carries a
      Wyrd-signed ticket".
    - `oracle/analytical_transport.rs:459-467`: "covered by the ticket signature".
    - `oracle/analytical_transport.rs:678-693`: "mint tickets", "Signed wire form".
    - `oracle/analytical_transport.rs:883-886`: "the signature must not vary".
    - `oracle/analytical_transport.rs:973,992,1001,1029`: "Tower layer that signs governed stage
      operations".
    - Further hits across `analytical.rs`, `follower.rs`, and `dispatcher.rs`: about 74, 57, and 23 lines
      matching `sign|ticket`.
  - Touched `wyrd-spec` files still describe signing:
    - `crates/wyrd-spec/src/vala/assignment_authority.rs:1-9` and `:503-505`: "the value a leader signs
      into the ticket claims".
    - `crates/wyrd-spec/src/vala/audit_detail.rs:380-384`: "signed into the ticket claims", "a signed
      Analytical stage ticket".
  - That text is published in the public generated schema
    `crates/wyrd-spec/schemas/bifrost_audit_event.json:1376,1383`.
  - Type and trait names still say ticket, and they now wrap unsigned contexts:
    - `oracle/peer.rs`: `PeerTicketClaims` (`:19`), `StageTicketClaims` (`:147`),
      `PeerTicketVerifier::verify_peer_ticket` (`:754-759`),
      `append_unverified_ticket_rejection` / `append_verified_ticket_violation` (`:736,745`).
    - `analytical_transport.rs:905` `ticket_ttl`.
    - `peer_service.rs:68` `ScribeFragmentFault::RejectTicket` ("Reject the fragment's peer ticket").
    - `wyrd-server/src/state.rs:147` "Verifier admitting inbound peer fragment tickets".
- **Consequence:** These are security-boundary docs. A maintainer, or an agent reading the generated
  audit schema, is told that a leader signature makes the claims tamper-evident. That is false: origin is
  only the mTLS cluster identity, and every field is checked against receiver state. Someone changing a
  receiver check could drop a field comparison because they believe "the signature covers it". They
  could also spend time looking for signing keys that no longer exist. The two vocabularies are exactly
  the "both ticket and mTLS" shape the task forbids.
- **Required correction (smallest):**
  - Rename the ticket vocabulary to the context vocabulary `peer_authority.rs` already uses. For
    example: `PeerContextClaims`, `StageContextClaims`, `PeerContextVerifier::verify_context`,
    `context_ttl`, `RejectContext`, and `append_*_context_*`.
  - Rewrite the cited rustdoc so it says each field is compared against receiver-derived state and origin
    is mTLS, with no signing.
  - Regenerate schemas with `mise run codegen:regen` and verify with `mise run codegen:check`.
  - **Testable:** `rg -n -i '\bticket|\bsign(s|ed|ature)\b' crates/vala/vala-bifrost-redux/src/oracle
    crates/wyrd/wyrd-server/src/oracle crates/wyrd/wyrd-server/src/state.rs
    crates/wyrd-spec/src/vala/{assignment_authority,audit_detail}.rs crates/wyrd-spec/schemas` returns no
    peer-plane hits. `codegen:check` and the peer lanes (`mise run test:server:peer`) stay green.

### MR-2 — MISSING — Python stubs omit the new `WyrdClient.server_url` / `grpc_url` properties

- **Obligation:** `AGENTS.md` §8 layer 5: stubs must reflect the source. Maintainer-style: "Generated
  `.pyi` files must reflect the source"; a public declaration that hides a shape is a finding.
- **Evidence:**
  - `sdks/wyrd-sdk-python/src/client.rs:56-67` adds `#[getter] server_url` and `#[getter] grpc_url`
    (commit `4ae6a1992`, the TASK-006 Eval replay carried by TASK-004).
  - TS declares both: `sdks/wyrd-sdk-ts/wyrd/index.d.ts` / `index.d.cts` and `src/index.ts:1088-1099`.
  - The hand-authored Python stub source `sdks/wyrd-sdk-python/python/wyrd/stubs/client.pyi` and the
    assembled `python/wyrd/client/__init__.pyi:9-49` declare only `__init__` and `on_behalf_of`.
  - The new test `tests/unit/client/test_client.py` reads `derived.server_url` / `.grpc_url`, but that
    file is not in the `py:typecheck` file list (`mise.toml:1070`), so nothing catches the gap.
- **Consequence:** Python users and agents get "unknown attribute" from `ty`/pyright on a supported
  public property, and Python stops matching the TS surface for the same client.
- **Required correction:** Add `@property def server_url(self) -> str` and
  `@property def grpc_url(self) -> str` to `stubs/client.pyi`. Their docstrings should match the Rust
  rustdoc, including the `50051` derivation. Reassemble and verify with `mise run codegen:check`.
  **Testable:** add a property read to a typechecked public-typing test, or add `test_client.py` to the
  `py:typecheck` set, and `mise run py:typecheck` passes.

### MR-3 — INCORRECT — Forge comments and rustdoc still describe the deleted unbounded and estimated-memory model

- **Obligation:** Task line 536 ("replace old ... Forge-estimate ... descriptions"); `AGENTS.md` §16.
- **Evidence:**
  - `crates/vala/vala-bifrost-redux/src/forge/managed/policy.rs:272-275`: "Memory and spill are left
    unset so the processor selects the unbounded pool and no disk manager, matching the Forge execution
    model." In fact, `executor.rs:405-439` `governed_context_for` installs the governed pool
    (`resources.rewrite_memory_pool()`) and a `SpillLease` on `forge-spill`.
  - `forge/worker.rs:2319-2323`: "makes the parallelism and memory budgets describe this worker's real
    load". `ForgeWorkerConfig` (`worker.rs:678-693`) no longer has a memory budget, because
    `compaction_memory_budget_bytes` was removed in this range.
  - `forge/worker.rs:8424-8427`: the test doc says "a zero memory budget admits no plan", but the test
    has no such case.
- **Consequence:** The policy comment tells the next maintainer that the core config is supposed to
  select an unbounded pool. Someone "restoring the model" could remove the governed pool or spill lease,
  silently reintroducing uncharged Forge memory. The worker docs point readers at a budget that does not
  exist.
- **Required correction:** Rewrite those three comments. The policy comment should say memory and spill
  come from the attempt's governed `ManagedExecutionContext`. The worker docs should mention parallelism
  only. **Testable:** `rg -n "unbounded pool|memory budget" crates/vala/vala-bifrost-redux/src/forge`
  returns nothing stale. Existing `executor::tests::rewrite_pool_charges_root_and_releases_on_cancel` and
  `worker_admission_bounds_must_be_positive_and_ordered` remain unchanged and green.

### MR-4 — INCORRECT — Scribe and storage ownership docs describe deleted reservations and refusals

- **Obligation:** `AGENTS.md` §16 (rustdoc must describe invariants and side effects accurately).
- **Evidence:**
  - Two field docs say "Move-only footer memory child retained through sealed inspection":
    `scribe/staging_runtime.rs:68` (`AssembleRequest::memory`) and `scribe/claim_assembly.rs:84`
    (`AssembleClaimRequest::memory`). The field type is now `crate::resources::ScribeResources`, a
    `#[derive(Clone)]` capability over the shared governor (`resources.rs:1207-1221`). It is neither
    move-only nor a footer reservation, and `EncodedFooterReservation` was deleted.
  - `storage/telemetry.rs:264`: `StorageRequestOutcome::RateLimited` is documented as "Node-wide
    admission or the backend refused for rate reasons". Node-wide admission no longer refuses: it waits
    inside the bound (`storage/mod.rs:903-929` `acquire_request`, and the `run_once` doc). Only backend
    throttling (`storage/error.rs:114`) produces it.
  - `scribe/telemetry.rs:908`: "held to the same rules as the contention one". The contention registry
    was deleted with `scribe/contention.rs`.
- **Consequence:** A maintainer reading the assembly request expects RAII release on drop. They might
  thread it as an owned reservation, or treat cloning it as a double-charge bug. An operator or maintainer
  reading `rate_limited` would look for node-admission saturation that cannot produce that series.
- **Required correction:** Re-document both `memory` fields as the Scribe capability that assembly
  charges its materialized batches against. Narrow the `RateLimited` doc to backend throttling. Drop the
  contention reference. **Testable:** a docs-only change; `mise run lints` and the existing
  scribe/storage unit tests stay green.

### MR-5 — DRIFT — `AdmissionConfig::memory_limit_bytes` is inert on the production path

- **Obligation:** `AGENTS.md` §5 (owner holds meaningful state); §15 ("remove or decline speculative
  work", fix at the shared owner).
- **Evidence:**
  - `scribe/admission.rs:116-121` documents the field as "Pod memory budget an embedded Scribe sizes its
    private governor from ... admission itself enforces no byte ceiling against it."
  - Production boot still computes and passes it: `wyrd-server/src/boot/mod.rs:650,789`
    (`memory_limit_bytes: pod_memory_limit`), then calls `with_config_and_memory`, which never reads it.
  - The only reader is the embedded/test constructor `scribe/mod.rs:624-626`
    (`embedded_scribe_resources`).
  - At the base, the field drove production admission (`git show a56ab7569:.../scribe/admission.rs`
    lines 253-255, 564, 614). TASK-004 moved the cap to the governor but left the input wired through
    boot.
- **Consequence:** The production Scribe byte cap now appears in two places, one live (the governor's
  `managed_memory_bytes`) and one dead (`AdmissionConfig`). A maintainer changing the boot value, or
  reading it to understand Scribe's ceiling, gets no effect and a wrong mental model.
- **Required correction:** Remove `memory_limit_bytes` from the public `AdmissionConfig`. Give
  `embedded_scribe_resources` its cap through the existing embedded or test-support constructors, for
  example a `const` default or a test-support parameter. Delete the boot assignment. **Testable:** it
  compiles. `mise run test:bifrost` (the scribe embedded and unit tests) and
  `config::tests::server_memory_minimum_rejects_impossible_plan` stay green, and
  `rg memory_limit_bytes crates/wyrd/wyrd-server/src/boot` no longer matches the Scribe admission config.

### MR-6 — INCORRECT — `ObservationEnqueue` claims tracked tasks that nothing ever waits on

- **Obligation:** `AGENTS.md` §16 (docs must describe side effects and cancellation accurately);
  maintainer-style "Documentation: explain what a maintainer cannot infer".
- **Evidence:**
  - `wyrd-server/src/verification/observations.rs:1-11` (module doc: "enqueues ... in a tracked
    background task") and `:34-36` (field `tasks: TaskTracker`, "In-flight enqueue tasks").
  - The tracker is created privately at `:47` and only ever `spawn`ed at `:113`. It is never `close()`d
    or `wait()`ed, and boot (`boot/mod.rs:1105`) hands the owner to Gate without keeping a shutdown
    handle.
  - Compare `oracle/query_audit.rs`, whose tracker is joined at shutdown.
- **Consequence:** A reader concludes that shutdown drains pending Eval enqueues. In fact they are
  detached and can be dropped at process exit. Best effort is allowed by the spec, but the docs claim a
  guarantee the code does not provide. The `TaskTracker` field adds a dependency with no behavior.
- **Required correction:** Pick one and make code and docs agree.
  - Either expose `close_and_wait` on `ObservationEnqueue` and call it from the server shutdown path that
    already drains other tracked tasks;
  - or replace the tracker with plain `tokio::spawn` and document the enqueue as detached and
    best-effort at shutdown.

  **Testable:** with the first option, a unit test that spawns through the hook, closes, and asserts
  `wait()` completes after the task. With the second option, a docs-only change plus
  `mise run test:wyrd` green.

## Calibration notes (non-blocking)

- **C1:** The "503 is retryable except `QueryResourcesExhausted`" rule is coded twice,
  `grpc/query.rs:262-266` and `query/routes.rs:470-471`, each with its own test. A single predicate on
  the error (for example `BifrostError::is_retryable_unavailable`) would make the next non-retryable 503 a
  one-line change. The duplication is small and both paths are tested.
- **C2:** `QueryResourceSnapshot` (`oracle/admission.rs:472-492`) sets `peer_slots` to the same value as
  `admission_slots` and documents it as "peer-worker slot units", although the peer-worker path is
  deleted. `OracleShutdownReport::peer_running` is `live_slot_units()`, which counts all slot units, not
  just peer ones. The names predate TASK-004, so this may be acceptable as test-tier naming.
- **C3:** On `OracleQueryMemoryHold::park_next_after_rows` (`resources.rs:1684-1688`), the first doc line
  ("Arms the next admitted query without a park to park ...") reads garbled.
- **C4:** In `BifrostPeerConfig::read_bundle`, `.filter(|_| self.is_enabled())` is redundant after
  `tls_dir` is `Some`, and the function returns `Result<_, String>` beside the `ConfigError`-typed
  `validate`. The field `BifrostResourceConfig::memory_limit_bytes` maps to the policy's
  `bifrost_memory_limit_bytes` and env `WYRD_BIFROST_MEMORY_LIMIT_BYTES`; aligning the field name would
  help grep.
- **C5:** `ForgeCompactionQueue::resource_map` now stores only parallelism (`HashMap<_, u32>`), so
  `parallelism_by_plan` would describe it. `ORACLE_PARTITION_MEMORY_BYTES` is documented as the per-query
  minimum grant, not a per-partition figure.
- **C6:** `resources.rs` is still a 6.4k-line module (4.5k non-test). Its size predates TASK-004 and it
  shrank in this range, so it is not a finding. Splitting the cgroup and host detection (`MemoryCgroup`,
  `detect_*`) and the test-tier hold/park into sibling modules would ease navigation.
- **C7:** `verification/eval.rs` `ReadError` takes `From<String>`/`From<&str>`, and
  `ObservationEnqueue::enqueue` returns `Result<usize, String>`. Both are stringly crate-local errors
  where §4 prefers `thiserror`. They are acceptable at a fail-open, log-only boundary.
- **C8:** `wyrd-sql/tenant_conn.rs` now has two encodings of the tenant bind:
  - `BIND_CURRENT_TENANT_SQL` is parameterized, hard-codes `'app.current_tenant'` rather than
    `CURRENT_TENANT_GUC`, and is still used by vala-sql `forge_operations.rs:1382`;
  - `begin_tenant_sql` inlines the value and uses the constant.

  The constant's doc ("so the tenant value can be bound as a parameter") no longer describes
  `TenantConn`. One shared fragment would keep them from diverging.

## Overall

**FAIL.** There are six material maintainability findings: MR-1 (signed-ticket vocabulary and docs left
on an unsigned mTLS protocol, including public schema text), MR-2 (missing Python stub properties),
MR-3, MR-4, MR-6 (docs describing deleted or absent behavior), and MR-5 (an inert production config
input). Each correction is small and locally testable. None changes runtime behavior except the MR-6
option to drain at shutdown.
