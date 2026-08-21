# Clean Code Maintainability Reference

Use this file to review coding practices that affect long-term maintenance.
These are heuristics. Report only issues that create a concrete maintenance
failure mode or violate a clear local convention.

## Responsibility And Cohesion

Look for code where one function, class, module, component, or test helper owns
too many concepts.

Strong evidence:

- unrelated branches change for unrelated reasons
- one function performs input parsing, authorization, domain decisions,
  persistence, event emission, and response rendering
- local comparable code separates those responsibilities
- the code is hard to test without unrelated setup
- a change to one behavior risks breaking another behavior in the same unit

Better fixes usually extract named pure decision logic, move side effects to the
existing boundary, or split orchestration from transformation.

## Complexity

Flag complexity when it hides behavior or raises bug risk.

Strong evidence:

- deeply nested conditionals where guard clauses or named predicates would make
  failure paths obvious
- repeated boolean flags or mode combinations that allow invalid states
- long parameter lists with same-typed values or unclear ordering
- duplicated validation, serialization, route construction, or error mapping
- implicit state machines encoded through strings, nullable fields, or comments

Do not flag complex code that is inherently complex and already structured in
the clearest local style. Suggest simplification only when the replacement is
obvious enough to be actionable.

## Simpler Way Check

Always ask: is there a simpler way to accomplish exactly what this change
needs?

Flag changed code when an experienced maintainer could point to a concrete
smaller shape that is easier to read, easier to test, and consistent with local
patterns and industry-standard practice. Strong evidence includes:

- a multi-line implementation that can be replaced by one clear standard
  library call, language construct, derive, framework primitive, local helper,
  typed constructor, iterator combinator, or direct expression
- a new abstraction, wrapper, builder, trait/interface, config layer, state
  machine, or compatibility layer that does not remove real complexity
- hand-rolled parsing, filtering, mapping, validation, serialization, route
  construction, or error handling where the repo already has a smaller helper
  or established pattern
- broad rewrites that overwrite nearby structure when a targeted local edit
  would solve the same problem

Do not report "shorter" as its own virtue. Report only when the simpler
alternative is behavior-preserving, concrete enough to implement, and reduces a
real maintenance burden without weakening best practices, industry standards,
safety, correctness, accessibility, security, observability, or error quality.

## Naming

Names should expose intent at the caller and maintainer level.

Flag names that:

- hide side effects or lifecycle changes
- use vague verbs such as `handle`, `process`, `manage`, `do`, or `run` when
  narrower local verbs exist
- require reading the body to distinguish validation, creation, persistence,
  registration, rendering, or dispatch
- use implementation details where domain terms are expected
- diverge from nearby API, CLI, schema, docs, or test vocabulary

## Documentation, Docstrings, And Comments

Documentation is required when the changed surface makes a commitment that a
caller, future maintainer, or smaller agent cannot infer safely from types and
names.

Flag missing docs, docstrings, doc comments, or explanatory comments for:

- public APIs, exported types, CLI commands, config, schemas, MCP tools, SDK
  methods, generated contract surfaces, or framework extension points
- non-obvious invariants, ordering constraints, side effects, security
  assumptions, idempotency, retries, timeouts, concurrency assumptions, or
  lifecycle states
- tricky algorithms, state transitions, parsing rules, data migrations, or
  compatibility constraints
- test fixtures whose intent is unclear enough that future maintainers may
  delete or weaken them

Do not require comments that restate code. Prefer renaming, type improvements,
or extraction over comments when the code can be made self-explanatory.

## API And Type Shape

Flag shapes that make misuse likely:

- boolean parameters that select behavior at public boundaries
- raw strings where the repo uses typed IDs, enums, newtypes, literal unions,
  or validated models
- structs/classes with fields that are valid only in undocumented
  combinations
- broad `dict`/map/object parameters for known schemas
- public methods that expose internal storage, transport, provider, or UI
  representation details
- builders or config objects that add ceremony for one or two obvious inputs

## Review Evidence

Every finding should answer:

- What future change becomes harder?
- What caller misuse, maintenance mistake, or test fragility becomes more
  likely?
- What local pattern or changed code proves this is real?
- What smaller, concrete shape would reduce the risk?
- Is there a simpler local API, language feature, or direct expression that
  preserves behavior, follows best practices, and removes unnecessary code?
