import { AsyncLocalStorage } from "node:async_hooks";

import { type Context, type ContextManager, ROOT_CONTEXT } from "@opentelemetry/api";

/**
 * The smallest real context manager, so `context.with` makes a span active.
 *
 * `@opentelemetry/api` is a no-op until an application registers one; the SDK
 * must read whatever the application registered, and this is that stand-in.
 */
export class StorageContextManager implements ContextManager {
  readonly #storage = new AsyncLocalStorage<Context>();

  active(): Context {
    return this.#storage.getStore() ?? ROOT_CONTEXT;
  }

  with<A extends unknown[], F extends (...args: A) => ReturnType<F>>(
    active: Context,
    fn: F,
    thisArg?: ThisParameterType<F>,
    ...args: A
  ): ReturnType<F> {
    return this.#storage.run(active, () => fn.call(thisArg, ...args));
  }

  bind<T>(_context: Context, target: T): T {
    return target;
  }

  enable(): this {
    return this;
  }

  disable(): this {
    this.#storage.disable();
    return this;
  }
}
