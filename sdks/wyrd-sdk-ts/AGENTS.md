# wyrd-sdk-ts

Rules for the `wyrd-ai` TypeScript package. The root `AGENTS.md` still
applies. The package is a first-class client: it projects server contracts and
bridges through N-API into `wyrd-client`, without owning durable server
behavior.

## Placement

- Public source and package metadata: `wyrd/`. Native binding: `native/`.
  Test-only bindings stay private to `testing/` and `native-testing/`; the
  production package never exposes them.
- `wyrd/index.d.ts` is napi-generated from the Rust binding. Commit it, never
  hand-edit it; `mise run ts:napi:check` fails on drift.
- Hand-written wrappers may add ergonomics (builders, batching, retry policy,
  tracing hooks) but never fork field names, error semantics, or lifecycle.

## N-API bridge

- Keep `#[napi]` thin, with the PyO3 rules: extract and validate at the
  boundary, call Rust-native APIs, convert errors at the edge.
- Never hold a V8 handle across `.await` in an async napi task.
- The binding may project the single shared `wyrd_client::WyrdClient` as a
  thin handle for authentication and delegation (`onBehalfOf`). Every
  server-facing facade opens only through `static async connect({ client? })`,
  which awaits the Rust `connect_with`. Exchange, bearer access, caching,
  renewal, retry, headers, and transport stay private to Rust.
- Wrap `wyrd_client::Bifrost`. Never assemble a separate query client, a second
  auth or transport path, or a per-call gRPC transport.
- Batch boundary crossings: pass a batch of records, not one call per record.
- Pass raw `Buffer`/`Uint8Array` across the boundary; parse JSON once.
- Reuse Arrow/IPC buffers only when no consumer retains or mutates them;
  otherwise transfer or copy at the owned boundary.
- Long CPU work belongs in the Rust owner with bounded execution or a worker
  thread, never on the main JavaScript thread.

## Types and API shape

- `unknown` over `any`; never `Function`, `Object`, or wrapper types
  (`String`, `Number`, `Boolean`).
- String-literal unions over enums for closed sets; tagged unions for
  multi-shape results; branded types for identifiers.
- Function types over call-signature-only interfaces; union or optional
  parameters over overloads on constants.
- Types over interfaces unless declaration merging is required.
- Explicit return types on exported functions; `readonly` fields and
  `Readonly<T>`/`ReadonlyArray<T>` where not reassigned.
- Handle every union branch; finish switches with a `never` exhaustiveness
  check.
- Convert at the wire boundary, then call typed functions. Do not shape APIs
  around serialization convenience.
- Preserve Wyrd wire field names. Use typed option objects instead of long
  positional lists. Keep JSDoc `@param` and `@returns` aligned with the
  declaration.

```ts
export type CardKind =
  | 'Model' | 'Data' | 'Artifact' | 'Experiment'
  | 'Prompt' | 'Agent' | 'Mcp'
  | 'Service' | 'Policy' | 'Audit' | 'Verifier'
  | 'Source' | 'Trigger' | 'Operator'
  | 'External';

declare const tenantIdBrand: unique symbol;
export type TenantId = string & { readonly [tenantIdBrand]: true };
```

## Style

- Match the surrounding file; do not introduce a second convention.
- Kebab-case source file names. Named exports only. Re-export only the public
  surface, never deep internals.
- `const` by default, `let` when reassigned, never `var`; `===` only; `??` and
  `?.` over `||`/`&&` defaults.
- Function declarations for top-level named functions, arrows for callbacks.
- Do not mark a synchronous body `async`.

## Errors

The SDK throws its `WyrdError` class with `code`, `status`, `title`, `detail`,
optional `remediation`, and JSON-safe `details`. Its values project the
derive-backed catalog exactly; `code` is typed by the generated
`WyrdErrorCode` literal union. Narrow on `code`; never parse `message`, never
invent TypeScript-only codes.

## Async

- Run independent work concurrently only inside an explicit bound; a bare
  `Promise.all` over an unbounded collection is not admission control.
- `Promise.allSettled` only when partial success is part of the contract.
- Accept a caller `AbortSignal` for cancellable operations, compose it with a
  timeout, and release native resources when either fires.

## Tests and checks

- Journey: `mise run ts:test:integration` (client → `WyrdTestServer` →
  client). Every user- or agent-facing capability ships one.
- Unit: `mise run ts:test:unit`.
- Before finishing: `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`,
  and `codegen:check` when a contract changed.
