---
id: TASK-006-R4
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 42
requirements: [REQ-157, REQ-165, AC-034, AC-035, AC-037]
depends_on: [TASK-006-R3]
parent_task: TASK-006
remediates: [FIND-TASK-006-19, FIND-TASK-006-20, FIND-TASK-006-23, FIND-TASK-006-24, FIND-TASK-006-25, FIND-TASK-006-26]
---

# TASK-006 R4 — Finish SQL readiness and Oracle autoscaling

## Authority and subject

- Approved authority: [spec revision 42](../../spec.md).
- Original task: [TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Prior remediation: [TASK-006 R3](../TASK-006-r3/TASK-006-R3-startup-trust-closure.md).
- Current review: [R4 verdict](verdict.md) and [independent finding validation](findings-validation.md).
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; R3 remediation base: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`; reviewed candidate: `58cabb529b93da366959db796ac3b596a6c6c1e6`.

The six findings below are the complete bounded correction set. Preserve the other closed TASK-006 findings, the approved local-image proof, peer trust/readiness, Eval behavior and the existing three startup guides. The user's revision-42 direction supersedes the R3 task's Scribe autoscaling topology: change the original kind journey, not add another one.

## Diagnoses and selected corrections

### FIND-TASK-006-19 — Wrong tenant column passes SQL readiness

**Violated obligation and consequence.** REQ-157 and AC-035 require readiness to refuse an ineffective tenant-isolation policy. `crates/wyrd/wyrd-sql/src/schema_check.rs:222-259` matches any lower-case column in the policy expression. An owner can change `wyrd.auth_users`' named `tenant_isolation` policy from `data_tenant_id = wyrd.current_tenant()` to `id = wyrd.current_tenant()`. Migration ledger, RLS flags, policy name, role, command and permissiveness remain valid, so both owner post-migration validation and serving readiness pass while tenant auth rows are governed by the wrong key. Existing mutation tests rename or remove policies and do not catch this substitution.

**Correction outcome and approach.** In the existing `OperatorPool` readiness check, require the approved `data_tenant_id` expression for both `USING` and `WITH CHECK`. Preserve the current forced-RLS and other policy checks. Do not add a general SQL policy parser or a second readiness layer. This closes the exact drift path at the shared owner used by both migration and serving.

**Focused acceptance proof.** Mutate one migrated table to use another UUID column in its otherwise valid named policy; both owner post-migration check and serving validation must refuse. Restore the policy and prove both pass. Run that exact Postgres test through the repository's mise wrapper, then `mise run test:sql` and `mise run test:server:startup`.

### FIND-TASK-006-20 — Raw pool in the new migration API

**Violated rule and consequence.** `architecture/agent-rules.md` prohibits raw `PgPool` in library signatures outside pool construction. `crates/wyrd/wyrd-sql/src/lib.rs:85` exposes public `MigrationLease::acquire(owner: &PgPool, wait)`; the one-off CLI, fixture and tests call it. The database-wide detached owner-session lease is required and correctly bounded, but this public entrypoint reintroduces the raw-pool API that R3 finding 20 set out to remove. `check:from-pools-allowlist` guards construction, so its green result does not prove this signature compliant.

**Correction outcome and approach.** Move lease acquisition behind the existing SQL migration owner (`SqlStore`), which already owns the pool and migration entrypoint. Have the one-off server command call that typed owner. Adapt fixtures/tests through their existing test-owned pool setup. Retain the same bounded direct-session lock across ordered Wyrd/Vala migrations and post-validation; do not pass owner credentials to serving or turn the owner login into an `OperatorPool`.

**Focused acceptance proof.** No new raw-pool library signature remains. The existing competing-migrator test still proves serialization, bounded wait and retry. Run its exact mise-wrapped command, `mise run check:from-pools-allowlist`, `mise run test:sql`, and `mise run test:server:startup`.

### FIND-TASK-006-23 — Function-scoped peer transport import

**Violated rule and consequence.** `architecture/agent-rules.md` requires production imports at module top except for narrow exceptions. `crates/wyrd/wyrd-server/src/grpc/mod.rs:163-165` imports concrete `TcpConnectInfo` and `TlsConnectInfo` inside `TransportPlane::admits`, which the mounted private listener calls before body polling. This hides a real module dependency and violates the explicit repository rule.

**Correction outcome and approach.** Move those two imports to the existing module import group. Keep admission behavior unchanged. **Focused acceptance proof:** inspect the import location and run `mise run fmt` and `mise run lints`; no new behavioral test is needed.

### FIND-TASK-006-24 — Qualified activation error types

**Violated rule and consequence.** New active role-activation signatures at `crates/wyrd/wyrd-server/src/state.rs:666,1040,1702` return `crate::boot::ServerBootError` rather than an imported bare type. This violates the repository's signature style rule and obscures the module dependency; `Bifrost::activate_peer_roles` is called by the serving peer task.

**Correction outcome and approach.** Import `ServerBootError` once at module top and use it in those three signatures without changing activation order or failure behavior. **Focused acceptance proof:** inspect the signatures and run `mise run fmt` and `mise run lints`.

### FIND-TASK-006-25 — Development Kubernetes local image path cannot start

**Violated obligation and consequence.** R3 requires a complete development Kubernetes guide under spec revision 41's local-image path. `docs/src/content/docs/self-hosting/kubernetes-development.svx:30-39,91-101,125-153` offers `kind load docker-image wyrd`, but the displayed migration Job and StatefulSet request `registry.example.com/wyrd@sha256:…` and omit `imagePullPolicy: Never`. A kind node with only the loaded `wyrd` tag cannot start either workload, so the documented local migration and server sequence stops before readiness.

**Correction outcome and approach.** In that existing guide, make the local kind branch select the loaded image reference with `imagePullPolicy: Never` in both workloads. Keep the digest-pinned registry branch for a cluster that can pull images. Reuse the displayed examples; do not add a registry, new script or second image recipe.

**Focused acceptance proof.** Follow the documented local branch through a completed migration Job and `/readyz`, or validate both rendered workload image and pull-policy values against the loaded kind image. Run `mise run docs:check`.

### FIND-TASK-006-26 — Kind and production read autoscaling target the wrong tier

**Violated obligation and consequence.** Revision-42 AC-037 requires the original kind journey to scale Oracle replicas on successful Oracle-executed reads per second. `scripts/server/test-kind-autoscale.sh` and `deploy/kubernetes/kind/` instead scale a Scribe template 1→2 from `wyrd_http_requests_total`, while the fixed anchor's Oracle executes the read. This proves ingress scale, not Oracle read capacity. `docs/src/content/docs/self-hosting/kubernetes-production.svx:154-243,279-304` repeats the Scribe HPA and omits a pod scrape/relabel path for the `namespace` and `pod` labels its adapter selects, plus a complete workload apply sequence.

**Correction outcome and approach.** Change the existing kind script and manifests to keep one fixed `all` anchor and scale an Oracle-only workload (`WYRD_TARGET=oracle`) from one to two replicas with its own HPA. Rate the existing `oracle_query_duration_seconds_count{outcome="success"}` per Oracle pod, including the scrape and `namespace`/`pod` label path. Drive bounded reads through the initial Oracle so its measured execution rate triggers the HPA; then prove the new Oracle joins at its injected address and executes a peer-dispatched query using remote Scribe data over mTLS. Preserve immutable image-ID checks and the no-certificate peer refusal. Remove Scribe autoscaling from this journey; add no second kind test, server metric, controller or control plane. Update the production guide to apply an Oracle-only Deployment with serving credentials, shared object store, peer TLS and suitable scratch storage; show its complete apply order, per-pod metric collection and Oracle HPA. Choose production thresholds and caps from load tests.

**Focused acceptance proof.** Rerun the one `mise run test:server:kind` journey: record the official image ID/source commit, show a per-pod Oracle success rate above about 10/s under bounded load, observe Oracle HPA 1→2 without direct scaling, prove the new Oracle's injected address and real peer-dispatched execution using remote Scribe data, and retain no-certificate refusal. Walk the production guide's resources in order and confirm its Oracle metric appears in the custom metrics API. Run `mise run docs:check`. A production cluster deployment is not required for document acceptance, but a static link/build pass alone does not prove the workflow. The previous Scribe-based kind pass is historical evidence only.

## Constraints and non-goals

- Keep the revision-42 pre-release immutable local image ID criterion and separate future published-digest release gate. Do not demand a published image now or weaken existing startup/kind assertions.
- Preserve the bounded migration-wide owner lease, role split, RLS, audit, tenant isolation, peer mTLS/identity, listener readiness, and prior findings 1–18 and 21–22.
- Do not add public routes, compatibility paths, peer tickets, a second registry, new deployment automation, a generic policy parser, or a new test harness for pure style changes.
- Keep the three startup guides and fix only the demonstrated gaps; examples must identify their credential and storage sources without embedding real secrets.

## Verification and evidence

Run each focused proof above and record the exact `mise exec -- cargo nextest run --locked` package, target, and `test(=...)` selector for every specifically named Rust test added or changed. Preserve the owning Postgres setup wrapper. Then run `mise run test:sql`, `mise run test:server:startup`, `mise run test:server:kind`, `mise run check:from-pools-allowlist`, `mise run docs:check`, `mise run fmt`, `mise run lints`, and `git diff --check`. Run `mise run test:server:peer` if peer activation/admission code changes beyond the two style fixes. The complete TASK-006 re-review will reassess the cumulative candidate against the original task and revision-42 spec.

Stop for a new approved spec revision if a further correction requires a new product, public API, security, concurrency, resource-ownership or persistent-data decision. The six selected corrections fit revision 42 and may be implemented directly with `$wyrd-implement`.

## Implementation evidence

Candidate: `a4a82dfc5` (spec revision 43). Official image
`sha256:deeb0879e89a05fc8a86afc5e1c4b24aa7d06565bc14a0d12391e8e9a434ce4d`,
built from the clean commit `a4a82dfc5` by the lanes below.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-19: a tenant policy on any column other than `data_tenant_id` fails readiness | `219b3429d`: `wyrd-sql/src/schema_check.rs` compares `pg_get_expr` for USING and WITH CHECK exactly to `TENANT_ISOLATION_EXPR` | `pg_tests::tenant_policy_on_another_column_fails_readiness` (fails on the old code with `got Ok(())`); `mise run test:sql` | PASS |
| FIND-20: the migration lease is acquired through `SqlStore`, not a raw pool | `2cfed514c`: `SqlStore::migration_lease`, `impl From<PgPool> for SqlStore`; `wyrd-server` `migrate()` and fixture callers moved | `pg_tests::migration_lease_serializes_and_bounds_competing_migrators`; `mise run check:from-pools-allowlist`; `mise run test:sql` | PASS |
| FIND-23: peer transport imports at module top | `c51513081`: `grpc/mod.rs` | `mise run fmt`, `mise run lints` | PASS |
| FIND-24: activation error types imported, not qualified | `c51513081`: `state.rs` | `mise run fmt`, `mise run lints` | PASS |
| FIND-25: the development guide's local image path starts | `e490c60e3`: `image: wyrd:latest` with `imagePullPolicy: Never`; `0366f2f61`: the entrypoint creates `/var/lib/wyrd/storage` on an empty volume | guide walk: migration Job complete, `wyrd-0` `/readyz` 200 | PASS |
| FIND-26: kind and production scale the Oracle read tier on Oracle-executed reads | `05a601477`, `831aa9631`: kind `wyrd-oracle` Deployment and HPA on `wyrd_oracle_reads_per_second`; `3b1b80fbb`: production guide anchor, Oracle Deployment, and HPA; `a4a82dfc5`: production peer pods start with edge TLS | `mise run test:server:kind`: HPA 1→2 at 13111m reads/s, new Oracle joined `https://10.244.0.16:50052`, its successful reads rose while the anchor and first Oracle stayed idle, and the anchor's Scribe tail fences rose; guide walk: custom metrics list both Oracle pods, HPA measured `202m/10`, restart recovers | PASS |

### Commands

- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-sql --test pg_migration --test-threads=1 -E "test(=pg_tests::tenant_policy_on_another_column_fails_readiness) | test(=pg_tests::migration_lease_serializes_and_bounds_competing_migrators)"'`: PASS (2/2).
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::production_peer_target_needs_no_public_grpc_certificate)'`: PASS. It failed before `a4a82dfc5` with `production peer-bearing targets require grpc certificate_chain_path and private_key_path`.
- `mise run test:sql`: PASS (166, 6, 113, 2).
- `mise run test:server:startup`: PASS on the image above.
- `mise run test:server:kind`: PASS on the image above.
- `mise run test:server:peer`: PASS (9/9). It ran because revision 43 changed peer-mode startup validation.
- `mise run check:from-pools-allowlist`, `mise run docs:check`, `mise run fmt`, `mise run lints`, `git diff --check`: PASS.

### Diagnoses

- **Kind custom metric never appeared.** *Symptom:* `custom metric wyrd_oracle_reads_per_second never appeared`. *Evidence:* the adapter listed no series. *Cause:* `oracle_query_duration_seconds` exists only after that pod executes its first query. *Fix site:* the kind script sends one baseline read through the first Oracle (`831aa9631`), not a new server metric.
- **Empty volume hides the storage root.** *Symptom:* dev guide `wyrd-0` crash-looped with `invalid path 'path does not exist: /var/lib/wyrd/storage'`. *Cause:* a Kubernetes PVC mounts empty, but only Docker named volumes copy the image's directory. *Fix site:* the shared image entrypoint (`0366f2f61`), which covers every volume mount.
- **Production peer pods refused to start.** *Symptom:* the production guide's anchor exited with `production peer-bearing targets require grpc certificate_chain_path and private_key_path`. *Cause:* a leftover direct-public-TLS rule demanded certificates for the internal gRPC listener that the image's nginx reaches in plaintext. *Fix site:* the user approved deleting `WYRD_GRPC_CERTIFICATE_CHAIN_FILE` and `WYRD_GRPC_PRIVATE_KEY_FILE` and the rule (spec revision 43, `9d1e1b517`, `a4a82dfc5`). Peer mTLS through `WYRD_PEER_TLS_DIR` is unchanged, and the shared `wyrd-tonic` TLS field is untouched.

### Guide walk

A scratch script (not committed) creates a kind cluster and applies the guides' YAML blocks verbatim, in the guides' order. Kind-only deviations:
- the local image with `imagePullPolicy: Never`;
- RustFS S3 with static keys in place of workload identity;
- smaller CPU, memory, and volume sizes;
- an HPA target of 10 in place of "from load testing".

The production half uses a fresh database. A separate earlier deployment's Forge lease, which lasts 15 minutes and belongs to a different node identity, correctly holds a new anchor on standby. A restarted anchor keeps its node identity and reclaims its lease at once. Result: `guide walkthrough: PASS`.

### Scope

"Peer-dispatched" means the read executes on the new Oracle and fetches the anchor's Scribe tail over peer mTLS.

Non-goals kept:
- The kind threshold of 10 is not a production default.
- `deployment-mixed.yaml` is unchanged.
- No new server metric, second kind test, controller, public route, peer ticket, policy parser, or registry was added.
- The owner lease, RLS, audit, and peer mTLS are preserved.
- Examples embed no real secrets.

**Status:** IMPLEMENTED
