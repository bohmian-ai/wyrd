# TASK-008-R1 cumulative confirmation verdict

**FIX_REQUIRED** — FIND-007-4/5/6 are closed. No executable regression was
found. One new artifact-only whitespace gate violation remains: FIND-007-7.

## Immutable subject and authority

- Candidate: `23eafa368bca19208faf8311eb7b5421e3660b38`, detached in this worktree.
- R1 parent/prior reviewed candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`.
- TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
- Original TASK-007 cumulative base: `a7582db587c6170a290760f1741673125612b797`.
- Approved authority: [spec revision 20](../../spec.md), REQ-014/015, AC-016/017 and applicable invariants; [TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md), [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md), both prior review/remediation packets, [TASK-008-R1](../task-008-review/TASK-008-R1-close-review-gaps.md), repository rules and applicable routed authorities covered by the independent reports.

The review assessed the complete cumulative TASK-007/008 scope, including prior
findings and remediation, plus the R1 correction. [Subject](subject.md),
[navigation](navigation.md), [cumulative diff](cumulative.diff) and
[R1 diff](r1.diff) preserve its boundaries. Unrelated TASK-006 benchmark and
lifecycle work remains excluded as previously established. No CodeGraph index
exists. HEAD and tracked source remained unchanged through final inspection.
No source edit, commit, merge or deployment occurred.

Maintainer decisions stand: FIND-007-3 is accepted unchanged; Postgres
`data_tenant_id` columns are excluded; the error code is
`WYRD_VALA_500_QUERY_TENANT_INVARIANT`. None is re-raised.

## Independent review results

Every role used a separate fresh agent. All required reports are complete.
Discovery reviewers received shared inputs without other discovery conclusions.
The orchestrator reconciled reports; the fresh validator independently checked
the proposal, source and prior-finding closure.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL, artifact gate proposal |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Persistent data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/native lifecycle | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| Fresh Ponytail validation | [findings-validation.md](findings-validation.md) | One confirmed finding |

[Claim comparison](claim-comparison.md) groups results by obligation. No focused
follow-up was needed: no runtime conflict or unreviewed reachable path remained;
the sole gate proposal had direct, reproducible source and command evidence for
the mandatory independent validator.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-014/TASK-007: shared staged Parquet scan and pruning; delete custom decoder/per-batch compilation | Existing HotParquetExec and shared metadata/pruning owners; focused staged pruning passes | PASS |
| Memory source with signed projection/filter | MemorySourceConfig and one signed conjunction; nonzero-ordinal projection/filter proof passes | PASS |
| Session partitions, including empty/sparse/mixed inputs | Fixed session-size memory groups and staged partitions; exact-once source test and follower shape proof pass | PASS |
| Native Arrow without IPC/hash; counts/fingerprint/terminal preserved; remote unchanged | Existing producer tally and decoder reconcile; completion and incremental-frame proofs pass; supplied three peer-service units pass | PASS |
| Scribe-local IO, query grants and staged leases | Independent lifecycle/domain traces; lease-drop and publication-overlap proofs pass | PASS |
| REQ-015/TASK-008: no managed tenant row in envelope/write/schema/scan | Deleted column/stamping/filter/codec, retired field ID; schema/writer checks pass and supplied distributed closure assertions agree | PASS |
| Authenticated footer on staged/assembled/published/Forge files | Seal/table binding travels through existing writers and both Forge rewrite callers; writer units pass, supplied promotion/rewrite journeys pass | PASS |
| Missing/foreign footer fails before file rows; no compatibility path | Mandatory published loader and shared hot/staged proof, including cache hits; footer/hot negative tests pass, supplied distributed COUNT(*) refusal passes | PASS |
| Memory stays seal-bound; deletion preserves tenant authorization/audit | Independent tenancy/source traces retain authenticated assignment and leader refusal event; supplied role/write-read journeys pass | PASS |
| Architecture/user docs and generated audit projection | Changed envelope/footer docs and matching generated TenantFile schemas; furnished codegen/docs results assessed within limits | PASS |
| Preserve ACK/WAL/publication/admission/remote/sibling behavior | Cumulative system/data/concurrency traces establish retained owners and failure boundaries; R1 has no executable change | PASS |
| FIND-007-4: imports/bare declarations | Corrected module imports preserve concrete types and feature scopes; scoped Clippy passes | PASS, CLOSED |
| FIND-007-5: exact named recipes and truthful attribution | Concrete names with package/features/target/environment templates, matching retained one-test PASS summaries, distinct historical/current/deferred evidence | PASS, CLOSED |
| FIND-007-6: changed schema test panic contract | Substantive rustdoc matches canonical ordering/tenant absence/non-null identity assertions; test body unchanged and focused test passes | PASS, CLOSED |
| R1 correction introduces no regression | Equivalent imports/type spelling, rustdoc and evidence only; independent source reviews and scoped checks agree | PASS |
| Required whitespace proof over immutable correction/cumulative change | Ten log trailing spaces and one report EOF diagnostic; both committed-range checks exit 2 | FAIL, FIND-007-7 |
| Excluded concerns and caller-owned benchmark/full lane | No SQL-column change, FIND-007-3 escalation or new capacity claim; prohibited execution avoided | PASS, scope respected |

PASS rows reconcile source coverage and available proof; they do not claim
fresh environment-backed journeys or benchmark qualification. AC-016 capacity
measurement remains caller-owned under the explicit execution restriction.

## Validated ledger and prior closure

Only independently confirmed findings enter this verdict.

| ID / status / class | Source ID and location | Consequence and smallest correction |
|---|---|---|
| FIND-007-7 / CONFIRMED / VIOLATION | RSTD-R1-1; `review/task-008-review/final-named.log:1-10` and `task-review-invariants.md:55` under this change | Immutable reviewed diff fails required whitespace gate. Remove only ten trailing spaces and extra EOF blank line; preserve commands/results/conclusions. Prove cumulative and correction range whitespace checks exit 0. |

FIND-007-1/2 remain corrected in the cumulative candidate. FIND-007-3 remains
accepted unchanged. FIND-007-4/5/6 are independently CLOSED. FIND-007-7 is
distinct from proof-recipe finding 5: named proof precision is now complete;
newly committed artifacts contain a separate gate violation. No runtime
defect, corrupt result or false historical test outcome is alleged.

The final validator selected existing Git checks and artifact-owner cleanup.
No product, API, architecture, security, concurrency or persistent-data
decision is needed. The required gate remains red even though an empty
post-commit working-tree diff check passes.

## Verification limits and delivery

[Verification](verification.md) and captured logs record **13/13 focused redux
tests passing**, scoped format and warnings-denied test-support all-targets
Clippy passing, and the committed-range whitespace failure. Source/task/docs
R1 whitespace passes when scoped separately; that isolates but does not waive
the artifact failure. HEAD and tracked source stayed immutable.

The supplied integrated-tree log records 13 redux units, 3 server units and 9
journeys, each selecting one passing test, plus migration setup. Those server,
journey, codegen/docs and broad gate results were inspected, not rerun here.
No Postgres-wrapped test, full mise lane or benchmark ran in this review.
Build/state paths used the requested worktree-local directories.

One self-contained bounded remediation is ready for `$wyrd-implement`:
[TASK-008-R2-normalize-review-evidence-whitespace.md](TASK-008-R2-normalize-review-evidence-whitespace.md).
It changes only artifact whitespace and needs no runtime rerun. The verdict
is **FIX_REQUIRED**, solely for FIND-007-7.
