---
id: TASK-004
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
previous_id: TASK-005
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001-R4]
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

Each scenario follows an administrator or PR author. Protocol operations,
permission variants, ingress faults and delivery races are test cases within
those journeys, using the focused proofs below.

### Scenario 1 — Connect the GitHub App and configure repository verification

**Behavior.** A tenant administrator creates a redacted integration, completes
verified installation association, explicitly allows a repository and configures
verification. A team publishes reusable Verifiers; application repositories select
shared versions or local definitions through trusted target-tree `wyrd.toml`.
Configuration is validated as a complete suite before work can be admitted.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| App authentication, tokens, reconciliation and Check protocol calls | All GitHub transport/host/identity/retry behavior extends the existing library client. [Library protocol cases](#proof-1). |
| Create/read/associate/rotate/disable/remove; replayed/wrong callback, forged installation, missing permissions or conflicting association | Existing redacted view, no credentials exposed, verified tenant binding and one active installation; conflicts do not silently replace associations. [Administration journey](#proof-2). |
| Empty allowlist, exact/all repository scopes, independent read/run/write and verifier grants, revocation/suspension, equal numeric IDs across hosts/tenants | Installation alone grants nothing; exact intersection and current revocation hold without retargeting identity or deleting evidence. [Access cases](#proof-3). |
| Shared/local Verifiers, pinned imports, unsafe paths/symlinks, version conflicts and separate publisher/execution authority | Existing full loader/declaration/upload saga is reused; no independent YAML traversal. [Trusted declaration cases](#proof-4). |
| Any PR target branch, unchanged/changed/removed selection, missing/invalid/empty configuration and PR-head weakening | Stable per-branch reconciliation; target configuration governs and invalid selection produces an honest configuration Check, not an empty successful suite. [Configuration cases](#proof-5). |

**RED.** Extend the [administration journey](#proof-2) through repository allowlisting
and trusted configuration using the real SDK/server, supported by access/declaration/
configuration cases. Current shared integration/subject/permission/configuration
contracts cannot complete that setup. Confirm planned selectors before RED.

**GREEN.** Extend the library provider operations and existing Wyrd integration,
sealing/RBAC, config loader, declaration and binding owners. Preserve the agreed
shared request/result/view contracts and Card behavior; no new client or policy plane.

**REFACTOR.** Share existing complete workflows and short transactions rather than
a second GitHub client, token stack, loader or permission engine.

### Scenario 2 — A PR event runs verification and produces the correct Check

**Behavior.** After setup, a PR event selects the exact trusted configuration and
runs an ordinary Verifier through the existing queue/engine. The PR author sees
one Check per selected binding and exact head, progressing to its correct terminal
conclusion and authorized report links. Manual execute/enqueue use the same repository
target contract. Delivery retry does not rerun verification. Task 005 later adds
extraction; this task's journey uses an existing no-input Verifier.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Invalid signature/body/association/size and accepted/duplicate webhook delivery | Refuse invalid input before work; persistence failure returns retryable non-2xx, while accepted 2xx survives process loss. Delivery-ID dedup differs from logical execution generation. [Ingress cases](#proof-6). |
| Newer/older/duplicate PR generations, target changes, explicit rerun and default-branch maintenance events | Monotonic supersession and correct event policy; maintenance remains pending until Task 005 consumes it. [Event cases](#proof-7). |
| Verification outcomes and configuration failure | Passed → success; failed → failure; inconclusive/errored → action_required; cancelled/superseded → cancelled; timed_out → timed_out. Each binding/head has its own Check; invalid configuration has its own failure Check, never empty-suite success. [Check journey](#proof-8). |
| Lost create/update reply, credential rotation or late running update | One reconciled Check association, terminal monotonicity and no engine rerun. [Check journey](#proof-8). |
| Bounded backlog and a stalled provider/repository | Scoped indexed identity lookups, bounded claims/pages, unrelated progress and no transaction spanning provider IO. [Backlog cases](#proof-9). |
| Held delivery after terminal state, absent old PostgreSQL payload rows and unavailable Bifrost state | Existing identity/result remains selected; delivery stays independent, and source uncertainty does not create duplicate reviews. [Run-state integration cases](#lifecycle-proof). |

**RED.** Extend the [Check journey](#proof-8) to drive signed ingress → trusted
selection → queue/Verifier → exact-head Check against the real server and mock
GitHub, using the supporting cases above. Assert one logical execution, terminal
mapping and unchanged provider-call count during retries. This lifecycle is missing.

**GREEN.** Reuse current admission, accepted authority, generation/webhook and
independent delivery owners with library Check reconciliation. Use Task 001's
selected execution state; later delivery status never changes the verdict.

**REFACTOR.** Keep delivery and verification retry responsibilities separate;
no copied queue, terminal tombstones, private archive protocol, comments,
annotations or re-review on Check retry.

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

Proof numbers identify supporting tests, not additional implementation scenarios.
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

Supporting setup/declaration/API proof: two application repositories select the
same exact registered Verifier without reading its definition repository; one
also imports local YAML and transitive Agent/Prompt/Artifact files. Assert exact
identities/publisher attribution, interrupted upload/declaration replay, immutable
version conflicts and no dispatch for an invalid suite member. Round-trip the
public-contract Card/repository request, result and binding arms through shared
schemas and SDKs: unchanged existing Card JSON/Service bindings, decimal-string
GitHub IDs, server-pinned revisions, exact execute 200/enqueue 202 responses, and
both/neither target arms refused before provider or engine calls.

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

## Obligations

Line numbers refer to this file before the ledger was appended. "P<n>" is the
planned proof selector of that number; each row's assertion lives in the named
test once written.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| L-01 | 30, 70 | App installation verification, narrowed token exchange, installation repository listing, Check create/update/list and webhook verify/decode live in the library over GitHubClient's transport/retry/bounds | P1 `github_app_protocol_reuses_existing_client` |
| L-02 | 30, 99 | Lost Check create is not replayed; reconciled by external_id | P1 Ambiguous/Reconcile + single call count |
| L-03 | 154 | GHES `/api/v3` host runs the same App/token/check calls; only App endpoints see the JWT, others only the narrowed token | P1 host loop |
| L-04 | 194 | Wyrd pins an immutable pushed library revision (git+rev), no path/branch | Cargo.toml/Cargo.lock audit |
| A-01 | 31, 71 | Create 201 redacted view; read/rotate/disable/remove 200 redacted; credentials never returned | P2 |
| A-02 | 31, 71 | Association uses single-use state bound to the authenticated tenant/admin; replayed or wrong-tenant/admin state refuses | P2 |
| A-03 | 31, 71 | Forged installation (GitHub says other App / not found) and missing permissions refuse; nothing activated | P2 |
| A-04 | 31, 71 | One active installation per tenant; an installation in another tenant refuses without disclosure or replacement | P2 |
| A-05 | 31 | Rotation with omitted field keeps it; null/empty refuses; failed validation keeps current credentials | P2 |
| A-06 | 31 | Credentials sealed with existing sealing keyring and included in key rotation | P2 + sealed-table rewrap unit |
| R-01 | 33, 72 | New installation (incl. `all` selection) allows no repository | P3 |
| R-02 | 33, 72 | Exact and all repository scopes; read/run/write independent; run also needs verifier:run | P3 |
| R-03 | 33, 72 | Revocation and installation suspension block new work without deleting evidence | P3 |
| R-04 | 33, 72 | Equal numeric repository IDs on different hosts/tenants are distinct | P3 |
| R-05 | 33 | integrations:read/write and repositories:* permission strings/scopes as in public-contracts | P3 + permission unit |
| X-01 | 32, 147 | execute (200) and enqueue (202) accept the repository arm; Card arm unchanged | P4 |
| X-02 | 32, 147 | Both/neither target arms refused before provider/engine work | P4 |
| X-03 | 32, 147 | Result subject repository wrapper with server-pinned target_ref/base/head and decimal-string IDs; Card result JSON unchanged | P4 |
| C-01 | 34, 73 | Strict WyrdConfig `[[verification.pull_requests]] verifier = [...]`; unknown keys refuse | P4 + config unit |
| C-02 | 34, 73 | Git-tree bytes go through the shared loader workflow; local YAML and transitive imports declared under the publisher principal | P4 |
| C-03 | 34, 73 | Absolute/traversal/symlink/submodule/remote imports refuse | P4 |
| C-04 | 34, 73 | Immutable version conflict refuses; no dispatch for an invalid suite member | P4 |
| C-05 | 34, 73 | Two application repositories reuse one registered Verifier version without reading its definition repository | P4 |
| C-06 | 34 | Publisher and execution principals are distinct and attributed | P4 |
| B-01 | 35, 74 | Per-branch stable check identity; unchanged selection reuses revision; changed revises; removed retires only on that branch | P5 |
| B-02 | 35, 74 | Configuration read at pinned target base, never PR head | P5 |
| B-03 | 35, 74 | Missing/invalid/empty configuration → action_required configuration Check, no empty success | P5, P8 |
| I-01 | 36, 97 | Invalid signature/body/association/size refused before persistence | P6 |
| I-02 | 36, 97 | Accepted delivery persisted before 2xx; persistence failure → retryable non-2xx | P6 |
| I-03 | 36, 97 | Delivery-ID dedupe distinct from logical execution identity; duplicate logical delivery does not duplicate work | P6 |
| E-01 | 36, 98 | Newer base/head supersedes unfinished older work; older event cannot supersede newer | P7 |
| E-02 | 36, 98 | Draft waits; ready_for_review admits; closed stops unfinished work; base edit retargets; metadata edit ignored | P7 |
| E-03 | 36, 98 | Explicit rerun adds a generation | P7 |
| E-04 | 36, 38 | Default-branch push records maintenance durably pending, never completed by ingress | P7 |
| K-01 | 99 | passed→success, failed→failure, inconclusive/errored→action_required, cancelled/superseded→cancelled, timed_out→timed_out | P8 |
| K-02 | 99 | One Check per binding/head; configuration failure has its own Check | P8 |
| K-03 | 100 | Lost create/update and credential rotation reconcile to one Check; late running cannot regress terminal; no engine rerun | P8 |
| K-04 | 36 | One logical execution and unchanged provider-call count during delivery retries | P8 |
| Q-01 | 101 | Scoped indexed identity lookups, bounded claims/pages, unrelated repository progress, no transaction across provider IO | P9 |
| S-01 | 104 | Held delivery after terminal state uses retained run state; no old PostgreSQL payload rows; unavailable Bifrost state does not duplicate reviews | Lifecycle proof |
| U-01 | 166 | Integration/repository/execute/binding/check operations and denials on HTTP/OpenAPI, Rust, Python, TypeScript, CLI, MCP | public-surface selectors |
| N-01 | 199 | No new GitHub client/token/retry stack/loader/workflow service/fake Card; no PR-head enforcement; no partial-suite success | diff audit |

## Implementation Evidence

### P4 notes (trusted declaration)

- Reuse map: `wyrd_loader` stays the only loader. It gains `load_checkout`, which uses a contained `PathSandbox` that refuses absolute references. The existing `load` admits them with an advisory, which would let a server read host paths. It also gains `DeclarationInput::phases`. The registry rule in `wyrd-design.md` §Heavy artifacts declares each artifact-bearing Card alone. REQ-038 requires reusing heavy-artifact completion, and no existing workflow declared a transitive heavy import. Phases therefore declare each heavy Card alone, then the composite, with its sibling references rewritten to direct references.
- Server declaration reuses the `wyrd_client::cards::prepare_declaration` stamping, `declare_card`, `complete_card` and storage reconcile. `upload_server_bytes` shares `upload_local_blob`'s pending-row validation. The idempotency key is `repository-declaration-{sha256(host,repo,base,path)}-{phase}`.
- Diagnosis (P4 "symlinked import" assertion):
  - **Symptom:** `summary.contains("judge.yaml")` failed at repository_verification.rs:2574.
  - **Evidence:** the summary was `review.yaml: [WYRD_LOADER_400_IO] No such file or directory; … unresolved path reference at spec.requirements[0].check.agent`.
  - **Cause:** `materialize` omits symlinks by design. The loader's IO diagnostic names the importing file and the slot, not the reference.
  - **Fix site:** the test expectation only (it now expects `check.agent`). Refusal behavior is correct.
  - **Diagnostician (fresh, read-only):** confirmed the same cause. It noted the optional improvement of naming `ref_path` in `resolve_contained`. Not taken: it is unrequested and would change messages on every loader surface.
- Diagnosis (P4 first wait):
  - **Symptom:** waiting for `Delivered` on the in-progress local Check never settled.
  - **Cause:** CheckPublisher keeps an unfinished Check leased and pending until the execution is terminal (the held Agent upstream never answers).
  - **Fix:** the test waits for a created Check Run and a dispatched execution. Not a timing change.
- Process exception: the P4 server declaration code was written before its journey. The first run's RED was the loader refusing the transitive heavy Artifact (`WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION`), which drove `phases`.
- Passing: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc '… -E "test(/^repository_verification::/)"'` gave 7 passed. `wyrd-loader` lib gave 53 passed (`checkout_phases_declare_imported_artifacts_alone_first`, `contained_sandbox_refuses_absolute_target`). `wyrd-server` `verification::repositories::tests::project_maps_every_status_without_a_verdict` passed.

### P9 notes (backlog and stalled providers)

- Gap: `repository_checks` and `repository_bindings` read without `LIMIT`, and `RepositoryCheckPage`/`RepositoryBindingPage` had no `next_cursor`. They now follow the repository listing's keyset pattern: `cursor` on `RepositoryCheckQuery` and the new `RepositoryBindingQuery`, a nullable `next_cursor`, and pages of 100. A foreign cursor gets `400 WYRD_SPEC_400_VALIDATION`. The Rust client's `bindings`/`checks` take the cursor.
- Deviation (mechanics): the planned selector `wyrd-server … pg_repository_verification::webhook_backlog_uses_scoped_bounded_queries` became two tests at the owners.
  - The SQL properties are an inline `wyrd-sql` `pg_tests`, following the repository default for inline tests and the existing `verifier_executions` EXPLAIN pattern. That covers EXPLAIN over private SQL consts, bounded per-pull-request claims, keyset pages and terminal monotonicity.
  - Unrelated progress and no-transaction-across-provider-IO need the real worker and mock GitHub, so they are the SDK journey `repository_verification::stalled_repository_leaves_others_progressing`.
  - Commands:
    - `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --lib --test-threads=1 -E "test(=queries::repository_reviews::pg_tests::webhook_backlog_uses_scoped_bounded_queries)"'` → 1 passed.
    - `… -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::stalled_repository_leaves_others_progressing)"` → 1 passed.
- A new index migration was tried and removed. The test passed without it because `repository_reviews_identity` already leads with tenant/host/repository/pull request.
- Process exception: pagination was written before the pg test. Mutation evidence: with `LIMIT $5 + 1000` the test failed with `left: [250, 0] right: [100, 100, 50]`.
- Diagnosis (stalled journey):
  - **Symptom:** `never settled: 0` waiting on `/repositories/102/tarball/{BASE_SHA}`.
  - **Evidence:** the trace shows `stall-1` accepted and nothing after it. `review()` reads the pull request first (`repositories.rs:341`), and the archive comes later (`:363`, `:574`).
  - **Cause:** the stalled pull request read blocks before any tarball request, so the test waited on the wrong path.
  - **Fix site:** the test wait path, now `/repositories/102/pulls/5`. Server and fixture unchanged.
  - **Diagnostician (fresh, read-only):** same cause and fix site; no other callers affected.

### Lifecycle notes (S-01)

- `repository_verification::check_delivery_uses_retained_run_state`:
  - Check writes are refused until the execution is `Completed`.
  - The test asserts the dropped PostgreSQL execution-state tables are absent (`existing_relations_for_test`).
  - It makes Bifrost tail discovery unavailable and redelivers the same event under a new delivery id. The trace shows the publisher retrying with `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`.
  - It asserts one review with the same execution and no `action_required`. After availability returns, it asserts one success Check Run from the same execution.
- Defect found and fixed: `get_execution` documents that a terminal result Scribe has not yet published is absent, and `project()` concluded `action_required` from that gap. That would contradict S-01 and K-01. A completion without a visible result now projects `in_progress` and polls again.
  - RED: `verification::repositories::tests::project_maps_every_status_without_a_verdict` failed with `left: ("completed", Some("action_required")) right: ("in_progress", None)`.
  - GREEN: the same test passes.
- Process exception: the journey passed on its first run, because the outage surfaced as a retryable 503 rather than as an absent result. The absent-result path is proven by the unit test above.
- All `repository_verification::` journeys: 9 passed (60.1s).

### Earlier diagnoses and process exceptions (P2, P3, A-06, P5–P8)

- P2, P3, A-06, P5, P6, P7, P8: production code for the administration, access, sealing rewrap, configuration, ingress, event and Check slices was written before each journey existed. Each journey was then run against it. Recorded as process exceptions, not RED.
- P7 diagnosis:
  - **Symptom:** the "slow" Verifier finished in 16ms, so supersession found nothing unfinished.
  - **Cause:** the code sandbox deliberately does not handle `time.sleep` (OsCall → NotHandled). The fixture could not hold an execution open.
  - **Fix site:** the fixture. The slow Verifier is now an Agent check against a wiremock upstream with a 600s delay. Not a timing change in production.
- P6 diagnosis:
  - **Symptom:** the ingress journey's recorded-delivery assertions saw deliveries already consumed.
  - **Cause:** the new RepositoryWorker delivery consumer processed recorded deliveries during the ingress journey.
  - **Fix site:** the test setup. The ingress journey runs without a delivery consumer (commit 14a0c4850).
  - **Diagnostician (fresh, read-only):** same cause and fix site.

### Evidence table

`RV` = `sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs`; each journey is cited at its function line, followed by the assertions it contains. `LIB` = `bohmian-code-review-pipeline/tests/pipeline_journey.rs` at pinned rev `9fdf6dc77a329f34d96b7ad4f397331d2d18216b`. All `RV` selectors use the Proof 2 wrapper (`-p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1`); `-E "test(/^repository_verification::/)"` gave 9 passed (59.2s) after the final assertion changes.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| L-01: App/token/repository/check/webhook protocol in the library over GitHubClient | library `app.rs:141,173` (App client wraps `GitHubClient`), `checks.rs:166` (`impl GitHubClient`), so App and Check calls share its transport, retry policy and bounds; Wyrd `components/github/provider.rs` only signs `AppJwt` | LIB:801-831, 855-874, 914-929, 957-967; library `webhook.rs:440` `signature_covers_the_unchanged_body` (positive verify); library `github.rs:2834` `rate_limits_retry_within_bounds_or_report_typed_outcomes` (the shared retry policy); RV:1026 correctly signed deliveries answer 202 through the library verify | P1 passed at the pinned rev; library unit tests at the same rev; RV ingress journey | PASS |
| L-02: lost create not replayed; reconciled by external_id | library `Disposition::Reconcile` | LIB:872-879, 893-898 (one `CreateCheckRun` call, adopted run 77 by `binding-1:gen-1`) | P1 | PASS |
| L-03: GHES `/api/v3` same calls; JWT only on App endpoints | library host handling | LIB:772-774 (public + GHES loop), 932-947 | P1 | PASS |
| L-04: immutable library pin | `Cargo.toml:271`, `Cargo.lock:993,1006,1052-1054`, `wyrd-server/Cargo.toml:31` (`workspace = true`) | `git+…?rev=9fdf6dc…`, no path/branch | Cargo.lock audit | PASS |
| A-01: create 201, read/rotate/disable/remove 200, redacted | `components/github/routes.rs:116` (`StatusCode::CREATED`), `service.rs` views | RV:142 raw HTTP "create answers 201", "read answers 200", "rotate answers 200", "disable answers 200", "remove answers 200", each decoded as `GitHubIntegrationView`; 19 outcomes through `assert_redacted`; `pg_openapi_contract.rs:1992` response `$ref`s and no `private_key`/`webhook_secret` in the view schema | RV `installation_association_is_verified_and_tenant_bound`; `pg_openapi_contract::github_contract_publishes_every_operation_and_refuses_a_foreign_cursor` | PASS |
| A-02: single-use state bound to tenant/admin | association state in `service.rs` | RV:142 unknown, other-admin (same tenant) and replayed state → `GitHubAssociationRefused`; "state is bound to the tenant that began it": tenant A state completed by tenant B on its own integration → `GitHubAssociationRefused` | RV association journey | PASS |
| A-03: forged/other-App/missing-permission/suspended refuse, nothing activated | `service.rs` complete_association | RV:142 installations 8, 9, 10, 12 refuse; status stays `Pending` | RV association journey | PASS |
| A-04: one active per tenant; cross-tenant claim refuses without disclosure | unique active index + service | RV:142 second active → `Conflict`; other tenant claim → `Conflict` without the id; tenant-isolated list/get; installation still 7 | RV association journey | PASS |
| A-05: omitted kept; null/empty refuse; failed validation keeps current | `RotateGitHubIntegrationRequest`, service rotate | RV:142 empty/blank/rejected key → `Invalid`, reconcile still authenticates; `crates/wyrd-spec/src/repository.rs` `rotation_keeps_omitted_and_refuses_null` | RV association journey; wyrd-spec lib | PASS |
| A-06: sealed with keyring, included in rotation | `components/github/credentials.rs` | `credentials.rs:176` `key_rotation_rewraps_stored_integration_credentials` (resealed, new key opens, old fails) | wyrd-server lib pg test | PASS |
| R-01: new installation (incl. `all`) allows nothing | reconcile inserts `allowed=false`; fixture serves `repository_selection: "all"` (`wyrd-testing/src/github/mod.rs`) | RV:630 empty before reconcile; `[("101", false), ("102", false)]` with all-selection; verify → `RepositoryNotAllowed` | RV `repository_access_requires_every_grant` | PASS |
| R-02: exact/all scopes; read/run/write independent; run needs verifier:run | `wyrd-runtime/src/permission.rs`, service `inspect` | RV:630 all/exact run, other-repo exact, read+write without run, and `repositories:run` + `verifier:read` without `verifier:run` → `PermissionDeniedRbac`; run grants no read, read/write no cancel; `permission.rs` `repository_grants_are_exact_and_action_independent` (exact host/id, and a read/run/write matrix where each action covers only itself) | RV access journey; wyrd-runtime lib `permission::tests::repository_grants_are_exact_and_action_independent` passed | PASS |
| R-03: revocation and suspension block new work, evidence kept | service admission checks | RV:630 revoke → `RepositoryNotAllowed` + "revocation keeps recorded evidence"; suspend → `RepositoryNotFound` + "suspension keeps recorded evidence" | RV access journey | PASS |
| R-04: equal ids on other hosts/tenants distinct | identity includes tenant + host | RV:630 public host and other tenant → `RepositoryNotFound`, other tenant list empty; permission unit host exactness | RV access journey | PASS |
| R-05: integrations/repositories permission strings | `permission.rs` | `permission.rs` scope parse units; RV:142 `integrations:read` cannot create/associate, viewer cannot list | wyrd-runtime lib; RV association journey | PASS |
| X-01: execute 200 / enqueue 202 repository arm; Card arm unchanged | `VerificationRequest.repository` | RV:630 SDK verify/submit; RV:2494 raw execute 200 and enqueue 202 on the repository arm; Card arm execute 200 with `subject.uid` and no `repository`, Card arm enqueue 202 with `execution_id`; OpenAPI `VerificationRequest` publishes `card_uid` and `repository` | RV access + declaration journeys; OpenAPI github contract test | PASS |
| X-02: both/neither refused before provider/engine | request validation | RV:2494 both/neither → 400 `WYRD_SPEC_400_VALIDATION` for execute and enqueue, pulls request count unchanged | RV `trusted_git_declaration_reuses_shared_loader` | PASS |
| X-03: repository subject pinned, decimal-string ids; Card JSON unchanged | `VerificationSubject` | RV:630, RV:2494 pinned `target_ref`/base/head and `"101"`; `repository.rs` `card_subject_keeps_raw_card_ref_json`, `repository_subject_is_wrapped` | RV journeys; wyrd-spec lib | PASS |
| C-01: strict `[[verification.pull_requests]]` | `wyrd-config/src/config.rs` | `config.rs:270` `pull_request_verification_selects_verifiers_strictly`; RV:2127 unknown key → configuration Check | wyrd-config lib; RV branch journey | PASS |
| C-02: git-tree bytes through the shared loader; publisher declares | `wyrd_loader::load_checkout`, `DeclarationInput::phases` | RV:2494 local Verifier/Artifact/Agent/Prompt declared once, active, by publisher; interrupted declaration completes; `wyrd-loader` `checkout_phases_declare_imported_artifacts_alone_first` | RV declaration journey; wyrd-loader lib | PASS |
| C-03: absolute/traversal/symlink/submodule/remote refuse | contained `PathSandbox` | RV:2494 eight unsafe cases → one `action_required` configuration Check, no binding/execution; `contained_sandbox_refuses_absolute_target` | RV declaration journey; wyrd-loader lib. The submodule case serves a path absent from the tarball, which is how a GitHub archive presents a gitlink (`repositories.rs:821` treats it as missing), so it is refused as a missing import | PASS |
| C-04: immutable conflict refuses; no dispatch of an invalid suite | declaration saga | RV:2494 conflict case → configuration Check, one `revision-ready@1.0.0` row, bindings stay 2 | RV declaration journey | PASS |
| C-05: two repositories reuse one version without reading its definition | binding by registered uid | RV:2494 "a second repository reuses the registered version"; `repositories_read() == {101, 102}` "reuse reads only the application repositories" | RV declaration journey | PASS |
| C-06: publisher and execution principals distinct, attributed | integration principals | RV:2494 publisher ≠ executor; Cards attributed to publisher; executions run as executor | RV declaration journey | PASS |
| B-01: per-branch identity; unchanged reuses; changed revises; removed retires on that branch | `project_bindings` per `target_ref` | RV:2127 per-branch bindings; unchanged keeps; changed revises same `binding_id`; removed retires, release untouched | RV `target_configuration_reconciles_per_branch` | PASS |
| B-02: config at pinned base, never head | intake reads base tarball | RV:2127 checks follow base config; head tarball count 0 | RV branch journey | PASS |
| B-03: missing/invalid/empty → action_required configuration Check | configuration Check | RV:2127 three cases → one `Wyrd / configuration` `action_required`, no binding/execution; RV:1392 configuration Check | RV branch + checks journeys | PASS |
| I-01: invalid signature/body/association/size refused before persistence | `components/github/webhook.rs` | RV:1026 401 `WEBHOOK_REJECTED` cases, 413 over limit, no rows | RV `signed_webhook_acknowledges_durable_work` | PASS |
| I-02: persisted before 2xx; failure → retryable non-2xx | webhook insert then 202 | RV:1026 202 only with pending row; refused insert 503 no row; redelivery 202; rows survive restart | RV ingress journey | PASS |
| I-03: delivery-id dedupe distinct from logical identity | delivery table + review identity | RV:1026 same id 200 no new row, new id own row; RV:1392 same logical event under new id leaves checks and writes unchanged | RV ingress + checks journeys | PASS |
| E-01: newer supersedes unfinished older; older cannot supersede newer | `SUPERSEDE_REVIEWS_SQL` | RV:1777 newer head cancels old Check/execution; "an older event did not supersede the newer work" | RV `pr_events_cannot_supersede_newer_generations` | PASS |
| E-02: draft waits; ready admits; closed stops; base edit retargets; metadata ignored | event handling in `repositories.rs` | RV:1777 "a draft waits"; "ready_for_review admits the review"; closed cancels; base edit → `refs/heads/release`; "a metadata edit did not supersede the newer work" (isolated drain) | RV events journey | PASS |
| E-03: rerun adds a generation | rerun path | RV:1777 same subject, new `execution_id`, prior Check cancelled | RV events journey | PASS |
| E-04: default-branch push records maintenance pending | push handling | RV:1777 only `refs/heads/main` recorded, `pending` after drain | RV events journey | PASS |
| K-01: verdict/status → conclusion mapping | `verification/repositories.rs` `project` | `repositories.rs:1395` `project_concludes_completed_verdicts` (passed→success, failed→failure, inconclusive→action_required); `:1371` `project_maps_every_status_without_a_verdict` (cancelled, timed_out, errored); RV:1392 passed/failed/broken; RV:1777 superseded→cancelled | wyrd-server lib `verification::repositories::tests::` 2 passed; RV journeys | PASS |
| K-02: one Check per binding/head; configuration Check separate | CheckPublisher | RV:1392 one Check per binding on exact head, 3 executions; configuration Check has no binding/execution | RV `check_delivery_is_independent_and_monotonic` | PASS |
| K-03: lost create/update and rotation reconcile to one Check; no regress; no rerun | CheckPublisher reconcile + `ADVANCE_CHECK_SQL` | RV:1392: lost create and lost update (`lose_next_check_update`, `check_update_lost()` asserted) leave one completed run per name; held Checks (writes refused) across a private-key rotation deliver one run each with the same `(name, execution_id)` map ("rotation reruns nothing"); refused configuration writes retry into one Check; redelivery leaves `again == settled`; `repository_reviews.rs:1119` late `in_progress` leaves completed/failure v3 | RV checks journey; wyrd-sql pg test | PASS |
| K-04: one logical execution; provider calls unchanged during retries | delivery idempotency | RV:1392 one execution per binding; check and check-run counts unchanged after redelivery | RV checks journey | PASS |
| Q-01: scoped indexed lookups, bounded claims/pages, unrelated progress, no txn across IO | `wyrd-sql/src/queries/repository_reviews.rs` consts; keyset pages in `service.rs` | `repository_reviews.rs:1119` claims ≤10 one per PR, pages 100/100/50, foreign cursor empty, EXPLAIN without Seq Scan; RV:2942 unrelated repo succeeds, oldest open txn younger than stall, stalled repo publishes nothing | `-p wyrd-sql --lib -E "test(=queries::repository_reviews::pg_tests::webhook_backlog_uses_scoped_bounded_queries)"` 1 passed; RV `stalled_repository_leaves_others_progressing` | PASS |
| S-01: held delivery uses retained run state; no old PG rows; unavailable state does not duplicate | `project` completed-without-result → in_progress | RV:3035 not delivered while held; old relations absent; unavailable → one Check, same execution, no `action_required`; then one success run from the same execution | RV `check_delivery_uses_retained_run_state` | PASS |
| U-01 HTTP/OpenAPI | routes + `into_params(parameter_in = Query)` + webhook `security(())` | `pg_openapi_contract.rs:1992`: all 15 operations with statuses; response `$ref` per operation (GitHubIntegrationView, GitHubAssociationStart, RepositoryAccessPage/View, RepositoryBindingPage, RepositoryCheckPage) and list items; no credential property in the view schema; execute/enqueue `VerificationRequest` publishes `card_uid` and `repository`; query `cursor`; live first page and foreign cursor 400. Webhook `security(())` proven by `unauthenticated_requests_match_each_operation_declared_security` | `-p wyrd-server --features test-support --test integration -E "test(/^pg_openapi_contract::/)"` 24 passed; github test rerun after the schema assertions passed | PASS |
| U-01 Rust | `wyrd-client/src/github.rs` | RV:142 (create/read/rotate/disable/remove statuses, association, redaction); RV:630 (list/reconcile/allow/activation view/revoke, both arms, every write refused without `integrations:write`, bindings/checks refused without `repositories:read`); the other RV journeys for configuration, bindings and Checks | RV 9 passed (63.5s) | PASS |
| U-01 Python | `sdks/wyrd-sdk-python/src/github.rs`, `python/wyrd/github` | `test_repository_verification.py:160`: write-denial table for all 10 writes `:262-283` (`code == DENIED` at `:280`, unchanged state at `:282-283`); exact `INTEGRATION_FIELDS` at `:197,234,243,456,458`; `ACCESS_FIELDS` at `:258,318,325,428` | Python selector 1 passed (rerun at `1341920bb`) | PASS |
| U-01 TypeScript | `native/src/github.rs`, `wyrd/src/index.ts` `GitHub` | `repository-verification.test.ts:158`: reader create/beginAssociation/completeAssociation refused `:215-218`; rotate/disable/remove/reconcile/allow/revoke/setPullRequests refused up to `:249` (each `DENIED` = `WYRD_PERMISSION_403_DENIED_RBAC`, `:74`); unchanged state `:254`; exact integration keys `:238`; exact access keys `:285`; exact Check keys `:364` and binding keys `:367`, with value checks | TS selector 1 passed (rerun at `487ccb0b6`) | PASS |
| U-01 MCP | `wyrd-server/src/mcp/github.rs` (15 tools), `mcp/verification.rs` | `wyrd-mcp/tests/bifrost/mcp/verification.rs:5148`: configured tree + signed webhook `:5403-5437`; delivered Check fields `:5464-5471`; binding fields `:5482-5490`; write denials incl. complete_association/allow `:5572-5609`; exact integration keys `:5237`; access view `:5362` | MCP selector 1 passed (rerun at `1341920bb`) | PASS |
| U-01 CLI | `wyrd-cli/src/github.rs` (`wyrd github`, 17 verbs) | `wyrd-cli/tests/card_lifecycle.rs:3227`: configured tree + signed webhook `:3542-3575`; Check `:3593-3611`; binding `:3618-3629`; `bindings` and `checks` without `repositories:read` refused (`:3673`); all 10 write verbs refused `:3684-3707`; exact integration keys `:3347`; access view `:3440` | CLI selector 1 passed (rerun at `1341920bb`) | PASS |
| N-01: no new client/token/retry/loader/workflow/fake Card; no PR-head enforcement; no partial-suite success | diff audit: all transport through the library `GitHubClient`; Wyrd only signs `AppJwt`; `wyrd_loader` the only loader; no Repository/Service Card kind; config read at base (B-02); invalid member → configuration Check, no dispatch (C-04) | B-02, C-04 assertions above | `git diff main..HEAD` audit | PASS |

Per-surface omissions (each covered by the Rust journey on the same server contract): Python/TS/MCP/CLI do not repeat the full association-refusal matrix, suspension, or equal-id-on-another-host. Python and TS cannot construct a both/neither request through the handle (X-02 is proven at HTTP in RV:2494).

Process exceptions in this pass:
- `project_concludes_completed_verdicts` characterizes existing production code. No RED because the `Some(_)` arm already mapped inconclusive.
- The CLI test was written alongside its command (no RED run).
- R-03 suspension evidence, C-05 `repositories_read`, the isolated E-02 metadata drain and the A-01 raw HTTP statuses all strengthen existing journeys that passed on their first run.

U-01 defects found by the OpenAPI test and fixed at their owners:
- The repository queries were published as path parameters. Each now has `into_params(parameter_in = Query)`.
- The webhook route inherited token security. It now has `security(())` and documents its 400.

`docs/src/content/docs/reference/mcp-tools.md` lists the new tools.

Owner checks at `2eb39b1c6`:
- Passing: `mise run fmt`, `mise run lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`, `py:lints`, `py:typecheck`, `ts:typecheck` and `git diff --check main..HEAD`.
- `docs:check` passes after the README quickstart snippet was synced with `examples/quickstart/quickstart.ts`. Its first run failed `readme.test.ts`.
- The association journey with the raw HTTP 201/200 assertions passed (1 passed).

Final status: IMPLEMENTED at `487ccb0b6`.
- Three fresh read-only checks were run:
  - The first named L-01, A-01, A-02, K-03, X-01, R-02 and U-01.
  - The second named R-02, U-01 TypeScript and U-01 CLI, plus citations.
  - The third, narrow, check confirmed those resolved, leaving one TS citation, now corrected.
- Every ledger obligation is PASS above.
- Non-goals stayed excluded: no PR-head enforcement, no new GitHub client or loader, no Repository Card.
- Remaining limit: GHES appliance compatibility has not been claimed. That needs a live appliance run, which the task requires to be invoked explicitly.
