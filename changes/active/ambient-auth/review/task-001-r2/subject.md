# Immutable TASK-001 re-review subject

Repository root: /home/thorrester/Documents/GitHub/wyrd-worktrees/split/ambient-auth
Original base: 5bcd9e4872f1376f1722922fa63b67384243be04
Current candidate: 0cfbf6422709cc9d3de680788c616f82f29255da
Reported remediation candidate: 276f648f4 (immediate parent; current successor only changes a related spec reference)
Previous reviewed candidate: 824f0c491ea2686977c9da9fce03ac4ef65c01b1
External root: /home/thorrester/Documents/GitHub/wyrd-example-ts
External original base: bef9fc05ec1725070588bcd49efdc937b02b0131
External current candidate: 000ed1f2f587a6629a471c51aadff5e05f0c820b
External previous candidate: b283666df93a32cb73aa8c07f881d9271797dff0
Spec: changes/active/ambient-auth/spec.md approved r3
Original task: changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md
Remediation task and evidence: changes/active/ambient-auth/review/task-001-r1/TASK-001-R1-complete-consumers-and-reuse-principals.md
Prior verdict / validated findings: review/task-001-r1/verdict.md and findings-validation.md (same change root)
Review skill: .agents/skills/wyrd-task-review/SKILL.md
New review directory: changes/active/ambient-auth/review/task-001-r2

Read the complete original-base-to-current cumulative diff, including prior review artifacts as historical evidence, not new runtime behavior. Use previous-candidate-to-current diff to locate remediation changes; retain full original task acceptance coverage. Preserve stable FIND-TASK-001-1/2 IDs. Do not mistake a prior accepted/closed claim for source proof. Inspect sources/available records rather than implementation chat summaries. The evidence is in the task/remediation files above. No new specification behavior is requested.

## Common starting map (expand independently)

- TS production owner: sdks/wyrd-sdk-ts/wyrd/src/index.ts WyrdClient.httpAuth/accessToken; native and shared wyrd-client credential/cache owner. Python native src/client.rs getter; python/wyrd/client/_http_auth.py dual HTTPX adapter; source/generated stubs; retained Gateway administration. CLI auth/access_token.rs dispatch and auth/mod.rs; auth_issue_key_journey.rs.
- Full original test surfaces: TS gateway unit, gateway-inference, mcp-reference-client, support/server.ts and local-development.ts; Python test_http_auth.py/test_client_http_auth.py/local-development substitution; existing gateway/admin/export coverage. Docs stock-client guide and generated llms, dev-only Anthropic dependency/lock.
- Remediation changed TS gateway-inference.test.ts to use existing Principals create/revoke; deleted raw revokedApiKey helper from support/server.ts and corrected type-only imports. Trace its actual role/principal lifecycle and negative assertions through existing native/shared owner.
- External src/agent.ts exports its existing single WyrdClient; scripts/investigate.ts uses its httpAuth/serverUrl. Expand all tracked src/scripts/test consumers, package/tsconfig and ignored package supply separately. Preserve MCP toolFilter/connect/close/tracing/model behavior.
- Evidence reports a composed ignored SDK build that also includes split/agents-trace-processor (2347acd97). That composition is not the immutable Wyrd candidate. Inspect package/build provenance without treating uncommitted dependency contents as committed candidate source or silently equating them to candidate-only proof. The example's trace processor usage predates this task; determine what evidence establishes the changed transport and what remains a separate integration dependency.
- Proof definitions: mise.toml, TESTING.md, spec AC001–008. Bounded task proof versus final whole-change aggregate must be distinguished. Do not rerun full suites.

## Authority and operation limits

Read AGENTS.md, architecture/agent-rules.md, active design/doctrine, reference router, spec-driven-development and maintainer-style, relevant Rust/PyO3/Python/TS/testing/errors references and security posture. Deployed topology: user-owned stock provider/MCP processes → shared Rust credential owner → single logical wyrd-server edge across possible replicas/targeted pods → gateway/MCP permission checks. No server/Bifrost implementation is changed; trace actual source where required.

Source is immutable; do not change reviewed source, packages, generated output, manifests, config, credentials, server state, vendor links or worktrees. Do not print secrets. Do not run live provider calls or reuse another session's credential store. No cleanup/deletion of scratch resources is authorized by review. Read-only provenance inspection is allowed. Do not start Cargo-backed jobs (shared target serialization); focused existing non-mutating checks may use mise if needed. Write only your assigned new report. You are not alone; do not revert others' changes. Use CodeGraph first where indexed. Verify both HEADs before finishing.

Discovery reviewers must not read other r2 reviewers' reports or receive their conclusions. Both implementation reviewers audit full cumulative acceptance matrices; role lenses do not narrow their scope. Follow skill thresholds: material behavior/contract consequences or deletion of unearned code only; missing touched Rust rustdoc remains blocking. Return PASS/FAIL/BLOCKED with complete coverage and source-local IDs. Preserve prior findings only if actually still open; new IDs are allocated by final validator after FIND-TASK-001-2.
