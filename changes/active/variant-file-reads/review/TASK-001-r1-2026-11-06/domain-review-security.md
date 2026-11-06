# Security domain review — TASK-001

Result: **FAIL** (one proposed bounded validation finding, SEC-001).

Subject: `5234961d51798581a026834cfffd5efaadc226b4...9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`. HEAD and the spec SHA-256 still match `scope.md` at completion; the task matches its recorded supplemental-evidence hash `dc933d52d5111145011fd431ecc2a88c2b29c54d0006f69e20987eb8103bdf9b`. This is a static review; no builds or tests were run and no reviewed source was edited.

## Reviewed boundary and authority

Reviewed executable expressions carried by live placeholders through fragment authority, receiver preflight, source resolution, captured memory execution and staged siblings, and returned logical schemas. Authority: root AGENTS.md; the task-review skill; approved spec revision 2 REQ-001/002/004 and task Scenario 1; `architecture/wyrd-security-posture.md` sections Peer identity and distributed Oracle and Tenant and data isolation; `architecture/bifrost-design.md` Immutable planning and source authority, Variant SQL, Nested-field reads, Resource and failure invariants. No nested AGENTS.md exists under the changed Vala paths. The full diff was inspected, including test/support changes; source outside that diff was followed where it owns authentication, tenant binding, footer proof, or expression decoding.

| Obligation | Source evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Requirements are covered by existing fragment and assignment authority before execution | `live.rs::fragment` encodes requirements before fingerprinting; `dispatcher.rs::peer_ticket_claims` binds fingerprint and assignment digest; `peer_service.rs::ScribeFragmentExecutor::execute` checks claim binding and recomputes assignment digest; `follower.rs::preflight` recomputes plan fingerprint and validates exact scan set/fences/tenant/table | Existing malformed-request matrix; new requirement propagation test; no execution performed | PASS |
| Off-closure requirements and schema mismatches refuse before tail/source IO | `signed_closure_schema` checks the authenticated full schema; `signed_requirements` attempts requirement validation before `tail.open`, but does not validate all expression column references and ignores filter type errors | `learned_variant_requirements_reach_memory` covers a bare invalid output, nonboolean valid filter, and misaligned names; omits invalid filter index and invalid column nested under a boolean expression | FAIL: SEC-001 |
| Local and remote paths share enforcement, without a new public query selector | Peer RPC and `OraclePeerTransport` implementations both invoke `ScribeFragmentExecutor::execute`; requirement structs remain in runtime crate/private physical codec; no public wire/SDK/schema/manifests changed | Added mixed and role-separated journey variants, permission refusal assertion | PASS |
| Source tenant/table/query/capture/generation and schema authority preserved | Resolver checks writer epoch, schema fingerprint and closure before open; `tail_rpc.rs::open_capture` matches stream, query, binding and unexpired retained capture; `memtable.rs::LiveCapture::take/read` selects tenant/table cohort; decode independently compares full fingerprint and derived output schema | Capturing-tail tests and existing capture/refusal tests inspected | PASS except malformed-expression validation above |
| Staged sibling retains owner-side file IO, lease and tenant proof | `live_leaf` retains returned staged lease and wraps the existing `hot_parquet_scan` with exact filter/output operators; `exec.rs::hot_parquet_scan` installs `TenantParquetReaderFactory`; footer validation remains in `TenantParquetReader::get_metadata`; no leader remote-footer path added | Existing staged source tests inspected; no runtime execution performed | PASS |
| Deadline/permission/cancellation authority not widened by transport changes | Existing dispatcher claims carry permission digest and bounded deadlines; follower paths still use the same admitted grant/capture context; leader `LiveFragmentRead::into_stream` selects cancellation/deadline; `open_capture` rejects expired captures | New journey refuses caller without query permission before fragments and checks cancellation release | PASS for authority; eager allocation/cancellation effects belong to selected invariant/system review |

## Security Audit

### Critical

None proposed.

### High

None proposed. The supported threat model trusts cluster-member processes authenticated by private mTLS and explicitly excludes a compromised member with the shared peer key. Fragment claims are not independently cryptographically signed in `OraclePeerAuthority::verify_before_decode`; that existing design is not introduced or weakened by this change. No unauthenticated/public caller path to arbitrary physical expression bytes was found.

### Medium

- **SEC-001 — INCORRECT: an out-of-closure expression can reach the tail before refusal.** Location: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:150`, particularly lines 160–164 in `signed_requirements`. Violated obligation: task Scenario 1 explicitly requires assignment/schema mismatch refusal before source IO, and the task requires validated source expression requirements; the function also promises to refuse a filter/output naming anything outside the authenticated closure.

  Source-proven path: use an otherwise authenticated assignment whose closure has two fields and a payload filter `Column("outside", 2)` with empty outputs. The pinned fork at `5f4b171` constructs `Column` from protobuf without schema checking (`datafusion/physical-expr/src/expressions/column.rs::try_from_proto`). `Column::data_type` returns an index error, but `is_ok_and` converts that error to false, so the refusal branch is skipped. Empty outputs plan successfully. Resolver then calls `tail.open` at `follower.rs:757`, and `open_capture` reads the retained source before memory evaluation can fail. An empty retained source can return successfully without ever evaluating the invalid filter. A boolean wrapper such as `IsNull(Column("outside", 2))` also survives even if only the ignored-error issue is fixed: pinned `IsNullExpr::data_type` returns Boolean without consulting its child. The same nested invalid column can occur in an output, because `outputs.project_schema` is not a recursive column validator.

  Consequence/materiality: malformed executable requirements cross the exact pre-source validation boundary this task promises, touching retained source state and potentially staged file metadata before discovering the mismatch; empty sources can accept the invalid requirement entirely. This is a bounded trust-boundary acceptance failure, not demonstrated cross-tenant disclosure or a public attacker exploit. It is retained because pre-IO refusal is an explicit acceptance obligation and correction is local and small, rather than as general hostile-peer hardening.

  Smallest correction: keep validation in the existing `signed_requirements` owner before `tail.open`; validate every referenced column in filters and outputs against the authenticated closure using existing DataFusion column traversal (`physical_expr::utils::collect_columns`) and bounds/type checks, and propagate filter `data_type` failure instead of accepting it. Keep exact output schema derivation, fallback and signature/assignment machinery unchanged. A new planner, transport, authority API or repeated consumer guard is unnecessary. Inspect this complete helper and both callers (`ScribeTailResolver::resolve` and post-resolution `PhysicalPlanFollower::decode`) when applying the correction.

  Focused closure proof: extend the existing capturing-tail requirement test with (1) out-of-range bare filter plus empty outputs, (2) out-of-range column under a boolean null-test filter and (3) the same null-test output. Assert resolution refusal and unchanged recorded tail-open count, including a source that would return no batches. Retain the valid reordered/repeated outputs and zero-column COUNT assertions. No broad new harness is needed.

### Low / Defense In Depth

No additional recommendations; unrelated or speculative hardening was excluded.

### Positive Controls

- The exact expression bytes are included in the plan fingerprint and checked against peer-context fragment identity; assignment closure/cut/schema authority is separately recomputed before source IO.
- The follower derives closure and output schemas from its authenticated catalog schema, then independently revalidates returned provider schemas; callers cannot supply a replacement schema.
- Capture lookup remains bound to query, tenant/table, stream and expiry; physical memory projection is limited to required fields, so the malformed-index gap above does not create access to arbitrary omitted table columns.
- Staged files still use tenant-verifying readers and retained leases; narrow projection and COUNT do not replace footer verification with exact merged statistics.
- Existing output framing checks plan identity and terminal completion; requirements do not add another source tier or public query surface.

## Coverage and verification limits

Inspected changed production owners `oracle/{live,codec,follower}.rs`, `scribe/{memtable,tail_rpc,shards}.rs`, persistence caller updates, distributed learned-memory journey, peer scalar-subquery assertion and moved query rendering support. Followed unchanged `oracle/{dispatcher,peer,exec}.rs`, server `oracle/{peer_service,peer_authority}.rs`, retained-capture selection and pinned DataFusion expression decoding/column/null-test implementations. No dependency, persisted format, SQL, public SDK or public protocol changes occur in the reviewed range.

No tests, fuzzing, actual peer traffic, or runtime allocation measurements were performed. Recorded implementation test pass claims were not treated as independently executed evidence. Eager evaluation now happens inside capture reads; the root orchestrator was notified to reconcile resource accounting and cancellation behavior with the selected invariant/system specialists rather than duplicate that boundary here. Source correctness of SEC-001 is static and requires the focused test above for executable closure evidence.

The supplemental independent scalar-subquery diagnosis was consumed and compared with the codec/producer/test source: physical-plan Debug no longer reveals filter literals because they reside in opaque extension bytes; the replacement decodes the same placeholder and checks its filter. This changes observation without weakening the scalar literal/result assertions. The diagnosis does not affect SEC-001 or establish runtime verification of it.

Skipped unrelated pre-existing peer trust redesign and whole-repository dependency auditing. Risk: malformed expression pre-IO refusal remains incomplete pending independent validation and correction of SEC-001.
