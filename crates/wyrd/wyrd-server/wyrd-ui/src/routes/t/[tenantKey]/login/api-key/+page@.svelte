<script lang="ts">
  import type { PageProps } from './$types';
  import logo from '../../../../../../brand/logo.svg?inline';
  let { data, form }: PageProps = $props();
  let issue = $derived(form?.problem ?? data.problem);
</script>

<svelte:head><title>API key sign-in · {data.tenant.key} · Wyrd</title></svelte:head>
<main class="entry">
  <div class="card">
    <header>
      <img src={logo} alt="" width="28" height="28" /><span class="wordmark">Wyrd</span>
      <span class="product">WYRD</span>
    </header>
    <h1>Sign in to {data.tenant.key}</h1>
    {#if issue}
      <div role="alert">
        <div class="app-problem" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
          <strong>{issue.status >= 500 ? '✕ Sign-in unavailable' : `⊘ ${issue.title}`}</strong>
          <p>{issue.status >= 500 ? 'Please try again later.' : issue.remediation}</p>
          <code>{issue.code}</code>
        </div>
      </div>
    {/if}
    {#if !issue || issue.status < 500}
      <form class="api-key" method="POST">
        <label for="api-key">API key</label>
        <input id="api-key" class="app-input" type="password" name="apiKey" required autocomplete="off" />
        <p>Recovery sign-in with an operator API key, for a tenant without SSO or when SSO is unavailable. This browser keeps the key only inside an encrypted, HttpOnly cookie.</p>
        <button class="app-control primary wide" type="submit">Sign in with API key</button>
      </form>
    {/if}
    <a class="back" href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to SSO sign-in</a>
  </div>
</main>

<style>
  .entry {
    min-height: 100dvh;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
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
    border-top: 4px solid var(--declare);
    border-radius: var(--r);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
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
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
    text-align: center;
    overflow-wrap: anywhere;
  }
  .api-key {
    display: grid;
    gap: 10px;
  }
  .back {
    justify-self: center;
    font: 12px var(--font-sans);
    color: var(--muted);
  }
  label {
    font: var(--weight-strong) 11px var(--font-mono);
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
