# Bifrost Oracle activation and rollback

Keep the generated profile in `candidate` status until a maintainer records
approval. Apply storage and Postgres readiness first, then mixed pods, then
role-separated ingest/query pods. Verify `/readyz`, authenticated gRPC
ingest, authenticated HTTP query, terminal frames, audit rows, and telemetry
before shifting either Service selector. Roll back by applying
`rollback-mixed.yaml`, wait for both capability labels to become ready, and
repeat the smoke sequence. Reports and candidate profiles are retained under
`crates/wyrd/wyrd-testing/benches/oracle/`.
