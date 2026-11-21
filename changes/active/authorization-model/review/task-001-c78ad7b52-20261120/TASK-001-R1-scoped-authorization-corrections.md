---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-004, REQ-006, REQ-008, REQ-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10]
---

# Close scoped authorization gaps

An administrator can delegate only authorized assignments; a scoped caller can perform the covered Card operation and retain access to its execution history. Requests and streams must use authority at least as current as the process has observed, with repeated warm human requests staying in memory. Agents receive the promised administration projection and accurate instructions. One focused live-peer journey supplies the missing composed-boundary proof.

## Contract and review context

Approved contract: changes/active/authorization-model/spec.md, SPEC-authorization-model revision 2 at 38ac3ca8a0c365825124aa3e99a36ba19888e26f.
Original task: changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md.
Reviewed base: eeb3bbd4f891ae88b00ce1979810eb2a11d04871.
Reviewed candidate: c78ad7b5241fbbfb54e85e2390c6a694787debfc.
Implementation range: 8e33913ed^..c78ad7b52, 38 commits.
Review-time HEAD: 3139c08cb0b7c0194583a66af3ab31bef28dc14f, identical implementation; later authority/design/specification commits excluded from the implementation review.

The settled findings, exact source locations, evidence and dispositions are in findings-validation.md beside verdict.md. This task corrects those existing obligations; it adds no product behavior. Applicable AGENTS.md, owning architecture, TESTING.md and repository implementation workflow remain governing.

## Correction boundaries

### Administration and target validation

FIND-TASK-001-1, REQ-008: a caller with only service_accounts:write can submit an unrestricted Service with admin/All assignments, receive its key and acquire tenant administration. The family-only wildcard check in server components/principals/routes.rs:301 does not authorize ordinary assignment creation; shared roles.rs validates data rather than the assigning caller. Existing pg_principal_roles assertions establish both the lesser caller and its explicit-binding refusal.

Enforce the existing tenant assignment-administration decision in the Service provisioning owner before applying requested assignments. Refusal commits no account, binding or credential. Preserve supported, separately authorized empty-assignment credential management and authorized admin provisioning. HTTP, CLI, SDK and eventual MCP callers consume this same server decision; no client-side substitute is sufficient.

FIND-TASK-001-5, REQ-001/008: shared wyrd-auth roles.rs:42 accepts exact/object scope JSON after pure syntax/applicability checks. Unknown/foreign exact UIDs, mismatched Card family identity and invalid existing-object references can become bindings or consumed invitation intent. SQL foreign keys cover role/subject, not target JSON.

Extend the existing assignment-validation owner using the existing tenant-bound target owners, and reuse it for binding/account/invitation creation and invitation consumption. Establish exact Card UID/family congruence and validate existing-object references. Invalid invitation consumption creates neither User nor binding. Preserve family-name coverage of future versions and provable retained/tombstoned targets; a missing live Card is not enough to reject valid retained identity. This requires neither another grant store nor new schema machinery.

### Authority observation and warm acquisition

FIND-TASK-001-2, REQ-004: AuthorityCache::notify records only a boolean, and install can accept an older in-flight load at the installed revision and clear a newer observation. Confirmation also learns a changed revision without fencing the old snapshot before its awaited load; failure leaves known-obsolete grants usable. See authority.rs:543,:568,:614 and their request/stream consumers.

Preserve the highest observed authority state through the existing cache's confirmation, notification, load and install flow. An older result cannot make the entry usable; acquisition either reaches the observed state or refuses unavailable. Preserve the one sanctioned permission-state reload, short memory locks and background compilation. Requests and subsequent stream output obey the same observation boundary.

FIND-TASK-001-6, REQ-002/004: refreshed human tokens carry a consumed refresh UUID, while the snapshot's credential map contains API keys. authority.rs:402 compares whole-second iat with loaded_at using >=, so requests within the issuance second repeatedly reload/recompile even after successful acquisition. A subsequent database failure forces refusal despite the confirmed usable snapshot.

Correct credential-miss acquisition in the existing authority owner so the already-confirmed human token/state does not trigger repeated reload. Preserve genuine new-account/new-key miss handling, current key ceilings, provenance, delegation and the five-minute outage bound. Do not remove a required first acquisition or turn an unknown actual API key into an unbounded grant.

### Scoped mutations and retained identity

FIND-TASK-001-3, REQ-001/008: Card and Card-upload consumers require cards:write at All, although the contract compiles valid family/exact write/delete grants. Deletion also consumes write rather than the applicable delete operation. A family editor can read its Model but cannot author its covered release or complete/delete its covered files/versions.

Correct target authorization at the existing server Card/storage owners, using the one checker and trusted scopes. Cover each declaration graph member and each exact completion/upload/deletion owner before unauthorized persistence. Preserve graph atomicity/idempotency, file validation and unrelated-resource refusal. All thin clients consume these server decisions; no broader grant is a workaround.

FIND-TASK-001-4, REQ-006/010: verification/service/inspection.rs:694 reconstructs direct identity through the live get_card_by_uid lookup, whose SQL excludes deleted Cards. A direct execution with retained records and no queue/accepted state becomes not-found after its subject or Verifier is deleted.

Resolve direct retained identity through the existing trusted retained/tombstone mechanism, preserving the exact parent and original workload, subject, Verifier and actor. Public Card reads must continue hiding deleted definitions. Do not infer a parent from mutable labels, broaden SYSTEM access, or change attribution. Status/details and downstream retained readers remain consistent with SQL's retained-parent boundary.

### Required MCP projection and guidance

FIND-TASK-001-7, REQ-008/010: the principal MCP catalog/dispatch has no access-summary, scoped-binding, custom-role or local-group administration projection. The existing scoped-read journey provisions through Rust and therefore does not establish that MCP capability.

Extend the existing principal MCP adapter with thin typed projections of the existing non-secret server access contracts. Preserve read discovery, explicit write authority, shared validation/audit and existing exclusion of plaintext credential issuance. No separate administration lifecycle or permission checker is required.

FIND-TASK-001-8, REQ-004/006/008/010: existing Permissions/Add people guides call removed principal role and SDK grant_role/grantRole APIs and describe renewal-only authority updates. Role guidance incorrectly grants viewer audit and editor approval; retained-read docs and MCP descriptions still require the referenced Verifier rather than the trusted parent.

Repair affected existing sections/descriptions identified in findings-validation.md to current tested calls, explicit scopes, six agreed bundles, healthy five-second propagation and trusted-parent reads. Reuse journey-backed copyable examples. Remove obsolete guidance; add no compatibility aliases, redesigned guides or new workflows.

### Mandatory item documentation and peer proof

FIND-TASK-001-9, root AGENTS.md changed-item rustdoc rule: restore RoleAdminError intent and document the new authority SQL/binding constants and decode_source, including its error condition. Check changed items in these owners for the same bounded omission. Source review and relevant formatting/lints suffice; no runtime test or new documentation checker is needed solely for this correction.

FIND-TASK-001-10, REQ-006/AC-001: local scoped SQL and published Parquet assertions exist, but the peer test round-trips a MemTable physical plan in-process. Real peer journeys do not prove the same-tenant managed-parent visibility boundary for a family/exact caller.

Provide existing credible evidence or one focused public Bifrost journey using the current live-peer fixture and trusted managed-record producers. Establish actual peer participation and assert visible identities/counts plus hidden-parent exclusion. Carry forward the existing local Parquet and codec proof. This finding alone requires no production changes.

## Observable closure and verification

Confirm actual test names and targets after implementation and record exact selectors. The owners below are existing runners, not invented test names. Cargo-backed execution remains sequential. Use repository-managed Postgres setup and normal production observability for failures.

| Closure scenario | Findings / requirements | Narrowest credible proof and runner | Evidence |
| --- | --- | --- | --- |
| Credential administrator attempts stronger custom/admin assignments during unrestricted Service creation; refusal leaves no account/binding/key. Authorized provisioning and supported empty-assignment creation remain usable | 1 / REQ-008 | Focused server integration or public Rust SDK journey in existing pg_principal_roles/principal_roles owners; mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=<confirmed-name>)', or wyrd-ai integration with -P journey --run-ignored=all, under existing Postgres setup | PASS. `ab8d54b98`. `pg_principal_roles::provisioning_a_services_roles_requires_tenant_administration` (credential admin refused admin/custom assignments, no account/binding/key; admin and empty-assignment creation succeed); `stable_account_restrictions_and_credentials` updated. All 11 `pg_principal_roles` pass via `wyrd-server --test integration --run-ignored=all -E 'test(/^pg_principal_roles::/)'`. |
| Binding and invitation consumption refuse unknown, foreign and family-mismatched exact identities atomically; valid exact, future-family and provable retained targets remain usable | 5 / REQ-001,008 | Existing pg_principal_roles and pg_browser_sessions public server journeys; exact confirmed integration selectors; SQL seam tests only where needed for transactional refusal | PASS. `84af11d2e`, rustdoc `7a020b64b`. `roles.rs::target_exists` in `validate_assignments` (bindings, account creation, invitation issue and consumption). `pg_principal_roles::exact_card_grants_must_name_a_registered_version_of_their_family` (unknown, foreign, other-family UIDs refused; live, deleted and future-family accepted); `pg_browser_sessions::scoped_invitations_enroll_exactly_their_assignments` refuses an unknown exact version. New `wyrd-auth` → `vala-sql` edge passes `check:deps`. |
| Notification N+1 arrives during a load of N; N cannot install as usable. Changed-revision confirmation followed by load failure also refuses obsolete state | 2 / REQ-004 | Deterministic in-owner authority unit cases, exact wyrd-auth --lib selectors; one focused real-server access-removal observation in existing replica owner to connect the corrected state to public refusal | PASS. `cc40c8271`. `wyrd-auth --lib -E 'test(/authority::/)'` (9 pass) incl. `an_older_load_never_serves_after_a_newer_revision_is_observed` (N+1 during load of N; malformed notification). Real-server: `pg_principal_roles::access_changes_reach_another_replica_within_the_bound` PASS 1.9s. Waits: `reload_superseded` refusal adds no wait. |
| Local CLI refresh followed by repeated same-second reads causes no repeated compilation/permission reload after necessary acquisition; confirmed snapshot survives a subsequent confirmation outage within its bound | 6 / REQ-002,004 | Deterministic authority unit case plus focused local saved-login public journey in principal_roles; existing authority PG outage owner for real-dependency confirmation refusal. Keep timing assertions deterministic and preserve new-key restrictive-ceiling regression coverage | PASS. `cc40c8271`, `ed9e0a221`. Unit `authority::tests::a_settled_token_stops_reloading_in_its_issuance_second`; pg `authority::pg_tests::a_settled_token_needs_no_reload_during_an_outage`; journey `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=principal_roles::a_family_bound_viewer_reads_only_that_family)'` (renewed saved login, 20 reads, `authority.compiles()` unchanged). Waits: a settled id removes repeat reloads; the one sanctioned miss reload is unchanged. |
| Family editor declares the covered release, completes its Card files and deletes a covered version; exact/custom grants permit applicable mutations. Other-family and uncovered graph members refuse without unauthorized persistence | 3 / REQ-001,008 | Focused real SDK/server Card journey using existing Card lifecycle/verification fixtures, exact confirmed selectors. Exercise upload admission/completion through the real storage consumer | PASS. `04b892c2c`. `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=principal_roles::scoped_editors_change_only_the_cards_they_cover)'`: family editor declares a release (real upload + completion), deletes a later release by UID; Prompt declare/delete refused; mixed desk graph refused with no member persisted; exact-version editor deletes by reference, refused the Prompt. Regression: `wyrd-server -E 'test(/card\|upload\|storage\|declar\|repositor/)'` 105 pass; `declare_and_pull::` journeys pass. Waits: one registry statement for the target scope on write routes only (not a hot path). |
| Direct execution remains readable after subject deletion and separately Verifier deletion, with no queue state; family/exact readers retain visibility and unrelated readers remain unknown | 4 / REQ-006,010 | Focused pg_verification_routes or Rust SDK direct-verify journey, with actual retained SQL/status/details assertions and exact selector | PASS. `e84182888`. `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=durable_verifier_work::a_direct_execution_survives_deleting_its_cards)'`: after deleting the referencing Service, then the Agent subject and the Verifier, admin and family reader `wait` equals the original result and `details` read; an unrelated family reader gets `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`. |
| Real MCP client discovers typed access reads; administrator creates/inspects/removes family binding and uses role/group projections; directly named non-admin write refuses without mutation | 7 / REQ-008,010 | Existing wyrd-mcp --test mcp journey under test:bifrost:journey:mcp setup; exact confirmed selector rather than full suite. Check generated argument/response parity | PASS. `66802ff54`. `wyrd-mcp --test mcp -P journey --run-ignored=all` whole binary 52/52, incl. `principals::pg_tests::an_agent_administers_scoped_access` (put/list role, group create/member, family bind, access summary with group source, list bindings, unbind, teardown; unknown role `WYRD_SPEC_400_VALIDATION`; viewer not offered writes and refused on write and read). Catalog parity in `discovery` and `connectivity`; `codegen:check` PASS; `reference/mcp-tools.md` updated. |
| Copied administration calls and role/parent/propagation descriptions match actual consumers; changed Rust items have required intent/error docs | 8,9 / REQ-004,006,008,010 and AGENTS.md | Focused source/content review against relevant passing CLI/SDK journeys, applicable docs checks, Rust format/lints; no behavioral test solely for item prose | PASS. `6391076d9`, `7a020b64b`, `1f6c4b03e`. Content review against passing journeys; `docs:check` PASS; `mise run fmt`, `lints` PASS. Open notes: `add-people.md` 'no invite step' line; Python/TS `test_deferred_evidence` approver-only key; SDK snippets inline, not `CodeFromFile`; no CLI journey for `principal access`. |
| Public family/exact reader queries two same-tenant parents through actual peer participation; only covered identities/counts return, including a hidden-parent predicate | 10 / REQ-006, AC-001 | One live-peer public journey in wyrd-testing --test oracle -P journey --run-ignored=all with its exact confirmed selector and repository Postgres. Existing local publication/codec evidence carries forward | PASS. `284f80115`. `cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::scoped_readers_query_only_their_parents_records_across_pods)'` PASS 11.8s: four pods, real verifications for two parents, every query Analytical with a peer graph lease and a peer body poll; family and exact-version readers see only their parent's count; hidden-parent filter returns no row and count 0. Limit: exact reader bound to the other parent, not a second version of one family. |

Use the setup in mise test:principals:integration and test:bifrost:journey:{oracle,mcp} rather than ad-hoc databases. Python/TypeScript/CLI projections must remain correct; add focused surface tests where changed adapters need their own proof. New MCP or wire projections require applicable codegen checks; Python/TS contracts, if changed, require their owner lint/type/export checks. Complete affected Rust format/lints, dependency and tenant-boundary checks without weakening gates. Run documentation checks for affected served guidance. Do not replace focused closure with gate or full journey aggregates; integrated change review owns those broad suites.

Cache fixes touch verification, gateway and query ingress. Enumerate every added/moved PostgreSQL, Scribe, object-store and Vault wait and match it to sanctioned acquisition; warm authorization remains memory-only and audit staging nonblocking. Historical TDD repair is not required.

## Preserved behavior and non-goals

Keep tenant separation, scoped additive grants, explicit gateway semantics, credential and delegation ceilings, accepted-work freeze/recovery, principal-revocation cancellation, runtime family ownership and exact attribution. Preserve existing provider identity/trust checks and local no-IdP login. Access/derived-row identity does not replace workload identity.

No new principal model, parallel grant store/checker, compatibility reader, historical backfill, migration purpose, deployment cutover, new Access UI, request inbox, notification, credential secret projection or generic row-policy system is authorized. This packet does not authorize a new migration or changes to architecture. A newly necessary material contract/persistence change requires the existing specification workflow; ordinary private implementation choices remain with the owner.

Return focused execution evidence and limits against the new immutable candidate. Do not launch implementation from review, initiate the next review, commit, push or change Git identity.
