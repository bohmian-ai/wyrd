# Packet-wide Task Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Branch: `worktree-agent-ac58f45cb5b747685`
- Base, excluded: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Pinned RisingWave reference: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The candidate identity remained unchanged throughout review. Only review
artifacts under this directory were added to the working tree.

## Verdict

**FIX_REQUIRED**

The independent validated ledger contains 17 bounded findings. None requires a
new product, public API, tenancy, security, compatibility, concurrency, or
persistent-data decision, so `SPEC_REVISION_REQUIRED` is not warranted.

## Independent review results

| Review | Result | Material output |
|---|---|---|
| Behavior | FAIL | `BEH-001`–`BEH-003` |
| Invariants | FAIL | `INV-REV-001`–`INV-REV-005` |
| Repository standards | FAIL | `STD-001`–`STD-004` |
| Maintainer | FAIL | `MNT-001`–`MNT-006` |
| System resilience | FAIL | `SYS-001`, `SYS-002` |
| Concurrency domain | FAIL | `CONC-001`–`CONC-003` |
| Data/durability domain | FAIL | `DATA-DUR-001`, `DATA-DUR-002` |
| Tenancy/security domain | PASS | No material finding; corrected S1 command passed 1/1 |
| SDK/contracts domain | FAIL | `D-SDK-001`–`D-SDK-004` |
| Focused follow-up | RESOLVED | Confirmed the four disputed seams |
| Ponytail validation | FAIL | 17 deduplicated `FIND-*` entries retained |

## Per-task acceptance matrices

### TASK-001 — leader and promotion

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| One PostgreSQL-elected leader; standbys take over | Election, token, graceful failover, and journeys exist | PASS |
| Loss/expiry of leadership stops dispatch and timer work | Renewal awaits unbounded promotion IO; cached and cloned terms remain usable after lease expiry/revocation | **INCORRECT — `FIND-TASK-001-1`** |
| Lost hints/restart do not strand hot promotion | Durable promotion-debt reconciliation and settlement tests exist | PASS |
| RisingWave comparison rows are populated | TASK-001 comparison is populated against the pinned revision | PASS |

### TASK-002 — pull and worker results

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Capacity-bounded pull, oldest-due selection, worker-side physical planning | Source and focused/journey coverage match the required pull shape | PASS |
| Accepted dispatch remains owned by pull/report protocol | Shutdown after durable insert calls generic retry and makes the row fair-claimable | **INCORRECT — `FIND-TASK-002-1`** |
| Current shipped fork is compared with upstream and prior fork across all mandatory rows | Evidence stops at the old fork and leaves the shipped `ef97aea` comparison incomplete | **MISSING — `FIND-TASK-002-2`** |
| Release-mode capacity evidence | Recorded qualifying runs cover the revised numeric gates | PASS |

### TASK-003 — maintenance and removal

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Planning-demand live state is removed by forward migration with callers retired | Forward drop migration and consumer rewrite exist | PASS |
| Manifest rewrite precedes expiry and cleanup preserves authoritative roots | Main paths and journeys exist, subject to the R1 authority-lifetime finding | PASS with cross-task dependency |
| Refused prepared cleanup preserves its exact attempt and replays | Generic failure settlement rejects the retained `prepared` state | **REGRESSION — `FIND-TASK-003-1`** |
| No stale test prohibition rejects approved immediate sibling maintenance | `production_closeout` still bans tenant-wide expiry/cleanup siblings | **VIOLATION — `FIND-TASK-005-R1-4`** |

### TASK-004 — compaction defaults and type

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Compaction defaults on and omitted type resolves to `small-files` | Runtime default is correct | PASS |
| Public values are `auto`, `full`, `small-files`, `files-with-delete`; docs agree | Wire uses underscore spellings and canonical docs/schema say omission defaults to full | **INCORRECT — `FIND-TASK-004-1`** |
| Rust, Python, and TypeScript first-class SDKs expose the contract | Rust SDK cannot name the enum without importing `wyrd-spec` directly | **MISSING — `FIND-TASK-004-2`** |
| Re-registration conflict and copy-on-write behavior | Focused contract and journey coverage exists | PASS |

### TASK-005 — Iceberg filtering across tiers, surviving scenarios 0–3

**Verdict: PASS**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Iceberg assigns physical field IDs for built-in and custom tables | Source, schema, promotion, and mismatch coverage exist | PASS |
| Hot, promoted, and rewritten files preserve filtering layout | Tiered pruning journeys cover the surviving scenarios | PASS |
| Binary page bounds and typed Bloom/min-max pruning remain exact | Focused mixed-predicate and binary coverage exists | PASS |
| Scenario 4 follows its original task text | Superseded by revision 11 and TASK-005-R1; correctly excluded from this task verdict | PASS |

This task's surviving behavior passes, but packet-wide repository findings still
prevent acceptance of the cumulative candidate.

### TASK-005-R1 — active-table reader cut

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| A dropped query cannot release before analytical descendants stop | Claim drop spawns deletion independently of supervisor cancellation/join | **INCORRECT — `FIND-TASK-005-R1-1`** |
| `abandon_after` equals the query's one absolute deadline | Acquisition and the one retry reuse an earlier duration and rebase it on later PostgreSQL time | **INCORRECT — `FIND-TASK-005-R1-2`** |
| Reader acquisition and every destructive effect have one serial order | Forge releases the exclusive authority transaction before catalog/object effects | **INCORRECT — `FIND-TASK-005-R1-3`** |
| Tests permit immediate sibling maintenance after release | Closeout retains a stale tenant-wide route ban | **VIOLATION — `FIND-TASK-005-R1-4`** |
| Durable SQL remains in the SQL owner | Redux embeds a duplicate raw `bifrost_tables` query | **VIOLATION — `FIND-TASK-005-R1-5`** |
| Tenant isolation and narrow catalog-pointer authority | RLS/grant review and corrected focused test passed | PASS |
| Deleted reader epochs/frontiers/IO gates/query caps remain absent | Static and source review found no prohibited replacement protocol | PASS |

### TASK-006 — Python API docstrings

**Verdict: FIX_REQUIRED**

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Public runtime, source stubs, generated stubs, and help agree | Session, Agent, Role, and error declarations contradict runtime and omit public methods | **INCORRECT — `FIND-TASK-006-1`** |
| Callback help documents behavior users can safely invoke | After-hook raise-to-abort prose reaches `unreachable!` in runtime consumers | **INCORRECT — `FIND-TASK-006-2`** |
| Python tests use top-level functions | Reviewed Python tests follow the repository rule | PASS |
| Generated declarations come from their owners | Generation path exists; correction must occur in source inputs | PASS with remediation |

## Cross-task seams, journeys, and regressions

1. **Leadership lifetime:** TASK-001's SQL lease, TASK-002's peer pull/report,
   and TASK-003 maintenance share a cached term that is neither renewed
   independently of promotion IO nor revoked inside an already-running pass.
2. **Reader/destruction ordering:** TASK-005-R1 records the row atomically with
   the cut, but TASK-003 destructive paths release the same authority before
   external effects. The reader-before-check journeys do not cover the
   reader-after-check/pre-effect race.
3. **Analytical abandonment:** normal terminal settlement joins descendants
   before release, while drop settlement does not; the existing journey
   releases its paused follower before checking the claim and therefore misses
   the required ordering.
4. **Cleanup settlement:** active-read refusal correctly preserves a prepared
   candidate, but TASK-002/003 generic retry settlement cannot represent that
   state and loses attempt ownership.
5. **Contract projection:** TASK-004's canonical wire/default errors propagate
   into schemas and all SDKs; TASK-006 separately contains runtime/stub drift.
6. **Pinned comparison:** the scheduler comparison is present, but the required
   physical-planner/fork matrix does not cover the shipped fork revision.
7. **Repository completion:** changed Rust symbols have mandatory rustdoc gaps
   (`FIND-PACKET-1`), changed modules contain prohibited local imports and
   qualified signatures (`FIND-PACKET-2`), two packet files fail the clean-diff
   requirement (`FIND-PACKET-3`), and this intentionally broad, shared-infra
   range has no recorded `mise run gate` (`FIND-PACKET-4`).

## Validated finding ledger

- `FIND-TASK-001-1`
- `FIND-TASK-002-1`, `FIND-TASK-002-2`
- `FIND-TASK-003-1`
- `FIND-TASK-004-1`, `FIND-TASK-004-2`
- `FIND-TASK-005-R1-1` through `FIND-TASK-005-R1-5`
- `FIND-TASK-006-1`, `FIND-TASK-006-2`
- `FIND-PACKET-1` through `FIND-PACKET-4`

The source evidence, discovery-source mapping, exact locations, consequences,
minimal corrections, and closure proofs are preserved in
`findings-validation.md`.

## Follow-up decision

A follow-up was required because discovery materially conflicted on dropped
analytical ownership, destructive lock lifetime, TASK-002 comparison
completion, and repository-gate interpretation. `followup-review.md` resolved
all four from source. No uncertainty remains that blocks remediation.

## Verification limits

- Supplied `verify:bifrost`, principals integration, format, and lint results
  do not cover the validated timing/lifetime races.
- TASK-005-R1's recorded S1 selector omits `pg_tests::` and selects zero tests;
  the security reviewer ran the corrected exact selector and it passed 1/1.
- The broad required `mise run gate` has not been recorded for this candidate.
- `git diff --check c1508b375..7ac45dec9` reports the two validated EOF defects.
- No broad suite was rerun after review because source remained immutable.

## Remediation routing

Route the following independently bounded artifacts to fresh `$wyrd-implement`
agents in dependency order where their write sets overlap:

1. `TASK-001-R1-revocable-leader-lifetime.md`
2. `TASK-002-R1-dispatch-ownership-and-fork-proof.md`
3. `TASK-003-R1-prepared-cleanup-settlement.md`
4. `TASK-004-R1-compaction-contract-parity.md`
5. `TASK-005-R2-active-read-lifetime-and-authority.md`
6. `TASK-006-R1-python-runtime-doc-parity.md`
7. `TASK-PACKET-R1-standards-and-final-gate.md` last, after source remediation
