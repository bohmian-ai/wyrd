# Tenancy and Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Candidate remained `HEAD` throughout this review.

## Reviewed boundary

This pass reviewed only the changed Workflow graph-loading trust boundary:

- authored `path`/registration-only `Sibling` versus external `ref` semantics;
- exact Card reads, UID assertions, version and space identity, and Active status;
- tenant-qualified server registration resolution under `TenantConn`/RLS;
- missing, inactive, deleted, foreign-tenant, wrong-kind, and mismatched references;
- resolved graph validation before durable registration; and
- whether caller-controlled `CardRef` data can widen tenant or authorization scope.

No credential, secret, URL-fetch, cryptographic, dependency-manifest, or deployment-environment boundary was materially changed by this task.

## Authority and source coverage

| Boundary | Authority | Source/caller evidence | Result |
|---|---|---|---|
| Tenant selection and registry isolation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` “Security principles” and “Tenant and data isolation”; `architecture/references/doctrine/architecture-constraints.md` “Identity And Tenant Isolation” | `resolve_card_references` and `EffectiveSpecs::{validate_workflows,load}` use caller-created `&mut TenantConn`; `select_card_uids_by_ref_batch` and `get_card_by_uid` execute under RLS; service construction derives the connection from `caller.data_tenant_id` | PASS |
| Authenticated/audited external Card reads | `architecture/wyrd-security-posture.md` “Security principles”; `architecture/references/architecture/patterns.md` “Client Pattern” and “Server Pattern” | `WorkflowLoader::read` calls `Cards::get(CardSelector::exact)`; `/v1/cards/by-ref` authorizes and audits before the tenant-scoped service read | PASS, subject to TEN-SEC-001 because some authored external refs never reach this path |
| Exact identity and lifecycle | Revision 11 REQ-024/025/029 and INV-005; `architecture/wyrd-design.md` “Reference forms” and “Path resolution rules”; `architecture/references/doctrine/positioning-and-vocabulary.md` “CardRef Shape” | `CardSelector::Exact` asserts kind/space/name/version and optional UID; `WorkflowLoader::read` additionally requires `status.phase == "active"`; SQL excludes deleted Cards; integration coverage checks wrong kind, non-pin versions, UID mismatch, foreign tenant, pending, and deleted dependencies | PASS, subject to TEN-SEC-001 |
| Server composite validation and persistence | Revision 11 REQ-014/028 and AC-002/006; `architecture/wyrd-design.md` path/reference intent rules | `EffectiveSpecs::validate_workflows` resolves external references through Active tenant reads before `write_registration`; `write_registration` rechecks and locks Active external identities before binding UIDs and committing | PASS |
| Local versus external reference intent | `architecture/wyrd-design.md:1619-1622`; Revision 11 REQ-025 and TASK-002 packet-local seam | `wyrd-loader` preserves `Sibling` and `Ref` as distinct variants, but `WorkflowLoader::load_file` inserts all path-loaded bodies into `WorkflowGraph::bodies`, and `WorkflowGraph::{missing,agent}`/`GraphResolver` subsequently match only `CardRefIdentity` | FAIL — TEN-SEC-001 |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **TEN-SEC-001 — INCORRECT** — [`crates/shared/wyrd-client/src/workflow_loader.rs:95`](../../../../../crates/shared/wyrd-client/src/workflow_loader.rs), [`workflow_loader.rs:221`](../../../../../crates/shared/wyrd-client/src/workflow_loader.rs), and [`workflow_loader.rs:238`](../../../../../crates/shared/wyrd-client/src/workflow_loader.rs): local path-loaded bodies and registry-fetched bodies share one `HashMap<CardRefIdentity, (CardRef, Spec)>`, while `CardRefIdentity` intentionally excludes UID and reference provenance. `load_file` preloads every path dependency before `fetch_missing`; therefore a `Sibling` body with the same `(kind, space, name, version)` as an authored external `Ref` makes that external dependency appear present. `fetch_missing` skips `WorkflowLoader::read`, and `GraphResolver` hydrates the external slot from the local body. This violates REQ-025, INV-005, the task's requirement that external refs use exact Cards reads, and the design rule that an authored external `Ref` remains external even when it shares a named identity with a sibling. A reachable crafted bundle can include a path-loaded Agent or Prompt and an external `ref` with the same named identity; the nominal external branch then executes the caller-controlled local body without the expected registry authorization/audit, Active-status check, or stored UID consistency assertion. The caller already has authority to execute local bundle content, so this is not demonstrated as cross-tenant privilege escalation, but it is a concrete trust/provenance bypass for applications that rely on `ref` to select an approved registered dependency. **Correction:** preserve reference-form provenance through `WorkflowGraph` lookup so a `Sibling` is satisfied only by its path-loaded sibling body and a `Ref` only by the result of its exact Cards read; do not let a named-identity collision substitute one source for the other. **Closure proof:** add a `WorkflowLoader::load_file` test containing both a path dependency and an external ref with the same named identity but different bodies/UID expectations. Without a client it must return `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`; with a client it must issue the exact read and hydrate the external slot from the registered Active Card while the sibling slot retains the local body.

### Low / Defense In Depth

None.

### Positive Controls

- Tenant identity is not accepted from Workflow or `CardRef` payloads; server dependency resolution uses the verified caller's `TenantConn` and Postgres RLS.
- External registration references are selected only when Active and are rechecked inside the write transaction before their UIDs are persisted.
- Client exact reads assert the full named identity and any supplied UID against the returned Card, then reject non-Active lifecycle state before hydration.
- Foreign-tenant and deleted Cards collapse to not-found behavior at the existing Cards boundary; pending dependencies are refused before dispatch.
- The changed graph loading and registration paths do not resolve secrets, call providers, or execute tools during validation.

## Verification evidence and limits

- Reviewed the complete base-to-candidate diff and traced `WorkflowLoader::{load_file,load_registered,fetch_missing,read}`, `WorkflowGraph::{insert,missing,validate,hydrate}`, `GraphResolver`, `Cards::get(CardSelector::exact)`, the `/v1/cards/by-ref` authorization path, `EffectiveSpecs::{validate_workflows,load}`, `select_card_uids_by_ref_batch`, `get_card_by_uid`, and the registration write-time Active recheck.
- `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd/target mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow_loader::tests::hydrate_local_workflow_graph)'` passed (1 test). That test covers ordinary offline hydration and a ref with no local identity collision; it does not exercise TEN-SEC-001.
- The candidate integration test statically covers foreign tenant, pending/deleted dependency, wrong kind, invalid version, and UID mismatch outcomes. This reviewer did not rerun the Postgres target.
- No existing test combines a path-loaded dependency and an authored external ref with the same named identity, even though `wyrd-loader::tests::path_and_external_refs_keep_distinct_wire_projections` proves that this distinct-provenance shape is supported and reachable.

## Overall result

**FAIL** — one bounded, source-local trust/provenance defect remains: **TEN-SEC-001**.
