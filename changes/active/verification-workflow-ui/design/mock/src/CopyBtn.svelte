<script>
  import { ui } from './store.svelte.js';

  // Copies exact text; without clipboard access the shell shows it for copying by hand.
  let { label, text, testid } = $props();
  let done = $state(false);
  async function copy() {
    try { await navigator.clipboard.writeText(text); done = true; setTimeout(() => (done = false), 1200); }
    catch { ui.fallback = text; }
  }
</script>

<button type="button" class="btn btn-sm" class:btn-copied={done} data-copy={text} data-testid={testid} onclick={copy}>{done ? 'Copied' : label}</button>
