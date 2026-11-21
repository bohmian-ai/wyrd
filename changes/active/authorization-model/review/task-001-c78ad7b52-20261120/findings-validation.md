# TASK-001 independent validation

## Subject and method

Base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
Candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
Approved contract: SPEC-authorization-model revision 2, committed in `38ac3ca8a0c365825124aa3e99a36ba19888e26f`.
Original task: `changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md`.
Checked-out source at `3139c08cb0b7c0194583a66af3ab31bef28dc14f` matches the implementation candidate; later authority/design/spec changes are outside the implementation diff.

The four discovery reports were read and each proposed blocker was traced independently through its producer, authorization consumer, relevant caller and assertions. The implementation diff was read directly; source navigation began with CodeGraph, with focused reads for bodies its responses omitted. Governing requirements were read independently from the approved specification and owning security/Bifrost/runtime-identity authority, alongside applicable repository, SQL, SDK, testing and maintainer rules. This report validates findings; the reconciled acceptance matrix belongs in verdict.md.

No source edit, implementation, Git mutation or test execution was performed by this validation assignment. Review execution results and environment limits are recorded in verification.md. Existing task execution records remain evidence bound to their named commits, rather than reruns claimed here. Current instructions exclude TDD reporting.

## Retained findings

### FIND-TASK-001-1 — Service provisioning bypasses binding administration

Disposition: **CONFIRMED** from BEH-001. Classification: **VIOLATION**.

Requirement: REQ-008; architecture/wyrd-security-posture.md, Access administration. Tenant admins administer scoped assignments; a lesser credential-management capability cannot bypass that boundary.

Location: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:301`, `:310`, `:343`; shared validation at `crates/wyrd/wyrd-auth/src/roles.rs:26`.

Starting state/trigger: a caller with only `service_accounts:write` sends `POST /v1/principals` with default/unrestricted workload restriction and an `admin`/All assignment. The same supported caller is refused explicit binding creation.

Expected: refuse unauthorized assignment administration before committing an account, binding or credential. Actual: wildcard authorization is conditional only on a family restriction; ordinary provisioning calls the lesser service-account check, validates role/scope data without checking the assigner's authority, inserts the requested binding and returns a first key. The common role compiler accepts admin at All. The key then authenticates an account with that current authority, without intersecting it with the creator's permissions.

Evidence: `pg_principal_roles::binding_writes_require_tenant_administration` at test file `:358` establishes the credential-admin caller and explicit-binding refusal; `stable_account_restrictions_and_credentials` at `:743` explicitly allows unrestricted provisioning with supplied assignments under the lesser permission. The route and issuer provide the remaining escalation path. This is source-established; no live exploit execution is claimed.

Consequence: tenant privilege escalation through a supported public endpoint.

Smallest correction/closure: reuse the tenant assignment-administration decision in Service provisioning before applying assignments. Preserve any supported, separately authorized empty-assignment credential-only flow. A focused real-server journey must refuse stronger custom/admin assignment requests with no account, binding or key committed, while preserving authorized provisioning. Do not alter the closed grant model or weaken explicit binding checks.

### FIND-TASK-001-2 — Authority cache loses a newer observed revision

Disposition: **CONFIRMED** from INV-001, consolidating its two manifestations. Classification: **INCORRECT**.

Requirement: REQ-004; architecture/wyrd-security-posture.md, Current authority and freshness. Observing newer authorization state must prevent use of obsolete grants until refresh succeeds.

Location: `crates/wyrd/wyrd-auth/src/authority.rs:543` installation, `:568` notification and `:614` changed-revision confirmation; affected acquisition at `:491` and stream revalidation at `:698`.

Starting state/trigger: revision N grants access; a load reads N, an administrator removes access at N+1, the process observes N+1, then the older load installs. Expected: N remains unusable until a load reaches the observed revision. Actual: notification stores only `known_newer = true`; installation compares only the installed snapshot revision, accepts equal N, resets invalidation to false and renews freshness. `install(N)`, `notify(N+1)`, `install(N)`, `cached(tenant)` therefore returns known-obsolete grants. The await between load and installation permits this real concurrency sequence.

The same cause appears when confirmation reads a newer revision: its changed branch awaits a full load without first invalidating N. Other requests can still use N, and a load failure merely logs the error. A successful newer-revision read already prohibits this fallback.

Evidence: complete installation/notification/confirmation bodies and their acquire/reload consumers. Existing unit tests and the real replica propagation journey exercise sequential freshness/normal propagation, without either interleaving. Three relevant existing unit tests passed during review; they do not falsify these paths.

Consequence: a caller or open stream can continue reading after this process knows access was withdrawn; a subsequent outage can prolong the stale interval.

Smallest correction/closure: preserve observed revision knowledge in the existing cache across every load/install, including confirmation observations. Return only a snapshot that satisfies that knowledge, or refuse unavailable. Add deterministic notification-versus-load and failed changed-confirmation regressions, plus a focused real-server access-removal observation. No second resolver or warm SQL is needed.

### FIND-TASK-001-3 — Card mutation consumers reject valid scoped authority

Disposition: **CONFIRMED** from DOMAIN-01. Classification: **MISSING**.

Requirement: REQ-001 scoped operations; REQ-008 editor authoring within applicable scopes; security posture Permission model. Original task owns Card and storage consumers.

Location: `crates/wyrd/wyrd-server/src/components/cards/routes.rs:824`, with declaration/completion/delete callers at `:513`, `:581`, `:643`, `:705`; Card-owned upload check at `crates/wyrd/wyrd-server/src/components/storage/routes.rs:706`.

Starting state/trigger: a principal holds an editor or custom applicable Card-write binding for one Model family, or an exact Card grant for supported completion/deletion, and invokes the corresponding mutation. Expected: check the operation against each affected Card's family/exact identity; permit covered targets and refuse uncovered targets without partial mutation. Actual: all listed consumers require `Permission::card_write()` at All before reaching the owner. A family/exact grant cannot cover All; the audit label containing a target UID does not change that required scope. Delete consumers also check write rather than the catalog's delete action.

Evidence: `wyrd-spec/src/auth/permission.rs:400` explicitly permits scoped Card write/delete, and `auth/access.rs:309` compiles those scopes. Runtime containment requires scope coverage. The listed route helper always requires All. The new editor authoring journey binds editor at tenant scope, so its pass does not test this case.

Consequence: users must receive tenant-wide authoring authority to perform a covered operation on one family, defeating the approved delegation boundary.

Smallest correction/closure: consume target-specific authority in the existing Card/storage authorization owners, using the applicable operation and the one checker. Declaration checks must cover every affected graph member; exact completion/upload/deletion must resolve their trusted owner. Add focused real-server allowed and other-family/graph-member refusal journeys with no unauthorized persistence. Preserve existing graph/idempotency and file-validation behavior; do not broaden grants.

### FIND-TASK-001-4 — Soft deletion removes access to retained direct executions

Disposition: **CONFIRMED** from DOMAIN-02. Classification: **INCORRECT**.

Requirement: REQ-006 and REQ-010; security posture Resource and retained-data visibility. Trusted parents and family history remain usable after Card deletion without changing public Card definition visibility.

Location: `crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:534` direct fallback and `:694` direct identity resolution; imported `crates/wyrd/wyrd-sql/src/queries/cards/get.rs:12` lookup.

Starting state/trigger: direct verification has published inputs/results, with no queue or accepted-state row, and a family reader can inspect it. Soft-delete its subject Card or Verifier, then reopen the retained execution. Expected: retained reads preserve the exact trusted identity, while an unrelated reader remains not-found. Actual: direct fallback unconditionally fetches both Cards using `get_card_by_uid`; its SQL excludes deleted rows and maps either missing Card to execution-not-found before parent authorization. The reader's grant and retained rows cannot prevent that refusal.

Evidence: inspection imports the live-Card function directly; its own rustdoc incorrectly claims deleted Cards are included. The SQL predicate is `status != 'deleted'`. The direct-execution journey establishes the no-queue starting state; PR queue-cleanup coverage does not cover direct fallback.

Consequence: dedicated history reads disagree with retained SQL visibility after supported deletion, including for otherwise-authorized administrators/family readers.

Smallest correction/closure: obtain retained direct identity from the existing trusted retained/tombstone owner or retained record fields, preserving original workload, subject, Verifier and actor attribution. Do not make public Card reads return deleted definitions or infer identity from labels. A focused direct-verify journey must retain status/details after subject and separately Verifier deletion, with family/exact readers allowed and unrelated readers indistinguishable from unknown execution.

### FIND-TASK-001-5 — Binding and invitation validation accepts invalid exact targets

Disposition: **REVISED** from BEH-002: retain existing exact/object reference validation only; future family-name grants remain supported. Classification: **MISSING**.

Requirement: REQ-001 validates tenant references and exact UID identity; REQ-008 revalidates invitation scopes/tenant at consumption; security posture Permission model requires validated exact UIDs and rejection of unknown/cross-tenant assignments.

Location: `crates/wyrd/wyrd-auth/src/roles.rs:42`; callers include `components/principals/access.rs:334`, User/Service provisioning, invitation issue and `wyrd-auth/src/passwords.rs:488`. `wyrd-sql/src/queries/auth/role_bindings.rs:21` persists the supplied scope JSON.

Starting state/trigger: an administrator binds or invites a viewer with an exact Card scope whose UID is unknown, foreign-tenant or inconsistent with the supplied kind/name. Expected: refuse the invalid reference, and invalid invitation consumption creates neither User nor binding. Actual: shared validation checks only typed syntax, role lookup and operation/scope compatibility. Pure `CardPermissionScope::validate` rejects External but cannot establish existence, tenancy or family congruence; no later caller resolves the target before insertion/consumption. The same pure-path omission affects existing-object scopes such as a Bifrost table UID or GitHub connection ID.

Evidence: complete shared validator and binding consumer, scope validation bodies and JSON insert. Tenant-qualified FKs establish subject/role validity, not references encoded within scope JSON. Scoped invitation assertions cover missing/inapplicable scopes, deleted roles and lost issuer authority, omitting invalid exact identities.

Consequence: the server commits invalid authority and can enroll a User from invalid intent. This is not a claim of cross-tenant disclosure: request tenancy and matching still constrain normal reads.

Smallest correction/closure: extend the common server assignment-validation owner through existing tenant-bound target owners; validate exact UID/family congruence and existing-object references, reusing that check at binding/account/invitation creation and consumption. Preserve future Card-family versions and provable retained targets, including tombstones. Add focused unknown, foreign and mismatched exact binding/invitation refusals proving atomicity, alongside valid/family/retained cases. No new migration or policy service is justified.

### FIND-TASK-001-6 — Refreshed human tokens repeatedly reload warm authority

Disposition: **CONFIRMED** from INV-002, with the outage claim limited to the forced reload branch. Classification: **VIOLATION**.

Requirement: REQ-004 forbids warm-request permission SQL and grant compilation; REQ-002 preserves local CLI refresh. A usable confirmed bearer snapshot may serve within the defined confirmation-failure bound.

Location: `crates/wyrd/wyrd-auth/src/authority.rs:402` `predates`, `:529` authorization, `:136` compilation; credential producer `wyrd-auth/src/issuance.rs:224`; snapshot credential source `wyrd-sql/src/queries/auth/authority.rs:44`.

Starting state/trigger: a CLI human refresh produces a bearer carrying the consumed refresh row's UUID as credential provenance. Two requests arrive in that token's issuance second, with an already-loaded current snapshot. Expected: after necessary acquisition, ordinary requests reuse memory. Actual: the snapshot contains API keys only, so the refresh UUID remains unknown. `iat.timestamp() >= loaded_at.timestamp()` remains true after any successful load in the same whole second; each request reloads and recompiles the entire tenant. If the database is then unavailable in that interval, the forced reload fails despite a confirmed usable cached snapshot.

Evidence: `TenantGrant::Refresh` returns consumed UUID as credential ID; the authority SQL loads only API keys; `predates` and authorization implement the repeated branch. The existing unknown-credential test uses a token five seconds older than the load and does not cover equality after acquisition. The replica compile-counter test uses a stored Service API key.

Consequence: normal refreshed-human requests add permission-store waits/full tenant compilation to otherwise warm verification, gateway or query ingress. No extreme traffic assumption is needed.

Smallest correction/closure: make successful credential-miss acquisition establish the already-confirmed token/state without repeating work for refresh provenance; preserve mandatory ceilings and genuine newly inserted key/account miss handling. Add deterministic same-second unit coverage and a focused local refresh journey observing no repeated compilation/permission read after acquisition, with the confirmed snapshot usable during subsequent confirmation failure. Retain the five-minute refusal boundary.

### FIND-TASK-001-7 — Scoped access administration has no MCP projection

Disposition: **CONFIRMED** from REPO-01. Classification: **MISSING**.

Requirement: REQ-008 common administration contracts through HTTP/CLI and thin first-class SDK/MCP; original task assigns administration/access-summary projections to TASK-001.

Location: `crates/wyrd/wyrd-server/src/mcp/principals.rs:41`, `:57`, `:69`, `:78`; catalog/dispatch in `mcp/mod.rs:179`, `:227`, `:388`.

Starting state/trigger: a tenant administrator uses MCP to inspect Alice's access, create/remove a scoped binding or administer the corresponding role/group. Expected: typed thin tools reach the existing Rust administration owner with the same authorization and audit. Actual: complete descriptors/dispatch expose only credential metadata, self profile, credential revocation and workload-restriction replacement; none reaches access summaries, roles, bindings or groups.

Evidence: full tool lists, registration and dispatch. The new scoped MCP read journey provisions its caller through Rust Principals and calls MCP Card reads; it does not prove administration projection.

Consequence: the approved administration workflow is unavailable on a required agent surface.

Smallest correction/closure: extend the existing adapter with non-secret access-summary/role/binding/group projections and shared typed contracts, reusing Rust validation/mutation/audit. Preserve read discovery, explicit write authority and existing plaintext credential-issuance exclusion. A focused real MCP journey must administer a family grant, inspect its source/scope, remove it, exercise roles/groups and refuse a directly named non-admin write without mutation. No second administration implementation is needed.

### FIND-TASK-001-8 — Served instructions call removed APIs and describe obsolete access

Disposition: **CONFIRMED** from REPO-02, including the same stale requirement in machine-facing MCP descriptions. Classification: **REGRESSION**.

Requirement: REQ-010/task affected served documentation and consumers; REQ-004 current propagation; REQ-006 trusted parent; REQ-008 approved role bundles/scoped administration.

Locations: `docs/src/content/docs/operate/grant-and-inspect-permissions.md:24`, `:45`, `:76`; `operate/add-people.md:143`; `understand/identity-and-authorization.md:43`; `build-and-verify/verify/approve-a-pull-request.md:15`; `reference/mcp-tools.md:42`; matching MCP execution-read descriptions in `crates/wyrd/wyrd-server/src/mcp/verification.rs:166` and sibling descriptors.

Starting state/trigger: an administrator follows the shipped grant guide or a reader selects authority from these descriptions. Expected: usable explicit-scope calls and accurate current grants/role/parent behavior. Actual: copied commands use removed `principal role` and SDK `grant_role`/`grantRole` methods and old response fields. Guidance describes changes only after token renewal, viewer audit access, editor approval and execution reads through the referenced Verifier, all superseded by this change.

Evidence: guide contents compared with CLI `principal/mod.rs`, the current Rust Principals methods and their SDK projections, approved role bundles and retained-parent consumers. MCP read descriptor strings independently still name the superseded Verifier permission. These are executable workflow/authority claims, not writing preferences.

Consequence: documented grant commands fail and users/agents request the wrong authority for retained reads, audit or approvals.

Smallest correction/closure: repair the affected existing sections/descriptors to the tested access/bind/unbind contracts, explicit scopes, current bundles, healthy five-second propagation and trusted-parent reads. Reuse tested copyable examples; add no aliases or new workflows. Validate content against source and actual relevant CLI/journey evidence and run applicable docs checks. Production authorization changes are unnecessary for this finding.

### FIND-TASK-001-9 — New and changed Rust items omit mandatory rustdoc

Disposition: **CONFIRMED** from REPO-03. Classification: **VIOLATION**.

Authority: root AGENTS.md explicitly requires rustdoc on every new/changed Rust item and states that missing rustdoc blocks merge.

Location: `crates/wyrd/wyrd-auth/src/roles.rs:95` public RoleAdminError; `crates/wyrd/wyrd-sql/src/queries/auth/authority.rs:14` AUTHORITY_STATE_SQL; new SQL constants at `auth/role_bindings.rs:21` through `:104` and fallible `decode_source` at `:407`.

Starting state/trigger: documenting or maintaining the changed authorization owners. Expected: concise item intent/workflow/invariants and error behavior for fallible items. Actual: the diff removes RoleAdminError's former type documentation, while the new declarations have none. Module-level prose does not meet the explicit item rule.

Evidence: exact source declarations and base-to-candidate removal of `Role administration failure.`. This is an explicit acceptance rule; no subjective readability threshold or runtime failure is claimed.

Consequence: mandatory repository acceptance remains unsatisfied.

Smallest correction/closure: restore concise type intent and document the listed new items, including decode_source's invalid-source error. Check changed items in these owners for the same bounded omission; leave unrelated comments alone. Source review and affected format/lint checks suffice; no behavioral test or new checker is required.

### FIND-TASK-001-10 — Live peer enforcement lacks scoped-parent journey proof

Disposition: **REVISED** after separate focused validation in peer-proof-validation.md. Classification: **EVIDENCE_GAP**. The proposed missing published-source portion is rejected; only live-peer returned-data proof is retained.

Requirement: REQ-006 names source-independent managed-verification visibility through peers; TASK-001 acceptance scenario 1 and AC-001 require peer execution without another family's rows/counts. TESTING.md requires the actual public returned-data dependency boundary.

Location of inadequate proof: `crates/vala/vala-bifrost-redux/src/oracle/codec.rs:754`, compared with actual public peer fixtures in `crates/wyrd/wyrd-testing/tests/bifrost/oracle/support.rs:242` and `peer_cluster.rs:1151` and the distributed/published journey consumers.

Starting state/trigger: one tenant has retained managed-verification records for two parents; a family/exact reader queries through real distributed Oracle. Expected proof: a public client query actually uses a peer and returns only its authorized parent's rows/counts, including when the query mentions the hidden parent. Available proof: the masked-plan test round-trips a physical plan over a captured MemTable leaf in one process. Live cluster readers use admin authority or ordinary gateway/table scopes, without the same-tenant managed-parent assertions. Those tests establish codec/source/transport seams, but not this required composed boundary.

Evidence: the separate source-backed focused check inspected the complete Oracle journey tree, credential producers and task execution references. The Rust scoped SQL journey at `sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:877` does establish published Parquet reads: its flush path publishes residue and retires Arrow copies. Therefore no redundant published-only proof is requested. No implementation failure or live exploit is inferred from missing peer proof.

Consequence: a required security boundary remains unproven at the real public peer consumer, despite passing local and codec tests.

Smallest correction/closure: identify existing credible execution evidence or add one focused public Bifrost live-peer journey using the current cluster and trusted managed-record producers. Establish real peer participation, assert allowed identities/counts and hidden-parent exclusion for the restricted reader. Carry forward existing local published and codec proof. Run its confirmed exact selector with the existing Oracle journey/Postgres setup; no production rewrite or full suite is required solely for this evidence gap.

## Coverage reconciliation and limits

All nine original discovery proposals are retained with the revisions above. The additional peer-proof gap received a fresh focused independent check before retention; its published-source portion was rejected. None creates additional product authority. The primary behavior scope plus security/retained-data, cache/lifecycle and repository/projection specialists covers the materially affected boundaries. Separate system/maintainer reviewers are unnecessary: production failure/recovery/waits were assigned to the lifecycle boundary, and the primary plus repository boundary covered ownership, simplicity and maintainability. There is no missing selected discovery report.

For the single verdict matrix: REQ-001 fails on target validation and scoped mutations; REQ-002's local device/anchor behavior has source/assertion-backed recorded evidence, with warm refreshed requests failing REQ-004 separately; REQ-003 has stable two-release/root/attribution proof; REQ-004 fails on current-state fencing and repeated warm reload; REQ-005's claim/ceiling/delegation/accepted-authority paths have inspected assertions and recorded proof, with cache-related enforcement qualifications carried by REQ-004; REQ-006 fails retained direct reads after deletion and has the independently validated live-peer proof gap; REQ-007 has source/assertion-backed configuration isolation and retained-read separation; REQ-008 fails provisioning administration and MCP projection; REQ-010 fails affected consumer documentation. The explicit rustdoc rule fails independently. New Access UI and request-copy presentation remain TASK-002 scope.

The original task's broad totals are not treated as a proof of omitted scenarios. Existing local login/password-anchor, group/custom/exact reads, provider coexistence, runtime attribution, propagation, stream interruption, approval-parent and download assertions were inspected along the relevant consumer paths. Source proves the defects above without an additional production edit. Recorded passes cannot establish the missing negative/adversarial paths. Fresh live PostgreSQL confirmation was unavailable because setup was denied Docker socket access; the three review-run unit passes prove only their sequential cases.

Recommendation: **FIX_REQUIRED**. Settled approved behavior and the mandatory repository rule permit bounded correction; no specification decision is required by these retained findings. Fresh live dependency reruns are limited, but source-backed defects and available inadequate peer proof do not require a blanket BLOCKED verdict. No prior remediation closure or convergence assessment applies to this first review.

Skipped: implementation, broad suite reruns and excluded TDD assessment. Risk: provisioning escalation and known-stale authority permit unauthorized access until corrected; no live exploit reproduction is claimed.
