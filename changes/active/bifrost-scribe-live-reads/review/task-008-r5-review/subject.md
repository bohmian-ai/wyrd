# Immutable review subject

Candidate HEAD: 1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027. Parent ca99db0af5a0d898ef67834699405c1c73719f56. Cumulative TASK-007 base a7582db587c6170a290760f1741673125612b797; TASK-008 base f7bebf704d6f3b1dd20d041e70c6ca512c0da307.

Spec: changes/active/bifrost-scribe-live-reads/spec.md revision 20. Original tasks: tasks/TASK-007-one-parquet-scan-for-live-reads.md and tasks/TASK-008-tenant-proven-per-file.md in that packet. Prior verdict, findings-validation and TASK-008-R5 remediation/evidence: review/task-008-r4-review/. All prior remediation inputs remain in the packet. Read these and complete cumulative.diff and correction.diff; do not treat the navigation as complete coverage.

This is a confirmation review: confirm FIND-007-10/11/12 closure; reassess cumulative original task and preserved prior closures. The new maintainer decision removes shutdown residue publication: audit correctness regressions only (durability, replay, restart publication, leftover references). Do not challenge that decision. Other fixed decisions: FIND-007-3 unchanged; Postgres data_tenant_id columns out of scope; error WYRD_VALA_500_QUERY_TENANT_INVARIANT; no live-read cap.

STATIC ONLY: no cargo, nextest, mise, builds, tests, benchmark or commits. Write only assigned review report. Source immutable. No .codegraph directory exists.

## Starting navigation

- server app/server.rs BoundServer peer-task runner selection; grpc/mod.rs role composition; app/supervise.rs transport-before-role stop; bifrost state.rs role ownership/drain.
- wyrd-tonic server/mod.rs serve_grpc_with_listener, serve_peer_grpc_with_listener, StoppingIo IO methods and Connected; mTLS connect identity and public listener.
- vala-bifrost-redux scribe/mod.rs ScribeImpl shutdown, close_lanes, startup/recovery and publish tick; persistence.rs drain, publish_residue, restored staging; assembly.rs staged claims; WAL and publication fences.
- TASK-007 shared HotParquetExec scan, oracle dispatcher remote status mapping; TASK-008 per-file footer tenant proof, memory seals, assembly and Forge producers, no row data_tenant_id.
- tests in wyrd-testing oracle/distributed and peer_network/analytical, Scribe restart/replay/publication and server peer harness; manifests/mise.toml/TESTING.md for evidence interpretation.
- Authorities: AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, bifrost-design.md, references/README.md, languages/spec-driven-development.md and maintainer-style.md; applicable routed references.

## Supplied verification (not rerun)

R5 task appended exact classifier (1 pass) and three focused remote footer/lost-Scribe/window-blocked recipes (3 pass); fmt and lints exit zero; server peer 11/11. User supplies both git diff --check bases green. New shutdown decision evidence: 100M-row benchmark 5 files mean 487 MiB, clean stop 0.6s; redux integration 872/872; Scribe journeys 20/20; server journeys 26/26. Prior packet contains attributable broader and RED/GREEN evidence. Do not invent fresh runtime verification.
