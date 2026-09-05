<script lang="ts">
  import { toggleMode } from '$lib/theme.svelte';
  import Button from '$lib/components/Button.svelte';
</script>

<main class="mx-auto max-w-5xl p-8">
  <h1 class="text-4xl font-black uppercase" style="font-family:var(--font-display)">wyrd</h1>
  <p class="mt-2" style="color:var(--muted)">The AI layer. Phase 0 skeleton.</p>

  <div class="mt-6 flex gap-3">
    <Button href="/styleguide" variant="primary">Design system →</Button>
    <button type="button" class="mode-toggle" onclick={toggleMode}>Toggle mode</button>
  </div>
</main>

<style>
  .mode-toggle {
    font: 700 13px var(--font-mono);
    padding: 8px 13px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .mode-toggle:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
</style>
