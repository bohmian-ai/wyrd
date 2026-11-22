<!-- Hash router plus the preview strip. The strip is not product chrome. -->
<script>
  import Signup from './Signup.svelte';
  import Workspace from './Workspace.svelte';
  import Entry from './Entry.svelte';
  import { STATES } from './data.js';

  let hash = $state(location.hash || '#/signup');
  $effect(() => {
    const on = () => { hash = location.hash || '#/signup'; };
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });
  const path = $derived(hash.slice(1).split('?')[0]);
  const query = $derived(new URLSearchParams(hash.split('?')[1] ?? ''));
  const tenant = $derived(path.match(/^\/t\/([a-z0-9-]+)$/)?.[1]);

  let mode = $state(new URLSearchParams(location.search).get('mode') === 'light' ? 'light' : 'dark');
  $effect(() => { document.documentElement.dataset.mode = mode; });
  function toggleMode() {
    mode = mode === 'dark' ? 'light' : 'dark';
    const u = new URL(location.href);
    if (mode === 'light') u.searchParams.set('mode', 'light'); else u.searchParams.delete('mode');
    history.replaceState(null, '', u);
  }
</script>

<div class="preview-strip">
  <span class="preview-label">design preview · sample data</span>
  <label class="preview-label" for="state">State</label>
  <select id="state" data-testid="states" value={STATES.some(([, h]) => h === hash) ? hash : ''} onchange={(e) => { if (e.currentTarget.value) location.hash = e.currentTarget.value; }}>
    <option value="">(reached by using the page)</option>
    {#each STATES as [label, h] (h)}<option value={h}>{label}</option>{/each}
  </select>
  <span class="preview-gap"></span>
  <button class="preview-btn" data-testid="theme" aria-label="Switch to {mode === 'dark' ? 'light' : 'dark'} mode" onclick={toggleMode}>◐ {mode === 'dark' ? 'Light' : 'Dark'}</button>
</div>

{#key hash}
  {#if path === '/signup'}
    <Signup {query} />
  {:else if path === '/signup/workspace'}
    <Workspace {query} />
  {:else if tenant}
    <main class="entry landing" data-testid="landing">
      <h1>{tenant}</h1>
      <p>Placeholder for the workspace landing page at <code>/t/{tenant}</code>. The creator arrives signed in as its administrator.</p>
    </main>
  {:else if path === '/'}
    <Entry heading="Welcome to Wyrd"><p class="placeholder">Placeholder for the existing sign-in page. This mock does not redesign it.</p><nav class="links"><a href="#/signup">Create a workspace</a></nav></Entry>
  {:else}
    <Entry heading="Not found"><p class="placeholder" data-testid="missing">This preview has no page at <code>{path}</code>.</p><nav class="links"><a href="#/signup">Create a workspace</a></nav></Entry>
  {/if}
{/key}

<style>
  .landing {
    display: grid;
    gap: 8px;
    align-content: start;
    margin: 24px;
    padding: 24px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
  }
  .landing h1 {
    font: var(--weight-strong) 20px var(--font-mono);
  }
  .landing p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  .placeholder {
    text-align: center;
  }
  code {
    font: 11px var(--font-mono);
    color: var(--text);
  }
</style>
