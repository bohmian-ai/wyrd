# SOLID Review Reference

Use this file to calibrate SOLID findings. SOLID is a review lens, not a quota
or an excuse to force object-oriented patterns into code that is simpler without
them. Report issues only when the changed code creates concrete maintenance
risk, caller misuse risk, untestable design, brittle extension points, or local
architecture drift.

## General Rules

- Prefer the repo's existing architecture over textbook purity.
- Treat closed enums, explicit match statements, and simple concrete functions
  as good design when the domain is intentionally closed.
- Do not require traits, interfaces, abstract classes, dependency injection, or
  builders unless they reduce real coupling or make a real boundary testable.
- Consolidate repeated SOLID failures under one root-cause finding.
- Cite changed code and the nearby local pattern that proves the finding is not
  personal preference.

## Single Responsibility Principle

A unit should have one clear reason to change. "Unit" can mean a function,
method, type, module, component, route handler, command, or test helper.

Flag when changed code:

- mixes domain logic with transport, persistence, presentation, logging,
  authorization, or test fixture setup in a way local patterns avoid
- validates, transforms, persists, emits side effects, and renders responses in
  one oversized function
- makes a helper responsible for unrelated resources or lifecycle phases
- forces tests to exercise a full stack because pure decision logic is trapped
  inside I/O code
- has names that hide multiple responsibilities, such as `process`, `handle`,
  `manage`, or `sync`, while the body performs unrelated work

Do not flag only because a function is long. Length is supporting evidence, not
the issue.

## Open/Closed Principle

Code should be open to expected extension and closed to repeated modification at
the wrong layer. In Rust and TypeScript, exhaustive matching over a closed enum
can be exactly the right design.

Flag when changed code:

- adds another copy-pasted branch to an already-growing family where local code
  has a registry, strategy, trait, table, component map, or typed dispatch
  pattern
- requires editing high-level orchestration code every time a low-level variant
  is added, even though variants are meant to be independently extensible
- encodes extension points with string switches instead of typed variants,
  registries, or capability objects used elsewhere
- uses feature flags or booleans as a substitute for clear mode types when new
  modes are foreseeable from the current diff

Do not flag closed protocol/card/API enums merely because new variants require
updating match arms. Exhaustiveness can be the contract.

## Liskov Substitution Principle

Subtypes, trait implementations, protocols, subclasses, or component
implementations must satisfy the promises of the abstraction they implement.

Flag when changed code:

- implements a trait/interface but rejects valid inputs accepted by the
  contract without documenting and encoding narrower preconditions
- returns weaker postconditions, different error semantics, different lifecycle
  behavior, or surprising side effects from one implementation
- requires callers to type-check or downcast before using a polymorphic value
- makes a test double that behaves differently enough to hide production bugs
- extends a base class/component but breaks expected event, prop, state,
  cleanup, or rendering behavior

Do not invent LSP findings where there is no substitutable abstraction.

## Interface Segregation Principle

Callers should depend on the capabilities they need, not broad "god"
interfaces.

Flag when changed code:

- adds methods to a broad trait/interface that only one caller or one
  implementation can support
- forces implementations to provide no-op, panic, `todo!`, `unimplemented!`, or
  unsupported branches for capabilities they do not have
- passes a large context/service object into functions that need one or two
  capabilities
- exposes component props or API request shapes that require unrelated fields
  for independent actions
- makes tests build large fake objects for small behavior

Prefer small capability traits, narrower parameter structs, or concrete helper
functions when local patterns support them.

## Dependency Inversion Principle

High-level policy should not depend directly on low-level transport, storage,
provider, or framework details when that coupling makes change or testing hard.

Flag when changed code:

- constructs concrete database, HTTP, filesystem, provider, or clock
  dependencies inside business logic instead of receiving an existing local
  abstraction or boundary dependency
- makes core/domain crates import UI, HTTP, CLI, database, provider, or test
  helper types
- makes pure decisions depend on global state, environment variables, current
  time, random values, or process state without an injectable boundary
- hides side effects in constructors or simple-looking helpers
- prevents focused tests because dependencies cannot be substituted at the
  existing boundary

Do not demand dependency injection where the concrete dependency is the intended
owner, the code is already at an I/O boundary, or a local pattern intentionally
uses concrete types.
