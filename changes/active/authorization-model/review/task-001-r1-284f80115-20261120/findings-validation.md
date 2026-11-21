# TASK-001 R1 independent closure validation

## Subject and method

Candidate `284f801159a391e92493c3072878fd644c9d173b`; remediation base `3139c08cb0b7c0194583a66af3ab31bef28dc14f`; prior candidate `c78ad7b5241fbbfb54e85e2390c6a694787debfc`; cumulative base `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`. Approved contract: SPEC-authorization-model revision 2, original TASK-001 and prior TASK-001-R1. See subject.md for identity and navigation details.

The fresh remediation reviewer inspected the latest diff, requirements, prior validated evidence, assertions and affected callers. CodeGraph preceded source tracing. Governing repository, documentation, SQL and language instructions, workflow, maintainer guidance, TESTING.md and applicable architecture were read. Unchanged source-backed acceptance evidence carries forward; this is not a new audit of all unchanged implementation. User-reported open notes are claims, not additional requirements.

Separate fresh source validation independently checked the cache observation finding in focused-validation.md and consumer/documentation/fixture/proof proposals in consumer-validation.md. Their evidence is reconciled below. These selected boundaries cover shared-state failure topology, security, actual projections and mandatory local rules. No further broad specialist vote is needed for unchanged areas already covered in the prior review. New findings were not retained solely on this reviewer's own discovery.

## Prior finding closure

| Finding | Disposition | Source and assertion evidence |
| --- | --- | --- |
| FIND-TASK-001-1 | CLOSED | principals/routes.rs:313 checks wildcard binding administration before mutation for any supplied assignments. pg_principal_roles::provisioning_a_services_roles_requires_tenant_administration asserts admin/custom escalation refusal, absent account/credential, audit refusal, permitted empty assignments and authorized provisioning. Recorded server closure results carry forward. |
| FIND-TASK-001-2 | OPEN | Numeric observation protects existing entries, but observations before first install disappear and unknown invalidation can be overwritten by an older in-flight load. Independent focused validation confirms the same original lifecycle defect. |
| FIND-TASK-001-3 | CLOSED, production correction | cards/service.rs::authorize_declared_families covers every submitted family and inferred Repository identity; cards/routes.rs::authorize_card_uid and delete_scope resolve target scope; storage reuses it; deletion checks cards:delete. New SDK assertions exercise real uploads/completion, covered family/exact deletion and unrelated/mixed-graph refusal without persisted members. Test-fixture violations are separately FIND-13. |
| FIND-TASK-001-4 | CLOSED | inspection.rs:705 changes to tombstone-inclusive get_card_for_reconciliation. New direct execution journey asserts same retained result/details after subject and Verifier deletion, family success, unrelated not-found. Public Card reads and attribution remain unchanged. |
| FIND-TASK-001-5 | CLOSED | roles.rs::validate_assignments resolves exact Card UID/family, table UID/catalog/schema and tenant GitHub connection through existing tenant-bound owners. bind_assignments reuses it, as do invitation issue/consumption before atomic commit. Tests assert unknown/foreign/wrong-family refusals, no account/binding persistence, and live/deleted/future-family success. No new persistence mechanism. |
| FIND-TASK-001-6 | CLOSED | authority.rs:406-459 settles confirmed missing token IDs after a sanctioned reload. Real renewed saved-login journey makes 20 reads without recompilation; deterministic same-second unit distinguishes settled refresh provenance from a new credential. Fresh unit and Postgres outage component tests pass. Key ceilings/new-key acquisition remain. |
| FIND-TASK-001-7 | CLOSED | Twelve typed nonsecret MCP tools delegate to the existing HTTP access handlers. Reads stay discoverable; mutations require wildcard administration. New MCP journey asserts roles/groups/membership/binding/source/access/removal and directly named non-admin refusals. Recorded 52/52 result carries forward. |
| FIND-TASK-001-8 | OPEN | Most removed API, propagation and parent guidance fixed; add-people still denies passwords/invitations and directs no-IdP humans to Service keys. Newly replaced SDK snippets violate required tested-source embedding. |
| FIND-TASK-001-9 | CLOSED | RoleAdminError intent restored; authority/binding/membership/revision SQL constants and decode_source have item intent/error rustdoc. Specific original omissions corrected; recorded format/lint evidence carries forward. |
| FIND-TASK-001-10 | CLOSED | distributed.rs:2057 public journey produces trusted verification rows, forces Analytical queries through real peer graph leases and peer body polling, and checks permitted identities/counts plus hidden-parent empty/count-zero results. Exact reader is another family; R1 asked actual peer parent-boundary proof. Prior exact-version discrimination plus this real peer composition suffices; a second same-family version is not a new mandatory peer variant. |

## Retained findings

### FIND-TASK-001-2 — Cold/in-flight authority loads can lose observed invalidation

Disposition CONFIRMED, incomplete correction; classification INCORRECT. Authority: REQ-004 and security posture Current authority and freshness prohibit allows from known-obsolete state.

Locations: wyrd-auth/src/authority.rs:553-568 load/commit/install, :611-633 installation, :638-641 observation, :646-662 notification. Production listener: server components/auth/state.rs:111-134; bearer consumer: token_extract.rs:176-187. SQL authority.rs:18-50 reads grants/revision together; role_bindings.rs:68-81 notifies on commit.

Starting state: a running replica has no entry for a tenant. Its first request reads revision N and old grants; before installation, removal commits N+1 and the independent listener receives it. observe only calls get_mut, so N+1 is discarded. Vacant installation initializes observed=N and returns obsolete grants usable. A malformed notification similarly fences only present entries and a previously started load can replace/re-confirm them. Later background repair does not satisfy immediate known-invalidation refusal.

The new passing warm-entry test installs N before notifying; it does not cover this race. This is independently source-proven through the production SQL/await/listener/consumer path, not a newly executed exploit.

Smallest correction: preserve observations independently of installed snapshot existence and prevent loads begun before unknown invalidation from clearing it, within AuthorityCache. Deterministic cold N/load, N+1/observe, N/install refusal, N+1/install success; separately malformed invalidation during load without a higher numeric fence. Preserve memory-only warm checks, sanctioned acquisition, short locks and the five-minute unobserved outage bound. No schema or new authorization mechanism. Full independent trace: focused-validation.md.

### FIND-TASK-001-8 — Affected human onboarding and copied examples remain inconsistent

Disposition CONFIRMED incomplete correction; classification REGRESSION and mandatory documentation violation. Authority: REQ-002/008 local passwords/invitations without an IdP; REQ-010 affected consumers; docs/AGENTS.md Examples requires copyable SDK code from tested CodeFromFile sources.

Locations: docs/src/content/docs/operate/add-people.md:9,:184-192 and grant-and-inspect-permissions.md:46,60,78. A maintainer following no-IdP human onboarding is told Wyrd has no passwords/invite step and to create a Service account/key lacking browser sign-in. Newly replaced scoped SDK snippets remain hand-authored inline code rather than tested-source embeds.

Consequence: supported local human workflow is misrepresented and copied SDK guidance bypasses the mandatory executable-example boundary. Smallest correction: repair affected paragraphs to actual local User/invitation/password/device flows while preserving optional provider setup; embed tested source or remove unnecessary duplicate SDK snippets while retaining usable tested guidance. Applicable documentation/example checks and source review suffice; no production redesign. Independently confirmed in consumer-validation.md.

### FIND-TASK-001-11 — Python and TypeScript approval journeys still expect approver-only success

Disposition CONFIRMED, missed original consumer defect; classification REGRESSION. Authority: REQ-006/008 approver plus readable retained parent, REQ-010 update affected fixtures/consumers; AGENTS.md shipped language journey requirement.

Locations: sdks/wyrd-sdk-python/tests/integration/test_deferred_evidence.py:108-111,152-159; sdks/wyrd-sdk-ts/wyrd/tests/integration/deferred-evidence.test.ts corresponding clientFor/approver calls. Both bootstrap only the approver role and expect approval of a Repository-parent execution to succeed. The role intentionally provides no implicit different-parent read. Production correctly refuses this caller; these language-specific supported journeys cannot pass their positive assertions under the approved model.

Smallest correction: update those existing journeys through public Principals bindings so the successful approver has the relevant parent read. Assert approver-only refusal, then permitted approval/replay with explicit parent authority, retaining viewer/editor refusal and original decision attribution. Run each exact Python/TypeScript journey under existing repository setup. Do not broaden the approver role or weaken the production check. Independently source-confirmed in consumer-validation.md; no fresh language test result is claimed here.

### FIND-TASK-001-12 — CLI scoped access administration lacks public journey proof

Disposition CONFIRMED evidence gap; classification EVIDENCE_GAP. Authority: TASK-001 AC-001/003 programmatic HTTP/CLI grant administration, REQ-008/010 shared CLI contracts, AGENTS.md journey proof on shipped surfaces.

Location: new CLI principal/access.rs dispatch and existing crates/wyrd/wyrd-cli/tests/principal_journey.rs/card_lifecycle.rs consumers, traced in consumer-validation.md. Current CLI tests prove lifecycle/revocation/scoped reads; administration in those fixtures occurs through SDK/helpers. None drives the new access show/grant/revoke CLI path. Passing Rust/HTTP administration tests do not establish argument parsing, subject/scope projection, binding selection and returned CLI behavior.

Smallest correction: one focused existing CLI journey using public in-process CLI functions against WyrdTestServer/Postgres: admin grants one family, show exposes the binding/source/scope, current client can read that family, revoke the returned binding removes that access after normal propagation, and non-admin mutation refuses without a binding. Existing authority propagation tests can supply timing-bound proof; no duplicate broad suite or new CLI feature. Identify/run the exact selector after adding proof. No production defect is inferred merely from missing evidence. Independently confirmed in consumer-validation.md.

### FIND-TASK-001-13 — New public journeys rewrite Card YAML at runtime

Disposition CONFIRMED fix-introduced mandatory test-rule violation; classification VIOLATION. Authority: TESTING.md client-facing tests requires checked-in YAML from fixtures/cards and forbids building/editing YAML, plus AGENTS.md tests follow local rules.

Locations: sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:1320 onward reads support-model YAML, replaces version and writes temporary YAML; crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:2087 onward copies/replaces quickstart YAML to make another Service family. These additions construct the journey's Card inputs through string rewriting instead of the required checked-in authoring fixtures.

Smallest correction: use checked-in fixture Cards for the second release and other family, sharing unchanged referenced fixtures normally; keep public declaration and all existing positive/negative/peer assertions. Rerun only the two affected exact journey selectors. No production changes, new fixture framework or weakened assertion. Independently confirmed in consumer-validation.md.

## Reconciled acceptance matrix

The single reconciled requirement matrix is recorded in [verdict.md](verdict.md#reconciled-acceptance-matrix). Prior finding closure above preserves the supporting evidence without duplicating that matrix.

## Wait inventory

Numeric observation fencing adds no wait. Acquire/reload remains the sanctioned permission-state load on miss/invalidation; streams use that same owner. Settlement removes repeated warm reloads. Warm checks use process memory and short mutexes without a lock across await. Exact Card write/upload/delete scope resolution adds one registry statement on non-hot mutation routes. Card/table/GitHub target validation adds tenant-bound lookups only in administration/invitation writes; MCP reuses those handlers. Direct retained identity changes an existing lookup to tombstone-inclusive SQL without adding a statement. No Scribe/object/Vault wait is added, and audit remains staged nonblocking. The remaining cache finding calls for lifecycle fencing, not warm SQL.

## Executed verification and limits

- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=authority::tests::an_older_load_never_serves_after_a_newer_revision_is_observed) | test(=authority::tests::a_settled_token_stops_reloading_in_its_issuance_second)'`: exit 0, 2/2 pass; run `7e033c33-e66f-4ede-a9a8-c19d15c287db`, test time 0.004s after 5.30s build. This proves the listed sequential warm cases, not the retained cold/malformed race.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=authority::pg_tests::a_settled_token_needs_no_reload_during_an_outage)'`: exit 0, 1/1 pass; run `7506dbef-e504-4886-80ce-9c7ded88d605`, test time 1.212s. Repository-managed Postgres was available. The test asserts stable memory refusal of unknown identity after the database closes; it does not itself prove successful human bearer reads. The successful local refresh journey and deterministic unit supply that separate component evidence for FIND-6.

Recorded closure results/checks were inspected against source and assertions; they are not fresh reruns of all journeys. No broad suites, excluded qualification runs, implementation edits, commits or pushes. TDD compliance reporting remains excluded by explicit maintainer instruction. Final HEAD remained `284f801159a391e92493c3072878fd644c9d173b`.

Recommendation: FIX_REQUIRED. Eight original findings close; FIND-2 and FIND-8 remain incomplete. New FIND-11/12 are missed original consumer/proof obligations; FIND-13 is introduced by correction tests. No specification revision is needed. This is the first remediation round, so the two-unsuccessful-round convergence threshold has not been reached.

Skipped: broad reruns and excluded process/qualification assessment. Risk: known-obsolete authority can still allow in the demonstrated cold/in-flight race until corrected.
