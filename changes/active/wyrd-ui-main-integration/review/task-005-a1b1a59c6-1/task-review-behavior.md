# Task 005 behavior review

Overall: **FAIL**. Five proposed material findings require independent validation. Human visual acceptance is also unavailable, so this report does not establish complete acceptance even after source corrections.

## Subject and method

First full review of `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6...a1b1a59c6f28b63d08973e2eccce168bbe80bd92`; HEAD remained the candidate. Authority is approved revision 28 of `spec.md` and original `tasks/TASK-005-cards-and-prompts.md`. Packet identities and executed checks are in `verification.md`. No previous acceptance evidence carries forward.

Read the task-review/UI skills, repository and TypeScript SDK rules, spec-driven workflow, maintainer guide, brand DESIGN, and relevant Card/Prompt authority. CodeGraph preceded code discovery. Traced registry requests through portable loaders and Node/MCP executors, conventional inventory/detail hosts, shared renderer/catalog/exports, Prompt compiler/status projection, seeded registration, and changed Observe streaming consumers. No source was changed. Specialist coverage follows `scope.md`; this report covers comprehensive behavior, rather than replacing those independent reports.

## Requirement matrix

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless qualified.

| Requirement / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Registry search, current kinds/status and honest states; filter changes restart paging | `cards.ts::inventoryFilter/loadCardInventory`, `schemas.ts::CARD_KINDS/CARD_STATUSES`, `CardInventory`, inventory route | Inventory unit proof and authorized-host assertions; review unit file 18/18 | PASS for covered requests/states |
| Server cursor pagination remains navigable without dropping remaining rows | `CardInventory` emits incremented `pages`; `schemas.ts` caps input/action at 20 | Direct validator reproduction below; no threshold regression in tests | FAIL, B-003 |
| Exact identity and stable invalid/not-found refusal; denied prerequisite prevents dependent reads | `detail/+page.server.ts` validates CardRef before IO and gates panels on header; `loadCardHeader`, `readPrompt`, `loadUsedBy`, `loadVersionDiff` retain exact references | Unit negative/prerequisite proofs; authorized-host test definitions and executed narrow host proof | PASS |
| Back preserves inventory state; tab/version/relationship navigation retains or replaces complete selected reference, stale results cannot replace newer selection | `cards-host.ts`, inventory/detail hosts, `WyrdView` existing cancellation owner | URL/action unit assertions and held-response WyrdView proof; production-browser definition exists, no review browser run | PASS statically; browser interaction remains unverified |
| All kinds keep registered Files tab and actual authorized metadata/state | shared tabs; `loadArtifactTable`; `ArtifactTable`; Node `Cards.listArtifacts` | Unit populated/empty/denied/error assertions; live Node assertion | PASS for Node/conventional; MCP FAIL, B-002 |
| Raw selected YAML and native Prompt body remain faithful | `CardFacts`, `PromptMessages`, `VersionDiff` all call `format.ts::fmtYaml` | Unit expected-output checks miss whitespace preservation; direct reproduction below | FAIL, B-001 |
| Authored portable/native forms, compiler-derived variables, using Agents, explicit two-version diff and single-version empty state; no Prompt telemetry | `loadPromptMessages/loadPromptVariables/loadUsedBy/loadVersionDiff`, Prompt components; Rust `PromptSpec::status` and `hydrate_card` | Prompt unit proof, live host assertions, no telemetry requests | PASS for covered authored forms and ordinary version lists |
| Applicable copy/navigation actions report actual outcomes | Exact navigation intents and action state; header/inventory clipboard feedback; Metadata hash copy | Unit intents and host actions; hash rejection has no observer | FAIL, B-004 |
| Shared package/catalog/schema/fixtures/renderer closure and existing custom imports | `index.ts`, `schemas.ts`, `catalog.json`, `WyrdView`, gallery fixture documents | Review check/build/codegen; unit schema/render checks | PASS |
| Every owned component has built-AppBridge T4 proof | Only CardInventory, PromptMessages and UsedBy added to built-host document; previous foundation covers CardHeader | `ui-library.integration.test.ts:666–686` | FAIL, B-005 |
| Real conventional journey and normal dev launch provide representative real Cards/Prompt/files without fixture fallback | BFF routes; `scripts/dev/example-app.sh::setup` public declarations; fixtures; existing lifecycle test extended | Production-BFF and lifecycle definitions cover pins/rail/files/Prompt; prior claims available, review browser unavailable | Source implemented; live review browser proof BLOCKED |
| Separate component and inventory/generic-detail/Prompt desktop/mobile light/dark human checkpoints | Gallery and conventional pages exist | No human acceptance evidence; `scope.md`/`verification.md` record unavailable browser | FAIL acceptance gate / BLOCKED inspection |
| Preserve credentials, tenant isolation, audit owner, shared executor/loader lifecycle; no alternate store/cache/domain parser | Type-only browser contract imports; server session-bound Node executor; unchanged server authorization; existing shared render/cancellation path; compiler projection reused | Authorized-host denial/foreign-tenant cases; domain report owns full boundary audit | PASS in behavior tracing; defer independent security verdict |

## Proposed findings

### B-001 — INCORRECT: YAML formatting changes authored Prompt whitespace

Obligation: REQ-009/011 and task scenarios 2–3 require raw selected YAML and unchanged authored native content. Location: `packages/ui/src/lib/format.ts:103–104`.

Producer-to-consumer trace: the exact Card read preserves authored strings; `loadCardFacts`/`loadPromptMessages` return them unchanged; `CardFacts.svelte:35`, `PromptMessages.svelte:37` and `VersionDiff` serialize them with this helper. It applies `trimEnd()` to every multiline line and cannot retain multiple final newlines with its clip/strip-only indicators. For authored system/message text `one  \ntwo`, the YAML contains `one\ntwo`. Two spaces before a newline are meaningful Markdown hard-break syntax and can be deliberately included in a Prompt. Inspecting/copying the raw selected definition therefore presents different authored content. Native body extension strings and diff sides share this cause.

Executed without edits:

`mise exec -- node --input-type=module -e 'import { fmtYaml } from "./crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/format.ts"; for (const system of ["one  \\ntwo", "one\\n\\n", "one\\n  \\ntwo"]) { console.log(JSON.stringify({input:system, yaml:fmtYaml({spec:{system}})})); }'`

The actual command used JavaScript newline escapes; observed first output was input `"one  \ntwo"`, YAML `"spec:\n  system: |-\n    one\n    two"`. `"one\n\n"` produced `"spec:\n  system: |\n    one\n"`.

Smallest correction: preserve string values in the existing formatter. Its existing JSON-quoted scalar path is valid YAML and can handle whitespace-sensitive strings without a new parser/dependency. If block presentation remains, it must preserve line whitespace and trailing-newline count. Add focused round-trip/value assertions, including a whitespace-sensitive Prompt string reaching the YAML/native component. Do not build a replacement serialization layer. Correction burden is local and small.

### B-002 — MISSING: ArtifactTable cannot read files in the required MCP host

Obligation: REQ-030 says every in-scope component works in hosts 1–3; task scenario 4 and acceptance require authorized Node/MCP reads. Location: `packages/ui/src/lib/mcp.ts:17–18`; live proof `src/lib/ui-library.integration.test.ts:582–588`.

Trace: `loadArtifactTable` emits `cards.list_artifacts`; Node uses the existing authorized `Cards.listArtifacts`, whereas MCP dispatches a tool that does not exist. Every authorized MCP viewer of a file-carrying Card consequently receives tool-not-found, even though its files are readable through the established server operation. The live test encodes this error as its expected MCP outcome. This is a permanently missing required host capability, not an authorization denial or a legitimate empty inventory.

The original task's Implementation Evidence claims a maintainer accepted deferral, but the approved revision and task acceptance still require this host, and no independent human waiver was available to this review. A claimed waiver cannot establish the exception.

Smallest correction boundary: reuse the existing authorized registered-file listing owner through the MCP read-tool surface and its executor; preserve permission/tenant/audit behavior. Prove populated, empty and denied files through that real host. No new storage, registration or alternate IO client is justified. If the maintainer intends deferral, reconcile authority explicitly instead of claiming task completion from an expected tool-not-found. This correction crosses the existing MCP projection boundary but does not require a new data contract.

### B-003 — INCORRECT: inventory pagination dead-ends after 1,000 versions

Obligation: REQ-008 and scenario 1 require server cursor pagination without dropping registry rows. Locations: `packages/ui/src/lib/schemas.ts:105–106,121–122,460–462`; `CardInventory.svelte` Load more handler; `cards.ts::loadCardInventory`.

Trace: a registry with 1,001 matching versions returns 50 rows per page and a non-null cursor after page 20. `loadCardInventory` returns `more: true`, so the shared component offers Load more and emits `{pages:21}`. Both the conventional host and renderer validate that intent against a maximum of 20 and return `WYRD_SPEC_400_VALIDATION`. The remainder of this ordinary growing versioned registry cannot be browsed. Back-state reconstruction does not justify presenting an unusable continuation.

Executed from UI directory: `mise exec -- node --input-type=module -e 'import { validateIntent, validatePanel } from "wyrd-ai-ui/loaders"; console.log(JSON.stringify({page20: validatePanel("CardInventory", {pages:20}), next:validateIntent("CardInventory", {name:"cards.more",input:{pages:21}})}));'`. Page20 validates; the next intent fails with `CardInventory cards.more input.pages: Too big: expected number to be <=20`.

Smallest correction: close the pagination boundary in the existing inventory loader/schema/host workflow so a returned server cursor always has a supported continuation. Preserve bounded per-read work and URL/Back/filter semantics; simply hiding the remaining rows is insufficient. Add one threshold case with a cursor after the twentieth page and prove the next rows remain reachable without duplicate/drop. No registry cache or traversal owner is needed.

### B-004 — INCORRECT: visible Metadata copy controls swallow failure

Obligation: task requires copy actions to report actual completion; scenario 2 says copy reports actual host completion. Location: `packages/ui/src/lib/components/CardFacts.svelte:43–44`.

Trace: real registered Cards expose `metadata.spec_hash` (and file-carrying Cards may expose artifact_hash); `loadCardFacts` passes it to the conventional overview Metadata panel. Its visible Copy spec hash / Copy artifact hash controls invoke clipboard IO and silently catch rejection. A denied or unsupported clipboard operation therefore produces no failure feedback, and success has no acknowledgement. Unlike hypothetical controls, these are directly reachable on the seeded normal workflow. Header and inventory already own small local clipboard feedback patterns that meet the requirement.

Smallest correction: use that local feedback pattern for these controls, preserving the exact copied value, and prove success/rejected clipboard outcomes beside the control. No action framework or new shared helper is required. This is a small explicit acceptance correction, not a redesign.

### B-005 — MISSING: five owned components lack required built-AppBridge proof

Obligation: REQ-030 T4 applies to every component and task acceptance repeats it. Location: `src/lib/ui-library.integration.test.ts:666–686`.

The only new built-HTML AppBridge document contains CardInventory, PromptMessages and UsedBy. Earlier foundation and Observe documents do not name CardFacts, RelationshipList, ArtifactTable, PromptVariables or VersionDiff. Unit rendering through WyrdView and successful package builds cannot establish those panels' tool-response shaping and rendered result in the actual iframe artifact. This is a specific mandatory coverage gap; it does not assert an unproven runtime bug.

Smallest correction: extend the existing built-host proof with those five panels and their necessary fixture tool responses/assertions, including the comparison dependent stage and file operation. Keep the existing AppBridge helper; no new harness. Execution of that focused proof closes this gap.

## Limits and regression assessment

Changed Observe server load and await-block consumers preserve direct SSR resolution and show explicit loading/error states for streamed navigation. No additional source-proven behavior regression was found; system reviewer owns rejection/cancellation propagation analysis. Existing cancellation owner is reused for shared panels. No extra store/cache/transport/parser was introduced; the YAML helper's fidelity issue is addressed above rather than demanding a new abstraction.

Review execution evidence currently establishes 18 unit tests, clean Svelte checks, build/codegen and the narrow authorized-host selector (see orchestrator's live updates in `verification.md`). It does not establish visual fidelity or human acceptance. The unavailable browser is not treated as a design defect or as permission to infer a visual PASS. Production browser and development-lifecycle prior claims are distinguished from review reruns. No live model, broad aggregate gate, or new browser lifecycle was run by this reviewer.
