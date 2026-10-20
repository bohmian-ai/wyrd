# Immutable R2 review subject

Repository: /Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration
Original cumulative base: 8651b96fd0c049ab5e63b0606c98326af1fc8848
Candidate: 71909c64d888caac85bcb184bbf2aba7c25e9e0b
Previous reviewed candidate / R2 implementation base: 2741395a33d5e1e5ee5d7022916627917254a663
Earlier reviewed candidate: 843484f6cceba1f48139349f66794966221c5d37
Approved spec: changes/active/wyrd-ui-main-integration/spec.md (revision 10).

Read `git diff 8651b96fd...71909c64d` for complete cumulative source acceptance;
use `git diff 2741395a3...71909c64d` only for remediation navigation/closure.
No diff/patch/archive is stored here. Review source is immutable.

## Required task and prior review inputs

All paths below are under changes/active/wyrd-ui-main-integration/:
- Original tasks/TASK-001-distribution-login.md; retained tasks/TASK-002-remove-old-ui.md.
- Earlier review/tasks-001-002-843484f6c/{verdict,findings-validation}.md and TASK-001-R1-installation-browser-sessions.md.
- Prior review/task-001-r1-2741395a3/{verdict,findings-validation}.md.
- Current assignment review/task-001-r1-2741395a3/TASK-001-R2-session-boundary-fixes.md, including its execution evidence. Read evidence as claims, not acceptance.

Prior IDs: FIND-TASK-001-1 installer cause source-corrected (verify retained
closure); FIND-TASK-001-2 nginx execution; FIND-TASK-001-3 native public cookie
forwarding; -4 current metadata; -5 callback unavailable; -6 container cleanup;
-7 test-only env; -8 selected logout tenant binding; -9 finite rustdoc. Preserve
IDs and trace shared causes. New retained findings start FIND-TASK-001-10 only
when genuinely new; no per-caller rediscovery. REQ-033 is approved human scope,
not retroactive reviewer finding. Do not revive confidential Node/OAuth or
credential-cookie encryption.

## Current caller direction / acceptance scope

The caller explicitly said: “we dont need to do the nginx issue right now”.
FIND-TASK-001-2's Linux/nginx execution is deferred and is NOT a blocker to
this R2 correction review. Do not run known-refused macOS image builds or ask
to reapprove this deferral. The latest request also describes the full official
image matrix, signed native runtime and human visual checks as still open for
later stages. Record all as DEFERRED/UNVERIFIED, never as executed PASS or new
implementation defects. Current acceptance is the seven R2 source corrections
and their regression boundaries; cumulative source still must satisfy original
requirements as amended by spec10. A PASS here does not certify final TASK-001
platform/visual completion. This scope statement conveys user authority, not
an intended source verdict. Actual source flaws and mandatory standards are
not waived by deferred execution.

## Shared navigation map — expand/verify independently

- Installer/build/DevServer cumulative owners and signed/cache/current selection
  remain in scope for retained closure, not redesign or repeated broad tests.
- UI forwardAuth in upstream.ts, auth hook, BrowserSessions.read/metadata/logout,
  BrowserSession.api, root/tenant layouts/actions/settings; direct public
  /auth/browser/session and logout differ from Node internal calls.
- Rust BrowserSessions durable owner and logout mutation, HTTP auth/browser.rs
  login_refused/cookie/lifecycle adapters, callback completion carrying typed
  errors; shared provider/login/issuer/SQL owners and bearer-only siblings.
- SQL browser session migration/queries/digest/RLS/time/credential/connection
  binding; selected-route vs embedded value tenant; existing resolver/recovery
  pattern and no current permission requirement solely for logout.
- scripts/server/test-startup-identity.sh and startup-common.sh cleanup trap;
  existing native/nginx fixture transport in identity_ui_e2e; no new setting.
- Exact tests: browser-sessions.test.ts, upstream.test.ts, native production-auth
  driver, Rust auth/browser pg_tests callback classification, pg_browser_sessions
  authority mutation, identity_ui_e2e native public endpoints; shell cleanup
  failure probe; finite formatter/module/commit-versus-delivery docs.
- Changed native provider-replacement journey expectation is a claim requiring
  diagnosis validation: activation retires the actor's connection-bound session,
  so subsequent layout no longer fabricates signed-in metadata. Inspect source,
  trace/evidence and independent diagnostician record, not just assertion line.
- Prior cumulative old UI deletion, distribution, native/container lifecycle,
  contracts/OpenAPI/source-driven docs and protocol confinement remain regression
  scope. No .codegraph exists.

## Review mechanics

Reports only in this new directory. Read task-review skill, AGENTS/agent-rules,
reference router and applicable full authority. No source/tests/task/config/
generated/older-report edits. Each fresh discovery role receives this same map,
not other fresh conclusions. Root coordinates execution: no cold Cargo builds,
broad suites or shared identity/service mutation; fast nonmutating checks are
fine. Do not use user completion narrative as proof. Inspect actual owning
symbols and complete relevant caller/sibling paths; no speculative hardening,
new env/secrets/frameworks or optional style rounds. Candidate changes → BLOCKED.

Initial trace diff SHA-256: c40d5eb244036dd64dc81c3031eb3bb33ef8af2876bd2b819dcbef3850a09ea0. Preserve unchanged through review.
