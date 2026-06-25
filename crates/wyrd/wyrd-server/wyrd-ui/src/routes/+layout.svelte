<script lang="ts">
  import '../app.css';
  import ModeProvider from '$lib/components/ModeProvider.svelte';

  let { children } = $props();
</script>

<ModeProvider>
  {@render children?.()}
</ModeProvider>
