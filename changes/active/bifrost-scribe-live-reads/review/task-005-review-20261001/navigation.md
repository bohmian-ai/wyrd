# TASK-005 review navigation

- Immutable subject: `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2`; exclude `1f1cbcf5f`.
- Authority: user-supplied original task at `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (revision 14); current approved `changes/active/bifrost-scribe-live-reads/spec.md` (revision 24); `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and applicable references selected through `architecture/references/README.md`. The candidate's in-repo task differs; evaluate the user-supplied original and record any authority conflict.
- Gate write/query owners: `crates/vala/vala-bifrost-redux/src/gate/{mod,error}.rs`; transport failure edges: `crates/wyrd/wyrd-server/src/{http,grpc}/otlp.rs`. Trace callers from OTLP through Gate and Scribe/Oracle.
- Scribe owners: `src/scribe/{telemetry,ingress,assembly,staging_runtime,execution_lanes,memory,shards,persistence,wal,mod}.rs`; relevant test consumers in `wyrd-testing/tests/bifrost/scribe/{telemetry,write_read}.rs` and `scribe/staging_runtime.rs` tests. Trace restore, claim, settlement, insert, freeze, publication, and readiness.
- Oracle owners: `src/oracle/{mod,telemetry,admission,dispatcher,exec,query_stream,pruning,planner,live,follower}.rs`; test consumers in `wyrd-testing/tests/bifrost/oracle/{published,capacity,peer_network/analytical}.rs`. Trace queue to admission to stream terminal and peer work.
- Shared storage owners: `src/storage/{mod,telemetry,cache}.rs`; trace logical cached requests versus backend I/O and remaining metric consumers.
- Forge owner: `src/forge/worker.rs`, catalog settlement in `src/catalog/{bifrost_catalog,iceberg_storage}.rs`; test consumer `wyrd-testing/tests/bifrost/forge/live_rewrite.rs`. Trace committed settlement and recovery paths.
- Other consumers: `wyrd-testing/src/bifrost/{telemetry,scribe_workload,cluster}.rs`, server metrics registration, benchmark support, architecture and docs. Task evidence lists exact commands and captures; validate claims against source.
- `.codegraph/` is absent, so use repository navigation and `rg`.

This map is only a starting point. Every reviewer must expand its own source and caller coverage.
