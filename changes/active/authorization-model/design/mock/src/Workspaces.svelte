<!-- #/workspaces: ask for a link listing the workspaces an address may sign in to. Every address gets the same answer. -->
<script>
  import Entry from './Entry.svelte';
  import CheckEmail from './CheckEmail.svelte';
  import { P, emailError } from './data.js';
  let { query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  let issue = $state.raw(preview === 'throttled' ? P.emailThrottled : preview === 'unavailable' ? P.discoveryOff : null);
  let email = $state(preview === 'sent' ? 'someone@example.test' : '');
  let checked = $state(false);
  let sent = $state(preview === 'sent');
  const error = $derived(checked ? emailError(email) : '');

  // Simulates POST /auth/browser/workspaces/request {email}: 202 check_email whether or not the address is known.
  function request(event) {
    event.preventDefault();
    checked = true;
    if (error) return document.getElementById('email').focus();
    issue = null;
    sent = true;
  }
  const verify = () => setTimeout(() => { location.hash = '#/workspaces/verify?preview=proven'; }, 600);
</script>

<Entry heading={sent ? 'Check your email' : 'Find your workspaces'} {issue}>
  {#if sent}
    <CheckEmail address={email.trim()} onverify={verify} again="#/workspaces" />
  {:else if preview === 'unavailable'}
    <a class="app-control primary wide" href="#/">Enter a workspace URL</a>
  {:else}
    <p class="intro">We email a link that lists the workspaces where this address is a verified recovery email. The message itself names no workspace.</p>
    <form class="fields" novalidate onsubmit={request}>
      <label for="email">Email</label>
      <input id="email" class="app-input" type="email" name="email" autocomplete="email" bind:value={email}
        aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'email-error' : undefined} />
      {#if error}<p class="field-error" id="email-error">⊘ {error}</p>{/if}
      <button class="app-control primary wide" type="submit" data-testid="discover">Email me a link</button>
    </form>
    <nav class="links"><a href="#/">Enter a workspace URL</a></nav>
  {/if}
</Entry>

<style>
  .intro {
    text-align: center;
  }
</style>
