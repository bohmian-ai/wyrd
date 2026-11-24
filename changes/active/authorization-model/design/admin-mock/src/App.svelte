<!-- Hash router plus the preview strip. The strip is not product chrome. -->
<script>
  import Shell from './Shell.svelte';
  import Admin from './Admin.svelte';
  import Account from './Account.svelte';
  import Platform from './Platform.svelte';
  import Login from './Login.svelte';
  import { STATES } from './data.js';

  const start = '#/t/gtest/settings/people';
  let hash = $state(location.hash || start);
  $effect(() => {
    const on = () => { hash = location.hash || start; };
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });
  const path = $derived(hash.slice(1).split('?')[0]);
  const query = $derived(new URLSearchParams(hash.split('?')[1] ?? ''));
  const section = $derived(path.match(/^\/t\/gtest\/settings\/([a-z-]+)$/)?.[1]);
  const platform = $derived(path.match(/^\/platform\/ui(?:\/([a-z]+))?$/));

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
</div>

{#key path}
  {#if path === '/t/gtest/login' || path === '/t/gtest/signup'}
    <div class="shell"><Login join={path === '/t/gtest/signup'} /></div>
  {:else}
  <Shell kind={platform ? 'platform' : 'tenant'} {path} {mode} {toggleMode}>
    {#if platform}
      <Platform view={platform[1] ?? ''} />
    {:else if section}
      <Admin {section} {query} />
    {:else if path === '/t/gtest/account'}
      <Account />
    {:else}
      <main class="work"><header class="pagehead"><div><h1>Not in this preview</h1><p>This mock covers Settings, Your account and the platform console. <a href={start}>Open Settings</a>.</p></div></header></main>
    {/if}
  </Shell>
  {/if}
{/key}
