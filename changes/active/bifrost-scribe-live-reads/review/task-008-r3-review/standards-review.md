# Independent repository standards review

Result: **FAIL**. FIND-007-4 is closed. Two bounded standards proposals remain: committed evidence whitespace and an authoritative description of the deleted live-read cap. No runtime defect is alleged by either proposal.

## Immutable subject and limits

- Candidate: `2f188cb6185061a43db36122aad68b5e253308d1`; parent: `9c3d7ecb982435919924dfa8e6930352b27a9b7e`.
- Cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`; TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
- Inputs: subject.md, cumulative and parent diffs, spec revision 20, TASK-007/008, R3 remediation and prior final verdict, TASK-006 diagnosis evidence. No current sibling discovery report was consulted.
- Current user decisions govern: no live snapshot byte/batch cap; accepted tenant error code stands; FIND-007-3 and Postgres tenant columns remain excluded. The cap decision is accepted, not challenged.
- Static only. No cargo, nextest, mise, builds, tests, benchmark, source edits or commits. Supplied historical/current test results are evidence claims, not newly executed proof. No CodeGraph index exists.

## Authority and changed-surface coverage

The reference router is `architecture/references/README.md`. AGENTS.md and agent-rules apply to every row. Focused references are subordinate to current user authority and owning architecture; older row-tenant/audit-WAL prose in secondary references does not reverse the approved decisions.

| Changed surface and owner | Applicable authority | Source coverage |
|---|---|---|
| Pure wire contracts, Scribe cut and assignment authority (`wyrd-spec`) | AGENTS §§2–6, 9, 11–12, 16; wyrd-design client/Bifrost contracts; bifrost-design distributed authority; references architecture/patterns, doctrine/architecture-constraints, languages/rust-core, errors, spec-driven-development | `vala/api.rs::ScribeProviderCut`, `assignment_authority.rs` domain and cut encoding/normative vector, managed columns, audit detail enum and generated audit schema |
| Private proto, descriptor and conversions (`wyrd-tonic`) | Same contract authorities; agent-rules generated-artifact and import rules; testing-workflows contract/proto drift | `wyrd.v1.proto` reserved tags/names; `private_conversion.rs` typed round trip; changed `.bin` noted as generated evidence, not independently regenerated |
| Scribe snapshot, schema, write/footer ownership (`vala-bifrost-redux`) | bifrost-design table identity, append/staging/live-tail lifecycle; rust-core; domain/olap-serving, arrow-analytical-interop, analytical-operations-reliability, iceberg; AGENTS §§3–6, 10–12, 16 | Memtable seal selection and shallow projection, shard snapshot mailbox, tail request/service and staged lease; envelope/schema/table changes; writer/claim assembly footer construction and persistence consumers |
| Shared Oracle scan, follower/native/remote result boundaries (`vala-bifrost-redux`) | bifrost-design Oracle/terminal/admission authority; wyrd-security-posture peer/tenant boundaries; domain/datafusion, olap-serving, iceberg, analytical-operations-reliability; references/errors | Catalog provider helpers; `exec.rs` published footer loader/hot metadata proof/cache/scan projection; follower leaf MemorySourceConfig/HotParquetExec/filter composition; live cut/decoder, native tally, dispatcher classification and scan metrics; leader audit classification |
| Forge footer propagation and identity (`vala-bifrost-redux`) | bifrost-design Forge publication; domain/iceberg/datafusion/analytical-operations-reliability; AGENTS §§3–6, 9–12, 16 | Managed policy/core config, executor and worker call sites, rewrite writer properties, table-bound tenant propagation; promotion/rewrite remains owned by Forge |
| Data root and private peer shutdown (`wyrd-server`) | AGENTS §§3–6, 9–12, 16; bifrost-design system boundary/live-tail/shutdown; security-posture peer identity; operations/reliability-and-recovery; domain/vala-architecture and analytical-operations-reliability | `BifrostDataRoot::prepare` and boot caller; volume roots/staging consumers; `build_peer_grpc`, OraclePeerGrpc constructor/execute_fragment and server shutdown-token owners; preserved role-local service dispatch |
| Client URL and generated config schema (`wyrd-client`) | wyrd-design shared client model; AGENTS §§2–4, 8–12; architecture/patterns, doctrine/architecture-constraints, errors/testing-workflows | HTTP default 8080, config comments/tests and both generated schema copies, client-configuration docs; no analytical dependency relocation into client tier |
| Retained real-server journeys and TASK-006 intersections (`wyrd-testing`, nextest/mise) | AGENTS §11; TESTING.md; languages/testing-workflows, spec-driven-development, implementation-execution; operations/reliability-and-recovery | PeerCluster/WyrdTestCluster ownership, affected Oracle distributed/peer network paths, deleted process-harness registration, nextest peer group, binary/mise registration and supplied evidence. Unrelated benchmark completion remains outside this audit |
| Architecture, docs and active evidence packet | AGENTS §§1–2, 12, 14; wyrd-design/doctrine; bifrost-design; reference router and spec-driven-development/implementation-execution | Cumulative footer tenant docs and managed envelope removals; current live-tail contract paragraph; TASK-006 diagnosis and static committed-range whitespace |

No Python, TypeScript, PyO3, N-API or UI source/declaration was materially changed in this reviewed range. Their independent coding rules are therefore not triggered by a native private Scribe cut; public client/tenant contracts remain shared, and supplied language journey evidence remains attributed to its original runs.

## Rule results

| Applicable rule | Evidence and assessment | Result |
|---|---|---|
| Imported bare names in newly introduced declarations; imports at module top (agent-rules) | Catalog imports `Error as IcebergError` and `provider_error(error: IcebergError)` at line 1801; Oracle already imports `DataFusionError` at line 16 and `is_tenant_refusal(error: &DataFusionError)` at line 4330. Bodies and caller types remain the same. Catalog's three provider call sites and Oracle audit/error/terminal consumers continue through these helpers | PASS; FIND-007-4 CLOSED |
| Foundation and client/server ownership (AGENTS §§2–3; patterns) | Cut/digest remains synchronous pure `wyrd-spec`; runtime/filesystem/grpc remain Vala/server. Added OpenDAL storage is pinned at the existing Iceberg revision and remains in the analytical owner; Rust version is 1.94.0, supporting the stdlib absolute path operation | PASS |
| Struct-centered owner and meaningful dependency composition (AGENTS §5) | Snapshot changes retain Memtable/ShardOwner/FetchLiveTailService; root change stays on BifrostDataRoot; shutdown token is an explicit OraclePeerGrpc field. Pure conversion/classification/digest helpers do not own IO workflows. No new parallel durable owner | PASS for changed task paths |
| Narrow async boundary and error propagation (AGENTS §§4, 6) | Root normalization stays synchronous and maps cwd failure to existing Unusable error; follower file size work uses async filesystem; peer stream awaits source/cancellation and reports existing Unavailable class; no new runtime or production unwrap | PASS |
| Tenant identity, no compatibility downgrade, canonical stable errors (AGENTS §§2, 9–10; security posture) | File proof precedes Arrow reader metadata in hot/staged and published paths, including cache hits; missing/duplicate/foreign tenant has existing QueryTenantInvariant. Signed cut keeps epoch/partition/projection authority. Memtable selection still tests seal tenant/table/range. Leader owns first-refusal canonical security event | PASS for standards boundary; domain correctness independently reviewed |
| Preserve durability/lease ownership (bifrost-design) | Snapshot cap deletion removes count/byte refusal, not seal membership/authority. Staged scan retains staged lease; normalized data root supplies every managed local child and retains exclusive lock/error behavior | PASS |
| Private listeners and cancellation ownership (AGENTS §§3, 6, 9; bifrost-design) | Only server builds/mounts peer router; same server shutdown token is passed to OraclePeerGrpc; open stream selects cancellation and drops existing producer on unavailable exit. Existing admission/grpc service owners remain | PASS |
| Contract parity and versioned digest (agent-rules; testing-workflows) | Tags 7/8 and names reserved rather than reused; domain and conversions remove cap fields together; v6 domain and vector length 460 reflect 12 deleted bytes. Supplied generated `.bin` evidence is recorded; no runtime/codegen run performed here | PASS statically; supplied generation proof |
| Substantive touched-item documentation (AGENTS §16) | New field, constructor/execute behavior, absolute-root errors/test panics and digest encoding are documented. Cap error docs remove exhausted snapshot ceilings. No new placeholder doc was established | PASS except authoritative leftover below |
| Credible tests and no gate weakening (AGENTS §11; testing-workflows) | Deleted over-cap tests expressed the expressly deleted behavior; capacity fault injection/mailbox behavior and real-server journeys remain. Relative-root test is in-module; supplied peer-loss journey is a runtime-owned journey. Historical evidence is not represented as this review's execution | PASS within static limits |
| Architecture reflects approved durable/material decision (AGENTS §§1–2, 14; spec-driven-development Change artifact lifecycle) | `architecture/bifrost-design.md:311–312` still explicitly requires retained-byte and batch-count enforcement for this exact live fragment path | FAIL; RSTD-R3-2 |
| Required final whitespace check and truthful final evidence (AGENTS §12; implementation-execution Focused verification) | Both explicit cumulative and parent-to-candidate `git diff --check` fail at new TASK-006 line 82 despite its line 101 PASS claim | FAIL; RSTD-R3-1 |

## Proposed findings

### RSTD-R3-1 — committed evidence reintroduces a failing whitespace gate

- Classification: VIOLATION; prior finding family: FIND-007-7 (the prior R2 content-preserving normalization remains closed for its original files; this is a recurrence at a newly changed evidence site).
- Obligation: AGENTS §12 requires green relevant checks and says a red gate blocks completion; implementation-execution explicitly requires `git diff --check` during focused verification/final audit.
- Exact location: `changes/active/bifrost-scribe-live-reads/tasks/TASK-006-real-server-benchmark-and-remove-process-harness.md:82`.
- Evidence: the parent diff adds a blank line containing two spaces. `git diff --check a7582db58 HEAD` and `git diff --check HEAD~1 HEAD` each report `82: trailing whitespace`. The same new evidence block claims that check PASS at line 101.
- Consequence: the committed candidate fails its required content/whitespace gate. This is not a runtime defect or a reason to rerun the benchmark.
- Smallest testable correction: remove only the spaces on the blank line, preserving all diagnosis/evidence content and the surrounding list. Attribute the final clean check to the corrected candidate rather than treating the current PASS statement as proof.
- Closure proof: explicit cumulative-base-to-candidate and remediation-parent-to-candidate `git diff --check` both exit 0; inspect the one-line content-preserving diff. No tests/builds are needed.

### RSTD-R3-2 — active Bifrost authority still describes the deleted live snapshot cap

- Classification: INCORRECT documentation/authority alignment.
- Obligation: current user explicitly approves no batch/byte cap and asks to inspect leftover cap references; AGENTS §§1–2 and §14 retain architecture as current durable authority. Spec-driven-development's lifecycle requires lasting material decisions in the owning architecture rather than only ephemeral implementation evidence.
- Exact location: `architecture/bifrost-design.md:311–312`, within “Live tail, recovery, and shutdown”.
- Evidence: the paragraph says “Projection, signed predicate, physical partition, retained bytes, batch count, deadline, and cancellation are enforced.” The candidate deletes `ScribeProviderCut.maximum_batch_count/maximum_retained_bytes`, FetchLiveTailRequest caps, ReadableBatchLimits/Collector and the shard merge cap. Their names survive in production only as reserved proto fields or explicit v5→v6 history. The TASK-006 diagnosis records the intended no-cap decision, but the active architecture paragraph still instructs future contributors to enforce it.
- Consequence: the owning authority contradicts approved and implemented behavior and presents a removed live-source refusal as a retained guarantee. This is the leftover directly caused by the approved deletion, not unrelated historical documentation debt.
- Smallest testable correction: update only this live-tail contract to remove snapshot retained-byte/batch-count enforcement, state that the snapshot holds shallow source references with execution retention governed by the query pool, and preserve projection, predicates, partition, deadline, cancellation and staged-lease ownership. Do not restore a cap or alter ingestion/result/transport limits.
- Closure proof: static comparison of this paragraph with current cut/request/collector and follower pool ownership; targeted search must distinguish proto reservations and explanatory digest history from active live snapshot promises. No new checker or test is needed.

## Verification and disposition

FIND-007-4 closes from exact source/import/caller inspection. Supplied TASK-006 evidence reports contract/source suites, root RED/GREEN, peer-loss journey and formatting/lints; those are not rerun here. The source-level standards review found no new runtime standards violation in the cap deletion, absolute root or shutdown changes. Both retained proposals have bounded content-only corrections; neither needs a new product, concurrency, tenancy or persistent-data decision.

HEAD was re-read as `2f188cb6185061a43db36122aad68b5e253308d1` during the review. **Overall result: FAIL**, for RSTD-R3-1 and RSTD-R3-2 only.
