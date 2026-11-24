<!-- A password field with a Show/Hide control, as production's PasswordInput. -->
<script>
  let { id, value = $bindable(), ...rest } = $props();
  let shown = $state(false);
</script>

<span class="password">
  <input {id} class="app-input" type={shown ? 'text' : 'password'} bind:value {...rest} />
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
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
