<!--
  A password field with a keyboard-reachable Show/Hide control. Toggling
  changes only the input type, so the typed or pasted value, the password
  manager's autocomplete and the form submission are untouched. Length and
  policy are the server's to judge.
-->
<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  let { id, ...rest }: HTMLInputAttributes & { id: string } = $props();
  let shown = $state(false);
</script>

<span class="password">
  <input {id} class="app-input" type={shown ? 'text' : 'password'} {...rest} />
  <button class="app-control" type="button" aria-controls={id} aria-pressed={shown} onclick={() => (shown = !shown)}>
    {shown ? 'Hide' : 'Show'}<span class="sr-only"> password</span>
  </button>
</span>

<style>
  .password {
    display: flex;
    gap: 6px;
  }
  .password input {
    flex: 1;
    min-width: 0;
  }
</style>
