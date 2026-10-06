# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Authority: approved `changes/active/forge-concurrent-planning/spec.md`
  revision 11, `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/bifrost-design.md`, `architecture/wyrd-design.md`, and
  `architecture/references/languages/maintainer-style.md`

## Overall result

**FAIL**

The principal runtime owners are generally discoverable and cohesive, and the
packet deletes substantial superseded reader/planning machinery. The candidate
still fails the maintainer gate because changed public contracts disagree with
the implemented default, the Python typing sources remain observably different
from their runtime objects after an exhaustive documentation task, materially
changed Rust items are undocumented, and one new Forge module places durable
SQL outside the durable SQL owner.

## Changed-surface coverage

| Surface | Owners and consumers inspected | Assessment |
|---|---|---|
| Forge leadership and volatile schedule | `forge/leader.rs`, `forge/leadership.rs`, `forge/settings.rs`, `forge/scheduler.rs`, `forge/worker.rs`; peer handlers, SQL election owner, production routes, and capacity probes | The concrete `ForgeSchedule`, `ForgeLeadership`, and `ForgeWorker` owners are discoverable. `DueIndex` is task-earned by the approved O(limit log tables) selection requirement rather than speculative abstraction. Findings MNT-001, MNT-005, and MNT-006 apply at adjacent seams. |
| Promotion, maintenance, and cleanup | `forge/scribe_promotion.rs`, `forge/gc.rs`, `forge/expire.rs`, `forge/orphan_gc.rs`, `forge/table_authority.rs`; SQL operation owners and Forge integration tests | Promotion/expiration/orphan responsibilities remain separate and the removed planner/reader modules reduce conceptual load. The new table-authority SQL placement splits ownership (MNT-006). |
| Oracle active-read lifecycle | `oracle/planner.rs`, `oracle/mod.rs`, `oracle/exec.rs`, `oracle/query_stream.rs`; `vala-sql` active-read query owner and stream settlement consumers | `ActiveReadClaim` is a meaningful invariant-bearing owner and its propagation into stream settlement is legible. Documentation coverage and signature style are incomplete (MNT-004, MNT-005). |
| Scribe and Iceberg filtering/publication | catalog, promoted-object, managed-column, Scribe publication/staging/runtime, assignment predicate and field-ID changes; relevant integration/journey tests | The changes reuse existing Scribe/catalog owners and remove obsolete schema duplication. One materially changed digest helper lacks the required contract documentation (MNT-004). |
| Wire contracts and first-class SDKs | `wyrd-spec` Bifrost contracts and schemas, `wyrd-client::TableConfig`, server register/describe path, Python/TypeScript projections and tests | The type is carried end to end, but its default is documented inconsistently in source contracts and generated schemas (MNT-001). |
| Python documentation and declarations | Runtime `wyrd` modules, hand-authored `python/wyrd/stubs/*.pyi`, generated public stubs, PyO3 source for representative native classes | The generated files follow their hand-authored sources, but generation faithfully reproduces source declarations that do not match runtime (MNT-002 and MNT-003). |
| Capacity and regression harnesses | `bifrost_forge_capacity/{main,deployment,fleet,probe,report,schedule,host,dependencies}.rs`, shared capacity helpers, production journey support | The modules separate deployment, workload, measurement, and reporting rather than building a generic benchmark framework. No additional blocking maintainer finding was found in this surface. |
| Documentation and architecture | Bifrost architecture, Forge docs, quickstart/schema/configuration docs, revision-11 packet evidence | Product documentation describes the new `small-files` default correctly, which makes the contradictory generated/API documentation in MNT-001 more costly rather than harmless. |

## Material findings

### MNT-001 — Public compaction contracts still document the superseded `full` default

- Severity: **must-fix**
- Changed locations:
  - `crates/shared/wyrd-client/src/bifrost/table.rs:63` and `:235`
  - `crates/wyrd-spec/src/vala/api.rs:143-164`
  - generated schemas including
    `crates/wyrd-spec/schemas/bifrost_register_table_request.json:22` and
    `crates/wyrd-spec/schemas/bifrost_table_description.json:31`
  - `sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts:232`
- Governing principle: public names, types, generated declarations, tests, and
  documentation must describe the same operation; generated artifacts must be
  corrected through their source. Revision 11 REQ-013 makes omitted
  `compaction_type` resolve to `small-files`.
- Evidence: `ForgeTableSettings::default()` uses
  `ForgeCompactionType::SmallFiles`, and the Forge documentation says the same,
  while the newly added Rust contract docs, schema descriptions, and TypeScript
  test name say omission means `full`.
- Maintenance cost: a maintainer cannot tell whether changing the server,
  client, schema, or test is a bug fix because the repository publishes two
  incompatible defaults. Code generation then multiplies the wrong source
  description into every consumer surface.
- Smallest correction: make the `wyrd-spec` and `wyrd-client` source rustdoc
  describe the revision-11 `small-files` default, rename the TypeScript test to
  assert omission rather than `full`, and regenerate the JSON/public SDK
  projections. Do not add another compatibility/default layer.

### MNT-002 — `SessionTurn` source stubs describe a different Python object than PyO3 exports

- Severity: **must-fix**
- Changed location: `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:18-64`
  (copied to `python/wyrd/agent/__init__.pyi`).
- Runtime evidence: `crates/skald/skald-agent/src/python.rs:492-535` exports a
  keyword-only constructor `(*, role, content, call_id=None)`, returns `role`
  as `str`, exposes `call_id`, and provides `model_dump` / `model_dump_json`.
  The source stub accepts positional `role` and `content`, calls the field
  `tool_call_id`, types `role` as `Role`, and advertises `to_dict` instead.
- Governing principle: `maintainer-style.md` requires public Python signatures
  and generated declarations to match the runtime contract; TASK-006 explicitly
  required discovered typing drift to be fixed at its source.
- Maintenance cost: editors approve calls that fail at runtime and reject the
  actual API. A future maintainer must inspect Rust to discover even the field
  name, defeating the purpose of the declared Python surface.
- Smallest correction: update the hand-authored `SessionTurn` declaration to
  the exact PyO3 signature, properties, and methods, include the runtime role
  values (including `system` where exposed), regenerate the public stub, and
  add one top-level Python typing/runtime parity test for construction and
  accessors.

### MNT-003 — `WyrdError` source stubs retain a constructor the runtime exception does not implement

- Severity: **must-fix**
- Changed location: `sdks/wyrd-sdk-python/python/wyrd/stubs/error.pyi:37-52`
  and the same constructors on its subclasses.
- Runtime evidence: `crates/shared/wyrd-utils/src/py.rs:13-21` creates ordinary
  PyO3 exception classes. The changed stub advertises keyword-only `details`
  and `remediation` parameters even while its new docstring concedes that
  direct construction merely stores positional `args` and does not populate
  the declared attributes.
- Governing principle: typed declarations cannot claim parameters or state
  that documentation immediately says do not exist; TASK-006 required runtime
  and typing surfaces to agree.
- Maintenance cost: this is a self-contradictory public API. Type checking
  endorses construction that the runtime rejects, and subclasses duplicate the
  same false signature four times.
- Smallest correction: remove the invented specialized constructor signatures
  and describe the actual exception-construction contract, keeping
  `build_wyrd_error` as the one typed route to a populated structured error;
  regenerate public stubs and cover direct versus builder construction in a
  top-level Python test.

### MNT-004 — Materially changed Rust items still lack substantive rustdoc

- Severity: **must-fix**
- Changed locations:
  - `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:336-401` — the
    `build_frames` description was left attached to `next_frame_event`, while
    materially changed `build_frames` has no rustdoc at all.
  - `crates/wyrd-spec/src/vala/assignment_authority.rs:270-290` —
    `push_literal` was extended for byte literals and remains undocumented;
    both it and `push_predicate` are fallible without `# Errors`.
  - `crates/vala/vala-bifrost-redux/tests/integration/forge/expired_cleanup.rs:500-501`
    — the materially renamed gate test has no rustdoc or `# Panics` section.
- Governing rule: `AGENTS.md` section 16 and `architecture/agent-rules.md`
  require substantive rustdoc on every new or materially modified Rust item,
  including private helpers and tests, with `# Errors` and `# Panics` where
  applicable. Missing documentation is explicitly a hard blocker.
- Maintenance cost: the most lifecycle-sensitive function in the new Oracle
  release path has no stated ownership/cancellation contract, while its
  neighboring helper carries the wrong introductory sentence. The digest
  encoder does not state its canonical byte contract or failure conditions,
  and the cleanup regression test does not state the invariant it proves.
- Smallest correction: move the lazy-stream ownership text onto
  `build_frames`, leave `next_frame_event` only its own event-selection
  contract, and document the digest helpers and test with their actual
  invariants and error/panic behavior. Do not add a documentation-suppression
  attribute.

### MNT-005 — New Rust signatures hide dependencies behind fully qualified paths

- Severity: **should-fix**
- Changed locations:
  - `crates/vala/vala-bifrost-redux/src/forge/settings.rs:96-111`
  - `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:88-125`
  - representative worker construction at
    `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2804-2823`
- Governing rule: `architecture/agent-rules.md` requires dependency types to be
  imported at module scope and used by bare name in fields, signatures, trait
  impls, and bounds; function-scoped `use` statements are disallowed except for
  the documented narrow trait case.
- Evidence: the new conversion impls spell
  `wyrd_spec::vala::api::CompactionTypeWire` in impl/signature positions and
  import it inside `from`; `ActiveReadClaim` spells `wyrd_spec::DataTenantId`
  and `uuid::Uuid` in fields/signatures; the new dispatch envelope constructs
  `vala_sql` row types by full path despite the module already owning a large
  dependency list.
- Maintenance cost: these modules no longer expose their dependency surface in
  the top import block, and equivalent symbols are written in several forms.
  Refactors require textual path hunting instead of one import update.
- Smallest correction: add the used types to the module-level imports and use
  their bare names consistently. This needs no wrapper, alias layer, or new
  module.

### MNT-006 — The new Forge authority wrapper owns durable registry SQL outside `vala-sql`

- Severity: **must-fix**
- Changed location:
  `crates/vala/vala-bifrost-redux/src/forge/table_authority.rs:45-72`.
- Governing principle: `AGENTS.md` section 3/15 assigns Vala durable Postgres
  behavior to `vala-sql`; the maintainer guide requires workflows to live with
  their existing owner. The same module already delegates the active-read and
  claimed-snapshot statements to
  `BifrostTableMaintenanceAuthority` in `vala-sql`.
- Evidence: `TableAuthority::identity` embeds a raw query against
  `vala.bifrost_tables`, decodes the persisted UID, and then hands the result to
  the SQL-layer owner. Its four consumers consequently depend on a Redux-local
  reconstruction step before they can call the durable authority owner.
- Maintenance cost: table identity schema and decoding now have two owners:
  the durable SQL layer and a Forge engine module. A schema or identity change
  can update the query owner without updating this raw consumer, and callers
  must compose two layers to perform one authority lookup.
- Smallest correction: move the tenant-scoped table-identity resolution into
  the existing `BifrostTableMaintenanceAuthority` (or an existing `vala-sql`
  table-registry query owner) and have the Redux `TableAuthority` delegate to
  that typed method. Keep the Redux struct only as the transaction-scoped
  orchestration owner; do not introduce a second repository trait.

## Calibration notes

- No finding is raised solely for the size of `ForgeWorker` or the capacity
  harness. The changed methods were traced to their existing state-owning
  structs, and the benchmark modules have distinct deployed responsibilities.
- No finding is raised for the internal due index: revision 11 explicitly
  requires indexed oldest-due selection and a comparison test, so the added
  structure has a current caller and a measured reason.
- The deletion of `planning_scheduler.rs`, `reader_protection.rs`,
  `reader_pins.rs`, and the duplicate managed-column module is materially
  simpler and aligns with the approved removal scope.

## Verification limits

This was a read-only maintainer audit of the immutable candidate plus the one
assigned report. Reported green `verify:bifrost`, principal integration,
format, lint, codegen, and diff checks were considered evidence but do not
cover the semantic documentation/declaration mismatches above. No Cargo-backed
lane was rerun because the findings are source-visible and the shared checkout
was under concurrent review.
