# TASK-027 system-resilience review

Result: **FAIL**. Two proposed findings require independent validation. No source changed.

## Immutable subject and authority

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda` (HEAD reconfirmed during review).
Reviewed cumulative base-to-candidate diff; identified task changes through `git show d33de26e4 d90d98a12`. Upstream Rust/build/planning changes in the cumulative range do not implement this task and are not attributed to it.

Authorities inspected: AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development, maintainer-style, architecture/patterns, testing-workflows, applicable client-model/public-surface sections of wyrd-design.md and wyrd-doctrine.mdx; approved revision-12 spec REQ-030/036 and AC-015; original TASK-027; Wyrd UI skill and Svelte-5 patterns; brand DESIGN.md. No `.codegraph` exists per subject. This report audits only changed runtime failure and recovery behavior; it does not propose new durable availability guarantees.

## Deployed paths and ownership

| Path | Runtime owner and dependencies | TASK-027 effect |
|---|---|---|
| Gallery | Two WyrdView instances share the page's transient `doc`; `fixtureExecutor` supplies reads; callbacks replace `doc` | Document-driven grid and local editing; page reload restores authored document |
| Custom app `/view` | WyrdView → remoteLoader → `/view/load` → existing loadPanel/executor → Wyrd service | Local edits feed back into `doc`; layout is not sent to the loader endpoint |
| `mountView` | Svelte mount owns renderer lifecycle; destroy calls unmount | Optional callback passes unchanged through mount options; no persistence owner added |
| MCP App | App tool result destroys old mount, creates new mount; executor uses host callServerTool with AbortSignal | No-op layout handler intentionally leaves edits in renderer-local `kept` state until next result; host credential stays outside document |
| Renderer | `parsed`, content key, loader identity, controllers, panels, kept/draft, pure place/edit | Same read pipeline; layout metadata excluded from content key; no second loader/transport/workflow owner |

The existing renderer, mountView, remoteLoader and MCP executor are reused. Pure `layout.ts` is the new geometry needed for placement; it does not replace a pre-existing lifecycle owner. The change adds no server listener, database state, retry queue, health check or shared-process failure policy.

## Failure/recovery assessment

| Event | Source evidence | What stops / survives / recovers | Assessment |
|---|---|---|---|
| Layout-only edit or host echoes document | WyrdView.svelte:61–89, 108–139 | Loaded panel data and in-flight reads survive because layout is excluded from content and executor/loader identity is retained | Scoped test verifies same executor + updated doc causes no extra reads |
| New content/range; old read resolves late | :77–89, 181–214 | Old controllers abort; old results cannot settle through aborted/unregistered controller; new generation loads independently | Existing TASK-003 test covers supersession; no TASK-027 server availability regression found |
| One panel rejects or remote endpoint is unavailable | :193–213; view.ts remoteLoader; mcp.ts read | Failure becomes that panel's result, sibling panels remain available; no automatic retry amplification | Existing failure owner preserved; task adds only overflow/layout containment |
| Pointer cancellation by browser | :169–178 | pointercancel removes listeners and drops draft without reporting; completed `kept` edits survive | Source supports local recovery; no dedicated pointercancel assertion in supplied tests |
| Escape during active drag | :145–178 | Draft disappears momentarily, but captured pointer listener remains and can recreate it | SYS-027-1 |
| Multiple completed edits without host echo | :95–139; app/main.ts:23–26 | Screen remembers all edits in kept; emitted document only remembers the most recent edited panel | SYS-027-2 |
| New MCP result / page reload / process replacement | app/main.ts:23–26; view.ts mountView | Old view unmounts and aborts loads; fresh view uses written document. Ephemeral edits intentionally disappear | Conforms to no persistence; no new server crash/recovery mechanism |
| Narrow width | :91–115, 255–281 | Panels reorder into full-width fixed-height boxes; handles removed; no reads introduced by width | Supplied narrow test verifies ordering and no handles; fixed-height overflow is intentional |

No finding recommends crashing a request owner, component, process or service. Corrections stay inside renderer edit state; service, authentication and query behavior must remain unchanged.

## Proposed findings

### SYS-027-1 — Escape does not terminate an active pointer edit

- Classification: **INCORRECT**.
- Obligation: REQ-036 editing cancellation; TASK-027 scenario 3 requires pointer and keyboard editing to be cancellable, with Escape restoring pre-edit state and no completion report.
- Location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:153–154, 159–178`; move-handle event wiring at :263–265; resize corner at :281.
- Producer → consumer: `drag` captures the pointer and registers a `move` closure that writes `draft` unconditionally. `key` handles Escape by clearing only `draft`. Its handler does not remove or invalidate those closures. On the next captured pointermove, the same drag writes a fresh draft from `from`; pointerup then calls `commit` and reports it. A reachable sequence is focus the move handle, pointerdown, pointermove, press Escape, move again, pointerup. The corner additionally has no Escape key handler and pointerdown prevents a focus transition.
- Observable consequence: a cancelled move/resize can return and commit after Escape. This affects all editable hosts because they share this renderer; in the gallery/custom app the unexpected callback replaces the host document, and in MCP it changes the per-render layout.
- Existing alternatives/callers: `key`, `drag`, `commit` and the parsed-document reset are the only edit-state producers; all hosts above delegate here. Browser pointercancel already has cleanup in `end`, but Escape does not enter that cleanup. Reuse this existing local drag lifecycle rather than adding another gesture framework.
- Smallest testable correction: make cancellation end the active renderer-owned gesture so subsequent move/up events from that gesture cannot create or commit a draft; make Escape reachable during both handle move and corner resize. Preserve keyboard-only cancel, pointercancel, once-per-completion callbacks and zero reloads. Do not change host persistence or loader behavior.
- Focused closure proof: simulate an active captured drag, Escape, further pointermove and pointerup; assert original placement restored and callback count unchanged. Repeat for resize, plus normal completion still reports once. Existing keyboard-only Escape test at ui-library.test.ts:439–443 does not exercise the still-active pointer listener.

### SYS-027-2 — Completed local edits are omitted from later whole-document callbacks

- Classification: **INCORRECT** (public handler output diverges from rendered state).
- Obligation: REQ-036 and AC-015 require each completed edit to report the whole updated document; MCP edits must last for the render, and hosts decide what to keep.
- Location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:108–109, 134–139`.
- Producer → consumer: `commit` stores each completed layout in `kept[id]`; placements use all of `kept`. The emitted document instead maps `parsed.data.panels` and substitutes only the current `id`. With a callback that records results without immediately echoing a new doc (a supported mountView host; MCP uses the literal no-op handler), commit panel A then panel B. The screen uses A-new and B-new, but the second emitted document contains A-original and B-new. Echoing that second emitted document later clears kept at :100–105 and visibly loses A's edit.
- Observable consequence: a host accepting a whole-document result can revert earlier completed changes even though no new authored document was loaded; the callback fails its specified contract. This is transient-layout state loss, not database durability loss.
- Existing alternatives/callers: gallery/custom app immediately echo doc and mask the gap; mountView accepts arbitrary synchronous callbacks, and MCP explicitly relies on renderer-local retention. No other callback builder exists. The existing `kept` owner already has the information needed; no saved-view state or second document store is needed.
- Smallest testable correction: construct callback output from every completed effective layout held by the renderer, replacing the current edit once, while retaining unplaced/unmodified panel representation and not rewriting collision push-down positions into the document. New host documents must still reset transient edits. Preserve input immutability, no reloads, and MCP per-render lifetime.
- Focused closure proof: with a recording handler that does not echo props, edit A then B; assert second callback contains both edits and original input is unchanged. Echo second callback and assert no layout regression or reload. Existing scenario edits only A repeatedly and therefore cannot detect omission of a sibling's kept layout.

## Verification limits

Read original task's acceptance evidence and orchestrator verification.md: all four exact TASK-027 tests passed, with TASK-003 regressions and package/UI checks also rerun by orchestrator. Read actual tests and failure/supersession paths; this role did not rerun browsers, builds, Linux bundles or live service journeys. No dedicated executable reproduction was added because reviewed source is immutable and this assignment owns only its report; findings above are source-traced claims for independent validation. Historical browser evidence proves normal move/resize and mobile geometry but does not record active-drag Escape or two-panel callback retention.

No source, tests, fixtures, dependencies or deployment configuration changed by this review. Remaining risk is the two bounded renderer edit-state failures above; broader infrastructure recovery remains outside TASK-027.
