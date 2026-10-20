import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';
import { readFileSync } from 'node:fs';

// The released Wyrd version, read from `[workspace.package]` in the root
// Cargo.toml that every crate and SDK version follows, so the header never
// needs a manual edit. A missing version fails the build.
function wyrdVersion(): string {
  const cargo = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
  const version = cargo.match(/^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!version) throw new Error('docs: no version under [workspace.package] in Cargo.toml');
  return version;
}

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  define: { __WYRD_VERSION__: JSON.stringify(wyrdVersion()) },
  test: {
    include: ['src/**/*.test.ts']
  }
});
