<script>
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { ROLES } from './data.js';
  let { query } = $props();
  const open = $derived(query.get('open'));
  const base = '#/t/gtest/settings/roles';
</script>

<header class="pagehead">
  <div><h1>Roles</h1><p>What each role allows. A role applies wherever it is granted: the whole workspace, or one Card or repository.</p></div>
  <div class="actions"><button class="app-control primary lg"><Icon d={I.plus} />New role</button></div>
</header>
<div class="tablewrap">
  <table>
    <thead><tr><th>Role</th><th>Allows</th><th class="right">Members</th><th class="right"></th></tr></thead>
    <tbody>
      {#each ROLES as r (r.name)}
        <tr>
          <td style="white-space:nowrap"><b>{r.name}</b> <span class="muted" style="font-size:12px">{r.builtIn ? 'built-in' : 'custom'}</span></td>
          <td>
            {r.summary}
            {#if open === r.name}
              <div style="margin-top:8px">{#each r.ops as op (op)}<span class="role mono">{op}</span>{/each}</div>
            {/if}
          </td>
          <td class="right">{#if r.members}<a href="#/t/gtest/settings/people">{r.members}</a>{:else}<span class="muted">0</span>{/if}</td>
          <td class="right"><a class="app-control" href={open === r.name ? base : `${base}?open=${r.name}`} aria-expanded={open === r.name}>{open === r.name ? 'Hide' : `${r.ops.length === 1 && r.ops[0] === '*' ? 'All' : r.ops.length} operations`}</a></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
