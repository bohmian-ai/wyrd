# Repository standards review

Result: **PASS**. No material repository-rule findings.

## Immutable subject and method

Reviewed the complete `5bcd9e4872f1376f1722922fa63b67384243be04` →
`eeac9c9a45b599b1727086ba30582d3f02b3230a` range against the current repository
authorities and approved specification revision 1. HEAD remained the pinned
candidate. This was an independent standards discovery pass; no sibling
reviewer conclusions were read. No reviewed source was edited and no Cargo
command was run.

CodeGraph was used before source navigation. The shared navigation map was
expanded with the complete diff, the removed Rust test bodies, surrounding MCP
descriptor and dispatch code, Python runtime/stub exports and moved helpers,
the TypeScript public module/type fixtures, generator consumers, package
configuration, CI selection and docs workflows. No nearer AGENTS.md or
CLONES.md was found under the changed source trees.

## Authority coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, the complete
reference router, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, the approved spec/task, and the workflow
references `languages/spec-driven-development.md` and
`languages/implementation-execution.md`.

| Changed surface | Additional applicable authorities read | Source coverage |
| --- | --- | --- |
| Rust schema extraction/example; deleted contract snapshots/readers | `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/maintainer-style.md`, `languages/errors.md`, `languages/testing-workflows.md` | `gen_schemas.rs` complete; every changed `wyrd-spec/src` diff and removed snapshot assertion; `wyrd-spec/Cargo.toml` |
| Native Python Card read and aliases; Skald wrapper comment | Same Rust/ownership authorities plus `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md` | `PyCards::get`, `PyCardEnvelope` fields/aliases and existing projection; shared Rust read/identity owner; wrapper module change; SDK feature manifest |
| Authored Python stubs, moved Python definitions, callable schemas, package/config/tooling | Python/PyO3, maintainer, errors and testing references | Public/root stubs and exports; private `_card`, `_session`, `_client`, `_policy`, `_telemetry`, `_commands`; tool decorator/introspection; `_schema` deletion; `check_stubs.py`; `pyproject.toml`; removed assembler/template tree |
| Python journeys, support functions and removed generated-model unit test | Python API, testing, maintainer and implementation-execution references | Registry content/identity journey, workflow relationship helpers, state trusted-artifact helper, binding-id helper and other mapping conversions; deleted test's generated-model-only purpose |
| TypeScript lowerer, generated types, public reexports and compile-only tests | TypeScript guide, Rust contract/ownership, maintainer and testing references | `gen_ts_types.py`, generated `card-types.ts` diff, `src/index.ts` import/reexport and declaration removal, both changed type fixtures; existing native/public declaration ownership |
| MCP input schema construction | Agent-harness, Rust, errors, ownership and testing references | Gateway descriptors, literal schema helper, common `principals::schema_of`, dispatch body types, permission filtering and existing catalog regression |
| Docs generators, indexes and diagram checks | Agent-harness, maintainer and testing references | Both surviving Python generators, deleted inactive renderer, llms index/appendix changes, diagram enum comparison, docs tasks and workflows |
| mise/CI selection, verification and repository-rule updates | Testing, agent-harness, Python API, PyO3, maintainer, architecture patterns; `TESTING.md` | Complete config/rules diff, relevant task bodies, CI selector and updated selector assertion, docs setup/publish jobs; original wording compared with approved authored-stub decision |

No SQL signatures/fields, transaction lifecycle, server URL fetches,
authentication decisions, audit writers, durable tables, or Bifrost engine
behavior are materially changed. SQL capability, security, analytical
execution/storage and operations authorities therefore add no separate
changed implementation surface. The relocated Python Bifrost facade keeps
calling the same native facade; contract/type projection changes do not move
ingest, query, admission or durability ownership into Python.

## Rule results

| Rule | Result | Evidence |
| --- | --- | --- |
| AGENTS §§2–3: Rust contract authority, sole shared client, SDK ownership, foundational exclusions | PASS | Contract extraction remains a `wyrd-spec` example using existing derives. Library changes only remove snapshot tests. Python retains `Card` inside the existing SDK-owned envelope and calls `self.inner.get`; TS projects Rust roots. No manifest dependency or feature is added. |
| AGENTS §§4–6: ownership, struct-centered Rust, synchronous pure work, borrowing and error handling | PASS | Existing `PyCards`/`PyCardEnvelope` own retained dependencies/data. `contracts`, `literal` and `schema_of` are invocation-local deterministic helpers, not replacement service owners. The registry call releases the GIL and uses the existing runtime. Alias strings are borrowed into `PyTuple::new`. Invariant schema serialization failures remain explicit. |
| AGENTS §16 and agent-rules rustdoc requirement | PASS | New `roots!`/`contracts`/`literal`, materially changed generator `main`, Card getter/alias getter and fields have workflow/invariant docs; fallible functions state Errors and invariant schema helpers state Panics. MCP descriptor docs explain the request-type derivation. Removed Rust test helpers are not surviving undocumented changed items. No new Rust test helper/function is introduced. |
| AGENTS §§7–8: PyO3 boundary, authored native stubs, single inline pure-Python declaration, public exports | PASS | The native change stays in the Python SDK, uses existing registered `CardEnvelope`, and retains structured error conversion. Cards stubs return the native envelope. Runtime protocols and Python helper APIs live once in stub-less modules and are reexported. Root `cards.Card` reassignment and generated model exports are removed. Lint/stub validation and public typechecking pass. |
| Python API/stubs documentation/export/layout obligations | PASS | `check_stubs.py` validates public documentation/Args, constructors, duplicate class names within a stub, matching package `__all__`, root layout, and reexported private Python definitions without generating declarations. Existing public native declarations remain authored in place; generated banners/template assembly are removed. |
| TypeScript guide: generated wire authority, explicit public typing and separate N-API declarations | PASS | Public names reexport generated Rust-derived types. Generated fields retain readonly declarations; closed string sets are unions. Existing N-API declaration generation and public tsc declaration ownership are preserved. Type fixtures add timestamp/owning-enum assertions and account for Rust nullability. Independent compiler and lint checks pass. This standards pass does not substitute compilation for the domain review of lowering precision. |
| Agent-harness: MCP contracts derive from the actual DTO; permission/error ownership stays server-side | PASS | All five replacement descriptors call the common `schema_of::<T>()`; dispatch deserializes those same DTOs and still delegates to `GatewayAdministration`. Argument-free/named literal schemas, permission filtering and effects remain. No new schema authority or route is introduced. |
| AGENTS §§11–12: meaningful tests retained; authorized check retirement | PASS | Deleted Rust tests only generate/compare retired snapshots. Serialization/validation/error tests remain. The Python generated-model test is retired with that model; the real-server retrieval journey retains per-kind content checks and adds native identity, alias and serialization assertions. Workflow/state/support assertions become equivalent mapping reads. Diagram enum agreement still compares Rust-derived values. |
| AGENTS §12: checks must protect live boundaries without duplicate authority | PASS | Stub copying/drift is replaced by checks of the sole authored files. `codegen:check` continues hashing consumed UI/error/TS outputs and running theme-token checks. The retired files/snapshot checks are explicitly authorized in REQ-001/003/006/009 and INV-005; no deleted-name ban or new dependency is introduced. |
| Verification scope and command precision | PASS with recorded limits | Task evidence records exact renamed Python journey and gateway test commands, RED/GREEN, language lints/typechecks, units, formats, codegen, docs and N-API checks. The independent verification record corroborates all three lints, Python typing and the TS compiler body. The approved task explicitly defers the integrated `gate` to change review; its absence here is not a task-review violation. |
| Rule/documentation updates preserve stronger authority | PASS | Updated AGENTS/agent-rules and routed references consistently distinguish authored `.pyi` from actually generated TS/error/UI/N-API artifacts, as expressly approved by REQ-009. Changes do not waive runtime tests, native signature parity, MCP checks, or the remaining generated-artifact rule. |
| SQL capability types, tenancy, durable writes, audit, SSRF | N/A to changed behavior | No production SQL connection signature/field, query, permission evaluation, audit path or outbound server fetch is added or modified by the cumulative diff. MCP request parsing remains at its existing authorization owner. |

## Non-blocking observation

`mise.toml:559`, `:607` and `:619` remove `pipefail` from three existing
benchmark bash bodies. These edits are unrelated to the stated cleanup, but
the inspected bodies and exported `phase` contain no pipelines, so no current
failure-propagation consequence was established. They are not a material
finding under this review's threshold and do not justify a standalone
remediation round.

## Findings and verification limits

Material finding ledger: **empty**.

This reviewer independently ran `mise run py:lints` (Ruff and stub checker:
PASS) and `git diff --check 5bcd9e4 eeac9c9` (PASS). Other independently
executed checks and their exact limits are recorded in `verification.md`.
Earlier runtime, regeneration, docs and packaging-related results remain
implementation evidence, not newly executed tests by this reviewer. No fresh
native build, live server journey, codegen mutation, or broad integrated gate
was run in this standards pass. The result is a standards acceptance judgment,
not a claim that every required change-review lane has been rerun.
