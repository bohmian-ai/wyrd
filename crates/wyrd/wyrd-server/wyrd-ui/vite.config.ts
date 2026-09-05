import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Vitest resolves server modules; keep its optimized dependencies out of the browser cache.
  cacheDir: process.env.VITEST ? 'node_modules/.vite-vitest' : 'node_modules/.vite',
  plugins: [tailwindcss(), sveltekit(), svelteTesting()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./vitest-setup.ts'],
    include: ['src/**/*.{test,spec}.{js,ts}']
  }
});
