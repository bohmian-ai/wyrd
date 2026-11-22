<!-- #/signup: prove an identity with an operator-enabled provider before naming a workspace. -->
<script>
  import Entry from './Entry.svelte';
  import { PROVIDERS, PROBLEMS } from './data.js';
  let { query } = $props();
  const preview = $derived(query.get('preview'));
  const providers = $derived((query.get('providers') ?? 'google,github').split(',').filter((p) => p in PROVIDERS));
  const issue = $derived(query.get('signup') === 'failed' ? PROBLEMS.failed : preview === 'throttled' ? PROBLEMS.throttled : null);
  const names = $derived(providers.map((p) => PROVIDERS[p]).join(' or '));
  // Simulates POST /auth/signup and the provider round trip.
  const start = (provider) => (event) => {
    event.preventDefault();
    location.hash = `#/signup/workspace?identity=${provider}`;
  };
</script>

<Entry heading="Create a workspace" {issue}>
  {#if preview === 'disabled'}
    <p class="intro" role="status" data-testid="disabled">Creating a workspace is turned off on this deployment. To join an existing workspace, open your invitation link or ask its administrator to invite you.</p>
    <a class="app-control wide" href="#/">Sign in to an existing workspace</a>
  {:else}
    <p class="intro">Confirm who you are with {names}, then name your workspace. You become its administrator.</p>
    {#each providers as provider (provider)}
      <form onsubmit={start(provider)}>
        <button class="app-control wide" type="submit" data-provider={provider}>Continue with {PROVIDERS[provider]}</button>
      </form>
    {/each}
    <nav class="links" aria-label="Existing workspace"><a href="#/">Already have a workspace? Sign in</a></nav>
  {/if}
</Entry>

<style>
  .intro {
    text-align: center;
  }
</style>
