# TASK-005 UI and presentation review

Subject: base `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`, candidate `78cd99091c81e5a7d706ac6f54a8222e62705e0a`. HEAD matched candidate before inspection. Source was not modified. This independent specialist covers presentation, the shared shell, the twelve approved differences, provider artwork, and keyboard/narrow-layout behavior. Overall workflow and principal-security acceptance belong to the other assigned reviewers.

## Authority and navigation

Read the root AGENTS instructions, task-review and UI skills, finalized `design/admin-console.md`, approved revision 12 REQ-021 and AC-016, TASK-005 scenarios 4–10, and the relevant `brand/DESIGN.md` sections (provider exception, geometry, layout/responsiveness). No `.codegraph/` exists. Production owners are `src/lib/{Shell,Overlay,Mark,Pill}.svelte`, `console.css`, tenant Settings layouts/pages, and platform layouts/pages. The browser/BFF consumer proof lives in `src/lib/ui-pages.integration.test.ts` and `src/lib/server/auth/{production-auth,onboarding}.integration.test.ts`; the reusable modal owner is `packages/ui/src/lib/components/Drawer.svelte`.

## Coverage and the approved differences

The production shell and six-section Settings list have the approved structure, names, route ownership, and shared presentation with the platform console. `console.css` follows the mock's one-work-region hierarchy and token colors. The mock preview strip and sample identities are absent. The name block uses server profile projections, and account switching/sign-out remain in Your account. Provider buttons retain their existing POST workflows and add fixed artwork. `Mark.svelte`'s Google paths/colors and GitHub path/currentColor agree with `design/admin-mock/src/Mark.svelte`; marks are decorative beside visible method names. The production `brand/DESIGN.md` includes the approved exception.

All twelve listed deviations are maintainer-approved task inputs, not defects:

| Approved difference | Source inspected |
|---|---|
| 1. Disabled Find or ask | `Shell.svelte` `find` control; tenant layout supplies `find` |
| 2. Phone top navigation | `Shell.svelte` breakpoint at 767px |
| 3. No Secrets section count | Settings `(admin)/+layout.svelte` uses null |
| 4. People columns/segments/exact search | People header, filter links and GET search |
| 5. Service-account columns/stats | Service accounts table and status strip |
| 6. Your account extra sections/omissions | Account page profile, Password, API keys, Session |
| 7. Roles server ordering | Roles page iterates server rows without sorting |
| 8. Secrets rotation and row counts | Secrets inventory/page; no age warning |
| 9. Platform omissions/Mail set label | Platform workspaces/admins/secrets/mail pages |
| 10. Absolute invitation expiry | People uses `fmtInstant(Date.parse(one.expires_at))` |
| 11. API key session label | Tenant layout server `identity()` handles /me 403 as API key |
| 12. Signup creator display-name timing | Accepted scope exception; provider persistence correctness assigned to domain reviewer |

No request for renewed approval of these deviations is warranted.

The connected `People and Service accounts work from the keyboard and fit a phone and a desktop in light and dark` test (ui-pages integration lines 3386–3420) checks 1280/390 widths, both modes, no document overflow, keyboard section navigation and a visible focus ring. It does not check modal focus containment or the other new tables. People, Service accounts and Roles explicitly provide labeled focusable scrolling regions. Four other new table owners do not, as detailed below.

## Captures and proof limits

`captureAdmin` (ui-pages integration lines 925–938) captures at 1280/390, both modes, using the persisted mode cookie. Its tenant call at 2986 covers People, invited filter, invite drawer, Service accounts, Roles/expanded Role, Sign-in/setup step 1, Secrets and Integrations; 2987 covers Your account and 4064 covers all six platform destinations. These helpers connect captures to real production journeys, rather than disconnected page mocks.

Historical image artifacts are available locally in `/tmp/claude-501/-Users-stevenforrester-Documents-GitHub-wyrd-docs/7b78784a-9da4-41c0-b3bb-051b214fa562/scratchpad/prod-shots4/` and mock artifacts in `/var/folders/rm/bsgsfq3947358gs6gxv7vzyc0000gn/T/wyrd-admin-mock-wpHWPB/`. Inspected the matching People light/1280 pair, production invite dark/390, and Secrets dark/390. They corroborate the structure and approved deviations. They are historical temporary files without immutable candidate manifests; this review does not claim freshly captured candidate screenshots or comprehensive independent comparison of every state. The task records the earlier comparison and subsequent fixes; candidate source preserves those corrections.

The task also asks for matched sign-in/signup/invitation captures (scenario 9). The available production capture helper does not cover those routes, and no corresponding production captures were located in the historical capture directories searched. Source establishes artwork/color/label wiring; the recorded invited-Google journey establishes interaction. This is a presentation-proof limit for reconciliation by the primary/validator, not an assertion that those buttons are wrong. No production browser journey or build was rerun by this specialist.

## Proposed blocking findings

### UI-01 — INCORRECT: new modal overlays leave the background keyboard-active

- Authority: REQ-021's built drawer requirement; TASK-005 scenario 10 keyboard/focus behavior; UI skill accessibility requirements and existing native modal owner.
- Location: `crates/wyrd/wyrd-server/wyrd-ui/src/lib/Overlay.svelte:25–49`; consumers include People invitation/account drawers, Service-account creation, Role create/edit/delete and Secrets replacement/revoke.
- Starting conditions and trigger: an administrator opens Invite people, a create/edit drawer, or a destructive confirmation; focus moves to the overlay. Press Shift+Tab from the focused overlay, or continue Tab after its final control.
- Expected: a modal's keyboard navigation stays inside the active modal until dismissal; background navigation/actions are unavailable while it declares `aria-modal=true`.
- Actual: the component focuses a `div` once and handles Escape globally. The scrim is an ordinary focusable anchor before it; neither branch uses native `showModal()`, traps Tab, nor makes the page inert. Shift+Tab reaches the scrim and then background controls; forward tabbing exits the overlay as well. `aria-modal` does not enforce modality. This affects keyboard users in the normal advertised drawer workflow and can navigate away from pending forms or activate controls hidden under the scrim.
- Evidence: complete Overlay source; no ancestor layout marks background inert. The existing built `packages/ui/src/lib/components/Drawer.svelte:23–38` already uses native `<dialog>.showModal()` specifically for focus trapping and Escape. Current keyboard test checks navigation and a focus ring only, not this invariant.
- Smallest correction boundary: restore true modality at the shared overlay owner, reusing/adapting the existing native-dialog mechanism while preserving the approved mock geometry, URL-owned open/close state and write-free cancellation. Do not fix each page separately or add a dependency. A focused production drawer/confirmation keyboard journey should assert Tab and Shift+Tab containment, Escape/cancel without writes, and return to a usable trigger.

### UI-02 — VIOLATION: four new dense tables omit the required labeled keyboard scroll region

- Authority: `brand/DESIGN.md` Layout & responsiveness, “Tables, trace waterfalls, and code keep their widths inside a focusable, labeled horizontal scroll region”; UI skill accessibility; TASK-005 scenario 10 narrow/keyboard presentation.
- Locations: tenant `settings/(admin)/secrets/+page.svelte:72`; platform `ui/workspaces/+page.svelte:19`, `ui/admins/+page.svelte:13`, and `ui/secrets/+page.svelte:53` (under the production routes tree).
- Starting conditions and trigger: a keyboard user opens a populated Secrets or platform table at phone width; its columns overflow the work region and they try to focus/scroll the table or identify the scrolling region using assistive navigation.
- Expected: the horizontally scrolling table wrapper is keyboard-focusable and has an accessible region label, as People/Service accounts/Roles already do.
- Actual: these wrappers are plain `<div class="tablewrap">` with overflow-x auto from console.css, without tabindex, region role or label. Browser-specific auto-focusability is not a portable substitute for the explicit repository contract, and provides no label. Platform Admins in particular has a nonfocusable read-only root/admin row in supported configurations, so not even an internal control guarantees keyboard access to overflow.
- Evidence: all four wrapper declarations; contrast with People line 89, Service accounts line 46 and Roles line 82, which implement the same shared CSS with `role="region"`, `aria-label` and `tabindex="0"`. The existing responsive journey only exercises People and Service accounts.
- Smallest correction boundary: apply the existing labeled/focusable table-wrapper pattern to these four owners. A focused phone-width populated-table check should locate the named regions, focus them from the keyboard and verify horizontal scrolling stays inside the region with no document overflow. Preserve the server-owned rows and actions.

## Scope and process

No performance hot path is changed by the UI boundary inspected; no benchmark is claimed. No Cargo, codegen, dependency installation or production mutation was performed. The task's recorded ui:check and connected journey results are implementation evidence, not fresh execution by this reviewer. TDD history for server behavior is outside this presentation assignment; task scenario 9 explicitly exempts presentation work from manufactured RED/GREEN ordering. Unrelated merged PR244 Bifrost/benchmark work was not audited here.
