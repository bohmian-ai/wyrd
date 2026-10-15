---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-sdk-client-instantiation
spec_revision: 9
requirements: [REQ-002, REQ-006, REQ-009, REQ-012, REQ-016, INV-002, INV-003, INV-004, AC-006]
depends_on: [TASK-001]
parent_task:
remediates: []
---

# Document no-client SDK instantiation as the default

## Outcome and Value

Readers of the docsite and root README understand that `client` is always
optional. Common examples instantiate handles without a client and let Wyrd
read environment or saved configuration automatically. Explicit clients are
shown separately for testing, advanced configuration, or delegated identity.
Python's eager factories and lazy constructors have the same optionality.

Execute with `$wyrd-implement`. This task owns the docsite and README portion
of REQ-012 and AC-006. TASK-001 owns runtime behavior, architecture/language
references, executable examples, SDK API docs, stubs, and SDK declarations.
The dependency is real: examples must match the implemented signatures and
awaiting behavior, and docsite `CodeFromFile` references consume example sources
that TASK-001 migrates.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| First SDK connection | `docs/src/content/docs/get-started/setup.md` — Check it works; `get-started/quickstart.md` — Prerequisites and language tabs | Setup explains saved credentials; quickstart consumes support-desk sources with `CodeFromFile`; `docs:check` | No compact cross-language explanation that client omission is the normal form for every handle | Extend existing setup guidance with no-client examples and environment/saved-configuration explanation; link from quickstart where useful | None; reuse existing pages and language tabs |
| Handle usage and optional explicit configuration | `understand/identity-and-authorization.md` — credential lookup and delegation; `operate/grant-and-inspect-permissions.md` — Principals examples; `build-and-verify/observe/write-and-query-custom-data-with-bifrost.md` — registration examples | Rust Principals explicitly builds `WyrdClient`; TypeScript HTTP examples use synchronous factories; Python Bifrost examples use constructors | Old forms and timing remain; examples do not clearly distinguish default ambient usage from optional explicit clients | Update relevant handle snippets and imports to TASK-001's API, leading with no-client forms and labeling explicit forms optional | None; do not duplicate resolution or introduce a new configuration guide |
| README entry points | Root `README.md` — verification workflow and Documentation; `examples/support-desk/README.md` — environment setup; `examples/README.md` and language READMEs | Root workflow uses local WyrdState loading; support-desk README already names `WYRD_SERVER_URL` and `WYRD_API_KEY` | Root README does not state the SDK handle default | Add a compact root explanation and link to setup; update relevant existing README guidance without replacing local loader examples | None; no new README or tutorial required |
| Generated docsite references and verification | `docs/scripts/generate_llms_txt.py::{sidebar_pages, read_frontmatter}` and existing `docs:generate` / `docs:check` tasks | `docs:check` generates API pages and agent indexes, checks drift, tests content, checks commands/links, builds, and checks accessibility | Generated outputs and embedded source ranges may need refresh after content/source changes | Refresh through existing generators, update affected `CodeFromFile` references, and run the existing docs gate | None; no new checker or generator |

Paths are guidance, not an implementation allowlist. Sweep relevant docsite
pages and READMEs that construct in-scope handles; leave unrelated prose alone.
Keep existing page structure, language tabs, and product workflows.

Do not change SDK code, API shape, configuration precedence, credential
selection, authentication, permissions, server behavior, or test fixtures.
Do not add compatibility names or new root Python exports. `WyrdClient`
construction, stock-client authentication adapters, and local loaders retain
their existing roles. Do not remove a justified explicit client from a
delegation or transport-sharing example merely to make every snippet identical.
Generated docsite artifacts must come from their source and existing generator.

## Approach

1. Read TASK-001's implemented public signatures and authority updates.
2. Make setup and the root README explicitly lead with no-client instantiation;
   explain environment/saved configuration without restating resolver logic.
3. Update relevant language tabs, imports, awaiting, README guidance, and
   embedded-source references; show optional explicit clients separately.
4. Regenerate affected docsite outputs and verify the existing docs gate plus
   the focused static acceptance review.

## Proof Strategy

TDD is not applicable: this task changes documentation and generated docsite
outputs only. Runtime optional-client and eager/lazy behavior is proved by
TASK-001's exact journeys; do not manufacture a failing test or add a snippet
checker for prose. Review the touched examples against the implemented public
surface, then use the existing docs gate for content, generation, links,
commands, build, and accessibility proof.

## Acceptance Criteria

- The root README and docsite setup guidance explicitly state that the client
  is always optional and the normal usage omits it. They explain automatic
  environment/saved-configuration resolution without requiring manual lookup
  or construction of `WyrdClient` (REQ-002, REQ-012, AC-006).
- Primary language examples lead with Rust `X::connect`, Python `X.connect()`
  and lazy `X()`, and TypeScript `await X.connect()`, with required handle
  options where applicable and no explicit client. Python examples include
  `Cards`, `Principals`, and `Bifrost` consistently; async Bifrost distinguishes
  `AsyncBifrost()` from `await AsyncBifrost.connect()` (REQ-006, REQ-009).
- Optional explicit-client examples use the implemented forms and explain
  testing, advanced configuration, or delegation. No mixed example implies
  that a particular handle or handle-specific option requires a client.
- Snippets use the new `auth` module paths, Python owning-submodule imports,
  and correct awaiting. Eager/lazy timing and bearer/cache qualifications match
  the specification. Resolver precedence, delegated identity, and local-loader
  semantics remain unchanged; no real credential is included (REQ-016,
  INV-002–INV-004).
- Relevant docsite/README construction examples and `CodeFromFile` references
  match TASK-001's implementation. Generated docsite outputs are source-driven,
  and `mise run docs:check` passes (AC-006).

## Expected Write Set and Consumer Closure

- `README.md` and other existing relevant READMEs, particularly
  `examples/support-desk/README.md`.
- Authored pages under `docs/src/content/docs/`, starting with the setup,
  quickstart, identity, permissions, and Bifrost pages identified above.
- Affected generated docsite outputs under `docs/src/content/docs/api/` and
  `docs/public/llms*.txt`, regenerated only through existing tooling.

Executable example sources, architecture references, SDK docs/stubs, and SDK
generated declarations remain TASK-001's responsibility. Changes here consume
that completed work rather than changing its runtime contracts.

## Verification and Evidence

Record the touched pages/READMEs and a static acceptance review covering
default no-client forms, optional explicit forms, Python eager/lazy parity,
imports/awaiting, configuration accuracy, and embedded-source references.

```bash
mise run docs:generate
mise run docs:check
git diff --check
```

`docs:check` includes generation/drift, content tests, command/link checks,
build, and accessibility; do not duplicate those component lanes. Review root
and example README links and snippets explicitly because the docsite gate does
not establish their SDK semantics. No runtime or full SDK journey suite is
required for this documentation-only task.

## Material Stop Conditions

- TASK-001's public surface cannot support the required no-client forms or
  differs from the approved specification.
- Documentation would require a new API, configuration option, resolver
  precedence, authentication path, compatibility form, or changed local-loader
  behavior.
- Required proof would need a new checker, generated source hand-edit, or
  weakening an existing gate.

## Authority Links

- [Approved specification revision 9](../spec.md)
- [Implementation dependency](TASK-001-unify-sdk-handle-instantiation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd client model](../../../../architecture/wyrd-design.md#client-model)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
