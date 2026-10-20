<script lang="ts">
  import type { PageProps } from './$types';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import logo from '../../brand/logo.svg?inline';
  const webLeft = [0.1, 0.26, 0.42, 0.58, 0.74, 0.9];
  const webMiddle = [0.3, 0.5, 0.7];
  const webRight = [0.08, 0.22, 0.38, 0.54, 0.7, 0.86];
  let { data, form }: PageProps = $props();
  let issue = $derived(form?.problem ?? data.problem);
  let reauthentication = $derived(form?.reauthentication ?? data.reauthentication);
  let search = $state('');
  let matches = $derived(
    (data.session?.tenants ?? []).filter((tenant) =>
      `${tenant.name} ${tenant.key}`.toLowerCase().includes(search.toLowerCase())
    )
  );
</script>

<svelte:head><title>{data.session ? 'Your workspace' : 'Sign in'} · Wyrd</title></svelte:head>
<main class="entry">
  <svg class="connection-web" viewBox="0 0 800 620" preserveAspectRatio="xMidYMid slice" aria-hidden="true" focusable="false">
    {#each webLeft as y, i}
      <path class="incoming" d={`M 40 ${y * 620} C 190 ${y * 620}, 234 ${webMiddle[i % 3] * 620}, 384 ${webMiddle[i % 3] * 620}`} />
      <circle class="incoming-node" cx="40" cy={y * 620} r="3.5" />
    {/each}
    {#each webRight as y, i}
      <path class="outgoing" d={`M 384 ${webMiddle[(i + 1) % 3] * 620} C 514 ${webMiddle[(i + 1) % 3] * 620}, 640 ${y * 620}, 770 ${y * 620}`} />
      <circle class="outgoing-node" cx="770" cy={y * 620} r="3.5" />
    {/each}
    {#each webMiddle as y}<rect x="379" y={y * 620 - 5} width="10" height="10" rx="2.5" />{/each}
  </svg>
  <div class="entry-foreground">
  <header>
    <span class="brand"><img src={logo} alt="" width="28" height="28" /><span class="wordmark">Wyrd</span></span>
    <span class="product">WYRD</span>
    {#if data.session}
      <details><summary class="app-control">{data.session.subject.name}</summary><form method="POST" action="?/logout"><button class="app-control" type="submit">Sign out</button></form></details>
    {/if}
    <button class="app-control" type="button" onclick={toggleMode} aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`}>◐ {theme.mode === 'dark' ? 'Light' : 'Dark'}</button>
  </header>
  <h1>{!data.session ? 'Welcome to Wyrd' : reauthentication ? `Sign in again for ${reauthentication.name}` : data.session.tenants.length === 0 ? 'No tenant access' : 'Choose a tenant'}</h1>
  <p class="intro">{!data.session ? 'Sign in to continue to your workspace.' : data.session.tenants.length === 0 ? 'Contact your administrator to request access.' : 'Choose where to work. Your tenant’s data appears after you continue.'}</p>
  <div class="entry-content">
    {#if issue}
      <div class="login-error" role="alert">
        <div class="app-problem" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
          <strong>{issue.status >= 500 ? '✕ Sign-in unavailable' : `⊘ ${issue.title}`}</strong>
          <p>{issue.status >= 500 ? 'Please try again later.' : issue.remediation}</p>
        </div>
        <details class="error-details"><summary>Technical details</summary><code>{issue.code}</code></details>
      </div>
    {/if}
    {#if !data.session && data.tenantEntry}
      <form class="tenant-entry" method="POST" action="?/tenant">
        <label for="tenant-key">Tenant</label>
        <input id="tenant-key" class="app-input" name="tenantKey" required autocomplete="organization" placeholder="acme" pattern={'[a-z0-9][a-z0-9_\\-]{0,62}'} />
        <button class="app-control primary sign-in" type="submit">Continue to sign-in</button>
      </form>
    {:else if !data.session}
      <form method="POST" action="?/login"><button class="app-control primary sign-in" type="submit">Sign in with SSO</button></form>
    {:else if reauthentication}
      <p>This tenant requires a fresh sign-in before you continue.</p>
      <form method="POST" action="?/reauthenticate">
        <input type="hidden" name="tenantKey" value={reauthentication.key} />
        <button class="app-control primary sign-in" type="submit">Sign in with SSO</button>
      </form>
    {:else if data.session.tenants.length === 0}
      <p class="access-help">Ask your workspace administrator to grant you access to a tenant.</p>
    {:else}
      <div class="chooser">
        <label class="visually-hidden" for="tenant-search">Search tenants</label>
        <input id="tenant-search" class="app-input" placeholder="⌕ filter tenants" type="search" bind:value={search} />
        <form method="POST" action="/?/switch">
          {#each matches as tenant (tenant.key)}
            <button class="app-control" type="submit" name="tenantKey" value={tenant.key}>{tenant.name}</button>
          {:else}
            <p role="status">No matching tenants.</p>
          {/each}
        </form>
      </div>
    {/if}
  </div>
  </div>
</main>

<style>
  .entry {
    min-height: 100dvh;
    position: relative;
    isolation: isolate;
    overflow: hidden;
    display: grid;
    /* A bounded track, so the card's width: 100% resolves against the viewport, not its content. */
    grid-template-columns: minmax(0, 1fr);
    place-items: center;
    padding: 64px 24px 96px;
    background: var(--bg);
  }
  .connection-web {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: -1;
    pointer-events: none;
  }
  .connection-web path {
    fill: none;
    stroke-width: 1;
    opacity: 0.42;
  }
  .incoming {
    stroke: var(--declare);
  }
  .outgoing {
    stroke: var(--observe);
  }
  .incoming-node {
    fill: var(--declare);
    opacity: 0.85;
  }
  .outgoing-node {
    fill: var(--observe);
    opacity: 0.9;
  }
  /* Lime at thread opacity vanishes on the light canvas; deepen it toward ink
     there only. The token itself stays the same lime in both modes. */
  :global([data-mode='light']) .outgoing {
    stroke: color-mix(in srgb, var(--observe) 70%, var(--text));
    opacity: 0.55;
  }
  :global([data-mode='light']) .outgoing-node {
    fill: color-mix(in srgb, var(--observe) 70%, var(--text));
  }
  .connection-web rect {
    fill: var(--text);
  }
  .entry-foreground {
    width: 100%;
    max-width: 456px;
    padding: 36px 32px;
    background: var(--surface);
    border: 2px solid var(--border);
    border-top: 4px solid var(--declare);
    border-radius: var(--r);
  }
  header,
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  header {
    justify-content: center;
    flex-wrap: wrap;
    gap: 14px;
    margin-bottom: 32px;
  }
  img {
    border-radius: var(--r);
  }
  .wordmark {
    font: 600 18px var(--font-sans);
  }
  .product {
    font: var(--weight-strong) 10px var(--font-mono);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 4px 12px;
    background: var(--declare);
    color: var(--declare-ink);
  }
  header .app-control {
    font-size: 10px;
    padding: 4px 8px;
    box-shadow: none;
    background: var(--surface-2);
  }
  details {
    position: relative;
  }
  details form {
    position: absolute;
    top: 100%;
    right: 0;
    padding-top: 8px;
  }
  h1 {
    font: var(--weight-strong) 26px var(--font-sans);
    text-align: center;
  }
  .intro {
    text-align: center;
    margin: 12px auto 28px;
    max-width: 360px;
  }
  .entry-content {
    width: 100%;
    max-width: 360px;
    margin: auto;
    display: grid;
    gap: 20px;
  }
  .sign-in {
    width: 100%;
    padding: 12px 16px;
  }
  .login-error {
    display: grid;
    gap: 10px;
  }
  .error-details {
    font: 11px var(--font-mono);
    color: var(--muted);
  }
  .error-details summary {
    cursor: pointer;
  }
  .error-details code {
    display: block;
    margin-top: 8px;
    overflow-wrap: anywhere;
  }
  .tenant-entry {
    display: grid;
    gap: 10px;
  }
  .tenant-entry label {
    font: 700 11px var(--font-mono);
  }
  .access-help {
    text-align: center;
  }
  .chooser,
  .chooser form {
    display: grid;
    gap: 8px;
  }
  .chooser {
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
  }
  .chooser input {
    background: var(--surface-2);
    border-color: transparent;
    font-weight: 400;
  }
  .chooser button {
    text-align: left;
    border: 0;
    border-bottom: 1px solid var(--border-soft);
    border-radius: 0;
    font-weight: 400;
    padding: 12px;
  }
  .chooser button:hover,
  .chooser button:focus-visible {
    background: var(--declare-soft);
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
    margin: 0;
  }
  @media (max-width: 767px) {
    .entry {
      padding: 48px 20px 96px;
    }
    .entry-foreground {
      padding: 28px 20px;
    }
  }
</style>
