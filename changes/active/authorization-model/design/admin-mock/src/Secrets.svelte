<!-- One inventory of the secrets gtest owns, from the four owners the server already exposes. -->
<script>
  import Pill from './Pill.svelte';
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { SECRETS } from './data.js';
  let { query } = $props();
  const base = '#/t/gtest/settings/secrets';
  const confirm = $derived(SECRETS.find((s) => s.name === query.get('confirm')));
  const live = SECRETS.filter((s) => s.state === 'active');
</script>

<header class="pagehead">
  <div><h1>Secrets</h1><p>Credentials gtest stores for providers, Operators and sign-in. Values are write-only and never shown.</p></div>
  <div class="actions"><a class="app-control primary lg" href="{base}?drawer=add"><Icon d={I.plus} />Add provider key</a></div>
</header>

<div class="stats">
  <div class="stat"><span class="v">{live.length}</span><span class="k">Active</span></div>
  <div class="stat"><span class="v">1</span><span class="k">Revoked</span></div>
</div>

<div class="tablewrap">
  <table>
    <thead><tr><th>Name</th><th>Used by</th><th>Last rotated</th><th>Status</th><th class="right"></th></tr></thead>
    <tbody>
      {#each SECRETS as s (s.name)}
        <tr>
          <td style="white-space:nowrap"><b>{s.name}</b><br /><span class="muted" style="font-size:12px">{s.kind}</span></td>
          <td>{s.usedBy}</td>
          <td><span class="muted">{s.rotated}</span></td>
          <td>{#if s.state === 'active'}<Pill tone="ok">Active</Pill>{:else}<Pill tone="off">Revoked</Pill>{/if}</td>
          <td class="right" style="white-space:nowrap">
            {#if s.state === 'active'}
              <button class="app-control">Replace</button>
              <details class="menu">
                <summary class="app-control" aria-label="More actions for {s.name}">⋯</summary>
                <div class="menu-list">
                  <button>Renew encryption<span>New data key; the value is unchanged</span></button>
                  {#if s.kind === 'Gateway provider key'}<a class="danger" href="{base}?confirm={s.name}">Revoke…</a>{/if}
                </div>
              </details>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

{#if query.get('drawer') === 'add'}
  <a class="scrim" href={base} aria-label="Close"></a>
  <aside class="drawer" aria-label="Add provider key">
    <header><h2>Add provider key</h2><a class="iconbtn" href={base} aria-label="Close">✕</a></header>
    <div class="body">
      <div class="field"><label for="n">Name</label><input id="n" class="app-input mono" value="openai-staging" /><span class="hint">Gateway deployments refer to the key by this name.</span></div>
      <div class="field"><label for="p">Provider</label><select id="p" class="app-input"><option>OpenAI</option><option>Anthropic</option><option>Google</option><option>Azure OpenAI</option></select></div>
      <div class="field"><label for="k">API key</label><input id="k" class="app-input mono" type="password" placeholder="sk-…" /><span class="hint">Encrypted before it is stored. Wyrd never shows it again.</span></div>
    </div>
    <footer><a class="app-control" href={base}>Cancel</a><a class="app-control primary" href={base}>Save key</a></footer>
  </aside>
{/if}

{#if confirm}
  <a class="scrim" href={base} aria-label="Cancel"></a>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="rv">
    <div class="body">
      <h2 id="rv">Revoke {confirm.name}?</h2>
      <p>Gateway calls through {confirm.usedBy} fail until you add a replacement. Revoking cannot be undone.</p>
    </div>
    <footer><a class="app-control" href={base}>Cancel</a><a class="app-control danger fill" href={base}>Revoke key</a></footer>
  </div>
{/if}
