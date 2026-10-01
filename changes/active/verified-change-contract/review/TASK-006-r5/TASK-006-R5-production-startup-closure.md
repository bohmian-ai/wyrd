---
id: TASK-006-R5
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 43
requirements: [REQ-153, REQ-155, AC-034, AC-035, AC-037]
depends_on: [TASK-006-R4]
parent_task: TASK-006
remediates: [FIND-TASK-006-20, FIND-TASK-006-27, FIND-TASK-006-28, FIND-TASK-006-29, FIND-TASK-006-30]
---

# TASK-006 R5 — Close startup security and load bounds

## Authority and subject

- Approved authority: [spec revision 43](../../spec.md), including edge-terminated public TLS and independent peer mTLS.
- Original task: [TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Prior remediation: [TASK-006 R4](../TASK-006-r4/TASK-006-R4-sql-and-deployment-closure.md).
- Diagnosis and evidence: [R5 verdict](verdict.md) and [validated ledger](findings-validation.md).
- Immutable reviewed range: `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`.

These five findings are the complete bounded correction set. Preserve the accepted Eval behavior, official image, owner migration lease, Oracle autoscaling, peer trust, and prior findings other than reopened FIND-20.

## Diagnoses and selected corrections

### FIND-TASK-006-20 — A redundant SQL handle remains on the migration path

**Diagnosis.** R4 moved migration lease acquisition onto the pre-existing `SqlStore`, then added public `From<PgPool> for SqlStore` at `crates/wyrd/wyrd-sql/src/lib.rs:298-303`. Production uses `SqlStore` only in the one-off `wyrd-server migrate`; its other uses are SQL tests and fixtures. The migration command already wraps that same database-owner pool in `OperatorPool` for post-migration verification. Keeping both wrappers adds no capability and leaves a forbidden raw-pool library entrypoint, even though the SQL and allowlist lanes pass.

**Correction.** Delete `SqlStore`, including its raw-pool conversion and migration convenience methods. Put lease acquisition on the existing `OperatorPool`; the **one-off migration command** constructs it from the database-owner DSN using the existing pool builder, while normal serving continues to construct its separate `OperatorPool` from `wyrd_platform_admin`. Adapt fixture and integration tests to the existing owner-pool setup and the same lease path, without adding another production wrapper or test-only migration API. Keep the detached, bounded owner-session lease across ordered migrations and post-validation. The owner credential must never enter a serving process.

**Acceptance and proof.** No `SqlStore` or new raw-pool production library signature remains. Serving still uses only its app and platform-admin credentials; the one-off migrator alone uses the database owner. The exact competing-migrator test still proves serialization, bounded waiting and retry. Run its focused `mise exec -- cargo nextest run --locked` command with the repository-managed Postgres wrapper, then `mise run test:sql`, `mise run test:server:startup`, and `mise run check:from-pools-allowlist`.

### FIND-TASK-006-27 — The production gateway hop has no transport protection

**Diagnosis.** `architecture/operations/deployment-and-release.md:27-32` requires authenticated, encrypted gateway-to-server traffic. The production guide at `docs/src/content/docs/self-hosting/kubernetes-production.svx:339-371` ends public TLS at ingress, routes HTTP and gRPC to nginx's plaintext pod ports `8080` and `50051`, and admits those ports from every source. The guide walk proved pod boot, not this network boundary. Client tokens and bodies can therefore cross an unprotected cluster hop, and direct in-cluster access bypasses the gateway.

**Correction.** In the existing production guide, make an existing authenticated, encrypted ingress-to-pod transport facility a prerequisite for both HTTP and public gRPC. Show its enforced policy, with the public-port NetworkPolicy restricted to the authenticated ingress path. A strict mesh mTLS setup is an acceptable native cluster mechanism. Keep public TLS termination at the edge, plaintext nginx-to-Rust loopback, and separate peer mTLS on port `50052`; do not restore public Wyrd certificate variables or use peer certificates for ingress.

**Acceptance and proof.** The rendered or applied production routing enforces that protected hop. A real HTTP request and public gRPC request through the edge succeed, while an ordinary pod cannot directly reach the public pod ports. Run `mise run docs:check`; a boot-only guide walk is insufficient.

### FIND-TASK-006-28 — Production Postgres examples do not verify server identity

**Diagnosis.** The same operations authority at lines 42–43 requires verified certificates and an explicit trust root for external Postgres. The guide's owner, app and platform-admin DSN examples at `kubernetes-production.svx:70-72,94-97` provide bare URLs; `wyrd-sql/src/dsn.rs:113-116` parses but does not demand certificate verification. An operator can follow the guide while sending all three logins to an unauthenticated database endpoint.

**Correction.** Document certificate-verifying Postgres DSNs and the trusted provider CA location for **all three** roles using existing Postgres/SQLx TLS options. Add a connection check that fails for a mismatched server certificate. Keep local development DSNs and peer certificates separate; add no Wyrd-specific database TLS knob.

**Acceptance and proof.** Follow the documented three connections against a certificate-valid Postgres endpoint and prove a wrong-server-certificate case fails; run `mise run docs:check`.

### FIND-TASK-006-29 — Development second-tenant example exposes the root credential

**Diagnosis.** `kubernetes-development.svx:208-218` passes a live `WYRD_PLATFORM_CREDENTIAL` as a `kubectl exec` command argument for second-tenant setup. Kubernetes API request/audit records may retain that argument. The R3 credential journey and REQ-155 confine first-use disclosure to the invoking terminal; the guide's current reuse step undermines it.

**Correction.** Replace that one command with the existing authenticated `wyrd platform tenant create` CLI on the operator machine through the guide's port-forward, reading the credential from the operator's local secret environment. Preserve the one-time first-use terminal output; do not add a new setup mode or Kubernetes secret handoff.

**Acceptance and proof.** Following the guide creates the second tenant, and no root credential appears in a `kubectl exec` request or pod log. Run `mise run docs:check`.

### FIND-TASK-006-30 — The kind load Job cannot enforce its concurrency cap

**Diagnosis.** AC-037 requires bounded read rate, concurrency, duration and cleanup. `deploy/kubernetes/kind/load.yaml:35-43` backgrounds the complete `jobs` check and `read_once` conditional, so the check executes in a child shell that cannot see the parent's active reads. A passing kind HPA run therefore does not prove the eight-request concurrency limit; slow responses can accumulate beyond it.

**Correction.** Keep the existing Job, rate, curl timeout, duration and cleanup, but perform the active-job count in the parent shell and background only `read_once`. Do not add a load framework, second kind journey or server metric.

**Acceptance and proof.** A small shell check with held reads shows at most `MAX_INFLIGHT` active requests. Rerun the one `mise run test:server:kind` journey to retain the image, Oracle HPA 1→2, peer join and remote-read evidence.

## Constraints and verification

The corrections must preserve revision 43's edge public TLS contract, the official image's nginx-to-Rust loopback, dedicated peer CA/mTLS, owner-only migration credentials, RLS, audit, Oracle-only HPA and bounded local test duration. They introduce no new Wyrd route, certificate variable, peer ticket, image, controller, test harness or published-image requirement.

Record focused proof for each finding above. Every specifically named Rust test in the implementation evidence must include and run its exact `mise exec -- cargo nextest run --locked` command with the owning setup wrapper. Then run `mise run test:sql`, `mise run test:server:startup`, `mise run test:server:kind`, `mise run docs:check`, `mise run fmt`, `mise run lints`, `mise run check:from-pools-allowlist`, and `git diff --check`. Run `test:server:peer` if peer behavior or admission changes. The next task review must reassess the complete original base-to-candidate range.

**Status:** READY for `$wyrd-implement`. No spec revision is required for these bounded corrections.

## Implementation evidence

Candidate: `vcc/task-006` at `21030c17a` (commits `40661a40f`, `e112409be`, `433f5a8ab`, `0b7f9b69d`, `f68e6a8da`, `413c50160`, `683e89987`, `bb0d2b99e`, `dcf1321aa`, `21030c17a`). Image for the kind lanes and the guide walk: `sha256:4f1b645befa38935a1bfac34dad51da5fd51199c59e93a14d0c7afb112d0f94a`, built from `dcf1321aa` (later commits change only docs).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-20: no `SqlStore`; migrator alone uses the owner DSN; lease still serializes, bounds waiting and retries | `SqlStore` deleted; `OperatorPool::migration_lease` (`crates/wyrd/wyrd-sql/src/lib.rs`); `wyrd-server migrate` wraps its owner pool in `OperatorPool` (`crates/wyrd/wyrd-server/src/main.rs`) | `git grep SqlStore` finds nothing; focused lease test PASS; `test:sql` PASS (166, 6, 113 and 2 tests); `test:server:startup` PASS; `check:from-pools-allowlist` PASS | PASS |
| FIND-27: encrypted, authenticated edge-to-pod hop; real HTTP and public gRPC through the edge; an ordinary pod cannot reach the public ports | Production guide: Istio sidecars, STRICT `PeerAuthentication`, `AuthorizationPolicy` admitting only the edge identity on 8080/50051, NetworkPolicy, TLS Gateway; public gRPC routed by `HTTPRoute` (`21030c17a`) | Guide walk: readyz 200 on both hostnames through the edge; `kind_seed` (gRPC ingest plus HTTP read) PASS through `https://wyrd.localhost`; the anchor sidecar records edge→anchor gRPC 200s over `mutual_tls` = 1 and requests without mTLS = 0; ordinary pod to 8080 and to 50051 gives `000` (timeout); an unmeshed pod carrying the gateway's label gives `000` (reset); 40 reads through the query edge; `docs:check` PASS | PASS |
| FIND-28: verify-full DSNs and the CA for all three roles; a wrong certificate fails | Guide step 1: `?sslmode=verify-full` URLs, `PGSSLROOTCERT`, Secret `wyrd-db-ca` mounted in migrate, core and Oracle | Walk: `wyrd_owner\|t`, `wyrd_app\|t`, `wyrd_platform_admin\|t`; wrong CA gives `certificate verify failed`; wrong host gives `does not match host name`; migrate Job with the wrong CA fails with `UnknownIssuer`, and with the provider CA gives `migrated and valid`; the anchor and Oracles serve on verify-full | PASS |
| FIND-29: second tenant with no root credential in `kubectl exec` or pod logs | Dev guide: `wyrd platform tenant create` through the port-forward, with the credential read into the local environment | Walk: tenant `beta` created; the only exec was `wyrd-server setup --tenant acme`; the pod log contains neither credential; `docs:check` PASS | PASS |
| FIND-30: the load Job's in-flight cap holds | `deploy/kubernetes/kind/load.yaml` counts jobs in the parent shell (`jobs >/dev/null; jobs -p`) and backgrounds only `read_once` | Held-read check in the curl image: old loop reached 20 in flight, new loop at most 8; `test:server:kind` PASS (Oracle HPA 1→2 at 12969m reads/s, peer join, remote Scribe tail over mTLS) | PASS |

### Defects found by the walk and fixed

1. **Iceberg catalog SQLx had no TLS backend** (`f68e6a8da`).
   - **Symptom:** the anchor exited on boot against verify-full Postgres: `iceberg catalog error … TLS upgrade required by connect options but SQLx was built without TLS support enabled`.
   - **Evidence:** anchor boot trace in the guide walk. `iceberg-catalog-sql` pins sqlx 0.8.1, and `vala-bifrost-redux` declared that copy (`sqlx_catalog`) with no TLS feature.
   - **Cause:** that separate sqlx build could not negotiate TLS at all. `sslmode=prefer` silently used plaintext; `require` and `verify-*` failed.
   - **Fix site:** add `tls-rustls-aws-lc-rs` to `sqlx_catalog` in `crates/vala/vala-bifrost-redux/Cargo.toml`. The user approved it.
   - **Test:** `catalog_sqlx_negotiates_tls` fails without the feature and passes with it. The workspace hack was regenerated (`413c50160`).

2. **The SDK's gRPC client had an empty trust store on `https://`** (`bb0d2b99e`).
   - **Symptom:** through the TLS edge, HTTP returned 200, but `Bifrost::connect_with_table` failed with `TransportDown { transport: "grpc", message: "transport error" }`.
   - **Evidence:** `crates/shared/wyrd-client/src/transport/grpc.rs` used `ClientTlsConfig::new()` with no roots. In tonic 0.14.6, `service/tls.rs:83-107` loads native roots only after `with_native_roots()`. `openssl s_client` with the edge CA verified the same listener.
   - **Cause:** every public `https` gRPC dial from the Rust, Python and TypeScript SDKs failed verification. The lanes use plaintext nginx, so none of them caught it.
   - **Fix site:** that one call now uses `.with_native_roots()`, which honors `SSL_CERT_FILE`/`SSL_CERT_DIR`. An independent read-only diagnostician confirmed that cause and fix site. Every peer and Oracle TLS dial pins its CA explicitly and is unaffected, and plaintext dials are unchanged.
   - **Test:** `https_endpoint_trusts_the_platform_roots` failed with the walk's exact error before the fix and passes after it.

3. **The guide's `GRPCRoute`s never reached the pods under Istio 1.31.1** (`21030c17a`).
   - **Symptom:** after fix 2, `flush` failed with `Sink(Internal { message: "" })`, and the anchor logged nothing.
   - **Evidence:** `istioctl proxy-config route` showed the 50051 listener as `blackhole:50051 → 404`, and istiod warned `constructed http route config … on port 50051 with no vhosts`.
     - Both GRPCRoutes reported Accepted.
     - Deleting the same-hostname `HTTPRoute` restored the vhost, and restoring the guide's pair removed it again.
     - The two edge requests that failed carry flag `NR` (no route) and gRPC status 12.
   - **Cause:** Istio drops a `GRPCRoute` whose hostname matches an `HTTPRoute` attached to another listener of the same Gateway.
   - **Fix site:** the guide routes public gRPC with an `HTTPRoute` on the `grpc` listener, and says why. The backend port name `grpc` keeps the upstream on h2. With that change, seed, append and joined read all passed through the edge.

### Walk notes (kind-only; not committed)

- **Scope:** the walk (`scratchpad/walk5/walk.sh`) extracts every YAML block from both guides on each run. It applies only the printed kind deviations: image, rustfs endpoint and keys, smaller resources, `*.localhost` hostnames, a ClusterIP gateway Service, and rustfs port 9000 excluded from the sidecar.
- **Why port 9000 is excluded:** the kind object store is plaintext HTTP inside the mesh, and the sidecar returned `503 UC` on pooled connections that rustfs had closed. Production object stores use HTTPS, which the sidecar passes through as TCP.
- **Oracle readiness flaps against rustfs:** the storage probe (an OpenDAL `stat`) sometimes fails in about 1 ms with `BackendError`. Kubelet also recorded readiness 500/503. rustfs logged no matching error and the probe does not log the backend error, so the cause is not identified. The walk waits up to 60 s for `/readyz` 200 through the edge.
- **Shared disk:** during one run another session's `docker compose … benches/gateway … build` filled the shared Docker VM disk. That run hit Bifrost's 256 MiB free-space floor, and it was not used as evidence.

### Commands

```bash
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_migration \
  -E 'test(=pg_tests::migration_lease_serializes_and_bounds_competing_migrators)'   # PASS
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=catalog::iceberg_sql::tests::catalog_sqlx_negotiates_tls)'              # PASS
mise exec -- cargo nextest run --locked -p wyrd-client --test transport \
  -E 'test(=grpc::grpc_connection::https_endpoint_trusts_the_platform_roots)'       # PASS; whole binary 69/69
mise run test:sql                  # PASS
mise run test:server:startup       # PASS
mise run test:server:kind          # PASS
mise run docs:check                # PASS
mise run fmt                       # PASS
mise run lints                     # PASS
mise run check:from-pools-allowlist   # PASS
mise run check:workspace-hack      # PASS
mise run check:client-tier         # PASS
mise run check:registry-client-tier   # PASS
git diff --check main...HEAD       # clean
```

`test:server:peer` was not run because no peer behavior or admission changed.

### Non-goals kept

- None of the following were added: a Wyrd route, public certificate variable, peer ticket, image, controller, test harness, or Wyrd-specific database TLS knob.
- The nginx-to-Rust loopback, peer mTLS on 50052, owner-only migration, and Oracle-only HPA are unchanged.
- `deploy/kubernetes/bifrost/deployment-mixed.yaml` is unchanged. It still pairs a `GRPCRoute` with a same-hostname route; with Istio it would hit defect 3. This is recorded for a future task.
- `GrpcConfig.tls` (custom CA material) is still ignored by `build_endpoint`. This predates R5 and is out of scope.

Separate, user-directed change: spec revision 44 adds the `dist` Cargo profile (fat LTO, one codegen unit, stripped), and `release.yml` publishes the server with it (`683e89987`).

### Addendum: follow-up commits after the evidence

The user directed three follow-ups after the evidence above. They are on the same branch, so the candidate is now `895477da1`.

| Commit | Change | Why | Verification |
|---|---|---|---|
| `1303c20a8` | The readiness probes now log the underlying Postgres or storage error (`error = %error`), not only its class | The Oracle storage flap in kind logged only `backend`, so its cause could not be read | Health tests 6/6 under `with-test-postgres.sh`; `lints` |
| `b87993bfb` | `GrpcConfig.tls` and `HttpConfig.tls` removed, the two `transport_secrets` examples deleted, and the schemas regenerated | Both fields were accepted but never read, so a configured CA was silently ignored. Both transports verify `https` against the platform trust store, which honors `SSL_CERT_FILE` and `SSL_CERT_DIR` | `wyrd-client --test transport` 68/68; both Rust examples run; the Python example smoke test (7 tests) passes; `codegen:check`, `lints`, `py:lints` |
| `895477da1` | `deploy/kubernetes/bifrost/` deleted, together with its YAML contract test, `check:bifrost-oracle-deploy` and `wyrd-testing`'s `serde_yaml` dependency | Nothing deployed these manifests (their routes named no Gateway), and only that check read them. `bifrost-single-data-root` spec revision 3 (approved by the user) moves AC-004 onto `deploy/kubernetes/kind` and the production guide, which `test:server:kind` deploys | `cargo check -p wyrd-testing --tests`; `check:workspace-hack`; `fmt`; `lints`; `git diff --check` |

This supersedes two residual risks above: `deployment-mixed.yaml` no longer exists, and `GrpcConfig.tls` no longer exists.

**Status:** IMPLEMENTED. Route to `$wyrd-task-review` for the complete base-to-candidate range (candidate `895477da1` plus this evidence commit).
