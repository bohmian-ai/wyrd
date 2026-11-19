# TASK-002 security domain review

Result: **FAIL** — one proposed implementation finding (SEC-001), requiring independent validation. The task also needs the authority clarification below; that is not a request to implement live grant refresh.

## Immutable subject and coverage

Reviewed `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f...75c9bfa9e07163099eac40d5a556c4447c6a519d`; HEAD remained `75c9bfa9e07163099eac40d5a556c4447c6a519d`. Sources were not changed. Reviewed the original TASK-002, approved revision-18 obligations and publication/durability supplements; draft wait/approval additions are excluded.

Read the task-review skill, supplied repository rules, architecture authority map, applicable security/design sections, workflow/testing references, maintainer and agent-surface/error guides, and SDK/docs directory instructions. Used `mise exec -- codegraph explore` before source navigation. No subagents were used.

| Boundary | Sources inspected | Assessment |
| --- | --- | --- |
| Tool selection and closed dispatch | `wyrd-spec/card/agent.rs`, `card/verifier/validate.rs:549-591`; `hosted/tools.rs:130-191,222-331,342-370,591-664`; `vala-verify/blocks/tools.rs:136-172`; loop and trait diff | Catalog identities remain closed; provider spellings are derived; selection cannot create an additional tool or choose a principal. Checks cannot publish. Scratch reading is confined to the same workspace. |
| Accepted caller and permission checks | `hosted/blocks.rs` Execution/composition, `hosted/tools.rs:233-242`; `audit/mod.rs:121-170`; security posture:145-157,274-284 | Tool authorization receives the execution's accepted principal; actor/delegation attribution supplies no additional grant. Every call evaluates its exact tool permission. |
| File domain and tenant confinement | `hosted/publications.rs:129-183,195-232`; storage `service.rs` StorageCaller/file_upload_init; storage `routes.rs:644-666`; `components/verification/service.rs:1003-1048` | Tenant-only storage context and tenant-qualified evidence path preserve the tenant boundary. The hosted File write bypasses the existing File authorization/audit owner: SEC-001. |
| Scratch paths and isolation | `hosted/workspace.rs` materialize/host/writer/write/work_room; changed workspace tests; HostedTools::open | Fresh workspace per attempt, closed `/work` path handling, symlink-aware parent/target checks, atomic staged replacement and shared writer are used. No credential or host path is taken from a tool argument. Concurrency/resource conclusions are left to the selected specialist. |
| Identity injection and provenance | `PublishArgs`, `WriteArgs`, `parse` in tools.rs; `blocks/agent.rs` Role/producer diff; `RecordedTool::record:285-335`; public journey forged-producer cases | Producer and tenant come from the host. Closed deserialization rejects caller-supplied identity/auth fields. Records preserve raw submitted arguments, output digest/length and error. This is not evidence of arbitrary secret redaction; see proof limits. |
| Tests and changed public helpers | Rust `hosted_agent_tools` selection, scope/delegation, forged producer, cancellation and provenance cases; tools pg-test diff; Python conftest/support diff; Python/TS/MCP journey changes | Changes script local model replies and use ordinary server/client authorization. No new test bypass or production credential forwarding was found. Runtime claims were inspected, not rerun. |

## Authority clarification: role removal is not principal revocation

TASK-002 scenario 1 says “Invoke/File grant removed between calls or current authority reduced” must refuse the next protected call. This cannot be read as a requirement to reload role grants on every call:

* `architecture/wyrd-security-posture.md:145-157` explicitly preserves one accepted execution authority and says it “is not refreshed by later grant changes”; protected downstream actions check their own permission against that authority.
* `architecture/wyrd-security-posture.md:274-284` separately distinguishes issuance/credential state, existing token snapshots, and principal revocation cancelling unfinished executions. Existing tenant tokens retain snapshot authority until expiry. Principal revocation retires execution authority through cancellation; that does not make a role-assignment removal an immediate grant refresh for an accepted run.
* `audit::authorize` checks the supplied accepted principal. `Invocation` does not resolve current roles. `auth/revoke.rs:73-121` commits principal revocation through the existing owner; `components/principals/routes.rs:956-964` routes role removal through the ordinary direct-role owner instead.
* There is no independent File permission/grant to remove: verification File upload operations use any `verifier:run` grant, as `VerificationControl::authorize_evidence` and `storage/routes.rs:644-666` document. Exact execution admission separately authorizes the selected Verifier.

The candidate's immutable caller is consistent with stronger security authority. Correct the private task/proof wording through the planning owner to distinguish initial narrowed/delegated grants, repeated checks against accepted authority, role changes affecting subsequent issuance, and principal revocation/cancellation. Do not add a second resolver, authorization epoch, token revocation list or accepted/current grant intersection as a task fix. If the intended approved behavior truly requires immediate reduction after arbitrary role removal, changing that security contract requires specification/architecture revision and human approval; this review does not infer that decision.

The test named `tool_calls_recheck_permissions_and_revocation` (`hosted_agent_tools.rs:1232-1320`) proves initially narrowed direct/delegated grants and explicit cancellation. It does not mutate grants between calls or revoke a principal/credential. Its name cannot establish those unexercised behaviors.

## Proposed finding SEC-001 — hosted publication skips File domain authorization/audit

Classification: **MISSING**. Location introduced at `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:353-356`, calling `Publications::path`; underlying bypass at `hosted/publications.rs:195-232`.

Obligation: TASK-002's hosted permission/provenance requirements and REQ-023 require each tool invocation's own permission plus underlying capability authorization. `architecture/wyrd-design.md:2010-2017` requires the called capability's own permission and nonblocking audit for each decision; the existing File boundary is `VerificationControl::authorize_evidence`, not a new permission identity.

Reachable path: an ordinary authorized Agent writes `/work/report.md` and invokes `publish`. `Invocation::call` authorizes/audits only `tool:invoke(publish)`. `Invocation::publish` delegates to `Publications::path`; `Publications::complete` constructs `StorageCaller { data_tenant_id }`, calls raw `file_upload_init`, writes storage, calls `reconcile_upload`, and records inventory. None of those calls evaluates/stages the File capability permission. The low-level storage context explicitly contains no principal because its receiving adapter is responsible for authorization.

Existing sibling owner: normal File upload planning calls `VerificationControl::authorize_evidence` (`service.rs:304`), and public upload follow-ups call it via `storage/routes.rs:651-665`. It evaluates any accepted `verifier:run` grant and stages an allow/deny decision on the process Scribe outbox. Hosted publication reaches the same File lifecycle without this adapter obligation.

Consequence/materiality: new Agent-controlled durable File creation/completion has a tool-invoke audit but omits the mandated receiving File domain decision/audit. This is an explicit security boundary/accountability omission. **This is not evidence of a cross-tenant write or default-RBAC privilege escalation**: a supported run already passed exact Verifier admission, and its accepted snapshot ordinarily contains that grant. It also does not justify dynamic File-grant refresh. The missing domain boundary matters independently of those unsupported claims.

Smallest testable correction: retain Publications and the shared File lifecycle; reuse the existing `VerificationControl::authorize_evidence` mechanism at the trusted receiving File persistence boundary before protected File IO, under the same accepted caller, with normal nonblocking audit. Preserve snapshot authority, tenant paths, replay-before-work behavior and inventory/state acceptance. Do not create another uploader, permission enum, policy engine, or live-grant resolver. A focused existing hosted publication test should assert both tool invocation and File domain decisions are staged for a fresh path publication; an owner-level missing-verifier-run negative test should show no File IO after denial. Assert replay does not repeat already accepted File work. This is a bounded authorization/audit correction; it needs no new security decision.

## Verification limits and rejected expansions

This was static review. No Postgres journeys or other test suites were rerun. The existing permission journey demonstrates denied tool calls returning no output, initially narrowed delegated authority, and explicit cancellation; it does not demonstrate between-call role/principal revocation or a File permission decision. The task's all-green evidence table overstates those particular observations.

`RecordedTool::record` copies arguments before dispatch, including rejected fields. A closed schema prevents using a forged credential/identity field as authority; it does not sanitize that field in history. REQ-023 asks for sanitized provenance under sensitive capture policy, but no tool-specific sensitive-input policy or sanitization rules were established by the inspected owning sources. `ContextCapture` is explicitly limited to requirement `actual` values (`wyrd-spec/src/verification.rs:32-49`); applying it to all tool arguments would introduce a new behavior. I do not prescribe blanket content deletion, invented secret-key lists or a new capture policy as remediation without establishing that governing contract and a concrete disclosure path. Server-owned credentials are not supplied to these tools by the reviewed dispatch. This is a proof limit, not a second confirmed credential-leak finding.

Likewise, public schema discovery is not data authority. Selection advertises declared descriptors and invocation enforces permissions; merely seeing the `publish` schema does not prove a content leak. No remote source acquisition, GitHub connection policy, provider credential system, speculative sanitizer, or unrelated preexisting sandbox mechanism was added to this review's correction scope.

Packet SHA-256 values recorded at review start/end: spec `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`; publication supplement `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9`; durability supplement `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f`; TASK-002 `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`.

Skipped runtime execution and unrelated integration acquisition. Remaining risk: File domain audit coverage is absent; arbitrary provenance secret redaction has not been proved.
