---
id: SPEC-agent-readiness
revision: 2
status: approved
---

# Agent readiness

## Objective and approval

A person points a coding agent at the Wyrd docs and/or connects the Wyrd
MCP. With shell and file access, the agent carries out Declare -> Observe ->
Verify on that person's application without the person explaining Wyrd:

1. Declare a Service in YAML with its Agent, Prompt and Model components and
   exact Verifier bindings, validate it, and declare it to the server.
2. Pull the Service bundle, load it into the application through a first-class
   SDK, emit correlated telemetry and custom data, and invoke verification.
3. Obtain synchronous judgments and asynchronous terminal judgments, and
   inspect their attributable evidence on the server.

The owner fixed the surface roles, vocabulary, foreground development lifetime,
three-target platform scope, local-only check and opt-in live acceptance
scenario in the 2026-10-18 requests. The owner approved revision 2 on
2026-10-18. There are no open material decisions or implementation tasks in
this document.

Wyrd remains useful to humans through the same documented contracts. Agent
readiness requires discoverable, executable workflows and server evidence,
not familiarity with Wyrd internals or an agent's assertion of success.

## Existing owners and authority

The sweep found the application capabilities already present. The shared
`wyrd-client::cards::Cards` handle owns declaration orchestration and graph
download; `wyrd-loader` resolves authored files; `WyrdState`, Run and Observe
own client-side bundle loading, application correlation and calls to existing
verification endpoints. Rust, Python and TypeScript SDKs project these shared
owners. Registry writes, identity, authorization, storage and durable behavior
remain server-owned. Vala owns Verifier execution and the Bifrost evidence
substrate; this change does not replace either engine.

`DevServer` already installs a verified server, migrates, provisions, saves
protected credentials and endpoints, proves fresh-client access and supervises
a foreground child. `ServerInstaller`, `GlobalConfig` and `CredentialsFile`
remain the installation and local-persistence owners. The person or live
test harness owns that foreground lifetime; the coding agent connects to the
running server. The gap is documented lifetime guidance, not new process control.

The server MCP already reads Cards/evidence and executes or queues Run
verification. Its instructions and four guide topics currently teach
investigation. CLI Card operations have JSON output, but setup commands and
error streams do not provide a consistent machine contract. Documentation
contains substantial authoring and SDK guidance, but omits visible telemetry
initialization, changes names between recipes and leaves quickstart results
unverified. `llms-full.txt` currently contains another index, not page bodies.

Existing real-server SDK support-desk journeys and installed-CLI local-dev
journeys remain valuable proof. They do not establish autonomous onboarding
from the published quickstart: the SDK journeys exercise `examples/support-desk`,
whereas the newer quickstart embeds `examples/support-desk-app`.

The owner explicitly supersedes the four-target statement in `AGENTS.md` §2
with Linux x86_64, Linux arm64 and macOS arm64. The portability prohibitions
remain, as does `SPEC-local-dev-setup`'s foreground-only contract. Once
approved, the platform authority correction must land with its dependent
behavior. Nothing has shipped: this change replaces declaration and bundle contracts in place,
without compatibility aliases, data migration or mixed-version support.

## Scope and non-goals

In scope: readiness findings AR-1, AR-2, AR-3, AR-4, AR-5, AR-6 (local-development
lifetime docs only), AR-8, AR-9 and AR-10; vocabulary findings AR-11 through
AR-17; the three-target authority correction; CLI setup and Card machine contracts;
MCP instructions and `wyrd.guide` topic `start`; docs and agent reference
generation; first-class SDK discoverability; in-repository examples and
deterministic journeys; and the standalone owner-invoked live lane.

Out of scope: AR-7's Intel macOS installer expansion; MCP-only agents without
shell/file access; MCP declaration or upload tools; MCP application telemetry
or custom-row ingestion; detached lifetimes, new server stop commands or
service-manager behavior; a new application runtime, verification engine,
credential framework or evaluation
platform; automatic invocation of live coding agents by CI or `gate`; and
compatibility aliases, historical-data conversion or migration tooling.

Sibling standalone example repositories are an explicit follow-up outside
this change. In-repository code and docs must still be accurate and verifiable
without claiming those sibling repositories have been updated.

## Required behavior

### REQ-001 One discoverable Declare -> Observe -> Verify workflow

The docs entry point, setup, quickstart, Declare/Observe/Verify guides, CLI
help and MCP guidance teach the same sequence using only the final names.
Starting with CLI acquisition, PostgreSQL prerequisites, `wyrd server dev`
and `wyrd mcp install`, they explain required credentials/configuration,
explicit noninteractive inputs, server lifetime, host reload/reconnect and
the next command or SDK operation. They do not imply unavailable packages
or images are published or require a Wyrd checkout to run an installed server.

For local development, the person runs `wyrd server dev` in its own terminal
or as a background job they own, and waits for its readiness result. The
coding agent then connects to the already-running server through MCP and CLI.
The person remains responsible for the server's lifetime; no detached CLI mode
or new process-management contract is introduced.

The path teaches agents to map an application's components to Service YAML,
author and bind Verifiers, validate local files, declare heavy lineage anchors
through their existing workflow before referring to them, declare the Service,
read its exact identity, pull and load its bundle, instrument the application,
and inspect results. A gateway deployment/model string is not presented as a
Model Card. Auth and card-bound workload credential issuance remain explicit
where required; declaring a Card does not return a secret or grant arbitrary
authority. Repeating declaration preserves existing idempotency semantics.

Every runnable continuation uses the same declared aliases, table names,
Verifier names and input slots. Placeholder examples identify what must be
replaced and where to obtain it. The own-application path explains adaptation
instead of requiring an agent to clone the support-desk app unchanged.

### REQ-002 Declare and pre-execution check

`wyrd declare PATH` replaces `wyrd apply PATH`. `wyrd declare PATH --check`
replaces `wyrd plan PATH` and is the documented pre-execution validation and
lint step. Both accept a Card file or directory and `--format json`.

The check is purely local and makes no server contact, even when a server or
credential is configured. It performs no upload, principal provisioning or
version assignment. It validates the referenced files on disk, including transitive
Card paths and declared local artifact/source files, using the existing
loader, configuration rules and owning validators. Invalid envelopes, missing
or unreadable files, invalid references, cycles and unsupported declarations
produce structured diagnostics with stable codes, locations and remediation.
Valid input returns the declaration candidates and nonfatal diagnostics;
failure returns nonzero. Local success does not claim that remote identities
exist, authorization will succeed, or final versions have been assigned.

No MCP check tool is added. MCP guidance directs coding agents to the CLI
check; the server validates references authoritatively at declaration.

### REQ-003 Hard declaration rename across all projections

Use declare for Card declaration on every surface, retaining registry as a
noun. The rename covers generic and per-kind SDK registries, path and progress
variants, public CLI embedding/testing APIs, native bindings, authored Python
stubs, Rust exports, TypeScript declarations and generated types. In idiomatic
language spelling, `register`, `register_from_path`, `registerFromPath` and
`register_with_progress` become `declare`, `declare_from_path`,
`declareFromPath` and `declare_with_progress`.

Declaration contract names replace their registration/create operation names:
`CreateCardRequest/Response` become `DeclareCardRequest/Response`;
`RegistrationReceipt`, `RegistrationReplaySeed`, `RegistrationOperationId`,
`RegistrationInput`, `RegistrationPhase` and registration progress types use
`Declaration*`. `CardRegistrationOutcome` becomes `CardDeclarationOutcome`;
`RegistrationOutcomeKind::Registered` becomes
`DeclarationOutcomeKind::Declared`, serialized as `"declared"`.
Declaration-specific builders, descriptions, diagnostics and error catalog
language use the same vocabulary. `POST /v1/cards` keeps its path.

Persisted operation vocabulary changes in place: `card_registration_operations`
becomes `card_declaration_operations`, `registration_operation_id` becomes
`declaration_operation_id`, and associated constraints, indexes, RLS, FKs,
queries and schema sources agree. Stored replay seeds project the renamed
types and outcomes. Audit declaration types and discriminators use
`CardDeclaration*`, `Declare`, `"declare"` and `"card_declaration"` where
they currently identify registration. Distinct outcomes such as audit
`Created`, `IdempotentNoop`, `Deduplicated`, and operation/Card lifecycle
states keep their meanings; this is not a global text replacement.

No old spelling is exposed as an alias or accepted alternate wire/stored
value. Update current architecture, docs, tests, fixtures, examples and
generated projections together, including every stale `wyrd apply -f` in
`architecture/wyrd-design.md`. Historical change records may retain historical
names; unrelated provider/tool/module registration is not Card declaration.

### REQ-004 Get one envelope; pull one graph bundle

`get` reads one Card envelope, without downloading artifacts or requiring an
output directory. `wyrd get` prints that envelope; `--format json` prints its
canonical JSON projection. Preserve exact-reference and kind-qualified UID
lookup through the existing typed selector rules. Missing, ambiguous or
inconsistent selectors fail with actionable structured errors rather than
silently selecting another version.

Merge MCP `cards.get` and `cards.get_by_ref` into `cards.get`. Its typed input
accepts either the exact CardRef selector or the kind-qualified UID selector,
rejects ambiguous combinations and preserves both capabilities, authorization
and full-envelope output. Update the guide, catalog, server-owned tool
references and callers; retain no `cards.get_by_ref` alias.

`cards.pull` and `wyrd pull` replace graph-download `hydrate` and
`wyrd get --output-dir`. The CLI retains an explicit `--output-dir` for pull.
Complete is the default: success produces the exact root's transitive bundle,
manifest, alias/reference projections and integrity-verified artifact bytes
consumed by the existing SDK state loaders. A failed/incomplete pull is not
reported as a runnable bundle. Retain the explicit `--metadata-only` capability
as inspectable and non-runnable; state loaders continue to reject it for
runtime use. Get never becomes an alias for pull.

Public graph-download types use pull/bundle vocabulary: `PullSummary`,
`BundleMode`, `BundleManifest`, `BundleCardManifest` and
`BundleArtifactManifest` replace the corresponding hydration summary, mode
and manifest types. Root `metadata.yaml` uses `apiVersion: wyrd/bundle/v1`
and `mode` instead of the hydration-named discriminator and field. Mode
values remain `metadata` and `complete`. SDK projections and graph-download
errors use the final vocabulary. Readers/writers change together in place,
without accepting or converting the old bundle format. Runtime object
hydration and bundle loading are distinct from graph download and retain
their appropriate concepts.

Remove public `wyrd load` and its CLI embedding/testing projection. It is a
one-Card artifact materializer, not a graph bundle operation. Necessary
internal materialization and SDK state loading remain; neither is a pull
compatibility alias or a new public command.

### REQ-006 Consistent machine results and noninteractive recovery

Provide `--format json` for the required setup and lifecycle commands:
`server install`, `server dev`, `mcp install`, `declare` (including `--check`),
`get` and `pull`.
Retain existing machine output for adjacent Card lifecycle commands. Success
returns one typed JSON result on stdout. In JSON mode, progress and diagnostic
logs use the diagnostic log destination rather than either machine stream.
For foreground startup, emit its readiness result when ready, without waiting
for termination, and retain its foreground exit behavior. A later failure is
an error, not a second successful result or a retraction of earlier readiness.

In JSON mode a failure before completion/readiness produces no success on
stdout. Failures, including a foreground failure after readiness, emit one
structured error document on stderr without appended prose or interleaved progress.
Argument/usage failures follow the same machine-error contract when JSON was
requested. Errors preserve the derive-backed Wyrd code, safe detail, relevant
field/resource or startup stage, and actionable remediation; multiple local
validation diagnostics may be carried as typed error details. A failed
operation returns nonzero. Human mode remains concise and actionable.

Without a usable terminal, or with explicit noninteractive mode, required
commands never wait for a prompt. Missing inputs, unavailable credentials,
unknown hosts, occupied endpoints and invalid configuration identify the
safe next action. MCP installation requires explicit host selection when
unattended, remains repeatable and writes no credential into host config.
The documented host connection uses the existing proxy and saved-client
credential resolution, including its independent protocol negotiation.

### REQ-007 MCP teaches onboarding and retains verification ownership

Server `instructions` direct unfamiliar coding agents to
`wyrd.guide {"topic":"start"}` and current `tools/list`. Add `start` to the
typed guide topic contract. It is bounded static onboarding guidance,
available through the existing authenticated read surface, containing no
tenant data and granting no permission.

The topic teaches the complete final-vocabulary workflow, with CLI commands,
SDK/doc entry points, required credentials and setup, local `declare --check`,
get versus pull, instrumentation and shutdown, sync/async verification, and
server-evidence completion checks. An agent starting with MCP alone can learn
where to obtain and follow the full docs. Retain the existing investigation
topics, updated to the final names and precise semantics.

MCP inspects Cards/evidence and runs existing `verification.execute`,
`verification.enqueue` and execution reads under existing scopes and exact
Verifier authorization. It does not declare Cards, pull local bundles, upload
files, instrument the application or ingest its telemetry/custom rows. No
MCP declaration or upload tool is added. Explain file evidence preparation
through existing SDK/HTTP paths; a local path in the coding agent's workspace
is not a server-readable file. Existing evidence-planning behavior is not a
file-byte upload tool.

### REQ-008 Complete SDK Observe and Verify recipes

For Rust, Python and TypeScript, public docs, SDK documentation/stubs and
in-repository examples show how to pull and load the declared bundle, obtain
the correct Card views, start Bifrost and telemetry, attach applicable
framework instrumentation, emit correlated observations and schema-valid
custom rows, verify, submit, resume/poll and flush/shutdown. Required package
extras and framework hooks are visible in the onboarding path, not hidden
outside embedded snippet regions. SDKs do not discover/patch frameworks or
take over application execution.

The published quickstart path executes as documented. Embedded Rust/TS code
is checked against the same public SDK contracts and Service vocabulary even
where the runnable quickstart uses Python. Clearly identify reference copies,
their executable validation and runnable destinations; do not make sibling
repo updates a prerequisite for learning a missing SDK operation. Explain
heavy Model components without bypassing their existing declaration rules.
Keep the documented support-desk path and actual tested example aligned.

Distinguish buffered record admission, transport acknowledgment, analytical
visibility, synchronous verification and queued execution. `verify` awaits a
Run-mode judgment within the existing server deadline and returns a failed
verdict as a result; it dispatches no Operator. `submit` returns a handle for
Run-mode queued work, whose staging acknowledgment is not durable acceptance
or a completed judgment. Explain delayed visibility, bounded waits, terminal
execution errors and the existing abrupt-loss contract. Window submit records
samples and returns Recorded; it is not a Run execution handle. Schedule is
the existing automatic activation; observation arrival alone does not run
arbitrary checks. Preserve exact subject/Verifier attribution and the distinction
between an application Run and a verification execution.

### REQ-009 Agent references, errors and honest published evidence

`llms.txt` remains a compact navigable index with the canonical start route.
`llms-full.txt` contains readable page bodies for the published docs, including
the actual code/YAML used by embedded onboarding examples, rather than another
index or unresolved component markers. Both are generated from the owning
content/source snippets with working deploy-base-aware links and consistent
order; do not create a second manually maintained recipe.

The error catalog stays source-derived and discoverable from agent entry
points and MCP onboarding. Its names, fields and remediation agree with CLI,
HTTP, MCP and SDK behavior. Help and docs describe actual accepted arguments.
Capture and verify the published quickstart's expected responses and evidence
queries; remove source-derived/unverified qualifications only when execution
supplies that proof. Retain explicit uncertainty for anything not exercised.

### REQ-010 Supported targets and unchanged portability prohibitions

Update `AGENTS.md` §2 to the supported targets Linux x86_64, Linux arm64 and
macOS arm64 (Apple Silicon), and reconcile current normative platform claims
that would still promise macOS x86_64. Do not add Intel macOS release or
installer support. Every capability in this change works on the three
supported targets.

Keep the portability prohibitions: no dependency on cgroups, namespaces,
Landlock, seccomp, `/proc`, `prlimit`, setuid/credential switching,
`sandbox-exec` or similar mechanisms; no feature gated by `cfg(target_os)`
and no platform-specific deployment prerequisite. Use portable Rust or a
dependency's cross-platform facilities. This is a target-set correction,
not permission to weaken existing isolation or process-management guarantees.

### REQ-011 Standalone opt-in live agent readiness lane

Add tracked `mise.toml` task `test:agent-readiness:smoke:live`, following the
opt-in `test:gateway:smoke:live` and `test:operators:smoke:live` precedent.
The owner invokes it occasionally as functionality grows. Credentials and
local settings come through untracked `mise.local.toml`; no real values or
machine-specific authentication files enter tracked configuration. This lane
is excluded from `gate`, all CI dependencies and automatic runs. It does not
replace deterministic verification.

The harness preflights prerequisites, starts and owns an isolated foreground
`wyrd server dev`, and waits for its readiness result before launching either
agent. It builds and serves the docs from the same checkout/commit under test;
agents use that local docs build, not the published site. Onboarding links
used in the lane resolve to the local build without a published-site fallback.
It launches both Claude Code and Codex non-interactively. Each gets a fresh
isolated application directory and host configuration, a running server with
Wyrd MCP connected using `wyrd mcp install`, and the same bounded
application-building objective with local docs and MCP entry points.
The objective is to build and execute a simple application with declared
Service/components and Verifiers, observation and custom data, and synchronous
and asynchronous verification. It supplies no Wyrd-specific step-by-step
solution, prewritten integration, repository-internal plan/skill context or
human clarification. SDK packages/types and public help remain normal
discovery surfaces. Test identities/evidence distinguish the agents and runs;
neither agent can satisfy the other's evidence requirements.

The observer judges success from authorized public reads of the local Wyrd
server: exact declared Service/component Cards and Verifier bindings;
application observations correlated by Card and opaque Run IDs; telemetry and
custom rows; and genuine retained synchronous and queued asynchronous
Verifier judgments for that application. Async success requires a terminal
result, not just an execution ID, 202 response or completed-without-result
state. The expected checks must have an actual passed or failed verdict;
missing/inconclusive evidence is not success. A failed application verdict
may demonstrate working verification and is distinct from harness failure.
Never accept agent prose, fabricated result files or exit code alone as proof.
Runs are evidenced by correlated rows, not a new server Run registry.

Bound agent execution and evidence polling. Report per-agent outcome,
observed milestone/failure point, evidence identifiers and query observations,
transcript location and enough redacted diagnostics to locate the struggle.
Record tested Wyrd/docs and agent version/configuration identities to make a
run interpretable; one stochastic success is not a universal reliability
claim. Missing binaries/authentication or unavailable external services are
explicit blocked/failed outcomes, never skipped successes. The task returns
success only when both agents meet the evidence criteria, and still reports
each outcome when one fails. Retain diagnostic artifacts, settle owned
agent, docs-serving and Wyrd server processes on completion/cancellation,
and never alter the owner's normal host configuration or development data.
Secrets are not prompts, transcripts,
command arguments, summaries or checked-in artifacts.

## Invariants and boundaries

- **INV-001 Surface roles and owners.** MCP teaches, inspects and runs
  verification; CLI declares/pulls and manages local development; SDKs
  instrument user-owned applications through `wyrd-client`. Server/Vala
  retain durable behavior. No parallel loader, transport, verifier engine,
  lifecycle bootstrap or arbitrary-code runtime is introduced.
- **INV-002 Pure contracts and uniform projections.** `wyrd-spec` remains
  IO/async/PyO3-free. Public shapes/errors have one owning source; generated
  outputs come from it and Python authored stubs change with their native API.
- **INV-003 Authorization and secrets.** Preserve authenticated tenant and
  Card scope, RLS, policy, exact Verifier permissions, protected persistence
  and host credential isolation. Discovery/bindings/local validation grant no
  authority. Evaluate permission before acting; stage audit non-blocking on
  the existing Scribe outbox. No alternate audit writer or secret exposure.
- **INV-004 Honest durability and evidence.** No new acknowledgment, query
  completeness, exactly-once or loss guarantee. Preserve Bifrost and outbox
  contracts, replay identities and safe drain. Bounded polling and correlated
  readback establish test evidence; empty/partial/degraded results never imply
  a successful completed workflow. Shutdown does not erase retained data.
- **INV-005 Coordinated replacement.** No compatibility aliases, historical
  migrations, old-name readers or mixed-format deployment promise. Only
  declaration and graph-download vocabulary is renamed; registry nouns,
  unrelated registrations, runtime object hydration and independent lifecycle
  outcomes remain distinct. No reset of existing local data is implicit.
- **INV-006 Development and production separation.** The person or live
  harness starts and owns the foreground development server across the three
  supported targets; coding agents connect to it. Production remains standalone
  `wyrd-server` deployed from a container image on an image-hosting/orchestration
  platform that owns its lifetime. Docs state this distinction and current
  image availability; no new process manager is introduced.
- **INV-007 Evidence lanes.** Deterministic journeys remain required and
  credential-free with local provider fixtures. Live coding-agent evaluation
  is standalone, explicit, bounded and excluded from CI/gate. It creates no
  new persisted product evaluation ontology.

## Acceptance criteria and evidence classes

| ID | Required proof |
|---|---|
| AC-001 | From docs or MCP `start`, a reader can identify the supported setup, credentials, final commands, SDK operations and completion checks for an own-app Service with Agent/Prompt/Model components and Verifiers. Published quickstart execution, executable snippets and source review prove REQ-001/007/008/009 without relying on sibling repo changes. |
| AC-002 | `declare --check --format json` validates referenced disk files and reports located stable errors/remediation for invalid/missing/cyclic inputs, with no server contact whether configuration/credentials are absent or present. No MCP check tool exists. A subsequent real declaration validates references authoritatively on the server, preserves replay/conflict/version semantics and returns the renamed receipt. CLI/server journeys plus focused pure-validation proof establish REQ-002/003. |
| AC-003 | Actual Rust/Python/TypeScript imports and calls, CLI/help/testing APIs, HTTP served OpenAPI, SQL schema/replay responses and audit payloads use declaration names and `declared` where applicable. No old public alias is accepted. Runtime/export/serialization and real Postgres proof cover REQ-003 and preserve distinct lifecycle outcomes. |
| AC-004 | CLI/SDK/MCP get returns exactly one envelope with exact-reference and UID selectors, rejects ambiguous/refused lookups, and downloads no artifacts. MCP catalog advertises only the merged get tool. Real public-surface journeys and runtime catalog/schema checks prove REQ-004. |
| AC-005 | Pull produces a complete integrity-verified bundle that each SDK loads; metadata-only remains inspectable and non-runnable. New metadata types/fields/discriminator agree across readers/writers; failed pulls are not runnable successes. Removed `load` and old bundle/command/API names have no aliases. Real download/load journeys and format fixtures prove REQ-004. |
| AC-007 | Required commands have parseable JSON results and structured failures, including usage errors, missing credentials and denied operations; no prompts occur unattended and no progress/prose corrupts machine payloads. MCP host installation is isolated, repeatable and secret-free, and proxies forward onboarding instructions. CLI/MCP host and error journeys prove REQ-006/007. |
| AC-008 | Real-server Rust/Python/TypeScript journeys load the published bundle, emit custom data and telemetry, flush/shutdown/read back, receive a synchronous judgment and resume an asynchronous terminal judgment with exact Card/Run/Verifier attribution. Cover failed verdict versus execution failure, unknown binding/input, insufficient permission, schema conflict, replay and applicable drain/restart behavior. Existing journeys are retained and the published quickstart is actually exercised; REQ-008/009 and INV-004 do not become stronger durability promises. |
| AC-009 | Agent reference generation contains actual onboarding bodies and embedded snippets with valid links; error catalog/help and Declare/Observe/Verify names agree. Captured execution supports quickstart outputs and any removed Unverified claims. `docs:check`, example/snippet checks and source-driven generation prove REQ-001/008/009. |
| AC-010 | Current platform authorities state exactly Linux x86_64, Linux arm64 and macOS arm64; installer/platform checks agree and relevant CLI/SDK journeys supply evidence for each supported target. Local setup docs explain person-owned foreground/background-job startup and waiting for readiness; no new lifetime contract is introduced. No prohibited platform mechanism or Intel macOS expansion enters the change. Source review and target-specific execution evidence prove REQ-001/010/INV-006. |
| AC-011 | Deterministic CLI, three-SDK, MCP catalog, published-quickstart, served-OpenAPI, codegen, docs and applicable boundary checks pass. This cross-boundary contract/infrastructure change uses `mise run gate` as its final deterministic aggregate; retain focused iteration evidence and run required specialized proof separately only where the aggregate does not cover it. Prove that the live lane has no CI/gate dependency. Codegen does not substitute for runtime MCP or served OpenAPI checks. Report actual verification, not inferred coverage. |
| AC-012 | Culminating acceptance: the owner runs `mise run test:agent-readiness:smoke:live` with credentials/local settings in untracked `mise.local.toml`. The harness starts and owns local foreground Wyrd, waits for readiness, builds/serves docs from the same commit under test, then launches noninteractive Claude Code and Codex in fresh isolated app directories with the running server connected through `wyrd mcp install`. Each builds and executes a simple Wyrd-verified application using only those local docs, MCP and ordinary public tooling. Both independently leave declared Service/components with Verifiers, correlated observed Runs, telemetry/custom data and retained synchronous and asynchronous judgments on that server. Authorized server evidence determines each outcome; the report includes evidence IDs, transcript location and failure point. The harness settles its server, docs and agent processes at the end. Missing evidence or a blocked agent cannot pass. This occasional opt-in live proof follows REQ-011, stays outside gate/CI and supplements every deterministic criterion above. |

## Material decisions and open questions

The owner has fixed coding agents with shell/file access, the surface roles,
no MCP declaration/upload tools, hard vocabulary replacement, removal of
public CLI load, merged MCP get, person/harness-owned foreground startup,
three supported targets, sibling-repository deferral and the live acceptance
scenario using docs built from the commit under test. DEC-001 is resolved:
checking is purely local, without server contact or an MCP check tool; server
reference validation occurs at declaration. No open material decisions remain.
Private layout, helper names and test fixture organization remain
implementation-owned.
Implementation discovery requiring another material contract or boundary change
returns for a specification revision.

## Revision history and sources

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Records the 2026-10-18 agent-readiness sweep and authoritative owner decisions, with one open check-boundary decision. No implementation or approval is claimed. |
| 2 | approved | Removes the detached lifetime and stop requirements; retains person/harness-owned foreground startup and JSON readiness. Requires live agents to use a running server and locally served docs from the commit under test. Resolves DEC-001 to local-only checking with no MCP check tool; no open material decisions remain. |

Authority links: [AGENTS.md](../../../AGENTS.md),
[agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md),
[public doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md),
[security posture](../../../architecture/wyrd-security-posture.md),
[operations](../../../architecture/operations/README.md),
[deployment and release](../../../architecture/operations/deployment-and-release.md),
[reference router](../../../architecture/references/README.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[architecture constraints](../../../architecture/references/doctrine/architecture-constraints.md),
[positioning and vocabulary](../../../architecture/references/doctrine/positioning-and-vocabulary.md),
[architecture patterns](../../../architecture/references/architecture/patterns.md),
[agent harness](../../../architecture/references/languages/agent-harness.md),
[errors](../../../architecture/references/languages/errors.md),
[testing workflows](../../../architecture/references/languages/testing-workflows.md),
[telemetry](../../../architecture/references/domain/telemetry-observations.md),
[evaluation](../../../architecture/references/domain/evaluation.md), and
[local-dev setup spec](../local-dev-setup/spec.md).

Source anchors: `crates/wyrd/wyrd-cli/src/{cli.rs,card.rs,error.rs}`;
`wyrd-cli/src/server/{dev.rs,install.rs}` and `src/mcp/{mod.rs,proxy.rs}`;
`crates/shared/wyrd-client/src/{cards/handle.rs,cards/progress.rs,state.rs,observe/mod.rs}`;
`crates/shared/wyrd-loader/src/lib.rs`;
`crates/wyrd-spec/src/registry/{submission.rs,enums.rs,hydrated.rs}` and
`src/vala/audit_detail.rs`; `wyrd-server/src/components/cards/routes.rs`;
`wyrd-server/src/mcp/{mod.rs,guide.rs,cards.rs,verification.rs}`;
`crates/wyrd/wyrd-sql/{migrations/20260601000014_card_registration_operations.sql,src/queries/cards/register.rs}`;
the three `sdks/` public exports/stubs and support-desk/local-development
journeys; `examples/{support-desk,support-desk-app}`;
`docs/src/content/docs/{get-started,build-and-verify,connect,understand,api}`;
`docs/scripts/generate_llms_txt.py`, `docs/public/llms*.txt`; and `mise.toml`
live-lane precedents and deterministic gate definitions. These are baseline
anchors, not a private implementation layout prescription.
