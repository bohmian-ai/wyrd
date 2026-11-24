<!-- A platform console refusal: its title, what to do about it and Rust's code. -->
<script lang="ts">
  import { guidance } from '$lib/Entry.svelte';
  import type { WyrdProblem } from '$lib/server/problem';
  let { issue }: { issue: WyrdProblem } = $props();
</script>

<div role="alert">
  <div class="app-problem" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
    <strong>{issue.status >= 500 ? '✕' : '⊘'} {issue.title}</strong>
    <p>{guidance(issue)}</p>
    {#if issue.code}<code>{issue.code}</code>{/if}
  </div>
</div>
