<!-- The signed-out entry card, as wyrd-ui's src/lib/Entry.svelte: mark, heading, the safe problem, then the page; a 5xx hides the page. -->
<script>
  import logo from '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/logo.svg';
  import Problem from './Problem.svelte';
  let { heading, issue = null, children } = $props();
</script>

<main class="entry">
  <div class="card">
    <header>
      <img src={logo} alt="" width="28" height="28" /><span class="wordmark">Wyrd</span>
      <span class="product">WYRD</span>
    </header>
    <h1>{heading}</h1>
    {#if issue?.status >= 500}
      <div role="alert">
        <div class="app-problem" id="problem" data-testid="problem" data-state="error">
          <strong>✕ Sign-in unavailable</strong>
          <p>Please try again later.</p>
          <code>{issue.code}</code>
        </div>
      </div>
    {:else}
      {#if issue}<Problem {issue} />{/if}
      {@render children()}
    {/if}
  </div>
</main>

<style>
  .entry {
    min-height: calc(100dvh - 33px);
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
</style>
