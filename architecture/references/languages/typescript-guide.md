# TypeScript Guide

The `@wyrd/sdk` TypeScript client is a first-class Wyrd client. It projects
server contracts, mirrors the `WyrdError` catalog, and — where a napi native
addon is used — bridges into Rust-owned durable behavior via the same
patterns as PyO3.

References this guide leans on:

- Official Do's and Don'ts: <https://www.typescriptlang.org/docs/handbook/declaration-files/do-s-and-don-ts.html>
- TypeScript Style Guide (mkosir): <https://mkosir.github.io/typescript-style-guide/>

## Placement And Boundaries

- Public TypeScript surface lives in `@wyrd/sdk` (root `package.json`;
  see `mise run ts:build`, `ts:test:unit`, `ts:test:integration`,
  `ts:typecheck`, `ts:napi:check`).
- Napi-generated `index.d.ts` **must** be committed and verified by
  `mise run ts:napi:check` — never hand-edit it.
- Contract types, error unions, and OpenAPI-derived request/response
  shapes are **generated** from Wyrd sources. Do not hand-author parallel
  types.
- The TypeScript SDK may add ergonomic helpers (fluent builders, batching,
  retry policies, tracing hooks) but must not become the source of truth
  for durable server behavior.

## Types And Declarations

Follow the official Do's and Don'ts:

- **Prefer `unknown` over `any`.** `any` opts out of the type system;
  `unknown` forces callers to narrow before use.
- **Use union types over enums** for closed sets of string literals — they
  are erased at runtime and interop cleanly with generated schemas.
- **Do not use wrapper object types** (`String`, `Number`, `Boolean`) —
  use primitives (`string`, `number`, `boolean`).
- **Do not use `Function` or `Object`.** Use explicit function signatures
  and object shapes.
- **Prefer function types over call-signature-only interfaces** for
  callbacks: `type Handler = (evt: Event) => void`.
- **Avoid `overload on constants`.** Use union or optional parameters
  instead.
- **Return values should have a stable type.** If a function can return
  multiple shapes, model with a tagged union, not `T | undefined | null`.

## API Shape

Prefer APIs that make invalid states unrepresentable:

```ts
// Tagged union for closed sets
export type CardKind =
  | 'Model' | 'Data' | 'Artifact' | 'Experiment'
  | 'Prompt' | 'Agent' | 'Workflow' | 'Mcp'
  | 'Service' | 'Policy' | 'Audit' | 'Drift'
  | 'Eval' | 'Source' | 'Trigger' | 'Operator'
  | 'External';

// Discriminated union for outcomes
export type Result<T, E = WyrdError> =
  | { ok: true;  value: T }
  | { ok: false; error: E };

// Branded newtype for identifiers
export type TenantId = string & { readonly __brand: 'TenantId' };
export const TenantId = (id: string): TenantId => id as TenantId;
```

Do not shape TypeScript APIs around what is easiest to serialize. Convert
at the wire boundary, then call typed core functions.

## Performance Best Practices

Hot-path TypeScript in the SDK (napi bridge, request pipeline, batching)
runs alongside V8 optimizations that reward predictable shapes:

- **Keep object shapes monomorphic.** V8's hidden-class optimization
  degrades when the same variable holds objects with different property
  orders. Initialize all fields in a constructor or object literal at
  once; do not `delete` fields; assign properties in the same order every
  time.
- **Avoid `arguments`** in hot paths. Use rest parameters (`...args`)
  which the compiler can specialize.
- **Prefer `for` / `for-of` over `Array#forEach`** in hot loops. `forEach`
  allocates a closure per call and disables some optimizations.
- **Preallocate arrays** with `new Array(n)` when the size is known; avoid
  repeated `Array#push` in tight loops.
- **Batch napi boundary crossings.** Each JS ↔ Rust hop has fixed
  overhead — pass a batch of records, not one call per record.
- **Reuse buffers** for streaming Arrow / IPC bytes. Do not allocate a
  fresh `Uint8Array` per chunk.
- **Avoid `try`/`catch` inside hot inner loops.** Move error handling to
  the outer scope where possible; the V8 optimizer historically
  deoptimizes functions containing `try` blocks (this has improved but
  the guidance still holds for inner loops).
- **Prefer `Map` over object literals** for dictionaries with unknown or
  large key sets, or with non-string keys.
- **Prefer `Set#has` over `Array#includes`** for membership tests.
- **Avoid unnecessary `async`.** Every `async` function returns a fresh
  Promise; if the body is synchronous, do not mark it `async`.
- **Do not stringify large payloads twice.** Pass raw `Buffer` /
  `Uint8Array` through the napi boundary; parse JSON at one boundary
  only.
- **Guard against event-loop blocking.** Long CPU work belongs in the
  napi Rust side (which can `spawn_blocking`) or a worker thread — do
  not run it on the main JS thread.

## Style Guide Highlights (mkosir)

- **Consistency and readability first.** Match existing file style; do
  not introduce a second convention.
- **File names**: kebab-case for source (`card-registry.ts`), PascalCase
  for TSX component files.
- **Named exports > default exports.** Default exports weaken tree-shaking
  and make refactor rename hard.
- **`const` by default; `let` when reassigned; never `var`.**
- **Arrow functions for callbacks; function declarations for top-level
  named functions** (better stack traces).
- **Types over interfaces** unless declaration-merging is required.
- **Explicit return types on exported functions.** Inferred returns are
  fine for internal helpers; explicit at the module boundary.
- **`readonly` on all fields that are not reassigned.** Prefer immutable
  types (`Readonly<T>`, `ReadonlyArray<T>`).
- **Descriptive names**: `activeUsers`, not `au`; `parseCardRef`, not
  `parse`.
- **No barrel `index.ts` re-exports of deep internals.** Only re-export
  the public surface.
- **`==` never; `===` always.** No implicit type coercion.
- **`nullish coalescing (??) and optional chaining (?.)`** over
  `||` / `&&` chains for defaults.
- **Handle every union branch explicitly.** Rely on `never`
  exhaustiveness checks.

```ts
function describe(kind: CardKind): string {
  switch (kind) {
    case 'Model':      return 'model';
    case 'Data':       return 'data';
    // ... all other kinds ...
    case 'External':   return 'external';
    default: {
      const _exhaustive: never = kind;
      throw new Error(`unhandled kind: ${_exhaustive as string}`);
    }
  }
}
```

## Errors

Mirror the Wyrd error catalog as a discriminated union generated from the
`WyrdError` derive. Do not invent TypeScript-only error names or messages.

```ts
export type WyrdError =
  | { code: 'WYRD_REGISTRY_404_CARD_NOT_FOUND'; status: 404; title: string; remediation: string; message: string; details: unknown }
  | { code: 'WYRD_STORAGE_400_TENANT_PATH_MISMATCH'; status: 400; ... }
  | /* ... rest of catalog ... */;
```

The SDK throws a `WyrdErrorLike` object carrying `code`, `status`,
`message`, `remediation`, and `details`. Callers narrow on `code`.

## Async And Concurrency

- Use `async/await`; avoid raw `.then()` chains except at boundaries with
  legacy code.
- **`Promise.all` for parallel independent work** (not `for await`).
- **`Promise.allSettled` when partial success is meaningful.**
- **Bound concurrency** with a semaphore or a batching helper — do not
  fire N parallel requests when N is unbounded.
- Add explicit timeouts (`AbortSignal.timeout(ms)`) on every external
  call.

## Napi Bridge

For `@wyrd/sdk` napi-backed features:

- Napi Rust side lives under the appropriate `crates/bindings/*` crate
  (not yet in tree; plan pending). Until then, the TypeScript SDK talks
  to `wyrd-server` over HTTP + gRPC (`wyrd-tonic`).
- When napi is present, the `#[napi]` layer mirrors PyO3 rules: extract
  and validate at the boundary, call Rust-native APIs, convert errors at
  the edge.
- Do not hold V8 handles across `.await` inside async napi tasks (same
  spirit as the PyO3 `Bound<'py, T>` rule).

## Tests

Follow the three-tier taxonomy (`references/testing-workflows.md`):

- **User-journey**: `mise run ts:test:integration` — client → server →
  client against an in-process `WyrdTestServer`.
- **Unit**: `mise run ts:test:unit` — auth-parity, native error envelope
  handling, pure logic.

Every user/agent-facing SDK capability ships a user-journey test.
