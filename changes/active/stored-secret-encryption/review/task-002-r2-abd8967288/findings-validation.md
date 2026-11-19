# TASK-002-R2 independent validation

Recommendation: **PASS**. The explicitly requested shared Google credential
resolution is implemented with credible source and runtime proof. No new blocking
finding or unresolved acceptance obligation is retained. Original
FIND-TASK-002-1 through FIND-TASK-002-4 remain CLOSED.

## Immutable subject and review scope

Prior accepted commit / HEAD `52f69ed2da3846733be0d314c6627c95d308e8dd`;
candidate staged tree `abd896728870d606f3f79bf18b7be8b747208982`;
cumulative base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
The user explicitly supplied staged, uncommitted work for review. This review
uses that immutable tree rather than creating a commit; subject.md records the
exception to the normal commit-based subject and hashes the untracked task/prior
review evidence. Source and index match that tree; no candidate source was edited.

This fresh remediation validator read the actual prior-to-tree diff, approved
SPEC-stored-secret-encryption revision 1, original TASK-002, R2 task and evidence,
prior R1 verdict/validation, root and docs AGENTS, spec-driven workflow, TESTING,
applicable Rust/errors/maintainer guidance, security stored-secret authority and
deployment configuration authority. It traced shared resolver, both production
callers, provider credential ownership, local fixtures/assertions, dependencies,
setup reference, generated LLM projection and the runner description.
No build/test command was executed by this reviewer; fresh root execution is
separately bound to the same tree in verification.md.

## Reconciled acceptance matrix

The existing R2 scenarios implement the maintainer-requested scope under
REQ-005 and REQ-012. They are not a newly discovered FIND-5.

| Existing requirement / R2 scenario | Source and inspected assertions / current execution | Result |
| --- | --- | --- |
| REQ-005 / scenario 1: both existing inline sources reach both consumers | `wyrd-utils/src/secret.rs:76` owns the moved resolver; `wyrd-kms/src/gcp.rs:50–54` and `wyrd-storage/src/cloud.rs:94–97` both apply its result to the existing provider builder. KMS tests use production WyrdKms::new, seal/open and assert each request's bearer issuer; storage uses production build_signer and observes its real boot-probe bearer issuer. Each input alone is tested with ambient Google sources and HOME isolated. Root exact KMS/helper 5/5 and storage 2/2 pass. | PASS |
| REQ-005 / scenario 2: precedence, existing fallback and redaction | Shared tests cover each input, both, neither, invalid base64 and decoded non-UTF-8. Actual KMS and storage constructor tests select the base64 identity when both are set. KMS's no-inline case loads GOOGLE_APPLICATION_CREDENTIALS and authenticates encrypt/decrypt as the file identity. Unusable inline keys produce redacted constructor errors in both callers. Storage's unchanged no-endpoint/provider-chain branch plus the verbatim resolver move preserves its fallback; the local endpoint intentionally disables signing without an inline key and is not claimed to prove file authentication. | PASS |
| REQ-012 / scenario 3: setup and projections | `docs/src/content/docs/reference/stored-secret-encryption.md` authentication row and GCP setup name both variables, shared identity, separate KMS grant, base64 precedence and malformed-input rule, while retaining workload/file routes. llms-full.txt carries the identical generated changes; mise cloud runner description names the inputs. Recorded docs:generate/docs:check evidence supports projection parity; docs checks were not freshly rerun here. | PASS |

The full R2 task remains bounded to credential construction. It does not require
a new public API, SDK surface, authentication flow or full user-journey aggregate.
Production constructor integration proof, helper unit proof and content/projection
review are the task's stated tiers. Live Cloud KMS acceptance stays in its existing
credentialed CI boundary, as explicitly agreed by the maintainer.

## Implementation and preserved behavior

The old storage resolver is deleted and its body moved verbatim into the existing
shared secret module. Both production consumers call that one implementation;
there is no duplicate parser or new settings alias. A readable base64 variable is
considered first. Invalid base64 or decoded non-UTF-8 returns None without trying
the raw JSON variable. Otherwise the raw JSON variable is considered when the
base64 variable is absent/unreadable. None leaves the existing provider builder
chain intact. R2 preserves this agreed behavior rather than introducing a new
validation/fallback policy.

KMS applies resolved JSON before building its reusable client/credential provider.
It still uses the provider's existing service-account parsing and refreshing
credential provider. The later call method is unchanged and obtains credentials
through that retained provider. Storage changes only the resolver call target;
its endpoint/signature handling and normal non-emulator builder remain unchanged.
The returned credential identity is selected at construction rather than parsed
again during a caller's operation. No new cache, secret owner or provider API is
introduced.

The same provider library already parses these JSON keys, so no new credential
format or signing implementation is part of production. Tests generate usable RSA
keys through the installed AWS-LC implementation and inspect the selected issuer
from actual bearer headers. They do not inject a prebuilt credential provider to
bypass construction. Those tests establish input selection and local protocol use;
they do not assert that a real Google account/key/IAM grant has been provisioned.
That separate real-provider limit is disclosed, not hidden by the mocks.

## Dependencies, fixture scope and repository rules

The new storage-to-utils edge reuses a shared shell already consumed by KMS; it
does not create KMS-to-storage, SQL or data-plane dependencies. Utils gains only
an already installed base64 dependency plus a test-only environment fixture edge.
KMS's new wyrd-dev-fixtures edge and storage's wiremock edge are dev-dependencies.
The shared Google fixture has a coherent test-only responsibility, generates a
fresh private key in process and commits no key material. The direct AWS-LC entry
reuses the crypto implementation already present through object_store/rustls;
Cargo.lock adds edges, not another crypto package/version or provider SDK.
Generated workspace-hack changes track that feature union.

Fresh check:deps (including shipped-source constraints) and check:workspace-hack
passed without modifying the tree. Their result supports the source trace; the
scanner alone is not treated as proof of semantic credential parity. Existing
server-tier composition, tenant authorization, common envelope/key selection,
redaction owners and previous secret lifecycle fixes are untouched. No schema,
public wire contract, auth session rule or platform-specific branch was added.
New/changed items have role-specific rustdoc; the RSA fixture documents its named
test-environment panic invariants. No unjustified production abstraction or new
runtime was found.

## Carry-forward and reviewer applicability

R1 closed FIND-TASK-002-1–4 with auth, Operator and Gateway consumer proof.
This diff does not change their binding, CAS, rewrap-progress or pending-Gateway
owners. The shared helper addition leaves existing utils secret-file routines
unchanged and moves only the established storage parser. Therefore no specific
new cause invalidates their accepted evidence. REQ-001–004, REQ-008–009 and the
unaffected portions of REQ-005/012 carry forward from the R1 verdict rather than
triggering another broad discovery round.

One fresh remediation validator is sufficient for this narrow shared helper and
two existing startup consumers. Credential selection/security, dependency edges,
proof strength and documentation are explicitly covered above. New key generation
is confined to the test fixture and existing crypto library; it does not create a
new production custody or protocol boundary needing another specialist. No
unassigned material lifecycle/SQL/recovery boundary or new proposed finding was
found, so no additional reviewer is needed merely for another vote.

## Wait inventory and process history

R2 adds or moves no PostgreSQL, Scribe, object-storage, Vault or KMS wait on the
defined hot paths. It adds local environment/base64 resolution at the existing
KMS client-construction point; storage already performed that same work. Existing
storage boot probe and later provider operations are unchanged. No per-request
client/pool/runtime construction or lock across an await is introduced. The
previous sanctioned Gateway/background preparation inventory remains valid.
No benchmark is required absent the current explicit request withdrawn by the
maintainer's root AGENTS/spec rules.

R2's implementation record supplies expected behavioral RED before production
changes: KMS ignored both inputs and failed through credential fallback with
Unavailable/GcpKms/Credential. That failure corresponds to the source omission,
not a setup/compilation RED. Recorded TDD for applicable new KMS behavior is PASS;
current GREEN does not recreate historical RED. Storage and shared resolver tests
pin already-correct moved behavior and are verification-only; documentation is
not a runtime TDD cycle. Original TASK-002 scenarios 2/3 remain TDD VIOLATED and
R1's separately recorded history remains untouched.

## Verification limits and final disposition

Root's exact focused tests pass 7/7: shared resolver/KMS 5/5, nextest
`846fee67-6b77-46f2-9d01-16f7f6178032`; production storage signer 2/2, nextest
`a1a56e0c-8399-46a3-8a0c-e57eaa3cf9a5`. Fresh dependency and workspace-hack gates
also pass. Historical owner suites, fmt/lints and docs results remain explicitly
recorded implementation evidence; no fresh broad aggregate or docs generation
is claimed. Live AWS/GCP and Linux startup retain the accepted CI limits. Storage
file fallback is supported by unchanged production source and KMS builder proof,
not misrepresented as observed through the unsigned local emulator.

**PASS** is recommended for R2 and TASK-002's remaining assigned addition. No
retained blocking finding, new remediation task, or specification revision is
required. Subject hashes/tree remain the review identity; a later staged/source
change requires a new immutable subject rather than silently carrying this verdict.
