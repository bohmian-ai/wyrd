# Focused follow-up: publication File authorization

Result: **RESOLVED**. One proposed finding, `FOLLOWUP-SEC-1`, requires independent validation. This is a source-derived finding, not agreement with another report; no other reviewer reports were supplied or read.

Subject: base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`, candidate `75c9bfa9e07163099eac40d5a556c4447c6a519d`. HEAD is the candidate. The cumulative diff was inspected for the publication adapter and governing tool contract. Source is unchanged. Scope is exclusively the receiving File-domain decision and audit, including sibling upload/completion callers. No Cargo or runtime tests were run.

## Resolution

An admitted execution already holds the File grant: exact `verifier:run` covers the operation-level File staging requirement. There is no separate `files:write` permission, and no need to look up current roles or refresh accepted authority. However, possession of that grant and the earlier exact-Verifier admission decision do not replace the called File capability's receiving decision and its audit. The candidate performs only the tool decision at the new publication boundary. The resulting defect is a missing File-domain decision/audit, not a demonstrated ability for a publicly admitted caller to publish without any `verifier:run` grant.

The authority is explicit:

* `architecture/wyrd-design.md:1793–1799` requires operation-level `verifier:run` for File planning and follow-up operations and says the decision is audited; any exact or All grant suffices. This describes the File domain without inventing another permission.
* `architecture/wyrd-design.md:2009–2015` requires each tool call's own invoke permission followed by the called capability's permission, under accepted execution authority, with each decision audited. The task's “Limits and provenance” row and REQ-023 (`spec.md:475–482`) preserve invoke plus domain permission.
* `architecture/wyrd-security-posture.md:145–157` freezes accepted authority but still requires every protected downstream action to check its own permission against it. `:14–15` requires authorization at receiving boundaries. There is no exemption here for File publication.
* `architecture/wyrd-security-posture.md:825–828` makes audit cardinality follow decisions, not requests or engine mechanics. The requirement is an audited receiving domain decision; it does not imply an audit for every internal storage helper, object HEAD, SQL commit, inventory row, or publication checkpoint.

The source expressly exempts exact Agent/Prompt definition loading by placing it under the invocation's existing audited `verifier:run` (`wyrd-design.md:1788–1791`). No equivalent exception exempts File publication from the tool/domain rule. The public docs table's “Step Agents only” description for publish is a restriction on availability, not an alternative normative File permission rule.

## Complete path and sibling comparison

| Owner/caller | Source inspected | Decision and effect |
|---|---|---|
| Direct execution admission | `components/verification/service.rs:428–463,962–1003`; queued admission also calls `authorize_run` at `:688` | Checks exact Verifier plus subject attribution and stages that run decision. It is not a later File decision. |
| Accepted caller binding | `verification/hosted/blocks.rs:139–178`; `hosted/tools.rs:133–195` | HostedTools and Invocation keep the execution's accepted Caller. No tool argument supplies identity. |
| Actual model tool dispatch | `crates/vala/vala-verify/src/blocks/tools.rs:272–335`; `hosted/tools.rs:554–563,228–240,325–368` | RecordedTool delegates invoke_call; Builtin dispatches to Invocation::call. The call authorizes/audits exact `tool:invoke`, then publish builds its host-owned PublishCall and invokes Publications. Recorded ToolCallRecord is provenance, not canonical permission audit. |
| New production publication caller | `hosted/tools.rs:342–368` | The only production `Publications::new` caller. Other callers found are integration tests in `pg_durable_verifier_work.rs`. This task makes the backend a real model-selected capability. |
| File publication backend | `hosted/publications.rs:116–183,192–231` | `path` checks replay, freezes the path, completes each file, and accepts the publication. `complete` discards principal permissions into tenant-only StorageCaller, calls file_upload_init, writes bytes, calls reconcile_upload, and records the exact completed object. Neither path nor complete checks or stages File authorization. |
| Lower storage lifecycle | `wyrd-storage/src/service.rs:240–266,600–661`; `components/storage/routes.rs:711–718` | Storage is deliberately tenant-only. The projection comment says the receiving route has already evaluated and audited permission. Lower storage cannot supply the missing caller-domain decision. |
| Public File planning | `components/verification/service.rs:298–353,1005–1047` | `evidence` calls existing `authorize_evidence` once before processing its set of files. That owner checks any-scope `verifier:run` and stages Allowed/Denied before IO, without waiting for audit durability. |
| Public File upload operations | `components/storage/routes.rs:162–180,427–459,651–667` and completion/abort callers | They resolve persisted upload owner, then call `authorize_evidence` for File or cards-write for Card, before storage work. Tenant lookup prevents foreign sessions from selecting authority. |
| Sibling internal completion | `components/cards/routes.rs:572–589`; `components/cards/service.rs:1931–1938,2114–2190,2382–2420` | A receiving cards-write decision covers Card completion; verify_card_manifests can reconcile multiple uploads without another permission decision per internal helper. Background reconcile_card_claim (`:1954`) resumes the accepted lifecycle rather than creating fresh caller permission decisions. This rules out putting an audit in generic reconcile_upload or demanding one audit per object operation. |
| Inventory/checkpoint persistence | `components/storage/inventory.rs:213–261`; `hosted/publications.rs:273–309` | Inventory stages Object writes and awaits their ACK; publication acceptance checkpoints state. Neither is a permission verdict. Their durability/lineage cannot substitute for domain audit, nor should they gain artificial permission checks. |

Paths without a crate prefix in this table are under `crates/wyrd/wyrd-server/src/`.

## Proposed finding FOLLOWUP-SEC-1

Classification: **MISSING / mandatory authorization-and-audit contract**, suggested P2. Changed location: `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:353–355`, reaching existing `hosted/publications.rs:126–183,192–231`.

Supported scenario: a valid, admitted step Agent with `tool:invoke` for publish writes a file to private scratch and publishes its path. Exact Verifier admission stages an execute/submit permission event; Invocation::call stages `verification.tool.invoke` for publish. File initialization, byte retention, completion and publication acceptance then occur without any File-domain authorization/audit event. This is the normal newly enabled file/tree publication flow, not an unusual input or a forged producer. The candidate's successful journeys exercise it.

Observable consequence: canonical audit has the run admission and tool invocation decisions but never the required receiving File-domain decision governing retained File creation. A ToolCallRecord, object-inventory fact, and accepted publication prove different facts; none contains the missing File-domain permission verdict. It is material because architecture explicitly requires downstream domain authorization and its audit, even where an upstream grant makes the later allowance predictable. Do not label this an observed unauthorized cross-tenant upload or claim live grant revocation is required.

Smallest correction: reuse `VerificationControl::authorize_evidence` at the shared receiving file/path publication boundary, under Publications' accepted Caller, before replay returns content or snapshot/upload work starts. One publication request may bundle a directory's files, just as evidence planning bundles files under one decision. Keep exact tool invoke admission in Invocation::call. The receiving File-domain operation owns this guard; it is not a repeated integrity guard at each storage consumer. Keep shared storage tenant-only and leave lower upload, inventory, acceptance, cancellation and durable ACK behavior intact. JSON publication creates no File and does not need an invented File staging decision.

This recommendation adds the existing synchronous decision/outbox staging at one existing owner; it needs no new policy, principal, grant, File uploader, registry, dynamic authority source, transport, or durable audit wait. Deleting the File publication behavior would violate the task; reusing just its tenant-only storage leaves cannot meet the permission/audit contract. Do not require new authorization for the internal engine transitions or demand per-file/per-SQL audit counts.

Focused closure proof: use the existing server test/RecordingScribe patterns to publish a real scratch file/tree under an accepted caller whose verifier grant names one exact UID. Observe separate tool-invoke and File-domain Allowed audit decisions with unchanged subject/actor/delegation attribution, and confirm receipt/content/descriptor behavior remains intact. A focused direct receiving-owner negative test may establish that tool authority alone cannot reach upload IO; that tests the boundary, not a claimed public admission bypass. Existing `tools/pg_tests.rs:118–163` tests Card/Bifrost domain denials only. The new journey `hosted_agent_tools.rs:1231–1319` tests narrowed/delegated tool grants and cancellation, not a File-domain audit event; its name does not close this gap. Audit remains best effort: assert staging/drained observed decisions using the existing fixture, never block publication on canonical audit persistence.

## Review limits and immutability

Static source review only; no provider, Postgres, Cargo, or audit runtime run. Read original task and approved revision-18 obligations relevant to publication, the publication supplement, durability context, authority map, applicable security/design sections, workflow and maintainer guidance. Draft wait/approval additions were not used. No nested AGENTS.md exists in the two source owners inspected.

Packet hashes recorded during review and rechecked after report creation:

* `spec.md`: `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`
* `tasks/TASK-002-agent-tooling.md`: `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`
* `output-publication.md`: `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9`
* `execution-state-and-durability.md`: `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f`

Only this report was written. Existing skill edits and other review artifacts belong to concurrent work and were not altered. The proposed finding still needs independent validation; this report does not issue the task verdict.
