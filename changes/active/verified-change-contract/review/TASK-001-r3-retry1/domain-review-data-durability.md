# TASK-001 r3 retry 1 — Data, Durability, and Concurrency Domain Review

Reviewer: `domain-rev` (persistent data, migrations, durability, and
concurrency).

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `9d7b6266206f15136f306b66c06571066fc6bd13`
- Candidate HEAD at review start and completion:
  `9d7b6266206f15136f306b66c06571066fc6bd13`
- The source candidate was not modified. Pre-existing and concurrently written
  untracked review artifacts are outside the immutable candidate.

## Reviewed boundary

- The cumulative retirement of `vala.eval.runs`, `vala.eval.assertions`, and
  `vala.drift_alerts`, including their Bifrost registry, SQL owner, migration,
  and test surfaces.
- The `wyrd.cards` kind-constraint migration insofar as it changes persistent
  Card identity from the retired Drift/Eval kinds to Verifier.
- Round-2 closure of `FIND-TASK-001-11`: cluster stop/restart ownership,
  deletion of allocator-address identity, replacement application/Vala pool
  operations, retained WAL root, resource-plan re-derivation, clean resource
  counters, survivor usability, and peer-credential behavior.
- Round-2 closure of `FIND-TASK-001-12`: distributed Oracle terminal failure,
  asynchronous graph settlement, bounded observation, and release of all six
  previously asserted admission/resource/peer ownership fields.
- Committed focused and aggregate verification evidence, including the
  explicit no-retry repeated runs.

## Authority and source coverage

| Boundary | Authority | Source inspected | Result |
|---|---|---|---|
| Retired analytical/control tables | Spec REQ-116 and TASK-001 Scenario 3/AC-022; `AGENTS.md` ownership, migration, testing, completion, and Rust documentation rules; `architecture/bifrost-design.md`; analytical reliability and OLAP-serving references | `vala-bifrost-redux/src/tables/{mod,eval/*}.rs`; `vala-sql/migrations/20260910000028_drop_drift_alerts.sql`; deleted Vala SQL query/row/test owners; prior migration and cumulative diff | **FAIL**: runtime retirement is complete, but two materially changed registry items still document eight built-ins after the registry was reduced to six (`DDR3R1-1`) |
| Persistent Card-kind constraint | Spec REQ-109; TASK-001 Scenario 1; `AGENTS.md` server/contract rules | `wyrd-sql/migrations/20260601000025_verifier_card_kind.sql` and cumulative migration ordering | PASS |
| Restart lifecycle and pool ownership | `FIND-TASK-001-11`; TASK-001-R2; Bifrost startup/recovery/shutdown authority; repository no-retry completion rule | `WyrdTestCluster::{build_node,stop_node,restart_node,server_by_node}`; the two cluster restart tests; deletion of `PostgresPoolIdentity` and `postgres_pool_identity`; committed focused and aggregate evidence | PASS — prior finding closed |
| Oracle terminal cleanup | `FIND-TASK-001-12`; TASK-001-R2; Bifrost distributed execution/admission contract; analytical reliability cancellation rule | distributed predicate/projection journey, `OracleInspection`, `oracle_inspection`, admission inspection owners, and the existing capacity-settlement pattern | PASS — prior finding closed |
| Verification credibility | `AGENTS.md` §§11–12 and TASK-001-R2 focused/broader proof | remediation packet evidence for three consecutive `--retries 0` restart/Oracle runs and one green 9/9 `test:bifrost` aggregate | PASS |

## Domain conclusions

- `FIND-TASK-001-11` is closed. `stop_node` removes and shuts down the owned
  server before `restart_node` calls the existing `build_node` owner against
  retained node resources. The invalid pointer comparisons and their only
  helper are gone. Both restart tests now execute SQL through the replacement
  application and Vala pools; the multi-node test also exercises every running
  node after restart and the survivor before restart. The retained WAL-root,
  re-derived plan, reset resource counters, role, and credential assertions
  remain. No generation token, retry wrapper, fixed sleep, or new fixture was
  introduced.
- `FIND-TASK-001-12` is closed. The distributed journey still requires exact
  zero for `active_queries`, `queued_queries`, `reserved_memory_bytes`,
  `reserved_spill_bytes`, `peer_pending`, and `peer_running`. It re-reads the
  existing inspection every 100 ms for at most 300 intervals and returns the
  final nonzero snapshot on failure. This matches the real terminal-before-
  settlement ordering without turning cleanup into best effort or weakening
  any previously asserted field.
- The committed evidence records three consecutive focused passes for both
  restart tests and the Oracle journey with `--retries 0`, followed by a green
  9/9 Bifrost aggregate. No encountered failure is waived by retry in the
  remediation evidence.
- The two Eval built-ins no longer have table owners or entries in the closed
  registry, and the removed-name regression assertion covers both. The Drift
  alert table has one forward, highest-version drop migration after its
  unconditional creation; its SQL query, row, and integration-test owners are
  deleted. REQ-116 explicitly requires no retained-data migration because
  these surfaces never shipped.

## Material proposed findings

### DDR3R1-1 — The retired-table registry still documents eight built-ins

- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` §16 requires rustdoc on every materially
  modified Rust item to explain its actual workflow and invariants and makes
  incorrect documentation a hard blocker. TASK-001/REQ-116 requires the two
  Eval built-ins to be removed rather than remain part of the canonical table
  model.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:751` and `:861`.
- **Evidence:** the candidate correctly changes `BUILTIN_TABLES` and
  `builtin_tables()` from eight entries to six, but `builtin_fqns` still says
  it returns “the eight built-in logical FQNs,” and
  `physical_layout_contract_all_builtins` still describes “all eight
  built-ins.” Both items now execute over the six-entry registry. These are
  materially changed workflow contracts even though the stale numeral itself
  survived as context in the diff.
- **Observable consequence:** a maintainer reading the canonical Bifrost table
  owner is told that two more built-ins exist than can be resolved. That
  directly obscures whether the retired Eval tables remain part of the
  supported physical-layout contract and violates the repository's hard
  documentation rule.
- **Required testable correction:** change only those two rustdoc numerals from
  eight to six. Reuse the existing registry and tests; add no helper, check, or
  abstraction. Prove closure with the focused `vala-bifrost-redux` table test
  owner plus `mise run fmt` and `mise run lints` (the existing broader
  `test:bifrost` evidence remains applicable).

## Verification limits

- I did not rerun the multi-hour Bifrost aggregate. I inspected the complete
  base-to-candidate diff, current owners and call paths, and the focused and
  aggregate results committed in the immutable remediation packet.
- I did not review scoring algorithms, SDK projections, security/RBAC, or
  OpenAPI behavior except where necessary to establish table and lifecycle
  ownership.

## Overall result

**FAIL**

Both prior durability/concurrency findings are closed without weakened
assertions or retry waivers, and the persistent-table retirement is
functionally complete. `DDR3R1-1` remains a bounded repository-rule violation
in the changed canonical table owner.
