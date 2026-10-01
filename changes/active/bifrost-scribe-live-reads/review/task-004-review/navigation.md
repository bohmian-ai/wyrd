# Navigation map (starting point only; expand and verify against source)

Code diff: 434 files, +32.6k/-53.9k (excluding changes/). Commit list: `git log --oneline a56ab7569..990803fc0`.

- Eval replay (Scenario 1): wyrd-server/src/verification/{eval.rs,observations.rs,runner.rs,results.rs};
  redux gate/mod.rs ObservationAck / first_commit; wyrd-server boot/mod.rs compose_bifrost; app/server.rs worker;
  wyrd-client observe; sdks python/ts eval; tests wyrd-testing/tests/bifrost/server/eval_verification.rs.
- Boot/peer (Scenario 2): wyrd-server boot/{mod.rs,data_root.rs}, app/peer_plane.rs, components/health readiness,
  oracle/{peer_service.rs,peer_authority.rs,lifecycle_service.rs}; deleted peer_keyring.rs, grpc/peer_auth.rs;
  wyrd-spec vala/api.rs PeerContext; tests oracle/peer_network/{join,security,analytical,listener,support}.rs, peer_cluster.rs;
  mise test:server:peer.
- Scribe WAL role-local failure: redux scribe/wal.rs, scribe/mod.rs role lifecycle, BifrostResourceHealth; server readiness;
  test server::owner_inspection::scribe_wal_fault_is_role_local.
- Memory (Scenarios 3-4, R13): redux resources.rs (BifrostResourceGovernor, MemoryCgroup, follower_memory_pool),
  wyrd-server config.rs (server_memory_min_bytes), gate/limits.rs transport body owner, forge/managed/{executor.rs, memory.rs deleted},
  scribe/{admission,contention(deleted),geometry,ingress,material_plan,persistence,parquet_writer,memory,member_stager,shards}.rs,
  server otlp_decode.rs/otlp_json.rs/grpc/otlp.rs/http/otlp.rs, storage/{mod.rs,policy.rs,cache.rs}.
- Public error (Scenario 5): wyrd-spec vala/{error.rs,api.rs} QueryResourcesExhausted; redux oracle/{mod.rs,query_stream.rs};
  server query/routes.rs, grpc/query.rs query_status retry-after; http/error.rs; wyrd-client error.rs, bifrost/query.rs; SDK projections.
- R13-C/D, D10-D13: storage/mod.rs governed_decode/attempt_once; oracle/{dispatcher.rs,mod.rs,analytical.rs,admission.rs};
  proto ReserveNodeSlotsRequest; QueryExecutionPath -> QueryClass; OracleAudit synchronous.
- D15/D16: resources.rs cgroup walk; app::server check_age/publish_due loops.
- D19: wyrd-sql tenant_conn.rs begin order; scripts/check_tenant_isolation.py exemption; architecture/v1/00-foundations/sql-foundation.md.
- Docs: architecture/bifrost-design.md, architecture/references/domain/*, docs/src/content/docs/** (operator settings, errors).
