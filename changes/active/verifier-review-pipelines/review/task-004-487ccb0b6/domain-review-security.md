# TASK-004 security domain review

Result: **FAIL**.

Subject: `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9...487ccb0b60a2a3b40b54ee9e12099880d042ba99`.
First review; no carried-forward acceptance. The library source inspected is the
Cargo checkout at `9fdf6dc77a329f34d96b7ad4f397331d2d18216b`, confirmed with
`git rev-parse HEAD`, under
`/home/thorrester/.cargo/git/checkouts/bohmian-code-review-fb10b0c8e2c99a82/9fdf6dc`.

## Boundary and authority

Reviewed tenant/admin association state, installation and repository authority,
sealed credential lifecycle, webhook authentication, the pinned library's real
outbound transport, source imports, and current publishing/execution permissions.
Authority: TASK-004; approved specification REQ-024–027, REQ-036/038,
INV-002/003; `public-contracts.md`; root AGENTS and SQL AGENTS;
`architecture/wyrd-security-posture.md` request-principal, permission,
outbound-credentials/SSRF, tenant-isolation and verification-authority sections.
Read the task-review skill and review-scope completely. Used CodeGraph before
source navigation, then expanded through direct source and the actual Git diff.

| Obligation | Source and proof inspected | Result |
|---|---|---|
| Association binds tenant/admin and is single-use and expiring | `github/service.rs::begin_association/complete_association`; `queries/github.rs::insert_github_association_state/consume_github_association_state`; migration FORCE RLS and unique active tenant/installation indexes; administration journey wrong-admin/tenant, replay, suspended/missing-permission cases | PASS |
| Supplied installation ID is verified under App credentials | `complete_association` calls library `GitHubApp::installation`, checks returned app ID, suspension and `missing_permissions`; active association is a conditional update protected by unique indexes | PASS |
| Credentials sealed, redacted, rotated and removable | `github/credentials.rs` binds decrypted payload to tenant/integration and zeroizes plaintext; `service.rs::view/rotate/retire`; SQL CAS replacement and NULL on removal; keyring rewrap enumeration includes GitHub column; HTTP/MCP body decoders report positions only | PASS |
| Safe outbound destination resolution and connection | Provider delegates directly to pinned library's unscreened `ReqwestTransport` | FAIL: SEC-004-1 |
| Signed raw-body ingress and bounded decode before durable acknowledgement | `github/webhook.rs::accept`, library `WebhookSecret::verify` uses HMAC `verify_slice`, `WebhookEvent::decode` enforces size, integration/installation checks precede insert; router uses protected edge; commit precedes 200/202 | PASS |
| Typed exact host/repository scopes and public authorization | `permission.rs::validate/repository_*`, `permission_scope.rs::covers/validate`; manual execute/enqueue `authorize_run` requires Verifier run and repository run before pinning; `inspection.rs::retained/queued` checks exact repository read/run; HTTP and MCP delegate shared service and SDK/CLI delegate shared client | PASS |
| Installation, allowlist and current Wyrd grants intersect for source acquisition | `GitHubAdministration::repository` enforces active integration, installed/allowed row and narrowed operation token; automated trusted-tree acquisition never checks publisher source permission | FAIL: SEC-004-2 |
| Separate publisher/executor and normal declaration checks | `declare_local` obtains publisher through current `BackgroundAuthority::unattended`, `declare_loaded` uses existing declaration/upload owners; `submit` uses execution caller and `enqueue_pinned`; Check writes check current execution-principal repository write | PASS except missing source authorization above |
| Revocation cancels affected active work and preserves evidence | Revoke/disable/remove only mutate access/integration rows; no execution cancellation handoff | FAIL: SEC-004-3 |
| Pinned import paths cannot read host files or remote sources | TargetTree materializes regular entries only with relative normal components; `declare_local` rejects root/parent components; `load_checkout` uses contained `PathSandbox`; artifact sources resolve through `resolve_regular_file`; source stays pinned base, not PR head | PASS |

## Proposed material findings

### SEC-004-1 — GitHub transport bypasses deployment SSRF screening

Classification: **VIOLATION**. Location:
`crates/wyrd/wyrd-server/src/components/github/provider.rs:79,97–98,123–128`.

The security posture's outbound rule requires bounded DNS resolution, rejecting
forbidden resolved addresses, and connecting to those screened addresses without
re-resolution before fetching tenant-supplied URLs. A valid HTTPS spelling is
not sufficient.

Reachable producer-to-sink path: a caller with tenant `integrations:write`
creates an integration whose `host` is a tenant-supplied GHES base, using any
valid RSA key and two active principals. `create` verifies the spelling only.
`begin_association` issues state and `complete_association` calls
`app.installation` before any provider verification could refuse the fake GHES
endpoint. `GitHubProvider::host` calls library `GitHubHost::enterprise`; its
actual implementation (`identity.rs:378–394`) only checks the HTTPS prefix,
nonempty host, userinfo/query/fragment/whitespace and trailing slash. It accepts
private-address hostnames and numeric private addresses. Wyrd's `Transport::send`
delegates directly to the library transport. At the pinned revision,
`github.rs:165–171,193–206` builds an ordinary reqwest client and directly calls
`builder.send()`, with no DNS screening or pinned resolution.

Consequently a tenant administrator can make the shared server initiate requests
to deployment-private HTTPS services, including a trusted-certificate internal
hostname, through the association call. No real GitHub installation or successful
association is required to reach the outbound request. An attacker-controlled
DNS hostname can also resolve to a forbidden address. TLS validation and disabled
redirects do not implement the required network boundary. This is material in
supported multi-tenant deployments: tenant administration does not authorize
access to the host's internal network. The archive hop uses the same transport,
so screening only association would leave a sibling bypass.

Smallest correction boundary: keep GitHub protocol/retries/identity in the
library, and make its transport seam use the existing deployment-policy
`ScreenedHttp::client_for` mechanism for every effective URL, including archive
hops, before sending credentials. Existing server reuse evidence is
`verification/operators.rs::OperatorDelivery::client` at 770–790. Do not create a
new resolver, policy plane or second REST client, or add only a hostname list.
The provider currently receives only `GitHubConfig`; wire the existing deployment
policy to this owner. This is mandatory transport integration, not a new product
decision. Preserve the library's body/time/retry bounds and credential stripping
on archive redirects.

Focused proof: with production network policy and a controlled resolver, attempt
association with a GHES host resolving to loopback/private/link-local addresses;
assert refusal before transport sends/attaches credentials. Exercise an allowed
destination and a permitted archive redirect that resolves to a blocked address,
asserting each hop is screened and connection-pinned. Existing provider host
tests only reject HTTP/empty/trailing-slash strings and cannot establish this.

### SEC-004-2 — Automated imports do not authorize publisher source access

Classification: **MISSING**. Location:
`crates/wyrd/wyrd-server/src/verification/repositories.rs:324–378,573–580,739–760`.

REQ-025/026 require installation access ∩ allowlist ∩ Wyrd permissions for
repository source reads. REQ-038 explicitly requires the publishing principal's
normal declaration permissions **and source access**. The execution principal's
grants cannot supply the publisher's source authority.

`PullRequestIntake::review` obtains a `RepositoryClient` using only tenant,
repository key and provider permissions, reads the PR, and calls `trusted`.
`trusted` downloads the whole pinned archive before any publisher caller is
resolved. `declare_local` resolves the publishing principal only after loading
the import graph, and then checks normal Card/artifact permissions through
`declare_loaded`. Neither this path nor `GitHubAdministration::repository`
checks `Permission::repository_read` for the publisher. The latter deliberately
has no caller parameter and only enforces installation/allowlist/token narrowing.

Concrete supported scenario: assign the publishing Service principal the normal
`editor` role but no repository grant, configure a local Verifier import, allow
and activate the repository, and deliver a signed PR event. The target source is
downloaded and its Cards/artifacts are declared under that publisher anyway.
This is also the ordinary test setup: `automated_deployment` at
`sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs:1271–1299`
bootstraps the publisher with `editor` while repository grants are added only to
the executor. `builtin_roles.rs::EDITOR` contains no repository permission.
Revoking only the publisher's repository read therefore does not stop subsequent
source acquisitions/imports. A valid installation token is being treated as the
missing Wyrd source grant, contrary to the explicit intersection.

Smallest correction: resolve the publisher's current authority and authorize and
audit exact `Permission::repository_read(key)` before acquiring the trusted
repository source on its behalf; carry that same accepted publisher through the
existing import/declaration workflow. Reuse `BackgroundAuthority` and
`audit::authorize`, not a new policy helper. Keep execution run and Check write
authority separate. Update test setup to grant source read deliberately and add
one denied/mismatched repository publisher case. No new permission or API is
needed.

Focused proof: with an otherwise valid allowed/active integration, a publisher
holding Card/artifact writes but no exact repository read (then a grant on a
different host/repository), assert no archive/source request and no declaration
occurs. Grant the exact read and assert the ordinary local import succeeds with
publisher attribution; remove it before a subsequent event and assert source
acquisition stops. Do not rely on an execution-principal read grant for this test.

### SEC-004-3 — Repository revocation leaves admitted executions running

Classification: **MISSING**. Location:
`crates/wyrd/wyrd-server/src/components/github/service.rs:776–789,1249–1282` and
`crates/wyrd/wyrd-sql/src/queries/github.rs::set_github_repository_allowed`.

REQ-025 explicitly says revocation blocks new IO/publication **and cancels
affected active work**. After `set_allowed(false)`, `change` only updates
`allowed`/`pull_requests_enabled` and commits. `retire` similarly only disables
the integration (and optionally erases credentials). The SQL updates have no
execution mutation and the migration defines no revocation trigger. Sibling
repository reconciliation only marks rows `installed=false`; installation events
are decoded but ignored by `PullRequestIntake::handle`'s catch-all arm.

Concrete scenario: enqueue or start a slow repository Verifier, then revoke that
repository using the authorized public operation. The response says revoked, but
its existing execution remains queued/running and can finish/perform ordinary
engine operations under its accepted authority. The generic runner's target
resolution (`runner.rs:489–516`) checks Verifier/Card readiness, not repository
access; its dispatch uses the accepted execution authority. No revocation writer
calls the cancellation owner. Future GitHub clients and Check publication will
refuse the revoked allowlist, but that does not cancel an admitted execution.
The current access journey at `repository_verification.rs:883–911` asserts only
that a new execute after revoke/suspension fails and retained evidence remains
readable; it never holds an active execution across revocation.

Smallest correction: at repository/integration access revocation's existing
transaction boundary, cancel exactly the affected unfinished repository
executions through `VerifierExecutionQueue::cancel` (3335) or a bounded
repository-scoped extension of that same queue owner. Include manual repository
executions as well as webhook-created reviews; joining only repository-review
rows would miss the public execute/enqueue siblings. The existing cancellation
mechanism fences workers, notifies renewals and preserves pending terminal
delivery, so reuse it rather than adding another worker stop protocol or deleting
evidence. Apply the same outcome to observed installation removal/suspension
through its reconciliation owner, preserving unrelated repositories. This is
specified cancellation behavior, not a new lifecycle policy.

Focused proof: hold one manual and one automated execution unfinished for the
repository, revoke it, and observe cancellation/fencing without a successful
late settlement; retain a control execution for an unrelated repository. Repeat
the integration-disable/removal boundary against its affected work. Retained
evidence remains readable under ordinary read grants and terminal delivery stays
owed until its existing acknowledgement.

## Verification limits and immutable subject

This is a static source review. No Cargo/build/test/lint/codegen was run by this
reviewer; orchestration owns Cargo serialization and journey reruns. Test
assertions were inspected as evidence, not represented as independently rerun.
No live GitHub/GHES or private-network exploit was attempted. The findings above
identify direct source paths and missing mandatory checks/handoffs.

Final inspected HEAD remains `487ccb0b60a2a3b40b54ee9e12099880d042ba99`.
Source status showed only the pre-existing two modified skill files and
untracked change packets; this reviewer changes only this report. No other
reviewer report was read. Optional hardening and unrelated pre-existing debt
were excluded.
