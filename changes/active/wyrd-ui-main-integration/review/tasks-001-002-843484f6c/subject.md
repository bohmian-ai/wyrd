# Immutable review subject

Repository: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration
Base: 8651b96fd0c049ab5e63b0606c98326af1fc8848
Candidate: 843484f6cceba1f48139349f66794966221c5d37
Review the complete `git diff 8651b96fd...843484f6c`; planning-only commit d2e2d25a8 introduces the approved packet and is context, not unrelated product drift.
Spec: changes/active/wyrd-ui-main-integration/spec.md (approved revision 9).
Tasks: changes/active/wyrd-ui-main-integration/tasks/TASK-001-distribution-login.md and TASK-002-remove-old-ui.md.
Combined cumulative audit, with separate acceptance and finding attribution for each task. No previous implementation review.

## Navigation starting map (expand independently)

- Packaging: scripts/build.sh bundle/image; CLI server/install.rs ServerInstaller, extract, InstalledServer; tests/server_install.rs; release.yml and deployment-and-release.md.
- Native lifecycle: CLI server/dev.rs DevArgs::run, DevServer::{check_ui,start,run,ready,stop,exited}, settle, local_binds; tests/pg_tests.rs first/repeat/partial startup; Python tests/integration/test_local_development.py installed CLI interrupt/exit.
- Public origin/auth: UI hooks.server.ts, lib/server/upstream.ts forwardAuth/serverUrl; BrowserSessions and BrowserSession; routes login/callback/logout and retained tenant/platform login routes; RequestOrigin server owner; identity_ui_e2e.rs and production-auth.integration.test.ts.
- Container: Debian/Rocky Dockerfiles, deleted Ubuntu Dockerfile; extras entrypoint/nginx template; scripts/server/startup-common.sh and startup dev/prod mise lanes; release matrix.
- Old UI removal: components/features/mock directories, lib registry/views/wyrd/development; retained root/tenant layouts, chooser, home, settings, error, login forms; app.css, ui-library.test.ts, routing and auth tests, brand/README records.
- Rules: AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, applicable references routed by architecture/references/README.md, security posture and deployment/release. No .codegraph directory present.

## Evidence and limits

Task files contain implementation evidence tables and exact commands: treat these as claims to inspect, not acceptance. Read original task obligations separately from completion narratives. No reviewer receives another reviewer’s conclusions. Native Linux, official image runtime, real signed bundle digests/source-free launch, and human visual acceptance are explicitly unverified in supplied evidence. Do not manufacture passes or require new architecture to address missing evidence.

## Immutability

Tracked working tree has pre-existing trace changes in crates/wyrd-spec/src/vala/trace/mod.rs and deletion of trace_summary.rs; excluded from subject, must remain untouched. Inspect committed source with git show where necessary. Never edit source, task, spec, fixtures or configuration. Reports only in this directory. Do not run broad resource-heavy suites or alter existing running services. Record verification limits. Check candidate identity before returning.
