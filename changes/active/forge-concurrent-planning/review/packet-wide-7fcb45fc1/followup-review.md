# Focused follow-up review — `7fcb45fc1`

## Subject and scope

- Immutable candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Approved specification: `changes/active/forge-concurrent-planning/spec.md`, revision 11

The checkout was later than the candidate, so production and packet source was
read with commit-scoped `git show`, `git grep`, and `git diff`. This pass resolves
only the conflicts assigned by the orchestrator; it does not vote on the rest
of the discovery ledger.

## Resolution 1 — late object deletion after the lease bound

**Resolved: the expired-cleanup and orphan-GC timeout paths are safe under the
candidate's approved object-delete exception; reject the delete portion of
`BEH-REREVIEW-001` and `CONC-RR-002`.**

Inspected paths:

- `architecture/bifrost-design.md:861-880`
- `forge/worker.rs:7936-8299`
- `forge/orphan_gc.rs:155-256,579-624,1001-1150,1340-1591`
- `forge/protection_roots.rs`
- `tests/integration/forge/{expired_cleanup,orphan_cleanup}.rs`

Candidate commit `dc7a51eb2` records the intentional exception precisely:
exclusive authority is bounded by the Forge lease TTL, and an object-store call
still running at that bound becomes uncertain evidence for idempotent replay.
That is narrower than permission to release arbitrary destructive work.

Both object-delete paths prove the candidate unreachable while exclusive
authority excludes older cuts. Expired cleanup reloads current catalog and
durable roots per candidate and exempts only its exact prepared identity;
orphan GC considers only immutable attempt-generation paths, reloads catalog
and durable roots, and fences the producer lease. A newly admitted cut selects
the then-current pointer/hot rows and therefore cannot acquire either proven-
unreachable object. Forge-generated object identities are not repurposed into
later catalog state. A late backend completion can consequently delete only an
object the new cut cannot name. The retained Prepared/open evidence prevents a
second cleanup owner from treating the unknown result as complete.

The timeout tests correctly prove authority surrender and retained evidence.
They do not emulate late backend completion, but that omission does not create
the alleged reader race because source reachability makes the object absent
from every possible new cut before submission. Requiring object deletion to
retain the row lock until a hung backend eventually answers would directly
violate the approved lease-TTL availability bound.

## Resolution 2 — snapshot-expiry scope and unknown catalog acceptance

**Resolved: the complete-scope TTL bound is an approved obligation, not
speculative hardening; revise and retain prior `FIND-TASK-005-R1-3` for snapshot
expiry only.**

Inspected paths:

- `architecture/bifrost-design.md:861-880`
- `spec.md:291-327,390-402,418-451`
- prior `TASK-005-R2-active-read-lifetime-and-authority.md:61-71,89-107`
- `forge/expire.rs:240-337,711-802,979-1137`
- `vala-sql/migrations/20260910000025_oracle_reader_authority.sql:197-326`

The architecture says both that every destructive authority hold is bounded by
the Forge lease TTL and that object-store calls have the explicit uncertain-
effect exception. Snapshot expiry is different: its destructive effect changes
the catalog pointer that the next Oracle cut selects. The candidate acquires
exclusive authority and then performs unbounded SQL preparation/commit waits,
catalog reload, catalog commit handling, and authority-transaction commit; no
single deadline bounds that full scope. `complete_expiry` also explicitly
returns unknown acceptance when its catalog commit times out or is cancelled,
after which the caller releases authority. Oracle acquisition checks the
authority row but not the unresolved snapshot-expiry operation. A reader can
therefore select the pre-expiry pointer after authority release while the
already-submitted catalog commit can still land. Unlike an unreachable object,
that late effect changes the exact pointer the new cut names.

`DATA-DUR-REREVIEW-001` is therefore source-true, but its proposed TTL wrapper
alone is incomplete: reaching the bound after catalog submission must not admit
a cut while catalog acceptance is unknown. The smallest safe correction remains
at the existing snapshot-expiry/Oracle authority boundary: bound the complete
exclusive scope by one lease-derived deadline; rollback immediately when no
catalog effect was submitted; and, when submission is acceptance-unknown, use
the existing Prepared snapshot-expiry identity to refuse conflicting cut
acquisition until reconciliation establishes the stable pointer. This is not a
second maintenance protocol or a downstream reader recheck; it is the existing
one-statement cut authority refusing while its conflicting destructive effect
has no known order. Proof must pause after catalog submission, cross the bound,
race a cut, and show no cut commits until reconciliation chooses the old or new
pointer.

### Proposed finding `FUP-RR-001` — prior `FIND-TASK-005-R1-3` remains open for snapshot expiry

- **Classification:** INCORRECT.
- **Location:** `forge/expire.rs:267-323,711-802,1097-1137`; Oracle authority
  migration `:197-326`.
- **Consequence:** a new reader can commit the old cut before an uncertain late
  snapshot-expiry commit changes its catalog pointer, or an unbounded pre/post
  commit await can retain exclusive authority past the Forge lease TTL.
- **Correction:** apply the single bounded authority lifetime and unresolved-
  catalog-effect refusal described above; do not change the safe object-delete
  exception.

## Resolution 3 — leader check/use and scheduler unwind

**Resolved: both paths are reachable, neither is prevented by heartbeat or
server supervision, and both reopen prior `FIND-TASK-001-1` at the same local
held-term owner.**

Inspected paths:

- `forge/leadership.rs:168-353,355-473`
- `forge/scheduler.rs:134-205,506-530`
- `wyrd-server/src/app/supervise.rs:71-181`
- `wyrd-server/src/app/server.rs:626-660`

`accept`, `serve_pull`, and `serve_report` clone an `Arc<ForgeHeldTerm>` under
the slot lock, release the lock, and then touch its schedule. `set_held` can
replace and revoke that term in between. The schedule methods are synchronous,
but that does not make the race impossible; a deterministic pause after
selection reaches it. In particular, a pull can return a dispatch from a term
that has already been locally revoked.

Separately, `Forge::run` resigns only after a normal `tokio::join!` return.
When the spawned scheduler instance panics or its future is dropped, its
`ForgeRunGuard` clears only `running`; `restarting_worker` catches the join
failure and backs off without cancelling the process token. During backoff, and
especially across repeated reconstruction failures past the 30-second database
term, the cached local term remains routable. A successor can own PostgreSQL
while the old pod still accepts peer handlers. A later successful heartbeat
eventually revokes/refences it, but does not protect the intervening window.

One root correction belongs at `ForgeLeadership`: handler validation plus each
synchronous schedule operation must be one scoped use of the held slot, while
the scheduler RAII owner must synchronously revoke/clear that slot on every
exit or unwind before restart backoff. Best-effort asynchronous SQL resignation
can remain on normal shutdown. These are two lifecycle edges of the same held-
term owner and should remain one reopened stable finding, not two remediations.

### Proposed finding `FUP-RR-002` — prior `FIND-TASK-001-1` remains open

- **Classification:** INCORRECT / REGRESSION.
- **Location:** `forge/leadership.rs:205-247,364-473` and
  `forge/scheduler.rs:170-205,506-530`.
- **Consequence:** a revoked leader can emit one later schedule effect, and a
  failed scheduler can expose its cached term throughout restart backoff after
  PostgreSQL has elected a successor.
- **Correction proof:** deterministically interleave revocation with all three
  handlers, and force scheduler unwind after acquisition; during restart
  backoff the old pod must refuse all handlers while a standby can acquire.

## Resolution 4 — in-range unrelated commit

**Resolved: `406464a59` is packet drift and blocks PASS; the caller's
out-of-scope label cannot exclude a path already inside the immutable range.**

`git log e8d3cca13..7fcb45fc1` places `406464a59` inside the candidate, and that
commit adds only `changes/active/bifrost-variant/spec.md`, a separate 686-line
product specification with no Forge packet requirement or remediation
consumer. The task-review skill requires the complete base-to-candidate diff
and permits PASS only when no unrelated change entered it. Commit `5ab92b003`
is genuinely outside the candidate and remains excluded; `406464a59` is not.

### Proposed finding `FUP-RR-003` — unrelated specification inside candidate

- **Classification:** DRIFT.
- **Location:** `changes/active/bifrost-variant/spec.md:1-686`.
- **Correction:** produce a candidate whose cumulative range omits
  `406464a59`; no Forge code change is needed.

## Resolution 5 — narrow unique proposals

### Fork-only planning methods — rejected

`FORK-REV-001` is not an approved-obligation failure. TASK-002-R1 requires each
retained fork-only **module/item inventory row** to name a production consumer
and removal-sensitive proof. The retained `managed/boundary.rs`
`NonCommittingCompaction` is the production noncommitting rewrite seam and has
both. Its unused convenience planning methods are bounded dead API in the
dependency, but neither revision-11 REQ-005/010 nor the remediation acceptance
criterion requires pruning every uncalled method from a retained, justified
module. Further fork slimming is optional and cannot block this acceptance
audit.

### Stale Analytical-supervisor lifetime contract — confirmed

`oracle/analytical_supervisor.rs:1-9` says leader-stream ending implies no
cleanup and that the supervisor retains every driver and releases the graph.
The same candidate deletes its supervisor-owned lifecycle and makes
`LeaderStreamOwners`/`AnalyticalGraphLifecycle` the hard stream-owned lifetime.
The module was materially edited for that ownership change, so leaving the
load-bearing contract stated backwards violates the repository's substantive
rustdoc rule and invites reintroduction of the exact prior defect.

#### Proposed finding `FUP-RR-004`

- **Classification:** DRIFT / VIOLATION.
- **Correction:** replace only the stale module ownership paragraph with the
  actual stream-owned lifetime and the narrow capacity-residue exception.

### Task authority metadata — confirmed

`tasks/README.md:3,14` still calls revision 10 the approved and only authority,
while `spec.md:1-5` is approved revision 11; TASK-004's header omits the
`spec_revision` required by
`architecture/references/languages/spec-driven-development.md:134-168`.
Revision 11 materially changed this packet, so this is ambiguous authority, not
cosmetic staleness.

#### Proposed finding `FUP-RR-005`

- **Classification:** VIOLATION.
- **Correction:** name revision 11 as current authority in the index and add
  TASK-004's historical derivation revision (`spec_revision: 6`); preserve the
  other task derivation revisions.

### Free boxed restart lifecycle — confirmed

`wyrd-server/src/app/supervise.rs:78-181` makes mutable worker instance,
builder, observer, cancellation, identity, and exponential backoff one generic
free orchestration function returning `Pin<Box<dyn Future...>>`. Its three real
callers earn the shared mechanism, but they do not waive the hard
struct-centered rule: this is stateful multi-step lifecycle ownership, not a
stateless helper, and there is no runtime-pluggable future implementation.

#### Proposed finding `FUP-RR-006`

- **Classification:** VIOLATION.
- **Correction:** put the same state and loop on one focused concrete restart
  owner with an eager fallible constructor and async run method; preserve the
  three call sites and behavior without adding a trait or configuration layer.

## Final follow-up status

**RESOLVED.** All assigned conflicts were resolved from immutable source and
applicable authority. The object-delete timeout allegation and dead-fork-API
proposal should be rejected. Prior `FIND-TASK-001-1` and
`FIND-TASK-005-R1-3` remain open at the narrowed boundaries above, and
`FUP-RR-003` through `FUP-RR-006` are independently proposed for structured
Ponytail validation.
