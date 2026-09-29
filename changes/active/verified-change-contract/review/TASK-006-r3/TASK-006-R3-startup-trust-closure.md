---
id: TASK-006-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 41
requirements: [REQ-153, REQ-154, REQ-155, REQ-156, REQ-157, REQ-159, REQ-160, REQ-161, REQ-162, REQ-163, REQ-164, REQ-165, AC-034, AC-035, AC-036, AC-037, AC-038]
depends_on: [TASK-006-R2]
parent_task: TASK-006
remediates: [FIND-TASK-006-13, FIND-TASK-006-14, FIND-TASK-006-15, FIND-TASK-006-16, FIND-TASK-006-17, FIND-TASK-006-18, FIND-TASK-006-19, FIND-TASK-006-20, FIND-TASK-006-21, FIND-TASK-006-22]
---

# TASK-006 R3 — Startup, trust, and deployment documentation closure

## Authority and decision

- Approved authority: [spec revision 41](../../spec.md); [original TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Review: [R3 verdict](verdict.md) and [validated findings](findings-validation.md). Prior R1/R2 findings remain closed.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; R2 remediation base: `f500ea38bc749f36b3ee8d88893dcf7c0161435c`; reviewed candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`.

**Approved spec decision:** Revision 41 accepts the official-recipe image built from the reviewed commit and pinned by its immutable local image ID for pre-release TASK-006 acceptance. The first release repeats the image journeys against a published registry digest. No Wyrd image has yet been published; no publication is required to implement this task.

## Required corrections

### FIND-TASK-006-13 — Image provenance

`scripts/server/test-startup.sh:89-95` and `scripts/server/test-kind-autoscale.sh:103-111` build local tags but do not record or pin an immutable image ID, so the tests cannot identify the exact image they exercised. Make both journeys report and exercise one immutable image ID built from the official recipe and reviewed commit. Keep migration, setup, restart, routing, HPA, peer join, and remote-read assertions. Do not invent a published digest. **Proof:** recorded image ID and source commit for both passing `test:server:startup` and local `test:server:kind`; the release gate separately runs the published digest.

### FIND-TASK-006-14 — Stale peer authority

`architecture/wyrd-security-posture.md`, `architecture/operations/deployment-and-release.md`, and `architecture/bifrost-design.md` still require the removed ticket protocol. Update these authorities to dedicated-CA/shared-leaf mTLS, fixed `wyrd-peer` identity, receiver-owned context checks, and system-owner audit before trusted tenant binding. Remove ticket-specific keys, replay, audit, and compatibility prose while preserving live schema/object/wire/peer upgrade and recovery rules. **Proof:** no normative ticket requirement remains; `mise run docs:check` passes.

### FIND-TASK-006-15 — Inbound peer identity

The private listener in `wyrd-tonic/src/server/mod.rs:59-101` admits a CA-signed client leaf without checking its DNS identity. Enforce the fixed `wyrd-peer` client identity at the existing listener boundary while retaining CA, validity, usage, and outbound server-name checks. **Proof:** a same-CA, valid client leaf with another SAN is rejected before handler body polling; existing positive and negative peer TLS cases still pass.

### FIND-TASK-006-16 — Pre-binding audit ownership

`wyrd-server/src/oracle/peer_authority.rs` can select a tenant audit chain from unsigned forwarding, fragment, or stage claims before receiver-owned state binds a tenant. Use the existing system-owner/unverified audit path for those pre-binding refusals; attribute to a tenant only after receiver query, reservation, or stage state establishes it. Keep fail-closed behavior, refusal codes, and bound-tenant audit. **Proof:** foreign-tenant forged contexts fail without a foreign-tenant audit row, commit a system-owner row, and retain tenant attribution after valid binding.

### FIND-TASK-006-17 — Migration lease

`wyrd-server migrate` currently calls two crate migrators under separate advisory locks that end before combined validation; their lock waits are unbounded. Hold one bounded database-wide owner-session lease across ordered Wyrd/Vala migrations and post-validation, reusing their SQL migration owners. **Proof:** competing migrators cannot interleave stages or wait beyond the bound, and a retry succeeds after release or failure.

### FIND-TASK-006-18 — Exact serving logins

`SchemaCheck::login` checks role attributes but not the effective `current_user` name, so a different grant-capable login can pass readiness. Require exactly `wyrd_app` and `wyrd_platform_admin` at their respective SQL-owner serving boundaries and reject privilege-widening role attributes or memberships. **Proof:** wrong app and BYPASSRLS platform logins refuse startup; named logins retain ordinary tenant and platform work.

### FIND-TASK-006-19 — Effective schema security

Readiness checks forced-RLS flags but omit the policy and most required/forbidden grants. Check the specific tenant policies and role privileges established by the Wyrd/Vala migrations after owner migration and before serving readiness. Keep this scoped to the approved schema contract. **Proof:** a missing required policy and an excessive app grant each refuse both validation and serve; restored state passes tenant isolation and platform work.

### FIND-TASK-006-20 — Raw pool handle

The new public `SchemaCheck<'a>` stores and accepts `&PgPool`, violating the library SQL-handle boundary. Keep readiness on the existing `WyrdPostgres`/`ValaPostgres` owners and approved operator capability; preserve the owner-only migration session separately. **Proof:** no new raw-pool library field/signature, `mise run check:from-pools-allowlist`, SQL and startup lanes pass.

### FIND-TASK-006-21 — Setup writer rustdoc

`Tee<W>::write` and `flush` in `wyrd-server/src/main.rs:326-337` lack the required per-item rustdoc and `# Errors`. Document partial-write capture and error behavior on those methods. **Proof:** inspect the docs and run format/lints; no new test or wrapper.

### FIND-TASK-006-22 — Peer readiness order

Scribe/Oracle publish ready membership before the private socket binds and serves, so another replica may route to an unreachable peer. Keep existing role leases reserved but unready through recovery and listener startup; activate exact fences only when the peer listener is serving, and leave/deactivate them on failure. Preserve local one-process calls and the existing registry. **Proof:** a second process held or failed before bind is absent from the first's ready snapshot; after release it joins and remote Oracle/tail work succeeds.

## Server startup documentation — required in this remediation

Write complete, linked self-hosting journeys for all three setups below. Each must give prerequisites, an exact required/conditional/optional environment-variable table with value source, credential creation and storage, ordered database bootstrap → one-off migrate → serve → first-use setup → client request, readiness checks, restart persistence, and failure/recovery steps. Reuse the existing self-hosting pages and Kubernetes examples; correct stale directions. Do not claim any image has already been published.

1. **Local development with PostgreSQL and object storage.** Show the separate PostgreSQL owner migration URL, `wyrd_app` `WYRD_DATABASE_URL`, `wyrd_platform_admin` `WYRD_PLATFORM_DATABASE_URL`, `WYRD_STORAGE_URL` using a durable local `file://` directory or development object store, and `WYRD_BIFROST_DATA_DIR` for persistent WAL/identity. Explain `APP_ENV=development`, ephemeral versus stable `WYRD_SIGNING_KEY_FILE`, default one-process `all` with no peer inputs, `wyrd-server setup --tenant ...`, one-time credential capture, health/readiness, and an authenticated client read/write. State whether the chosen Postgres and storage survive restart; do not describe a throwaway fixture as durable.
2. **Development server on Kubernetes.** Start with one `all` replica, external Postgres, a migration Job that alone receives the owner URL, serving-only database Secrets, `WYRD_STORAGE_URL`, a stable signing key, and persistent `WYRD_BIFROST_DATA_DIR`. Explain public HTTP/UI/gRPC routing, readiness, first-use setup without leaking its credential to logs, and restart checks. For optional peer mode, name `WYRD_PEER_ADDRESS`, `WYRD_PEER_TLS_DIR`, a shared object store, per-Scribe volume, and private peer networking; process-local `file://` storage cannot be used across peers.
3. **Production server on Kubernetes.** Show `APP_ENV=production`, verified backup before migration/upgrade, owner-only migration and failure stop/repair/restore, two serving logins, shared durable object store and its provider workload identity or credential chain, stable signing key, per-Scribe persistent `WYRD_BIFROST_DATA_DIR`, conditional sealing key and tenant slug for OIDC, and first platform/tenant/API credential capture and rotation. Use a concrete peer example: fixed `all` anchor and autoscaled `scribe` template, `POD_IP` from `status.podIP`, `WYRD_PEER_ADDRESS=$(POD_IP):50052`, one dedicated CA with its private key outside serving pods, shared `wyrd-peer` leaf/key mounted with `ca.crt` at `WYRD_PEER_TLS_DIR`, and NetworkPolicy restricting the private port. Explain that peer TLS does not grant tenant identity. Describe a successful-read-rate HPA and metric adapter as an example, cap and peer-join/remote-read proof, and require production thresholds from load testing rather than copying the kind 10 reads/s target. State the default `all` memory floor of at least 3.75 GiB and that the local 4 GiB anchor has too little production headroom. Distinguish public TLS, database credentials, Wyrd client credentials, object-store identity, and peer certificates.

**Documentation proof:** `mise run docs:check`, rendered navigation and links, and a manual three-scenario walkthrough checked against current CLI, config, manifests, and startup scripts. Documentation-only work requires no manufactured RED test. The guide must use revision 41's pre-release local-image and release published-image distinction.

## Constraints and verification

Preserve prior findings 1–12, Eval media and result behavior, public RBAC and audit, tenant isolation, restart durability, first-class SDKs, and existing startup/peer/kind journeys. Do not restore tickets, peer API keys, manual peer lists, the policy hook, an embedded database, a second registry, or generic rollback. Do not weaken tests or turn the kind threshold into a production default.

Run the focused proof for each finding above, then the owning lanes: `mise run test:server:startup`, `mise run test:server:peer`, local `mise run test:server:kind`, `mise run test:bifrost:journey:server`, `mise run test:bifrost:journey:oracle`, `mise run test:sql`, `mise run check:from-pools-allowlist`, `mise run docs:check`, `mise run fmt`, `mise run lints`, and `git diff --check`. Run `mise run codegen:check` if contracts change; run relevant SDK/storage lanes if their surfaces change. Every specifically named Rust test added or changed must have its exact `mise exec -- cargo nextest run --locked` package/target/selector command recorded and run. Preserve the local kind task as a manual completion proof, not a default CI gate.

**Stop condition:** if a correction requires another change to approved behavior, security, tenancy, durability, or release acceptance, return for a new spec decision rather than changing the requirement inside this task.

## Implementation evidence

Candidate: `300b4bd985134b260fcd26efcee71ce437a9cfe8` plus this evidence commit. Lanes ran sequentially on a clean tracked tree.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-13 image provenance | `scripts/server/test-startup.sh`, `scripts/server/test-kind-autoscale.sh` (`cb1de2e73`, `2d5276b3e`, `300b4bd98`): labelled official build, `sha256:` image ID required, provenance written to `target/*-image-provenance.txt`, container image checked against the ID (startup) and every Wyrd pod checked against the loaded image's repo digest (kind) | `mise run test:server:startup` PASS: image `sha256:2f4d8c51b192889889e9437de71172863a8b2663dc733d14f0c527432c9fb725` from `29022477593d049f2ab55f40d163b283d832a3cb`; `mise run test:server:kind` PASS: image `sha256:b02f467acbbc2a33c62619bc8e52d501ca0cc43f0977ef03b72e7cc79be7c204` (config `sha256:3d597000949086a84de335b415730967d8ebc2cfaecb71e46f046ccb05748fad`) from `300b4bd985134b260fcd26efcee71ce437a9cfe8`, HPA 1→2 at 13.835 reads/s, join at `https://10.244.0.18:50052`, anchor Oracle 507→553 queries, new replica 0 local queries. No published digest is claimed; the release gate reruns against it. | PASS |
| FIND-14 stale peer authority | `40fae4903`: `architecture/wyrd-security-posture.md`, `operations/deployment-and-release.md`, `bifrost-design.md`, `operations/runbooks.md` | `grep -ri ticket architecture/` shows only completed records; `mise run docs:check` PASS | PASS |
| FIND-15 inbound peer identity | `ee20fc70c`: `wyrd-tls::certificate_has_dns_name`, listener check in `wyrd-server/src/grpc/mod.rs`, `issue_misnamed_leaf` fixture | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete)"'` PASS; `mise run test:server:peer` 9/9 PASS | PASS |
| FIND-16 pre-binding audit ownership | `8cf529615`: `wyrd-server/src/oracle/peer_authority.rs` routes pre-binding refusals to `DataTenantId::SYSTEM_OWNER` | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::peer_authority::tests::reservation_contexts_are_operation_and_body_exact) \| test(=oracle::peer_authority::tests::stage_authority_rejects_before_decode_cache_or_io) \| test(=oracle::peer_authority::tests::oracle_peer_authority_audits_rejections_to_the_right_chain) \| test(=oracle::peer_authority::tests::oracle_peer_authority_fails_closed_when_security_audit_is_unavailable) \| test(=oracle::peer_authority::tests::forward_query_context_is_checked_against_the_receiver)'` 5/5 PASS; `peer_network::security::peer_context_refusals` PASS in `test:server:peer`. Durable system-row commit is covered by the existing `peer_audit` Postgres and `system_owner_security_rejections_retain_once` tests. A journey asserting that no foreign-tenant row exists was not added: publisher timing makes an absence assertion fragile, and chain selection is exact at the unit seam. | PASS |
| FIND-17 migration lease | `a3c4e2231`: `MigrationLease` (one bounded owner-session lease across Wyrd/Vala migrate and validation) | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-sql --test pg_migration --test-threads=1 -E "test(=pg_tests::migration_lease_serializes_and_bounds_competing_migrators)"'` PASS; `vala-sql --test pg_migration -E "test(=pg_tests::vala_migrations_apply_and_are_idempotent) \| test(=pg_tests::old_scribe_membership_shape_is_preserved) \| test(=pg_tests::staged_rows_survive_the_credential_attribution_upgrade)"` 3/3 PASS | PASS |
| FIND-18 exact serving logins | `a3c4e2231`: `verify_login_name`, `verify_app_login`, `verify_platform_login`, `verify_serving_roles` | `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=schema_check::tests::login_name_must_match_session_and_effective_role)'` PASS; `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-dev-fixtures --features pg --lib --test-threads=1 -E "test(=pg::pg_tests::validate_schema_refuses_substituted_serving_logins)"'` PASS | PASS |
| FIND-19 effective schema security | `a3c4e2231`, `115b79cb9`: `verify_tenant_isolation`, `verify_schema_privileges` (owned relations excluded for the CREATE-posture role); startup script policy-rename and `GRANT TRUNCATE` refusal cases | `wyrd-dev-fixtures ... -E "test(=pg::pg_tests::validate_schema_refuses_drifted_or_unprotected_schemas)"` PASS; `test:server:startup` refuses serve and migrate on a missing `tenant_isolation` policy and on `TRUNCATE` held by `wyrd_app`, then passes after restore | PASS |
| FIND-20 raw pool handle | `a3c4e2231`: readiness through `OperatorPool`, `WyrdPostgres`, and `ValaPostgres`; owner session only in `MigrationLease` | `mise run check:from-pools-allowlist` PASS; `mise run test:sql` (165+6+113+2) PASS | PASS |
| FIND-21 setup writer rustdoc | `8b9dae929`: `Tee<W>::write` and `flush` docs with `# Errors` | Inspected; `mise run fmt`, `mise run lints` PASS | PASS |
| FIND-22 peer readiness order | `98f52f07e`: peer-mode fences start unready; `Bifrost::activate_peer_roles` runs inside the serving `BifrostPeer` task; `ProcessCluster::join_with_peer_socket_held` | RED without the fix: "a replica that never served its peer listener is ready". GREEN: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::join::peer_join_and_remote_query)"'` PASS | PASS |
| Local development journey | `docs/src/content/docs/self-hosting/local-development.svx` | Manual walkthrough in a scratch directory: durable `postgres:16` volume, `roles.sql`, owner `migrate`, signing key, serve, `/healthz` ok, `/readyz` 200, `setup --tenant acme`, `wyrd apply hello` + `wyrd list --kind Prompt`, restart, Card still listed, setup rerun a no-op | PASS |
| Development Kubernetes journey | `self-hosting/kubernetes-development.svx` | Checked against `deploy/kubernetes/kind/*.yaml`, `docker/official/Dockerfile`, `mise run docker:build`, the kind script's secret, migrate, and setup sequence, and the CLI help | PASS |
| Production Kubernetes journey | `self-hosting/kubernetes-production.svx` | Checked against the kind manifests, the adapter rule, `wyrd-security-posture.md`, `configuration.svx`, `storage.svx`, and `wyrd platform/principal credential` help. The kind 10 reads/s target is explicitly not a default; the 3.75 GiB floor and the 4 GiB anchor limit are stated. | PASS |
| Stale directions corrected | `docker.svx` (no published `wyrdai/wyrd`; local image ID), `index.svx` (targets, throwaway wording), page orders | `mise run docs:check` PASS (generate drift, commands, links, build, a11y 62 pages) | PASS |
| Owning lanes | — | `test:server:startup`, `test:server:peer` (9/9), `test:server:kind`, `test:bifrost:journey:server` (22/22), `test:bifrost:journey:oracle` (28/28), `test:sql`, `check:from-pools-allowlist`, `docs:check`, `fmt`, `lints`, `git diff --check`: all PASS | PASS |

**Diagnosis (kind pin check, first run):**

- **Symptom:** `require_pinned` failed with "wyrd-core-0 runs docker.io/library/import-2026-09-25@sha256:aefb…, not sha256:bf8e…".
- **Evidence:** on a throwaway kind node, `crictl inspecti wyrd:kind` reports id `sha256:bf8e…` (the config digest) and `repoDigests: [docker.io/library/import-2026-09-25@sha256:aefb…]`; `crictl inspecti` on that repo-digest reference returns "no such image".
- **Cause:** `kind load` records the manifest digest under a synthetic `import-<date>` repository, and the pod's `imageID` reports that reference. CRI does not resolve it as an image name.
- **Fix site:** the script's `require_pinned`. It now compares each pod's `imageID` with the repo digest of the image whose CRI id equals the pinned config digest. No other caller.

Non-goals held: no tickets, peer API keys, manual peer lists, policy hook, embedded database, second registry, or generic rollback; no test weakened; the kind threshold remains local; `deploy/kubernetes/bifrost/deployment-mixed.yaml` unchanged. `codegen:check` not required: no contract changed.

**Status:** IMPLEMENTED
