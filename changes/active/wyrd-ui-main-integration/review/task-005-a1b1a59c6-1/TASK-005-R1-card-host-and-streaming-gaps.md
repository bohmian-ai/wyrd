---
id: TASK-005-R1
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 28
parent_task: TASK-005
remediates: [FIND-005-1, FIND-005-2, FIND-005-3, FIND-005-4, FIND-005-5, FIND-005-6]
---

# Correct Card host completeness, fidelity and streamed failure handling

Route confirmed remediation to `$wyrd-implement` after the BLOCKED review's advanced subject/authority is reconciled. This preserves candidate-specific corrections under the approved behavior, not a new design plan or acceptance of the advanced checkout.

Authority: [approved revision 28](../../spec.md), [original Task 005](../../tasks/TASK-005-cards-and-prompts.md), [validated ledger](findings-validation.md), [verdict](verdict.md).
Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Original base: `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6`.
Reviewed candidate / remediation starting point: `a1b1a59c6f28b63d08973e2eccce168bbe80bd92`.
No prior remediation round exists. Retain the original task, this verdict and ledger for closure review.

UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Diagnosis and selected correction

### FIND-005-1: raw YAML changes authored whitespace

The exact server read and portable loaders preserve the authored strings, but `packages/ui/src/lib/format.ts:103–104` trims every multiline line and cannot preserve multiple final newlines with its literal-block chomping. Input `"one  \ntwo\n\n"` renders as YAML that decodes to `"one\ntwo\n"`. CardFacts YAML, native PromptMessages body/settings and structured VersionDiff sides share this formatter. Markdown hard breaks and provider-native text can therefore change in the supposedly raw definition; a comparison can hide the distinction it detected. This violates the explicit exact-version/authored-form obligation, rather than requesting general YAML hardening.

Correct serialization once in the existing `fmtYaml`/scalar owner. Use its existing JSON-quoted scalar representation for whitespace-sensitive multiline strings, or remove the defective literal specialization and use that representation for all multiline strings. Both retain valid YAML without a dependency. Preserve every input character and existing non-string structure. Block-layout aesthetics are not a separate requirement. A caller-only fix leaves sibling consumers wrong; a new serializer framework is unnecessary.

### FIND-005-2: Files cannot load through MCP

`loadArtifactTable` emits `cards.list_artifacts`, but `packages/ui/src/lib/mcp.ts:17–18` forwards it to a missing Rust MCP descriptor/dispatch. Node uses the existing authorized `Cards.listArtifacts` and works. The real-host test at `src/lib/ui-library.integration.test.ts:582–588` accepts the MCP error instead of requiring the registered inventory. Every authorized MCP Files panel fails even for normally readable files. REQ-030 and original scenario 4 require that host. The Implementation Evidence's claimed waiver has no supplied human approval and does not override those requirements.

Project the existing authorized listing through the read-only `cards.list_artifacts` MCP operation already selected by the executor, with exact `card_ref` input and existing `ArtifactInventoryResponse` output. Extend the existing Card descriptor/dispatch owners in `crates/wyrd/wyrd-server/src/mcp/{cards,mod}.rs`. Resolve the exact reference through existing authorized Card-read/UID validation, then reuse the authorized inventory path and `components/cards/service.rs::list_card_artifacts`. Share existing HTTP authorization/listing ownership where necessary, following Card GET/LIST; preserve permission decisions, their audit commit boundaries and the caller's tenant. This is a thin projection of an existing read, not a new storage, permission or file-lifecycle contract. Do not add a second transport or download workflow. The Node path and actual denied/empty/error behavior remain intact.

### FIND-005-3: continuation ends at 1,000 matching versions

`cards.ts:111–136` replays up to `input.pages` from the first page and discards the opaque next cursor, retaining only `more`. At twenty 50-row pages, a 1,001-version registry still offers continuation, but CardInventory emits `pages:21` and `schemas.ts:105–122,460–462` rejects it. Both conventional and custom hosts fail validation, leaving the rest of the supported registry unreachable. Increasing the replay cap moves the failure and grows per-load work; hiding continuation silently drops results.

Use current-page opaque-cursor pagination through the existing inventory loader/schema/action/URL owners. Each load fetches one fixed 50-row server page and returns its actual continuation; Next advances to that cursor. Preserve filters and selected cursor in the conventional URL so reload and normal history Back, including Back from detail, restore the same inventory page. A filter change clears cursor. Update shared component, catalog/fixture projections and conventional host together. Present Next as page navigation, without claiming prior pages remain appended. Original scenario 1 already requires Next/server cursors, not an accumulating DOM. No cursor decoding, server cursor change, registry cache or new lifecycle owner is required. This removes the repeated-read cause while retaining bounded work and exact identity.

### FIND-005-4: Metadata hash copy hides all outcomes

Real `metadata.spec_hash`/`artifact_hash` values reach the live CardFacts Metadata panel. Buttons at `CardFacts.svelte:43–44` call clipboard IO and silently swallow rejection; they show neither success nor failure. These are reachable seeded-detail controls, and the task explicitly requires truthful completion.

Reuse the small local clipboard-feedback pattern already used by CardHeader/CardInventory. Report accessible success or failure beside the control, including unavailable clipboard access, and copy the full exact hash. This requires local presentation state only; no loader/action framework or new helper is justified. Deleting the useful control avoids rather than completes the requirement.

### FIND-005-5: five components lack required built-artifact proof

The actual `appBridgeView` helper runs built `dist/wyrd-view.html` in an iframe with ext-apps AppBridge and fixture tool replies. Across all three calls, it renders CardHeader/Chart, Observe panels and CardInventory/PromptMessages/UsedBy. It never renders CardFacts, RelationshipList, ArtifactTable, PromptVariables or VersionDiff. Earlier real loader assertions do not exercise those renderer branches in that artifact. This is an explicit per-component T4 gap, not a claim that the components fail at runtime.

Extend the existing Card/Prompt built-host proof at `ui-library.integration.test.ts:666–688` with those five panels, compatible tool replies and visible per-panel results. VersionDiff must receive distinct exact-version reads/listing and demonstrate comparison; ArtifactTable must receive the existing inventory response. Verify dependent request stages/arguments where necessary. Reuse the current helper. Combine its file-tool fixtures with FIND-005-2 while retaining the distinction between fixture T4 and real MCP T5. No additional harness or broad host matrix is needed.

### FIND-005-6: cancelled streamed read can escape the request boundary

Observe `+page.server.ts:28–33` returns raw nested load promises on data requests. Native SDK cancellation rejects them, but SvelteKit attaches serialization rejection handlers only after the layout/page loads finish. A pending sibling layout connection leaves a window for a selected-record rejection to become unhandled. The built-route reproduction exits Node 1 before delayed observation; independent validation also drove a real GET socket disconnect through installed SvelteKit `getRequest` and confirmed request-signal abortion. Native acquisition/collection paths establish that sibling reads need not settle together. Official image supervision stops Rust/nginx when the BFF exits, so a cancelled drawer can affect unrelated service requests on that replica. The browser's later await/catch cannot protect this server window.

Immediately observe rejection on every intentionally streamed promise at this route owner, retaining the original rejecting promise for framework/client error rendering. Native Promise catch observation suffices. Cover log, waterfall and optional span record. Preserve direct-SSR Promise.all, immediate in-app loading state, original rejection reason and native executor cancellation. Do not return a replacement resolving undefined, await all reads before returning navigation, swallow errors globally, add retries, or change process supervision. The problem belongs at the new streaming boundary; executor cancellation and sibling layout ownership are already appropriate.

## Preserved behavior and non-goals

Keep exact Card pins, URL/tab/version semantics, authenticated Node/MCP IO, tenant separation, stable errors, audit behavior, registered-file metadata and compiler-derived variables. Keep all twelve registrable kinds. Preserve portable/native fields, Prompt's absence of telemetry, direct trace-link SSR and immediate drawer loading. Reuse existing loader, renderer, executor, MCP, unit/integration and development owners.

No registry cache, new storage, independent Prompt parser, optional Used-in computation, transport framework, download capability, process hook, supervisor redesign, broad refactor or unrelated Observe Metrics correction. Leave unrelated user edits and throwaway files alone. Follow narrower contributor rules and mandatory changed-Rust-item rustdoc. Regenerate projections from owners.

## Focused acceptance and proof

Add these cases to the existing test owners before correcting behavior; record actual exact selectors and RED/GREEN results. Reversible test organization remains implementation-owned.

| Finding | Required closure evidence |
|---|---|
| FIND-005-1 | Value-based serializer assertions preserve line-final spaces, whitespace-only lines and multiple final newlines; a native-body or structured-diff component assertion keeps distinct authored values distinguishable. Do not derive expected values by calling fmtYaml. |
| FIND-005-2 | Existing real Node/MCP host proof requires populated/empty files, stable denied access and foreign exact-ref absence; existing MCP tests cover descriptor/dispatch and exact-reference mismatch. Tool-not-found is no longer accepted as the capability outcome. |
| FIND-005-3 | More than twenty cursor continuations remain valid and reach each UID once; each load performs one fixed-size list request with the returned opaque cursor. Filter reset, URL round trip and Back from detail restore correct selected-page/filter state. Use executor responses, not a thousand-row live seed or new harness. |
| FIND-005-4 | Resolving and rejecting clipboard operations show accessible actual outcome and preserve exact copied hash; missing clipboard access also reports failure. |
| FIND-005-5 | All five omitted components visibly render their results in the actual built iframe under the existing host selector; comparison/file dependent stages use correct identities. |
| FIND-005-6 | A server-load test delays consumer/serializer observation, aborts before attachment and observes no unhandled rejection while the returned original promise still rejects with its reason. Cover log/waterfall/optional-record branches and retain direct-SSR behavior. No live BFF crash or container/load harness is needed. |

Run only focused new/corrected selectors and the narrow lanes covering this write set. Existing relevant commands:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^card and prompt components use authorized host reads and actions$"'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run fmt:check
mise run codegen:check
```

Select exact affected MCP Rust tests through existing nextest owner/managed Postgres as appropriate, record every actual selector and run narrow owner Clippy after Rust changes. Consult owning mise tasks first. Do not require full user-journey suites, development lifecycle or aggregate gates for this remediation; integrated change review owns broad runs.

Task completion additionally retains the original separate gallery, inventory, generic-detail and Prompt desktop/mobile light/dark acceptance. This review had no browser surface and grants no visual sign-off. Once browser access exists, record actual verified themes, viewports, keyboard/focus/touch/overflow and acceptance using the original canvases; code proof cannot substitute for that gate. Do not restart the user's live stack or manufacture acceptance.
