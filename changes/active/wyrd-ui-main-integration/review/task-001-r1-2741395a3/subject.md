# Immutable remediation review subject

Repository: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration
Original base: 8651b96fd0c049ab5e63b0606c98326af1fc8848
Candidate: 2741395a33d5e1e5ee5d7022916627917254a663
Previous reviewed candidate: 843484f6cceba1f48139349f66794966221c5d37
Remediation implementation base: deadf39af (approved revision-10 packet/architecture).

Read the full `git diff 8651b96fd...2741395a3` for cumulative acceptance, and
`git diff deadf39af...2741395a3` to locate remediation-changed owners. No diff
or source archive is stored here. Planning/reference additions are approved
context, not unrelated shipped behavior. Candidate must stay unchanged.

## Required inputs

Approved spec: changes/active/wyrd-ui-main-integration/spec.md (revision 10).
Original task: changes/active/wyrd-ui-main-integration/tasks/TASK-001-distribution-login.md.
Retained behavior context: tasks/TASK-002-remove-old-ui.md in same packet.
Prior verdict and ledger: review/tasks-001-002-843484f6c/{verdict,findings-validation}.md in same packet.
Remediation task: review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md in same packet; includes evidence tables. Evidence tables are claims, not acceptance. The original task's confidential Node/encrypted-cookie premise is explicitly superseded by approved revision 10: do not demand it again.
Prior finding IDs: FIND-TASK-001-1 (installation eligibility), FIND-TASK-001-2 (nginx real-provider proof). Preserve IDs on retained findings; new findings start at FIND-TASK-001-3. REQ-033 was human-directed, never retroactively a reviewer finding.

## Shared navigation starting map — expand and verify independently

- Installer: CLI ServerInstaller::{installed,install}, extract/check_payload/activate; DevArgs installed lookup; server_install fixture/preservation/healthy-cache tests; build.sh payload producer.
- Sessions: wyrd-spec auth/browser.rs, LoginInitiation in auth/oidc.rs, token/device/client-id contracts and error catalog; wyrd-auth/browser_sessions.rs owning lifecycle; login/callback/issuance/refresh/cli_logins siblings.
- Persistence: wyrd-sql migration 20261020000000_auth_browser_sessions.sql; queries/auth/browser_sessions.rs, login_state.rs and refresh_tokens.rs; RLS and expiry/cleanup/migration tests.
- Server boundary: auth/browser.rs, callback.rs, metadata.rs, oauth.rs, components/auth/routes.rs, http/middleware/authenticate.rs, router.rs/openapi.rs. Trace actual principal/permission/audit consumers and bearer-only platform/MCP/OTLP/gateway/gRPC siblings.
- UI: BrowserSessions/BrowserSession now request forwarding; upstream.ts auth/cookie/origin forwarding; hooks and root/tenant layouts/actions/settings; deleted Node callback and OAuth dependencies; production-auth driver and auth/upstream/routing tests.
- Runtime/proof: identity_ui_e2e native/two-replica and nginx tests; pg_browser_sessions, pg_openapi_contract; scripts/identity/with-identity-providers.sh; scripts/server/test-startup-identity.sh and existing startup-common/entrypoint/nginx; mise test:identity:ui and :container plus CLI journey build prerequisites.
- Cumulative original distribution/removal: build/release/official Debian/Rocky images, DevServer lifecycle, old UI delete/keep inventory and all retained consumers.
- Authority: AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, wyrd-security-posture.md, deployment/release, reference router and applicable language/testing/maintainer references. No .codegraph exists.

## Scope and execution rules

Read-only source acceptance audit. Reports only in this new directory; do not edit source, tests, tasks, prior reports, fixtures, config or generated artifacts. Review full caller/producer/sibling paths before proposing a bounded correction. No optional improvements or new secret/env/framework/session machinery. Follow current AGENTS rules including substantive private rustdoc. Review assertions changed to address failures against actual diagnoses and independent diagnostician evidence.

Do not receive other fresh discovery conclusions. Do not use the user's implementation summary as proof. Source and the task's explicit execution evidence are the inputs. Distinguish executable proof missing from proof written but unexecuted. Cross-platform and human visual evidence is absent in the task; never infer a pass. Do not rerun known-blocked image builds on macOS or mutate shared identity services during parallel review. No cold Cargo rebuilds or broad aggregates; any focused execution must be coordinated with root. Non-mutating inspections and fast UI checks are fine.

This session has preserved unrelated trace edits. Initial trace diff SHA-256: c40d5eb244036dd64dc81c3031eb3bb33ef8af2876bd2b819dcbef3850a09ea0. Source/candidate remains immutable; final HEAD check required.
