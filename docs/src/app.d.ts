// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
  namespace App {}
  /** The workspace version from the root Cargo.toml, injected by vite.config.ts. */
  const __WYRD_VERSION__: string;
  /** The published site URL, origin plus base, without a trailing slash; from svelte.config.js. */
  const __WYRD_SITE__: string;
}

export {};
