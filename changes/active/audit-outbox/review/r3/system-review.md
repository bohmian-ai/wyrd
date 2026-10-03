# Audit outbox r3 system-resilience review

## Immutable subject and scope

- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128..e54b1244f`
- Authority: `changes/active/audit-outbox/spec.md`, approved revision 3;
  `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/bifrost-design.md`;
  `architecture/wyrd-security-posture.md`; and
  `architecture/operations/runbooks.md`.
- Closure scope: `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, and
  `-3`, plus regressions introduced by this range. Earlier accepted code was
  inspected only where the range could make decisions lost, duplicated,
  misattributed, or cross-tenant.
- Explicitly deferred: `FIND-AUDIT-OUTBOX-5`, `mise run bench:capacity`, and
  `mise run gate`.

The repository has no `.codegraph/` directory. `HEAD` was the candidate before
and after this review.

## Deployed-path evidence

| Path | Process and dependency topology | Candidate effect | Affected capability |
|---|---|---|---|
| Decision capture | Every API-serving `wyrd-server` process shares one `AuditOutbox<AuditSink>`; request surfaces enqueue and return while its writer commits tenant batches to Postgres. | `Outbox::stage` now fences with the handle-owned sender; sink future construction and polling panics are converted to ordinary tenant retry; terminal abandonment clears pending state and the gauge. | HTTP, gRPC, MCP, CLI, SDK, UI-backed server actions, Gate, Oracle, gateway, auth, and verification keep their non-blocking audit path. |
| Graceful shutdown | The server stops transports and drains MCP/gateway/Bifrost work before calling `audit_outbox.shutdown` with the same process deadline (`app/server.rs:744-878`). | Shutdown removes the sender before its first await, drains pre-fence work, refuses post-fence staging, and after deadline cancels the writer and reports/clears the exact remaining pending count (`outbox.rs:189-227`). | Accepted decisions drain until the existing server deadline; deadline residue remains an explicit audit gap rather than permanent live backlog. |
| Staging and unknown commit outcome | `AuditSink` opens one tenant-bound transaction, appends a batch, and commits (`vala-sql/src/audit_outbox.rs:90-110`). Staging uniqueness is tenant plus event ID under RLS. | `event_id` is selected through publication and projected into the retained row (`audit_staging.rs`, `tables/audit/projection.rs:228-265`). A retry while staged is skipped; a retry after retirement can create another retained row under the same ID. | Hash-chain order remains tenant-local and gap-free; duplicate physical rows become detectable. |
| Publication and retirement | Only a process with local Scribe runs `AuditPublisher`; every five seconds it freezes a tenant range, appends through Scribe, then advances the Postgres watermark and retires staged rows. Competing/restarted publishers reuse a frozen range (`audit/publication.rs:179-280`). | The retained schema gains `event_id`; the production publisher preserves it. | Raw `vala.system.audit_log` has enough identity to distinguish repeated delivery, but consumers must actually apply it. |
| Retained reads | The public query owner accepts caller SQL over any authorized table and is reached by HTTP (`query/routes.rs:241-296`), gRPC (`grpc/query.rs:103-143`), MCP (`mcp/bifrost.rs:376-426`), CLI (`wyrd-cli/src/query/mod.rs:49-87`), and the shared Rust/Python/TypeScript SDKs. | No production read owner was changed. Only `WyrdTestServer` helpers discard duplicate event IDs (`wyrd-testing/src/server.rs:1700-1800,1803-1856`). | Public count/list SQL still observes physical duplicate rows. There is no separate shipped audit-specific HTTP, MCP, CLI, SDK, or UI reader that repairs this result. |

## Failure and recovery assessment

| Failure or transition | What stops / remains available | Surviving state and recovery | Proof assessment |
|---|---|---|---|
| Postgres unavailable or a definite append/commit error | Only affected tenant audit commits back off; requests and other tenants continue. | Batch remains at the tenant queue front and retries from 50 ms to 5 s. | Existing focused and integration evidence remains credible; range does not regress it. |
| Sink future panics during construction or polling | The child write fails; the process, writer, other tenants, and request surfaces remain alive. | The child retains the batch, returns it to the ordinary front-of-queue retry path, increments write-failure rather than loss, and preserves later-item ordering (`outbox.rs:304-420`). | Focused test passed in this review. `FIND-12` is closed for the reachable sink panic the remediation required. The residual `JoinError` branch applies only to a panic/cancellation escaping that containment; no production `AuditSink` path to it was found. |
| Graceful shutdown races a producer | Transport/producer drains run before audit shutdown in the deployed server. At the generic boundary, the write lock removes the sender before waiting. | Pre-fence items drain. Post-fence calls never enter the queue and are counted. Deadline residue is counted once and pending/gauge end at zero. | Three focused lifecycle tests passed in this review. `FIND-13` is closed. |
| Abrupt process loss | In-memory queued audit is lost; durable staging and retained rows survive. | Restarted outbox cannot recover memory, which revision 3 explicitly accepts. A restarted publisher resumes frozen publication ranges. | No new abrupt-loss path was introduced by the range. |
| Unknown audit commit outcome while the row remains staged | Requests remain available; publisher may continue. | Retry finds the same tenant/event ID and allocates no second sequence. | Journey phase 1 exercises this production sink/staging behavior. |
| Unknown audit commit outcome after publisher retirement | Requests and publication remain available. | Retry can create another chain entry and retained row with the same event ID. Physical history therefore contains both rows. | The added journey proves two raw rows (`audit_publication.rs:1006-1007`) and then proves only test-harness post-processing collapses them (`:1033-1049`). It does not prove the shipped read contract. `FIND-11` remains open. |
| Publisher process cancellation, panic, restart, or competing replica | That process's in-flight cycle stops; request capture, Postgres staging, and other replicas remain available. | Frozen upper bound and staging survive; the next publisher reproduces the batch identity, Scribe deduplicates, and settlement resumes. | Candidate adds one column to the projection without changing this range identity. No regression found. |
| Rolling replacement across this candidate's retained schema | A previously registered old fingerprint would be refused by current built-in registration. | There is no compatibility path. However, the old retained schema is an unshipped intermediate inside this still-active change, the approved revision explicitly chooses the retained `event_id` schema change, no release tag contains either reviewed commit, and repository history deliberately removed the prior audit-only upgrade path because that schema had not shipped (`b7185d0ee`). | Not a finding for this closure review. Before the final change ships, all final binaries agree on the new schema. This judgment must not be reused if an old fingerprint has actually been deployed as a supported release. |
| Scribe/Iceberg or catalog unavailable | Staging remains durable; retained reads lag or refuse with their existing typed availability outcomes. | Publisher logs and retries a later sweep without retirement. | The range does not alter this failure propagation. |

## Prior-finding closure

| Finding | System result | Evidence |
|---|---|---|
| `FIND-AUDIT-OUTBOX-11` | **OPEN** | Retained rows carry `event_id`, but every shipped query transport returns the raw physical table. Only test-harness helpers collapse it, while the journey itself establishes that public SQL sees two rows. |
| `FIND-AUDIT-OUTBOX-12` | CLOSED | Construction/polling panics retain the owned batch and use ordinary retry. The focused panic test passed. |
| `FIND-AUDIT-OUTBOX-13` | CLOSED | Sender removal is the one-way admission fence; deadline abandonment clears and counts the exact residue. Both focused shutdown tests passed. |
| `FIND-AUDIT-OUTBOX-14` | CLOSED from the system lens | The checker change has no deployed runtime effect and introduces no system failure path. |
| `FIND-AUDIT-OUTBOX-7` | CLOSED from the system lens | Import spelling has no deployed runtime effect. |
| `FIND-AUDIT-OUTBOX-3` | CLOSED from the system lens | Operations prose now attributes failure to the shared audit outbox, matching the deployed owner. |

## Material proposed findings

### SYS-R3-001 — the shipped retained-audit read path still exposes duplicate decisions

- **Maps to prior finding:** `FIND-AUDIT-OUTBOX-11`
- **Classification:** `INCORRECT`
- **Violated obligation:** revision 3 `REQ-009` and `AC-009`: audit reads
  that count or list decisions collapse rows sharing `(tenant, event_id)`.
- **Exact location:** `crates/wyrd/wyrd-testing/src/server.rs:1700-1800` and
  `:1803-1856`; absence of the same behavior from the production owner at
  `crates/wyrd/wyrd-server/src/query/service.rs:185-221`; public consumers at
  `crates/wyrd/wyrd-server/src/query/routes.rs:241-296`,
  `crates/wyrd/wyrd-server/src/grpc/query.rs:103-143`, and
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-426`. The journey demonstrates
  the mismatch at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:1006-1007,1033-1049`.
- **Evidence:** `WyrdTestServer::retained_audit_records` uses an in-memory
  `HashSet` to skip repeated `event_id`, and `retained_audit_rows` rewrites its
  own count to `SELECT DISTINCT event_id`. Those are test-harness methods. The
  production query service forwards caller SQL unchanged to Oracle. HTTP,
  gRPC, MCP, the CLI, and Rust/Python/TypeScript SDKs all share that production
  service and advertise SQL over any authorized table. Therefore
  `SELECT count(*) FROM vala.system.audit_log ...` and an ordinary listing
  return both retained rows that the new journey intentionally creates.
- **Observable system consequence:** after a lost commit acknowledgement races
  publication retirement, operators and agents using shipped surfaces count
  one authorization decision twice and list it twice. Audit cardinality is
  wrong even though raw rows now expose the common event ID; downstream callers
  must know an internal delivery rule to repair the authoritative read.
- **Required correction:** enforce the one-row-per-tenant/event-ID decision
  projection in the production retained-audit read owner, below all public
  transports and SDKs, choosing the earliest sequence as the representative so
  every HTTP, gRPC, MCP, CLI, SDK, and UI query sees the same ordered decision
  set. Preserve raw publisher storage, the single publisher, tenant binding,
  chain columns, and arbitrary SQL semantics for other tables; do not add a
  second ledger/table or duplicate guards in each client.
- **Focused closure proof:** reproduce the existing unknown-commit → publish →
  retire → retry interleaving, then query through at least the production HTTP
  client with both a count and an ordered listing. Prove one logical decision,
  its earliest sequence/attribution, the later distinct decision, and tenant
  isolation. The same server-level proof covers gRPC/MCP/CLI/SDK projections
  because they all dispatch to the same production query owner; retain a
  transport journey for an agent-facing surface if repository journey policy
  requires it.

## Verification notes

- Reviewed the complete range and the owning outbox, audit sink, publisher,
  retained projection, catalog registration behavior, server shutdown order,
  public query transports, client surfaces, harness readers, journey, and
  applicable architecture/runbook authority.
- Ran:
  `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::a_panicking_write_is_retried_once_in_order_without_loss) | test(=outbox::tests::shutdown_refuses_items_staged_after_it_begins) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'`
  — 3 passed.
- `git diff --check cf5ee4128..e54b1244f` passed.
- The implementer's recorded Postgres journey and broader lane results were
  reviewed as evidence. They do not close `SYS-R3-001` because the journey's
  collapse assertions call harness-only readers after separately observing two
  rows through production SQL.
- Capacity and aggregate-gate evidence remain deferred exactly as directed.

## Overall result

**FAIL** — `FIND-AUDIT-OUTBOX-11` is not closed. The candidate makes retained
duplicates identifiable but does not make any shipped audit count/list surface
collapse them; the only collapse logic is in the test harness. No other
system-resilience regression was found in the reviewed range.
