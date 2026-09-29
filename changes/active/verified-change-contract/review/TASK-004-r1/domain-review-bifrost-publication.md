# Domain review: Bifrost result publication

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Boundary: Verification result Arrow construction, remote Bifrost publication,
  Gate/Scribe admission and acknowledgement, sealed replay/deduplication, fresh
  retry writes, and role-separated publication without local Scribe ownership.

The candidate was still `49ad24707de47378b9df51764034c49a129c6b8b`
when this report was written, and the base is its ancestor.

## Authority and source coverage

| Area | Authority and source inspected | Result |
|---|---|---|
| Required publication behavior | Approved spec revision 33, especially `REQ-085`, `REQ-086`, `REQ-087`, `REQ-119`, `REQ-121`, `REQ-122`, `AC-015`, `AC-023`, and `AC-024`; original `TASK-004` Scenario 5 | Covered |
| Physical schema and identity | `changes/active/verified-change-contract/architecture/logic/table_schema.md`; result/detail `DomainTable` declarations; `verification/results.rs`; Scribe native decode, scope resolution, and managed stamping | Covered |
| ACK and replay semantics | `architecture/bifrost-design.md`; analytical reliability, Arrow interop, and OLAP serving references; `wyrd-client::Bifrost::write_batch`; `SealedBatchSender`; gRPC transport; Gate native ingest; Scribe logical-batch identity and durable fence paths | Covered |
| Runner settlement and partial failure | `verification/{publisher,runner,results}.rs`; `VerifierRunQueue::complete/retry`; runtime limits and composition | Covered |
| Gate/Scribe authorization seam | Gate's closed result-table matrix, canonical audit decision, authenticated principal handoff, Card-scope validation, signed UID resolution, managed tenant/principal/Card stamping | Covered |
| Verification evidence | Result mapping unit tests; `pg_verification_runtime`; `pg_grpc_ingest_smoke`; role-separated `wyrd-testing` verification journey; Scribe correlation tests; implementation evidence recorded in `TASK-004` | Covered with the limits and findings below |

## Boundary trace

1. `VerifierRunner::publish` chooses one result ID and one event time, builds
   the payload once for that attempt, and calls `ResultPublisher::publish`.
2. `ResultPayloadBuilder` takes authored fields from the three table owners,
   appends only the admitted correlation columns, omits empty detail batches,
   and orders every non-empty detail batch before the one summary batch.
3. `ResultPublisher` mints the tenant's Verifier-scoped SYSTEM token, constructs
   `wyrd_client::Bifrost` against the configured gRPC endpoint, and awaits each
   `write_batch` sequentially. It has no local-Scribe handle.
4. `write_batch` encodes once and assigns one UUIDv7 batch ID. Both the bounded
   sender retry and the transport retry borrow that same sealed owner. Gate
   validates and audits each request, Scribe checks the table fingerprint and
   signed Verifier scope, stamps tenant/SYSTEM principal/Verifier UID, and ACKs
   only after its WAL and durable batch fence boundary.
5. Only `Ok(())` after every required batch becomes `Transition::Complete`.
   Publication refusal or timeout becomes a visible run retry; acknowledged
   earlier tables remain durable. A later run attempt rebuilds the result and
   invokes `write_batch` afresh, so it receives a new result and batch identity
   and is not represented as a deduplicated replay.
6. `VerifierRunQueue::complete` uses the lease fence and creates failed-result
   Operator dispatch rows in the same Postgres transaction. No dispatch is
   created on publication failure.

This source path satisfies the required detail-before-summary order, zero-detail
behavior, all-ACK completion gate, identical ambiguous-ACK replay, fresh-write
semantics, exact result/run/subject/owner/binding payload identity, trusted
SYSTEM/Verifier managed stamping, and no-local-Scribe design.

## Verification limits

- I did not rerun the expensive Postgres or multi-server lanes during this
  read-only review. The task records successful runs of the applicable lanes;
  source inspection confirms that the named focused tests exist and exercise
  the paths they claim.
- The same-server runtime suite proves ordering, zero-detail publication,
  partial detail success plus summary failure, fresh retry identity, exact
  sealed replay bytes/ID/table, and Scribe deduplication.
- The role-separated journey proves that a runner without local Scribe ownership
  can publish a zero-detail summary through the remote ingest endpoint and query
  it. It does not prove the complete identity/detail obligation in `AC-023`.
- No test drives a crash specifically between an acknowledged detail batch and
  an unacknowledged summary, despite that case being an explicit Scenario 5
  proof obligation. The available runner-crash test crashes before publication.

## Material proposed findings

### BIFROST-PUB-001 — MISSING: the role-separated journey does not verify result/detail identity

- **Violated obligation:** `AC-023` requires the multi-server journey to assert
  that result and detail rows carry the tenant SYSTEM `principal_id`, exact
  Verifier `card_uid`, and explicit subject/owner/binding identities. `TASK-004`
  Scenario 5 also requires multi-server gRPC/Scribe success coverage.
- **Location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:57-117`.
- **Evidence:** The journey enqueues a direct unscored Drift report
  (`VerifierReport::Drift(None)`), so it emits no detail batch. Its only query is
  `SELECT verdict ... WHERE run_id = ...`, and it asserts only one returned row.
  It never reads or asserts `principal_id`, `card_uid`, `subject_card_uid`,
  `owner_card_uid`, or `binding_id`, and cannot exercise the detail table.
- **Observable consequence:** The required role-separated proof can pass if the
  remote path stamps the wrong writer or Verifier identity, loses the explicit
  subject/owner/binding split, or cannot publish a detail batch. Lower-level
  mapping and stamping tests do not satisfy the explicitly required
  real-runtime, multi-server assertion.
- **Required testable correction:** Extend the existing role-separated journey
  (without a new harness) to publish a non-empty binding-created result and
  query both summary and matching detail through Oracle. Assert the exact
  SYSTEM principal, Verifier Card UID, subject UID, owner UID, binding ID, run
  ID, result ID join, and shared event time while retaining the proof that the
  runner node owns no local Scribe.

### BIFROST-PUB-002 — MISSING: no crash proof covers the detail-ACK/summary-unknown boundary

- **Violated obligation:** `TASK-004` Scenario 5 explicitly requires a crash
  test, and `REQ-086`/`REQ-087` require that partial result rows after a crash
  never authorize completed settlement or Operator dispatch without every
  required ACK.
- **Location:**
  `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:489-603` and
  `:800-843`; the unused crash-capable publication seam is
  `crates/wyrd/wyrd-server/src/verification/publisher.rs:238-249`.
- **Evidence:** The partial-write test injects a clean pre-send summary refusal,
  not a crash or ambiguous in-flight summary. The runner-crash test holds the
  engine and crashes before any result batch is published, then observes one
  summary after reclaim. No test acknowledges a detail batch, interrupts the
  publisher while the summary remains unacknowledged, and inspects the run and
  dispatch state across reclaim.
- **Observable consequence:** The mandated crash boundary is unproved. A
  regression that completes or dispatches from an acknowledged detail alone,
  or mishandles the durable run after losing the in-memory sealed summary,
  would not fail the current suite.
- **Required testable correction:** Using the existing runtime/Scribe harness
  and publication fault controls, add one focused test that observes a durable
  detail ACK, interrupts the runner before the summary ACK, and proves no
  completed settlement or dispatch occurs from that partial state. Then prove
  the same durable run is reclaimed within its existing attempt policy and
  settles only after a later attempt receives every required ACK.

## Overall result

**FAIL**

The publication implementation is internally consistent and no source-level
defect was found in the reviewed boundary. Acceptance nevertheless fails
because two explicit production-shaped proof obligations are absent; both are
bounded test corrections using existing owners and harnesses.
