# TASK-006 R5 findings validation

## Subject and scope

Immutable cumulative subject: `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`. Approved authority is `changes/active/verified-change-contract/spec.md` revision 43, the original TASK-006, R1–R4 verdicts, validated findings and remediation packets, `AGENTS.md`, `architecture/agent-rules.md`, and the applicable security, deployment, Bifrost, and testing authorities. `.codegraph/` is absent. I inspected the seven R5 Wave 1 reports and the actual source and callers of each proposed correction. This is static review; I did not rerun the environment lanes or edit product source.

## Validation of every Wave 1 proposal

| Wave 1 source ID | Disposition | Independent validation |
| --- | --- | --- |
| `TASKREV-R5-01`, `SR-R5-2`, `SEC-R5-01`, `DEP-1` | **REVISED, merged** | `kubernetes-production.svx:339-371` routes public traffic from an edge TLS terminator to nginx's plaintext pod listeners and admits all sources on those ports. `nginx.conf.template` confirms the listeners are plaintext. The operations authority requires authenticated, encrypted gateway-to-server transport. Revision 43's public TLS edge rule allows a distinct authenticated inner hop; it does not waive that rule. The four reports describe one defect. Keep edge TLS and the existing nginx-to-Rust loopback; use an existing cluster transport facility, not another Wyrd listener or peer certificate. |
| `SR-R5-1` | **REVISED** | `SqlStore::migration_lease` owns the correct detached owner session, but `impl From<PgPool> for SqlStore` at `wyrd-sql/src/lib.rs:298-303` exposes a new raw-pool library signature contrary to `architecture/agent-rules.md:6`. Its only call sites are `PgFixture::migrate` and three SQL integration-test modules; production `wyrd-server migrate` already uses `SqlStore::connect_with`. Remove the conversion while retaining the lease and test-owned owner connection setup. This **reopens prior FIND-TASK-006-20** rather than earning a new ID. |
| `SEC-R5-02` | **CONFIRMED** | The production guide's owner, app and platform-admin DSN examples at `kubernetes-production.svx:70-72,94-97` specify no certificate-verifying mode or trust root. `dsn.rs:113-116` only parses URLs, so the guide does not establish the mandatory external Postgres certificate verification in `deployment-and-release.md:42-43`. This is a documentation correction for all three logins; no new server config validation is required by this finding. |
| `SEC-R5-03` | **REVISED** | The development guide's second-tenant example at `kubernetes-development.svx:216-218` expands a live root credential into `kubectl exec` command arguments. That Kubernetes API request can retain the credential in request/audit records, while the first-use setup requirement confines disclosure to the invoking terminal. The existing `wyrd platform tenant create` CLI reads `WYRD_PLATFORM_CREDENTIAL` on the operator machine and can use the guide's existing port-forward. Replace the example with that path; do not invent a Kubernetes secret-injection workflow. |
| `DEP-2` | **CONFIRMED** | `kind/load.yaml:35-43` backgrounds the complete `[ jobs check ] && read_once` list. In both POSIX `sh` and `bash`, the `jobs` call therefore executes in the child shell and misses the parent's active request jobs. A local `sh` reproduction with a held parent job printed `child_admits_second` despite a cap of one. Curl timeout and Job deadline bound duration but cannot enforce AC-037's concurrency cap. Leave the check in the parent and background only the read. |
| `domain-review-data.md`, `domain-review-peer.md`, `domain-review-eval.md` | **Validated empty** | These reports propose no new findings. Their inspected migration, peer, and Eval owner paths do not contradict the bounded findings above; the data report's conclusion about the migration lease's behavior remains true even though the separate raw-pool signature violates the repository rule. |

## Final deduplicated finding ledger

### FIND-TASK-006-20 — REVISED / VIOLATION — raw-pool conversion remains public

- **Source:** `SR-R5-1`; reopens the same R4 finding.
- **Obligation:** `architecture/agent-rules.md:6` and the R4 FIND-20 closure forbid a new raw `PgPool` library signature while preserving an owner-session migration lease.
- **Location and evidence:** `crates/wyrd/wyrd-sql/src/lib.rs:226-303` contains the full `SqlStore::migration_lease` body and `From<PgPool>` conversion. Production's sole migration caller is `crates/wyrd/wyrd-server/src/main.rs:199-218`, which already creates a `SqlStore` from the owner DSN. All conversion callers are the shared fixture and SQL integration tests (`crates/shared/wyrd-dev-fixtures/src/pg.rs:373-397`, `crates/wyrd/wyrd-sql/tests/pg_migration.rs:39-90`, `pg_admin_principals.rs:996-1004`, `crates/vala/vala-sql/tests/pg_migration.rs:17-36`). No production caller needs the conversion.
- **Consequence:** Any production library can now turn an arbitrary raw pool into the public migration owner, reopening the prohibited handle boundary; the construction allowlist does not check that signature.
- **Decision-complete correction:** Delete only `From<PgPool> for SqlStore`. Reuse `SqlStore::connect_with` and the fixture's already-owned owner DSN/pool setup for migration tests; keep raw assertion pools confined to fixtures/tests. Preserve the detached, bounded advisory-lock session, ordered Wyrd/Vala migrations, and explicit release. Do not wrap the owner login in a serving `OperatorPool` or add another lease owner.
- **Focused proof:** Source check finds no new raw-pool library signature; the exact competing-migrator test, `test:sql`, startup, and `check:from-pools-allowlist` pass.

### FIND-TASK-006-27 — REVISED / VIOLATION — production ingress hop lacks required transport security

- **Sources:** `TASKREV-R5-01`, `SR-R5-2`, `SEC-R5-01`, `DEP-1`.
- **Obligation:** R3's complete production Kubernetes journey and `architecture/operations/deployment-and-release.md:27-32` require authenticated, encrypted gateway-to-server transport and a restricted local plaintext boundary.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:339-371` allows any source to reach ports `8080`/`50051` and terminates public TLS before routing to those Services. `docker/official/extras/nginx/nginx.conf.template:40-76` listens in plaintext there; its forwarding to Rust is loopback plaintext. Neither the guide nor the kind walkthrough establishes protection on ingress-to-pod traffic.
- **Consequence:** A deployment following the guide transmits tokens and request bodies on an unprotected cluster hop and permits direct in-cluster access that bypasses edge controls.
- **Decision-complete correction:** In the existing production guide, select an available authenticated and encrypted ingress-to-pod facility for **both** HTTP and public gRPC, make it a prerequisite, and show its enforced policy and ingress-only source restriction in the routing example. A strict mesh mTLS policy is one viable existing platform mechanism. Keep public TLS terminated at the edge, the official image's plaintext nginx-to-Rust loopback, and separate peer mTLS on `50052`; neither Wyrd public TLS variables nor peer certificates belong in this correction. This is a bounded deployment-documentation fix under the existing operations authority, not a new product/spec decision.
- **Focused proof:** Apply or render the documented transport and NetworkPolicy; through the edge, an HTTP request and public gRPC request succeed, while an ordinary pod cannot reach `8080`/`50051` directly. Run `docs:check`. A boot-only kind walk does not prove this boundary.

### FIND-TASK-006-28 — CONFIRMED / VIOLATION — production Postgres URLs omit certificate verification

- **Source:** `SEC-R5-02`.
- **Obligation:** `architecture/operations/deployment-and-release.md:42-43` requires verified certificates and explicit trust roots for external Postgres, and R3 requires a complete production credentialing journey.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-production.svx:70-72,94-97` describes all three production database credentials as bare URLs, with no verifying TLS setting or CA source; `crates/wyrd/wyrd-sql/src/dsn.rs:113-116` merely checks URL syntax.
- **Consequence:** The documented owner migration and serving-role setup can connect without authenticating the database server, exposing their credentials and data to an impostor endpoint.
- **Decision-complete correction:** Add certificate-verifying Postgres DSN examples and the trusted provider CA location for the owner, app and platform-admin logins in the production guide, plus a connection check that rejects a mismatched server certificate. Use the existing Postgres/SQLx TLS options; preserve development's local DSNs and peer TLS separation. Do not add a Wyrd-specific database TLS knob.
- **Focused proof:** Follow the three documented connections against a certificate-valid Postgres endpoint and prove one wrong-server-certificate case fails; run `docs:check`.

### FIND-TASK-006-29 — REVISED / INCORRECT — second-tenant command puts a root credential in exec arguments

- **Source:** `SEC-R5-03`.
- **Obligation:** The R3 development Kubernetes credential journey and REQ-155's terminal-confined disclosure require a safe way to reuse the one-time root credential.
- **Location and evidence:** `docs/src/content/docs/self-hosting/kubernetes-development.svx:208-218` prints the first-use credentials via `kubectl exec`, then instructs `kubectl exec ... env WYRD_PLATFORM_CREDENTIAL=wyrd_global_… wyrd-server setup --tenant beta`. The latter credential becomes a Kubernetes exec command argument. `crates/wyrd/wyrd-cli/src/platform/tenant.rs:29-61` already exposes authenticated `tenant create`; its endpoint reads the credential from the operator environment.
- **Consequence:** The stored root credential can be retained in Kubernetes API request/audit data during a routine second-tenant setup.
- **Decision-complete correction:** Replace that one command with the existing `wyrd platform tenant create` command from the operator machine through the documented port-forward, reading `WYRD_PLATFORM_CREDENTIAL` from the operator's local secret environment. Keep first-use `kubectl exec` behavior and one-time terminal output unchanged; add no setup mode or secret-handoff mechanism.
- **Focused proof:** Following the development guide creates a second tenant; the `kubectl exec` request contains no root credential. Run `docs:check`.

### FIND-TASK-006-30 — CONFIRMED / INCORRECT — kind read-load concurrency cap is ineffective

- **Source:** `DEP-2`.
- **Obligation:** AC-037 explicitly requires the local traffic generator to bound rate, concurrency, duration and cleanup.
- **Location and evidence:** `deploy/kubernetes/kind/load.yaml:35-43` uses `[ "$(jobs -p | wc -l)" -lt "$MAX_INFLIGHT" ] && read_once &`; the backgrounded list checks jobs in a child shell. The `read_once` caller is this Job only, and `scripts/server/test-kind-autoscale.sh` applies and deletes this Job in the one required kind journey.
- **Consequence:** Slow requests can accumulate beyond `MAX_INFLIGHT=8` despite the comment and the passing autoscaling result.
- **Decision-complete correction:** Keep the existing shell Job, rate, timeout and cleanup; evaluate the active-job count in the parent shell and background only `read_once`. No second kind journey, load framework, or new counter is needed.
- **Focused proof:** A small shell check with held requests shows the configured maximum is never exceeded; rerun the existing `test:server:kind` lane to retain Oracle HPA, peer, and image proof.

## Conclusion

Five bounded findings remain: reopened `FIND-TASK-006-20` and new `FIND-TASK-006-27` through `-30`. Prior findings `1–19` and `21–26` remain closed on the inspected paths. The approved revision 43 edge-only public TLS and separate peer mTLS can be preserved while fixing the production ingress guide. No retained correction requires a new product, public API, architecture, or persistent-data decision. Recommend `FIX_REQUIRED` with one R5 remediation packet.

## Post-review user correction to FIND-TASK-006-20

The original Wave 2 recommendation above retained `SqlStore`. The user subsequently directed that migration use the existing `OperatorPool` and that `SqlStore` be removed. Source tracing confirms that `SqlStore` has one production caller (`wyrd-server migrate`); all other callers are SQL tests and fixtures. The one-off command already wraps its database-owner pool in `OperatorPool` for post-migration schema verification. The [R5 remediation task](TASK-006-R5-production-startup-closure.md) therefore replaces the narrower recommendation: remove `SqlStore` and its raw-pool conversion, acquire the migration lease through `OperatorPool` backed by the owner DSN only in the one-off command, and adapt tests. This user-directed correction preserves the separate serving `wyrd_platform_admin` login and the existing lease semantics. It is an amendment to the review recommendation, not a claim that Wave 2 independently proposed deletion.
