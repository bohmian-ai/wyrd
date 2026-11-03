<!--
  The signed-out entry card shared by every tenant sign-in, signup and reset
  page: the mark, the page heading, the safe problem when there is one, and
  the page's forms. A server-side refusal (5xx) hides the forms.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { WyrdProblem } from '$lib/server/problem';
  import logo from '../../brand/logo.svg?inline';
  let { heading, issue, children }: { heading: string; issue: WyrdProblem | null; children: Snippet } = $props();
</script>

<main class="entry">
  <div class="card">
    <header>
      <img src={logo} alt="" width="28" height="28" /><span class="wordmark">Wyrd</span>
      <span class="product">WYRD</span>
    </header>
    <h1>{heading}</h1>
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
      {@render children()}
    {/if}
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
  .card :global(form.fields) {
    display: grid;
    gap: 10px;
  }
  .card :global(label) {
    font: var(--weight-strong) 11px var(--font-mono);
  }
  .card :global(p) {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
    margin: 0;
  }
  .card :global(.wide) {
    width: 100%;
    padding: 12px 16px;
  }
  .card :global(.links) {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px 16px;
    font: 12px var(--font-sans);
  }
  .card :global(.links a) {
    color: var(--muted);
  }
</style>
