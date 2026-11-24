<!-- Tenant sign-in card, to show the provider buttons. Layout follows the existing Entry page; only the marks are new. -->
<script>
  import Mark from './Mark.svelte';
  let { join = false } = $props();
</script>
<main class="login">
  <div class="logincard">
    <h1>{join ? 'Join gtest' : 'Sign in to gtest'}</h1>
    {#if join}<p class="muted">Priya invited you as a viewer.</p>{/if}
    {#each ['Google', 'GitHub'] as p (p)}
      <button class="app-control wide provider"><Mark method={p} size={18} />{join ? 'Join' : 'Continue'} with {p}</button>
    {/each}
    <button class="app-control wide provider"><Mark method="Company" size={18} />{join ? 'Join' : 'Sign in'} with SSO</button>
    <div class="or"><span>or</span></div>
    <label class="field"><span>Username</span><input class="app-input" /></label>
    <label class="field"><span>Password</span><input class="app-input" type="password" /></label>
    <button class="app-control primary wide">{join ? 'Create account' : 'Sign in'}</button>
  </div>
</main>
