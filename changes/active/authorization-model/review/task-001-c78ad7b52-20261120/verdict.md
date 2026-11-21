# TASK-001: FIX_REQUIRED

Scoped authorization is not ready for acceptance. Independent validation retains ten blocking findings: eight implementation, consumer or mandatory-rule defects, one missing required MCP projection, and one live-peer evidence gap. The full findings and their dispositions are recorded once in findings-validation.md. The correction packet is TASK-001-R1-scoped-authorization-corrections.md.

## Immutable subject

| Identity | Value |
| --- | --- |
| Repository | /home/thorrester/Documents/GitHub/wyrd |
| Branch | task/authorization-model/TASK-001 |
| Base | eeb3bbd4f891ae88b00ce1979810eb2a11d04871 |
| Candidate | c78ad7b5241fbbfb54e85e2390c6a694787debfc |
| Implementation range | 8e33913ed^..c78ad7b52, 38 commits |
| Inspected diff | git diff eeb3bbd4f891ae88b00ce1979810eb2a11d04871...c78ad7b5241fbbfb54e85e2390c6a694787debfc |
| Approved contract | SPEC-authorization-model revision 2, changes/active/authorization-model/spec.md |
| Spec commit | 38ac3ca8a0c365825124aa3e99a36ba19888e26f |
| Original task | changes/active/authorization-model/tasks/TASK-001-scoped-authorization.md |
| Worktree HEAD | 3139c08cb0b7c0194583a66af3ab31bef28dc14f |

Later 38ac3ca8a..3139c08cb authority/design/specification changes are governing context supplied for review, not implementation under review. Implementation paths at HEAD match the candidate. Commit identities and tracked source remained unchanged. Only this new review directory was written. This is a first review, with no prior remediation or convergence history.

## Authority and independent coverage

Authority: root and SQL/Python/TypeScript/docs AGENTS.md; changes/spec-driven-development.md; agent_docs/maintainer-style.md and applicable Rust/error/agent-surface guides; TESTING.md; architecture/README.md; architecture/wyrd-security-posture.md credential, authorization, tenant-binding and retained-data sections; architecture/wyrd-design.md Runtime identity and Repository; architecture/bifrost-design.md Public surface. The approved spec supplies the assigned backend behavior; TASK-002 retains new UI pages and access-request presentation.

| Assignment | Concrete boundary | Report |
| --- | --- | --- |
| behavior-rev | Complete assigned requirements and scenario coverage; local device/invitation/account flows; remaining rules, ownership and maintainability | task-review-behavior.md |
| domain-rev | Permission/ceiling security, trusted retained parents, scoped Card consumers, SQL masking and peer representation | task-review-domain.md |
| invariant-rev | Cache concurrency, revision writers, freshness, production lifecycle, failure/recovery, streams and hot-path waits | task-review-invariants.md |
| repo-rev | Cross-tier contracts/projections, MCP administration, SQL capabilities/RLS/migration scope, affected docs and rustdoc | task-review-repo.md |
| ponytail-rev | Independent coverage reconciliation and source validation of every discovery proposal | findings-validation.md |
| Focused independent check | Validator-originated peer/published proof concern, narrowed to live-peer evidence only | peer-proof-validation.md |

No separate system-rev was needed because failure propagation, production composition and recovery were explicitly assigned to invariant-rev. No separate maintainer-rev was needed because primary/repository coverage included ownership, reuse and changed-item maintainability. All selected reports are present. Discovery and validation used separate contexts; orchestration did not discover or validate its own findings.

## Reconciled acceptance matrix

PASS rows establish the stated boundary only; they do not waive intersecting failed requirements. Assertions and recorded execution evidence were inspected independently. Full details and navigation are in the boundary reports.

| Requirement / scenario | Implementation locations | Assertions and execution evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001–003 | wyrd-spec auth/access.rs::compile_binding; auth/permission_scope.rs; wyrd-auth roles.rs; server Card/storage authorization | SDK principal_roles.rs:1023 group/custom/exact reads and :355 provider coexistence, recorded focused passes; uncovered target-validation and scoped-mutation cases in FIND-TASK-001-3,5 | FAIL |
| REQ-002 / AC-001 | wyrd-auth cli_logins.rs; issuance.rs human anchor handling; server auth/cli_login.rs | SDK principal_roles.rs:764 local password/device approval and password-change refresh refusal; pg_browser_sessions scoped invitation assertions; recorded identity lane exit 0. Warm refreshed-request violation is assessed under REQ-004 | PASS |
| REQ-003 / AC-002 | WorkloadRestriction/WorkloadFamily; issuance/root validation and retained attribution writers | Rust principal_roles.rs:568 and Python test_principal_roles.py:238,:351 two releases, stable account/key, actor/subject attribution, no implicit grants, foreign/malformed roots; recorded signed-root mismatch pass | PASS |
| REQ-004 / AC-003 | wyrd-auth authority.rs; SQL auth authority/revision writers; server auth worker and query stream | pg_principal_roles.rs:1147 real-replica propagation/compile counter; SDK principal_roles.rs:177 stream stop; authority PG outage assertions; review rerun 3/3 sequential unit tests. Concurrent observation fence and refreshed-human warm path fail: FIND-TASK-001-2,6 | FAIL |
| REQ-005 / AC-002–003 | Shared issuer/verifier v2 claims; EffectivePermissions; delegated issuance; AcceptedAuthority and runner | Issue/verify identity-only/refused-legacy-claim assertions; delegation intersection tests; pg_verification_runtime.rs:5901 recovery retains byte-identical authority and attribution. Current cache enforcement failures remain recorded under REQ-004 | PASS |
| REQ-006 / AC-001 | Card list/read predicates; verification retained/direct identity; approval/download owners; Oracle restrict_verification_rows and codec | Scoped SQL SDK journey principal_roles.rs:877, actual Parquet publication through flush_staged; PR membership/count/queue-cleanup assertions, scoped approval/download passes and codec unit proof. Direct deletion fails and live-peer scoped-parent journey proof is absent: FIND-TASK-001-4,10 | FAIL |
| REQ-007 / AC-001 | GitHub connection configuration permissions and retained PR readers | pg_github_connection.rs:792 configurator versus Repository/OIDC/data isolation; PR retained-read and existing provider identity/admission assertions; recorded focused HTTP/MCP results | PASS |
| REQ-008 backend / AC-001–003 | Server principals/access.rs and routes.rs; invitation owner; SDK/CLI/MCP adapters | Explicit binding admin refusal pg_principal_roles.rs:358; scoped invitation atomicity pg_browser_sessions.rs:1329; public SDK access summaries/groups/custom roles. Provisioning bypass, scoped authoring, scope validation and MCP administration fail: FIND-TASK-001-1,3,5,7 | FAIL |
| REQ-010 / AC-001–003 | Existing spec/runtime/auth/SQL/server/client owners and generated projections | Source establishes common scoped-binding store, removed token grants/old assignment callers, SQL capability/RLS compliance; recorded generation/dependency/tenant checks. Required consumer guidance remains stale and MCP projection incomplete: FIND-TASK-001-7,8 | FAIL |
| Mandatory changed-item rustdoc | wyrd-auth roles.rs; SQL auth/authority.rs and role_bindings.rs | Exact new/changed declarations and base-to-candidate documentation removal; FIND-TASK-001-9 | FAIL |

REQ-009 presentation belongs to TASK-002. Its applicable indistinguishability boundary was considered with the assigned retained-read and execution-admission behavior; no new access-request subsystem is authorized.

## Retained finding references

FIND-TASK-001-1 through FIND-TASK-001-10 are retained as specified in findings-validation.md. Discovery proposals BEH-001/002, INV-001/002, DOMAIN-01/02 and REPO-01/02/03 were independently confirmed or revised. The additional evidence concern received a separate focused source check before retention. Missing published-Parquet proof was rejected; only real live-peer scoped-parent proof remains in FIND-TASK-001-10. No rejected proposal generates remediation work.

No approved-behavior ambiguity or material boundary revision is needed for these corrections. The result is FIX_REQUIRED, rather than SPEC_REVISION_REQUIRED or blanket BLOCKED.

## Hot-path waits

| Added or moved wait | Owner and sanctioned boundary | Verdict implication |
| --- | --- | --- |
| Bearer authority acquisition on miss, invalidation or expired confirmation | AuthorityCache::reload, one permission-state reload | Sanctioned acquisition; FIND-TASK-001-2 concerns preserving observed state around it |
| Potentially newer account/key miss | AuthorityCache::authorize → reload, permission-state cache miss | Genuine miss is sanctioned; repeated confirmed refreshed-human reload is FIND-TASK-001-6 |
| Authority revalidation before subsequent query frames | StreamAuthority::revalidate → acquire; memory when usable, same sanctioned reload when invalid/expired | Adds no warm storage wait; affected by FIND-TASK-001-2 |
| Revision confirmation and changed-state load | Existing background control-plane worker | Outside the caller's hot-path wait; must still preserve invalidation |
| Permission audit | Existing process Scribe outbox staging | No added durability wait; remains nonblocking |

No added/moved object-store or Vault wait, or audit acknowledgement wait, was identified in the reviewed authorization changes. Existing unrelated execution, query and append waits are not newly introduced by this range. Source and wait findings are detailed in task-review-invariants.md.

## Verification limits and process exclusions

Three exact cache freshness/stream unit tests ran during review and passed, as recorded with their command and run identity in verification.md. Focused PostgreSQL setup failed before migration/tests because Docker socket access was denied. Broad suites and live exploits were not performed; task-recorded executions remain qualified evidence at their recorded commits. Post-a28f2889d additions are test/evidence changes with their own recorded focused pass, not a reason to discard unchanged production evidence wholesale.

Behavioral verification: FIX_REQUIRED. TDD compliance reporting is excluded by explicit maintainer instruction and has no effect on this verdict. Implementation, commit, push and Git configuration changes were not performed. The caller receives the remediation packet; this review launches no implementation or further task review.
