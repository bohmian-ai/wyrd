# Independent invariant review — TASK-001 and TASK-002

Subject: base `8651b96fd0c049ab5e63b0606c98326af1fc8848`, candidate `843484f6cceba1f48139349f66794966221c5d37`. HEAD was checked unchanged before writing. Approved authority: revision 9 of `changes/active/wyrd-ui-main-integration/spec.md`; original TASK-001 and TASK-002. No previous implementation findings exist.

Overall: **FAIL**. TASK-001: **FAIL** (installation validity does not cover all activation paths). TASK-002: **BLOCKED for acceptance evidence** (source obligations appear satisfied; mandatory human checkpoint remains unperformed). No source edits or service changes were made.

## Coverage and authority

Read the full cumulative product diff, changed-file inventory, both tasks, applicable specification requirements and invariants, AGENTS.md supplied with the subject, agent rules, spec-driven development, maintainer style, reference router, testing workflow, doctrine, and deployment/release authority. Planning/reference additions belong to the approved packet, not independent product features. Existing trace work is excluded and untouched. There is no CodeGraph index.

Inspected owners and consumers: `ServerInstaller::{installed,install,download_verified,activate}`, `extract`, `InstalledServer::binary`; `DevArgs::run`, `DevServer::{check_ui,ui_command,start,run,ready,exited,stop}`, `settle`; package production dependencies and bundle/image/release producers; native fixture installation, first/repeat/partial startup and installed Python lifetime assertions; production auth forwarder, hook, BrowserSessions import/type changes, origin configuration and identity driver; nginx/entrypoint/startup probes; the complete REQ-032 delete inventory, retained route producers/markup, relocated browser-safe types, routing tests and UI regression.

The existing owners were extended rather than replaced. `DevServer` owns both child handles; the invocation-owned `settle` helper does not introduce another lifecycle controller. The existing build script remains the production payload producer. The new forwarder is the missing browser leg, while existing session/token machinery remains the BFF credential owner. Type relocation after deleting `views.ts` introduces no parallel durable contract. No deferred domain client was added.

## Acceptance matrix

PASS here means source evidence plus the available proof supports the bounded obligation; a FAIL row explicitly distinguishes an implementation defect from missing required evidence.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001 REQ-001: bundle contains production adapter-node UI and dependencies; system Node, no bundled executable | `scripts/build.sh::bundle` stages build/package and production node_modules; native UI command uses system node | Build import smoke exists; supplied UI build passes. Actual signed source-free native installation on three targets absent | FAIL — required deployed proof missing |
| REQ-002: signed identity and atomic activation include UI | `download_verified` still authenticates full archive; staging precedes `activate`; `extract` adds UI path checks | Signature/corruption/missing-UI fixture cases supplied as passing | PASS for signed archive coverage and staging; next two rows qualify payload validity |
| REQ-002: no incomplete cached version replaces usable current | `install.rs:228–249` checks only server binary before bypassing extraction and activating cached version; `installed` also checks only binary | No cached-incomplete version case | FAIL — INV-001 below |
| REQ-002: successful install actually contains runnable BFF | `extract:445–448` accepts placeholder files and empty node_modules | Positive fixture itself uses empty index.js, `{}` package and empty dependencies; never starts installed BFF | FAIL — INV-002 below |
| REQ-003/AC-002: first/repeat startup, Node prerequisites, conflict, partial failure, interruption, child death and cleanup | `check_ui`, two reservations, retained children, `exited` select, `stop` joins settlement; no new server-state writer | Supplied Mac first/repeat/partial and Python interrupt/exit pass; source shows child-death observation after setup, but no native child-death proof supplied | FAIL — mandatory negative-path proof incomplete; no additional established code defect |
| REQ-004: native auth goes through BFF at established origin; method/body/query/cookies/redirect/error preserved | `hooks.server.ts` forwards `/auth/`; `forwardAuth` targets configured server, strips incoming origin chain and disables redirect following | Forwarder tests cover request bodies, redirects, multiple cookies and unreachable upstream; identity driver now targets BFF | PASS |
| REQ-005: official Debian amd64/arm64 and Rocky amd64; no Ubuntu output | Dockerfiles, release matrix/default tags, build/startup defaults, deployment docs | Static diff; Linux image runtime not run | PASS for selected outputs; runtime remains next row |
| REQ-005/AC-004: nginx live routing, readiness, process cleanup and image identity | Entrypoint starts Node between Rust and nginx; native endpoints and public routes separated; startup adds UI sign-in and UI-death probe | Neither dev nor prod startup lane run; no immutable image ID/source runtime proof; only native BFF identity proven | FAIL — required deployed evidence missing |
| REQ-006/AC-003: existing login/session/tenant/platform checks preserved | Existing BrowserSessions and BrowserSession own auth; minimal imports/type changes; native origin explicitly configured | Supplied identity suite including production BFF succeeds; container path absent | PASS for native regression; FAIL for complete dual-topology obligation |
| INV-001/INV-002: contracts, tenancy, secret boundary, no mock fallback | No server auth contract changes; kept loaders return safe metadata; BFF upstream requests retain credential ownership | Existing credential/isolation/refusal tests retained, identity regression supplied | PASS within reviewed source scope |
| INV-003–INV-006/AC-005: one build/lifecycle owner; no speculative domain/client integrations or prohibited target | Existing build, installer and DevServer extended; selected native/container matrices reconciled | Diff/manifests show no new domain client/dependency/test-only selector | PASS |
| INV-007: real proof, no fabricated cross-platform/human pass | Evidence explicitly records missing targets and fixture-only installation | Required runtime/human evidence incomplete | FAIL — outstanding acceptance proof |
| TASK-002 REQ-032: delete listed components/features/mocks/domain routes/tests | Cumulative deletion inventory matches specified paths; domain home loader removed too | Static diff; supplied removal search; retained-route test | PASS |
| REQ-032: delete mock switches/locals/wyrd, preserve local sign-in/scenario helpers | `development.ts`, hooks, app declarations, root actions, mise frontend task | No remaining source dependency on deleted domain owner; production auth tests retained | PASS |
| REQ-032: retained routes use plain markup, safe tenant/principal/sign-out/settings, no shared components | Root and tenant layout/page/error/login/settings markup, metadata types in actual owners | Supplied UI check/test/build; focused UI regression renders signed-in/layout and switch/logout forms | PASS |
| REQ-032: unchanged auth/routing/error behavior and production login | Mock-only assertions removed with their subject; live auth owners remain | Supplied merged identity/UI regression passes | PASS |
| TASK-002 constraints: no new package, domain integration or copied historical component | Manifest unchanged; no replacement shared component directory | Static diff | PASS |
| REQ-020 and task human checkpoints, both tasks | Retained branded markup is inspectable | No recorded native/container desktop/mobile light/dark human acceptance | FAIL — human checkpoint pending |
| AC-010 scoped preservation/trace noninterference | Changes packet retained; source range excludes user's trace changes | Static range/HEAD check | PASS; does not waive human/runtime evidence |

## Proposed findings

### INV-001 — INCORRECT — cached activation bypasses the new UI payload invariant

Task: TASK-001. Obligation: REQ-002, AC-001; installing an incomplete release must not replace a usable installation.

Location: `crates/wyrd/wyrd-cli/src/server/install.rs:228–249`, in the workflow immediately surrounding changed `extract`; sibling consumer `ServerInstaller::installed` at approximately 187–198; downstream refusal `DevServer::check_ui` in `dev.rs`.

Producer-to-consumer evidence: a version directory containing `wyrd-server` but missing UI is a valid output of the baseline installer. It is also a reachable incomplete local cached version. The newly strengthened `extract` never sees it: `install` checks only `dir.join("wyrd-server").is_file()`, skips download/extraction and unconditionally activates it. With a different usable `current`, `server install` swaps that current to this UI-less version and reports success. With the UI-less version already current, `installed()` selects it; `server dev` refuses it and recommends reinstalling, but reinstall takes the same fast path and cannot repair it. Both paths share the same newly incomplete definition of installed-release usability. This is consumer closure of the changed package contract, not a demand to redesign caching.

Smallest correction outcome: keep validity in the existing installer and apply the same required UI-payload eligibility before using cached versions, exposing an installed release, or switching current. An invalid cache must never be activated merely because its server binary exists. Reuse staged signed verification to restore it or return useful refusal while preserving the active usable release; do not add repair logic to DevServer or another controller, and preserve normal reuse of valid cache entries.

Focused closure proof: extend existing signed-release installer tests with a usable current plus a newer cached version containing only the binary; installation cannot activate it. Also cover reinstall of an already cached UI-less version so the documented recovery reaches a verified complete payload or gives a useful explicit refusal, rather than success followed by the same startup failure. Keep existing signature/corruption/cancellation behavior.

### INV-002 — INCORRECT — empty placeholder UI is accepted as a runnable replacement

Task: TASK-001. Obligation: REQ-002 explicitly requires a runnable BFF, not an empty directory; scenario 1 requires corrupt/missing/incompatible UI not replace current.

Location: `crates/wyrd/wyrd-cli/src/server/install.rs:445–454`; corroborating producer fixture `crates/wyrd/wyrd-cli/tests/server_install.rs:50–58`.

Evidence: the whole validity predicate is presence of the server, package.json, index.js, and a node_modules directory. The positive signed bundle fixture writes `package.json = {}`, `build/index.js = ""`, and an empty node_modules. That bundle passes the new predicate and is activated. Running its UI command exits without serving anything. A signed archive with a valid outer digest can still be an incomplete packaging output; signing authenticates those broken bytes and cannot establish runnability. `check_ui` only repeats index-file presence, so refusal occurs after migration/server launch when the empty UI exits; the previous current has already been replaced.

Smallest correction boundary: strengthen the installer's staged production-payload validation before activation, reusing the existing adapter-node production layout and bundle/image load proof instead of another packaging/lifecycle owner. The positive install proof must use a real runnable production BFF payload rather than placeholder path sentinels. At minimum reject missing adapter-node server/client assets, unusable package metadata or unresolved production imports, and prove source-free startup for accepted payloads. Preserve the signed identity boundary, atomic activation and old-current preservation; do not shift malformed-payload acceptance to DevServer after activation. INV-001 and INV-002 can be implemented together around one installer-owned eligibility rule.

Focused closure proof: existing failure-preservation scenario should include a correctly signed placeholder UI (current positive fixture), incomplete generated build and missing runtime dependency; each must leave the prior current untouched. Existing package runtime smoke should start the accepted source-free installed UI with system Node and obtain an HTTP answer. No new harness/dependency is needed; existing installer and native journey owners suffice.

## Diagnosis and proof assessment

The deleted CLI memory override is consistent with the recorded diagnosis: it supplied whole host memory while the server's existing resource owner already computes and reserves its budget. Removing that duplicate producer preserves user-supplied settings and avoids the banned macOS-only probe; no reason to restore it was found.

The changed SvelteKit form probe uses an existing browser endpoint and same-origin HTML form semantics rather than weakening origin enforcement. Deleted routing assertions target deleted mock behavior, not surviving live behavior. No introduced timeout/skip/assertion relaxation was identified as a substitute for a diagnosis.

No tests were rerun by this source-review agent, and no large build was started. Supplied test results were checked against their actual source coverage, not independently reproduced. Linux native runtime, all selected official images and real signed source-free installation remain unproved. TASK-002 has no proposed bounded code finding from this audit; its human checkpoint remains a real acceptance condition. Task matrix failures for missing evidence are intentionally not dressed up as new product design or speculative code hardening.
