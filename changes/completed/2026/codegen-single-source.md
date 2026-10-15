# One source for SDK types and consumed generated outputs

- Change ID: `SPEC-codegen-single-source`, approved revision 1.
- Completed: 2026-10-14.
- Reviewed task candidate: `eeac9c9a45b599b1727086ba30582d3f02b3230a` (task review FIX_REQUIRED, one finding).
- Remediation: TASK-001-R1 at `ffda2b6a4`, evidence `f03849e5a`; not re-reviewed.
- Completion target: `ca6eefa1a96acc11496cbd001e8415013da64f59`, after integrating main `3fad910f3cba59ad8544543c74b2d12b6468c3aa`.
- Delivery reference: branch `split/codegen-single-source`.
- Completion authority: explicit owner override without a remediation re-review or final change review. No change-review PASS is claimed.

## Intent and shipped behavior

Each language has one editable typing authority per API, and every generated
projection has a real consumer. Committed JSON Schema intermediates under
`crates/wyrd-spec/schemas/` and the eval/trace/observation schema fixture trees
are gone; `ui_problem_examples.json` remains generated from the Rust error
catalog and consumed by the UI.

- The five gateway MCP administration tools derive their input schemas from
  their owning Rust request types; tool names, descriptions, annotations,
  permissions, and execution are unchanged.
- `llms.txt`/`llms-full.txt` keep the page index without raw schema appendices;
  the dead card-docs generator and schema inventory are removed, and the
  diagram/server enum agreement reads transient Rust-derived contracts.
- Python `Cards.get` returns the native `wyrd.CardEnvelope` with exact
  identity, complete content, and `aliases == ()`. The generated Python Card
  model (`_card_types.py`, `Registered*`) is removed without compatibility
  aliases; `@tool` annotation schemas remain.
- Python package `.pyi` files, including `_wyrd.pyi`, are authored directly;
  `python/wyrd/stubs/` and the copy/assembly stage are deleted. Pure-Python
  definitions are declared once inline and re-exported by the stubs.
- TypeScript wire types in `card-types.ts` are generated from `wyrd-spec` roots
  through the transient `gen_schemas contracts` bundle; `index.ts` re-exports
  them under the existing public names. Response presence and nullability
  follow Rust serialization: always-emitted `Option` fields are required and
  nullable, fields skipped when `None` are optional and non-null.

## Lasting constraints and decisions

Rust `wyrd-spec` owns wire fields; SDKs project them. Serialization shape for
the TypeScript projection is declared at the Rust field: `#[serde(default)]`
on an always-emitted `Option` yields a `default: null` the output lowering
treats as required, and `wyrd_spec::schema::skipped_option::<T>` keeps the
ordinary `Option<T>` input schema while recording the serialized value in the
`x-wyrd-serialized` extension. Inputs stay lenient; no field lists, copied
interfaces, or consumer guards exist. Unsupported schema constructs fail
generation rather than widening to `JsonValue`.

Authored stubs are validated by `sdks/wyrd-sdk-python/scripts/check_stubs.py`
under `py:lints` (docstrings with `Args:`, `__init__` not `__new__`, no
duplicate classes, `__all__` parity, layout). Typechecking does not prove
native signature parity, so runtime and export tests stay. No dependency,
feature, extractor, or generator framework was added.

The intended compatibility break is removal of the generated Python Card model
and mapping-based retrieval. Wire, runtime, auth, tenancy, and durable formats
are unchanged.

## Acceptance evidence and limits

| Obligation | Evidence retained in the implementation |
|---|---|
| AC-001 schema retirement | Tree audit; `codegen:check` reproduces UI examples, error codes, and wire types. |
| AC-002 MCP schemas | Exact `gateway_catalog_annotates_every_administration_operation`. |
| AC-003 docs | `docs:check`. |
| AC-004 Python envelope | Exact real-server journey `test_cards_get_returns_every_kind_as_envelope` (red then green); Python integration lane. |
| AC-005 Python stubs/tools | `py:lints` including stub validation, `py:typecheck`, `py:test:unit` (592 passed). |
| AC-006 TypeScript wire types | `ts:typecheck` with strengthened type fixtures (11 failures before R1, green after), `ts:test:unit` (66), `ts:napi:check`, R1 schema helper unit test, `wyrd-spec` tests (878). |
| AC-007 lints | `lints`, `py:lints`, `ts:lints` pass. |
| AC-008 pipeline and rules | `codegen:check`, `check:ci-selection`, format checks, reference audit; `AGENTS.md`, agent rules, Python/testing references updated. |

After the main integration, `ts:typecheck` and `py:typecheck` passed again. The
spec's integrated `mise run gate`, a re-review of TASK-001-R1, and the final
change review were not run, at the owner's direction. Served OpenAPI was not
re-proved after R1; the added annotations are schemars-only or keep `Option`
fields optional.

## Current authority, owners and tests

- [AGENTS.md](../../../AGENTS.md) §8, [agent rules](../../../architecture/agent-rules.md), [Python API and stubs](../../../architecture/references/languages/python-api-and-stubs.md), [TypeScript guide](../../../architecture/references/languages/typescript-guide.md).
- [Schema helpers](../../../crates/wyrd-spec/src/schema.rs), [contract bundle](../../../crates/wyrd-spec/examples/gen_schemas.rs), [TypeScript lowerer](../../../scripts/gen_ts_types.py), [generated wire types](../../../sdks/wyrd-sdk-ts/wyrd/src/card-types.ts).
- [Gateway MCP catalog](../../../crates/wyrd/wyrd-server/src/mcp/gateway.rs), [llms generator](../../../docs/scripts/generate_llms_txt.py), [stub checker](../../../sdks/wyrd-sdk-python/scripts/check_stubs.py).
- [Python retrieval journey](../../../sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py), [TypeScript type fixtures](../../../sdks/wyrd-sdk-ts/wyrd/tests/types/).
