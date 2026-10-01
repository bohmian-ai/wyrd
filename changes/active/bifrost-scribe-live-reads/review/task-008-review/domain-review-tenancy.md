# Independent domain review: tenancy and security

Result: **PASS**. No material proposed tenancy/security finding.

## Subject and scope

Candidate `6e7add054e33701ca5ecb52a5c859948b15161a3`, immediate base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; TASK-007 cumulative original base `a7582db587c6170a290760f1741673125612b797`. Reviewed TASK-008 and the relevant TASK-007/R1 shared-scan, signed-source and native-output paths. Candidate HEAD remained unchanged during this audit. This fresh discovery review did not read the current attempt's other reviewer reports.

Authority: approved spec revision 20 REQ-014/015, INV-002, AC-016/017; TASK-008 and TASK-007/R1; AGENTS tenancy, trust-boundary and audit rules; architecture/agent-rules.md; Wyrd design/doctrine; architecture/bifrost-design.md physical tenant and peer authority sections. No CodeGraph index exists. The standing disposition for FIND-007-3, Postgres tenant-column scope and approved QueryTenantInvariant code are honored.

## Boundary and source coverage

| Boundary | Inspected evidence | Assessment |
|---|---|---|
| Authenticated query identity | `oracle/mod.rs:285-296` rejects a context tenant different from the principal; persisted leader leaves at `exec.rs:1904-1913` take this context's tenant | Tenant is independent of stored row contents and cannot be selected by SQL projection |
| Producer binding and seal | `scribe/parquet_writer.rs:388-406` compares binding, requested seal tenant, frozen seal tenant and table before materialization; `ArtifactPlan` gets that seal tenant at :352; :603-626 stamps and inspects the closed file | Removing the row stamp preserves the preexisting single-tenant seal authority |
| Assembled published files | `scribe/staging_runtime.rs:714-723` selects the stored context by claim key and passes `context.binding.tenant`; `claim_assembly.rs:251-267` uses the same rolling artifact writer | Assembly output records the tenant whose durable key/context selected its source members; no per-row relabeling mechanism added |
| Forge rewrite output | `forge/managed/executor.rs:199-202,286-289` passes `self.binding.tenant`; `forge/managed/policy.rs:264-270` feeds that tenant to `bifrost_rewrite_writer_properties`; `parquet/writer_properties.rs:122-129` sets one tenant key | Rewrites retain binding authority and stamp each output file without a row column |
| Footer trust boundary | `parquet/footer.rs:49-53,153-168` requires exactly one non-null value equal to the reading tenant; `oracle/exec.rs:1109-1146` preserves the typed invariant failure | Missing, foreign, duplicate and valueless tenant keys are refused; no compatibility fallback |
| Published leader scans | `exec.rs:995-1039` keys metadata by authenticated tenant and checks every cache hit/miss before handing metadata to Iceberg; :1424-1429 refuses key metadata that bypasses the loader; :1562-1570 requires loader presence | File checks precede row-group/page/row decoding, including empty-column projections |
| Hot leader scans | `oracle/mod.rs:3043` uses the cut binding tenant for hot metadata identity; `exec.rs:2942-2961` checks that identity's tenant against the retained footer before building the reader | The guard is independent of the requested projection/predicate and survives deletion of row-filter/tripwire nodes |
| Oracle followers | `follower.rs:1605-1609` compares each signed binding to authenticated tenant/table; :471-476 resolves the tenant-qualified catalog provider; :556-564 attaches tenant-proving published loader; :605-613 constructs hot keys from the signed binding | Both follower persisted tiers preserve the same tenant check and authenticated binding |
| Scribe staged and memory sources | `follower.rs:1060-1068` resolves the binding/schema before tail opening; :815-822 puts that assignment tenant in the staged key; `tail_rpc.rs:620-629,675-683` selects memory/staged sources by tenant/table; `exec.rs:2955` checks staged metadata before decode | Memory remains seal-bound; staged rows reach the shared hot reader proof; signed filter/projection does not substitute for tenancy |
| Refusal propagation | `peer_service.rs:674-675` maps typed staged refusal to `DispatchError::TenantInvariant`; :639 preserves it over private gRPC; `dispatcher.rs:1785` recovers it; `live.rs:647-666` excludes it from availability/degradation and restores the typed query error | A tenant fault is terminal rather than a retry/degraded omission |
| Security audit | `oracle/mod.rs:2423,2461` wraps Interactive and Analytical streams; :2486-2521 records the first tenant refusal once through the existing audit owner; :4332-4337 recognizes typed/local and transported refusal | Audit remains leader-owned and non-blocking; multiple refusing leaves do not create parallel audit authorities |
| Deleted guards and catalog consumers | Immediate diff deletes provider tenant filtering/tripwire and codec plumbing; catalog `provider`/`pinned_provider` production callers are the Oracle follower resolver; leader uses the pinned persisted owner | No reviewed reachable query consumer bypasses the replacement proof by directly executing an unwrapped catalog provider |

The added logic is one footer field and a single deterministic comparison reused by published and hot/staged readers. Mandatory published-loader routing and refusal propagation are needed because the deleted row node previously supplied the trust boundary; they do not introduce a new authorization policy or resource owner.

## Verification evidence and limits

Inspected the focused footer rejection test (`parquet/footer.rs:315-335`) and hot foreign/missing rejection test (`oracle/exec.rs:4841-4925`). They cover value/absence/duplicate handling and refusal on first stream poll before output. Inspected the distributed journey's corrupted foreign footer and zero-output/QueryTenantInvariant assertions (`wyrd-testing/tests/bifrost/oracle/distributed.rs:310-335`), the producer footer assertions, and compiled signed-source closure/binding tests. TASK-008 records focused and journey passes, but implementation evidence is a claim rather than an independent run.

This discovery pass was static only, as assigned. No Postgres wrapper, full mise lane, runtime test, commit or source edit was performed. The orchestrator owns allowed focused runtime verification. This report establishes source behavior and credible existing test scenarios; it does not independently certify historical test executions or all distributed deployments.

## Proposed findings

None. No out-of-scope hardening, optional refactor, disposed FIND-007-3 claim or Postgres column change is requested.
