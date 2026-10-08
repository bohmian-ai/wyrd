# Packet-wide task re-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original base, excluded: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Prior reviewed candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Remediation base, excluded from the remediation delta:
  `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Cumulative range: `c1508b375..7fcb45fc1`
- Remediation range: `e8d3cca13..7fcb45fc1`
- Authority used to review the immutable candidate:
  `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Owner-approved remediation authority: the same specification, revision 12
- Pinned RisingWave reference:
  `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The checked-out branch had advanced beyond the candidate before review began,
so every source conclusion used commit-scoped Git reads. The candidate object
remained unchanged and available throughout the review.

## Verdict

**FIX_REQUIRED**

The owner approved revision 12's minimal resolution for
`FIND-TASK-005-R1-3`: an unresolved prepared snapshot-expiry commit blocks new
Oracle cuts for that table until reconciliation establishes the stable old or
new pointer. The five retained findings now have decision-complete corrections
inside existing owners and are assigned together in
`TASK-PACKET-R2-close-remaining-findings.md`.

## Independent review results

| Review | Result | Material output |
|---|---|---|
| Behavior | FAIL | Reopened destructive-authority ordering and found in-range scope drift |
| Invariants | FAIL | Reopened leader handler revocation race |
| Repository standards | FAIL | Scope, packet metadata, and restart-owner findings |
| Maintainer | FAIL | Stale Analytical lifetime rustdoc |
| System resilience | FAIL | Scheduler unwind leaves a routable local term during restart backoff |
| Concurrency domain | FAIL | Leader check/use and destructive timeout findings |
| Data/durability domain | FAIL | Snapshot-expiry authority scope is not lease-bounded |
| Tenancy/security domain | PASS | Validated empty finding set |
| SDK/contracts domain | PASS | Validated empty finding set |
| Fork-comparison domain | FAIL | Retained unconsumed fork planning surface |
| Focused follow-up | RESOLVED | Narrowed object-delete exception, snapshot-expiry conflict, and unique proposals |
| Structured Ponytail validation | FIX_REQUIRED | Five findings remain; revision 12 resolves the sole specification decision |

## Reconciled per-task acceptance matrices

### TASK-001 — leader and promotion

**Verdict: FIX_REQUIRED**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Renewal is independent of promotion and maintenance IO | Dedicated renewal loop and failover evidence | PASS |
| Loss of the term stops notify, pull, report, promotion, and maintenance | Handler validation releases the slot lock before schedule use; scheduler unwind clears `running` but not the cached term before restart backoff | **INCORRECT / REGRESSION — `FIND-TASK-001-1`** |
| Hot-publication recovery and exactly-once settlement remain durable | Promotion debt and operation evidence paths remain intact | PASS |

### TASK-002 — pull and worker results

**Verdict: FIX_REQUIRED**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Accepted dispatch closes once at shutdown and reports `NotStarted` once | Guarded dispatched close precedes the single report; row never becomes fair-claimable | PASS |
| RisingWave-shaped pull, timeout, report, and worker-side planning | Source and focused/journey evidence remain consistent | PASS |
| Shipped fork contains only consumer-earned retained machinery | Comparison is populated, but the pinned fork retains unused boundary planning/accessor methods, redundant context, and a second test-only planning entry point | **DRIFT — `FIND-TASK-002-2`** |
| Release benchmark qualifies | Recorded 16/16 checks pass | PASS |

### TASK-003 — maintenance and removal

**Verdict: FIX_REQUIRED through the snapshot-expiry seam**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Planning-demand state and live callers are removed | Forward migration and consumer inspection | PASS |
| Refused or uncertain prepared cleanup retains exact ownership and replays | `CleanupRetained` bypasses generic retry settlement and preserves task/attempt/candidate | PASS |
| Object cleanup remains ordered with Oracle cuts | Expired and orphan candidates are proved unreachable before the explicit lease-TTL uncertainty exception | PASS |
| Snapshot expiry remains ordered with Oracle cuts and bounded by the lease TTL | Current protocol does not make unresolved prepared expiry block new cuts as revision 12 now requires | **INCORRECT — `FIND-TASK-005-R1-3`** |

### TASK-004 — compaction defaults and type

**Verdict: PASS**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Public values use `auto`, `full`, `small-files`, and `files-with-delete` | One kebab-case wire owner and generated/client projections | PASS |
| Omission means `small-files` | Runtime, schemas, SDK prose, and docs agree | PASS |
| Rust SDK can name the type through the client facade | `wyrd_client::bifrost` re-export and Rust SDK proof | PASS |

### TASK-005 — Iceberg filtering scenarios 0–3

**Verdict: PASS**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Iceberg-owned field IDs and hot/promoted/rewritten filtering remain exact | Original tiered pruning evidence remains intact; remediation did not remove the paths | PASS |
| Scenario 4 remains superseded by revision 11 and TASK-005-R1 | Packet authority and task state agree | PASS |

### TASK-005-R1 / TASK-005-R2 — active-table reader cut

**Verdict: FIX_REQUIRED**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Leader stream owns Analytical lifetime and revokes descendants before claim release | Inline lifecycle ownership, structural drop order, and held-cut proof | PASS |
| Every acquisition and retry uses the original absolute deadline | Remaining duration is derived immediately before each PostgreSQL acquisition | PASS |
| Destructive authority and cut acquisition have one order | Ordinary effects and the safe object-delete TTL exception pass; unresolved prepared snapshot expiry does not yet block new cuts as revision 12 requires | **INCORRECT — `FIND-TASK-005-R1-3`** |
| Immediate sibling maintenance and SQL ownership | Stale test ban removed; production registry/layout SQL resides in `vala-sql` | PASS |
| Ownership documentation states the repaired hard rule | `analytical_supervisor.rs` still describes supervisor-owned query lifetime | **VIOLATION — `FIND-PACKET-1`** |
| Deleted reader-cut machinery remains absent | No epoch, ancestry frontier, IO gate, alias, or retention-derived cap returned | PASS |

### TASK-006 — Python API docstrings and runtime parity

**Verdict: PASS**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Runtime, source stubs, generated stubs, exports, and help agree | Complete recorded parity inventory and top-level Python tests | PASS |
| After-hook raises follow documented non-panicking behavior | `after_model`/`after_agent` abort the run; `after_tool` fails only the call | PASS |
| Removed Args-format test did not weaken an approved property | It enforced duplicate prose types; typecheck and parity tests own the contract | PASS |

### Packet-wide standards and scope

**Verdict: FIX_REQUIRED**

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Changed Rust ownership contracts have substantive accurate rustdoc | Broad sweep passed except the backwards Analytical supervisor module contract | **VIOLATION — `FIND-PACKET-1`** |
| Import/signature shape, diff check, and broad final gate | Shape audit clean; cumulative diff check clean; benchmark 16/16 and aggregate gate green | PASS |
| Owner-approved adjacent candidate content | The owner explicitly permits `406464a59` and its Bifrost-variant specification in this candidate | PASS |
| Packet metadata identifies current and historical authority | Index says revision 10 is the only authority; TASK-004 omits `spec_revision: 6` | **VIOLATION — `FIND-PACKET-6`** |
| Free-function and struct ownership rule | Repository authority now requires a struct only when it earns retained state, resources, identity, invariants, or lifecycle safety; `restarting_worker` remains caller-owned per invocation | PASS |

## Cross-task seams and user journeys

| Seam | Result | Evidence |
|---|---|---|
| Leader term → notify/pull/report → dispatch | FAIL | Slot check/use can cross revocation, and unwind leaves the term routable during same-pod restart backoff (`FIND-TASK-001-1`) |
| Dispatch → shutdown → report/retry | PASS | Exact dispatched close prevents fair claim and reports once |
| Prepared cleanup → reader refusal → replay | PASS | Exact prepared attempt and frontier remain the only replay owner |
| Leader stream → follower revocation → claim release | PASS | Synchronous owner collapse precedes claim release; only capacity residue and consumerless remote IO may remain |
| Oracle cut → object deletion | PASS | Candidate is proved unreachable before the explicit bounded uncertainty exception |
| Oracle cut → snapshot-expiry catalog commit | FAIL | The code does not yet apply revision 12's prepared-claim cut barrier (`FIND-TASK-005-R1-3`) |
| Contract owner → Rust/Python/TypeScript projections | PASS | Compaction and Python callback/runtime contracts agree |
| Forge component failure → same-pod restart | FAIL | Availability improves, but authority is not ended before restart backoff (`FIND-TASK-001-1`) |
| Candidate scope → packet acceptance | PASS | The owner explicitly permits the in-range Bifrost-variant specification |

## Validated finding ledger

The complete source evidence, proposal disposition, consequences, minimal
correction boundaries, and focused closure proofs are in
`findings-validation.md`. The retained stable IDs are:

- `FIND-TASK-001-1`
- `FIND-TASK-002-2`
- `FIND-TASK-005-R1-3`
- `FIND-PACKET-1`
- `FIND-PACKET-6`

## Prior-finding closure

Closed: `FIND-TASK-002-1`, `FIND-TASK-003-1`, `FIND-TASK-004-1`,
`FIND-TASK-004-2`, `FIND-TASK-005-R1-1`, `FIND-TASK-005-R1-2`,
`FIND-TASK-005-R1-4`, `FIND-TASK-005-R1-5`, `FIND-TASK-006-1`,
`FIND-TASK-006-2`, `FIND-PACKET-2`, `FIND-PACKET-3`, and
`FIND-PACKET-4`.

Open/revised: the five IDs in the validated ledger above.

## Follow-up decision

The focused follow-up was required because discovery conflicted on the
lease-TTL object-delete path, snapshot-expiry authority duration, leader
lifecycle edges, and several unique standards proposals. It resolved every
assigned uncertainty. Structured validation rejected the generic late-object
delete allegation and first-refusal metric concern, and retained the
snapshot-only authority conflict. The owner then permitted the in-range
Bifrost-variant specification and loosened the struct rule so caller-owned
per-invocation orchestration may remain a free function, removing
`FIND-PACKET-5` and `FIND-PACKET-7` from the final ledger. The owner then
approved specification revision 12, selecting the existing prepared
snapshot-expiry claim as the lease-bounded Oracle cut barrier.

## Verification limits

- Review did not rerun worktree-based executable gates because the checked-out
  branch was already past the immutable candidate; it validated the recorded
  candidate-scoped commands and reran commit-scoped static checks.
- `git diff --check c1508b375..7fcb45fc1` and
  `git diff --check e8d3cca13..7fcb45fc1` both exit zero.
- The recorded release benchmark passes 16/16 and `mise run gate` passes on
  `30d31ebac`; `406464a59` and `7fcb45fc1` add specification/evidence only.
- The hung-delete tests prove bounded lock release and retained uncertainty but
  do not emulate a remote backend completing after its local future drops;
  source reachability proves that narrow object-delete exception safe.
- No proof closes the leader check/use or scheduler-unwind interleavings, and
  the candidate predates revision 12's acceptance-unknown snapshot-expiry cut
  barrier.
