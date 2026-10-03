<script lang="ts">
  import type { PageProps } from './$types';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import logo from '../../../../../brand/logo.svg?inline';
  let { data, form }: PageProps = $props();
  let issue = $derived(form?.problem ?? data.problem);
</script>

<svelte:head><title>Sign in · {data.tenant.key} · Wyrd</title></svelte:head>
<main class="entry">
  <div class="card">
    <header>
      <img src={logo} alt="" width="28" height="28" /><span class="wordmark">bohmian</span>
      <span class="product">WYRD</span>
    </header>
    <h1>Sign in to {data.tenant.key}</h1>
    {#if issue}
      <div role="alert">
        <StateBlock state={issue.status >= 500 ? 'error' : 'unauthorized'} title={issue.status >= 500 ? 'Sign-in unavailable' : issue.title} detail={issue.status >= 500 ? 'Please try again later.' : issue.remediation} code={issue.code} />
      </div>
    {/if}
    {#if !issue || issue.status < 500}
      <form method="POST" action="?/sso"><button class="app-control primary wide" type="submit">Sign in with SSO</button></form>
      <details>
        <summary>Sign in with an operator API key</summary>
        <form class="api-key" method="POST" action="?/apiKey">
          <label for="api-key">API key</label>
          <input id="api-key" class="app-input" type="password" name="apiKey" required autocomplete="off" />
          <p>For a tenant without SSO. Use an existing Wyrd API key for this tenant; this browser keeps it only inside an encrypted, HttpOnly cookie.</p>
          <button class="app-control primary wide" type="submit">Sign in with API key</button>
        </form>
      </details>
    {/if}
  </div>
</main>

<style>
  .entry {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 64px 24px;
    background: var(--bg);
  }
  .card {
    width: 100%;
    max-width: 420px;
    display: grid;
    gap: 20px;
    padding: 32px;
    background: var(--surface);
    border: 2px solid var(--border);
    border-top: 4px solid var(--brand-strong);
    border-radius: var(--r);
    box-shadow: 6px 6px 0 var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
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
  h1 {
    font: 700 24px var(--font-display);
    text-align: center;
    overflow-wrap: anywhere;
  }
  summary {
    font: 12px var(--font-sans);
    color: var(--muted);
    cursor: pointer;
  }
  .api-key {
    display: grid;
    gap: 10px;
    padding-top: 12px;
  }
  label {
    font: 700 11px var(--font-mono);
  }
  p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
    margin: 0;
  }
  .wide {
    width: 100%;
    padding: 12px 16px;
  }
</style>
