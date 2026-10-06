# SDK and contract domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Tasks: TASK-004, TASK-005, TASK-005-R1, and TASK-006

The candidate still resolved to the immutable commit above at the end of this
review.

## Boundary and authority coverage

| Surface | Authority and source inspected | Result |
|---|---|---|
| Compaction type/default contract | Spec REQ-011 through REQ-013; `wyrd-spec/src/vala/api.rs`; checked-in request/description schemas; `wyrd-client/src/bifrost/table.rs`; server registration service; catalog property projection; Forge settings and planning policy | FAIL — findings D-SDK-001 and D-SDK-002 |
| Rust SDK | `sdks/wyrd-sdk-rust/src/lib.rs`, its manifest, `wyrd-client::bifrost` exports, and the Rust Forge journey | FAIL — D-SDK-001 |
| Python SDK | Python runtime projection, PyO3 Bifrost projection, source stubs, generated stubs, public typing/unit/integration tests, and TASK-006's changed native docs | FAIL — D-SDK-003 and D-SDK-004 |
| TypeScript SDK | Native N-API projection, generated `.d.ts`/`.d.cts`, typed wrapper, unit test, and Bifrost integration journey | PASS |
| PyO3 ownership | New compaction boundary code remains in `sdks/wyrd-sdk-python`; TASK-006 edits outside it are documentation on approved existing owner-crate wrappers, not relocated behavior | PASS |
| Generated artifacts | Source schemas versus checked-in schema snapshots; source Python stubs versus assembled public stubs; N-API declarations versus native signatures | FAIL semantically — generation is internally consistent, but faithfully reproduces the wrong/default-drifted source docs and incomplete source stub |
| TASK-005 public contract seam | No new public filtering selector or second client contract was introduced; filtering remains an engine concern | PASS for this domain |
| TASK-005-R1 deadline/config projection | Existing `deadline_ms` request contract remains shared; local Oracle and `ReadyOracleForwarder` both use the boot-resolved `OracleConfig`; active-read acquisition binds the remaining duration as `p_deadline_ms`; protobuf removes the old field and retains only `reserved "reader_cut"` | PASS for this domain |
| Python test shape | Changed Python tests are top-level functions. Public tests exercise the public `wyrd.bifrost` projection; private imports in the unit fixture only load/check native registration helpers | PASS |

## Task acceptance matrix for this domain

| Task obligation | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| TASK-004 / REQ-011: absent enablement means compaction enabled | `forge/settings.rs:138-149`; property parsing preserves explicit `false` | settings unit test and recorded Bifrost journeys | PASS |
| TASK-004 / REQ-012: optional type crosses spec, server, catalog, and all three first-class SDKs | Wire/server/catalog/Python/TS paths are present, but `TableConfig::with_compaction_type` takes an SDK-unnameable `CompactionTypeWire` | Rust journey imports `wyrd_spec` directly rather than proving `wyrd-sdk-rust`; Python and TS journeys cover their public surfaces | FAIL — D-SDK-001 |
| TASK-004 / REQ-012: omit/repeat accepted; different stored value returns stable conflict | `wyrd-server/src/bifrost/service.rs:150-165`; catalog under-lock assertion; stable error projection in Rust and TS | server test and language journeys recorded in TASK-004 | PASS |
| TASK-004 / REQ-013: omitted type resolves to `small-files`; COW resolves to `full` | `forge/settings.rs:138-194`; `managed/policy.rs` builds the explicit two-file group filter and 75% threshold | settings/policy tests and Forge journeys | PASS in runtime behavior |
| TASK-004 / REQ-013: public contract and generated documentation state the same default | Rust wire/client docs and generated JSON schemas still state `full` | `codegen:check` can only prove source/snapshot equality, not semantic correctness | FAIL — D-SDK-002 |
| TASK-005: filtering changes do not create SDK/wire drift | No added user selector; no SDK filtering implementation | recorded tier tests; static contract inspection | PASS for this domain |
| TASK-005-R1: no reader-cut wire field or compatibility alias remains | `wyrd.v1.proto:222-223` reserves field/name only; no SDK projection remains | recorded `codegen:check`; source search | PASS |
| TASK-005-R1: default/explicit deadline reaches active-read acquisition consistently | `ReadyOracleForwarderInputs.config`; `OraclePlanner::request_deadline`; `ActiveReadOwner.deadline`; SQL acquisition binding | recorded S1/S7 evidence; static producer-to-sink trace | PASS |
| TASK-006: runtime help and typing sources accurately document every public API | `TableConfig` runtime/stub documentation is aligned, but the Agent source/generated stub is not the runtime API, and native callback help promises behavior the runtime cannot deliver | recorded format/lint/typecheck/codegen passes do not compare runtime introspection to the source stub or exercise raising after-callbacks | FAIL — D-SDK-003 and D-SDK-004 |
| TASK-006: generated files remain generator-owned | Source stubs and generated public stubs carry the same edits; no generator fork was added | recorded `codegen:check` | PASS |

## Material proposed findings

### D-SDK-001 — MISSING — the Rust SDK does not expose the compaction type it requires callers to pass

- **Violated obligation:** REQ-012 requires `compaction_type` end to end through
  the first-class Rust SDK, not merely inside the shared implementation crate.
- **Location:** `crates/shared/wyrd-client/src/bifrost/table.rs:233-248`,
  `crates/shared/wyrd-client/src/bifrost/mod.rs:51-69`,
  `sdks/wyrd-sdk-rust/src/lib.rs:27`, and
  `sdks/wyrd-sdk-rust/Cargo.toml:15-18`.
- **Evidence:** `TableConfig::with_compaction_type` and `compaction_type()` expose
  `wyrd_spec::vala::api::CompactionTypeWire`, but `wyrd_client::bifrost` does not
  re-export that enum. `wyrd-sdk-rust` re-exports only `wyrd_client` and does not
  depend directly on `wyrd-spec`, so an SDK consumer has no ordinary named path
  such as `wyrd_sdk::bifrost::CompactionTypeWire` from which to construct the
  required enum. The claimed Rust journey at
  `crates/wyrd/wyrd-testing/tests/bifrost/forge/live_rewrite.rs:9` imports the
  type directly from `wyrd_spec`, so it does not prove the public Rust SDK
  surface.
- **Observable consequence:** a normal `wyrd-sdk-rust` user cannot select
  `auto`, `full`, `small-files`, or `files-with-delete` without adding an
  implementation-level `wyrd-spec` dependency (or relying on contrived type
  inference). Rust is therefore not at parity with Python and TypeScript for
  the new public contract.
- **Required correction:** expose the existing wire enum through the owning
  public `bifrost` client surface so the thin Rust SDK re-export makes it
  nameable. Add a focused `wyrd-sdk-rust` compile/API test and drive the Rust
  registration journey through that public path rather than importing
  `wyrd_spec` for the option.

### D-SDK-002 — INCORRECT — canonical Rust and generated schema docs advertise the superseded `full` default

- **Violated obligation:** REQ-013 makes omitted `compaction_type` resolve to
  `small-files`; TASK-004 and TASK-006 require public contracts and generated
  docs to agree with that behavior.
- **Location:** `crates/wyrd-spec/src/vala/api.rs:143-168` and
  `crates/wyrd-spec/src/vala/api.rs:411-419`;
  `crates/shared/wyrd-client/src/bifrost/table.rs:60-65` and
  `crates/shared/wyrd-client/src/bifrost/table.rs:233-238`; generated snapshots
  `crates/wyrd-spec/schemas/bifrost_register_table_request.json:22,61-81` and
  `crates/wyrd-spec/schemas/bifrost_table_description.json:31,130-150` (with the
  same text in `tests/schemas`).
- **Evidence:** Forge's actual default is `ForgeCompactionType::SmallFiles` in
  `forge/settings.rs:138-149`, and the Python/TypeScript public docs say
  `small_files`. The canonical wire rustdoc instead says omitted means `full`
  and describes the `Full` enum variant as the default. Schema generation has
  propagated those false statements into both public JSON schemas.
- **Observable consequence:** Rust users and schema/documentation consumers are
  told that an omitted setting rewrites every live file, while the deployed
  worker selects only small files in groups of at least two. This is a direct
  public contract contradiction, not a cosmetic comment issue.
- **Required correction:** fix the canonical `wyrd-spec` and `wyrd-client`
  source documentation to state the revision-11 default, regenerate the schema
  snapshots, and prove the generated descriptions plus Rust/Python/TypeScript
  public docs all name the same omitted behavior.

### D-SDK-003 — INCORRECT — the TASK-006 Agent source stub and generated stub do not describe the runtime API

- **Violated obligation:** TASK-006 requires the runtime, source stub, and
  generated public stub to agree for every public callable and typed structure.
- **Location:** source
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:15-64,247-380`; generated
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi` carries the same shape;
  runtime `crates/skald/skald-agent/src/session.rs:30-60` and
  `crates/skald/skald-agent/src/python.rs:387-465,479-558`.
- **Evidence:** the stub omits runtime `Role.System`; declares `SessionTurn` as
  positional `role, content` plus `tool_call_id`, a `Role`-typed getter and
  `to_dict()`, while runtime requires keyword-only `role, content, call_id`,
  returns the role string, exposes `call_id`, and provides `model_dump`,
  `model_dump_json`, `model_validate`, and `model_validate_json`. The stub also
  omits runtime Agent methods `with_prompt`, `with_session`, `with_run_config`,
  and all six `add_*` callback registration methods. These are source-stub
  defects; regeneration correctly copies them and therefore cannot detect
  them.
- **Observable consequence:** type checking rejects valid public calls and
  accepts invalid constructor/property calls; editor help hides supported
  methods. The claimed all-public-API documentation inventory is incomplete.
- **Required correction:** reconcile the hand-authored Agent source stub against
  runtime introspection for the complete public Agent/session surface, then
  regenerate. Add focused public import/type assertions that cover the corrected
  constructor, properties, enum member, and methods so `py:typecheck` and
  `codegen:check` prove parity rather than only internal stub consistency.

### D-SDK-004 — INCORRECT — Python-visible Agent help promises safe exception handling that reaches `unreachable!`

- **Violated obligation:** TASK-006 requires accurate runtime help and forbids
  making documentation describe behavior the implementation lacks.
- **Location:** `crates/skald/skald-agent/src/python.rs:84-89` versus
  `crates/skald/skald-agent/src/python.rs:797-835` and
  `crates/skald/skald-agent/src/loop_runtime.rs:357-365,405-414,492-502`.
- **Evidence:** the Python constructor help says `after_agent_callback`,
  `after_model_callback`, and `after_tool_callback` may "raise to abort."
  `invoke_callback` converts a Python exception into `CallbackOutcome::Abort`,
  but each corresponding consumer treats `ChainResult::Abort` as unreachable
  and panics. This path is reachable whenever a documented after-callback
  raises.
- **Observable consequence:** a user following `help(wyrd.Agent)` can panic the
  agent execution path instead of receiving the documented callback-aborted
  outcome.
- **Required correction:** make the Python-visible native help, source stub, and
  generated stub state only behavior the runtime supports, and add a focused
  Python-runtime test for a raising after-agent, after-model, and after-tool
  callback. If an already-authoritative callback contract requires abort rather
  than panic, correct the existing callback owner at the shared source instead
  of documenting the panic; do not add downstream guards in each Python wrapper.

## Verification assessment and limits

- Reviewed the complete base-to-candidate changed-file inventory for the
  contract and SDK owners and traced the four findings through current source,
  callers, generated consumers, and relevant tests.
- Confirmed `git diff --check c1508b375..7ac45dec9` reports only two existing
  task/reference Markdown EOF blank-line warnings; no source whitespace issue
  affected this domain review.
- The recorded `mise run verify:bifrost`, `test:principals:integration`,
  `codegen:check`, Python typecheck, and TypeScript lanes are useful regression
  evidence. They do not falsify D-SDK-001 because no Rust SDK API test names the
  enum, D-SDK-002 because codegen reproduces source descriptions, or D-SDK-003
  because typecheck consumes the inaccurate stub rather than comparing it with
  runtime introspection. No current test exercises D-SDK-004's raising
  after-callback path.
- I did not start Cargo-backed verification in the shared checkout while other
  review agents were active. The findings are source-contract defects and do
  not depend on a timing-sensitive reproduction.
- No additional security, secret-handling, unbounded-work, or tenant-isolation
  defect was found in the reviewed SDK/contract boundary.

## Overall result

**FAIL**

The runtime compaction defaults and active-read deadline projection are
coherent, but the packet does not satisfy its first-class Rust SDK and public
documentation/stub obligations. Findings: D-SDK-001, D-SDK-002, D-SDK-003,
D-SDK-004.
