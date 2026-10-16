# Security, tenant, and evidence authority review

Result: **PASS**. No material proposed findings in this domain.

## Immutable boundary

Reviewed base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate `dddf985563029a004fa768110e36d0c5f12bcded`, using prior candidate `07adb7907951f343544e550f6cabc5b7aa3aadea` to locate remediation. HEAD was independently checked and matches. Scope is TASK-004 client/SDK projection, evidence transfer, reconnect authority, and authored hosted-tool permission/delegation journeys. Server authorization, hosted dispatch, and runtime integration remain the other tasks' responsibilities. Human decisions in subject.md are authoritative; code.run arguments are provisional and not a finding.

## Authority and source coverage

| Boundary | Authority | Source inspected and conclusion |
|---|---|---|
| Single client and original caller | AGENTS.md §§2–3, 8–9; agent-rules.md; wyrd-design.md client model and cross-service delegation; security-posture identity/token rules; spec INV-001–003 | `wyrd-client/src/verify/{mod.rs,execution.rs}`, `observe/mod.rs`; Python `src/{client.rs,verify.rs,observe/mod.rs}`; TS `native/src/verification.rs`, authored wrapper. Requests carry typed component/run/input identity, without trusted tenant/principal fields. Handles retain the supplied native client; no client-side permission decision or replacement auth transport introduced. |
| Evidence upload credentials and tenant authority | Security-posture trust boundaries; REQ-010–011; INV-003 | `RunEvidence::pin`, `File::pin`, `WyrdStorageClient::upload_evidence`, upload dispatch, `single_put.rs`, `local_fs.rs`, shared `transport/http.rs`. Evidence planning is authenticated through the retained client. SinglePut reuses the existing external stream transport without Wyrd bearer headers; LocalFs uses the existing authenticated origin-constrained transport. Filename selects no storage path. Digest/existing Artifact authorization remains server-owned; client does not treat digest possession as permission. |
| Tool scope and domain grants | Spec REQ-008; INV-003–004; architecture agent-harness reference | Contract `auth/permission_scope.rs` and runtime `permission.rs` preserve the existing typed exact `Tool(BuiltinTool)` scope, coverage, and resource/action validation. SDK principal handles continue forwarding existing typed owner requests; no parallel tool policy added. The new fixtures/journeys use the existing role/principal and RFC 8693 delegation mechanisms. |
| Hosted-tool denial and delegation | TASK-004 Scenario 3 and R1 FIND-004-5; REQ-008 | Rust `verifier_building_blocks.rs` helpers, cases and upstream script; Python and TS corresponding files; shared principal seed helpers; `tool-review.yaml` and `investigator.yaml`. See closure matrix below. |
| Cross-tenant status isolation | REQ-012; INV-003 | All three `verify_in_real_time` journey families reconstruct a handle using another tenant's client and require wait timeout rather than a leaked result. Shared `Execution::status` uses the caller's authenticated transport, never tenant data from the execution ID. |
| Documentation contract | REQ-008; INV-001,003,004 | `reference/agent-tools-and-authorization.md` publishes global/exact tool wire grants, independent domain authorization, accepted original subject/delegation, no SYSTEM privilege, credential exclusion, and non-blocking audit. Runtime enforcement is explicitly a later integrated proof obligation. |

The navigation started with CodeGraph and expanded to shared transfer/auth owners, language wrappers, fixture producers, seed helpers, and the three real SDK journey consumers. No discovery peer report was read.

## R1 closure in this domain

| Prior obligation | Source evidence | Assessment |
|---|---|---|
| FIND-004-5: hosted Card/Bifrost/code calls | Rust `hosted_tools_need_the_tool_and_domain_grants` (1116), Python `test_hosted_tools_need_the_tool_and_domain_grants` (476), TS matching test (579); three tools actually requested by the provider script; registered fixture declares all three | Closed at the approved authored-journey level. These exercise hosted verifier Agent dispatch, rather than substituting a direct Bifrost call. |
| FIND-004-5: two independent gates and exact scope | Cases allow all three, deny all with only domain grants, allow only code with only invocation grants, and allow only Card read with exact `cards.get_by_ref` invocation scope. All languages inspect recorded calls, stable denial code, absent output digest and zero returned bytes on denial; the provider's follow-up model request must omit Card contents when Card read is denied. | Closed at authored level. No SDK authorization shortcut is used. |
| FIND-004-5: delegation attenuation and attribution | Rust `delegated_tools_keep_the_narrower_authority` (1157), Python equivalent (492), TS equivalent (596): broad subject, Card-only delegate, ordinary `on_behalf_of` exchange; Card allowed, query/code denied. Published result readback uses execution_id and checks exact subject and actor principal IDs. | Closed at authored level; grant intersection and protected downstream access will be executed after TASK-005. |
| FIND-004-2: retained authority on Python lazy/eager/queued handles | `PyExecution::queued` uses `LazyConnection::ready`; constructor retains optional supplied client through `LazyConnection::new`; `handle` calls shared `Execution::connect_with` or ambient `connect`; eager `connect` initializes that same path. Python focused tests distinguish offline construction, failed exchange retry, eager exchange and handle reuse. | Closed for security/client ownership. The new ready constructor cannot accidentally resolve ambient credentials because its OnceLock is already initialized. |
| FIND-004-6: result attribution correlation | Three-language delegated readers select `sub_principal_id, act_principal_id` where `execution_id` equals returned execution identity, preserving separate result identity. | Closed for this domain. |

Other prior findings concern behavior, typing, fixtures, documentation, or approved scope and are assessed by their assigned reviewers; this report does not claim their full closure.

## Verification limits

This is an independent source audit. No Cargo invocation, full journey, gate, external server, environment-variable dump, or production mutation was performed. The orchestrator’s independent `verification.md` records passing Rust integration compile/discovery, scoped four-crate Clippy, Python/TS typechecks and journey discovery, and the exact fixture contract test. Three selected shared-client mock tests are blocked before assertions by sandbox port binding. Earlier implementation green claims are not treated as independent execution evidence here. The authored test paths and assertions have been traced, but this PASS does not certify the not-yet-integrated server tool authorizer, audit emission, delegation persistence through restart, or sandbox isolation. Those remain merged-change proof under the explicit human-approved deferral, not missing TASK-004 execution.

Findings: none. No speculative hardening or scope expansion requested.
