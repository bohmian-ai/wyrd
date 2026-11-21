# TASK-001 behavioral review

## Subject and scope

Base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
Candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
Implementation range: the specified 38 commits, inspected with `git diff eeb3bbd4f891ae88b00ce1979810eb2a11d04871...c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
Approved contract: `changes/active/authorization-model/spec.md`, revision 2, recorded in `38ac3ca8a0c365825124aa3e99a36ba19888e26f`.
Original task: `changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md`.
Working source is identical to the candidate for implementation paths; later authority/design/spec commits are governing context, not implementation under review.

This report covers the assigned backend requirements, connected scenarios, actual assertions, and account/device/invitation administration paths. Cache concurrency, managed-row/credential security, and broad cross-tier/persistence coverage have separate assigned boundary reviews. Their results require reconciliation in validation; this report does not assume their conclusions. TASK-002 owns new Access UI and request-access presentation. TDD history is excluded by the current review instruction.

Authority consulted: root `AGENTS.md`; `changes/spec-driven-development.md`, including authority and ambiguity rules; `agent_docs/maintainer-style.md`; `TESTING.md`; `architecture/wyrd-security-posture.md` Authorization and policy, OAuth authorization server, and runtime principal rules; `architecture/wyrd-design.md` Runtime identity and Repository; applicable SQL rules. Source navigation began with CodeGraph. Its queries did not locate several newly added owners accurately, so direct source reads covered those owners.

## Navigation and assertion map

The shared contract is `wyrd-spec/src/auth/access.rs`: operation-only roles and `compile_binding` apply explicit scopes and derive execution-read inheritance from Card read. `wyrd-auth/src/roles.rs` validates and binds assignments for all account, invitation, and explicit-binding callers. The ordinary explicit-binding path is `wyrd-server/src/components/principals/access.rs`; Service creation is the separate caller in `principals/routes.rs`. Local CLI approval connects server `auth/cli_login.rs` to `CliLogins::approve_signed_in`, device redemption, and `TenantTokenIssuer::issue_human_session`. Password signup uses invitation consumption and the shared assignment owner in one tenant transaction.

User-visible proof inspected includes:

- Rust `principal_roles.rs:107` grants/unbinds editor and observes the next authoring request; `:177` consumes a real query stream, removes authority, waits for refusal, and asserts early stream failure with no success terminal.
- Rust `principal_roles.rs:277` creates/invites local Users and rejects invalid account inputs and non-admin callers; `:355` preserves direct/provider provenance; `:764` performs local password/device saved login, checks family list/read/not-found, changes the password, and asserts refresh refusal.
- Rust `principal_roles.rs:877` creates two Services sharing a Verifier, queries retained results/inputs, joins and counts as a family-scoped local User, and asserts the unrelated table refusal; `:1023` exercises group membership removal, a custom Card-reader role and exact-version exclusion after publishing a second version.
- Rust `principal_roles.rs:568` uses one runtime account/key across two releases and token renewal, retains each run/subject, rejects duplicate live family ownership and a foreign workload root, then revokes the account. Python `test_principal_roles.py:238` additionally checks retained subject and actor IDs; `:351` checks declaring releases grants no account, dependency authoring is refused, and malformed/unknown roots are refused.
- Server `pg_browser_sessions.rs:1329` records scoped invitation assignments, checks missing/inapplicable scope refusals, deletes the referenced role, removes issuer authority, and checks failed consumption leaves no User. This does not test target-scope existence or UID/family congruence.
- Server `pg_principal_roles.rs:358` proves the explicit binding endpoint denies a supported `service_accounts:write`-only credential administrator. `:743` independently proves that same permission alone reaches unrestricted Service creation with assignments. These two assertions expose an untested boundary between the paths.
- Server `pg_github_connection.rs:792` distinguishes GitHub-only configuration authority from OIDC administration, Card reads and Repository-only grants. The task also records focused PR membership, parent-scoped approval, execution/download, raw SQL and peer-plan results, assigned to the retained-data boundary review for full source validation.

## Primary coverage matrix

`PASS` below means the listed behavior has inspected source/assertions and recorded focused execution evidence; it does not override a separate boundary finding. `UNRESOLVED` identifies work assigned to another required report, not a claim that implementation is missing.

| Requirement / scenarios | Implementation and inspected proof | Result |
| --- | --- | --- |
| REQ-001 / AC-001, AC-002, AC-003 | `auth/access.rs::compile_binding`, runtime `EffectivePermissions`, shared `roles.rs`, SDK group/custom/exact test at `principal_roles.rs:1023`, direct/provider test at `:355`; explicit account/group/role references are tenant-bound. Resource-reference validation is absent in the common scope validator; BEH-002. | FAIL |
| REQ-002 / AC-001 | `CliLogins::authorize` no longer requires an IdP; signed-in approval takes one `HumanLoginAnchor`; `issue_human_access` locks the refresh family and original password/provider anchor. Rust `principal_roles.rs:764` drives local device login and password-change renewal refusal. Device one-use/poll/client/origin logic remains in the existing owners; recorded identity regression lane covers provider paths. | PASS |
| REQ-003 / AC-002 | Runtime family remains separate from operation scope. Rust `principal_roles.rs:568` and Python `test_principal_roles.py:238,:351` assert stable identity, two releases, attribution, no declaration-created account, foreign/malformed root refusal and ungranted authoring. Exact signed-root mismatch proof is recorded in `pg_principal_roles::workload_roots_and_families_are_services_only`. | PASS |
| REQ-004 / AC-003 | Real replica test and compile-counter assertions are recorded; Rust `principal_roles.rs:177` asserts public stream interruption. Cache acquisition, freshness, invalidation race and production worker coverage require the assigned invariant report. | UNRESOLVED |
| REQ-005 / AC-002, AC-003 | `EffectivePermissions` layers current grants and restrictions; actual identity-only issue/verify tests, delegation intersections, personal-key ceiling and recovered accepted-authority tests are named in the task. Full current credential/actor and accepted-work boundary tracing requires the assigned security/invariant reports. | UNRESOLVED |
| REQ-006 / AC-001 | Card-to-execution inheritance is role-name-independent in `compile_binding`; family-scoped Rust SQL journey checks rows, joins and counts. Trusted-parent capture, all retained-table producers, pre-expression masking, sensitive data, peer and pagination coverage require the assigned retained-data report. | UNRESOLVED |
| REQ-007 / AC-001 | GitHub configuration now requests `github_connections` authority and has explicit negative HTTP assertions in `pg_github_connection.rs:792`; PR retained reads are independent of that permission. Existing provider identity/admission and retained disconnect coverage remain assigned to the retained-data/security report. | UNRESOLVED |
| REQ-008 backend / AC-001, AC-002, AC-003 | Explicit access writes require wildcard; local User creation requires User management plus wildcard; signup rechecks live issuer authority and role/scope applicability transactionally. However unrestricted Service creation binds arbitrary requested roles under only `service_accounts:write`; BEH-001. Invitation target-scope validation shares BEH-002. | FAIL |
| REQ-010 / all three | Existing account/role/credential owners are extended, superseded assignment store/callers replaced, and no token-snapshot compatibility reader appears in the inspected flow. Cross-tier exports, migration authority, generated projections and changed-owner rule coverage require the assigned repository report. | UNRESOLVED |

## Proposed findings

### BEH-001 — Service creation bypasses scoped-assignment administration

Classification: `VIOLATION`.

Requirement: REQ-008 requires tenant-admin scoped-assignment administration and prevents a non-admin direct-request bypass. `architecture/wyrd-security-posture.md:740` assigns tenant access administration to tenant admins. The explicit binding owner enforces wildcard before writing. `principals/routes.rs` itself states that a credential administrator cannot raise authority through assignment writes.

Location: `crates/wyrd/wyrd-server/src/components/principals/routes.rs:301-348`, especially the family-only wildcard check and unconditional requested-assignment binding; `wyrd-auth/src/roles.rs:26-56` performs data validation, not caller authorization.

Starting state and trigger: a supported custom-role caller holds only `service_accounts:write` and lacks wildcard. Submit `POST /v1/principals` with an unrestricted/default workload restriction and `assignments: [{"role":"admin","scope":"all"}]`.

Expected: the scoped-assignment administration check refuses before creating the requested grant/account/credential. A credential-management permission cannot create a new administrator by choosing a role in account creation.

Actual: wildcard is checked only when a family restriction is present. Otherwise the route checks only `service_accounts:write`, validates that the admin role and All scope are a valid combination, inserts the Service, binds admin, and returns its first key. Exchanging that key yields a live account whose current cached grants contain wildcard. No downstream issuer path intersects those grants with the creating caller's authority. The same caller is denied `POST /v1/access/bindings` yet can reach its effect through Service provisioning.

Evidence: `pg_principal_roles.rs:358-425` expressly establishes the credential-admin starting state and direct-binding refusal; `:743-813` establishes that only `service_accounts:write` admits unrestricted account creation with supplied assignments. `routes.rs:343-352` then persists the requested assignments and key in one commit. `compile_binding` accepts admin at All. These are source-backed control paths; no exploit execution is claimed.

Consequence: tenant privilege escalation through a public administration endpoint.

Smallest correction: enforce the existing tenant assignment-administration decision in the Service provisioning owner before applying requested scoped assignments, preserving the existing separately authorized empty-assignment account/credential flow if it remains supported. Route all first-class consumers through that owner. Add a public server/SDK negative journey using the credential-admin starting state, arbitrary stronger/custom/admin assignments, and assertions that no grant/account/key is committed; retain the admin success and intended credential-management cases. Do not weaken the explicit binding endpoint or change the grant model.

### BEH-002 — Assignment validation does not resolve exact target references

Classification: `MISSING`.

Requirement: REQ-001 requires server validation of operation/scope applicability and tenant references, including exact versions following their validated UID. REQ-008 requires referenced scopes and tenant to be revalidated when an invitation is consumed. `architecture/wyrd-security-posture.md:650-665` requires validated exact UIDs and rejection of unknown/cross-tenant assignments. `CardPermissionScope` documents that its exact family's identity comes from trusted server state.

Location: `crates/wyrd/wyrd-auth/src/roles.rs:42-54`; consumers are explicit binding creation (`principals/access.rs:331-351`), User/Service creation, invitation issue and password signup (`passwords.rs:488`). SQL `role_bindings.rs:21-26` persists the supplied scope JSON; its foreign keys cover subject and role, not JSON object references.

Starting state and trigger: a legitimate tenant admin submits a viewer binding or scoped invitation with an exact Card scope whose UID belongs to another tenant, is unknown, or names a different kind/family than the supplied values. The same omission applies to existing-object Bifrost table and GitHub connection IDs. The example deliberately excludes family-name grants for future versions.

Expected: invalid tenant/object references are refused; failed invitation consumption creates neither the User nor a binding. A valid exact target's family is established from authoritative tenant-owned or retained identity rather than accepted from the caller.

Actual: `scope.validate()` has no tenant capability and only validates identity syntax/registrable kind. The remaining IO is lookup of the role by name. `compile_binding` checks operation/scope compatibility but cannot establish resource existence or UID/family congruence. The role ID is returned and the supplied JSON is stored without resolving its target; password signup repeats this same insufficient validation and commits the User. No later step in these administration flows supplies the missing check.

Evidence: full validator body at `roles.rs:26-56`, `bind_assignments` at `:67-88`, `CardPermissionScope::validate` at `permission_scope.rs:153-164`, and the binding insert. The scoped-invitation test covers missing/inapplicable scopes and deleted roles but not invalid exact target references. No cross-tenant data disclosure is claimed: normal reads still derive tenancy from the authenticated context and compare scopes, so the observed defect is accepting invalid authority and enrollment rather than a demonstrated RLS escape.

Consequence: invalid assignments and invitations succeed despite the required trust-boundary validation; administrators can be shown a durable exact grant that refers to no valid target of that tenant, and invitation acceptance commits an account from such invalid intent.

Smallest correction: extend the common server assignment-validation owner to validate existing exact/object references using their existing tenant-scoped owners and establish exact Card family congruence. Reuse it at issue/bind/account creation and consumption. Preserve future-version family semantics and retained-history grants; do not require a live Card for an otherwise provable retained target. Add focused public binding and invitation journeys for unknown, foreign-tenant, and mismatched exact identities, proving invalid consumption commits neither User nor binding and valid/retained/family cases remain usable. No schema or new policy mechanism is needed.

## Execution evidence and limits

The task's recorded focused executions were checked against actual assertion bodies and their consumer paths above. The later commits after `a28f2889d` add the Repository-parent download test and task evidence; they do not alter the reviewed production mechanisms, so the earlier relevant execution evidence is not automatically discarded. The task records the new download test passing separately.

A focused rerun was requested for `pg_browser_sessions::scoped_invitations_enroll_exactly_their_assignments` and `pg_principal_roles::binding_writes_require_tenant_administration`. Repository-managed setup failed before migrations or test execution because access to `/home/thorrester/.config/colima/default/docker.sock` was denied. No new PostgreSQL execution or exploit reproduction is claimed. This environment limitation does not itself establish a defect or invalidate the independently inspected recorded execution results. Functional acceptance remains blocked by the proposed source-backed findings pending independent validation.

Skipped: broad suite reruns, implementation changes, and excluded TDD/history-comparison assessment. Risk: the Service provisioning path admits a concrete privilege-escalation flow until corrected and independently validated.
