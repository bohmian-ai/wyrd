# Immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/split/ambient-auth
Base: 5bcd9e4872f1376f1722922fa63b67384243be04
Candidate: 824f0c491ea2686977c9da9fce03ac4ef65c01b1
Original task: changes/active/ambient-auth/tasks/TASK-001-client-http-auth.md
Approved spec: changes/active/ambient-auth/spec.md revision 3
Review skill: .agents/skills/wyrd-task-review/SKILL.md
Review directory: changes/active/ambient-auth/review/task-001-r1
External example repository: /home/thorrester/Documents/GitHub/wyrd-example-ts
External base: bef9fc0 (parent of candidate)
External candidate: b283666df93a32cb73aa8c07f881d9271797dff0

Use git diff BASE CANDIDATE for the complete cumulative diff (27 files); do not review only the latest commit. Source is immutable. Write only your assigned report; do not implement, modify tracked source, install packages, or regenerate artifacts. Other reviewers share the filesystem; do not revert others' work. Use CodeGraph before code traversal where indexed. Expand this starting map independently. Check HEAD remains the candidate before finishing.

## Starting navigation map (not coverage or conclusions)

- TS public client: sdks/wyrd-sdk-ts/wyrd/src/index.ts WyrdClient.httpAuth, existing accessToken; native credential owner is crates/shared/wyrd-client/src/client.rs::access_token and auth.rs middleware. Removed helpers must be checked against all consumers.
- Python: sdks/wyrd-sdk-python/src/client.rs PyWyrdClient::http_auth; python/wyrd/client/_http_auth.py HttpAuth; gateway auth removal and retained administration; python/wyrd/stubs/client.pyi and gateway.pyi plus generated public stubs; scripts/assemble_stubs.py.
- CLI: crates/wyrd/wyrd-cli/src/auth/access_token.rs, auth/mod.rs, existing client.rs ambient construction; tests/auth_issue_key_journey.rs and principal_journey helpers.
- TS tests: gateway unit, gateway-inference integration, mcp-reference-client integration; tests/support/server.ts and local-development.ts. package.json/pnpm-lock.yaml add Anthropic test dependency.
- Python tests: unit/client/test_http_auth.py; integration/gateway/test_client_http_auth.py; integration/test_local_development.py; existing gateway administration/inference/native-client and exports/typecheck tests.
- Docs: docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md; docs/public/llms-full.txt.
- External example: src/agent.ts; discover all MCP consumers and vendored package projections rather than assuming the one file is complete.
- Proof definitions: mise.toml, TESTING.md, spec AC-001 through AC-008, task Implementation evidence. Treat recorded results as claims and inspect their scope. Evidence currently lives in the task, not a separate verification.md. No prior task-review verdict exists; earlier plan-readiness finding concerned retaining gateway administration and was followed by approved spec r3.

## Authorities and topology

Read root AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, architecture/references/README.md, relevant language/testing references, architecture/references/languages/spec-driven-development.md, maintainer-style.md, and applicable security posture. No server/Bifrost implementation is changed. Caller-owned provider SDKs and MCP clients run in application processes, token acquisition uses shared Rust client middleware, HTTP reaches the single logical wyrd-server public edge (potentially multiple replicas/targeted pods), and gateway/MCP authorize the verified principal. Trace actual source rather than taking this description as proof.

## Required independence

Discovery reviewers receive no other reviewer's conclusions. Follow your assigned role in wyrd-task-review. Source-local IDs must be unique to your report. Return complete coverage and an explicit PASS/FAIL/BLOCKED. Behavior and invariant reviewers must provide full obligation acceptance matrices. Only material findings under the skill threshold; missing changed Rust rustdoc remains blocking. Do not convert task review into optional redesign. Do not run full suites; available proof can be evaluated statically and any targeted verification must use mise.
