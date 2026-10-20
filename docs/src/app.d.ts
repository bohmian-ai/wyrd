// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
  namespace App {}
  /** The workspace version from the root Cargo.toml, injected by vite.config.ts. */
  const __WYRD_VERSION__: string;
}

export {};
