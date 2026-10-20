import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  // The SDK loads its native addon at runtime; it is never bundled.
  ssr: { external: ['wyrd-ai'] }
});
