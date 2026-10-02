# TASK-002 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

The candidate was exactly the stated commit at review start. CodeGraph could
not supply a navigation result because its CLI reported that no usable
`.codegraph` index exists, so this review traced the cumulative diff and callers
with repository search and direct source inspection.

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Loader parsing and validation | `materialize_inline_files`, `is_inlineable_slot`, `unwrap_reference_form`, `is_inlineable_body`, `validate_card`, `load`, `resolve_tree`, the canonical `ReferenceSlotVisitor`, parser/loader tests, and the checked-in bundle | Pure Workflow validation is placed on the existing loader pipeline and diagnostics remain source-aware. The new raw-YAML keyed-form normalization duplicates and incompletely covers the canonical inlineable-slot inventory (MNT-001). |
| Shared client composition | `WorkflowLoader::{new,with_client,load_file,load_registered,fetch_missing,read}`, `WorkflowGraph::{new,from_submission,insert,missing,validate,hydrate}`, `GraphResolver`, conversion helpers, root exports, Cards exact reads, Skald resolver traits, and unit tests | `WorkflowLoader` is a meaningful dependency-owning handle; IO methods remain async and graph work remains synchronous. Names, argument/return types, errors, and rustdoc expose the main path clearly. |
| Skald runtime seam | `Workflow::{from_card,from_card_with_agent_resolver,from_yaml_str,load}`, `AgentResolver`, `PromptResolver`, `Agent::from_card`, and relevant workflow/agent tests | The candidate documents rather than duplicates bundle loading. Resolver ownership stays outside Skald and the runtime remains the single hydration/execution seam. |
| Server registration composition | `resolve_card_references`, `EffectiveSpecs::{new,validate_bindings,validate_baselines,validate_workflows,load}`, reference binding helpers, Cards registration callers, and SQL lookup owners | `validate_workflows` belongs on the existing effective-spec owner and reuses its tenant-scoped cache/read path. The staged loop is discoverable and documented; no new free-function workflow or duplicated executor appears. |
| Cargo/dependency shape | `wyrd-client` and `wyrd-server` manifests plus lockfile edges and existing server uses of `wyrd-client` | Added Skald dependencies are used at their owning boundaries; no new feature or third-party dependency was introduced. The server already depended on `wyrd-client`; the candidate reuses that established edge. |
| Integration and journey proof | `workflow_loader::tests::hydrate_local_workflow_graph`, `wyrd-loader::tests::load_explicit_workflow_bundle`, all helpers and both journeys in `pg_workflow_registration.rs` | The external test target earns its place by driving the real client/server/Postgres boundary. Tests assert durable identities, relationships, refusal codes, no partial writes, pinned execution, and no dispatch after refusal. Scenario names and helpers are domain-specific despite the necessarily broad journeys. |
| Examples and declarations | Code-review Workflow, three Agent Cards, input, `examples/README.md`, public Rust exports, and searches across Python/TypeScript/generated declarations | The example uses the native Prompt request shape and the same loader/runtime. Revision 11 deliberately adds no Python/TypeScript Workflow-loader surface, so no generated declaration counterpart is required in this task. |

## Material finding

### MNT-001 — Loader keyed-form normalization is a second, incomplete reference-slot inventory

- **Changed location:** `crates/shared/wyrd-loader/src/parse.rs:248-265` and
  `:316-370`.
- **Governing rule:** `architecture/wyrd-design.md` “Reference-slot inventory”
  says every reference-bearing field is owned in one canonical inventory and a
  new field participates in loader projection without a second edit. The task's
  REFACTOR instruction likewise requires reuse of that inventory rather than a
  Workflow-specific path parser. The maintainer guide requires shared rules to
  stay with their owner so callers do not need to know where an agent duplicated
  them.
- **Evidence:** `is_inlineable_slot` manually recognizes only `prompt`,
  `judge_ref`, and an Agent action's `target`. The canonical visitor currently
  also owns `InlineableTrigger` at `verified_by[].runs_on` and
  `InlineableOperator` at `verified_by[].on_failure[]`
  (`crates/wyrd-spec/src/refs/mod.rs:35-45,488-527`). Because normalization
  happens before typed deserialization, authored keyed forms such as
  `runs_on: { inline: ... }` and an `on_failure` element using `ref:`, `path:`,
  or `inline:` remain wrapped and fail to decode. The only new focused parser
  test exercises a `prompt` slot, so it cannot detect this divergence.
- **Concrete maintenance cost:** maintainers must now update two inventories for
  every inlineable reference slot, and the new helper is already stale against
  current source. Valid authoring behavior therefore depends on the field name
  and can regress whenever the canonical visitor grows, contrary to the
  repository's single-source rule.
- **Smallest testable correction:** make loader discriminator normalization
  project from one reference-slot owner that covers every current
  `InlineableRef` shape, rather than retaining the three-name list in
  `parse.rs`. Add one focused loader test that exercises the existing
  `runs_on` and `on_failure[]` keyed forms (including list-element handling) and
  keep the Workflow `target`/Agent `prompt` bundle proof green. Do not add a
  second general parser or change the wire shape.

## Calibration notes

- `WorkflowGraph::bodies` stores `(CardRef, Spec)` although current consumers
  read only the `Spec`; deleting the unused stored reference would be smaller,
  but it creates no concrete unsafe-change cost in this task and is not a
  blocking finding.
- The two Postgres journey functions are long, but their breadth follows the
  task's required cross-boundary matrices. Line count alone is not a finding,
  and the shared setup/refusal helpers keep the scenarios followable.
- `WorkflowGraph` is root-re-exported beside `WorkflowLoader`. The graph is also
  the intentional server/client validation handoff, so treating that exposure
  as removable would require an ownership decision beyond a maintainer-style
  preference; no finding is raised.

## Verification notes

- Reviewed the complete base-to-candidate diff, all changed Rust modules,
  manifests, examples, direct callers, and relevant tests.
- `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c..e165360b1264d3628b13b02c41567c047bf96930`
  exited 0.
- The task records successful focused loader/client/Postgres tests and the
  `test:shared`, `test:skald`, `test:cards:integration`, `codegen:check`,
  `check:client-tier`, `check:pyo3-scope`, `check:registry-tx-coupling`, `fmt`,
  and `lints` lanes. This reviewer inspected that evidence but did not rerun the
  cargo/mise lanes; independent orchestration owns final verification.
- Candidate immutability must be rechecked by the orchestrator after all review
  reports are complete.

## Overall result

**FAIL** — MNT-001 leaves the changed loader authoring seam with a duplicated,
already-incomplete slot inventory. The remaining reviewed surfaces satisfy the
maintainer-style requirements.
