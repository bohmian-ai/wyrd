# Focused source-query boundary follow-up

Result: **RESOLVED**. This resolves the two assigned source uncertainties; it is not a task verdict or independent finding validation.

Subject: cumulative `ef3a57b07...e4b1e313a`, candidate `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`, unchanged at the final source check. Authority is the user-approved specification revision 13 and `tasks/TASK-001-grouped-verification.md`. The revision-14 draft exception is excluded, as are TASK-002 scheduled inputs and TASK-003 pressure/progress. No source/test changes or tests were made. Only this report was added in the change packet.

## Scope and source coverage

Read the applicable sections of AGENTS, agent rules, spec-driven development, maintainer style, Wyrd design/doctrine, Bifrost design, security posture, the original task and approved requirement text. Inspected the cumulative changed grouped-query/scheduler path and its unchanged authority, query, producer, queue and runner owners. CodeGraph preceded source navigation. Read DC-002, the invariant review's raw-row finding and withheld payload-limit claim, the standards permission/audit PASS, and SEC-001.

The full relevant function bodies were inspected, not only cited lines:

| Producer/owner | Functions and relevant source spans | Consumer/sibling significance |
|---|---|---|
| Group scheduling | `verification/scheduler.rs::poll_groups`, lines 255–311; `verification/authority.rs::BackgroundAuthority::resolve`, lines 239–264 | Produces current accepted owner authority and SYSTEM actor for the grouped poll. |
| Source selection | `verification/groups.rs::GroupPoller::poll`, lines 122–186; `BindingPoll::{candidates,identities,snapshot,payloads,surfaces,query,run}`, lines 217–493 | All grouped discovery, identity and input/outcome payload statements share one query caller; only `snapshot` calls `payloads`. |
| Membership/value owners | `groups.rs::Sources::{new,rows}`, lines 527–598; `Ended::{judge,executions}`, lines 631–673; `Payloads::value`, lines 691–717 | Owner/exact tuples are selected before payload reads; null fallback has no reason distinguishing absence from omitted rows. |
| Queue authority | `wyrd-sql/.../verifier_executions.rs::selected_results`, lines 1699–1720; `refused_submissions`, lines 1730–1745; `enqueue_group`, lines 1764–1814; their complete SQL constants at 250–265 | Refused IDs are removed before membership; selected result IDs protect queued-result authority; snapshot and accepted owner authority are stored separately. |
| Query authority | `verification/authority.rs::SystemReadAuthority::{resolve,table_uid,serving,context,into_context}`, lines 84–200; `components/auth/caller_extractor.rs::Caller::accepted`, lines 74–83; `query/service.rs::{QueryAuthority,oracle_context,stream_query}`, lines 31–227 | Distinguishes manufactured SYSTEM grants from accepted-owner grants, and attribution from permission. |
| Shared query execution | `query/scheduled.rs::ScheduledQueryCaller::{new,authenticated,run_with,dispatch,record_object_denial,consume_to_terminal}`; `vala-bifrost-redux/.../oracle/mod.rs::{query_sql,query_sql_with_deadline,prepare_query_attempt}`, lines 1570–1595 and 1744–1785; `oracle/planner.rs::{read_and_materialize,pin_cut}`, lines 99–208 | Each statement pins its own cut; terminal validation proves that statement's completion, not completeness of an application's larger intended selection. |
| Siblings/producers | `VerificationControl::{enqueue,stage_direct,read_result}`; `ResultPayloadBuilder::inputs`; `verification/hosted/traces.rs::CallerTraces::fetch`; `verification/drift.rs::Reader::authority`; `TenantTokenIssuer::recover`; runner grouped arm and `Caller::accepted`, lines 420–438 | Ordinary input/result production, normal protected readers, Drift's separate exception, and the later grouped ceiling short circuit were traced. |

No additional finding areas were opened.

## 1. Payload bounds and SQL completeness

### Mixed over/under-ceiling tuples: resolved, reachable

The source-execution bound is per resolved tuple/reference, not a bound on the whole binding. `snapshot` constructs each tuple's ordered distinct list, then produces `readable` only from lists whose length is at most the ceiling (`groups.rs:293–308`). An over-limit tuple therefore contributes no ID to `readable`.

That selected set does not constrain either payload SQL statement. The entire statement-generating closure at `groups.rs:376–386` is:

```rust
let select = |columns: &str, table: &str, at: &str| {
    format!(
        "SELECT {columns}, {at} FROM vala.verification.{table} WHERE {} \
         AND group_id = {} AND {at} < {} LIMIT {}",
        self.sources.eligible,
        ObservationWindow::text(group.as_str()),
        ObservationWindow::instant(cutoff),
        ROWS_PER_GROUP + 1
    )
};
```

`Sources::new` builds `eligible` as the owner and OR of **all** declared source tuples, plus non-null group. It contains no readable-ID condition. Both sibling loops call this closure and receive Arrow batches before `readable.contains(&execution)` filters at lines 400 and 434. `run` first collects every returned batch into a `Vec<RecordBatch>`; ignoring an unwanted Variant cell in Rust does not prevent its payload from being queried/transferred/retained in those batches.

A normal, small case proves the gap without a race: ceiling 20, tuple A has 21 eligible executions, tuple B has one, both have input-source slots, and the group is due. Forty-four ordinary input/result identity rows are below the 100000 raw-row check. A's IDs are excluded from `readable`; B's ID makes it nonempty. The input statement still selects all 22 input payload rows. The result-source sibling behaves identically when outcome slots are declared. The eventual runner short circuit correctly produces an inconclusive result, but occurs **after** those reads. It does not enforce the read bound.

REQ-009 bounds how many distinct source executions each reference may read, requires the limit outcome, and forbids eagerly reading unbounded payloads to detect overflow; REQ-006 prohibits materializing all group payloads. TASK-001 scenario 4 explicitly places ceiling detection before payload collection. The code's own snapshot contract also says an over-limit binding reads nothing. Tuple isolation and finality are useful, separate predicates and cannot satisfy this missing selection predicate.

### Payload LIMIT: resolved with an essential qualification

On an unchanged visible row set, the payload LIMIT is **not independently shown to lose rows**: identities read both surfaces and reject more than 100000 raw rows, whereas each payload statement permits 100001 rows from one surface with the additional cutoff. The invariant review correctly withheld a claim that the LIMIT necessarily truncates every accepted snapshot. Transport truncation is also correctly rejected by `ScheduledQueryCaller` and is not this issue.

The allowed visibility-change path does make SQL-limited incompleteness reachable:

1. The identity query sees a small, due group and pins execution E in a below-ceiling list. The group can be due by `max_age` while its finality cutoff remains in the future; alternatively older-timed source rows may simply be published late. Both behaviors are approved in REQ-003–005.
2. Before the payload statement pins its cut, additional ordinary source rows for the same owner/tuple/group become visible, timed before that cutoff. They are absent from `readable` but qualify for the broad payload statement. Source input time is the server execution start (`ResultPayloadBuilder::inputs`, line 353), and staging/publication is separate from the preceding query. No producer-side freeze or group-close barrier exists.
3. Every dispatched statement independently calls `prepare_query_attempt` → planner `pin_cut` → catalog cut read/materialization. Reusing an `AuthorizedQueryContext` does not pin a multi-statement data snapshot. The cache may reuse a cut, but there is no invariant forcing all these statements to see the same cut; the identity two-table and payload single-table statements also have different resolved scan sets.
4. Once the later statement has more than 100001 qualifying payload rows, its unordered LIMIT may return 100001 rows excluding E. SQL specifies no source order here. A physical scan that encounters the newly visible rows first is permitted; no application invariant guarantees E is within the prefix. This requires a changed visibility cut, not a failed stream. A large asynchronously published backlog supplies the rows without increasing the already completed identity query's count.
5. The query can complete successfully with a valid terminal and correct terminal row count for its limited SQL. `payloads` checks neither a sentinel count nor completion of its selected membership. The missing E value falls through `Payloads::value` to null, and `snapshot`/`enqueue_group` pin it as ordinary input. Genuine absent slots/results must remain null; omission by the query cap is a different condition.

This is a source-level reachable interleaving, not an executed reproduction, and it does not assert any particular scan order is inevitable. It establishes that the implementation relies on an ordering/visibility guarantee neither SQL nor the shared owner provides. The ordinary terminal consumer cannot repair a statement deliberately selecting only a prefix.

### Proposed local finding FUP-001 — INCORRECT

**Relationship:** narrows and corroborates the payload portion of DC-002. Keep it consolidated with that proposed source-selection finding rather than inventing a second per-table finding. The already reported raw-identity skip is distinct evidence in DC-002/invariant review and was not independently broadened here.

**Location:** `groups.rs::snapshot`/`payloads`, particularly lines 293–308 and 376–438. **Consequence:** over-ceiling references are read despite their exclusion from the selected membership; changing visibility can let unrelated rows exhaust the SQL prefix and pin a false absence for selected evidence.

**Smallest existing-owner correction:** carry `snapshot`'s existing readable selection into the Bifrost SQL at `BindingPoll::payloads`, consistently for input and result statements, preserving owner/tuple/group/finality predicates. Select queued authoritative results using the existing `selected_results` authority. Bound/deduplicate the selected query result so completion means complete selected coverage; if a retained physical-row cap can cut that selection, return a retryable read failure instead of treating the prefix as complete. Do not add a second query consumer, snapshot service, group lifecycle barrier, or guard in the evaluator. Merely checking a sentinel while continuing to read over-ceiling tuples would leave the small mixed-tuple defect open. Merely adding IDs while retaining an unchecked cap that can still cut physical duplicates would leave that remaining cap unsupported.

**Focused proof:** use one group with two source tuples at 21 and 1 against ceiling 20; verify the explicit inconclusive result and capture/inspect the generated input and outcome SQL/result membership to prove A's payloads are not fetched. At the source query seam, independently exercise a complete successfully terminated SQL prefix that omits a selected payload and prove it cannot enqueue a partial snapshot. A deterministic small fixture at the existing selection seam is sufficient; do not generate host load. Preserve null for a genuinely absent result and authoritative queued-result selection. No extra transport-protocol implementation or broad suite is needed to close this local gap.

## 2. Accepted source-read permission versus SYSTEM attribution

### Conflict resolution

The standards PASS establishes reuse of an existing helper and no new audit writer. It does **not** establish that the selected helper's authority is valid for the new caller. SEC-001's authority distinction is supported by the actual producer-to-consumer chain.

1. `poll_groups` obtains `ExecutionAuthority` from `BackgroundAuthority::resolve`. `TenantTokenIssuer::recover` resolves the owner's current grants without recording new activity or issuing a token. Recovery does not require the owner to hold Bifrost read grants; active identity and that particular downstream permission are distinct. `ExecutionAuthority` carries accepted owner permissions plus SYSTEM as actor.
2. `GroupPoller::poll` discards that owner permission set **for all its reads**. It calls `SystemReadAuthority::resolve` for `ExecutionInputsTable` and `ResultsTable`. That method resolves registered table UIDs and constructs new table-scoped `Permission { resource: BifrostQuery, action: Read, ... }` values, then creates a SYSTEM `Principal` from those grants (`authority.rs:93–129`). It does not intersect them with owner grants.
3. The complete `serving` operation changes only served-subject and delegation history (`authority.rs:178–188`). Its rustdoc explicitly says authorization remains SYSTEM's narrow grants, never the subject's permissions. The `effective_permissions.is_empty()` test at `groups.rs:142` therefore tests those newly constructed grants, not the recovered owner permission set.
4. `ScheduledQueryCaller::new` sets `caller: None`. Its `dispatch` invokes `state.bifrost.query_sql(self.context.clone(), request)` directly. The planner's `authorize_resolved_tables` correctly authorizes the context it receives before object reads; that context is SYSTEM with manufactured grants. Thus this is not a missing Oracle permission check or audit write. It is the wrong principal/permission authority being supplied to a working check.
5. All grouped discovery, identities and sensitive input/outcome statements use that one reader. The selected payload values are then frozen into `GroupSnapshot.inputs`; `enqueue_group` stores them alongside the separately recovered owner `AcceptedAuthority`. Later `Caller::accepted` in the runner cannot undo the earlier protected read. A group with no ceiling overflow proceeds to assertions or a model judge using those values.

The path remains reachable with an active owner whose workload role was revoked and replaced with invocation/model permissions but no query permission, or a query scope excluding verification tables. Security posture explicitly preserves administrator revocation on Card reapply; there is no mandatory inseparable workload role that makes missing Bifrost grants impossible. Exact owner/source tuple predicates prevent mixing another owner but grant no table/payload permission.

### Governing authority and siblings

The correct spec identifier is **REQ-011**, `spec.md:524–528`, not SEC-001's REQ-010 label: existing authorization applies to every evidence read; a reference cannot widen accepted execution permission to another table/payload; sensitive inputs/results need existing permissions. INV-003 preserves tenant/permission boundaries. Wyrd design's execution-identity section says SYSTEM authorizes nothing extra. Security posture lines 110–137 and 212–219 expressly permits the tokenless table-read exception for scheduled Drift's `vala.drift.observations`; it does not permit a new verification-payload exception. These authorities agree; no new security decision is necessary to correct the implementation.

All production `SystemReadAuthority::resolve` callers were enumerated: grouped polling and Drift's `Reader::authority`. Drift passes only `vala.drift.observations` and keeps its approved exception. Its behavior must remain intact. The ordinary result reader (`VerificationControl::read_result`) and trace reader (`CallerTraces::fetch`) already use `ScheduledQueryCaller::authenticated`. That path derives `oracle_context` from the accepted caller, admits the query capability, authorizes the actual resolved tables, and records denial through existing non-blocking audit. `Caller::accepted` already retains owner grants and delegation while assigning SYSTEM solely as executor. Hosted check-tool invocation is another sibling using accepted authority; tool permission is an additional boundary and need not be added to this server-owned source read.

### Proposed local finding FUP-002 — VIOLATION

**Relationship:** corroborates SEC-001 with the corrected REQ-011 identifier; consolidate with SEC-001 in validation. **Location:** `GroupPoller::poll`, `groups.rs:130–150`. **Consequence:** protected recorded inputs/outcomes reach a group snapshot and judge despite the accepted owner's missing or excluding read grant. Audit of SYSTEM's allowed read does not express the denial required for that owner.

**Smallest correction:** in this grouped owner only, reuse `Caller::accepted` from the recovered `ExecutionAuthority` with the existing authenticated `ScheduledQueryCaller`. All discovery/identity/payload statements then share the accepted owner permission boundary and ordinary audits. Remove this caller's manufactured SYSTEM authority; preserve Drift's exception, read settlement, existing cursor retry, tenant/tuple isolation, producer no-IO attribution and queue fencing. Do not add new permissions, broad grants, a role restoration on registration, a new audit writer, or an extra permission-only check followed by the same privileged read.

**Focused proof:** keep the owner runtime-active with invocation/model permissions; omit Bifrost capability and separately grant a scope excluding a verification source table. A due group must not deliver its protected payload into a snapshot/judge; inspect the ordinary denial audit. Grant the required existing source-table permission and show a later poll succeeds. Cover input and outcome sources through the same owner and preserve the existing Drift authority test. Existing live-span tool denial tests cover a later, different consumer and cannot establish this source-read boundary.

## Evidence limit

This pass supplies source evidence and reachable states, not final `CONFIRMED`/`REVISED` decisions. The independent validator must reconcile these local proposals with DC-002 and SEC-001. Root-reported green journeys and MCP/OpenAPI cases do not exercise the small mixed-tuple SQL predicate, accepted-owner source-read denial, or the legal changed-cut SQL-prefix case; no claim is made that transport terminal tests fail.
