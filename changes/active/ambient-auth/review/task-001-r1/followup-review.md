# Focused follow-up — external example dependency and MCP boundary

Result: **RESOLVED**. The reports describe the same incomplete external migration, but immutable source supports the remaining MCP consumer as the implementation defect. The stale local package proves an unsuccessful local dependency/proof setup; it does not independently establish a committed dependency regression.

## Subject and source coverage

- Wyrd base `5bcd9e4872f1376f1722922fa63b67384243be04`, candidate `824f0c491ea2686977c9da9fce03ac4ef65c01b1`.
- External repository `/home/thorrester/Documents/GitHub/wyrd-example-ts`, base `bef9fc0`, candidate `b283666df93a32cb73aa8c07f881d9271797dff0`.
- Read the immutable subject, approved spec r3, original task/evidence, `standards-review.md` (STD-001), `maintainer-review.md` (MAINT-001), `task-review-behavior.md` (BEH-001), and `system-review.md`. This follow-up resolves only their external dependency/MCP discrepancy, without revisiting other findings.
- Read task-review authority, agent rules, spec-driven-development and maintainer-style guidance; applicable supplied AGENTS.md rules require completed example callers and honest scoped proof. AC-006 is the decisive approved obligation.
- Used CodeGraph before Wyrd source navigation. External repository has no `.codegraph/`; read its immutable objects with `git show`, `git ls-tree`, `git diff`, and `git grep`.
- Inspected external candidate's complete tracked path list, complete external diff, `src/agent.ts`, `scripts/investigate.ts`, `package.json`, `package-lock.json`, `.gitignore`, `README.md`, and `tsconfig.json`. Searched committed `src`, `scripts`, and `test` for client/MCP/helper consumers.
- Inspected Wyrd's cumulative diff inventory, public `WyrdClient.httpAuth` through CodeGraph, SDK package manifests, `tsconfig.build.json`, and canonical `mise.toml` build/pack definitions at lines 1485–1542. Inspected filesystem link metadata and ignore provenance solely to distinguish local inputs from tracked candidate objects.

## Evidence resolving the discrepancy

| Claim or uncertainty | Source evidence | Resolution |
|---|---|---|
| The external candidate commits a stale SDK projection | External candidate tree contains 17 application/config/service/test files and no `vendor` content or gitlink. `.gitignore:2` excludes `vendor/wyrd-ai`. The complete external diff changes only `src/agent.ts`. | False as a statement about committed SDK contents: there is no SDK projection in that immutable tree. |
| The declared package forces the old SDK version lacking `httpAuth` | `package.json:23` declares `file:vendor/wyrd-ai`. Lock's `node_modules/wyrd-ai` entry is a link to that directory; its `vendor/wyrd-ai` metadata gives version `0.1.0` but no content digest, Git identity, or wrapper source. | The tracked declaration selects a local directory, not the specific stale bytes observed in this session. It cannot establish that every valid dependency supply for this candidate lacks the getter. |
| The currently resolved SDK lacks the new member | Filesystem `node_modules/wyrd-ai` links to `../vendor/wyrd-ai`; `vendor/wyrd-ai` is an ignored symlink to a separate scratch checkout outside both immutable subjects. Discovery reviewers observed its wrapper/declaration lacked the getter and reported TS2339. | Credible current-environment failure and missing successful example proof. Not a second source-proven candidate regression. Those filesystem bytes have no immutable subject identity. No inference about the historical symlink target follows from the committed lock. |
| A built SDK rather than just native declaration regeneration is needed | Wyrd package exports point to `dist/index.js` and `dist/index.d.ts`. `ts:build` runs N-API build and TypeScript compilation; `tsconfig.build.json` emits executable JS and declarations from `src`. `ts:pack:check` consumes that build through the existing pack/import checker. | External proof must supply the built candidate wrapper and declarations plus its normal native dependency. Updating only native `index.d.ts`, hand-editing vendor declarations, or accepting an old `dist` cannot establish closure. |
| The example has no MCP connection | Committed `package.json:12` runs `node scripts/investigate.ts`; README's Observe instructions advertise it. `scripts/investigate.ts:8–9,21–26` imports `MCPServerStreamableHttp` and `wyrdFetch`, then connects with `fetch: wyrdFetch()`. `tsconfig.json` includes `scripts`. | Contradicted by immutable source. This is an explicit runnable and documented application consumer, not ignored SDK test material. |
| AC-006 is satisfied by the changed OpenAI construction | AC-006 requires one `WyrdClient` and `httpAuth` for OpenAI and MCP. Candidate `src/agent.ts:19–20` constructs the client and supplies its transport to OpenAI, but does not export the client. The investigation script imports only `gateway` from that module and still calls the removed helper. | Incomplete. Supplying the reviewed SDK fixes the local missing-getter problem but exposes the tracked removed-helper import at MCP. Keeping the old dependency cannot satisfy both the new OpenAI call and the approved removed-helper contract. |

The external README supplies no SDK vendoring/build step, and no tracked external build script supplies the missing directory. That absence limits standalone reproduction from the pinned application commit. It is unchanged by this candidate and does not justify expanding this bounded migration into package-distribution redesign. AC-006's word “builds” describes constructing one client; the separate AGENTS.md example verification requirement still requires a working example check.

## Finding reconciliation and smallest closure

No additional independent finding is proposed. Retain one missing external-consumer migration under BEH-001 / the MCP portion of MAINT-001, subject to independent validation. Revise MAINT-001 to distinguish its tracked MCP defect from the local dependency proof gap. STD-001's separately asserted committed dependency regression is unsupported by the immutable objects; its failed local check remains a valid verification limitation and must not be reported as a successful example build.

The smallest source correction is to reuse the client already constructed by `src/agent.ts` for the existing investigation MCP transport and remove the `wyrdFetch` import/call. Preserve its gateway model, tool filter, tracing, and investigation flow. There is no need for a compatibility export, local auth adapter, new client, package feature, registry publication, or newly tracked vendor tree.

The smallest credible closure proof has two parts:

1. Immutable application source shows OpenAI and MCP using that same client-owned transport and no removed-helper import in committed application consumers.
2. Supply the built reviewed SDK to the example's existing `file:vendor/wyrd-ai` dependency, record its candidate identity and actual resolution, then run `mise exec -- npm run typecheck` and `mise exec -- npm test` in the external repository. Establish the ordinary MCP connection using the existing investigation command against configured Wyrd; if that command requires live provider/server credentials unavailable to the reviewer, report the runtime limit explicitly. Typecheck covers the script import and transport typing but does not by itself prove MCP authentication. Existing Wyrd stock-client journeys support the transport behavior and need not be rerun wholesale merely to close this source migration.

Refreshing or repointing the ignored package is environment preparation for that proof, not a requirement to commit generated package contents. Record how the local package was supplied rather than claim a nonexistent canonical external vendoring workflow. No approved product or packaging decision is required to complete the tracked migration.

## Limits and identity

Read-only investigation; no dependency installation, build, test rerun, generated output, symlink mutation, or implementation change. This report does not independently reproduce the discovery typecheck output or claim full external deployability.

Final checks: Wyrd HEAD remains `824f0c491ea2686977c9da9fce03ac4ef65c01b1`; external HEAD remains `b283666df93a32cb73aa8c07f881d9271797dff0`. Only this assigned report was written by this reviewer.
