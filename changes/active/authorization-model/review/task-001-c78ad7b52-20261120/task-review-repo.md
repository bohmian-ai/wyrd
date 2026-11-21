# TASK-001 repository boundary review

## Subject and scope

Base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
Candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
Approved spec: `SPEC-authorization-model`, revision 2, at
`38ac3ca8a0c365825124aa3e99a36ba19888e26f`.
Original task: `changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md`.
Checked-out implementation at `3139c08cb0b7c0194583a66af3ab31bef28dc14f`
matches candidate; later authority/design commits are outside implementation review.

Assigned boundary: public contract ownership and projections, MCP administration,
affected CLI/SDK/documentation consumers, SQL capabilities/migration rules, and
required Rust item documentation. No complete acceptance matrix is repeated here.

Governing material: root and SQL/Python/TypeScript/docs `AGENTS.md`,
`changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, Rust/error/
agent-surface topic guides, `TESTING.md`, approved REQ-001/008/010 and persistence
changes, security-posture authorization and PostgreSQL tenant-binding rules.

## Coverage

- Wire definitions move to pure `wyrd-spec/src/auth`; runtime re-exports the
  common permission definitions. The new contract types have no IO/PyO3/SQL
  dependency. `wyrd-client::Principals` remains the SDK-facing owner.
- HTTP access role/binding/group routes are mounted by
  `components/principals/routes.rs:71-96` with typed contracts. Administrative
  authorization, tenant connection acquisition, revision advancement and commit
  are shared in `access.rs:44-62`.
- Rust `Principals` exposes access summaries, role operations, bindings and groups.
  Python native methods, registration, public package and authored auth stubs,
  and TypeScript native/wrapper methods project those calls. Generation roots in
  `scripts/gen_ts_types.py` and `wyrd-spec/examples/gen_schemas.rs`, generated TS
  wire types and N-API declarations were updated in the range.
- CLI `principal access` replaces `principal role` and exposes explicit-scope
  binding grants/removal, custom roles and groups. Its removed public consumers
  are covered in REPO-02.
- Actual Rust journey assertions at `principal_roles.rs:1023-1191` drive public
  clients through group membership changes, family bindings, group-source access
  summaries, and exact custom-role reads that refuse newer/other versions.
  Python/TS scoped test and adapter changes were inspected; task-recorded runs
  are execution evidence rather than new executions performed here.
- New SQL query functions accept `TenantConn` and execute through its transaction;
  borrowed-connection functions do not commit. New tables lead with tenant ID,
  force RLS, use tenant-qualified references, and stay within approved schema
  purposes. No new SQL function or applied-migration edit occurs in the range.
  Existing operator-session policies are reused. Production SQL capability types
  and statements were inspected independently of scanner results.
- Shared assignment validation checks syntax/applicability without target-owner
  resolution. The primary behavior review owns that proposed cause; no duplicate
  finding is recorded here.

## Proposed findings

### REPO-01 — MISSING: scoped access has no MCP administration projection

Requirement: REQ-008 explicitly requires common administration contracts through
HTTP/CLI and thin first-class SDK/MCP projections. Original task lines 62-65
assigns administration/access-summary projections to this backend task.

Location: `crates/wyrd/wyrd-server/src/mcp/principals.rs:41-79`;
`crates/wyrd/wyrd-server/src/mcp/mod.rs:177-193,224-243,388-422`.

Scenario: a tenant administrator connects a real MCP client to inspect Alice's
bindings, bind viewer at a Repository family, administer a custom role/group,
or remove that binding.

Expected: typed MCP operations reach the existing Rust access owner and return
the same scoped-administration results, retaining read discovery and write
permission enforcement. Actual: the complete principal descriptor lists contain
only `principals.list_credentials`, `principals.me`,
`principals.revoke_credential` and `principals.set_workload_restriction`.
Registration and dispatch have no access-summary, role, binding or group operation.
Naming an absent tool cannot reach the owner, so this workflow requires changing
surfaces despite the approved MCP promise.

Evidence: complete principal descriptor lists and dispatch above. The added
`a_family_bound_agent_reads_only_that_family` journey at
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:840-933` provisions the scoped
account through Rust `Principals` at lines 876-891 and calls only MCP `cards.list`
and `cards.get`; it proves read filtering rather than administration projection.

Smallest correction: extend the existing principal MCP adapter/typed arguments to
project existing non-secret access-summary, role, binding and optional-group
operations. Reuse server validation/mutation/audit, not a second checker/store.
Preserve the existing exclusion of plaintext credential issuance from MCP.

Closure: a focused real MCP journey discovers typed read shapes, administers a
scoped grant and reads its result, removes it, and directly attempts a refused
non-admin write with no mutation. Cover role/group calls against shared owners
and schema parity. Use `test:bifrost:journey:mcp` setup and exact named selectors,
plus affected contract-generation and format/lint checks.

### REPO-02 — REGRESSION: served guidance calls removed APIs and teaches superseded authority

Requirement: REQ-010 requires replacing superseded assignment callers together;
REQ-008 requires usable scoped administration. Original task explicitly requires
updating affected served docs. REQ-004 defines current authority rather than
token-renewal updates; REQ-008 separates approver/auditor from editor/viewer.

Locations:
`docs/src/content/docs/operate/grant-and-inspect-permissions.md:24-76,108`;
`docs/src/content/docs/operate/add-people.md:143-146`;
`docs/src/content/docs/understand/identity-and-authorization.md:43-63`;
`docs/src/content/docs/build-and-verify/verify/approve-a-pull-request.md:15`;
`docs/src/content/docs/reference/mcp-tools.md:42-47`.

Scenario: an administrator copies the Permissions/Add people guide to grant access,
or a user follows role/retained-read guidance to select required authority.

Expected: documented calls use explicit scoped bindings and present the server's
current role/read/propagation behavior. Actual: Permissions still directs removed
`wyrd principal role list/grant/revoke`, Python/Rust `grant_role`, TypeScript
`grantRole`, and removed `changed`/`roles` response fields. These cannot execute:
`principal/mod.rs:24-43` replaced `Role` with `Access`, shared client
`principals/handle.rs:376-472` replaced assignment methods, and SDKs project that
replacement. Add people repeats the removed command. Both describe up-to-five-minute
role changes on token renewal. Identity and access claims four cumulative,
tenant-wide roles and viewer audit access; the approval guide claims editor
approval authority. MCP reference execution reads require the referenced Verifier
instead of the trusted execution parent.

Consequence: published copyable access workflows fail, and users are told to grant
wrong authority for audit, approvals and retained reads. This is changed API/behavior
drift, not prose preference.

Smallest correction: update affected existing sections to `principal access` and
SDK access/bind/unbind, explicit scope, six approved role bundles, healthy
five-second propagation and trusted-parent reads. Reuse tested copyable examples;
remove superseded calls rather than add compatibility aliases. Do not redesign
guides or add workflows.

Closure: content review against shared contracts and actual relevant journey/CLI
parser proof, plus docs checks (`docs:check` per docs rules). No production change
is needed for this finding.

### REPO-03 — VIOLATION: changed Rust items lack mandatory rustdoc

Authority: root `AGENTS.md`, Code, requires rustdoc on every new/changed Rust item
and explicitly says missing rustdoc blocks merge.

Locations: `crates/wyrd/wyrd-auth/src/roles.rs:95` removes existing type rustdoc
on public `RoleAdminError`; new
`crates/wyrd/wyrd-sql/src/queries/auth/authority.rs:14` declares
`AUTHORITY_STATE_SQL` without item rustdoc; new
`crates/wyrd/wyrd-sql/src/queries/auth/role_bindings.rs:21-108,407` declares SQL
constants and fallible `decode_source` without item rustdoc.

Scenario: the new authority/binding owner is documented or maintained. Expected:
changed items state intent/workflow/invariants, with error behavior on fallible
items. Actual: these declarations have no item documentation, and the public
error type loses its former documentation. Evidence is candidate source and the
base-to-candidate diff. Module documentation does not satisfy the explicit
per-item requirement. Consequence is a mandatory repository acceptance violation.

Smallest correction: document these changed items concisely, including
`decode_source`'s decode-error condition, and restore error-type intent. Check
other new items in touched owners for the same omission; leave untouched comments
alone. No behavioral test or production logic is required; run affected Rust
format/lint checks.

## Verification limits

No source/contracts were changed and no Cargo command, journey or docs build
was rerun. Source/content establish these proposals, which still require
independent validation. Runtime claims use actual assertions and task-recorded
execution evidence. Current instructions exclude TDD reporting. No prior remediation closure applies to this first review.
