# TASK-008 final confirmation verdict

**FIX_REQUIRED**, solely for residual **FIND-007-4**. **TASK-008-R2 / FIND-007-7 is CLOSED.** No new runtime regression was established against cumulative TASK-007/008, spec revision 20 REQ-014/015 and the reviewed AC-016/017 obligations.

## Immutable subject and scope

Candidate: `9c3d7ecb982435919924dfa8e6930352b27a9b7e`, detached HEAD.
Final-delta parent: `23eafa368bca19208faf8311eb7b5421e3660b38`.
Cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.
TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Authority: [approved spec revision 20](../../spec.md), [TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md), [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md), original/prior review and remediation packets, repository rules and applicable routed authorities. [Subject](subject.md), [navigation](navigation.md), [cumulative diff](cumulative.diff) and [final diff](final.diff) preserve review inputs.

The full cumulative task scope was reviewed, including prior fixes. Adjacent TASK-006 work was judged only where it intersects TASK-007/008 behavior. FIND-007-3, Postgres tenant columns, the approved `WYRD_VALA_500_QUERY_TENANT_INVARIANT` code and the pending lost-Scribe deadline decision stand without being re-raised. No CodeGraph index exists.

## Independent reports and comparison

Each role was a separate fresh agent; all required reports are complete. Discovery inputs contained no other current discovery conclusions or intended verdict. The fresh validator received every report and inspected source independently.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL, one proposal |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Persistent data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/native lifecycle | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| Fresh Ponytail validation | [findings-validation.md](findings-validation.md) | CONFIRMED residual FIND-007-4 |

[Claim comparison](claim-comparison.md) groups the sole proposal by the declaration obligation. No focused follow-up was needed: no conflicting runtime path or unreviewed consumer remained; the exact declaration claim was directly resolvable by independent validation. General earlier import-closure claims do not disprove the two cited sites. The prior R1 verdict missed them; this verdict corrects that overbroad closure assessment.

## Reconciled acceptance matrix

Detailed source/test locations are preserved in both task reports and the validator.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-014/TASK-007: shared Parquet staged scan, pruning; deleted custom decoder/per-batch compile | Scribe resolver composes HotParquetExec; shared row-group/page/cache reader; focused pruning asserts matching rows, pruned groups and positive bytes | PASS |
| Memory engine source, signed pushdown and session partitions | MemorySourceConfig, signed residual conjunction and session-sized sparse/empty/mixed groups; partition/projection/follower shape proofs | PASS |
| Native Arrow without IPC/hash; counts/fingerprint/terminal preserved; remote unchanged | Native tally and decoder reconcile; remote encoder remains at gRPC boundary; completion/incremental tests and supplied server/Oracle journeys | PASS |
| Scribe-local IO, query grants, staged leases | Authenticated resolver builds local IO; shared follower pool and stream-held lease; lease-drop/publication-overlap proof | PASS |
| REQ-015/TASK-008: no managed tenant row in writes, schemas or projections | Removed envelope/stamping/filter/tripwire/codec; retired field ID; schema/writer/closure assertions and generated parity | PASS |
| Authenticated tenant footer through staged/assembled/published/Forge files | Existing writer identity and table binding reach both rewrite callers; writer/assembly and retained promotion/rewrite proof | PASS |
| Missing/foreign footer refuses before file rows, including cache and COUNT(*) | Mandatory published loader and shared hot/staged proof; focused negatives and public local/distributed refusal journey | PASS |
| Memory seal binding, authorization and leader audit retained | Signed authenticated assignment and tenant-qualified memory snapshot; canonical non-blocking first-refusal audit | PASS |
| Docs and generated contracts | Footer/table identity docs and matching audit schema; retained docs/codegen results and furnished current no drift | PASS |
| Final scanned-byte metric introduces no regression | Validated completion folds existing accumulator once; query collector retains leaf handle; native/wire total and memtable-absence proof | PASS |
| Harness deletion/client default interactions | Retained production PeerCluster/server/journey paths; HTTP default/schema/docs agree on 8080; supplied testing/client/language outcomes | PASS |
| ACK/WAL/publication/admission and sibling services | Independent data/system/concurrency traces preserve owners, fences and request-scoped failures | PASS |
| FIND-007-1/2/5/6 prior corrections | Session shape, native reconciliation, exact evidence attribution and substantive panic docs remain | PASS, CLOSED |
| FIND-007-4: every newly added/materially changed declaration satisfies module-import rule | Two new private helper signatures still use qualified argument types; fresh validator confirms against mandatory agent rule | FAIL, residual OPEN |
| FIND-007-7/TASK-008-R2: content-preserving evidence normalization | Exact artifact diff removes ten trailing spaces/one extra EOF line; cumulative, parent and R1-parent Git checks exit 0 | PASS, CLOSED |
| Non-goals, static-only review and caller-owned capacity | Maintainer exclusions preserved; no builds/tests/benchmark/commit/source edits or new capacity claim | PASS |

PASS rows reflect bounded task behavior and credible available proof. AC-016 capacity qualification remains caller-owned on this occupied host; this verdict does not claim the entire specification's benchmark acceptance is complete.

## Validated finding ledger and correction

Only independently confirmed findings are retained.

| Stable ID | Discovery / status / class | Exact location | Consequence and minimum correction |
|---|---|---|---|
| FIND-007-4 | RSTD-FINAL-1 / CONFIRMED / VIOLATION | `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1801`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4332` | New `provider_error(iceberg::Error)` and `is_tenant_refusal(&datafusion::error::DataFusionError)` violate mandatory imported bare type signatures. Import existing Iceberg error in catalog; reuse Oracle's existing DataFusionError import. Preserve exact types, bodies, visibility and callers. |

This is incomplete cumulative closure of FIND-007-4, not a new runtime defect. Prior identified import sites are corrected; the two additional sites prevent complete closure. No new finding ID is assigned. FIND-007-7/R2 is independently closed. The validated new-runtime-regression ledger is empty.

Self-contained remediation: [TASK-008-R3-close-residual-declaration-imports.md](TASK-008-R3-close-residual-declaration-imports.md), routed to `$wyrd-implement`. No product, public API, architecture, security, concurrency or persistent-data decision is needed.

## Verification limits and immutability

[Static verification](verification.md) records clean committed-range whitespace checks. Independent validation also proves the corrected artifacts have no substantive content difference. No cargo, nextest, mise, builds, tests or capacity benchmark were run by this review; no source change or commit occurred. Final HEAD and tracked diff checks preserve the declared candidate.

User-furnished exact-tree results: Bifrost 8/9 lanes, with sole failure in the now-deleted process harness; Oracle journeys 40/40, server 26/26, Python/TypeScript pass. Post-edit lints exit 0, fmt/whitespace clean, client 314/314 with Postgres wrapper, live/follower 19/19, testing lib/bins 61/61 and regenerated contracts without drift. Historical named journey/docs/codegen proof remains distinct from furnished current results. No runtime blocker is invented from the requested execution restriction.

**Verdict: FIX_REQUIRED — FIND-007-4 only. R2 closed; no new runtime regression found.**
