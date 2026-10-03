# Invariant review — audit-outbox r3 closure

## Review findings

### Important

#### INV-R3-001 — repeated unknown commit outcomes can retain more than one extra copy

- **Classification:** `INCORRECT`
- **Prior finding / violated obligation:** `FIND-AUDIT-OUTBOX-11`; approved
  revision 3 `REQ-009` and `AC-009`.
- **Exact locations:**
  `crates/vala/vala-sql/src/audit_outbox.rs:102-109`,
  `crates/shared/wyrd-runtime/src/outbox.rs:391-422`,
  `crates/vala/vala-sql/src/queries/audit_staging.rs:113-132`, and
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:821-870,909-1054`.
- **Evidence:** `AuditSink::write` reports the result of `commit()` directly.
  Any error is returned to the generic outbox, which retains the same
  `StagedAuditEvent` and retries it indefinitely. The append-time event-ID
  lookup sees only live staging. If the first ambiguously successful commit is
  published and retired before retry, the retry allocates a second sequence for
  the same event ID. If that second commit also succeeds but reports an unknown
  outcome and its row is again retired during backoff, the next retry allocates
  a third sequence, and the same interleaving can repeat. Nothing in the writer,
  staging table, or retained table bounds the retained multiplicity at two.
  The added journey cannot falsify this path: `UnknownOutcomeSink` clears
  `lose_next_ack` on the first attempt and expressly makes every later write
  commit normally (`lines 854-869`), so it proves only one ambiguous outcome.
- **Observable consequence:** one authorization decision can occupy three or
  more valid, gap-free retained chain rows, contradicting AC-009's “at most one
  extra retained row” and the test's claimed “at most twice” boundary. Logical
  counts can also grow without bound on any read that does not collapse by
  event ID.
- **Required correction:** the current constraints cannot both guarantee
  unbounded retry/no loss and cap physical retained copies at two after repeated
  ambiguous outcomes: staging retirement deliberately deletes the only durable
  fence, while revision 3 prohibits a second durable identity owner or a
  retirement hold. Authority must either approve the ordinary at-least-once
  consequence (any number of detectable physical copies, with all logical reads
  collapsing them) or permit a durable identity lifetime/coordination mechanism.
  The closure proof must inject at least two successive commit-success/unknown-
  outcome cycles with publication and retirement between them and prove the
  newly approved bound.

#### INV-R3-002 — shipped audit read surfaces expose duplicate decisions

- **Classification:** `MISSING`
- **Prior finding / violated obligation:** `FIND-AUDIT-OUTBOX-11`; approved
  revision 3 `REQ-009` and `AC-009`; the same invariant is live authority in
  `architecture/bifrost-design.md:639-644` and
  `architecture/wyrd-security-posture.md:365-372`.
- **Exact locations:**
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1535-1557`,
  `crates/wyrd/wyrd-server/src/query/routes.rs:241-296`,
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:376-420`,
  `crates/shared/wyrd-client/src/bifrost/facade.rs:1-8,652-693`,
  `crates/wyrd/wyrd-cli/src/query/mod.rs:67-87`, and
  `crates/wyrd/wyrd-testing/src/server.rs:1700-1801,1803-1857`.
- **Evidence:** the authoritative `vala.system.audit_log` provider is the raw
  Iceberg provider; it has no event-ID collapse. `POST /v1/query` accepts an
  arbitrary `BifrostQueryRequest`, the MCP `bifrost.query` tool calls the same
  stream service, the CLI forwards caller SQL unchanged, and the shared
  `wyrd_client::Bifrost` facade (projected by the language SDKs) documents that
  SQL may query any authorized table. The new production-publisher journey
  demonstrates the result directly: its ordinary scheduled query observes two
  raw rows for one event ID at
  `audit_publication.rs:1006-1031`. Only `WyrdTestServer` helpers add
  `SELECT DISTINCT event_id` or discard duplicate event IDs in a local
  `HashSet`; those helpers are test harness, not HTTP, MCP, CLI, SDK, or server
  table behavior. No dedicated shipped audit list/count endpoint or UI audit
  reader was found, but the shipped generic HTTP/MCP/CLI/SDK query surfaces are
  already sufficient to reach the violating count/list path.
- **Observable consequence:** an authorized caller issuing
  `SELECT COUNT(*) ...` or listing rows from `vala.system.audit_log` sees one
  authorization decision more than once after the accepted retirement race.
  Audit results are therefore duplicated on production surfaces even though the
  harness reports one decision.
- **Required correction:** make the production-owned logical audit read path
  collapse `(tenant, event_id)` before count/list semantics reach any shared
  query surface, while retaining the raw physical evidence needed for chain and
  publication integrity behind its owning internal boundary. Reuse the one
  Oracle/query path shared by HTTP, MCP, CLI, and SDK rather than duplicating
  guards in each client. Focused proof must drive the production publisher to
  create duplicate physical rows, then query through public HTTP plus at least
  one independently projected agent/client surface and show both a list and a
  count return one logical decision; a raw internal integrity proof should still
  establish that the retained physical copies and chain rows remain available
  to their owner.

## Immutable subject and scope

- Repository root:
  `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `cf5ee4128ce0b842e00a0eb20770ab5c285dedd8`
- Candidate: `e54b1244f32950d1ab251dae6530c4e1694c78d5`
- Candidate was rechecked at report completion and remained unchanged.
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 3.
- Closure scope: `FIND-AUDIT-OUTBOX-11`, `-12`, `-13`, `-14`, `-7`, `-3`,
  plus regressions introduced by the range. `FIND-5`, `bench:capacity`, and the
  aggregate gate remain integration-deferred by user direction.
- CodeGraph was unavailable because this worktree has no `.codegraph/` index;
  source, callers, tests, and history were inspected directly.

## Navigation and state trace

| State / value | Producer and owner | Sink / sibling consumers | Failure or lifecycle boundary |
|---|---|---|---|
| Generic outbox item and pending count | `Outbox::stage`; handle-owned sender fence and atomics | `OutboxWriter::{dispatch,finish,release}`; sink write | Sink error/panic returns the owned vector to tenant-front retry; shutdown drops the only sender, drains, then abandons and clears terminal pending |
| Audit `event_id` | `StagedAuditEvent::from` | staging append lookup/unique key, `AuditStagingRow`, retained projection | Identity survives ordinary retry while staged; retirement deletes the append-time fence |
| Retained audit row | `AuditPublisher` projects `AuditStagingRow` through `AuditLogTable` | raw Oracle provider; public HTTP, MCP, CLI, SDK SQL readers; test-harness helpers | Publisher retirement permits later retry to allocate another sequence for the same event ID |
| Unwrap audit exclusion | explicit `CFG_TEST_MODULES` paths | repository Rust scan | basename-wide escape removed; each current entry resolves to a real `#[cfg(test)] mod tests;` declaration |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-11` / REQ-009: retained rows carry the staged event ID | `AuditStagingRow.event_id`; publication selects it; `AuditLogTable::EVENT_ID`; `project_record_batch` appends it in canonical order | Reviewed seven focused audit projection tests; independently rerun `tables::audit::` (7/7 passed) | **PASS** |
| `FIND-11` / REQ-009: a retry while the event remains staged consumes no sequence | append locks the chain head, reads same-tenant staged event IDs under RLS, and filters them before sequence allocation | Prior Postgres evidence and the production-publisher journey cover the still-staged interleaving | **PASS** |
| `FIND-11` / AC-009: retirement retry creates at most one extra retained row | No durable event-ID fence remains after retirement; every returned commit error is retried | Journey injects exactly one lost acknowledgement and therefore does not exercise consecutive ambiguous commits | **FAIL — INV-R3-001** |
| `FIND-11` / REQ-009 and AC-009: audit reads that count/list decisions collapse `(tenant,event_id)` | Only `WyrdTestServer::{retained_audit_records,retained_audit_rows}` collapse; raw catalog/provider and shared query surfaces do not | New journey explicitly observes two raw rows and one harness row; no public-surface collapse test exists | **FAIL — INV-R3-002** |
| `FIND-12`: a normal sink construction/poll panic retains and retries the batch ahead of later same-tenant work | `dispatch` catches construction and polling unwind before returning `(tenant, items, result)`; `finish` requeues items at the front and increments write failures | Independently reran all `outbox::` tests; panic/order/loss proof passed (7/7 suite) | **PASS** |
| `FIND-13`: shutdown fences admission at invocation, drains pre-fence items, reports exact deadline loss, and clears pending/gauge | `Outbox.queue` is a handle-owned `RwLock<Option<Sender>>`; shutdown takes/drops sender before awaiting; writer exit precedes one `pending.swap(0)` | Admission race and deadline-loss/pending/gauge tests passed in independently rerun outbox suite | **PASS** |
| `FIND-14`: no blanket `tests.rs` exclusion | `CFG_TEST_MODULES` lists four exact paths and their declaring modules; each declaration was checked and is guarded by `#[cfg(test)]` | Standalone checker fixtures passed; `check:unwrap-audit` passed with a writable temporary cache | **PASS** |
| `FIND-7`: changed declarations use top-level imports and bare names | `MutexGuard` at `outbox.rs:479,548`; `Uuid` at `audit_staging.rs:17` and bare declaration fields | Source inspection; implementer-recorded format/lints | **PASS** |
| `FIND-3`: live security authority names the shared owner | `architecture/wyrd-security-posture.md:424-427` says “audit outbox write failure” | Focused live-authority search found no remaining targeted “Oracle audit commit failure” phrase | **PASS** |
| Retained-schema fingerprint compatibility risk | Revision 3 explicitly approves `event_id` as an expensive-to-reverse retained-schema change. Repository history already removed an audit schema upgrader in `b7185d0ee` under the recorded decision that this schema had not shipped; no release compatibility contract in the reviewed authority requires an upgrade path | Static authority/history review only; no old physical table was upgraded | **PASS within approved pre-ship assumption; deployment state remains an integration limit** |
| No second audit table, ledger, WAL, relay, retirement delay, or publisher | Range carries the ID through the existing staging row, publisher, and retained table | Complete range/name review | **PASS** |
| No lost, misattributed, or cross-tenant decisions introduced by the range | Staging lookup remains RLS-bound through `TenantConn`; retained projection revalidates every row tenant; outbox shutdown/panic paths preserve or explicitly account for items | Focused source and outbox/projection tests | **PASS** |

## Schema compatibility risk judgment

The changed `AuditLogTable::arrow_fields()` necessarily changes both the
catalog user-schema fingerprint and the canonical physical schema. Existing
registered/physical tables with the old 14-column shape would be refused by
`BifrostCatalog::ensure_builtin` because `create_table_locked` rejects a
fingerprint mismatch and validates exact physical shape. The reviewed range has
no migration path.

That is not a new closure finding under the present authority: revision 3
explicitly approves the retained-schema change, and repository history contains
the prior deliberate removal of the only audit-specific schema-evolution path
because this retained schema was treated as unshipped. The acceptance is
therefore conditional on that repository-owned pre-ship fact. If integration
targets any deployment that already materialized the 14-column table, this
becomes a blocking compatibility failure rather than an acceptable limit and
requires an approved schema-evolution decision before rollout.

## Verification notes

- Independently run:
  - `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::/)'` — 7/7 passed.
  - `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::audit::/)'` — 7/7 passed.
  - `mise exec -- python3 scripts/test_check_unwrap_audit.py` — passed.
  - `mise run check:unwrap-audit` — passed after using a writable temporary cache; the first attempt was infrastructure-blocked by a read-only default cache, not a checker failure.
  - `git diff --check` — passed.
- Reviewed but did not independently rerun the Postgres journey. Its source is
  sufficient to establish both the intended one-ambiguity case and the raw
  production-query duplicate; the implementer reports its focused lane and
  `test:bifrost:journey:server` passed.
- `bench:capacity` and `mise run gate` were not run, per the explicit deferral.

## Overall result

**FAIL**

`FIND-AUDIT-OUTBOX-12`, `-13`, `-14`, `-7`, and `-3` are closed. Revision 3
does not close `FIND-AUDIT-OUTBOX-11`: repeated ambiguous commits can exceed
AC-009's physical duplicate bound, and the only logical collapse is in test
harness helpers while shipped HTTP/MCP/CLI/SDK query surfaces expose duplicate
audit decisions. No unrelated range regression was found.
