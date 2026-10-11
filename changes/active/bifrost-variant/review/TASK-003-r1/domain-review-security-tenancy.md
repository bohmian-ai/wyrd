# TASK-003 r1: security and tenancy domain review

Reviewer: `domain-rev` (security and tenancy), independent, no source changes.
Subject: base `b145767dd` to candidate `861a0342f` on
`wyrd/bifrost-variant/TASK-003`.

## Reviewed boundary

I traced each hop from the source and checked what it relies on:

1. **Analytical ingress.** `AnalyticalStageIngress::authorize_stage_message`
   (`oracle/analytical.rs:1669`) checks the stage authority and the framed
   message digest (`peer::stage_body_digest`) before any decode. The graph key
   comes from the verified claims (`attempt_key`, `analytical.rs:3026`), and
   `StageWireIdentity::to_binding` (`analytical_transport.rs:598`) requires the
   header-echoed tenant and graph to equal the claims. The first message fixes
   the graph's authority (`GraphLeaseBinding::activate` and `authorize`,
   `analytical.rs:854` and `:899`). Every later message must carry the same
   tenant, query ids, snapshot, permission digest, deadline, reservation, and
   cut. `AnalyticalStageEgress::record` (`:665`) stores the tenant from those
   same verified claims.
2. **Worker session and plan rewrite.** `AnalyticalSessionBuilder::build_session_state`
   (`analytical.rs:498`) resolves the graph from headers. It refuses an
   unregistered graph or one with no recorded egress identity. It installs
   `TenantWarehouseReads` as the `WorkerPlanRewriteHandler`, using the egress
   tenant. Upstream (`datafusion-distributed` `0b81b12`,
   `src/worker/impl_coordinator_channel.rs:78-83`) decodes the stage, runs the
   rewrite chain, and only then registers the plan. A handler error aborts
   registration.
3. **Footer proof.** `TenantWarehouseReads::bind` (`exec.rs:946`) rebinds every
   `DataSourceExec`/`FileScanConfig`/`ParquetSource` to
   `TenantParquetReaderFactory`. It refuses any non-Parquet file source and any
   `ObjectStoreUrl` other than the local placeholder. `TenantParquetReader::get_metadata`
   (`exec.rs:875`) calls `verify_scanned_footer_tenant` on every open, including
   cache hits, because the cached factory is the inner reader. Row counts are
   `Inexact` (`exec.rs:1267`), so neither `count(*)` nor event-time min/max can
   be answered from statistics without opening the file. I checked this against
   the fork's `Max::value_from_statistics`, which needs an exact `num_rows`.
   Published files with `key_metadata` are still refused (`exec.rs:1209`).
4. **Leader-side refusal audit.** `audit_tenant_refusal` (`oracle/mod.rs:2390`)
   wraps both the Interactive and the Analytical result streams
   (`mod.rs:2328`, `:2365`). `QueryCatalogError::find` now also reads a
   forwarded Parquet-external leaf (`mod.rs:3842`). A worker footer refusal
   therefore stays `QueryTenantInvariant` and emits one `tenant_file` security
   event.
5. **Live Scribe fragments.** `LiveScribeExec::fragment` (`live.rs:272`)
   encodes `FilterExec(pred, RemoteSourcePlaceholderExec)`, and
   `physical_plan_fingerprint` covers those bytes. On the Scribe,
   `ScribeFragmentExecutor::execute` (`wyrd-server/src/oracle/peer_service.rs:243`)
   checks the peer context, the protocol version,
   `claims.fragment_digest == request.plan_fingerprint`, the tenant and table
   binding, and the v9 assignment-authority digest. Follower `preflight`
   (`follower.rs:839`) recomputes the fingerprint over the received bytes and
   allows only `Filter` among non-leaf operators (no joins). It then resolves
   the staged provider (behind `TenantParquetReaderFactory`, `exec.rs:1044`),
   decodes, and runs `FilterPushdown`.
6. **Deletions.** I checked each deletion against the spec:
   - `ScanPredicate`/`ScanLeaf`/`ScanLiteral`, their proto messages, and
     `private_conversion`: deleted by revision 21. The proto fields are
     `reserved`, and the digest domain moved to `assignment-authority.v9`.
   - Persisted follower assignments (`PersistedFileAssignment`, file
     descriptors and their `is_valid`): deleted by revision 21. The old
     zero-sha cache-key guard served only the deleted custom cache. DataFusion's
     `FileMetadataCache` keys by object path, and every hit is still
     tenant-proved.
   - Wyrd event-time pruning: replaced by `FilePruner` and `PruningPredicate`.
     No security role.
   - `ORACLE_VARIANT_SQL_VERSION`: deleted from both peer digests and not
     replaced. See SEC-1.

## Authority and source coverage

- **Authority:**
  - `spec.md` (working tree): Oracle registration and distributed wire,
    INV-006, revision 21 history.
  - `spec-revision-26-datafusion-variant.md`.
  - `TASK-003-native-datafusion-read-path-plan.md`: step 3 items 2, 4 and 6,
    and the "Items 2+4+7 design" notes.
  - `AGENTS.md`: tenant separation and audit rules.
  - `architecture/bifrost-design.md`: Variant SQL and peer agreement
    paragraph.
- **Source in the candidate:**
  - `oracle/{analytical,analytical_transport,exec,live,follower,peer,codec,session,mod}.rs`
  - `wyrd-server/src/oracle/peer_service.rs`
  - `wyrd-spec/src/vala/{assignment_authority,api}.rs`
  - `wyrd-tonic/proto/wyrd.v1.proto`
  - `catalog/iceberg_storage.rs` (`relativize`)
- **Fork source:** `datafusion-distributed` `0b81b12` (rewrite-handler call
  site and event chain, work-unit feeds) and the `datafusion` `a4ff14550`
  min/max statistics path.
- **Tests read (not run):**
  - `exec::tests::decoded_stage_scans_read_only_as_the_bound_tenant`,
    `hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row`, and
    `hot_scan_row_counts_never_answer_a_query_unopened`
  - `live::tests::pushed_filter_ships_on_the_fragment_and_stays_on_the_leader`
  - the journey `peer_network::security::leaf_predicates_round_trip_and_execute`:
    one-literal plan change, wrong protocol version, and an anonymous dial
    each leave the execution counter unchanged
  - the journey `published.rs::prove_worker_tenant_refusal` (Analytical
    worker refusal, problem intact)
  - the journeys `distributed.rs::remote_staged_footer_refusal_fails_closed`
    (exactly one `tenant_file` audit) and the foreign-footer leg of
    `prove_selective_predicate_pruning`

## Verification limits

- I ran no tests. I relied on the recorded candidate evidence in the
  navigation map (`test:bifrost` passes with exit 0, 53/53 Oracle journeys,
  and 174/174 `oracle::` unit tests).
- I did not confirm at runtime that a worker footer refusal on an Analytical
  stage produces exactly one `tenant_file` audit. No journey asserts the audit
  count on the Analytical worker path. The staged-run journey does, for the
  Scribe path. The Analytical path shares `QueryCatalogError::find` with the
  problem mapping that `prove_worker_tenant_refusal` asserts, so the source
  indicates it works.
- I did not review MCP or SDK authorization, admission policy, or Forge. The
  diff does not change those authorization paths.

## Answers to the brief

- **Can a decoded stage read another tenant's objects or an unproved file?**
  No. The tenant is fixed per graph from verified claims, and a later message
  cannot change it. Every Parquet scan in a decoded stage is rebound to the
  footer-proving reader before registration, and every other file scan is
  refused. Warehouse-relative paths do not resolve without the bound factory,
  so a missing handler would fail closed rather than read unproved data.
- **Can a follower execute a plan not covered by the signed digest?** No.
  Analytical stages are covered by the framed-message digest before decode.
  Live fragments are covered by `fragment_digest == plan_fingerprint`, with the
  fingerprint recomputed over the received bytes, and the pushed predicate is
  inside those bytes.
- **Did a deletion remove a check the spec still requires?** Yes, one:
  refusing a peer on a different version before decode (SEC-1).
- **Are refusals audited?** The footer tenant refusal is audited on the leader
  for both classes, which satisfies the tenant-tripwire rule. The new `bind`
  structural refusal surfaces as a generic plan error. The approved task does
  not require it to be audited, so I raise no finding on that.

## Findings

### SEC-1: MISSING. The peer contract no longer refuses a worker built on a different DataFusion fork revision

- **Obligation:** spec revision 26, "Oracle registration and distributed wire":
  "`ORACLE_VARIANT_SQL_VERSION` is deleted; the peer contract fingerprint
  includes the DataFusion fork revision, and a worker on a different revision
  refuses before decoding a plan." The governing plan repeats this in its
  Wyrd switch list ("the peer fingerprint carries the fork revision").
- **Location:**
  - `crates/vala/vala-bifrost-redux/src/oracle/peer.rs:312-320`
    (`stage_body_digest`)
  - `crates/vala/vala-bifrost-redux/src/oracle/codec.rs:25-33`
    (`physical_plan_fingerprint`)
- **Evidence:**
  - The diff removes
    `hash.update(super::variant_sql::ORACLE_VARIANT_SQL_VERSION.to_be_bytes())`
    from `stage_body_digest` and the same line from
    `physical_plan_fingerprint`, and adds nothing in their place.
  - No fork-revision constant exists anywhere in `vala-bifrost-redux`,
    `wyrd-server`, or `wyrd-spec`.
  - `PEER_PROTOCOL_VERSION` (5), `STAGE_PROTOCOL_VERSION` (2), and
    `ORACLE_PHYSICAL_CODEC_VERSION` (1) are unchanged.
  - `OracleCapabilitiesV1` and `ScribeCapabilitiesV1`, which the participant
    cut digests, carry no DataFusion revision.
  - `architecture/bifrost-design.md:566-567` now says peers agree "through the
    pinned fork revision". That describes the build pin, not a runtime check.
- **Observable consequence:** During a rolling upgrade, or in any mixed
  deployment of two builds on different fork pins, a worker or Scribe decodes
  and executes a `datafusion-proto` stage, or a pushed `FilterExec` predicate,
  produced by another revision. Examples are a changed `ParquetSource` proto
  field or different `variant_get`/`->>` semantics resolved by name. The
  result is a silent wrong result, or a late decode failure, where the spec
  requires a refusal before decode. The base-to-candidate transition happens
  to refuse only because the removed constant changed both digests. The next
  fork re-pin will not be refused.
- **Testable correction:**
  - Bind one constant naming the pinned DataFusion fork revision into both
    `stage_body_digest` and `physical_plan_fingerprint`, so both the
    Analytical stage path and the live fragment path refuse.
  - Add a unit test proving that a digest computed under a different revision
    value is refused before `physical_plan_from_bytes*` or the stage decode.
    In the follower, this is the preflight effect counters at zero decodes.
  - Add a check that ties the constant to the `datafusion` source revision in
    `Cargo.lock`, so a re-pin cannot leave it stale.

### SEC-2: MISSING. The worker rewrite's "any other file scan is refused" branches have no test

- **Obligation:**
  - Plan step 3 item 4 and the "Items 2+4+7 design" note: the
    `WorkerPlanRewriteHandler` binds decoded Parquet scans to the tenant
    footer proof, and "Any other file scan is refused."
  - `spec.md`, Oracle registration and distributed wire: the handler "refuses
    any decoded file outside the authenticated tenant's warehouse prefix
    before execution."
  - INV-006: every tripwire is kept.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:957-962`,
  the two `plan_err!` branches of `TenantWarehouseReads::bind`.
- **Evidence:**
  - `decoded_stage_scans_read_only_as_the_bound_tenant` covers only the
    accept path: a proven tenant reads, and a foreign tenant fails at the
    footer.
  - A search of `crates/` finds no test that builds a decoded stage holding a
    non-Parquet `FileScanConfig` (for example CSV or JSON) or a non-local
    `ObjectStoreUrl` and asserts that `bind` or `rewrite_worker_plan` refuses.
  - Nothing proves that the upstream chain turns that refusal into a stage
    that never registers.
- **Observable consequence:** These two branches are what keep a decoded scan
  from reading through the default factory with no footer proof. A regression
  that drops or loosens them would leave `test:bifrost` green. Oracle never
  plans such a scan, so no journey would exercise the branch, and a scan
  without proof would execute on a worker unnoticed.
- **Testable correction:** Add focused `exec::tests` cases. Each one
  round-trips a stage through `datafusion-proto`, as the existing test does,
  with:
  - a CSV or JSON `FileScanConfig`;
  - a Parquet scan whose `ObjectStoreUrl` is not the local placeholder.

  Each case asserts that `TenantWarehouseReads::rewrite_worker_plan` returns
  the plan error and that no reader is created (zero `create_reader` calls).

## Non-findings worth recording

- `AnalyticalSessionBuilder::detached_local_worker` wraps `Worker::default()`
  with no tenant rewrite. It is only reachable at `detached://oracle.invalid`,
  which no participant advertises, so it is unreachable as a task target. This
  is unchanged in kind from the base.
- `cache_footers` (`exec.rs:1435`) pre-reads published footers into the node
  cache before the scan's proof runs. No footer content reaches the caller,
  and every scan open re-proves the cached metadata. Tenant isolation holds.
- `published_files`' `assigned` branch and `PublishedAssignment` are now dead.
  No production caller passes `Some`. This belongs to the standards domain, not
  security.

## Result

**FAIL.** SEC-1 is a spec-required pre-decode version refusal that the
deletions removed without replacing. SEC-2 is a missing proof for a security
refusal that the plan requires. On everything else in the brief, tenant
binding, digest coverage, the footer proof, and the refusal audit hold.
