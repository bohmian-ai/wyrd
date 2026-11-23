# TASK-003 independent behavior review

Subject: base `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`, candidate
`c93287b54bf944c8c8cc9323f81c12ba5174369c`. This is the first cumulative review,
using `git diff a5b1d5417...c93287b54`, not intermediate commits that briefly
exposed SDK methods. No source was changed. No CodeGraph index exists.

Authority: approved SPEC-stored-secret-encryption revision 1, assigned
REQ-002/006/007/008/009/012, original TASK-003 scenarios 1–5, current user
confirmation of HTTP and three CLI owners only, root/nested SQL/docs AGENTS,
`changes/spec-driven-development.md` (especially ambiguous wording, review,
and TDD), TESTING.md, maintainer/rust/agent-surfaces guides,
`architecture/README.md`, security posture stored-secret, outbound-credential,
tenant-isolation rules, and the approved administrator journey's identity,
replacement, revocation and propagation sections. Uncommitted instruction-file
edits are not implementation changes in this subject.

## Scope and navigation

I traced owner actions and their unchanged public predecessors: Operator
`OperatorConnectionControl::change` → `OperatorSecrets` → fenced SQL update;
Gateway `GatewayAdministration::{renew_credential_encryption,prepare_renewal}`
→ `ManagedSecretBinding::renew` → SQL CAS → existing audited refresh;
human `HumanConnections::{stage_renewal,renew_encryption}` → client-secret
owner → SQL; trusted issuer route → client-secret owner → SQL; platform route
→ client-secret owner → platform CAS. Consumers include delivery,
`load_snapshot`, `active_connection`, `PgIssuerResolver`, and platform sign-in.
Projections are five mounted utoipa HTTP handlers and three CLI commands;
the Operator helper is internal-feature-only and Gateway uses the existing
non-SDK `CredentialWriter`. No new SDK/MCP/UI surface, dependency, migration,
or benchmark was introduced.

The separate domain reviewer owns detailed tenant/permission, CAS/lifecycle,
refresh, remote-wait and failure boundary review. This report retains primary
requirement, public behavior, proof, docs, repository-rule and maintainability
coverage. No new common orchestration framework or owner duplication was found
that warrants a style finding.

## Acceptance matrix

PASS below means credible source/assertion coverage for the described bounded
claim, subject to the root's execution record and the domain report. It does
not turn partial lower-tier proof into a complete journey.

| Requirement / task scenario | Source and assertions | Result |
| --- | --- | --- |
| REQ-002/006; scenario 1, fresh encryption and unchanged plaintext | All five owners call `WyrdKms::renew` through existing bindings. Operator `pg_operator_delivery.rs:1054–1150` compares wrapped DEK/nonce and observes actual unchanged Authorization on delivery. Gateway `pg_administration_tests.rs:577–632` compares envelope, unchanged name/state/rotated_at, then loads and opens a fresh snapshot. Human `admin/identity.rs:948–989` compares id/revision/state/test stamp and opens unchanged secret. Trusted `admin/routes.rs:1528–1549` compares complete redacted view, changed envelope and resolver plaintext. Platform `platform_admin_e2e.rs:2032–2048` compares view, Transit encrypt count and performs confidential sign-in. Shared facade supplies random-DEK guarantees. | PASS for owner/encryption seam; complete journey gaps below |
| REQ-006/008; scenario 2, Operator replacement without old decrypt | `pg_operator_connection_routes.rs:365–406` corrupts old envelope, renewal refuses, PATCH stores a complete new envelope and increments secret version. Existing delivery rotation tests check actual next-attempt and replica behavior. | PASS |
| REQ-006/008; scenario 2, Gateway/human/platform existing replacement | Gateway existing put seals new typed value without opening old; replacement race test prepares REPLACED. Human existing candidate/test/activate and revision/session effects remain; `identity_e2e::tenant_connection_rotation_journey` exercises real activation/sign-in/replicas. Platform existing PUT seals supplied value. No changed auth protocol or session policy. | PASS structural and existing regression proof; root must distinguish inherited execution claims |
| REQ-006/008; scenario 2, trusted issuer replacement preserving references | Admin create remains INSERT-only; delete is FK-protected or cascades bindings. Boot upsert is not a public owner-authorized replace action. See B-1. | FAIL |
| REQ-007/009; scenario 3, revocation never undone | Operator disabled renewal preserves disabled status; race refuses, exact route assertions in `pg_operator_connection_routes.rs:278–307`. Gateway renewal/revoke race and revoked-state/envelope assertions `pg_administration_tests.rs:661–697`. Human deactivate race and no-active assertions `admin/identity.rs:1033–1068`. Trusted delete race and absent row `admin/routes.rs:1650–1671`; platform deletion race, 409 and 404 persistence `platform_admin_e2e.rs:2064–2103`. Existing restart/replica consuming tests remain. | PASS source/assertions; domain reviewer covers common-state proof |
| REQ-008/009; scenario 4, authority, redaction, failures | Operator real route wrong tenant/403 audit/outage; human/trusted direct handler wrong tenant/403 audit/outage/races; Gateway service outage/races, existing common audited authorization boundary. No caller-supplied decrypt selector; responses remain existing typed redacted views; KMS plaintext stays within existing facade. | PASS bounded owner assertions; domain report and journey gaps apply |
| REQ-006/008/012; scenario 5, HTTP/CLI coverage | Operator and platform use real routers; Operator/Gateway CLI use real server/Postgres. Human and trusted issuer new HTTP behavior is only direct handler invocation; trusted CLI talks only to wiremock. Human sessions and Gateway dispatch are not consumed after renewal in the new tests. See B-2. | FAIL evidence |
| HTTP/OpenAPI contracts and mandatory affected checks | Five routes are registered and annotated; existing Operator served-contract exact set still expects five operations, although six are shipped. See B-3. | FAIL |
| REQ-012 documentation | Resource/action and upstream distinctions added; actual renewal error/state behavior and timestamp metadata contradict blanket statements. See B-4. | FAIL bounded content correction |
| Scope and structural constraints | Cumulative diff excludes migrations, SDK/MCP, UI, runtime inference/verify/Bifrost changes and dependencies. New remote calls occur in administrative preparation before write transactions; detailed wait assessment assigned to domain reviewer. | PASS; no benchmark requested |

## Proposed blocking findings for independent validation

### B-1 — MISSING: trusted issuer has no reference-preserving credential replacement

Authority: REQ-006 requires typed replacement under the same identity and
client references, without old decrypt; REQ-008 requires failure to preserve
prior committed state; TASK-003 scenario 2 assigns that promise across owners.

Locations: `crates/wyrd/wyrd-server/src/components/admin/routes.rs:355` and
`:481–490`; `docs/src/content/docs/reference/stored-secret-encryption.md:129`;
TASK-003 replacement evidence's trusted-issuer exclusion.

Starting condition: a confidential trusted issuer is in use by one or more
workload bindings and its secret expires or is exposed. Trigger: administrator
tries to supply a replacement using the documented public path. Expected:
replace the secret under the existing issuer/bindings; no dependence on opening
the old envelope, no destructive intermediate state, and failure leaves the
prior committed state intact. Actual: POST uses `insert_trusted_issuer` and
conflicts for an existing issuer. Documented delete/add first fails with 409
because bindings reference it; `cascade=true` deletes those bindings. Re-add
does not restore them, and discovery/key-service failure after deletion leaves
the issuer absent. The existing boot-only `upsert_trusted_issuer` can preserve
bindings but is not the required authorized HTTP/CLI action, and boot seeding
does not implement the requested concurrent-revocation CAS contract.

Consequence: a supported issuer cannot replace its credential while keeping
client references; the documented workaround destroys the references or cannot
run. This is assigned missing behavior, not an unrelated preexisting bug or
request for a new authentication integration. The task statement that the
spec names no exact route cannot waive the behavioral requirement.

Smallest correction boundary: existing trusted-issuer administration, existing
typed auth/sealing/SQL owners and existing service_accounts:write boundary.
Provide the assigned replacement through HTTP/CLI with current-state fencing,
preserve bindings/issuer identity, never open old ciphertext, and document it.
No authentication redesign, new grant/token protocol, migration, SDK or MCP
projection is needed. Prove replacement of a bound issuer, failed preparation,
unreadable prior envelope and losing-to-revoke behavior through focused real
server/Postgres journeys and the existing resolver/exchange consumer.

### B-2 — EVIDENCE_GAP: incomplete renewal journeys and downstream/session proof

Authority: AGENTS Testing 1; REQ-006/009; TASK-003 scenarios 1 and 5 require
real-server journeys on shipped surfaces and downstream use, including unchanged
human sessions. Lower-tier integration is useful but does not substitute.

Locations: `crates/wyrd/wyrd-cli/tests/secret_sources.rs:166–217` starts a
`MockServer` and returns a fabricated successful issuer view;
`crates/wyrd/wyrd-server/src/components/admin/routes.rs:1528–1549` invokes the
handler directly and opens the resolver; `components/admin/identity.rs:948–989`
invokes the handler directly and opens `active_connection`, but never signs
in or refreshes/uses an existing session after renewal;
`components/gateway/pg_administration_tests.rs:621–632` opens a new snapshot
but never dispatches through the gateway after renewal. Repository-wide search
for renewal route/method calls finds no other human/trusted renewal journeys;
unchanged identity/issuer/gateway journeys never invoke the new renewal.

Expected proof: administrators use each shipped surface against the real
composed server and Postgres, then existing consumers continue under unchanged
references and session semantics. Actual proof leaves HTTP extraction/routing
and real trusted CLI authentication/persistence disconnected from their owner
tests; no human session is created or exercised across renewal; a prepared
Gateway secret is not observed by a provider request after renewal.

Consequence: the task's strongest acceptance promises lack required executable
proof. This finding does not assert production routing, sign-in or dispatch is
broken. A green rerun of these same lower-tier tests cannot close it.

Smallest correction: extend the existing identity journey(s) to call human HTTP
renewal and trusted CLI renewal, observe unchanged connection/revision, a real
subsequent confidential login, retained session use/refresh, and successful
workload exchange against the renewed issuer. Extend an existing Gateway
journey to renew then observe the unchanged key in actual provider dispatch.
Keep existing detailed owner race tests; do not duplicate all their permutations
at every surface or add SDK/MCP methods. Use focused existing journey runners
with repository Postgres and appropriate OIDC/provider fixtures. Root may bind
newly supplied already-existing executable evidence if it proves these paths.

### B-3 — REGRESSION: the affected served Operator contract test necessarily fails

Authority: AGENTS Testing (never weaken/circumvent a gate),
agent_docs/agent-surfaces.md (prove changed OpenAPI against served document),
TASK-003 HTTP/OpenAPI scope.

Location: `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:2110–2147`.
Starting condition: candidate server is composed normally. Trigger: run
`operator_connection_contract_publishes_five_typed_operations`. Expected:
the served-contract check passes and precisely describes all supported
Operator operations. Actual: its BTreeSet equality still contains only the
five original operations, while `operator_connections_router` now mounts
POST `/{connection_id}/renew-encryption` as a sixth operation. Enumeration
collects every method below the prefix, so equality fails by construction.
The task's check summary does not name this mandatory affected test.

Smallest correction: update this existing exact-set contract test for the new
sixth operation and check its bodyless POST, typed redacted response and UUID
path contract. Preserve exact-set coverage rather than relax the assertion.
Run its exact nextest selector under managed Postgres. Root is executing the
selector separately; a runtime addendum can corroborate this source result.

### B-4 — INCORRECT: renewal reference contradicts supported owner behavior

Authority: REQ-012 requires documentation of actual action/effects;
TASK-003 requires documenting owner modes and effects. Location:
`docs/src/content/docs/reference/stored-secret-encryption.md:120,137` and its
generated `docs/public/llms-full.txt` projection.

Starting condition: admin renews a disabled Operator connection or an inactive
human connection and follows the common renewal error table. Expected: the
reference accurately identifies the response and preserved state. Actual:
table says any disabled/revoked resource returns 409; Operator explicitly
supports successful renewal while staying disabled (route test :294–307), and
human inactive/tombstoned lookup returns 404 (:1060–1068), not 409. The prose
also says the view does not change, but Operator/Gateway/human views expose
`updated_at` and their renewal SQL updates that timestamp. The semantic fields,
revision, credentials and sessions are the invariant, not byte-identical views.

Consequence: an agent using the reference misclassifies successful supported
disabled renewal or retries an unavailable inactive resource, and cannot rely
on the claimed response contract. Smallest correction: scope the errors per
owner/condition, say which semantic fields remain unchanged and permit mutation
timestamps, regenerate the existing docs projection and run docs checks. Keep
disabled Operator renewal behavior; do not change lifecycle to fit prose.

## Evidence and process limits

Implementation records report server owner suites, 13 focused integration
cases, CLI 7/7 and checks green, but contain no detailed execution output or
nextest run identifiers. Root is rerunning bounded owner/integration/OpenAPI
tests; this report does not claim execution it did not perform. I ran only
source/read searches and `git diff --check a5b1d5417...c93287b54` (clean).

`TransitFixture` is a wiremock Transit protocol endpoint, not a live Vault
daemon (`wyrd-testing/src/kms.rs:16,27–44`). It tests production HTTP adapters,
context and remote failure/race boundaries with real Postgres, but owner tests
must not be described as live Vault integration. Earlier facade/provider
acceptance may be carried forward as applicable; I do not require every owner
journey to provision live cloud KMS/Vault.

TDD: **VIOLATED**, as accurately recorded in TASK-003. Gateway/human/issuer
RED was compile-only, Gateway CLI production preceded its behavioral RED, and
issuer/platform outage/race tests first ran after implementation. Fresh GREEN
cannot establish historical test-first execution. Preserve this separately;
do not demand code churn or fabricated RED to repair history.

Provisional recommendation: **FIX_REQUIRED**, pending independent validation
and the separate domain report. B-1 through B-4 are proposals, not stable
retained finding IDs. No implementation, commit, push, merge, or deployment was
performed. Broad gates, live provider tests and benchmarks were not rerun.
