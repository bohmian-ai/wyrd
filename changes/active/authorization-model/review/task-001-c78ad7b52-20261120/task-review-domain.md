# TASK-001 domain review

## Subject and scope

Base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
Candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
Approved contract: `changes/active/authorization-model/spec.md`, revision 2,
recorded by `38ac3ca8a0c365825124aa3e99a36ba19888e26f`.
Original task: `changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md`.

The implementation diff was read directly between those commits. The later
authority and design commits are governing context, not implementation changes.
This report covers the security boundary between scoped grants, tokens and
accepted work; Card and retained execution reads; approval/download access;
managed verification SQL and its peer representation. Cache propagation and
cross-tier projection completeness are separate assignments.

Authority consulted: root `AGENTS.md`; `changes/spec-driven-development.md`
including ambiguous wording; `agent_docs/maintainer-style.md`; `TESTING.md`;
`architecture/wyrd-security-posture.md` credential, delegation, authorization and
retained-data sections; `architecture/wyrd-design.md` runtime identity and
Repository sections; `architecture/bifrost-design.md` public authorization.

## Boundary trace and proof assessed

- `wyrd-spec/src/auth/access.rs::compile_binding` compiles direct/group/custom
  operations through one applicability check. `cards:read` adds
  `executions:read` at the same scope, so custom Card readers inherit retained
  reads without requiring a built-in role name. The compiler rejects a wholly
  inapplicable binding instead of replacing its scope with All.
- `wyrd-spec/src/auth/permission.rs` and `permission_scope.rs` enforce separate
  resource, action and scope axes. Family includes future versions; exact scope
  requires the same trusted kind/name/UID. `PermissionSet::insert` preserves
  explicit gateway grants beside All; `intersection` refuses unrepresentable
  open-only deployment intersections. `EffectivePermissions` requires every
  ceiling and fails closed when a flattened Card boundary cannot be represented.
- Built-in bundles in `wyrd-runtime/src/builtin_roles.rs` separate viewer,
  editor, workload, approver, auditor and admin. Scope compilation and tests
  cover the principal read/approval/audit boundaries. A missed write consumer is
  detailed below.
- Shared issue/verify shells use token version 2, deny unknown claims, remove
  permissions/roles and carry an optional signed ceiling. Signature-only
  principals start with empty authority. `TenantAuthority::authorize` attaches
  current subject/actor grants and applicable credential/signed limits.
  `DelegateToken::execute` resolves current subject authority; issuance combines
  the actor and inherited restrictions before signing. Existing focused tests
  inspect token claims, scoped intersections, disjoint/unrepresentable cases
  and root validation. No legacy token reader was found in this path.
- `AcceptedAuthority::new` serializes the flattened admitted ceiling and
  identity. `Caller::accepted` and runner dispatch retain that authority with
  SYSTEM as executor, rather than granting SYSTEM permissions to the accepted
  subject. The task cites the real `pg_verification_runtime` recovery assertion
  checking byte-identical authority after account/key changes. Principal
  revocation cancellation remains a separate lifecycle boundary.
- Card list/version predicates consume `EffectivePermissions::card_objects`
  before pagination. UID/ref/latest and Card downloads check the same target
  scope. Scoped local blob reads must resolve a completed file of a visible
  Card. The task's SDK/CLI/MCP family-read assertions check actual membership and
  not-found for the other family, rather than merely successful requests.
- `VerificationControl::retained` applies coarse execution operation admission,
  obtains the queue/accepted/direct identity, then requires parent execution
  read. A denied reader sees execution-not-found. Approval additionally requires
  the exact Verifier approve operation. Publication/download operations reuse
  retained execution authorization. Real approval and Repository-family download
  journey assertions cover allowed and other-parent refusals. The direct
  execution deletion path is defective as detailed below.
- The PR admission path uses exact registered Verifier scopes and the selected
  declaration authority. Retained review membership is queried from Bifrost;
  GitHub configuration is separate and retained reads do not call the provider.
  `pg_github_pull_requests` checks family-scoped membership and count equality,
  execution reads and other-family refusal. Queue removal is exercised there.
- Oracle admits every resolved source separately. Managed verification scans
  are rewritten before optimization; scoped readers receive parent filters and
  CASE projections retaining Arrow field metadata. The source assertions cover
  family, exact, table-only, parentless, join, subquery, CTE, count and a predicate
  that would fail on a hidden row. Native physical plan encoding/decoding keeps
  the masked plan, as checked by the codec peer unit test. Forwarded context
  admission uses the same `reads_bifrost` capability. Mixed ordinary sources
  still require their table scope. Captured payload column authorization remains
  independent, including predicate uses.

Recorded execution results were inspected in the task and assertions were read;
they were not rerun by this assignment. The recorded masked-peer check uses a
MemTable and the physical codec. It does not by itself establish a scoped caller's
complete public query path through live peers or published Parquet. The primary
coverage review must reconcile that distinction with other journey evidence.

## Proposed blocking findings

### DOMAIN-01 — Scoped Card authorship is accepted as a binding but rejected by write consumers

Classification: `MISSING`.

Requirement: REQ-001 supports Card family/exact scopes for operation bindings;
REQ-008 defines editor authoring within applicable scopes. Security posture
Permission model requires the resource, action and scope to cover the actual
operation target. The task explicitly owns Card/storage consumers.

Locations:

- `crates/wyrd/wyrd-server/src/components/cards/routes.rs:824`, especially 833;
  callers at 513, 581, 643 and 705.
- `crates/wyrd/wyrd-server/src/components/storage/routes.rs:706`, especially 715;
  upload admission at 98 and upload-owner checks at 691.
- Producer: `crates/wyrd-spec/src/auth/permission.rs:400` permits Card-scoped
  `cards:write|delete`; `access.rs:309` preserves those scopes during compilation.

Starting conditions and trigger: an administrator binds editor to a User or
Service at Model family `payments`, without a tenant-wide editor binding. The
caller reads payments successfully, then declares its next release, completes
its declared files, or deletes an exact payments version. The same problem
occurs for a custom scoped Card writer and exact-version completion/deletion.

Expected: authorize the applicable write against each affected registered or
declared Card identity; permit the covered operation and refuse uncovered graph
members/other families without partial mutation. Card file stages must preserve
the same target boundary.

Actual: every listed Card mutation calls `authorize_card_write`, which constructs
`Permission::card_write()` with `PermissionScope::All`. The shared checker
correctly refuses a family/exact grant because it cannot cover All. The scope
string in the audit resource is only a label, not the required permission scope.
Storage repeats the same broad check after identifying a Card-owned upload.
Deletion also never consumes the compiled `cards:delete` operation.

Evidence: this is a direct producer-to-consumer mismatch. The role compiler emits
the scoped operations and `PermissionScope::covers` never lets Card scope cover
All. The consumer requires All before calling the declaration/completion/delete
service. The new scope journeys exercise reads and binding propagation; their
editor mutation occurs with tenant scope and does not exercise this starting
state. No runtime failure is claimed from a new test run.

Consequence: administrators must grant authoring across the tenant to make the
advertised family-scoped editor usable. That defeats the requested ability to
delegate work on one resource.

Smallest correction boundary: Card write authorization at the existing server
Card/storage owners, consuming the same checker and trusted scope types. Check
every submitted graph member and the exact Card owning each completion/upload;
preserve transaction/idempotency and deny unrelated targets. Use the applicable
action for deletion. Do not broaden a narrow grant or add another checker.
Close with focused real-server SDK/HTTP journeys covering a family editor's
declaration/file completion/deletion, an exact grant's supported mutations, and
an uncovered sibling/graph member with no unauthorized persistence. Thin callers
must reach the same server decision.

### DOMAIN-02 — Deleting a Card hides retained direct executions

Classification: `INCORRECT`.

Requirement: REQ-006 captures the trusted exact authorization parent and keeps
derived records readable after Card deletion; family grants cover retained
history. Security posture Access and refresh tokens says retained identity and
metadata do not require fetching a deleted Card. REQ-010 requires normal retained
behavior without backfill machinery.

Locations:

- `crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:534`
  selects the direct-result path when queue and accepted state are absent;
  `direct_identity` at 694, especially 704 and 709-710.
- `crates/wyrd/wyrd-sql/src/queries/cards/get.rs:12`, especially 17, excludes
  deleted Card rows from `get_card_by_uid`.
- `crates/wyrd/wyrd-server/src/verification/inputs.rs:225` exposes the direct
  result's IDs; `VerificationControl::get_execution` consumes `retained` at
  `components/verification/service.rs:278`.

Starting conditions and trigger: a successful direct verification has retained
input/result rows and a reader holds the subject family's read binding. Direct
work has no queue row or accepted run state. Delete the subject Card (or its
Verifier) through the supported soft-delete operation and then reopen that
execution through the dedicated status/details route. Stored evidence and the
reader's family binding remain.

Expected: the same authorized reader can read the retained execution with the
original exact identity and attribution. An unrelated reader remains not-found;
the public Card definition itself remains deleted.

Actual: `retained` finds the direct result but calls `direct_identity` before
parent authorization. That method reloads both exact Cards using
`get_card_by_uid`, whose SQL excludes `status = 'deleted'`, and maps either missing
Card to execution-not-found. Therefore a supported Card deletion withdraws access
to already-retained direct verification history. Its rustdoc claims deleted Cards
are included, but the imported SQL function does the opposite.

Evidence: direct work's no-queue behavior is asserted by
`pg_verification_routes::direct_execution_judges_inline_without_enqueueing`.
The source path to `get_card_by_uid` is unconditional for the direct fallback.
The SQL tombstone filter proves the post-deletion refusal independently of
permissions. Queue-cleanup PR coverage does not exercise this direct/no-state
path; no supplied direct-deletion assertion was identified. No runtime failure
is claimed from a new test run.

Consequence: dedicated retained reads disagree with raw SQL's stored-parent
boundary after deletion, including for admins and otherwise-authorized family
readers.

Smallest correction boundary: retained direct identity resolution in the existing
verification reader/writer owners. Consume trustworthy retained identity or the
existing tenant-qualified retained-tombstone owner where sufficient; do not make
public Card reads include deleted definitions or infer identity from labels.
Preserve subject, owning workload, exact Verifier and actor attribution.
Close with a focused direct-verify journey: read before deletion, delete the
subject and separately the Verifier, then read status/details with the same
family and exact grants while unrelated scope remains not-found. Confirm raw SQL
and the dedicated read agree without a queue row or a live-Card lookup.

## Verification limits

No implementation, schema, test source or configuration was changed. No Cargo
command or live dependency test was run by this assignment. The findings are
source-established behavior, proposed for independent validation; this report
does not declare final retained finding IDs or a final verdict. TDD history is excluded by the explicit review request.
