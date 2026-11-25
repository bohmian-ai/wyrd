# TASK-001 first implementation review
Repository /home/thorrester/Documents/GitHub/wyrd; immutable checkout /tmp/wyrd-task001-review-dd98.
Base 7b36f8fc6; candidate dd98e9ef78d20e7de47e7af623644fed0ff9075a. Complete base...candidate diff. Original task at candidate. No prior formal review or carried acceptance.
User says ignore BLOCKED: assess implementation independently of status label; distinguish missing visual acceptance and aggregate failures from source-proven defects. No implementation edits.
Authority: root AGENTS.md, changes/spec-driven-development.md, agent_docs/maintainer-style.md, applicable architecture, UI skill and brand authority. No nested AGENTS under changed paths (inventory only found SQL under crates). Approved working-tree spec revision 2 is authority; committed copy says draft. Differences only approval/scope-transfer recording, no behavioral change. Other dirty source excluded, including brand/components.json contracts.

## Navigation map
- verifier.ts definitionGraph/readVerifier/loadVerifierDefinition/loadRunsOn: exact Card -> projected graph and inbound owner bindings through Context/existing executors.
- verifier-layout.ts layoutDefinition -> VerifierGraph/Node/Edge.svelte; compare existing graph/layout owners and callers.
- VerifierDefinition.svelte/RunsOn.svelte -> selected declarations and binding rows.
- schemas.ts/loaders.ts/catalog.json/index.ts/WyrdView.svelte -> portable catalog, Node/MCP/BFF/custom consumers.
- cards-host.ts/cards/detail route -> exact version, existing header/tabs and shared components.
- vite.app.config.ts -> built MCP environment substitution; manifests move xyflow/add approved Dagre.
- ui-fixtures.ts/_gallery and ui-library/ui-pages unit/integration -> topology, interaction, host and browser proof. vitest-setup.ts ResizeObserver.
- scripts/dev/example-app.sh -> public setup Service bindings.
Map is starting point only; reviewers independently expand source coverage. Reviewers read actual diff, not completion summary. Candidate task Evidence is verification claims to falsify.

## Contract and specialist applicability
|Contract/invariant; owners/producers/consumers|Role and reason|
|Exact topology, read-only interaction, versions and states; VerifierSpec/Card -> projection/layout -> components/page|behavior-rev mandatory comprehensive first review|
|New graph projection/layout/binding loader ownership; portable package -> catalog/page/Node/MCP|maintainer-rev selected: substantial new abstraction/ownership, compare existing full workflows|
|Cross-host contracts/exports/build/dev/test rules; package schemas -> MCP artifact/BFF/example/dev|repo-rev selected: broad cross-package delivery and contracts|
|Scoped exact Card and inbound reads; readVerifier/loadRunsOn -> executors -> viewers|domain-rev security selected: new multi-Card read orchestration and denial prerequisite handling|
|Shared state/lifecycle; local selection plus existing stale panel load owner|invariant-rev omitted: ephemeral UI state, no new shared lifecycle/cache/concurrency owner; behavior covers superseded load|
|Failure/recovery; existing read failures -> component states, built browser artifact|system-rev omitted: no process/deployment/retry/recovery changes; behavior/repo cover errors and mount|
Independent validator must recheck selection and coverage. Reports only here; source immutable.

Approved working-tree spec SHA-256: ab12f59e6c2582e711ca4735f1933a1c15f54f3ab33c3c97f7bf0d12e7708e84
