---
id: TASK-004
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
previous_id: TASK-005
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001]
req: [REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-034, REQ-035, REQ-036, REQ-037, REQ-038, REQ-040, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
inv: [INV-002, INV-003, INV-006, INV-007, INV-010, INV-012, INV-013, INV-017, INV-018]
ac: [AC-001, AC-002, AC-011, AC-014, AC-015, AC-016, AC-019, AC-026, AC-027, AC-028, AC-030, AC-031, AC-032, AC-048, AC-049, AC-050, AC-051]
---

# GitHub integrations, repository configuration and PR checks

## Outcome and Value

Implement tenant GitHub administration and repository PR automation around ordinary Verifiers. Reuse/extend the library's existing GitHubClient for every GitHub protocol operation; Wyrd owns tenant authority, trusted configuration, durable event admission and execution/delivery coordination.

## What Changes and What Must Stay True

Task 001 supplies the single Bifrost execution-state source. GitHub
installation/access, binding/generation/webhook dedup and independently retryable
Check delivery retain their own PostgreSQL control lifetimes. Delivery references
the selected execution/report, projects later status without changing verdict,
and does not require a legacy execution payload row or rerun verification.

| Implement | Required implementation logic |
|---|---|
| **Library GitHub operations** | Extend bohmian-code-review's outer provider crate for App authentication/installation tokens, installation/repository reconciliation, Check Run create/update/reconciliation and webhook protocol helpers. Reuse GitHubClient/GitHubHost, bounded transport and verified identity. Wyrd must not add an independent REST client, token implementation or provider retry stack. |
| **Tenant integration administration** | Add create/redacted-read/verified-association/rotate/disable/remove and repository reconcile/allow/revoke operations on HTTP/shared client/SDKs/CLI/MCP. Bind single-use installation callback state to authenticated tenant/admin; verify installation permissions/ownership. Each tenant has at most one active GitHub App installation association; each installation belongs to at most one tenant. Refuse conflicts without silently replacing an existing association. Use existing envelope sealing/key rotation; short-lived credentials stay server-only. |
| **Existing execute/enqueue API extension** | Extend both POST /v1/verification/execute and /enqueue with the same card_uid-or-repository request arm; preserve existing Card inputs/results and 200 result versus 202 execution-ID responses. Wyrd pins repository/PR revisions server-side; reject both/neither target arms before provider/engine work. Expose the agreed repository result/binding shapes through the same shared types and client. |
| **Repository authority and subjects** | Extend typed RBAC/scope contracts for integration read/write, repository source read, PR invoke and check publication. Access is installation ∩ explicit allowlist ∩ accepted Wyrd authority ∩ binding/operation. New allowlist is empty. Extend binding/execution/result subject with typed repository identity/target/PR revision; preserve Card subject wire behavior and subject-less Verifier registration, without fake Run/Card. |
| **Trusted configuration and declaration** | Extend strict WyrdConfig with verification.pull_requests[].verifier only. Add pinned Git-tree bytes to the existing loader's full parse/defaults/resolve/validate/order workflow and composite declaration/upload saga. Resolve root wyrd.toml and relative dependencies only from pinned PR target tree; refuse unsafe/remote/symlink imports and immutable version conflicts. Publisher and execution principals remain distinct. |
| **Binding reconciliation** | Validate the complete selection before dispatch. Persist branch-scoped stable check/family identity and exact version/config/dependency revision; preserve unchanged revisions, revise changed content and retire removed selections only on that branch. Missing/invalid/empty config produces action_required configuration check. |
| **Durable PR ingress and delivery** | Verify raw-body HMAC, association and size before durable acknowledgement; persist accepted delivery before 2xx. Dedupe host/App/delivery separately from logical binding/config/repository/PR/base/head execution identity; explicit rerun adds generation. Handle event policy/supersession monotonically. Submit existing Verifier engine. Persist independently retryable check delivery; reconcile ambiguous remote create and prevent terminal regression. Task 005 consumes default-push maintenance. |

Task 004 does not depend on extraction or Agent tooling: its repository automation proof uses an ordinary existing no-input code/assert Verifier. Default-push maintenance is durably pending until Task 005 supplies its consumer; never mark it completed merely because ingress acknowledged it. Both tasks use the same immutable library dependency lineage; Task 005 extends the delivered Task 004 revision.

The shared request/result/binding arms, integration views, permission strings and
scopes are fixed in [public contracts](../public-contracts.md). Implement them
in wyrd-spec and the shared server/client owners; do not design SDK-specific
subjects or permissions.

## Approach

1. Extend repository subject, permission/integration and trusted-config authority/contracts.
2. Deliver reusable App/repository/check operations through existing library GitHubClient and pin that revision.
3. Implement verified encrypted connections and explicit repository access using current security/SQL owners.
4. Extend the shared loader/declaration flow for pinned Git trees; reconcile bindings.
5. Implement durable webhook/generation admission and ordinary engine submission, then independent Check Run delivery.
6. Project public operations and prove trust, lifecycle, replay and bounded backlog behavior.

## Ordered Implementation Scenarios

### 1. Extend GitHub protocol operations

**Behavior.** Through the existing public library client, verify App installation/repository facts, obtain narrowed token and create/update/reconcile a check. Run the same provider cases for public GitHub and a GHES /api/v3 base, asserting endpoint prefixes, authentication placement and trusted archive redirects. Exercise bounded provider error/rate-limit responses and host restrictions; no Wyrd policy/scheduler enters the library.

**RED.** Public library journey asserts request identities/permissions, bounded response behavior and stable check association after an ambiguous response. Expected RED: the required App/check protocol operations are absent from the existing client. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Add operations to existing GitHubClient/provider facade using its transport, host and request policy; parameterize existing provider/pipeline journeys by host instead of duplicating clients or suites.

**REFACTOR.** Share token/identity/request logic; do not add a Wyrd REST wrapper. Keep this and earlier scenario tests green.

### 2. Administer an integration

**Behavior.** Admin creates/redacted-reads, verifies association, rotates, disables/removes integration. Assert the exact redacted view fields and no credentials in every response. Table: wrong/replayed callback state, forged installation, missing permissions, a second installation for the same tenant, an installation already associated
with another tenant, or caller without integrations:write → refusal/no secret disclosure.

**RED.** Public journey asserts association state, redacted payloads and zero source IO before verification for each refusal. Expected RED: verified tenant App integration operations do not exist. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Implement tenant-bound single-use association and encrypted credential lifecycle through existing RBAC/sealing and library App operations.

**REFACTOR.** Reuse security primitives without storing GitHub installs as human OIDC connections. Keep this and earlier scenario tests green.

### 3. Allow and revoke repositories

**Behavior.** Installation access alone permits no source. Explicitly allow numeric repository, then verify repositories:read/run/write independently, using exact host/repository scope and all. A repository run grant without verifier:run cannot invoke; integration management grants no source access. Revoke/suspend access during work: next IO refuses and affected active work cancels. Equal IDs on different hosts/tenants stay isolated; rename labels do not retarget identity.

**RED.** Assert provider requests and exact permission denial with explicit clients; retained evidence still needs ordinary read permission and is not silently purged. Expected RED: repository-specific grant intersection/current revocation is not implemented. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Implement typed scope matching and intersection at every external operation, using accepted authority plus current connection state.

**REFACTOR.** Keep one Wyrd permission plane, not an installation policy engine. Keep this and earlier scenario tests green.

### 4. Load shared/local Verifiers

**Behavior.** Two app repositories select the same exact independently registered Verifier without reading its definition repo; one also imports local YAML and transitive Agent/Prompt/Artifact files. Interrupted declaration/upload recovers; changed payload under an existing version conflicts; an invalid member prevents any suite dispatch.

**RED.** Exercise both direct execute and queued enqueue using the same repository target and inspect their exact 200/202 responses and repository result subjects. SDK declaration journey asserts exact registered identities, publisher attribution, import replay and zero partial suite calls. Round-trip the exact public-contract Card/repository request, result and binding arms through shared schemas and SDKs; assert unchanged existing Card JSON, decimal-string GitHub IDs, server-pinned revisions and rejection of both/neither request arms before provider/engine calls. Existing Card/Service bindings remain unchanged. Expected RED: the loader cannot read the pinned Git-tree source and reconcile repository bindings. Run [proof 4](#proof-4); names are planned until added/listed.

**GREEN.** Extend WyrdConfig and shared loader/declaration byte-source support; complete imports as publisher, then freeze exact references and repository binding before execution.

**REFACTOR.** Reuse the whole existing dependency traversal/upload saga, not another Git YAML loader. Keep this and earlier scenario tests green.

### 5. Enforce trusted target configuration

**Behavior.** Table: default/non-default targets → use their own pinned configuration without branch approval; PR-head edits → ignored for enforcement; unsafe imports → refuse; missing/empty/duplicate/conflicting selection → configuration failure; unrelated commit → same binding revision; changed dependency → new revision; removal/deletion → retire only target branch; branch rename → resolve the new ref’s pinned configuration through the same reconciliation.

**RED.** Assert pinned config/tree/digests, binding/check identities and no head-controlled/partial execution for each row. Expected RED: trusted branch-scoped PR configuration reconciliation is absent. Run [proof 5](#proof-5); names are planned until added/listed.

**GREEN.** Read literal root config from the pinned PR target tree and reconcile branch-scoped revisions under current-generation guards.

**REFACTOR.** Use existing binding identities and declaration validation; no service config fallback. Keep this and earlier scenario tests green.

### 6. Accept webhooks durably

**Behavior.** Invalid HMAC/body size/association admits nothing. Persistence failure returns retryable non-2xx. Accepted 2xx survives process loss. Duplicate delivery resumes failed admission; another delivery for the same logical revision creates no duplicate execution; explicit rerun gets a new generation.

**RED.** Real HTTP/SQL journey loses responses/restarts workers and asserts delivery/execution identities, counts and acknowledgement behavior. Expected RED: signed deliveries have no durable admission/dedup owner. Run [proof 6](#proof-6); names are planned until added/listed.

**GREEN.** Verify unchanged bounded raw bytes, persist delivery then process asynchronously under accepted authority; use separate unique provider/logical identities.

**REFACTOR.** Reuse bounded leased work/admission; no inline review or memory-only queue. Keep this and earlier scenario tests green.

### 7. Apply PR event policy

**Behavior.** Opened/reopened/synchronize/ready and retarget admit approved work; drafts wait; metadata-only edits do not retrigger; close cancels. New head/target supersedes unfinished old work; old deliveries cannot undo newer head/retarget/configuration. Default push durably queues maintenance.

**RED.** Table-driven HTTP/SDK journey asserts exact execution generations, target selection, cancellation and maintenance state after out-of-order events. Expected RED: PR head/target generations are not monotonically coordinated. Run [proof 7](#proof-7); names are planned until added/listed.

**GREEN.** Persist monotonic PR/configuration generation decisions before binding reconciliation/submission; Task 005 completes maintenance through public library lifecycle.

**REFACTOR.** Share generation checks across ingress/recovery instead of timestamp-only ordering. Keep this and earlier scenario tests green.

### 8. Deliver checks independently

**Behavior.** Table: passed→success; failed→failure; inconclusive/errored→action_required; cancelled/superseded→cancelled; timed_out→timed_out. Each selected binding has its own check/head; configuration failure has its own check. Lose remote-create reply, rotate credentials and replay running after terminal: one associated check, no terminal regression or engine rerun.

**RED.** Assert remote check association/state and unchanged model/execution count across delivery retries; links remain authorized/private and no comments/annotations are posted. Expected RED: Check Runs have no durable association/reconciliation delivery state. Run [proof 8](#proof-8); names are planned until added/listed.

**GREEN.** Persist delivery intent and reconcile ambiguous writes through library Check Run operations and existing effect/lease owners, separate from verdict.

**REFACTOR.** Reuse effect retry/accounting without a second generic dispatch framework. Keep this and earlier scenario tests green.

### 9. Bound repository automation

**Behavior.** With bounded representative deliveries/repositories, verify dedup/generation/binding/delivery claims use scoped indexes and bounded pages/batches. One delayed repository/provider must not serialize unrelated eligible work; no database transaction spans provider calls.

**RED.** Real SQL plan tests plus deterministic stalled-provider integration assert bounded returned work, indexed identity lookups and unrelated repository progress. Expected RED: new delivery/generation/binding queries and bounded claims are absent. Run [proof 9](#proof-9); names are planned until added/listed.

**GREEN.** Add necessary unique/scoped indexes and extend current bounded queue/delivery ownership; release unrelated permits during backoff.

**REFACTOR.** Reuse batching/leased-work patterns and avoid tenant-wide history scans. Keep this and earlier scenario tests green.

### 10. Keep delivery and dedup independent of run-state storage

**Behavior.** Finish an ordinary no-input Verifier while Check delivery is
held. Retry ambiguous delivery, duplicate/stale webhooks and source-read failures
without depending on PostgreSQL invocation/publication payload rows. There is one
logical review; Check association/supersession and current revocation remain
correct, and delivery retries never call the verification engine.

**RED.** Extend independent delivery/dedup proof with recovered Bifrost terminal
selection and a missing/unavailable state source. Current joins depend on old
execution rows. Run [lifecycle proof](#lifecycle-proof).

**GREEN.** Preserve current integration/generation/admission/delivery owners and
reference the foundation's accepted execution identity/result. Keep independently
changing delivery state in its existing owner and shared projection path.

**REFACTOR.** No terminal tombstone, copied delivery queue, new archive protocol
or re-review on provider retry.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-024–026; INV-002/003; AC-014–016 | Protocol, association and access proofs establish verified ownership, encrypted/redacted lifecycle, default-empty allowlist, typed grants and revocation. |
| REQ-027/034–038/040; INV-012/013; AC-002/026–028/030–032 | Loader/config proofs establish subject-less shared reuse, one TOML/loader, exact local imports, complete suite validation, branch trust and honest revisions; preserve Card subjects. |
| REQ-028–030; INV-006/007/010; AC-011/019 | Ingress/event/check proofs establish durable dedup, monotonic generations, correct terminal mapping and retry without review replay; all GitHub protocol operations reuse the library client. |
| REQ-032; AC-001/021/031 contribution | Use an existing ordinary no-input Verifier to prove repository binding→engine→check before Task 005 extraction exists. Mirror admin/config/execute/status/denial operations on all surfaces; Task 006 proves full review. |
| High-volume requirement | Backlog proof covers scoped unique identities/indexed lookups, bounded claims/pages and independent repository progress, not a synthetic QPS claim. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. Task 001 owns loading, acceptance, replay and old-consumer replacement. |

## Implementation Reference

Existing Operator/delivery claim/retry/fence and repository generation/webhook owners
remain operational after evaluation. Use Task 001 accepted-state terminal/replay reader
and keep stable execution/report references, rather than queue-row FK custody of
completed evidence.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| Library GitHubClient/GitHubHost; outer provider crate | Already owns verified bounded GitHub requests. Extend App/token/repository/check/webhook protocol helpers there; never add another Wyrd GitHub client. |
| WyrdConfig::load; wyrd-loader::load/build_declaration_input; resolve/validate/order; build_submission::prepare | Current loader performs the full filesystem graph workflow. Add pinned byte-source acquisition to that owner and preserve declaration/artifact validation/replay; do not duplicate traversal or parse independent leaf files. |
| Permission/PermissionScope; Caller/AcceptedAuthority; connection queries/sealing/keyring | Reuse typed RBAC, tenant SQL, envelope encryption and redaction/rotation primitives. GitHub association has its own domain records; not human sign-in or Operator connection records. |
| VerificationControl; NewBinding/project_bindings; VerifierExecutionQueue | Current subjects are Card-centric. Extend typed repository alternatives while preserving old payloads; keep engine/admission/recovery in existing owners. |
| verification::operators/leases/claims; existing delivery tests; SDK declare_and_pull/verify_in_real_time; MCP verification | Reuse effect fencing, bounded claims and current transport/journey harnesses. Add GitHub association/generation/reconciliation behavior without another scheduler or client. |

## Verification and Evidence

All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- cargo test --locked -p bohmian-code-review-pipeline --test pipeline_journey --all-features github_app_protocol_reuses_existing_client -- --exact --nocapture
```

### Proof 2

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::installation_association_is_verified_and_tenant_bound)"'
```

### Proof 3

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::repository_access_requires_every_grant)"'
```

### Proof 4

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::trusted_git_declaration_reuses_shared_loader)"'
```

### Proof 5

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::target_configuration_reconciles_per_branch)"'
```

### Proof 6

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::signed_webhook_acknowledges_durable_work)"'
```

### Proof 7

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::pr_events_cannot_supersede_newer_generations)"'
```

### Proof 8

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::check_delivery_is_independent_and_monotonic)"'
```

### Proof 9

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_repository_verification::webhook_backlog_uses_scoped_bounded_queries)"'
```

### GHES compatibility evidence

In the library checkout, reuse `mise run test:pipeline` and `mise run test:pipeline:journey`. Existing host/archive tests already cover GHES. Extend the current pipeline/provider cases to run with an enterprise `/api/v3` host, including App tokens, complete PR evidence and Check Runs; assert foreign/unsafe redirects refuse and tokens are not forwarded. Routine CI uses local controlled transports and needs no appliance credentials.

Before claiming compatibility with a GHES release, run the same real provider/journey path against a controlled GHES installation: App token → allowed repository → PR extraction/readback → verifier → Check Run → retained report. Configure the existing live helpers for enterprise hosts and portable execution first; record the tested appliance release and network/certificate setup. Live appliance verification is explicitly invoked with server-side credentials; local tests alone do not prove appliance compatibility. This is verification of the same integration, not another GHES implementation.

### Lifecycle proof

New selector is **planned**. Use the existing target/setup below, add/list the actual
test before RED, and record a nonzero selection plus the missing-behavior failure and
closure result. Extend the task's existing Python/TS/MCP public-closure case table with
its recovery outcomes rather than adding full suites; Task 006 uses existing
combined-surface cases. No runtime proof is claimed during planning.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::check_delivery_uses_retained_run_state)"'
```

### Public surface closure

Mirror integration create/redacted-read/associate/rotate/disable/remove, repository list/allow/revoke/PR activation, configuration, both repository execute/enqueue target arms, binding/check reads and individual permission denials. Assert the shared redacted integration, access/activation, binding/check and execution response shapes from the public-contract operation list on every surface. Extend this planned story as a case table of those named operations; a single successful source read is insufficient. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_repository_verification.py::test_repository_access_requires_every_grant'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/repository-verification.test.ts -t "^repository_access_requires_every_grant$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::repository_verification_repository_access_requires_every_grant)"'
```

### Upstream delivery

Run from bohmian-code-review after the focused public proofs:

```bash
mise run test:pipeline
mise run fmt:check
mise run lints
```

Record the delivered immutable library revision, lockfile and provider/pipeline evidence. No mutable branch, developer path or uncommitted external dependency is accepted; publishing/pushing is not authorized by this plan.

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No new GitHub client, token/retry stack, YAML/dependency loader, workflow service or fake Repository/Service Card. Protocol support/version changes belong in library authority. No PR-head enforcement, partial-suite success or blind ambiguous-check replay.

## Authority Links

[Shared authorization and wire contracts](../public-contracts.md).

[Approved spec, revision 18](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
