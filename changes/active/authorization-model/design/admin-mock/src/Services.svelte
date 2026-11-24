<script>
  import Pill from './Pill.svelte';
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { SERVICES } from './data.js';
</script>

<header class="pagehead">
  <div><h1>Service accounts</h1><p>Non-human accounts for CI, schedulers and agents. Each signs in with an API key.</p></div>
  <div class="actions"><button class="app-control primary lg"><Icon d={I.plus} />New service account</button></div>
</header>
<div class="stats">
  <div class="stat"><span class="v">{SERVICES.length}</span><span class="k">Accounts</span></div>
  <div class="stat"><span class="v">3</span><span class="k">Active keys</span></div>
  <div class="stat"><span class="v">0</span><span class="k">Keys unused 90+ days</span></div>
</div>
<div class="tablewrap">
  <table>
    <thead><tr><th>Name</th><th>Restriction</th><th>Keys</th><th>Last used</th><th>Status</th></tr></thead>
    <tbody>
      {#each SERVICES as s (s.id)}
        <tr class="link"><td><b>{s.name}</b></td><td><span class="role">{s.restriction}</span></td><td>{s.keys}</td><td class="muted">{s.lastUsed}</td><td><Pill tone="ok">Active</Pill></td></tr>
      {/each}
    </tbody>
  </table>
</div>
