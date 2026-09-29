# TASK-006 R3 findings validation

## Immutable subject and method

- Cumulative task base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- R2 remediation base: `f500ea38bc749f36b3ee8d88893dcf7c0161435c`.
- Candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4` (also `HEAD` during this validation).
- Authority: `AGENTS.md`, `architecture/agent-rules.md`, applicable security, Bifrost, deployment, Postgres and testing architecture, approved `spec.md` revision 40, original TASK-006, R1/R2 verdicts and validated findings, and the R2 remediation task. The cumulative and remediation diff inventories and all seven Wave 1 reports were read. Source was not edited.

I independently checked every proposed path in current source and traced the callers of the functions whose correction would change behavior. The task review's acceptance matrix remains the task matrix; this ledger validates its proposed finding and the standards and domain proposals. The user has since confirmed that no official image has ever been published and has not authorized publication. That fact makes AC-034/037's published-image proof unavailable for this pre-release candidate. **FIND-TASK-006-13 requires a specification revision before TASK-006 can pass; the other retained findings have bounded corrections within approved behavior.**

## Proposed-finding disposition

| Wave 1 source | Status | Reason |
|---|---|---|
| TASKREV-006-R3-001 | REVISED — SPEC_REVISION_REQUIRED | AC-034 and AC-037 explicitly require a pinned published image; both journeys build a local tag, and the user confirms no published image exists. The approved proof cannot be performed in this pre-release review without publishing or revising the acceptance criterion. |
| STDS-001, SEC-03, DEPLOY-1 | REVISED, merged | One authority synchronization defect covers security posture, deployment/release, and the Bifrost audit sentence. |
| STDS-002 | CONFIRMED | Separate per-crate locks end before the combined migration and validation end; both use unbounded `pg_advisory_lock`. |
| STDS-003, DATA-R3-2 | REVISED, merged | The same serving-readiness gap covers effective RLS policy and privilege checks in both SQL owners. |
| STDS-004 | CONFIRMED | New public `SchemaCheck` carries and accepts a raw `PgPool` in library code. |
| STDS-005 | CONFIRMED | Both new fallible `Tee` trait methods lack required per-item rustdoc. |
| SEC-01 | REVISED | The listener accepts a same-CA client leaf regardless of its identity; the existing positive test already uses a distinct leaf with the right SAN. Preserve CA and EKU checks while enforcing the fixed DNS identity. |
| SEC-02 | REVISED | Forwarding and fragment failures select a tenant directly from unsigned claims; stage binding failures do so before the claimed tenant matches the trusted binding. The correction applies only before trusted tenant binding. |
| DATA-R3-1 | CONFIRMED | Login checks compare only superuser and BYPASSRLS flags; either DSN may name a different role. |
| PEER-1 | CONFIRMED | Both role activations precede private listener bind/serve, and other replicas select ready registry rows directly. |

## Deduplicated retained ledger

### FIND-TASK-006-13 — REVISED / MISSING / SPEC_REVISION_REQUIRED — published image proof

**Sources:** TASKREV-006-R3-001. **Obligation:** AC-034, AC-037, REQ-153. **Location:** `scripts/server/test-startup.sh:89-95`, `scripts/server/test-kind-autoscale.sh:103-111`, `deploy/kubernetes/kind/wyrd.yaml:29-33,91-96`.

Both scripts compile `wyrd-server` from the checkout, copy it to `binary/`, and build a local Docker tag. The startup helper then uses that tag for every migration and serve invocation; the kind helper loads `wyrd:kind` into kind, matching the manifest's `imagePullPolicy: Never`. No tested image has a published registry digest. The release workflow's separate image publication cannot make a local build artifact identical by implication. The user confirms no official image has ever been published, so there is no digest to select or pull. A passing local lane therefore cannot satisfy the currently approved published-image criterion.

**Decision required:** Do not prescribe publication or a pull of a nonexistent artifact as implementation remediation. The approved AC-034/037 proof must be revised for this pre-release task, or publication must be separately authorized and completed before acceptance. A minimal proposed revision would accept these existing startup and kind journeys against the official image recipe pinned by a local immutable image ID for pre-release acceptance, while requiring a published-digest run at the actual release gate. That changes the approved acceptance criterion and needs explicit spec approval; a local ID is not evidence of publication. Preserve all existing journey assertions whichever criterion is approved.

**Focused closure proof after the decision:** Under a revised pre-release criterion, record the immutable image ID and run `test:server:startup` and `test:server:kind` on that exact official-recipe build; under the current criterion, record a published registry digest and run both journeys on it. The current local-tag passes alone close neither version of the proof.

### FIND-TASK-006-14 — REVISED / VIOLATION — stale peer architecture

**Sources:** STDS-001, SEC-03, DEPLOY-1. **Obligation:** REQ-160, REQ-164, REQ-165 and the repository authority hierarchy. **Location:** `architecture/wyrd-security-posture.md:35,247-267,371`, `architecture/operations/deployment-and-release.md:33-34,171,189`, `architecture/bifrost-design.md:833-836`.

These active authorities still prescribe signed purpose tickets, signing keys, replay checks, and ticket claims/manifest compatibility after the candidate deletes that protocol. The last Bifrost sentence also describes ticket rejection as the system-owner audit case. Following the documents would provision nonexistent credentials or reintroduce the removed replay path.

**Correction:** Edit these existing authorities to state the approved dedicated-CA/shared-leaf mTLS boundary, fixed `wyrd-peer` identity, receiver-owned context validation, and system-owner audit of pre-binding failures. Delete only ticket-specific trust, rotation, replay, audit, fingerprint and manifest instructions. Keep live schema/object/wire/peer compatibility and rollout recovery rules.

**Focused closure proof:** Search the governing architecture for remaining normative ticket requirements and run `mise run docs:check`; inspect the surviving peer compatibility and audit text against REQ-160/161/164.

### FIND-TASK-006-15 — REVISED / INCORRECT — inbound peer identity

**Sources:** SEC-01. **Obligation:** REQ-160 and AC-036. **Location:** `crates/wyrd/wyrd-tonic/src/server/mod.rs:59-101`, `crates/wyrd/wyrd-server/src/grpc/mod.rs:348-384`, `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/{support.rs:95-146,listener.rs:219-268}`.

`mutual_tls_server` is the one production private-listener builder. Its `ServerTlsConfig` requires a CA-signed client certificate but has no client DNS-name admission. `build_peer_grpc` mounts every private service on that builder. Outbound `PeerDial` verifies the server name only. Its accepted positive client leaf is issued by the test CA with `wyrd-peer` SAN; a same-CA leaf with a different SAN is not tested or rejected by the inbound listener. A wrong-identity holder of the CA's client credential can reach private handlers.

**Correction:** Enforce the approved fixed client DNS identity in the existing private listener admission, retaining CA chain, validity, client/server usage, and the current outbound server-name check. Do not add peer keys or a second application authorization protocol.

**Focused closure proof:** In the existing listener journey, present a valid client-auth leaf signed by the dedicated CA whose DNS SAN differs from `wyrd-peer`; prove it is dropped before application body polling. Keep the positive and anonymous, foreign-CA, expired and wrong-server-name cases.

### FIND-TASK-006-16 — REVISED / VIOLATION — unsigned context controls tenant audit attribution

**Sources:** SEC-02. **Obligation:** REQ-161, INV-016, `architecture/wyrd-security-posture.md` security principles and system-owner audit rule. **Location:** `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:117-185,206-296,393-464,497-510`.

`ForwardedOracleQuery::accept` calls `verify_forward_query`, which extracts `data_tenant_id` from unsigned JSON and uses it for an audit append on wrong audience, fence, or expiry before matching any receiver query state. The fragment verifier likewise calls `reject_verified` using an unsigned decoded tenant on wrong audience/fence/expiry; its production caller is the `PeerTicketVerifier` implementation used by private fragment handling. `authorize_stage_inner` uses the claimed tenant for `verify_binding` failures before the claim matches the receiver-derived `StageBinding`; the stage ingress calls it before plan/cache/storage IO. These refusals are reachable by a connected peer, which can choose a foreign tenant audit chain even though the operation returns no data. Post-binding tenant attribution is a different case and must be preserved.

**Correction:** Reuse the existing system-owner/unverified audit append for every pre-binding refusal in forwarding, fragment and stage authority. Attribute a rejection to a tenant only after receiver-owned query/reservation/stage state establishes that tenant. Keep fail-closed audit semantics and the current refusal codes; do not move public admission or tenant IO.

**Focused closure proof:** Present a foreign tenant in at least a stale-fence or wrong-audience forwarded or fragment context and a stage binding mismatch; verify refusal and no foreign-tenant audit row, with the system-owner row committed. Keep a bound-tenant rejection assertion to ensure legitimate attribution remains.

### FIND-TASK-006-17 — CONFIRMED / VIOLATION — migration lease spans only each crate

**Sources:** STDS-002. **Obligation:** `architecture/operations/deployment-and-release.md:128-154` one bounded physical-domain migration lease. **Location:** `crates/wyrd/wyrd-server/src/main.rs:196-218`, `crates/wyrd/wyrd-sql/src/lib.rs:36-122`, `crates/vala/vala-sql/src/lib.rs:35-115`.

The one-off `migrate` command calls the two crate migrators and post-checks sequentially. Each migrator acquires a different advisory key on a separate session and releases it before the next call. Both acquire with unbounded `pg_advisory_lock`. Two release slots can interleave Wyrd and Vala stages and validation, or wait indefinitely, despite the governing physical-domain lease.

**Correction:** Have the one-off migration owner hold one bounded, direct-session advisory lock with a fixed key for the connected database across Wyrd migration, Vala migration, and post-validation. Let the existing crate migrators perform their ordered SQL under that lease; retire redundant per-crate session locks once the common owner guarantees serialization. Keep owner credentials outside serving.

**Focused closure proof:** Run two migrators against one database with the first held inside its migration/validation sequence; prove the second neither enters a stage concurrently nor waits beyond the documented bound, and that a subsequent retry succeeds after release/failure.

### FIND-TASK-006-18 — CONFIRMED / INCORRECT — wrong serving PostgreSQL login

**Sources:** DATA-R3-1. **Obligation:** REQ-156/157 and AC-035 exact two-login boundary. **Location:** `crates/wyrd/wyrd-sql/src/schema_check.rs:82-96`, `crates/wyrd/wyrd-sql/src/postgres.rs:107-121`, `crates/vala/vala-sql/src/postgres.rs:100-107`.

`SchemaCheck::login` obtains `current_user` but checks only `rolsuper` and `rolbypassrls`. Both production serving owners call it during boot, so a differently named non-superuser role with matching BYPASSRLS posture and sufficient inherited grants can pass. The platform DSN may likewise point at an owner-equivalent BYPASSRLS login. The DSN parser only checks URL syntax. Serving can become ready outside the two approved identities.

**Correction:** At the existing SQL-owner readiness boundary require `current_user` to be exactly `wyrd_app` or `wyrd_platform_admin` as appropriate, and reject prohibited role attributes or effective role membership that defeats the narrow grants. Preserve the owner-only migration process and the two serving-pool split.

**Focused closure proof:** Against migrated Postgres, substitute a grant-capable wrong app login and a BYPASSRLS wrong platform login; each must refuse serving readiness while the two named logins still pass.

### FIND-TASK-006-19 — REVISED / INCORRECT — schema security readiness is incomplete

**Sources:** STDS-003, DATA-R3-2. **Obligation:** REQ-156/157, AC-035, and `architecture/operations/deployment-and-release.md:144-154`. **Location:** `crates/wyrd/wyrd-sql/src/schema_check.rs:99-125`, `crates/wyrd/wyrd-sql/src/postgres.rs:107-121`, `crates/vala/vala-sql/src/postgres.rs:100-119`, `crates/wyrd/wyrd-server/src/main.rs:203-215`.

`forced_rls` examines only table flags, not the policy that makes RLS effective. Wyrd has no grant checks and Vala tests only app `USAGE` on `iceberg_catalog`. The owner can add `CREATE` on `wyrd` to `wyrd_app` or remove a `tenant_isolation` policy without altering either migration ledger or forced-RLS flags; one-off validation and serving boot still pass. This is a live security and availability failure, not a request for a general schema linter.

**Correction:** Extend the existing Wyrd/Vala SQL-owner readiness checks to verify the specific tenant-isolation policies and required/forbidden role privileges established by their migrations, including no app catalog access and no unrestricted platform DDL. Run the same security checks after owner migration and before serving readiness. Keep checks scoped to the approved schema/role contract, not arbitrary future SQL.

**Focused closure proof:** After a good migration, alter one required policy and one relevant grant (for example app `CREATE` on `wyrd`), and show both one-off validation and serve boot refuse; restore them and show ordinary tenant and platform work passes.

### FIND-TASK-006-20 — CONFIRMED / VIOLATION — raw pool in new library handle

**Sources:** STDS-004. **Obligation:** `architecture/agent-rules.md:6` raw-pool boundary. **Location:** `crates/wyrd/wyrd-sql/src/schema_check.rs:10-29`, with production callers in `wyrd-sql/src/postgres.rs:114-120` and `vala-sql/src/postgres.rs:100-106`.

The new public `SchemaCheck<'a>` stores `&PgPool` and accepts it in its constructor. This is a reusable SQL operation handle, outside the rule's pool-construction exception; the two serving owners and one-off command propagate raw pools into it. That creates another callable path around `TenantConn` and `OperatorPool` even though its current queries are read-only.

**Correction:** Keep serving readiness operations on the existing `WyrdPostgres`/`ValaPostgres` role-owned handles and approved operator capability, with no public general raw-pool operation handle. Preserve the one-off owner session as the migration boundary; fold common read-only validation into the existing owners without duplicating the security contract or introducing a new broad abstraction.

**Focused closure proof:** Run `mise run check:from-pools-allowlist` and the SQL/startup readiness tests; inspect changed library signatures and fields for no new raw `PgPool` propagation.

### FIND-TASK-006-21 — CONFIRMED / VIOLATION — missing rustdoc on setup writer

**Sources:** STDS-005. **Obligation:** `architecture/agent-rules.md:35` per-item Rust documentation and fallible `# Errors`. **Location:** `crates/wyrd/wyrd-server/src/main.rs:326-337`.

The new `Tee<W>` has documented fields, but its `Write::write` and `flush` methods have no rustdoc. `write` forwards a possibly partial terminal write and captures exactly that prefix; either method can return terminal IO failure. The methods are exercised by the one-off root setup path, so this is not dormant code.

**Correction:** Document the intent, partial-write/capture behavior, and `# Errors` on these two existing methods. No new wrapper or test is needed.

**Focused closure proof:** Inspect both method docs and run the normal Rust format/lint checks; the existing setup journey covers behavior.

### FIND-TASK-006-22 — CONFIRMED / INCORRECT — ready role published before peer socket serves

**Sources:** PEER-1. **Obligation:** REQ-160/162 and AC-036 reachable ready membership. **Location:** `crates/wyrd/wyrd-server/src/boot/mod.rs:856-896,1876-1906`, `crates/wyrd/wyrd-server/src/app/server.rs:366-397,712-742`, `crates/vala/vala-sql/src/queries/cluster_nodes.rs:197-221`.

Scribe activates its reserved role immediately after WAL recovery; Oracle activates after startup reconciliation. `WyrdServer::bind` takes the peer TCP socket later, and `BoundServer::run` begins serving it later still. Membership polling and remote Oracle/tail selection use fresh `ready=true` registry rows without consulting that process's local `/readyz` or `peer_plane` bit. During ordinary join another replica can dial a ready role whose socket is not bound; if bind fails, the row remains ready until shutdown or heartbeat expiry. A fused read can fail strictly during this window.

**Correction:** Use the existing reserved, unready role leases through recovery and peer listener bind. Activate the exact Scribe/Oracle fences only when the bound peer listener is serving; on bind/start failure leave them unready or explicitly deactivate them. Coordinate this in the current boot/server lifecycle and retain local one-process behavior and the `peer_plane` health state. Do not add another registry.

**Focused closure proof:** In the existing two-process peer journey hold or fail the second process before bind/serve and assert that the first process's ready snapshot excludes its roles. Release the hold, then assert discovery and remote Oracle/tail work.

## Prior-finding continuity and validation limits

`FIND-TASK-006-1` through `-12` remain closed on the inspected cumulative candidate. Their original owners and the recorded exact Eval, SQL, server, Oracle, Python and TypeScript journey outcomes were reviewed in the prior verdicts and R2 evidence; the Wave 1 Eval review found no reopened Eval gap. Those IDs are preserved and new IDs begin at `-13`.

This is static source validation. I did not rerun Docker, kind, SQL or full journey lanes. The recorded green runs validate the paths they exercised, but they did not exercise the wrong-identity, wrong-role, policy/grant mutation, startup race, or published-digest cases above. No bounded code correction requires a new product, public API, or unapproved architecture decision. Finding 13 instead needs an approved acceptance-criterion decision because the required published artifact does not exist.
