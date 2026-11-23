<!-- #/signup: prove a mailbox (local signup) or an identity with an operator-enabled provider before naming a workspace. -->
<script>
  import Entry from './Entry.svelte';
  import CheckEmail from './CheckEmail.svelte';
  import { PROVIDERS, PROBLEMS, emailError } from './data.js';
  let { query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  // GET /auth/browser/methods: local_signup and signup_providers.
  const local = q('local') !== 'off';
  const providers = (q('providers') ?? 'google,github').split(',').filter((p) => p in PROVIDERS);
  const names = providers.map((p) => PROVIDERS[p]).join(' or ');
  let issue = $state.raw(q('signup') === 'failed' ? PROBLEMS.failed : preview === 'throttled' ? PROBLEMS.throttled : null);
  let email = $state(preview === 'sent' ? 'owner@example.test' : '');
  let checked = $state(false);
  let sent = $state(preview === 'sent');
  const error = $derived(checked ? emailError(email) : '');

  // Simulates POST /auth/signup {method:"email", email}: 202 check_email and the sealed flow cookie.
  function byEmail(event) {
    event.preventDefault();
    checked = true;
    if (error) return document.getElementById('email').focus();
    issue = null;
    sent = true;
  }
  // Simulates POST /auth/signup {method:"social", provider} and the provider round trip.
  const start = (provider) => (event) => {
    event.preventDefault();
    location.hash = `#/signup/workspace?identity=${provider}`;
  };
  // Simulates POST /auth/signup/verify with a pasted code.
  const verify = () => setTimeout(() => { location.hash = '#/signup/verify?preview=proven'; }, 600);
</script>

<Entry heading={sent ? 'Check your email' : 'Create a workspace'} {issue}>
  {#if preview === 'disabled'}
    <p class="intro" role="status" data-testid="disabled">Creating a workspace is turned off on this deployment. To join an existing workspace, open your invitation link or ask its administrator to invite you.</p>
    <a class="app-control wide" href="#/">Sign in to an existing workspace</a>
  {:else if sent}
    <CheckEmail address={email.trim()} onverify={verify} again="#/signup" />
  {:else}
    <p class="intro">Confirm {local ? `your email${providers.length ? `, or who you are with ${names},` : ''}` : `who you are with ${names},`} then name your workspace. You become its administrator.</p>
    {#if local}
      <form class="fields" novalidate onsubmit={byEmail}>
        <label for="email">Email</label>
        <input id="email" class="app-input" type="email" name="email" autocomplete="email" bind:value={email}
          aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'email-error' : undefined} />
        {#if error}<p class="field-error" id="email-error">⊘ {error}</p>{/if}
        <button class="app-control primary wide" type="submit" data-testid="email-start">Continue with email</button>
      </form>
      {#if providers.length}<p class="or">or</p>{/if}
    {/if}
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
