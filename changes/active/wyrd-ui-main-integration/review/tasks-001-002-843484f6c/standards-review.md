# Repository standards review

Result: **PASS** for material repository standards in both TASK-001 and TASK-002. This is a standards assessment, not task acceptance or deployment qualification. No material proposed findings. Required runtime and human acceptance evidence remains incomplete as listed below.

## Immutable subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
- Candidate: `843484f6cceba1f48139349f66794966221c5d37`.
- Inspected cumulative range: `git diff 8651b96fd...843484f6c` and owning source/callers, rather than the latest commit alone.
- Authority packet: approved revision 9, original TASK-001 and TASK-002, their evidence tables, and `subject.md`. Planning-only packet additions supply context; they are not independently shipped application surfaces.
- The unrelated trace edits in the working tree are excluded. No source, task, configuration, or fixture edits were made by this reviewer. No CodeGraph directory is present.

## Authority and changed-surface coverage

All paths below are repository-relative. `UI` means `crates/wyrd/wyrd-server/wyrd-ui`.

| Changed surface | Governing authority | Source and consumer coverage |
|---|---|---|
| Native server launch, UI prerequisites, readiness, exits, settlement, loopback inputs | AGENTS §§2–6, 9, 11–12; agent-rules; references/languages/{rust-core,maintainer-style,errors,testing-workflows}; references/architecture/patterns; deployment-and-release | Full `wyrd-cli/src/server/dev.rs` changed owner and helpers, including `DevArgs::run`, `DevServer`, `ui_command`, `check_ui`, `start`, `run`, `ready`, `exited`, `stop`, `settle`, `local_binds`; existing migration/setup/config/credential owners remain in use |
| Verified installation and release payload | AGENTS §§2–6, 9, 11; security posture credential/provenance boundaries; deployment-and-release | `server/install.rs::extract`, enclosing `ServerInstaller` flow, `InstalledServer`; `tests/server_install.rs` fixture and failed-update caller |
| Rust launch/identity test harnesses | AGENTS §§4–6, 11–12; agent-rules private rustdoc and test placement; references/languages/testing-workflows | `tests/pg_tests.rs` full `LocalDev` fixture, new `ui_build` and `sign_in_through_ui`, changed first/repeat/partial tests; server `tests/integration/identity_ui_e2e.rs::start_bff` and its callers |
| Production auth forwarding and hook | AGENTS §§2, 9–11; design doctrine/client model/runtime identity; wyrd-doctrine public surfaces; security posture trust/OAuth/SSRF boundaries; UI skill and SvelteKit reference | `hooks.server.ts`, full `upstream.ts`, forwarding/readiness tests, `BrowserSessions` server calls and unchanged session authority; real production-auth journey driver and its Rust fixture |
| UI type relocation and retained development identity | AGENTS §§2, 9; references/languages/{typescript-guide,errors}; UI skill/server-client boundary | `app.d.ts`, auth/session and browser-session type imports, problem projection, `development.ts`, root server load/actions and direct consumers |
| Retained layouts, chooser, landing page, login and settings, error state, CSS | UI skill, brand/DESIGN and palette/contracts, UI references/{wyrd-sveltekit-architecture,wyrd-svelte5-patterns,wyrd-testing-verification}; AGENTS §§9, 11 | Root `+layout.*`, `+page.*`, `+error.svelte`, tenant `+layout.svelte`, `+page.svelte`, both tenant login pages, settings page, app.css; ui-library and routing/session tests |
| Removed components/features/mock/domain routes and their tests | AGENTS §§2, 11–12; spec-driven-development authority and approved removal scope; UI server ownership rules | Complete deletion inventory: components, features, registry/views, mock/changes/wyrd server adapters, styleguide, tenant cards/changes/observe routes, obsolete tenant loader; retained import changes and removal of chart-only test polyfill |
| UI README and brand metadata | AGENTS documentation/generated-output rules; UI brand authority | README, brand/DESIGN, components.json comment; palette, generator, theme and generated wire/problem artifacts are unchanged |
| Debian/Rocky images, nginx, process entrypoint | AGENTS supported-target/ownership rules; security posture transport boundaries; operations/deployment-and-release | Both Dockerfiles, deleted Ubuntu image, full entrypoint/nginx template, image build invocation and release matrix consumers |
| Bundle/image scripts, release and CI workflows, mise configuration, startup/kind probes | AGENTS §§2, 11–12; deployment-and-release; references/languages/{testing-workflows,implementation-execution} | `scripts/build.sh`, startup-common/dev/prod, kind image call, `.github/workflows/{release,lints-test}.yml`, changed mise tasks and pinned tools |
| Python installed-CLI process tests | AGENTS §11 runtime ownership; references/languages/{python-api-and-stubs,testing-workflows} | Changed `test_local_development.py` readiness helper, process-exit selection, fixture UI installation, listener settlement and their callers |
| Architecture/operational documentation and change packet | AGENTS authority/target rules; reference router; spec-driven-development | AGENTS target correction, deployment/release bundle and image topology, troubleshooting memory instruction removal; packet authority and recorded verification limitations |

The router's Bifrost/data/Arrow, PyO3, and N-API implementation references do not govern a changed implementation here: no analytical owner, SQL query, binding, SDK contract, or generated declaration changes in this range. Removing the CLI memory override leaves resource sizing with the existing server owner; it introduces no replacement Bifrost algorithm.

## Applicable-rule results

| Rule | Result | Evidence |
|---|---|---|
| Durable behavior stays in its Rust server owner; clients do not duplicate setup, registration, audit, or resource sizing | PASS | CLI still invokes installed `migrate`/`setup`; deleted `detect_memory` override is not replaced; UI forwards auth rather than authenticating provider claims itself |
| Concrete ownership for retained dependencies/resources; no utility structs or new single-implementation traits | PASS | Existing `DevServer` owns both child handles and selected installation/binds; `settle` owns only its invocation's consumed child, as permitted by AGENTS §5 |
| Synchronous computation; async confined to awaited IO | PASS | `local_binds` and Node version parsing are synchronous; async launch/readiness/settlement methods await process/network/signal IO |
| Rust private-item documentation, errors, panics and material side effects | PASS | New fields/constants/helpers, `ui_command`, `check_ui`, `settle`, `ui_build`, sign-in helper and materially changed test methods have rustdoc; fallible helpers have Errors sections, test invariants Panics sections, startup/settlement docs describe retained state and cancellation |
| No production input unwrap, secret-bearing string diagnostics, or ad hoc runtime | PASS | New CLI failures use existing `WyrdCliError::ServerDev`; `expect` remains invariant-scoped; credentials still use existing protected configuration and `SecretString`; no new runtime |
| Cross-platform product behavior on three supported native targets; no product OS-specific resource probe | PASS (source) | Deleted macOS sysctl probe; UI lifecycle uses existing Tokio process handling; release matrix excludes Intel Darwin/Ubuntu and Rocky arm64 as required. Linux execution remains unverified |
| Tenant SQL capability and transaction boundaries | PASS / unaffected | Actual changed production signatures/fields contain no SQL pool, connection, transaction, or manual tenant-filter additions; durable database behavior remains behind existing server commands and authenticated API owners |
| Auth and audit authority remain server-owned | PASS | Hook resolves retained browser session owner; forwarding changes transport only; there is no added permission engine, audit sink, or token verification path |
| Secrets and backend access remain server-side | PASS | UI's server-only auth modules retain tokens; relocated `SessionMetadata` is an explicit browser-safe projection; route markup reads principal/tenant metadata, not access tokens |
| Existing structured errors and generated authority reused | PASS | `problem.ts` still projects Rust-generated examples; `forwardAuth` uses that existing upstream problem; server responses and OAuth refusals are returned without new code vocabulary |
| SSRF handling matches the effective trust boundary | PASS | `forwardAuth` selects only configured `serverUrl`, not browser-supplied host or an arbitrary external URL, and does not follow redirects. This is an operator-owned server transport, not a tenant Source fetch |
| Typed Svelte 5 data and local UI state; no new UI framework | PASS | Retained routes use generated PageProps/LayoutProps, `$props`, `$state`, `$derived`; package manifest and lockfile are unchanged |
| Tokens, semantic state labels, native forms and keyboard controls retained | PASS (source) | Added CSS uses existing variables; rewritten notices carry words/glyphs and login action alerts; tenant switch/sign-out/settings forms remain semantic. Visual/accessibility runtime acceptance is not inferred from this source pass |
| Generated outputs remain source-driven | PASS | No generated theme, wire schema, error projection, N-API declaration or Python stub edits |
| Tests belong to their runtime and correct tier | PASS | Rust integration tests drive real processes and HTTP; Python PTY/CLI tests stay Python-owned; production-auth journey remains managed by the identity integration owner; unit forwarding tests complement the real BFF path |
| Removed tests correspond to removed functionality, rather than circumventing failures | PASS | Deleted domain/component tests track deleted mocks/routes/components; routing tests now assert retained sign-in/tenant/sign-out behavior; production auth/session/error tests remain |
| No weakened gate or new suppression used to clear a test | PASS | No changed `allow`/`ignore`, blanket boundary exemption, or weakened production error check found; new readiness/exit tests exercise new lifecycle behavior |
| Official image/bundle topology and publication are coherent in source | PASS (static) | Bundle stages UI build/package/production dependencies; Dockerfiles copy the payload and pinned Node; release installs build tools and checks tar entry; nginx preserves server routes and sends other paths to BFF |
| Runtime verification is reported honestly | PASS as reporting; not runtime qualification | Both tasks explicitly leave Linux/native signed-bundle/image/visual proof unverified. Those missing results cannot be promoted to acceptance by a standards review |
| Diff hygiene | PASS for implementation; non-blocking packet whitespace failure | `git diff --check base...candidate -- ':!changes'` is clean. Full cumulative check reports trailing whitespace/new EOF lines in planning/archive/canvas packet files, including intentional Markdown hard breaks |

## Non-blocking notes

- `UI/src/lib/ui-library.test.ts:15` embeds `TASK-002` in a permanent test title. Agent-rules says not to hardcode task IDs in code. The original task itself prescribes this title. This is a naming-only conflict with no observed behavioral, security, tenant, durability, or public-contract consequence; it does not warrant a remediation round under this review's threshold.
- Packet whitespace above makes a claim that the *entire cumulative range* passes `git diff --check` inaccurate. The product implementation range passes. This is evidence precision, not a material implementation finding.
- The `/proc` use added in startup-common is a Linux-container-only fault-injection probe, not product resource management or a prerequisite for a native target. It does not restore the removed product-specific sysctl behavior.

## Verification and limits

Independently executed:

- `bash -n scripts/build.sh docker/official/extras/entrypoint.sh scripts/server/startup-common.sh scripts/server/test-startup-dev.sh scripts/server/test-startup-prod.sh scripts/server/test-kind-autoscale.sh` — PASS.
- `mise exec -- uv run --directory sdks/wyrd-sdk-python ruff format --check tests/integration/test_local_development.py` — PASS (one file already formatted).
- `git diff --check 8651b96fd...843484f6c -- ':!changes'` — PASS.
- Full cumulative `git diff --check` — nonzero for packet whitespace, detailed above.

Available implementation evidence records fmt/lints/check:deps/py:lints/docs:check and UI check/unit/build passes, focused installed-CLI tests, and the real identity suite. These were assessed against their source/test owners; no heavy suite was rerun by this reviewer.

Linux native targets, source-free real signed installs and bundle digests, Debian amd64/arm64 and Rocky amd64 image execution, and human visual acceptance remain unverified. UI manual verification requires both modes, responsive layout and keyboard/focus checks; a source pass does not supply them. No broad gate or live service operation was run here.

## Per-task standards result

| Task | Material findings | Result |
|---|---|---|
| TASK-001 distribution/login | None | PASS, with runtime qualification limits above |
| TASK-002 old UI removal | None | PASS, with human visual acceptance outstanding |

Overall: **PASS** (repository standards only). Finding ledger: **empty**.

Candidate identity rechecked before handoff: `843484f6cceba1f48139349f66794966221c5d37`.
