<script>
  import Fact from './Fact.svelte';
  import Lifecycle from './Lifecycle.svelte';
  import { REPOS } from './catalog.js';
  import { ui, execs, sync, access, readiness, connected, admin, setsOf, execHref } from './store.svelte.js';

  const SHOW = [['all', 'All'], ['ready', 'Ready'], ['inactive', 'Not verifying'], ['undeclared', 'No Card']];
  // Without a working connection Wyrd cannot list provider repositories that have no Card.
  const reachable = $derived(REPOS.filter((r) => r.card || connected()));
  const rows = $derived(reachable.filter((r) => {
    const rk = readiness(r).key;
    const term = ui.q.trim().toLowerCase();
    const okShow = ui.show === 'all' || (ui.show === 'ready' && rk === 'ready') || (ui.show === 'inactive' && (rk === 'inactive' || rk === 'none')) || (ui.show === 'undeclared' && rk === 'undeclared');
    return okShow && (!term || `${r.card || ''} ${r.full}`.toLowerCase().includes(term));
  }));
  const latestOf = (r) => { const s = setsOf(r.card)[0]; return s && execs[s.members[0]] && { s, x: execs[s.members[0]] }; };
  const setShow = (k) => { ui.show = k; sync(); };
</script>

  <div class="page-head">
    <div class="grow"><h1>Repositories</h1>
      <p class="desc">Repository Cards and the GitHub repositories this tenant can reach. Registration, GitHub access and automatic verification are separate facts.</p></div>
    <a class="btn btn-sm" href="#/settings/github">{admin() ? 'GitHub connection' : 'GitHub connection (view)'}</a>
  </div>
  {#if !connected()}
    <div class="banner warn" role="status" data-testid="conn-banner">{ui.conn === 'lost' ? 'GitHub installation access is lost.' : 'GitHub is not connected.'} Registered Repository Cards stay readable; no new verification starts. <a href="#/settings/github">Connection settings</a></div>
  {/if}
  <div class="toolbar">
    <input type="search" class="search" placeholder="Filter by Card or GitHub repository" aria-label="Filter repositories" value={ui.q} oninput={(e) => { ui.q = e.currentTarget.value; sync(); }}>
    <div class="segs" role="group" aria-label="Show">
      {#each SHOW as [k, text] (k)}<button type="button" class="seg" data-show={k} aria-pressed={ui.show === k} onclick={() => setShow(k)}>{text}</button>{/each}
    </div>
    <span class="sub" aria-live="polite" data-testid="repo-count">{rows.length} of {reachable.length} shown</span>
  </div>
  <div class="outs-wrap">
    <table class="outs" data-testid="repo-table">
      <thead><tr><th scope="col">Repository</th><th scope="col">Card version</th><th scope="col">GitHub access</th><th scope="col">Automatic verification</th><th scope="col">Latest execution</th><th scope="col">Status · verdict</th></tr></thead>
      <tbody>
        {#each rows as r (r.full)}
          {@const rd = readiness(r)}
          {@const last = r.card && latestOf(r)}
          <tr data-repo={r.full}>
            <td class="cell"><div class="cellbox">
              {#if r.card}<a class="mono" href="#/repositories/{r.card}">{r.card}</a><span class="sub">Repository Card · {r.full}</span>
              {:else}<a class="mono" href="#/repositories/github/{r.id}">{r.full}</a><span class="sub"><span class="tag" data-nocard>no Card</span> GitHub repository only</span>{/if}
            </div></td>
            <td class="cell"><div class="cellbox">
              {#if r.card}<span class="mono" data-selected={r.selected}>{r.selected}</span><span class="sub">selected of {Object.keys(r.versions).length}</span>{:else}<span class="muted mono">—</span>{/if}
            </div></td>
            <td class="cell"><div class="cellbox"><Fact f={access(r)} name="access" /></div></td>
            <td class="cell"><div class="cellbox"><Fact f={rd} name="ready" /><span class="sub">{rd.why}</span></div></td>
            <td class="cell"><div class="cellbox">
              {#if last}<a class="mono" href={execHref(last.x.id)} data-exe={last.x.id}>{last.x.id}</a><span class="sub">#{last.s.pr} · {last.s.head}</span>{:else}<span class="muted mono">—</span>{/if}
            </div></td>
            <td class="cell"><div class="cellbox">{#if last}<span class="ident"><Lifecycle x={last.x} /></span>{:else}<span class="muted mono">—</span>{/if}</div></td>
          </tr>
        {:else}
          <tr><td colspan="6"><span class="prose muted">No repositories match this filter.</span></td></tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="note">Granting GitHub access never registers a Repository Card or binds Verifiers. Declare a wyrd.toml to register one.</p>
