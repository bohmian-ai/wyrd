# Independent tenancy/security domain review

Result: **PASS**. Material proposed findings: none.

## Subject and authority

Reviewed detached candidate `9c3d7ecb982435919924dfa8e6930352b27a9b7e`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`, and final delta from `23eafa368bca19208faf8311eb7b5421e3660b38`. HEAD was checked and remained the candidate. Read shared subject/navigation, cumulative and final change surfaces, original tasks, revision 20 REQ-014/015, AC-016/017, INV-002/006, prior verdict and remediation packets. Governing authority: AGENTS.md tenancy/server/audit rules, architecture/agent-rules.md, spec-driven-development and maintainer-style references, relevant Wyrd design/doctrine and Bifrost tenant/file/read-audit boundaries. No CodeGraph index exists.

This report covers authenticated tenancy, footer proof, signed scan authority and error/audit propagation. TASK-006 changes were considered only for interactions with these boundaries. Maintainer exclusions stand: FIND-007-3 unchanged, Postgres tenant columns excluded, canonical error is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`; the known lost-Scribe deadline behavior is not a finding.

## Boundary coverage and source evidence

| Boundary / obligation | Producer-to-consumer evidence | Assessment |
|---|---|---|
| Query tenant comes from authenticated authority | `oracle/mod.rs:285` `AuthorizedQueryContext::try_new` refuses disagreement with the principal; leader published footers and hot metadata keys use that context or its resolved cut binding, rather than trusting the removed row column | PASS |
| Staged and assembled objects carry their owning tenant | `scribe/parquet_writer.rs:388` checks binding, seal tenant and frozen seal key, including table identity, before encoding; ArtifactPlan supplies the tenant to `BifrostFooterIdentity::new` at line 603. `staging_runtime.rs:722` gives assembly the binding tenant; `claim_assembly.rs:266` passes it into the artifact plan. `parquet/footer.rs:100` stamps schema/object/tenant metadata | PASS |
| Forge rewrites retain tenant binding | Both managed executor configurations pass `self.binding.tenant` (`forge/managed/executor.rs:202,289`) through existing table policy to `parquet/writer_properties.rs:128` footer metadata; the physical table identity comes from the same binding | PASS |
| Every opened published/hot/staged file is proven before decoding | Published loader checks after every metadata cache load at `oracle/exec.rs:1040`, including cache hits. Published execute refuses absent loader (`1569`) and start_stream refuses key metadata that could bypass it (`1431`). Hot/staged shared stream calls `tenant_proven_reader_metadata` before builder/selection/decode (`2961`), comparing with the authenticated metadata-key tenant. The same pure comparison is used by both paths (`1115`) | PASS |
| Missing, valueless, duplicated or foreign footer cannot pass | `parquet/footer.rs:46` exact tenant comparison and `footer_value` at line 152 require exactly one valued field; missing metadata and duplicates refuse. No legacy-file fallback exists. Error is typed QueryTenantInvariant before a row from the opened file can be produced | PASS |
| In-memory live rows remain seal-bound | `scribe/tail_rpc.rs:620,677` scopes both memtable snapshot and staged lease by request binding tenant/table. Scribe resolver derives binding from authenticated assignment before snapshot; `oracle/follower.rs:803` staged keys use that assignment tenant. Memory projection removes the obsolete row field without changing cohort ownership | PASS |
| Distributed scan closure stays authenticated after tripwire-codec deletion | `peer_service.rs:287-340` obtains tenant from verified peer ticket, checks assignment tenant/table, fences, protocol, plan and manifest digests, then recomputes assignment-authority digest before IO. `follower.rs:1580-1630` checks physical fingerprint, exact authenticated fences/reservation/binding, unique assignment identities and encoded scan set. Required columns/predicates remain signed and schema-checked before live-tail open; published followers install authenticated PublishedFooters (`follower.rs:550`) | PASS |
| Tenant refusal remains terminal and audited at leader | `peer_service.rs:671` preserves typed tenant failures as DispatchError::TenantInvariant. `live.rs:664,678` never classifies these as availability; it returns the typed query error. `oracle/mod.rs:2486` wraps returned query streams, stages the first refusal once with verified context and TenantFile, and passes error unchanged. `query_audit.rs:148` uses existing tracked canonical tenant append; failures are logged/counted, matching architecture's non-blocking exception | PASS |
| Final scan-metric delta does not weaken peer/file authority | `live.rs:773` reconciles native fingerprint/rows/bytes and remote schema/completion/fingerprint/digest/counts before recording statistics (`792,837`); completed decoder refuses later frames. Only telemetry accumulation changed. No footer proof or assignment check was removed by this delta | PASS |
| Process harness deletion and HTTP URL correction do not replace production security boundaries | Removed test-only process surfaces do not own production ticket verification, table bindings, footer readers or audit. Existing server/PeerCluster journeys remain. Client default change to 8080 affects endpoint discovery; authentication/transport authorization behavior is unchanged | PASS |

Removing the per-row filter/tripwire and its codec/audit ownership is required by REQ-015, not an unsupported loss of a guard: the authenticated binding survives independently in signed assignments and scan owners. Footer verification belongs at the shared file-open boundary, including cached metadata; no repeated downstream row guard is required.

## Verification evidence and limits

Static inspection only: no builds, cargo, nextest, mise, tests, benchmark, commits or source changes. Exact-tree runtime results are furnished by the user, not independently rerun: Oracle journeys 40/40, server 26/26 and Python/TypeScript passed; final oracle live/follower 19/19, client 314/314 and testing 61/61 passed; lints/fmt/codegen clean. The aggregate's sole failed process-harness test has been removed with that harness; this does not constitute fresh runtime proof of the final tree.

Inspected focused footer/hot refusal tests and distributed journey source (`crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:301-334`): a foreign footer under COUNT(*) must produce zero rows and the exact QueryTenantInvariant outcome, covering the path that previously depended on a hidden row field. Retained public-role journey `tests/bifrost/server/query.rs:927` exercises granted table scopes. Native and wire decoder tests cover completion tampering; final staged-byte test verifies both completion forms. Source and furnished results support the domain obligations; capacity measurement remains caller-owned.

R2 only normalizes prior evidence whitespace and has no security/runtime effect; substantive gate closure belongs to the standards/final validation reports. No new tenancy/security regression or bounded domain correction was identified.

## Findings

None. **PASS**.
