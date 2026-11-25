# TASK-005 domain review

Subject: base `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`, candidate
`78cd99091c81e5a7d706ac6f54a8222e62705e0a`. Candidate HEAD remained unchanged.
Approved authority: authorization-model spec revision 12, REQ-021 and AC-016,
the two revision-12 persistence rows, and TASK-005 scenarios 1–3; preservation
of company-login and platform-plane boundaries follows REQ-016/018 and the
security posture's browser, permission-model and Postgres-tenant sections.
Read the SQL/Python/TypeScript AGENTS and Rust/agent-surface/maintainer guides.
No CodeGraph directory exists. This is a static independent boundary review;
execution results below are attributed to the task record and maintainer
handoff, not fresh executions.

## Boundary coverage

- **Display-name production and storage:** `wyrd-auth-oidc/src/claims.rs`
  takes the nonblank standard `name`; `github.rs::GithubUser::display_name`
  prefers profile name then login. Company `finish_id_token_exchange` and
  social `finish_social` refresh through `users.rs::refresh_display_name`
  inside the same tenant transaction as successful authorization/session
  completion. An absent name is a no-op; subsequent refusal rolls the update
  back. No name enters identity matching or grants. Nullable columns are the
  two expressly authorized edits to the unshipped auth migration; existing
  forced RLS remains. Signup-name deferral is approved difference 12.
- **Reads and authorization:** `accounts.rs::current_user_profile` rejects
  non-Users, obtains the verified caller's tenant and principal ID, and reads
  that User only. `routes.rs::list_principals` retains admin authorization,
  bounded keyset paging and tenant-bound SQL. Authentication derives from
  username/linked issuer, not email. `auth/mod.rs` shares the assignable union
  and role-holder subquery between directory filters/counts and Role member
  counts; Group membership expands into accounts and `IN` prevents duplicate
  members. Directory roles retain distinct role/scope pairs, while full access
  reads retain source-qualified grants. Deleted accounts and nonassignable
  internal principals remain outside the union.
- **Role descriptions:** seed updates server-set built-in descriptions while
  preserving operations. PUT retains admin/built-in checks, validates the
  200-character limit and sets/clears the description in its existing
  transaction. Counts are computed in SQL, not the browser.
- **Consumers:** shared Rust types flow through `wyrd-client` and Rust SDK
  re-exports; Python `principals.rs` projects serde output and adds the `role`
  keyword with authored stub parity; TypeScript's unchanged native serde
  boundary consumes the extended query and returns generated public wire
  types. MCP `principals.me` uses the shared self-profile owner, Role PUT now
  forwards description, and Role list shares its typed owner. CLI forwards
  `--description`. No new directory MCP tool was promised or introduced.
- **Assertions:** SQL
  `display_names_are_nullable_refreshed_and_tenant_confined` and
  `role_holders_filter_count_and_member_count_alike` cover nullable/refresh
  storage, tenant confinement, direct/Group/overlapping grants, suspended
  accounts, empty Groups, filtered paging and member-count equality. Server
  `directory_projects_names_methods_roles_and_counts` proves projections,
  scopes, descriptions, refusal and foreign-tenant exclusion. Company
  `provider_sign_ins_store_and_refresh_the_display_name` drives controlled
  signed Google/company tokens through real callbacks and reads the stored
  result; the GitHub enrollment/return journey separately asserts blank-name
  login fallback and replacement by a later profile name. Rust/Python/TS
  principal journeys exercise their real clients and new fields, filter,
  counts, foreign tenants and built-in refusal. Their display-name/self-email
  samples are null; populated provider names are proved through real server
  HTTP/browser journeys, a limitation explicitly recorded in TASK-005.
- **Hand merge:** reviewed merge `8cc9b03d3` and both versions of
  `platform_admin_e2e.rs`. `identity_token_reply_with` and the new
  `identity_token_body` converge on `identity_claims`/`signed_identity_body`;
  issuer, audience, nonce, expiry and `email_verified` all survive, including
  false values supplied by negative tests. Named replies add only `name`.
  Company `corp_token` still supplies its company audience/groups and uses
  the common signer. Discovery retains RFC 9207 issuer advertising, JWKS and
  caller-selected token endpoint. The console's held-token callback test
  reaches that endpoint, removes/reinstalls the connection, asserts refusal,
  no cookie/no live session, and successful fresh/password/API alternatives.
  Contacts' post-answer mail changes poll for a new message before redeeming
  the actual proof; they do not synthesize proof or bypass binding. No defect
  found in these hand-merged token helpers.

## Proposed finding D1 — lost served-contract assertions

**Classification:** EVIDENCE_GAP with a test-coverage regression. **Blocking.**

**Authority:** REQ-021 read-projection table and SDK/MCP projection promise;
TASK-005 scenario 3 explicitly assigns served OpenAPI proof to
`pg_openapi_contract.rs`. Root AGENTS prohibits deleted or weakened tests;
TASK-005 scenario 6 preserves predecessor company-policy contracts.

**Location:**
`crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:389`
(`identity_connection_operations_publish_their_contract`) and the missing
`principal_read_projections_publish_their_contract` test, formerly at
`cc5cea627:644` immediately after the tenant-login contract test.

**Trigger and expected result:** after integrating PR 244, run the task's
OpenAPI/principal lane against the candidate. The served-document assertions
must still check the new profile/directory/Role fields, requiredness, method
enum, role query parameter and description refusal, plus the existing
company-login-policy contract assertions.

**Actual proof:** candidate has no test named
`principal_read_projections_publish_their_contract` and no `display_name`,
`member_count` or `PrincipalStatusCounts` assertions in this file. It exists
in `git show cc5cea627:crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs`
at lines 634–720. The cumulative diff also reduces the identity contract
operation array from eight to six, removes GET/PUT `/v1/identity/login-policy`,
the `LoginPolicyChange` request-body assertion, and exact safe
`LoginPolicyView` fields. TASK-005 still cites the now-absent principal test
and reports post-merge passing OpenAPI/principal lanes; a passing lane cannot
execute a test absent from its source. MCP schema checks cover MCP, not the
served OpenAPI document. This is not proof that the generated HTTP contract
itself is wrong.

**Consequence:** required HTTP contract evidence was lost during integration,
and the retained evidence record overstates candidate coverage. A generated
contract regression in these additions would escape the claimed focused
checks.

**Smallest correction:** restore these lost assertions in the existing served
OpenAPI test owner while retaining PR 244's platform-cookie assertions. Run
the exact restored principal test and the existing identity-connection test
inside the repository-managed Postgres lifecycle, and update TASK-005's
evidence to the restored candidate. No production change is prescribed unless
those assertions expose an actual contract defect. Preserve all current
platform/session tests.

## Limits

No Cargo, database, browser or external-provider execution was performed by
this reviewer. The user reports post-merge `platform_admin_e2e`, principals,
OpenAPI and codegen lanes passed; this review checked their actual assertion
boundaries and identified D1 despite those reported results. The broad merged
Bifrost/benchmark implementation is outside TASK-005's domain assignment.
This specialist report makes no overall task verdict and no historical TDD
claim. No production files changed.
