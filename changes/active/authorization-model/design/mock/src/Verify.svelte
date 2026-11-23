<!-- The fixed pages an email link opens: /signup/verify, /workspaces/verify and /t/<slug>/contact/verify.
     Page code POSTs the fragment's proof, then clears it from the address bar; without a fragment the code is pasted.
     The mock's hash is its router, so the fragment rides the hash query as `proof=`. -->
<script>
  import Entry from './Entry.svelte';
  import CheckEmail from './CheckEmail.svelte';
  import { P, DISCOVERED, PAGE_SIZE } from './data.js';
  let { purpose, tenant = '', query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  const REFUSALS = { invalid: P.proofInvalid, expired: P.proofExpired, 'signed-out': P.signedOut, 'read-expired': P.readExpired };
  const HEADINGS = { signup: 'Confirm your email', workspaces: 'Find your workspaces', contact: 'Verify your recovery email' };
  const DONE = { signup: 'Email confirmed', workspaces: 'Your workspaces', contact: 'Recovery email verified' };

  let phase = $state(preview === 'checking' || q('proof') !== null ? 'checking' : preview in REFUSALS && preview !== 'read-expired' ? 'refused' : preview === 'proven' || preview === 'empty' || preview === 'read-expired' ? 'done' : 'paste');
  let issue = $state.raw(REFUSALS[preview] ?? null);
  let shown = $state.raw(preview === 'empty' ? [] : DISCOVERED.slice(0, PAGE_SIZE));
  let cursor = $state(preview === 'proven' ? 'c1' : null);
  let loading = $state(false);

  // POST <verify> {proof}; success sets the next cookie (completion, discovery read) or answers 204 for a contact.
  function verify() {
    phase = 'checking';
    setTimeout(() => { phase = 'done'; }, 700);
  }
  if (q('proof') !== null) {
    history.replaceState(null, '', location.href.replace(/([?&])proof=[^&]*&?/, '$1').replace(/[?&]$/, ''));
    verify();
  }
  // GET /auth/browser/workspaces?cursor=<next_cursor>, with the five-minute read cookie.
  function more() {
    loading = true;
    setTimeout(() => {
      shown = [...shown, ...DISCOVERED.slice(shown.length, shown.length + PAGE_SIZE)];
      cursor = shown.length < DISCOVERED.length ? 'c2' : null;
      loading = false;
    }, 500);
  }
  const restart = () => ({ signup: ['#/signup', 'Start again'], workspaces: ['#/workspaces', 'Request a new link'], contact: [`#/t/${tenant}/settings/account`, 'Back to account'] })[purpose];
</script>

<Entry heading={phase === 'done' ? DONE[purpose] : HEADINGS[purpose]} {issue}>
  {#if phase === 'checking'}
    <p class="center" role="status" aria-busy="true" data-testid="checking">Checking the link…</p>
  {:else if phase === 'paste'}
    <CheckEmail onverify={verify} again={restart()[0]} againLabel={restart()[1]} />
  {:else if phase === 'refused'}
    {#if preview === 'signed-out'}
      <a class="app-control primary wide" href="#/t/{tenant}/login">Sign in to {tenant}</a>
    {:else}
      <a class="app-control primary wide" href={restart()[0]}>{restart()[1]}</a>
    {/if}
  {:else if purpose === 'signup'}
    <p class="done center" role="status" data-testid="proven"><span class="ok">✓</span> This browser can now create your workspace for the next five minutes.</p>
    <a class="app-control primary wide" href="#/signup/workspace?identity=email" data-testid="continue">Name your workspace</a>
  {:else if purpose === 'contact'}
    <p class="done center" role="status" data-testid="proven"><span class="ok">✓</span> Password reset codes for your {tenant} account now go to this address. A previous recovery email was told of the change.</p>
    <a class="app-control wide" href="#/t/{tenant}/settings/account">Back to account</a>
  {:else}
    {#if shown.length}
      <p class="center">Sign in to each workspace on its own page. This list stays readable for five minutes.</p>
      <ul data-testid="workspaces" aria-label="Workspaces for this address">
        {#each shown as w (w.tenant)}
          <li><a href="#{w.login_url}"><span class="name">{w.display_name}</span><code>{w.tenant}</code></a></li>
        {/each}
      </ul>
      {#if cursor && !issue}
        <button class="app-control wide" type="button" onclick={more} disabled={loading} aria-busy={loading ? 'true' : undefined} data-testid="more">{loading ? 'Loading…' : 'More workspaces'}</button>
      {/if}
    {:else}
      <p class="center" role="status" data-testid="empty">No workspaces to show for this address. Only workspaces where it is a verified recovery email appear here.</p>
    {/if}
    <nav class="links"><a href="#/">Enter a workspace URL</a>{#if issue}<a href="#/workspaces">Request a new link</a>{/if}</nav>
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border-soft);
  }
  li a {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    padding: 8px 4px;
    border-bottom: 1px solid var(--border-soft);
    text-decoration: none;
    font: 13px var(--font-sans);
  }
  li a:hover .name {
    text-decoration: underline;
  }
  .name {
    color: var(--declare);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  li code {
    color: var(--muted);
  }
</style>
