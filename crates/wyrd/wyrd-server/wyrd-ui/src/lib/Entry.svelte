<!--
  The signed-out entry card shared by every tenant sign-in, signup and reset
  page: the mark, the page heading, the safe problem when there is one, and
  the page's forms. A server-side refusal (5xx) hides the forms.
-->
<script lang="ts" module>
  import type { WyrdProblem } from '$lib/server/problem';

  /** What the person should do about `issue`: when to retry a throttled attempt, else its remediation. */
  export function guidance(issue: WyrdProblem): string {
    const wait = (issue.details as { retry_after_seconds?: number } | null)?.retry_after_seconds;
    if (!wait) return issue.remediation;
    const minutes = Math.ceil(wait / 60);
    return `Try again in ${wait < 60 ? `${wait} seconds` : `${minutes} minute${minutes === 1 ? '' : 's'}`}.`;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
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
        <div class="app-problem" id="problem" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
          <strong>{issue.status >= 500 ? '✕ Sign-in unavailable' : `⊘ ${issue.title}`}</strong>
          <p>{issue.status >= 500 ? 'Please try again later.' : guidance(issue)}</p>
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
    text-decoration: underline;
  }
  .card :global(.or) {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 10px;
    font: 11px var(--font-mono);
    color: var(--muted);
  }
  .card :global(.or)::before,
  .card :global(.or)::after {
    content: '';
    border-top: 1px solid var(--border-soft);
  }
  @media (max-width: 767px) {
    .entry {
      padding: 32px 16px;
    }
    .card {
      padding: 28px 20px;
    }
  }
  .card :global(b) {
    color: var(--text);
    font-weight: var(--weight-strong);
    overflow-wrap: anywhere;
  }
  .card :global(:not(.app-problem) > code) {
    font: 11px var(--font-mono);
    color: var(--text);
    overflow-wrap: anywhere;
  }
</style>
