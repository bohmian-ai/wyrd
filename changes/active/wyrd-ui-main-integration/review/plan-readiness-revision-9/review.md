Review ID: wyrd-ui-main-integration-main-8651b96-spec-revision-9
Target: changes/active/wyrd-ui-main-integration/review/plan-readiness-revision-9/spec-reviewed.md (draft revision 9; frozen identity in subject.json)
Gate: Spec
Verdict: Approve
Architecture axis: PASS
Executability axis: Not applicable
Critical: 0
Major: 0
Task coverage: Not applicable

## Evidence and scope

Read the complete frozen revision-9 specification against committed main `8651b96fd0c049ab5e63b0606c98326af1fc8848`. The reviewed spec SHA-256 is `c42dac36664b55b93af964ad839de4891a7ddfa4651e789cf3702ea9f54563b2`, recorded in the adjacent `subject.json`. Current user instructions, repository agent rules, architecture, and the plan-readiness-reviewer rubric govern this independent specification review. Earlier findings were re-evaluated rather than carried forward automatically.

Checked the material read, rendering, credential, action, grouped-verification, packaging, prerequisite, and completion boundaries. Relevant current sources include:

- `crates/wyrd-spec/src/vala/api.rs:555–605`: the shared scalar `QueryParam` encoding and existing bound-query request.
- `crates/wyrd/wyrd-server/src/query/collect.rs:125–203`: current MCP query argument gap and its existing request projection; reviewed spec lines 490–500 expressly assign the extension to foundation work rather than claim it already exists.
- `crates/wyrd/wyrd-server/src/query/collect.rs:333–451` and `:546–577`: bounded collection, terminal validation, positional rows, schema triples, and the existing projected terminal.
- `crates/wyrd/wyrd-server/src/mcp/cards.rs:25–112`, `crates/wyrd-spec/src/registry/reads.rs:69–136`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1639–1654`: exact CardRef reads and current list contracts on both surfaces.
- `sdks/wyrd-sdk-ts/wyrd/src/index.ts:394–435`, `:1247–1282`, and `:2199–2236`: Node Arrow stream/schema/terminal access and execution readback; the file's top-level native import at `:1–95` confirms why browser-facing library entry points must remain isolated from the Node executor.
- `architecture/wyrd-design.md:1274–1375`, `crates/wyrd-spec/src/verification/execution.rs:173–199`, `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:15–71`, and `crates/wyrd/wyrd-server/src/components/verification/service.rs:203–318`: current owner-scoped groups, repeated group-end snapshots, selected queued result readback, and sensitive result payloads.
- The preserved canvas inventory and the spec's individual desktop/mobile, light/dark gallery and shipping acceptance.

The user's existing dirty trace-summary removal is not treated as committed-main capability and was left untouched. No archived task packet is implementation authority.

## Findings

No blocking findings.

The prior material gaps are resolved at the specification boundary:

- **Read parity:** upload planning is excluded; exact CardRef/list/read mappings are named; the existing MCP query tool receives the shared typed parameters as explicitly scoped work (reviewed spec lines 470–503).
- **Dependent reads and cancellation:** shared loaders may sequence reads; failed prerequisites stop dependent work; superseded loads cannot replace newer state. Transport and credentials remain in executors, and new routes require an evidenced domain need (lines 440–469).
- **Actions:** components emit declared typed intents; authorized hosts execute through existing server capabilities and return pending/success/denied/error state. Documents cannot inject handlers, tools, or authority. Missing host projections are prerequisites, and unavailable actions cannot pass production completion (lines 568–594).
- **Component completion:** fixture-ready work is explicitly distinguished from production-complete work. Individual desktop/mobile acceptance, light/dark states, interaction/accessibility checks, and applicable live proof precede shipment and conventional pages (lines 212–225, 730–761, and AC-011 at line 911).
- **Grouped verification:** the UI preserves owner scope and exact recorded reviews; pinned snapshots define members; existing execution readback supplies the selected queued result. `group_end` is no longer a once-only session close (lines 297–350).
- **SQL and query results:** built-in templates and authored SQL are distinguished. Range/bucket binds, server validation, exact reproducible SQL, current MCP JSON normalization, limits, degraded results, and failure behavior are specified (lines 505–547).

## Axis assessment

Architecture axis: PASS. The proposed shared component/loader/catalog/view package can serve the four hosts without importing the native SDK into browser execution or moving durable behavior into the frontend. The Node executor alone uses the native SDK; MCP uses the existing authenticated tool connection; shipped browser rendering uses the credential-protected BFF. Typed action intents preserve the same ownership boundary for mutations.

The specification establishes credible outcomes and proof classes for native packaging/login, deletion, foundation, the complete library, MCP App integration, and later pages. The T4 browser artifact is built before production MCP registration, avoiding a component-proof dependency cycle (lines 786–789). TASK-001 remains independent of domain integration. Existing auth imports and tests affected by deletion remain ordinary consumer closure under the retained-behavior and no-deleted-import acceptance, not an unresolved architecture decision.

Missing Service correlation, Service charts, non-sensitive requirement outcomes, Changes/Repos, Ask, saved views, and any missing action projections remain genuine external contract dependencies. They hold affected components and the full-library shipping gate. This verdict does not assert that these prerequisites ship on main, define them on the UI implementer's behalf, or authorize invented replacements.

Executability axis: Not applicable. No replacement task packet was presented for review. Exact task boundaries, source-backed reuse tables, focused verification commands, dependency ordering, and individual visual checkpoints must be assessed when tasks exist.

## Task coverage

Not applicable. This is a Spec gate, not task approval. Revision 9 remains a draft pending explicit human approval; the review verdict supplies technical readiness evidence only. It does not approve the specification on the user's behalf, authorize implementation, or permit dependent visual slices to bypass human inspection.

## Limitations

Static source/specification review only. No implementation, runtime test, build, native release, image routing exercise, browser-host execution, or visual inspection was performed. Only this assigned review report was written by the reviewer.

The full-library shipping gate necessarily waits for separately specified platform prerequisites and individual desktop/mobile human acceptance. Task planning must expose those waits instead of claiming fixture-only completion.
