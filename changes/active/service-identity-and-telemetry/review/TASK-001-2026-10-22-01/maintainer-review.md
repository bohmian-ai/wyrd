# Maintainer review — TASK-001

Result: **FAIL**. The change preserves the existing workflow owners and its public declarations agree with the changed native signatures. One required rustdoc correction remains; no new abstraction or owner replacement is needed.

## Subject and authorities

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry`.
- Base: `f38dff80327bb771975e2cdd8fbed283ab145159`.
- Candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`.
- Reviewed cumulative range: `git diff f38dff803...0a29bae4f4ff20e98d2c7937f3943877ac31bc62`, not the final commit alone.
- Inputs: approved revision 12 of `changes/active/service-identity-and-telemetry/spec.md`, original `tasks/TASK-001-flat-card-identity.md`, and this review's `subject.md`.
- Rules read: root `AGENTS.md`; SQL, Python, TypeScript and docs `AGENTS.md`; `changes/spec-driven-development.md`; `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, and `agent-surfaces.md`; architecture authority map, Card declaration/reference/config sections of `wyrd-design.md`, and tenant-binding/security principles in `wyrd-security-posture.md`.
- No `.codegraph/` directory exists. Source and candidate HEAD remained unchanged during inspection. No sibling report or implementation conversation was used.

## Changed-surface coverage

| Surface | Owning code, callers, and proof inspected | Maintainer assessment |
|---|---|---|
| Identity and wire values | `wyrd-spec` `CardRef`, `CardRefIdentity`, `CardRefScope`, display/text parser, `Metadata`, `CardMetadata`, IDs, registry reads/submissions/bundles, Card/Run projections, and reference tests | Flat identity lives in the existing contract owner. Removing the field removes the representable partial-space state rather than adding another normalization layer. Optional UID and exact version remain explicit. |
| Reference decode and validation | `reference.rs` `Ref::deserialize`, `InlineableRef::deserialize`, `looks_like_card_ref`; `Spec::from_kind_and_value` consumers; canonical reference-slot visitor; loader parse/resolve/validate; binding validation and reference-slot tests | Added dispatch stays at the serialization boundary. `looks_like_card_ref` only selects the useful error after native inline decoding fails; it does not become a second identity validator. `CardRef` still validates its fields. |
| Graph and canonical identity | `graph/{mod,canonical,composition,root,topo}.rs`, loader order/duplicate checks, server sibling identity cache and root/submission matching | Existing graph machinery is extended by deleting one tuple component. No new graph store, traversal, or identity resolver was introduced. Ordering and duplicate detection remain discoverable at their existing owners. |
| Workspace configuration and authoring | `wyrd-config` `Defaults`, `KindOverride`, discovery/cache, `apply_defaults`, `WyrdConfig::apply_defaults_fields`, Python config projection; loader `apply_defaults`, `card_ref_for`, `PathResolver`, and config/loader tests; `wyrd-cards` identity, CardRef, Agent/Data/Model/Prompt holders and constructors | Existing per-key merge owners remain authoritative; scalar fallback/inheritance is deleted. Path loading continues through `PathSandbox` and `PathResolver`; no replacement loader is introduced. Holder constructors continue projecting the Rust contract. |
| Registry persistence and version lines | Flat migration; card row types; cards declare/get/list/delete/relationships/version SQL/version resolve/field resolver/auth projection; service-account and verification-target consumers; registry integration additions and changed regression fixtures | SQL column lists, bind positions, named identity predicates, advisory lock inputs and returned typed projections remove space together. Existing `TenantConn`, declaration transaction, lifecycle, version allocator and projection owners remain in place. The new nested test helper needs the documentation correction below. |
| Server declaration and reads | Card routes, query DTOs, mapping, service `persist_node`, `resolve_existing`, inline Prompt projection/pinning, effective-spec/sibling resolution; hosted `TenantRegistry`; declaration route/OpenAPI/hosted-artifact tests | Public route and DTO changes are local to existing surfaces. Declaration remains in its existing transaction workflow. Hosted lookup uses the already-authorized exact-reference owner after the space-dependent fallback becomes unnecessary. |
| Shared client and offline state | `Cards`, `CardSelector`, get/delete/latest reads, graph fallback aliases, bundle reader/writer, state exact references and diagnostics, saga fixture projections; declaration/pull and state consumers | The public handle retains its method shape and shared transport. Selector variants still distinguish named/latest, UID assertions and exact references. Bundle aliases and offline identity checks remove the same dimension; no parallel bundle reader or state authority is introduced. |
| Auth and runtime consumers | Card-scope graph walk; service-account JSON containment projection; auth issuance/exchange/refresh/revoke test fixtures; shared principal/request-context/auth token fixtures; workload config and boot target construction | Identity closure is applied to existing owners. No new token parser, permission workflow, runtime account model or transport is added. |
| Python API and authored declarations | `src/state/mod.rs` summary, list/get selectors, resolve/delete, holder stamping and shared kwargs helpers; owner-crate PyO3 constructors; native testing CLI; public package exports/protocol and cards/data/model/prompt/config/auth/testing/verify stubs; registry-surface and declare/pull tests | Removed fields and parameters agree across native methods and stubs. Changing the four list boundaries and shared `RegistryListQuery` to `PyResult` keeps Python argument errors at the Python edge, using the existing `RegistryGetQuery` extraction conventions. It does not fork durable validation. |
| TypeScript API and generated declarations | Native `NativeCards::resolve_latest`, public `Cards.resolveLatest`, generated `card-types.ts`, both production napi declaration formats; native testing CLI selectors/options and both testing declaration surfaces; declare/pull tests | Native, wrapper and declarations consistently use kind/name. Wire CardRef, metadata, summary and list projections remove space. No unrelated type widening or new options are introduced by this diff. Generation commands were reported as passing, but were not rerun here. |
| CLI and agent-facing documentation | Card selectors/latest/list/check output, issue-key target parsing, init command, CLI journeys; MCP cards descriptors/guide and setup prompt; runtime docs snippets and generated error documentation | Existing commands and read tools retain their owners. Removed flags disappear rather than acquiring aliases. Runtime guide examples use the same flat contract. |
| Fixtures and remaining consumers | Checked-in Card graphs and bundle aliases/metadata/relationships; SDK fixtures/typecheck consumers; Skald Agent metadata and Vala drift/verification/correlation fixtures; examples and targeted docs/README changes | Changes follow the contract removal through consumers rather than introducing compatibility adapters. UI source searches found no Card-space property or Card-space selector requiring an implementation change; unrelated workspace/CSS terms are not identity consumers. |

## Existing-owner comparison

The materially changed parser/default/loader paths were compared with their pre-change implementations and callers, rather than accepted solely because the task named them:

- The pre-change `Ref` derived untagged decoding and the pre-existing custom `InlineableRef<T>` decoder already owned these forms. The change adds actionable malformed-reference diagnostics at those owners and preserves the reference-slot visitor. It does not introduce another schema walker. `CardRef`'s strict deserialization is the common field validator.
- `wyrd-config::apply_defaults` and `WyrdConfig::apply_defaults_fields` already serve wire metadata and holder metadata respectively. Both lose scalar-space handling while preserving label/annotation precedence. Existing constructor callers still go through `apply_repo_defaults`; no new discovery/cache path is added.
- `PathResolver` continues to own path containment, caching, depth and transitive discovery. The removed enclosing-space mutation is obsolete once reference identity no longer has that field. `card_ref_for` still requires the exact version used to create a sibling identity.
- Registry lookups, latest-version allocation, relationship rechecks and Card-principal projection remain their pre-existing SQL capabilities. The declaration service still owns their composition. Client get/pull/state and hosted verification reuse those capabilities rather than maintaining a second flat-identity workflow.
- Python list parsing uses its existing argument owner and extraction helpers. The error-type correction reaches all four callers; no new Python-only validator is added.

## Material finding

### MNT-001 — Required rustdoc is absent from a new test helper and incomplete on changed fallible registry helpers

Classification: **VIOLATION**. Blocking under the explicit rustdoc exception in root `AGENTS.md` and the task-review skill.

Changed locations:

- `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1114`, nested `register` in `flat_registry_identity_is_tenant_and_kind_scoped`.
- `crates/wyrd/wyrd-sql/src/queries/cards/declare.rs:461`, `select_card_uids_by_ref_batch`.
- `crates/wyrd/wyrd-sql/src/queries/cards/get.rs:143`, `find_card_by_ref`.

Rule: root `AGENTS.md` requires rustdoc on every new or changed Rust item and `# Errors` on fallible items. `agent_docs/rust.md` explicitly includes private helpers and test helpers. `maintainer-style.md` requires documentation to state meaningful error and workflow behavior. Missing rustdoc remains blocking even without a runtime defect.

Evidence and maintenance cost:

- The new nested `register` helper has no rustdoc. It opens its own tenant transaction, inserts a declaration operation and Card, commits on success, returns the expected Card-insert error, and panics on setup or commit failures. Its `Result` signature alone conceals that deliberate distinction; a maintainer changing the negative uniqueness test has to reconstruct which failures are expected to be observed and which abort setup.
- `select_card_uids_by_ref_batch` has only a one-line summary after this diff removes its old space-specific panic section. It returns registry errors for database failure, impossible row/ref correspondence and invalid stored UIDs, and omits unmatched/non-Active references from its result. There is no `# Errors` section describing the caller-visible contract.
- `find_card_by_ref` is changed to the new identity signature but still has only a one-line summary, despite returning both database-unavailable and invalid-stored-row failures. Its caller `service::resolve_existing` uses `None` to choose insertion/version behavior, so absence versus failure is consequential to safely maintaining declaration.

Nearby pattern: `get_card_by_ref` in the same `get.rs` module and `recheck_active_card_refs` in `relationships.rs` document their error contracts; the enclosing new registry test already documents its panic conditions.

Smallest correction: add substantive rustdoc to the new helper, including its transaction ownership, expected insertion error and setup/commit panic conditions; add accurate `# Errors` sections to the two changed fallible registry helpers. Preserve the current behavior and existing owners. No new test harness or abstraction is warranted. Source inspection of those items plus the existing narrow formatting/documentation checks is sufficient closure for this documentation finding.

Additional related documentation omissions to batch into that correction: the new `flat_card_reference_contract` test (`wyrd-spec/src/reference.rs:871`), `removed_space_defaults_are_rejected` (`wyrd-config/src/config.rs:195`) and `issue_key_refuses_removed_space_flag` (`wyrd-cli/src/auth/issue_key.rs:186`) document their intent but omit required `# Panics` sections despite their assertions and expect/unwrap calls.

## Verification and limits

Recorded task evidence reports passing focused reference/config/loader/registry/SDK/CLI/HTTP journeys, contract generation, napi parity, type checks, formatting and lint lanes. The source assertions were inspected as evidence of what the named tests actually prove; their names or green status were not treated as ownership proof. The task records environment limitations for OIDC, production UI and private saved-credential suites; this maintainer pass does not claim those ran.

No Cargo, build, test or generation command was run during this independent static review, as required by orchestrator coordination. No source, task, authority or other report was changed. This report proposes only MNT-001; there are no preference-only findings and no requested refactor.
