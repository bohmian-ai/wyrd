# Immutable subject and shared navigation map
Root: /Users/stevenforrester/Documents/GitHub/local-server-bootstrap-task-004
Base: cb5656b2b913dbd88a8f6736bf4a0baaf998960c
Candidate: 741e42ed97ac3ff9638217bbcd4f0c0583e6d464
Approved spec: changes/active/local-server-bootstrap/spec.md. Review TASK-004 obligations; later independent tasks are not this task's scope.
Original task and verification: changes/active/local-server-bootstrap/tasks/TASK-004-local-dev-journey.md. Use corrected commands in final evidence. Treat claims as hypotheses, not conclusions.
Separate draft docsite range: 2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5; inspect git show/diff without checkout.
No prior independent review in packet. Cleanup is part of initial cumulative review.
Source is immutable; write only assigned report. No staging, commits, or implementation edits. No .codegraph exists.

Starting map (expand independently, not complete coverage):
- CLI server/{mod,dev,install}.rs: DevArgs::run, DevServer start/migrate/provision/ready/wait/stop, ProvisionedTenant parse/credential, disclose_root, local_bind, save_local_tenant, verify_saved_client, interruption. Compare existing InstalledServer workflow and server-owned main migrate/setup/run.
- shared/wyrd-client/src/{global_config,credentials_file}.rs: GlobalConfig local_api_key/save_local; CredentialsFile config_at/document/replace/lock/read_text. Trace SavedLogins, token cache, client readers and sibling writers.
- wyrd-cli/src/mcp/{mod,hosts}.rs: host selection, existing installer/proxy, prompt cancellation and manual install caller.
- wyrd-cli/src/error.rs, Cargo.toml, Cargo.lock.
- sdks/wyrd-sdk-python/src/cli.rs and wyrd-sdk-ts/wyrd/src/cli.ts: signal ownership and adjacent commands.
- wyrd-cli/tests/{cli,pg_tests}.rs: pg_tests::server_dev_* journeys.
- Python tests/integration/test_local_development.py, Rust tests/integration/local_development.rs, TS tests/integration/local-development.test.ts, scripts/check_ts_package.mjs, mise.toml test:local-dev:journey and SDK lanes; prove CLI itself performs setup.
- docs reference/cli.svx and self-hosting/local-development.svx; separate docsite get-started/setup.md, operate/troubleshoot-a-deployment.md and developer-docsite-rebuild TASK-002.

Read AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, references/README.md and applicable routed references, references/languages/{spec-driven-development,maintainer-style,testing-workflows}.md, TESTING.md. Security posture and deployment/release apply to sensitive domains. Skill: .agents/skills/wyrd-task-review/SKILL.md. Read complete cumulative diff, not summary/latest cleanup alone. Implementation summaries are not review evidence.

## User review calibration during this turn

User: "this is local dev setup. do not overcomplicate the findings".
Apply local-development deployment and threat context. Retain concrete required behavior/regression failures and explicit constraints. Reject speculative hardening, hostile scenarios without credible scope grounding, and corrections that invent new machinery beyond the approved task. Seek the smallest existing-owner correction; do not turn local subprocess startup into production supervision infrastructure.
