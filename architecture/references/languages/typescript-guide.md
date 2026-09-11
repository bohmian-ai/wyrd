# TypeScript Guide

The `@wyrd/sdk` TypeScript client is a first-class Wyrd client. It projects
server contracts and bridges through the N-API binding into shared Rust client
behavior without owning durable server semantics.

References this guide leans on:

- Official Do's and Don'ts: <https://www.typescriptlang.org/docs/handbook/declaration-files/do-s-and-don-ts.html>
- TypeScript Style Guide (mkosir): <https://mkosir.github.io/typescript-style-guide/>

## Placement And Boundaries

- Public TypeScript source and package metadata live under
  `sdks/wyrd-sdk-ts`
  as `@wyrd/sdk` (see `mise run ts:build`, `ts:test:unit`,
  `ts:test:integration`, `ts:typecheck`, and `ts:napi:check`).
- Native bindings are private modules of `sdks/wyrd-sdk-ts` over
  `wyrd-client`. Test-only bindings remain private to the SDK
  testing package; production package dependencies must not expose them.
- Napi-generated `index.d.ts` **must** be committed and verified by
  `mise run ts:napi:check` — never hand-edit it.
- N-API declarations are generated from the Rust binding surface. Hand-authored
  ergonomic TypeScript wrappers may project those declarations and wire
  contracts, but they must not fork field names, error semantics, or lifecycle
  behavior.
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
declare const tenantIdBrand: unique symbol;
export type TenantId = string & { readonly [tenantIdBrand]: true };
export declare function tenantId(value: string): TenantId;
```

Do not shape TypeScript APIs around what is easiest to serialize. Convert
at the wire boundary, then call typed core functions.

## Performance Best Practices

Keep ordinary TypeScript clear and optimize measured hot paths. At the SDK and
N-API boundaries:

- **Batch napi boundary crossings.** Each JS ↔ Rust hop has fixed
  overhead — pass a batch of records, not one call per record.
- **Make buffer ownership explicit.** Reuse Arrow/IPC buffers only when no
  consumer retains or mutates them; otherwise transfer or copy at the owned
  boundary.
- **Avoid unnecessary `async`.** Every `async` function returns a fresh
  Promise; if the body is synchronous, do not mark it `async`.
- **Do not stringify large payloads twice.** Pass raw `Buffer` /
  `Uint8Array` through the napi boundary; parse JSON at one boundary
  only.
- **Guard against event-loop blocking.** Long CPU work belongs in the
  Rust owner with explicit bounded execution or a worker thread. Do not run it
  on the main JavaScript thread or create unbounded native work.

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

Project the Wyrd error catalog through the SDK's `WyrdError` class. Do not
invent TypeScript-only codes or messages.

```ts
export class WyrdError extends Error {
  readonly code: WyrdErrorCode;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly remediation: string | undefined;
  readonly details: unknown;
}
```

The SDK throws `WyrdError` instances carrying `code`, `status`, `title`,
`detail`, optional `remediation`, and `details`. Narrow error handling on
`code`; never parse `message` to recover structured metadata. Generated code
unions may refine the `code` property without replacing the runtime class.
`WyrdErrorCode` is generated as the literal union of stable catalog codes so
callers can narrow exhaustively.

## Async And Concurrency

- Use `async/await`; use raw promise chaining only when an API boundary makes it
  clearer.
- Run independent work concurrently only inside an explicit bound. A bare
  `Promise.all` over an unbounded collection is not admission control.
- Use `Promise.allSettled` only when partial success is part of the contract;
  otherwise fail and cancel every unfinished operation.
- Accept a caller `AbortSignal` for cancellable operations, compose it with an
  explicit timeout, and release native resources when either fires.

## Napi Bridge

For `@wyrd/sdk` N-API-backed features:

- The native boundary in `sdks/wyrd-sdk-ts` remains a thin binding over the
  shared Rust client behavior and server wire contracts.
- The `#[napi]` layer mirrors PyO3 rules: extract
  and validate at the boundary, call Rust-native APIs, convert errors at
  the edge.
- Do not hold V8 handles across `.await` inside async napi tasks (same
  spirit as the PyO3 `Bound<'py, T>` rule).
- The binding wraps `wyrd_client::Bifrost`. It must not assemble a
  separate `QueryClient`, raw `WyrdClient`, or per-call gRPC transport.

## Tests

Follow the [three-tier testing taxonomy](testing-workflows.md):

- **User-journey**: `mise run ts:test:integration` — client → server →
  client against an in-process `WyrdTestServer`.
- **Unit**: `mise run ts:test:unit` — auth-parity, native error envelope
  handling, pure logic.

Every user/agent-facing SDK capability ships a user-journey test.
