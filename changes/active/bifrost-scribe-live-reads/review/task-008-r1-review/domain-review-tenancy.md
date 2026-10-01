# Domain review: tenancy and security

Result: **PASS**. Material proposed findings: **none**.

## Immutable subject and limits

Reviewed candidate `23eafa368bca19208faf8311eb7b5421e3660b38`, correction parent `6e7add054e33701ca5ecb52a5c859948b15161a3`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, and TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. This is an independent static sensitive-domain review, not a test execution or a substitute for the task acceptance reviewers. Source remained immutable. No Postgres wrapper, mise lane, test, commit, or source modification was performed.

Scope is revision 20 REQ-014/015, AC-016/017, original TASK-007/008 and TASK-008-R1. Unrelated TASK-006 changes are excluded. FIND-007-3 remains accepted unchanged; Postgres tenant columns remain outside scope; `WYRD_VALA_500_QUERY_TENANT_INVARIANT` is the approved error.

## Authority and coverage

| Boundary | Governing authority | Source inspected and conclusion |
|---|---|---|
| Tenant source and physical write binding | AGENTS §§2/3/9; agent-rules tenant and audit boundaries; wyrd-design identities; security posture principles and tenant isolation; revision 20 REQ-015 | `scribe/ingress.rs:59–60,275–290,400–428,527–575`: frame tenant agrees with verified principal, physical binding is authenticated before admitted append; principal tenant is retained out of band. |
| Frozen member and staged footer | Bifrost design managed envelope and per-file tenant proof; REQ-015 | `scribe/member_stager.rs:162,201–202`; `scribe/parquet_writer.rs:270–285,344–409,594–627`: seal tenant, binding tenant and frozen seal key must agree before materialization; rolling writer stamps canonical `BifrostFooterIdentity` and re-inspects it. |
| Claim assembly and rewritten outputs | Bifrost design Scribe/Forge table-bound publication; REQ-015 | `scribe/staging_runtime.rs:640–669,710–723`, `claim_assembly.rs:85–88`; `forge/managed/executor.rs:181–202,274–289`, `policy.rs:255–271`, `parquet/writer_properties.rs:119–129`: recovered keys resolve tenant-qualified context; assembly and both Forge configuration paths stamp the existing table binding tenant. |
| Footer trust boundary and cache isolation | architecture-constraints identity and tenant isolation; olap-serving/datafusion per-file proof; REQ-015 and AC-017 | `parquet/footer.rs:26–51,149–168`; `storage/cache.rs:174–227`; `oracle/exec.rs:973–1047,1110–1147,2942–2959`: missing, valueless, duplicated or foreign tenant metadata is refused; metadata key contains authenticated tenant and table plus immutable-object identity; cache hits still pass the comparison before reader metadata is constructed. No proof cache or row fallback exists. |
| Published scan and distributed worker | Security posture peer identity and distributed Oracle; Bifrost design authenticated source binding; REQ-014/015 | `oracle/exec.rs:1421–1465`: every opened published reader receives tenant-proving loader; encrypted/key-metadata tasks that bypass it are refused. `oracle/follower.rs:454–476,554–562,1590–1622`: catalog and loader take authenticated assignment tenant, preflight rejects mismatched tenant/binding before provider use. |
| In-memory and staged local Scribe source | REQ-014, REQ-015 in-memory seal binding, INV-004 | `oracle/follower.rs:778–859,1042–1113`; `scribe/tail_rpc.rs:594–639,665–690`: live rows and staged leases are selected by tenant/table seal scope; staged keys use that same authenticated assignment tenant and shared hot scan; the resolver opens its local source only. In-memory rows have no per-row tenant work. |
| Native and remote fragment authority/refusal | Security posture peer identity; Bifrost design terminal integrity and nonblocking Oracle audit | `wyrd-server/src/oracle/peer_service.rs:240–355`: native execution still verifies peer ticket, tenant, binding, role/fence, plan and assignment-authority digest before provider IO. `peer_service.rs:630–677`, `oracle/dispatcher.rs:1785,1997`, `oracle/live.rs:663–665`: typed tenant refusal survives native execution and remote status projection and cannot become an eligible missing source or successful degraded result. |
| Refusal attribution and public terminal | AGENTS audit single-write-path; agent-rules nonblocking Oracle exception; revision 20 fail closed | `oracle/mod.rs:2423,2461,2486–2529,4315–4333`: both query execution paths wrap their source stream; the first tenant refusal produces one tenant-file security event using authenticated query context and preserves the failing error. No footer value chooses the audit tenant. |

The reference router, architecture-constraints, olap-serving and datafusion references were inspected with the owning architectural sections. Security posture still contains older row-tripwire and Oracle audit-spool prose; AGENTS/current Bifrost authority and approved revision 20 explicitly govern the changed behavior. That unrelated authority drift is not a task defect or a reason to reintroduce deleted mechanisms.

## Falsification and proof assessment

- Attempted cache-hit bypass: unsuccessful. The cache stores decoded metadata, not an accepted tenant proof; published and hot/staged reader opens compare every retained footer against the authenticated key/loader tenant before it reaches the reader.
- Attempted zero-column/count bypass: hot reader proves footer before projection/pruning/decode, and published scan installs the mandatory loader and rejects the alternate key-metadata path. The integrated `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` source injects a foreign file then exercises `count(*)`, requiring zero delivered rows and `QueryTenantInvariant`; it also rejects a per-row tenant closure.
- Attempted native trust-boundary bypass: unsuccessful. Native Arrow removes IPC and content hashing but retains the same `ScribeFragmentExecutor` verified ticket and assignment checks before tail IO. Remote failure mapping retains a distinct tenant class.
- Attempted in-memory tenant mix: unsuccessful on the reviewed reachable path. Caller authority supplies the tenant/table binding; memtable selection and staged-source leasing query that exact scope; member encoding independently refuses a divergent frozen seal key.
- Attempted producer omission: staged generation, ordered claim assembly and both Forge rewrite paths have canonical tenant stamps; unchanged promotion preserves the staged bytes and tenant footer. No compatibility reader accepts a missing field.

Available evidence is the TASK-008 integrated exact-proof inventory and `review/task-008-review/final-named.log`, whose terminal summaries record single selected tests for footer refusal, hot refusal, writer footer identity, staged scans, Forge rewrite/promotion, distributed count refusal, write/read and scoped-role journeys. I inspected the focused production/test paths and the recorded summaries; I did not recreate the external environment or independently rerun them. Full lane and benchmark remain caller-owned as requested.

## Correction regression and prior findings

The R1 source diff changes only import spelling/type declarations and the schema test panic documentation. Imported `DataTenantId`, `ArrowReaderMetadata` and `WriterProperties` preserve the concrete pre-correction types. The claim test uses the same root re-export already used in production. No expression, tenant comparison, metadata field, transport mapping, assertion, feature gate or runtime branch changes. The added proof inventory is evidence rather than executable behavior. No tenancy/security regression was found.

FIND-007-4/6 correction introduces no domain regression; full repository-rule closure belongs to the standards/maintainer reviewers. FIND-007-5 now supplies package/target/features/environment templates and exact names for the tenant proofs and journeys, with the recorded current execution evidence. No domain finding remains.
