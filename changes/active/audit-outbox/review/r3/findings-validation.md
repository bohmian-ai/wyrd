# Structured Ponytail finding validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Reviewed range: `cf5ee4128..e54b1244f`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 3
- Prior review: `changes/active/audit-outbox/review/r2/`
- Remediation authority:
  `changes/active/audit-outbox/review/r2/TASK-AUDIT-OUTBOX-R2-bounded-remediation.md`

The repository has no `.codegraph/` directory. Validation covered the complete
immutable range, the applicable repository and architecture authority, every
r3 discovery report, the focused follow-up, and the current source for each
producer, consumer, sibling consumer, and proposed correction boundary. The
user-directed scope is closure of `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`,
`-7`, and `-3`, plus regressions introduced by the range. `FIND-5`,
`bench:capacity`, and `mise run gate` remain integration-deferred and are not
findings here.

## Validation status

**COMPLETE — SPEC_REVISION_REQUIRED.** The final ledger contains one preserved
finding, `FIND-AUDIT-OUTBOX-11`. Revision 3 made the post-retirement duplicate
detectable, but the candidate does not give shipped audit decision reads the
required logical cardinality. Independently, repeated real unknown commit
outcomes can exceed AC-009's explicit maximum of one extra retained row. The
smallest safe resolution of that physical-copy conflict changes approved
persistent-delivery semantics and therefore requires human-approved
specification revision. The other five in-scope findings are closed, and no
separate range regression survives validation.

## Producer-to-consumer trace

1. `StagedAuditEvent::from` assigns one stable event ID. `AuditSink::write`
   opens a tenant-bound transaction, calls `append_audit_events`, and returns
   the result of `TenantConn::commit` directly
   (`crates/vala/vala-sql/src/audit_outbox.rs:90-110`). A connection failure
   after the database commit can therefore return `Err` although the row is
   durable.
2. `append_audit_events` checks the event ID only in current
   `vala.audit_staging` rows before allocating sequence numbers
   (`crates/vala/vala-sql/src/queries/audit_staging.rs:99-148`). That existing
   staging uniqueness correctly absorbs a retry while the first row remains
   staged.
3. The production publisher reads the frozen staging range, and the changed
   projection carries each row's event ID into the canonical retained schema
   (`crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:45-73` and
   `projection.rs:228-266`). Projection also verifies the authenticated tenant
   before constructing the retained batch (`projection.rs:76-135`). No second
   writer, ledger, table, relay, WAL, or publisher entered the range.
4. `settle_publication` advances the watermark and deletes staging through it
   (`crates/vala/vala-sql/src/queries/audit_staging.rs:472-522`). After that
   deletion, the same outbox-owned event ID has no durable append-time fence.
   `OutboxWriter::finish` retries every reported failure indefinitely at the
   tenant queue front (`crates/shared/wyrd-runtime/src/outbox.rs:368-423`). A
   later attempt can therefore allocate another sequence and retained row.
5. The added journey reaches one such retirement race and proves two physical
   retained rows (`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:988-1031`).
   Its `UnknownOutcomeSink`, however, arms only one ambiguous result and makes
   the next attempt succeed (`:821-870`), so it cannot prove the test name and
   AC-009 claim that each decision is retained at most twice.
6. The only logical collapse added by the range is test-harness behavior:
   `WyrdTestServer::retained_audit_records` discards later event IDs in a local
   `HashSet`, and `retained_audit_rows` selects `DISTINCT event_id`
   (`crates/wyrd/wyrd-testing/src/server.rs:1700-1800,1803-1856`). The journey
   explicitly distinguishes two raw rows from one harness decision
   (`audit_publication.rs:1006-1049`).
7. Shipped HTTP and gRPC query services pass the caller's
   `BifrostQueryRequest` to the common Oracle path unchanged; MCP calls the
   same `stream_query`; CLI and the Rust, Python, and TypeScript clients project
   that generic SQL contract. The catalog provider is the raw tenant Iceberg
   provider (`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1523-1557`).
   No retained-audit UI consumer or separate production audit count/list owner
   exists. Consequently ordinary authorized SQL such as `SELECT count(*)` or a
   row listing over `vala.system.audit_log` returns physical delivery attempts,
   not the decision cardinality required by REQ-009.

## Ponytail ladder

- **Delete:** the harness-only collapse cannot be deleted while preserving
  REQ-009. AC-009's physical “at most one extra” bound can be deleted only by an
  approved specification revision; no user outcome in revision 3 requires a
  maximum of two physical rows once every copy remains identifiable and every
  decision-level read collapses it.
- **Reuse repository behavior:** retain the existing staging uniqueness,
  publisher, raw retained rows, tenant binding, and common server query owner.
  Reuse the harness's established representative rule—the earliest sequence
  for an event ID—as the decision projection; do not copy a `HashSet` into each
  HTTP, MCP, CLI, or SDK consumer.
- **Native or installed mechanism:** the existing DataFusion/catalog query
  boundary can own a logical audit decision projection before caller
  aggregation. No dependency, second store, query-language rewrite, or client
  guard is warranted.
- **Minimum correction:** after the physical-multiplicity decision is approved,
  make the production owner of `vala.system.audit_log` reads expose one logical
  decision per tenant/event ID to ordinary count and list SQL, while leaving
  the physical rows available to their internal storage/publication integrity
  owner. Prove the existing production-publisher race through real public HTTP
  and the existing agent-facing MCP path; both listing and `COUNT(*)` must
  return one decision, and an internal assertion must still prove the repeated
  physical delivery rows and tenant association.

This boundary preserves arbitrary SQL for other tables and applies the audit
invariant once below every shipped transport. Rewriting caller SQL, requiring
callers to remember `DISTINCT`, or adding per-client filters would duplicate
policy and still leave sibling consumers wrong.

## Proposal dispositions

| Discovery proposal(s) | Disposition | Independent validation |
|---|---|---|
| `BEH-R3-001`, `INV-R3-002`, `STD-R3-002`, `MAINT-R3-01`, `SYS-R3-001`, `SEC-TEN-R3-001`, `PDATA-R3-001` | **CONFIRMED** | The production publisher creates the permitted duplicate, but only `wyrd-testing` collapses it. HTTP, gRPC, MCP, CLI, and all three SDK projections expose the same raw generic SQL result. Preserve `FIND-AUDIT-OUTBOX-11`; do not allocate a new ID for the same unclosed requirement. |
| `INV-R3-001`, `PDATA-R3-002`, `FOLLOWUP-R3-001` | **REVISED** | The repeated-ambiguity path is reachable through the production `AuditSink`, indefinite retry, publication retirement, and the staging-only append fence. It is not a separate new defect from the finding under remediation; it is a second reason revision 3 does not close `FIND-AUDIT-OUTBOX-11`. The minimum safe resolution is `SPEC_REVISION_REQUIRED`, not another downstream guard. |
| `STD-R3-001`, `SEC-TEN-R3-002` | **REJECTED** | The fingerprint mismatch is mechanically real for a table created from the prior unpublished shape, but checked-in release authority states that no Wyrd image has been published and the next artifact is the first release (`architecture/operations/deployment-and-release.md:209-213`). Revision 3 explicitly approves the retained-schema change, and `b7185d0ee` deliberately removed the unshipped audit compatibility machinery. An upgrade path would add compatibility state for no supported predecessor. This narrow rejection must be revisited if integration supplies the new fact that a pre-release retained catalog must survive into the first supported release. |
| `CONC-R3-001` | **REJECTED** | `FIND-AUDIT-OUTBOX-12` required containment of sink-future construction and polling while the child owns the batch. The candidate does that at `outbox.rs:328-342`, and the production `AuditSink` has no custom panicking destructor, error formatter, or panic payload. The proposal depends on a new malicious or broken generic sink whose teardown/`Display` itself panics; no current production consumer reaches it. Expanding this task into adversarial `Drop`/`Display` containment would not establish a complete generic guarantee because item destruction can also panic. |
| Discovery closure claims for `FIND-AUDIT-OUTBOX-12` | **CONFIRMED CLOSED** | Construction and polling panics return the owned vector through the existing front-of-queue retry/backoff/write-failure path. The focused panic/order test is among the independently reported 7/7 passing outbox tests. |
| Discovery closure claims for `FIND-AUDIT-OUTBOX-13` | **CONFIRMED CLOSED** | `Outbox::shutdown` takes the sole sender under the write lock before awaiting; `stage` sends under the read lock. Pre-fence work drains, post-fence work never enters the queue, and terminal `pending.swap(0)` clears/counts the exact deadline remainder and gauge. |
| Discovery closure claims for `FIND-AUDIT-OUTBOX-14` | **CONFIRMED CLOSED** | The basename-wide `tests.rs` exemption is gone. `CFG_TEST_MODULES` contains only the four verified cfg-test bodies, and identical allowlisted/production fixtures prove the production file is rejected. |
| Discovery closure claims for `FIND-AUDIT-OUTBOX-7` | **CONFIRMED CLOSED** | `MutexGuard` and `Uuid` are imported at module scope and used by bare name in the changed declarations. No wrapper or alias was introduced. |
| Discovery closure claims for `FIND-AUDIT-OUTBOX-3` | **CONFIRMED CLOSED** | The changed security authority now names the shared audit outbox write failure. No targeted “Oracle audit commit failure” owner remains. |

## Final deduplicated finding ledger

### FIND-AUDIT-OUTBOX-11 — revision-3 retained duplication is neither bounded nor collapsed on shipped reads

- **Discovery sources:** `BEH-R3-001`, `INV-R3-001`, `INV-R3-002`,
  `STD-R3-002`, `MAINT-R3-01`, `SYS-R3-001`, `SEC-TEN-R3-001`,
  `PDATA-R3-001`, `PDATA-R3-002`, and `FOLLOWUP-R3-001`
- **Status:** `REVISED`
- **Classification:** `INCORRECT / MISSING`
- **Violated obligation:** revision-3 REQ-003 and REQ-008 require indefinite
  retry; REQ-009 requires decision count/list reads to collapse retained rows
  sharing tenant/event ID; AC-009 requires both that logical collapse and at
  most one extra retained row after retirement.
- **Exact locations:**
  `crates/vala/vala-sql/src/audit_outbox.rs:102-109`;
  `crates/vala/vala-sql/src/queries/audit_staging.rs:113-130,491-522`;
  `crates/shared/wyrd-runtime/src/outbox.rs:391-423`;
  `crates/wyrd/wyrd-testing/src/server.rs:1700-1800,1803-1856`;
  insufficient proof at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-870,909-1054`;
  unchanged production dispatch at
  `crates/wyrd/wyrd-server/src/query/service.rs:185-221` and
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-426`.
- **Evidence:** one commit-success/unknown-result can be published and retired
  before retry, and the journey proves the resulting two physical rows. The
  same interleaving can happen again because every `Err` is retried, retirement
  deletes the only append-time ID fence, and the next attempt receives a new
  sequence and Scribe range identity. Nothing caps this cycle at two rows. The
  journey injects only one ambiguous result. Separately, every shipped audit
  read is generic canonical SQL over the raw provider; only test-harness
  helpers collapse repeated event IDs.
- **Observable consequence:** repeated ordinary database acknowledgement loss
  can retain three or more chain rows for one authorization decision, contrary
  to AC-009. After even one such retirement race, an authorized HTTP, gRPC,
  MCP, CLI, Rust, Python, or TypeScript caller can list or count that single
  decision more than once. The decision remains tenant-attributed and
  detectable, but shipped audit cardinality is wrong.
- **Decision required:** `SPEC_REVISION_REQUIRED`. The Ponytail minimum is to
  change AC-009 from a maximum of two physical rows to ordinary at-least-once
  delivery with any number of detectable copies, retaining stable event ID and
  mandatory logical read collapse. No second identity table, WAL, relay, or
  retirement delay is then needed. If the user instead requires the physical
  maximum, the revision must authorize a durable event-ID lifetime or explicit
  append/publication coordination mechanism; the current prohibition set and
  staging deletion make that bound unrepresentable.
- **Bounded implementation after approval:** use the existing production
  retained-audit query/table owner to collapse tenant/event ID before decision
  listing or aggregation, choosing the earliest sequence as the representative
  just as the existing harness does. Preserve raw rows for the internal
  storage/publication integrity owner. Do not rewrite arbitrary caller SQL or
  add client-side guards.
- **Focused closure proof after approval:** inject at least two consecutive
  commit-success/unknown-result cycles with publication and retirement between
  them. Prove the approved physical behavior and stable event ID; then query
  through authenticated HTTP and MCP using both a row listing and `COUNT(*)`
  and observe one logical decision. Prove a later distinct decision remains in
  order, the internal owner still observes all physical rows, the tenant is
  unchanged, and outbox pending/loss accounting settles correctly.

No new stable finding ID is assigned: both retained defects are revisions of
the same `FIND-AUDIT-OUTBOX-11` being closed in this remediation.

## Prior-finding closure

| Finding | Validated status | Evidence |
|---|---|---|
| `FIND-AUDIT-OUTBOX-11` | **OPEN — REVISED** | Retained identity is correctly carried, but repeated ambiguity exceeds AC-009's physical bound and shipped decision reads do not collapse. |
| `FIND-AUDIT-OUTBOX-12` | **CLOSED** | Reachable production sink construction/poll panics preserve and retry the batch; speculative hostile teardown/formatting expansion is rejected. |
| `FIND-AUDIT-OUTBOX-13` | **CLOSED** | Handle-owned sender removal is the one-way admission fence; deadline residue is counted once and cleared from pending/gauge. |
| `FIND-AUDIT-OUTBOX-14` | **CLOSED** | Narrow cfg-test allowlist and identical-body fixtures replace the basename bypass. |
| `FIND-AUDIT-OUTBOX-7` | **CLOSED** | Changed declarations use top-level imports and bare type names. |
| `FIND-AUDIT-OUTBOX-3` | **CLOSED** | Live security prose names the actual shared audit-outbox owner. |

No additional regression introduced by `cf5ee4128..e54b1244f` was validated.
The retained-schema mismatch is an accepted pre-release replacement under the
current release authority, not a compatibility regression in this immutable
subject.

## Verification limits

- Independently available review evidence records `wyrd-runtime` outbox tests
  7/7 passing, Redux audit projection tests 7/7 passing, checker fixtures
  passing, `check:unwrap-audit` passing with a writable temporary cache,
  `fmt:check` passing, and `git diff --check` passing.
- The Postgres-backed publisher journey could not be rerun in this sandbox
  because Docker access was denied. Its source and the implementer's recorded
  green run are sufficient to prove one ambiguous outcome, two physical rows,
  correct event-ID projection, and harness-only collapse; they do not prove
  the repeated-ambiguity bound or a shipped read surface.
- The docs check rerun was blocked by package-manager registry
  signature/version switching. The implementer's immutable evidence records it
  green, and the targeted authority wording was independently inspected.
- `FIND-5`, `mise run bench:capacity`, and `mise run gate` remain deferred to
  integration by explicit user direction and are not treated as failures.

## Completion

**COMPLETE.** All discovery proposals and corrections were validated against
current source and authority, sibling consumers were traced, contradictions
were resolved, and the final deduplicated ledger contains only
`FIND-AUDIT-OUTBOX-11`. The candidate must not receive a passing closure
verdict until the required persistent-delivery decision is approved and the
shipped decision-read contract is implemented and proved.
