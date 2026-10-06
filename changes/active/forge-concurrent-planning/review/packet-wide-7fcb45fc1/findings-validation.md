# Structured Ponytail findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original base, excluded: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base, excluded from the remediation delta:
  `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Cumulative range: `c1508b375..7fcb45fc1`
- Remediation range: `e8d3cca13..7fcb45fc1`
- Authority used to validate the immutable candidate:
  `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Owner-approved remediation authority: the same specification, revision 12
- RisingWave reference: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The checkout was later than the candidate. Production source, packet source,
and diffs were therefore inspected with commit-scoped Git reads. The candidate
object remained available and unchanged throughout validation.

## Validation method

I read the complete task-review skill, navigation map, revision-11 specification
and revision history, original task packet, prior verdict and finding ledger,
all seven remediation tasks, applicable repository authorities, cumulative and
remediation diffs, every discovery report, and the focused follow-up. Proposed
findings were then checked against the candidate source, their callers and
sibling consumers rather than accepted by reviewer agreement.

The owner-reconciled result is five retained findings. Four reuse prior stable
IDs because the remediation did not fully close the same obligation; one new
packet finding takes the next unused packet ID. The owner subsequently approved
revision 12, resolving the sole persistent-coordination decision and making all
five corrections implementation-ready.

## Proposal disposition

| Proposal | Disposition | Source-backed resolution |
|---|---|---|
| `INV-REREVIEW-001`, `CONC-RR-001`, `SYS-001`, `FUP-RR-002` | **REVISED and consolidated into `FIND-TASK-001-1`** | Handler use is not linearized with `set_held`, and scheduler unwind leaves the same locally held term routable during restart backoff; both are lifecycle edges of `ForgeLeadership`. |
| `FORK-REV-001` | **CONFIRMED as revised `FIND-TASK-002-2`** | The follow-up rejection is contradicted by the pinned fork: four public planning/accessor methods and their retained `context` field have only fork-test callers, while Wyrd uses only `new` and `rewrite`. This directly falsifies the prior comparison's consumer-complete claim and REQ-005/010 deletion obligation. |
| `BEH-REREVIEW-001`, `CONC-RR-002` | **REJECTED for expired-object and orphan deletion; REVISED for snapshot expiry as `FIND-TASK-005-R1-3`** | The approved architecture expressly permits an object-store operation still running at the lease bound to become uncertain after the object was proved unreachable and non-reusable. A late snapshot-expiry catalog commit is different because it changes the pointer a newly admitted cut names. |
| `DATA-DUR-REREVIEW-001`, `FUP-RR-001` | **REVISED into `FIND-TASK-005-R1-3`** | Snapshot expiry neither bounds its entire authority scope by the Forge lease TTL nor preserves ordering when catalog acceptance is unknown. A TTL wrapper alone would release the lock into the same unsafe pointer race. |
| `MNT-001`, `FUP-RR-004` | **REVISED into prior `FIND-PACKET-1`** | The packet-wide substantive-rustdoc audit missed module documentation that states the repaired Analytical ownership invariant backwards. |
| `BEH-REREVIEW-002`, `STD-RR-001`, `FUP-RR-003` | **REJECTED by owner scope decision** | The owner explicitly permits `406464a59` in this candidate, so its separate Bifrost-variant specification is not blocking drift. |
| `STD-RR-002`, `FUP-RR-005` | **CONFIRMED as `FIND-PACKET-6`** | The task index names superseded revision 10 as the only authority and TASK-004 omits mandatory `spec_revision`. |
| `STD-RR-003`, `FUP-RR-006` | **REJECTED after owner rule correction** | Repository authority now permits a free function to coordinate multi-step async work when its state and lifetime remain caller-owned for that invocation; `restarting_worker` does not by itself earn a retained owner. |
| Tenancy/security review | **VALIDATED EMPTY** | Tenant RLS, the narrow pointer definer, catalog grants, typed operator access, private peer authentication, and exact table authority remain correctly scoped. |
| SDK/contracts review | **VALIDATED EMPTY** | Compaction wire parity, SDK projection, Python runtime/stub parity, and callback outcomes are source-consistent. |
| Deleted Python Args-format test | **REJECTED as a finding** | It enforced duplicated signature types that TASK-006 expressly removes; `py:typecheck` and the remaining parity tests own the actual property. |
| First-refusal `capacity_refused` classification | **REJECTED as a finding** | No approved requirement creates a distinct durable or metric class; adding one would require a migration for an optional distinction. |

## Final deduplicated finding ledger

### FIND-TASK-001-1 — REVISED — INCORRECT / REGRESSION

- **Discovery sources:** `INV-REREVIEW-001`, `CONC-RR-001`, `SYS-001`,
  `FUP-RR-002`.
- **Violated obligation:** REQ-001 and INV-001 require one leader term and make
  loss of that term stop dispatch and leader maintenance. The new in-pod restart
  behavior may isolate Forge failure from the server only after ending the
  failed scheduler's authority.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:205-247,364-473`;
  `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:170-205,506-530`;
  `crates/wyrd/wyrd-server/src/app/supervise.rs:103-181`.
- **Evidence:** `term()`/`held()` clones an `Arc<ForgeHeldTerm>` and releases
  the slot read lock before `accept`, `serve_pull`, or `serve_report` touches
  its schedule. `set_held` can take the write lock, replace and revoke that
  exact term, after which the already-returned clone can still mutate or emit a
  dispatch. Separately, `Forge::run` calls `leadership.resign()` only after its
  `tokio::join!` returns normally. On panic or dropped future,
  `ForgeRunGuard::drop` clears only `running`; `restarting_worker` catches the
  join failure while the process cancellation token remains live, so the cached
  term stays routable during backoff and can overlap a PostgreSQL successor.
- **Observable consequence:** a revoked or failed leader can produce a later
  schedule effect, including a stale dispatch, while another replica owns the
  durable term.
- **Decision-complete minimal correction:** keep the correction on the existing
  `ForgeLeadership` owner. Make validation and each synchronous schedule
  operation one slot-guarded use so replacement/revocation cannot interleave;
  make the scheduler's existing RAII lifetime synchronously clear and revoke
  that same local slot on every exit or unwind before restart backoff. Preserve
  normal best-effort asynchronous SQL resignation. Do not add another lease,
  downstream cancellation checks, or a second scheduler owner.
- **Focused closure proof:** deterministically pause each handler after term
  selection, race replacement/revocation, and prove the operation either
  linearized before revocation or refuses with no later schedule effect. Force
  the real scheduler to fail or panic after acquisition and prove during restart
  backoff that the old pod refuses notify/pull/report, a standby acquires, and a
  rebuilt scheduler later contends normally.

### FIND-TASK-002-2 — REVISED — DRIFT

- **Discovery source:** `FORK-REV-001`; the follow-up rejected it without
  applying REQ-005/010 to the retained item's unused public half.
- **Violated obligation:** REQ-005 and REQ-010 require fork-only machinery with
  no proven production consumer to be deleted. The remediation's own comparison
  claims every retained fork-only item has a production consumer and
  removal-sensitive proof.
- **Exact location:** pinned fork `380a4d0717e1786b95c4aa9f257579af496b3c8c`,
  `core/src/managed/boundary.rs:38-41,75-110` and
  `core/src/compaction/mod.rs:475-498`; candidate evidence
  `changes/active/forge-concurrent-planning/tasks/TASK-002-pull-and-worker-results.md:400-418`.
- **Evidence:** Wyrd production calls only `NonCommittingCompaction::new` and
  `rewrite` from `forge/managed/executor.rs`. Fork-wide caller inspection finds
  `attempt_id`, `context`, `load_table`, and `plan_with_report` only in the
  boundary's tests; the retained `context` field exists only for those accessors
  and `Debug`. `Compaction::plan_compaction_with_report` is called only by the
  unused boundary method and fork tests. Wyrd's real planning path calls
  `CompactionPlanner::plan_compaction_with_report` directly from
  `forge/managed/policy.rs`.
- **Observable consequence:** the narrowed fork still ships a second unused
  planning entry point and redundant attempt state while the evidence says the
  retained surface is consumer-complete.
- **Decision-complete minimal correction:** narrow the existing fork boundary
  to the production-used noncommitting constructor and rewrite operation;
  remove its unused accessors/load/planning method, redundant stored context,
  and the now test-only `Compaction::plan_compaction_with_report`. Preserve
  `CompactionPlanner::plan_compaction_with_report`, injected governed execution,
  cancellation/drain, selection reports, and loose-output protection. Re-pin
  Wyrd to that immutable fork commit; add no replacement abstraction.
- **Focused closure proof:** fork and Wyrd source scans show no removed symbol;
  the existing noncommit, selection-report, governor, spill, cancellation, and
  loose-output tests pass, followed by the required exact comparison inventory.

### FIND-TASK-005-R1-3 — REVISED — INCORRECT

- **Discovery sources:** `BEH-REREVIEW-001` (snapshot portion),
  `DATA-DUR-REREVIEW-001`, `FUP-RR-001`. The object-delete portion of
  `BEH-REREVIEW-001` and `CONC-RR-002` is rejected.
- **Violated obligation:** REQ-007/014, INV-005/006/009, AC-006/009, the R2
  authority acceptance criterion, and `architecture/bifrost-design.md:875-880`
  require one ordering between a cut and snapshot expiry, require exclusive
  authority through the destructive effect's known outcome, and require the
  Forge lease TTL to bound the hold. The architecture grants the uncertain
  late-effect exception only to object-store calls.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/expire.rs:267-323,711-802,994-1137`;
  Oracle acquisition in
  `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:197-326`.
- **Evidence:** snapshot expiry acquires `ExclusiveTableAuthority` and then
  awaits SQL preparation/fencing, catalog loading, commit, traversal, and the
  authority transaction commit without one lease-derived deadline over the
  complete scope. `complete_expiry` returns reconciliation-required when its
  catalog commit times out or cancellation wins, explicitly because acceptance
  is unknown; its caller then drops `exclusive` and commits the authority
  transaction. Oracle acquisition waits only on the authority row and does not
  account for the open prepared expiry. A newly admitted reader can therefore
  select the old pointer after lock release while the submitted catalog commit
  may still replace it. Unlike a proven-unreachable object, the late effect
  changes the pointer that reader names.
- **Observable consequence:** the current path can either block all new cuts
  past the approved lease bound or release at/before that bound and permit a cut
  whose pointer is invalidated by an acceptance-unknown expiry commit.
- **Approved correction:** revision 12 reuses the existing prepared
  snapshot-expiry identity as a cut barrier. When catalog acceptance remains
  unknown at the lease bound, Forge retains the prepared claim and releases
  exclusive authority; the existing one-statement Oracle acquisition refuses a
  cut for that table until reconciliation establishes the stable old or new
  pointer and removes the claim. This adds no second protocol or state owner.
- **Required proof:** pause after catalog submission, cross the lease bound,
  race cut acquisition, and prove the stable visibility error is returned while
  the prepared claim remains. Reconcile both possible catalog outcomes and
  prove a later cut commits with the established pointer. Preserve the safe
  object-delete TTL behavior unchanged.

### FIND-PACKET-1 — REVISED — VIOLATION

- **Discovery sources:** `MNT-001`, `FUP-RR-004`.
- **Violated obligation:** the prior packet-wide finding and repository rustdoc
  rule require every materially changed ownership contract to describe the
  actual invariant substantively.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:1-24`,
  contradicted by `oracle/analytical.rs:2279-2295,2806-2839` and
  `oracle/query_stream.rs:530-561`.
- **Evidence:** the module says a leader stream ending implies no cleanup and
  that the supervisor retains every driver and releases the graph. The candidate
  deliberately removes supervisor-owned lifecycle and makes the leader stream
  own `AnalyticalGraphLifecycle`, synchronously revoke grants/followers, close
  exchanges, and abort local drivers before claim release.
- **Observable consequence:** the load-bearing ownership rule is documented
  backwards at the module entry point, inviting the exact detached-lifetime
  defect R2 removed.
- **Decision-complete minimal correction:** replace only the stale ownership
  paragraph with the implemented contract: the supervisor indexes/routes state
  and may retain failed capacity residue, while the leader stream solely owns
  graph execution lifetime; only envelope reclamation and already consumerless
  remote IO may continue after revocation. No code or new abstraction is needed.
- **Focused closure proof:** a documentation/source audit finds no statement of
  supervisor-owned query lifetime and the changed-symbol documentation audit,
  format, and lints pass.

### FIND-PACKET-6 — CONFIRMED — VIOLATION

- **Discovery sources:** `STD-RR-002`, `FUP-RR-005`.
- **Violated obligation:** spec-driven development requires each task to name
  its derivation revision and active packet navigation to identify the current
  approved authority unambiguously.
- **Exact location:**
  `changes/active/forge-concurrent-planning/tasks/README.md:3,14` and
  `tasks/TASK-004-compaction-defaults-and-type.md:1-7`.
- **Evidence:** the index calls revision 10 approved and the only Forge/Oracle
  authority while `spec.md` is approved revision 12 and revisions 10 through
  12 materially changed the packet. TASK-004 alone omits `spec_revision`; its
  creation history shows derivation from revision 6.
- **Observable consequence:** a fresh implementer or reviewer is directed to
  superseded authority and cannot establish TASK-004's approved derivation.
- **Decision-complete minimal correction:** make the index name revision 12 as
  current authority, add `spec_revision: 6` to TASK-004, and preserve every
  other task's historical derivation revision.
- **Focused closure proof:** scan every implementation/remediation header and
  the index for current authority, derivation revision, and `status: review`.

## Prior-finding closure

| Stable finding | Validation result | Source-backed conclusion |
|---|---|---|
| `FIND-TASK-001-1` | **OPEN / REVISED** | Ordinary renewal and cancellation improved, but handler check/use and scheduler unwind still expose the old term. |
| `FIND-TASK-002-1` | CLOSED | Post-insert shutdown uses the guarded dispatched close, leaves no fair-claimable row, and emits `NotStarted` only after the exact close wins. |
| `FIND-TASK-002-2` | **OPEN / REVISED** | Comparison rows are populated and most dead fork surface was deleted, but the retained boundary still contains unconsumed fork-only planning API and state. |
| `FIND-TASK-003-1` | CLOSED | `CleanupRetained` bypasses generic retry/terminal settlement and replays the same prepared task, attempt, and candidate after refusal or uncertainty. |
| `FIND-TASK-004-1` | CLOSED | The canonical enum is kebab-case, omission is `small-files`, old underscore spellings are rejected, and generated/client projections agree. |
| `FIND-TASK-004-2` | CLOSED | `wyrd_client::bifrost` re-exports the canonical enum and the thin Rust SDK names it without an implementation-layer dependency. |
| `FIND-TASK-005-R1-1` | CLOSED | `LeaderStreamOwners` drops inline Analytical lifecycle ownership before the claim; grants/followers are revoked and local drivers aborted synchronously under the approved consumerless-remote-IO decision. |
| `FIND-TASK-005-R1-2` | CLOSED | Initial acquisition and the bounded retry derive remaining time from one immutable deadline immediately before PostgreSQL stamps the row. |
| `FIND-TASK-005-R1-3` | **OPEN / REVISED** | The owned capability closes the ordinary check/effect gap and the object-delete TTL exception is safe, but snapshot expiry cannot satisfy both known-outcome ordering and the TTL with the current protocol. |
| `FIND-TASK-005-R1-4` | CLOSED | The stale tenant-wide sibling-maintenance prohibition is gone while exact orphan evidence remains. |
| `FIND-TASK-005-R1-5` | CLOSED | Production table-layout SQL is owned by `vala-sql`; Redux consumes the typed owner. |
| `FIND-TASK-006-1` | CLOSED | Runtime, hand-authored sources, generated stubs, exports, and top-level parity tests agree for the recorded inventory. |
| `FIND-TASK-006-2` | CLOSED | `after_model` and `after_agent` end as `CallbackAborted`; `after_tool` fails only that call and continues; former panic arms are gone. |
| `FIND-PACKET-1` | **OPEN / REVISED** | The broad doc/import sweep closed the named omissions, but one materially changed module still states the central ownership invariant backwards. |
| `FIND-PACKET-2` | CLOSED | Changed-range inspection found no prohibited local import or unjustified qualified signature beyond the documented narrow exceptions. |
| `FIND-PACKET-3` | CLOSED | `git diff --check c1508b375..7fcb45fc1` exits zero. |
| `FIND-PACKET-4` | CLOSED | The release benchmark passed 16/16 and `mise run gate` passed on `30d31ebac`; later Forge changes are evidence-only. |

## Rejected or narrowed claims

- Expired cleanup and orphan GC may surrender authority at the lease bound with
  an uncertain object-store result because the approved architecture names that
  exact exception, the object was freshly proved absent from every cut/root,
  identities are not repurposed, and the prepared owner prevents false
  settlement. Requiring an indefinitely held row lock would violate the same
  approved reader-availability bound.
- Remote Analytical IO may finish after leader-stream drop only after its grant
  and consumer have been revoked, per the recorded human decision. The retained
  `AnalyticalGraphRelease` owns capacity residue, not query lifetime or a claim.
- The retired Python Args-format test protected no approved behavior; restoring
  74 duplicated prose types would contradict TASK-006 rather than improve proof.
- No new tenant-isolation, authn/authz, secret, injection, host-clock,
  compatibility-alias, reader-epoch/frontier/IO-gate, or SDK-contract finding
  was source-confirmed.

## Final validation result

**FIX_REQUIRED.** The ledger is non-empty. Revision 12 approves the existing
prepared snapshot-expiry claim as the cut barrier, so all five findings now
have bounded, decision-complete corrections in
`TASK-PACKET-R2-close-remaining-findings.md`.
