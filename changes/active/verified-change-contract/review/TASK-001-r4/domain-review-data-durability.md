# TASK-001 r4 — Data, Durability, and Concurrency Domain Review

Reviewer: `domain-rev` (persistent data, durability, recovery, and
concurrency).

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate HEAD at review start and completion:
  `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- The candidate source was not modified; this report is a review artifact.

## Reviewed boundary

- Retirement of `vala.eval.runs`, `vala.eval.assertions`, and
  `vala.drift_alerts` across the closed Bifrost table registry and Vala SQL
  owner/migration surfaces.
- Closure of `FIND-TASK-001-15`: the canonical registry and physical-layout
  contract now describe the six supported built-ins.
- Closure of `FIND-TASK-001-11`: stop/restart ownership, replacement
  application/Vala pool usability, survivor availability, retained WAL root,
  resource-plan re-derivation, reset resource ownership, and peer credentials.
- Closure of `FIND-TASK-001-12`: bounded Oracle graph settlement while
  preserving the exact six-field zero-ownership invariant.
- Closure of `FIND-TASK-001-17` for the restart and Oracle proof groups:
  repository-managed environment wrappers, pinned `mise exec -- cargo nextest
  run --locked` commands, exact selectors, and `--retries 0` repetitions.

## Authority and source coverage

| Boundary | Authority | Source and evidence inspected | Result |
|---|---|---|---|
| Retired analytical/control tables | Approved spec REQ-116; TASK-001 Scenario 3/AC-022; `AGENTS.md`; `architecture/agent-rules.md`; `architecture/bifrost-design.md` | Complete base-to-candidate diff; `vala-bifrost-redux/src/tables/mod.rs`; removed Eval table owners; `vala-sql/migrations/20260910000028_drop_drift_alerts.sql`; removed SQL query, row, and test owners | PASS |
| Canonical built-in registry documentation | `FIND-TASK-001-15`; TASK-001-R3; repository Rust-documentation rules | `BUILTIN_TABLES: [BuiltinTableDefinition; 6]`, `builtin_tables`, `builtin_fqns`, removed-name regression coverage, and `physical_layout_contract_all_builtins` | PASS — prior finding closed |
| Restart lifecycle and pool ownership | `FIND-TASK-001-11`; TASK-001-R2; Bifrost startup/recovery/shutdown authority | `WyrdTestCluster` restart flow; both restart tests; deletion of `PostgresPoolIdentity`; focused evidence and prior aggregate evidence | PASS — prior finding remains closed |
| Oracle terminal cleanup | `FIND-TASK-001-12`; TASK-001-R2; Bifrost Oracle admission and terminal-settlement authority | Distributed Oracle journey, `oracle_inspection`, bounded polling, final failure snapshot, and all six ownership counters | PASS — prior finding remains closed |
| Focused proof form and credibility | `AGENTS.md` testing rules; spec-driven development test-command precision; TASK-001-R3 | R2 evidence cells, explicit addendum, R3 evidence, and commit `fea2021da` | PASS — prior finding closed |

## Domain conclusions

- The canonical registry contains exactly six definitions, its return type is
  sized to six, both corrected rustdocs say six, and the existing negative
  registry test names `eval.runs` and `eval.assertions` as removed. No new
  abstraction or duplicate count mechanism was introduced.
- Restart proof no longer relies on heap addresses. The stopped node is absent,
  a survivor is queried before restart, and every restarted/running node's
  application and Vala pools answer real SQL afterward. The retained WAL root,
  resource-plan equality, zeroed resource counters, and peer authorization
  checks remain intact.
- Oracle cleanup still requires exact zero for `active_queries`,
  `queued_queries`, `reserved_memory_bytes`, `reserved_spill_bytes`,
  `peer_pending`, and `peer_running`. Polling only accommodates the documented
  terminal-before-settlement ordering; it is bounded to 300 × 100 ms and
  reports the final nonzero snapshot on failure.
- The committed addendum records the full environment-owning commands for the
  restart pair and Oracle journey, each run three consecutive times with the
  exact selectors and `--retries 0`. The R2 cells were updated to show the
  rerun command form, while the addendum explicitly says the original evidence
  used raw Cargo and was rerun. That edit is materially honest: it neither
  conceals the chronology nor claims the original run used `mise`, and it puts
  the currently applicable passing command beside each finding. Reverting the
  cells would add historical duplication without strengthening proof.

## Material proposed findings

None.

## Verification limits

- I did not rerun the database-backed or multi-process tests; I independently
  inspected the complete cumulative source diff, current owners, and committed
  exact-command evidence.
- Scoring algorithms, SDK projections, registration semantics, RBAC, and
  OpenAPI are outside this domain except where required to trace persistent
  table retirement and runtime ownership.

## Overall result

**PASS**

The data, durability, recovery, and concurrency obligations within TASK-001 are
closed, and this domain retains no material finding.
