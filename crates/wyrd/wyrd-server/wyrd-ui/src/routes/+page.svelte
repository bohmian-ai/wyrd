<script lang="ts">
  import { toggleMode } from '$lib/theme.svelte';
  import Button from '$lib/components/Button.svelte';
</script>

<main class="mx-auto max-w-5xl p-8">
  <h1 class="text-4xl font-black uppercase" style="font-family:var(--font-display)">wyrd</h1>
  <p class="mt-2" style="color:var(--muted)">The AI layer. Phase 0 skeleton.</p>

  <div class="mt-6 flex gap-3">
    <Button href="/styleguide" variant="primary">Design system →</Button>
    <Button variant="ghost" onclick={toggleMode}>Toggle mode</Button>
  </div>
</main>
