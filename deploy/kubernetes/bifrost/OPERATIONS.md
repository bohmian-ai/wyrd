# Bifrost Oracle activation and rollback

Keep the generated profile in `candidate` status until a maintainer records
approval. Apply storage and Postgres readiness first, then mixed pods, then
role-separated ingest/query pods. Verify `/readyz`, authenticated gRPC
ingest, authenticated HTTP query, terminal frames, audit rows, and telemetry
before shifting either Service selector. Roll back by applying
`rollback-mixed.yaml`, wait for both capability labels to become ready, and
repeat the smoke sequence. Reports and candidate profiles are retained under
`crates/wyrd/wyrd-testing/benches/oracle/`.

Every Bifrost pod sets `WYRD_BIFROST_DATA_DIR` to one mounted volume; the server
derives the Scribe WAL, node identity, staged members, and Oracle spill beneath
it and locks it exclusively at boot. Scribe-bearing targets run as a
StatefulSet with a per-pod volume claim so replicas never share a WAL identity;
Oracle-only pods may use a pod-local `emptyDir`. `WYRD_SCRIBE_WAL_DIR` is
rejected.
