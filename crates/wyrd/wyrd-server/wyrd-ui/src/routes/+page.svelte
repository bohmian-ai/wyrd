<script lang="ts">
  import type { PageProps } from './$types';
  import TenantChooser from '$lib/components/app/TenantChooser.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import logo from '../../brand/logo.svg?inline';
  const webLeft = [0.1, 0.26, 0.42, 0.58, 0.74, 0.9];
  const webMiddle = [0.3, 0.5, 0.7];
  const webRight = [0.08, 0.22, 0.38, 0.54, 0.7, 0.86];
  let { data, form }: PageProps = $props();
  let issue = $derived(form?.problem ?? data.problem);
  let reauthentication = $derived(form?.reauthentication ?? data.reauthentication);
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
    <span class="brand"><img src={logo} alt="" width="28" height="28" /><span class="wordmark">bohmian</span></span>
    <span class="product">WYRD</span>
    {#if data.session}
      <details><summary class="app-control">{data.session.subject.name}</summary><form method="POST" action="?/logout"><input type="hidden" name="csrf" value={data.session.csrf} /><button class="app-control" type="submit">Sign out</button></form></details>
    {/if}
    <button class="app-control" type="button" onclick={toggleMode} aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`}>◐ {theme.mode === 'dark' ? 'Light' : 'Dark'}</button>
  </header>
  <h1>{!data.session ? 'Welcome to Wyrd' : reauthentication ? `Sign in again for ${reauthentication.name}` : data.session.tenants.length === 0 ? 'No tenant access' : 'Choose a tenant'}</h1>
  <p class="intro">{!data.session ? 'Sign in to continue to your workspace.' : data.session.tenants.length === 0 ? 'Contact your administrator to request access.' : 'Choose where to work. Your tenant’s data appears after you continue.'}</p>
  <div class="entry-content">
    {#if issue}
      <div class="login-error" role="alert">
        <StateBlock state={issue.status >= 500 ? 'error' : 'unauthorized'} title={issue.status >= 500 ? 'Sign-in unavailable' : issue.title} detail={issue.status >= 500 ? 'Please try again later.' : issue.remediation} />
        <details class="error-details"><summary>Technical details</summary><code>{issue.code}</code></details>
      </div>
    {/if}
    {#if !data.session}
      <form method="POST" action="?/login"><button class="app-control primary sign-in" type="submit">Sign in with SSO</button></form>
    {:else if reauthentication}
      <p>This tenant requires a fresh sign-in before you continue.</p>
      <form method="POST" action="?/reauthenticate">
        <input type="hidden" name="csrf" value={data.session.csrf} />
        <input type="hidden" name="tenantKey" value={reauthentication.key} />
        <button class="app-control primary sign-in" type="submit">Sign in with SSO</button>
      </form>
    {:else if data.session.tenants.length === 0}
      <p class="access-help">Ask your workspace administrator to grant you access to a tenant.</p>
    {:else}
      <Panel><TenantChooser session={data.session} /></Panel>
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
    stroke: var(--brand-strong);
  }
  .outgoing {
    stroke: var(--lime-text);
  }
  .incoming-node {
    fill: var(--brand-strong);
    opacity: 0.85;
  }
  .outgoing-node {
    fill: var(--lime-text);
    opacity: 0.9;
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
    border-top: 4px solid var(--brand-strong);
    border-radius: var(--r);
    box-shadow: 6px 6px 0 var(--shadow);
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
    font: 600 18px var(--font-serif);
  }
  .product {
    font: 700 10px var(--font-mono);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 4px 12px;
    background: var(--brand-btn);
    color: var(--brand-btn-ink);
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
    font: 700 26px var(--font-display);
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
  .access-help {
    text-align: center;
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
