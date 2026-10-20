# Independent maintainer review

Subject: base `8651b96fd0c049ab5e63b0606c98326af1fc8848` → candidate `843484f6cceba1f48139349f66794966221c5d37` in `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.

Scope: the complete cumulative committed range, including approved planning context; TASK-001 and TASK-002 evaluated separately. This report is an independent maintainer review, not a final acceptance verdict. Working-tree trace edits are excluded and untouched. No source edits or tests were performed.

## Authority and coverage

Reviewed under AGENTS.md, architecture/agent-rules.md, architecture/references/languages/maintainer-style.md, Rust core and testing-workflow references, the task-review skill, approved revision 9's relevant requirements/invariants, and the two original tasks. Deployment/release and Wyrd UI guidance supply the owning topology and frontend conventions. No CodeGraph index is present.

| Changed surface | Owning symbols, callers, and proof inspected | Maintainer assessment |
|---|---|---|
| Native CLI process lifecycle | `DevArgs::run`; complete `DevServer` owner, `new`, `command`, `ui_command`, `check_ui`, `run`, `start`, `migrate`, `provision`, `exited`, `ready`, `stop`; `settle`, `local_binds`, adjacent sealing/configuration/credential owners; server command dispatch | Extends the retained child owner instead of adding a lifecycle manager. Child observation and settlement are discoverable. `settle` consumes invocation-owned children and needs no utility struct. Pure bind parsing remains synchronous; added async operations await process/HTTP IO. |
| Signed installation | `ServerInstaller::{installed,install,download_verified,activate}`, `InstalledServer`, `extract`; callers in server dispatch and `DevArgs::run`; full installer fixture and changed replacement test | New payload definition is confined to fresh extraction; cached-version activation bypasses it. MR-001. |
| Native Rust/Python proof | `LocalDev` fixture and all changed methods, `ui_build`, first/repeat/partial-failure scenarios and their unchanged sibling scenarios; Python `wait_for_local_cli`, `local_deployment`, installed CLI fixture and bootstrap test | Existing harnesses extended with UI bind and cleanup assertions. No new harness/dependency. Fixture-installed UI is deliberately a checkout symlink, so this is not source-free release proof. |
| Browser public auth forwarding | `hooks.server.ts` and all hook branches; `upstream.ts::{serverUrl,forwardAuth,serverReady}`; BrowserSessions/BrowserSession complete owning module; forwarding unit assertions and production identity driver | Forwarding sits beside configured upstream resolution and is called only for `/auth/`. Does not create another OAuth/session owner. Explicit Response return and generated SvelteKit handler types retain meaningful contracts. |
| Identity runtime proof | Rust `start_bff` and its two callers, production identity fixture; TypeScript browser/gateway helpers, hostile headers and callback route changes | Existing production BFF proof now actually traverses forwarding. Native configured-origin topology is clear; it does not prove the nginx header-derived topology. |
| Retained UI state/types | `app.d.ts`, auth session types, problem projection, hooks, root layout/load/actions, tenant layout/home, login and API-key routes, settings, error page; unchanged tenant layout server and auth/settings owners | Deleted domain view types are moved to the owners that still consume them; no replacement domain client or shared component introduced. Existing auth/session action owners remain. Plain branded markup is within the explicit deletion scope. |
| Old UI, routes, mock removal | Full deleted-path inventory compared with REQ-032; surviving source and configuration consumers searched; routing/session regressions, `ui-library.test.ts`, removed ResizeObserver shim, README and brand references | Removes the prescribed graph, including tests that exist only for deleted behavior. Remaining tests preserve auth/tenant behavior and verify plain retained pages. No surviving deleted-module imports found. Non-blocking task-ID note below. |
| Build and deployment | Complete `scripts/build.sh`, Debian/Rocky Dockerfiles, entrypoint and nginx template; release/native/image matrix changes and packaging consumers; mise local-dev/frontend/build consumers | Uses the existing bundle owner and installed dependencies. Node prerequisites and payload shape are discoverable. No second build implementation or bundled native Node runtime. No generated API/declaration change. |
| Deployment proof and documentation | Startup common/dev/prod scripts, kind image consumer, lints/release workflows, AGENTS platform correction, deployment/release and troubleshooting docs | Selected image vocabulary and consumers are reconciled. Runtime image evidence remains missing; static inspection is not recorded as runtime success. |
| Planning and reference additions | Spec/task/plan/readiness records and canvas/historical reference additions in cumulative diff | Planning context has no production callers; no historical component import found in the resulting app. |

Rustdoc review includes new constants, retained owner fields, new/changed lifecycle methods, `settle`, `local_binds`, `extract`, changed Rust fixtures/helpers/tests and `start_bff`. These have substantive role/invariant documentation and applicable Errors/Panics notes; no missing-rustdoc blocker was found. UI wire projection types moved without widening their prior shapes. No authored Python stub, N-API declaration or durable public schema is changed.

## Material finding

### MR-001 — The installer has two incompatible definitions of a usable cached release

- **Task / classification:** TASK-001 / INCORRECT (bounded installer consumer closure).
- **Changed location:** `crates/wyrd/wyrd-cli/src/server/install.rs:445–449`, with sibling consumption at `:190–197` and `:228–248`; native refusal/recovery at `crates/wyrd/wyrd-cli/src/server/dev.rs:232–239`.
- **Governing principle:** Maintainer Style requires reading changed validation with its owning module and callers, reusing the existing owner, and making invariants safe to change. TASK-001 scenario 1 and REQ-002 require malformed/missing-UI replacements to be refused while preserving a working installation.
- **Evidence / reachable producer-to-consumer path:** New `extract` validation requires the server, package manifest, adapter entry and dependency directory. However `install` decides a cached candidate is reusable solely from `dir/wyrd-server.is_file()`, then calls `activate` without `extract`. `installed` likewise admits an active version using the server file alone. A version directory produced by the pre-change installer (which explicitly accepted empty `ui/build`) or a cached directory whose UI was removed therefore still activates successfully. This is reachable through ordinary `wyrd server install`, not only a test seam. If that cached version is latest, a healthy earlier `current` can be switched to it; subsequent `server dev` rejects the missing entry. Its instruction to rerun `server install` repeats the same successful cached activation.
- **Concrete maintenance and user cost:** Updating the installation invariant in its apparent owner does not update either cached path; maintainers must rediscover which entry point silently bypasses it. Users can get a reported successful install that cannot start its required UI, and the suggested recovery loops. This is not a demand to validate arbitrary JavaScript or add a new runtime verifier.
- **Smallest correction boundary:** Keep `ServerInstaller` as the installation owner and make the existing required-file/directory payload definition govern both freshly extracted and cached releases before activation. A cached candidate missing required payload must be refused without switching `current`; do not silently replace a directory that could be the active version. Align lookup/refusal guidance so a missing payload does not advertise an ineffective reinstall as successful recovery. Preserve staged signatures/digests, compatible-version behavior and healthy cached reuse; no new release format or lifecycle owner.
- **Focused closure proof:** Extend the existing installer replacement fixture to place an incomplete latest version under `versions/` while a complete older `current` is active; the install must refuse with the stable unverified error and leave the older link runnable. Cover required UI entry/dependency directory absence and retain the valid cached-reuse assertion. This finding is based on source control flow, not a newly run reproduction.

## Non-blocking calibration notes

`crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-library.test.ts:16` names `TASK-002` in a permanent test title. The agent rule forbids task IDs in code; a behavior-based title would read independently of this packet. It has no behavioral consequence and does not affect this review's result. Do not create a style-only remediation round for it.

No optional abstraction, naming or layout refactor is requested. The plain repeated state markup is specifically required by REQ-032 and should not be refactored back into a shared old-UI component.

## Results and limits

| Task | Maintainer result | Reason |
|---|---|---|
| TASK-001 | FAIL | MR-001: new installed payload invariant misses existing cached consumers. |
| TASK-002 | PASS | Retained owners/types/route consumers are closed and deletion does not introduce a material maintainability defect. |

Overall: **FAIL**. Proposed material ledger: **MR-001** only; independent validation must decide its retention.

Skipped runtime checks: supplied Mac test results were reviewed as evidence claims, not rerun. Linux/native image runtime, real signed source-free release launch/digests, and human visual acceptance remain unverified and are not waived by TASK-002's maintainer PASS. Candidate HEAD was checked at review completion and remains `843484f6cceba1f48139349f66794966221c5d37`.
