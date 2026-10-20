# Migration evidence — 2026-10-19

## Main refresh and readiness review

On the caller's instruction this worktree was fast-forwarded from `d3ce50626`
to fetched `origin/main` at `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
The pre-existing tracked trace-summary removal was stashed for the update and
reapplied without conflicts; its complete binary diff matched the pre-update
diff byte-for-byte. All 101 existing planning packet files matched the backup
after the update. The temporary stash was dropped only after those checks.
No local implementation was authored as part of the sync.

The tables below retain the original `d3ce50626` inspection as provenance.
Current verification capability claims must include this refresh:

- `group_id` is now carried by verify/submit and SDK projections, scoped to
  tenant and authenticated owner Service
  (`crates/wyrd-spec/src/verification/execution.rs:47–51`).
- `group_end {after}` uses a quiet period, and later activity can cause another
  recorded group judgment; it is not a once-per-session close rule
  (`crates/wyrd-spec/src/card/trigger.rs:49–54`,
  `architecture/wyrd-design.md:1275–1292`).
- Existing execution readback includes `owner_card_uid` and a pinned
  `GroupSnapshot` of source `execution_ids`
  (`crates/wyrd-spec/src/verification/execution.rs:173–182`,
  `crates/wyrd-spec/src/verification.rs:271–285`). SDK/MCP project these shared
  contracts. The result table includes `group_id`, `trigger_identity`,
  `execution_status`, and `verdict`. An execution's selected `result_id`, not
  an arbitrary retained row from an abandoned attempt, identifies its judgment
  (`crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:18–22`,
  :55–72).
- `owner_card_uid` is not a new `service_card_uid` column. The spec's proposed
  telemetry correlation and non-sensitive requirement reads remain separate
  prerequisites. Current requirement detail stays inside sensitive `outcome`.
- MCP parameterized-query parity is still absent: the query tool refuses
  `params` and builds an empty list (`query/collect.rs:125–139`, :197–203).
  `verification.evidence` still plans uploads as a scoped write
  (`mcp/verification.rs:97–110`), not retained-evidence retrieval.

An independent `$plan-readiness-reviewer` spec gate on this baseline returned
`Revise`, six Major findings and no Critical findings. The
[report](../review/plan-readiness/review.md), exact revision-5 subject and source
identity are retained in the packet. No replacement task packet existed to
review; execution readiness was not assessed. Draft revision 6 records the
accepted desktop/mobile completion gate and corrects grouped-verification facts,
while exposing the remaining material contracts for resolution. The later
task-packet gate will also use plan-readiness-reviewer, per caller instruction.

## Provenance and limits

| Source | Inspected identity | Status and role |
|---|---|---|
| Destination `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration` | branch `wyrd-ui-main-integration`, HEAD/local main `d3ce50626a8e380475c0561d8d53144c212e8603`, `main...HEAD` = `0 0` | Initially no tracked modifications; only untracked `changes/active/wyrd-ui-main-integration/`. Committed baseline, not a live migrated implementation |
| Historical `/Users/stevenforrester/Documents/GitHub/wyrd` | checked-out branch `wyrd-server-ui`, HEAD `7bfc3a8a282daf490a1ee649ce9fe74382d4fd35` | No tracked modifications; only untracked `changes/active/wyrd-ui-live-data/mocks/`. Committed migration evidence and untracked canvases are separately identified below |
| OpsML `/Users/stevenforrester/Documents/GitHub/opsml` | clean branch `rebrand`, HEAD `69936f4f9e1870e69fc8c9551e658b27355dc3fd` | Working implementation reference; actual code wins over its stale embedded-UI documentation |

Destination has no `.codegraph/`; code searches therefore used ordinary source
inspection. Three bounded read-only scouts inspected main launch/auth, OpsML,
and historical domain implementation. No builds, downloads, process launches,
runtime tests, or visual rendering were run. Test source is evidence of authored
coverage, never evidence that historical tests pass on current main.

Below, `UI/` abbreviates `crates/wyrd/wyrd-server/wyrd-ui/` in the specified
Wyrd commit. `CLI/` abbreviates `crates/wyrd/wyrd-cli/`. Lines are pinned to
these commits, not promises about future implementation line numbers.

## Main: existing production BFF and missing delivery closure

| Capability | Inspected owner/symbol | Baseline and precise gap |
|---|---|---|
| Production frontend | `UI/svelte.config.js`, `UI/package.json` | adapter-node, production Node process; Kit 2.70.3, adapter-node 5.5.7. Not a static/embedded app |
| Runtime toolchain | `mise.toml:20–23`; UI package and frozen lock | Node 26.2.0, pnpm 11.4.0 pinned. UI has no `engines` range; minimum production Node is not established by dependency ranges. Use the pinned Node 26.2.0 toolchain as the initial supported system runtime rather than inventing an older minimum |
| Session and login | `UI/src/lib/server/auth/browser-sessions.ts`: `BrowserSessions` :138, `begin` :168, `complete` :190, `read` :297, `logout` :314; `UI/src/hooks.server.ts:14–31` | Real PKCE/session/API-key sign-in already exists; domain mock data does not make login a mock |
| Credential protection | `browser-sessions.ts:33–41`, `BrowserSession.api:106–118`; `hooks.server.ts:11`; `auth/upstream.ts:17` | HttpOnly/SameSite=Lax, Secure for HTTPS, no-store; credentials remain server-side; existing `WYRD_SERVER_URL` selects Rust upstream and refuses non-HTTPS except loopback |
| Installer | `CLI/src/server/install.rs`: `ServerInstaller` :48, `InstalledServer` :65, install :220, extract :432 | Compatible stable release selection, signed checksums, staging/atomic activation exist. Extraction validates only Rust executable; comment implying UI exists is stale |
| Dev lifecycle | `CLI/src/server/dev.rs`: `DevServer` :43, `DevArgs::run` :74, command :182, run :211, start :241, stop :388 | One Rust child only. Missing UI prerequisite/payload check, UI process, readiness and teardown closure |
| Bind configuration | `dev.rs:33–41`, `local_binds:580–604` | Args `--tenant`/`--host`; existing Rust `WYRD_SERVER_BIND`/`WYRD_GRPC_BIND` loopback validation. No native UI setting; `WYRD_UI_PORT` at `mise.toml:1323–1343` is currently a Vite development setting, not an installed production interface |
| Build | `scripts/build.sh`: bundle :28, image :41 | Exactly one tracked `scripts/build.sh`; separate worktree copies are the same repository path. Bundle currently copies Rust only; image requires Linux |
| Containers | `docker/official/{ubuntu,debian,rocky}/Dockerfile`, `extras/entrypoint.sh`, `extras/nginx/nginx.conf.template` | Rust + nginx, no Node/UI; nginx sends HTTP to Rust. Extend current startup/release ownership |
| Release | `.github/workflows/release.yml:459–548`, :550–635 | Native bundles and Linux images are distinct. Existing native server jobs cover arm64 Darwin, x64/arm64 Linux and Rocky x64. `install.rs:401–410` rejects x64 Darwin. The caller subsequently confirmed Intel macOS is unsupported, so this is not a delivery gap; do not add that target |

### Resolved callback/origin constraint

`BrowserSessions.begin` constructs `/auth/authorize` at the BFF's browser origin
and `{that origin}/login/callback` as redirect URI (`browser-sessions.ts:141–180`).
Rust compares redirect URI exactly with `RequestOrigin + /login/callback`
(`crates/wyrd/wyrd-server/src/auth/authorize.rs:90–96`) and derives the provider
callback from that origin (:138–140).

`RequestOrigin::from_parts` (`src/auth/mod.rs:45–77`) reads the first forwarded
host/proto, then Host/authority fallback, and validates host structure.
`BrowserSession.api` already supplies public host/proto. No Svelte `/auth/*`
route or Vite auth proxy currently closes native browser routing.

**Design conclusion for approval:** keep native separate loopback Node/Rust
ports; add the missing same-origin browser auth forwarding in the BFF. Preserve
path/query/body, redirects and Set-Cookie without following external redirects;
establish trusted public host/proto from the UI request rather than accepting a
caller-supplied forwarding chain. Direct navigation to the Rust port with the
BFF callback would fail current exact-origin checks. No auth-check relaxation,
session redesign, native nginx prerequisite, or new origin-setting surface is
needed to solve this gap.

Historical nginx already supports a single public container origin:
`wyrd-server-ui:docker/official/extras/nginx/nginx.conf.template` routes
`/v1`, `/auth`, `/platform`, `/mcp`, metadata and health to Rust, remainder to
Node :3000, carrying public host/proto. Reconcile this with main's current
HTTP/gRPC routes; do not copy obsolete route inventories blindly.

## OpsML: actual working owners and caveats

All paths in this table are in OpsML commit `69936f4f`.

| Finding | Actual source |
|---|---|
| Node production runtime | `crates/opsml_server/opsml_ui/svelte.config.js:1–12`; `package.json:7–12`: adapter-node builds `build/`, starts `node build/index.js` |
| Production dependency packaging | `mise.toml:293–311`; `.github/workflows/build-assets.yml:187–198`, :278–297: build then production install; UI zip includes build, node_modules and package metadata, no Node executable |
| Native downloads and launches both | `crates/opsml_cli/src/actions/ui.rs::start_ui:123–174`, `download_ui_package:362–414`, `execute_ui:422–454`: Rust platform archive plus UI zip; runs system `node --version`, then Node |
| Native BFF and Rust separate ports | `execute_ui:417–454` uses UI :3000; `crates/opsml_server/src/main.rs:20–23` Rust default :8080; UI `src/lib/server/api/opsmlClient.ts::getBaseUrl:17–20` calls backend server-side |
| Native stop owns two processes | `actions/ui.rs::stop_ui:177–220`; Rust spawn/PID recording :489–501 |
| Official image supplies runtimes | `docker/official/ubuntu/Dockerfile:10–16`, :27–32 installs Node 22/nginx, copies UI production deps/build and Rust; Debian/Rocky equivalents inspected |
| Container launches three processes | `docker/official/extras/entrypoint.sh:97–138` launches Rust, Node :3000, nginx external default :8000 |
| nginx split | `extras/nginx/nginx.conf.template:56–64`, :76–123: `/opsml/api/` → Rust; UI/BFF remainder → Node, with host/proto/cookies |
| Separate release outputs | `.github/workflows/build-assets.yml:63–73`, :216–255, :300 onward: Darwin/native archives and Linux Docker jobs are distinct |
| Login BFF | UI `src/routes/opsml/user/login/+page.server.ts:10–44`; `src/lib/server/auth/validateToken.ts::loginUser:47–59`, cookie setter :70–92; `hooks.server.ts:45–84` bearer/refresh handling |
| SSO BFF | UI `src/routes/api/user/sso/+server.ts:10–18`, callback :9–22; `crates/opsml_auth/src/sso/providers/default.rs:24` reads configured callback |

OpsML's embedded-UI documentation contradicts its code. Reuse its distribution
and BFF pattern, not its auth semantics or defects: Node check only tests spawn,
not exit/version; Rust starts before that check without rollback; nginx hardcodes
Rust :8080 despite configurable server port; startup failures can skip cleanup;
cleanup returns 0 even after child failure. Native direct spawn assumes Node's
default port. These caveats justify Wyrd-owned prerequisite, readiness, failure,
exit-status and cleanup proof; they do not justify bundling Node or a new process
framework.

## Historical migration inventory

Historical column refers to commit `7bfc3a8a`. Main column refers only to committed
`d3ce5062`. A named historical test location records source coverage, not a
verified main command. Replacement tasks must inspect exact selectors before
naming tests as runnable proof.

| Surface | Main baseline | Historical work, callers and coverage evidence | Reconciliation |
|---|---|---|---|
| Shell/navigation | Existing SvelteKit shell/theme | Existing route/layout and compact-shell work; `Main.dc.html` reference | Preserve Evidence Thread; review shell independently |
| Cards inventory | `UI/src/lib/server/wyrd.ts:325–333` mock-only | `WyrdClient.cards:652`; production-auth integration :1154 | Registry paging, UID/current kinds; first registry consumer owns required integration |
| Exact detail | `WyrdClient.card:335` mock-only | `WyrdClient.card:724`, exact UID/by-ref/version reads | Current metadata, identity, relationships and artifact contracts |
| Data detail | Generic Card layout | Generic `liveCard`, production-auth integration :1438 | No historical dedicated Data workspace to recreate |
| Agent | No dedicated workspace | `features/cards/workspaces/agent/AgentWorkspace.svelte`, `card-live.ts::liveAgent:244`; integration :1474 | Separate visual outcome; current Prompt identity, execution_route, bindings |
| Prompt | No dedicated workspace | `workspaces/prompt/PromptWorkspace.svelte`, `livePrompt:302`; integration :1514 | Separate visual outcome; current portable/native forms |
| Model | No dedicated workspace | `workspaces/model/ModelWorkspace.svelte`, `liveModel:330`, `WyrdClient.artifacts:835`; integration :1396 | Separate visual outcome; current interface/signature/artifacts |
| Service | Existing workspace and tests | `WyrdClient.service:851`; definition/overview changes; integration :1570 | Extend existing workspace, exact components/bindings |
| Card evidence | No production analytical connection | `CardEvidence.svelte`, `WyrdClient.evidence:816`, `card-live.ts::cardEvidence:144`; `component-workspaces.test.ts` | Integrate at actual analytical consumer; historical verification SQL is obsolete |
| Query | No Query routes | `features/query/reads.server.ts::runQuery:48`, `readCatalog:73`, `describeTable:90`; tenant query routes; integration :2644, :2703, :2752, :2797 | Existing Bifrost facade, terminal-safe reads and bounded display |
| Traces | Fixture-only observe access | `WyrdClient.observeTraces:541`, `observeTrace:582`, `observe-live.ts`; integration :1734, :1845 | List/detail one trace workflow, separate from logs/metrics |
| Logs | Fixture-only | `WyrdClient.observeLogs:448`; integration :1945 | Current log schemas and bounded reads; independent checkpoint |
| Metrics | Fixture-only | `WyrdClient.observeMetrics:497`; integration :2044, :2142 | Current metric shapes/time bounds; independent checkpoint |
| Observe summary | Fixture-only | `WyrdClient.observe:407`; integration :1645 | Current signal/results semantics; own page checkpoint after consumers |
| Home summary | `WyrdClient.home:341–349` mock-only | `WyrdClient.home:899`, `home-live.ts`; integration :982 | Current counts/verification and partial-read states; own checkpoint |
| Verification/Drift | Existing mock UI/Verifier workspace | `observeAccess:391–395` rejects live; `observeEvals`, `observeEval`, `observeDrift:620–633` remain mocks; Verify canvases | Generic verification needs current contract work, not a claimed historical live port |
| Changes | Mock-only | `WyrdClient.changes:182–184` rejects live | Reference-only; no new live capability promised |
| Review seed | No equivalent production claim | `auth/review-seed.integration.test.ts`, `service-sim.ts`: twelve simulated Services via SDK | Old evaluation contract; do not introduce a shared seed/environment controller |

Historical `BrowserSession.query` (`auth/browser-sessions.ts:162–200`) dynamically
imports `@wyrd/sdk`, uses Bifrost and validates terminal metadata. Historical
`BrowserSession.api:121` and `server/generated/wyrd-http.d.ts` serve live reads.
None is proof that analytical clients/generated HTTP integration already exist
on main or belong in TASK-001. Current workspace discovery already has
`workspace-registry.ts::buildRegistry:35`, `workspaces:56`; main owns Service and
Verifier workspaces. Extend those owners rather than replace discovery.

### Contract disagreements resolved against main

- Historical `features/cards/core/types.ts:8–24` includes Workflow among 15 kinds.
  Main `crates/wyrd-spec/src/envelope.rs::CardKind:477`, `registrable:540` and
  architecture establish 14 native registrable kinds. Exclude Workflow workspace
  and kind, Tool/Skill/SubAgent kinds, and External registration.
- Historical `card-live.ts:61–65` queries `subject_card_uid` as subject and treats
  `card_uid` as Verifier. Current generic results use subject `card_uid`, exact
  `verifier_card_uid`, and `execution_id`; optional application `run_id` is distinct.
- Historical `observeAccess:392` uses `evals:read`; do not revive it as current
  verification authorization. Main's Verifier/binding/requirement/outcome and
  canonical table contracts govern. Home/Card/Service result meanings require
  reconciliation even when layouts and section-state handling are useful.

### Caller correction to platform authority

On 2026-10-19 the caller explicitly replaced the older repository rule requiring
all four OS/architecture combinations. Required native binaries are macOS
aarch64, Linux x86_64, and Linux aarch64. With the caller's subsequent Rocky
clarification, required OCI images are Linux Debian x86_64/aarch64 and Rocky
x86_64 only. Intel macOS is unsupported; Ubuntu images
are outside this selected matrix. These are requested outcomes, not a claim
that committed main or OpsML already provides every image variant. Owning
platform/release documentation must be reconciled during implementation.

The old four-target rule is at `AGENTS.md:74–77`; the older image matrix is at
`architecture/operations/deployment-and-release.md:215–227`. Current installer
targets already match the corrected native matrix (`CLI/src/server/install.rs:401–410`).
Main's release bundle/image matrices omit Rocky aarch64
(`.github/workflows/release.yml:463–475`, :564–585), consistent with the caller's
clarification; this is not a gap and no Rocky aarch64 job is required. Tags and
the startup image owner still default to Ubuntu (:660–668;
`scripts/server/startup-common.sh:55`), which must be reconciled with the selected
Debian/Rocky matrix. OpsML stable release likewise omits Rocky arm64
(`.github/workflows/build-assets.yml:374–389`). OpsML is a packaging-pattern
reference, not implementation authority.

Current main's precise drift shape is `VerifierSpec` → `Requirement.check` →
`Check::Drift(DriftCheck)` (`crates/wyrd-spec/src/card/verifier/mod.rs:50`,
:320–356, :402–407). There is no authored Verifier implementation discriminator
(`architecture/wyrd-design.md:1167–1169`). Drift is a check subtype within the
generic Verifier, not a separate Card kind. `CardKind` has no Eval, Evaluation,
Drift, or Workflow variant (`crates/wyrd-spec/src/envelope.rs:477–509`).

## Verification owners and known proof gaps

These are inspected existing commands, not executed results:

| Owner | Repository command and scope |
|---|---|
| Real UI identity | `mise run test:identity:journey` provisions/seeds Keycloak/Dex, builds UI and drives production BFF. After its repository-owned setup, focused existing test: `mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'` (`mise.toml:706–831`) |
| Native first run | `mise run test:local-dev:journey`; focused existing test: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'` |
| Installer stable selection | `mise exec -- cargo nextest run --locked -p wyrd-cli --test server_install -E 'test(=server_install_selects_latest_stable)'` |
| Installer failed replacement | `mise exec -- cargo nextest run --locked -p wyrd-cli --test server_install -E 'test(=server_install_preserves_previous_on_failure)'` |
| Linux image startup/routing | `mise run test:server:startup:dev`, `mise run test:server:startup:prod`; `startup-common.sh:54–58` builds with `scripts/build.sh image`, records image ID/source revision |
| UI package | No dedicated main UI mise check/unit lane. Existing scripts: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`, same prefix `test`, `build` |

The existing identity Rust harness (`identity_ui_e2e.rs:115–136`) starts two real
Node BFF builds with `HOST`, `PORT`, `PROTOCOL_HEADER`, `HOST_HEADER`, and
`WYRD_SERVER_URL`. Its TypeScript driver
`production-auth.integration.test.ts:48–57` manually sends `/auth/*` to Rust.
It therefore does not prove actual native BFF forwarding or nginx routing.
TASK-001 must close that gap within the existing identity/deployed-path owners,
without creating test-target/filter environment variables. Installer fixtures
(`CLI/tests/server_install.rs:47–50`) currently make an empty `ui/build`; they
do not prove a runnable package. Extend installer fixture expectations and
prove actual Node behavior in its owning Node/runtime journey.

Native release artifacts require their own OS/architecture evidence. Linux OCI
startup tasks neither prove Darwin installation nor require Docker image builds
on macOS. Keep all runtime proof in the appropriate language/process owner.

## Visual assets preserved

`references/canvas/archive/` contains `Agent`, `Home`, `Logs`, `Main`, `Metrics`,
`Model`, `Prompt`, `Query`, `QueryChart`, `TimePicker`, `Verify`, `VerifyDrift`,
and `VerifyResult` HTML canvases plus `canvas.json`. All 14 files were
byte-identical to historical worktree **untracked**
`changes/active/wyrd-ui-live-data/mocks/project/` files. They are not committed
historical capabilities. They were moved from `references/canvas/` to
`archive/` unchanged when the approved revision-18 canvas replaced them; their
hashes still match `preserved-files.json`.

`references/canvas/` now holds the approved revision-18 canvas (23 boards,
`.dc.html` plus rendered `png/`), described in its README.

Additional committed visual source locations include `UI/brand/DESIGN.md`,
`UI/brand/renders/styleguide.html`, `UI/brand/renders/product/**`, and historical
`changes/active/wyrd-ui-foundation/references/change-impact-map.html`; that map
labels itself a reference mock requiring spec adoption. Current brand authority
governs implementation. Canvas features do not implicitly add migration scope.

The brand system and change-impact map already exist on committed main.
Historical product renders differ in 23 files (README plus 22 SVGs, approximately
4.3 MB); `golden-O-09.svg` is a historical addition. These variants are copied
byte-for-byte into `references/historical-brand/`, with original source path,
commit, and SHA-256 in `references/historical-brand-manifest.json`. The unchanged
styleguide/design/map remain linked to existing tracked owners. Copying these
reference assets does not change the application's brand implementation.

The original spec and ten tasks now live under `archive/invalid-draft/`, with
their bytes unchanged. `references/preserved-files.json` records all 25 original
files and their SHA-256 hashes. Archived `approved` claims are invalid and must
never be used as execution authority.
